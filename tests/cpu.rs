#[path = "support/cpu_circuit.rs"]
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
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

#[test]
fn fixed_rng_lookup_proofs_are_reproducible() {
    let circuit = CpuCircuit { rows: 496 };
    let params = ParamsKZG::<Bls12>::unsafe_setup(9, ChaCha20Rng::from_seed([1; 32]));
    let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(&params, &circuit, 9).unwrap();
    let pk = keygen_pk(vk, &circuit).unwrap();
    // Stock seeds its vanishing polynomial per Rayon worker, so reproducibility
    // is defined at a fixed pool size as well as a fixed RNG.
    for threads in [1, 2, 4] {
        let mut reference: Option<Vec<u8>> = None;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        for _ in 0..3 {
            let proof = pool.install(|| {
                let mut transcript = CircuitTranscript::<State>::init();
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
                transcript.finalize()
            });
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
            // Stock places leftover lookup rows in `HashMap` order, so byte
            // reproducibility is promised only with the opt-in deterministic
            // order. Every proof above must verify in either build.
            if !cfg!(feature = "loki-deterministic-lookup-order") {
                continue;
            }
            if let Some(reference) = &reference {
                // Report hashes, rather than dumping a proof on failure.
                assert!(
                    reference == &proof,
                    "proof bytes differ: {} vs {}",
                    blake2b_simd::blake2b(reference).to_hex(),
                    blake2b_simd::blake2b(&proof).to_hex(),
                );
            } else {
                reference = Some(proof);
            }
        }
    }
}
