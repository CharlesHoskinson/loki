//! Public synthetic fixture only; the deterministic unsafe SRS is for tests.
#[path = "../tests/support/cpu_circuit.rs"]
mod cpu_circuit;

use blake2b_simd::State;
use cpu_circuit::CpuCircuit;
use midnight_curves::{Bls12, Fq};
use midnight_proofs::{
    plonk::{create_proof, keygen_pk, keygen_vk_with_k, prepare},
    poly::{
        commitment::Guard,
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
    },
    transcript::{CircuitTranscript, Transcript},
    utils::SerdeFormat,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use std::{fs, path::PathBuf, time::Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let k: u32 = args.next().unwrap_or_else(|| "16".into()).parse().unwrap();
    let repeats: usize = args.next().unwrap_or_else(|| "3".into()).parse().unwrap();
    let output = args.next().map(PathBuf::from);
    let circuit = CpuCircuit {
        rows: (1 << k) - 16,
    };
    let start = Instant::now();
    let params = ParamsKZG::<Bls12>::unsafe_setup(k, ChaCha20Rng::from_seed([1; 32]));
    let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(&params, &circuit, k).unwrap();
    let pk = keygen_pk(vk, &circuit).unwrap();
    println!(
        "k={k} threads={} setup_ms={:.3}",
        rayon::current_num_threads(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let mut first = None;
    for run in 0..repeats {
        let mut transcript = CircuitTranscript::<State>::init();
        let start = Instant::now();
        create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
            &params,
            &pk,
            &[circuit.clone()],
            #[cfg(feature = "committed-instances")]
            0,
            &[&[]],
            &mut transcript,
            ChaCha20Rng::from_seed([2; 32]),
        )
        .unwrap();
        let proof = transcript.finalize();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let mut transcript = CircuitTranscript::<State>::init_from_bytes(&proof);
        prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
            pk.get_vk(),
            #[cfg(feature = "committed-instances")]
            &[&[]],
            &[&[]],
            &mut transcript,
        )
        .unwrap()
        .verify(&params.verifier_params())
        .unwrap();
        println!(
            "run={run} prove_ms={elapsed:.3} bytes={} hash={} equal_first={}",
            proof.len(),
            blake2b_simd::blake2b(&proof).to_hex(),
            first.as_ref().map_or(true, |p| p == &proof)
        );
        if first.is_none() {
            first = Some(proof);
        }
    }
    if let Some(output) = output {
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("proof.bin"), first.unwrap()).unwrap();
        let mut vk = Vec::new();
        pk.get_vk().write(&mut vk, SerdeFormat::Processed).unwrap();
        fs::write(output.join("vk.bin"), vk).unwrap();
        let mut verifier_params = Vec::new();
        params
            .verifier_params()
            .write(&mut verifier_params, SerdeFormat::Processed)
            .unwrap();
        fs::write(output.join("params.bin"), verifier_params).unwrap();
    }
}
