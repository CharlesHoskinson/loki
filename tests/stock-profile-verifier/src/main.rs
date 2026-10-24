//! Verify exported Loki proofs using the unmodified registry crate.
use blake2b_simd::State;
use midnight_curves::{Bls12, Fq};
use midnight_proofs::{
    plonk::{keygen_vk_with_k, prepare},
    poly::{
        commitment::Guard,
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
    },
    transcript::{CircuitTranscript, Transcript},
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

#[path = "../../../examples/loki-profile/circuit.rs"]
mod circuit;
use circuit::ProfileCircuit;
type Scheme = KZGCommitmentScheme<Bls12>;

fn main() {
    let k: u32 = std::env::args().nth(1).unwrap().parse().unwrap();
    let path = std::env::args().nth(2).unwrap();
    let circuit = ProfileCircuit {
        rows: (1 << k) - 16,
    };
    let mut rng = ChaCha20Rng::from_seed([42; 32]);
    let params = ParamsKZG::<Bls12>::unsafe_setup(k, &mut rng);
    let vk = keygen_vk_with_k::<Fq, Scheme, _>(&params, &circuit, k).unwrap();
    let proof = std::fs::read(path).unwrap();
    let mut transcript = CircuitTranscript::<State>::init_from_bytes(&proof);
    let guard = prepare::<Fq, Scheme, _>(
        &vk,
        #[cfg(feature = "committed-instances")]
        &[&[]],
        &[&[]],
        &mut transcript,
    )
    .unwrap();
    transcript.assert_empty().unwrap();
    guard.verify(&params.verifier_params()).unwrap();
    println!(
        "Unmodified registry midnight-proofs 0.8.3 accepts Loki k{k} proof ({} bytes)",
        proof.len()
    );
}
