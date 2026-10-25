//! Expected-failure diagnostic for a pre-existing stock prover/verifier mismatch.
use crate::{circuit::MixedCircuit, logging::Logging};
use midnight_curves::{Bls12, Fq};
use midnight_proofs::{
    plonk::{commit_to_instances, create_proof, keygen_pk, keygen_vk_with_k, prepare},
    poly::{
        commitment::Guard,
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
    },
    transcript::Transcript,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

#[test]
fn diagnose_stock_mixed_instance_order() {
    let params = ParamsKZG::<Bls12>::unsafe_setup(5, ChaCha20Rng::seed_from_u64(8000));
    let circuits: Vec<_> = (0..3)
        .map(|p| MixedCircuit {
            values: [Fq::from(3 + p), Fq::from(7 + p)],
        })
        .collect();
    let vk = keygen_vk_with_k(&params, &circuits[0], 5).unwrap();
    let pk = keygen_pk(vk, &circuits[0]).unwrap();
    for proofs in 1..=3 {
        let public: Vec<Vec<Vec<Fq>>> = circuits[..proofs]
            .iter()
            .map(|c| c.values.iter().map(|v| vec![*v]).collect())
            .collect();
        let all: Vec<Vec<&[Fq]>> = public
            .iter()
            .map(|cols| cols.iter().map(Vec::as_slice).collect())
            .collect();
        let all: Vec<&[&[Fq]]> = all.iter().map(Vec::as_slice).collect();
        for committed in 0..=2 {
            let mut prover = Logging::init();
            create_proof(
                &params,
                &pk,
                &circuits[..proofs],
                committed,
                &all,
                &mut prover,
                ChaCha20Rng::seed_from_u64(8001),
            )
            .unwrap();
            let prover_ops = prover.ops.clone();
            let bytes = prover.finalize();
            let comms: Vec<Vec<_>> = public
                .iter()
                .map(|cols| {
                    cols[..committed]
                        .iter()
                        .map(|values| {
                            commit_to_instances::<Fq, KZGCommitmentScheme<Bls12>>(
                                &params,
                                pk.get_vk().get_domain(),
                                values,
                            )
                        })
                        .collect()
                })
                .collect();
            let comms: Vec<&[_]> = comms.iter().map(Vec::as_slice).collect();
            let normal: Vec<Vec<&[Fq]>> = public
                .iter()
                .map(|cols| cols[committed..].iter().map(Vec::as_slice).collect())
                .collect();
            let normal: Vec<&[&[Fq]]> = normal.iter().map(Vec::as_slice).collect();
            let mut verifier = Logging::init_from_bytes(&bytes);
            let accepted = match prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
                pk.get_vk(),
                &comms,
                &normal,
                &mut verifier,
            ) {
                Ok(guard) => guard.verify(&params.verifier_params()).is_ok(),
                Err(_) => false,
            };
            let prefix = 1 + proofs * (committed + 2 * (2 - committed));
            assert!(verifier.ops.len() >= prefix);
            let agree = prover_ops[..prefix] == verifier.ops[..prefix];
            let first_difference = prover_ops[..prefix]
                .iter()
                .zip(&verifier.ops[..prefix])
                .position(|(a, b)| a != b);
            let expected = !(proofs > 1 && committed == 1);
            assert_eq!(
                agree, expected,
                "instance absorption-order boundary changed"
            );
            assert_eq!(accepted, expected, "stock acceptance boundary changed");
            println!("instance-order proofs={proofs} committed={committed} plain={} absorbed_equal={agree} stock_accepted={accepted} first_difference={first_difference:?}", 2 - committed);
        }
    }
}
