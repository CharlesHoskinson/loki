extern crate stock as midnight_proofs;
#[path = "../../tests/support/cpu_circuit.rs"]
mod cpu_circuit;

use blake2b_simd::State;
use cpu_circuit::CpuCircuit;
use midnight_curves::{Bls12, Fq};
use std::{fs, path::PathBuf};
use stock::{
    plonk::{prepare, VerifyingKey},
    poly::{
        commitment::Guard,
        kzg::{params::ParamsVerifierKZG, KZGCommitmentScheme},
    },
    transcript::{CircuitTranscript, Transcript},
    utils::SerdeFormat,
};

#[test]
fn registry_verifier_accepts_loki_proof() {
    let fixture = std::env::var_os("LOKI_CPU_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/cpu-fixture"));
    let proof = fs::read(fixture.join("proof.bin"))
        .expect("run cpu-bench with target/cpu-fixture output first");
    let vk_bytes = fs::read(fixture.join("vk.bin")).unwrap();
    let params_bytes = fs::read(fixture.join("params.bin")).unwrap();
    let vk = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<_, CpuCircuit>(
        &mut &vk_bytes[..],
        SerdeFormat::Processed,
        #[cfg(feature = "circuit-params")]
        (),
    )
    .unwrap();
    let params =
        ParamsVerifierKZG::<Bls12>::read(&mut &params_bytes[..], SerdeFormat::Processed).unwrap();
    let verify = |proof: &[u8]| {
        let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
        prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
            &vk,
            #[cfg(feature = "committed-instances")]
            &[&[]],
            &[&[]],
            &mut transcript,
        )
        .map(|guard| guard.verify(&params).is_ok())
        .unwrap_or(false)
    };
    assert!(
        verify(&proof),
        "unmodified registry verifier must accept Loki proof"
    );
    let mut corrupted = proof;
    let last = corrupted.len() - 1;
    corrupted[last] ^= 1;
    assert!(!verify(&corrupted), "corrupted proof must be rejected");
}
