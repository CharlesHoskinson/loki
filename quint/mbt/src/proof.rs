use crate::{
    circuit::RandomCircuit,
    logging::{Logging, Plain, Raw},
};
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
use rand_core::{RngCore, SeedableRng};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Shape {
    pub proofs: usize,
    pub advice0: usize,
    pub advice1: usize,
    pub advice2: usize,
    pub phase_challenges1: usize,
    pub phase_challenges2: usize,
    pub phase_challenges: usize,
    pub lookups: usize,
    pub permutations: usize,
    pub trash: usize,
    pub quotient: usize,
    pub instance_values: usize,
    pub committed: bool,
    pub evaluations: usize,
    pub point_sets: usize,
    pub advice_queries: usize,
    pub fixed_queries: usize,
    pub instance_queries: usize,
    pub permutation_columns: usize,
}

pub fn prove(profile: usize, proofs: usize, committed: bool, seed: u64) -> (Shape, Vec<Raw>, bool) {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    // Synthetic test-only setup, never persisted. This is not a production SRS.
    let params = ParamsKZG::<Bls12>::unsafe_setup(5, &mut rng);
    let circuits: Vec<_> = (0..proofs)
        .map(|_| RandomCircuit {
            profile,
            value: Fq::from((rng.next_u32() % 8) as u64),
        })
        .collect();
    let vk = keygen_vk_with_k(&params, &circuits[0], 5).unwrap();
    let pk = keygen_pk(vk, &circuits[0]).unwrap();
    let cs = pk.get_vk().cs();
    let public: Vec<_> = circuits.iter().map(|c| vec![c.value]).collect();
    let instances: Vec<Vec<&[Fq]>> = public.iter().map(|v| vec![v.as_slice()]).collect();
    let instances: Vec<&[&[Fq]]> = instances.iter().map(Vec::as_slice).collect();
    let proof_rng = rng.clone();
    let mut logged = Logging::init();
    create_proof(
        &params,
        &pk,
        &circuits,
        usize::from(committed),
        &instances,
        &mut logged,
        proof_rng.clone(),
    )
    .unwrap();
    let ops = logged.ops.clone();
    let bytes = logged.finalize();
    let mut plain = Plain::init();
    create_proof(
        &params,
        &pk,
        &circuits,
        usize::from(committed),
        &instances,
        &mut plain,
        proof_rng,
    )
    .unwrap();
    let plain_bytes = plain.finalize();
    let byte_equal = bytes == plain_bytes;
    if profile == 0 {
        assert!(byte_equal, "logging wrapper changed no-lookup proof bytes");
    }

    let comms: Vec<Vec<_>> = public
        .iter()
        .map(|values| {
            if committed {
                vec![commit_to_instances::<Fq, KZGCommitmentScheme<Bls12>>(
                    &params,
                    pk.get_vk().get_domain(),
                    values,
                )]
            } else {
                vec![]
            }
        })
        .collect();
    let comms: Vec<&[_]> = comms.iter().map(Vec::as_slice).collect();
    let normal: Vec<Vec<&[Fq]>> = public
        .iter()
        .map(|values| {
            if committed {
                vec![]
            } else {
                vec![values.as_slice()]
            }
        })
        .collect();
    let normal: Vec<&[&[Fq]]> = normal.iter().map(Vec::as_slice).collect();
    let guard = prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
        pk.get_vk(),
        &comms,
        &normal,
        &mut Plain::init_from_bytes(&plain_bytes),
    )
    .unwrap();
    guard.verify(&params.verifier_params()).unwrap();
    let mut verifier = Logging::init_from_bytes(&bytes);
    let guard =
        prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(pk.get_vk(), &comms, &normal, &mut verifier)
            .unwrap();
    guard.verify(&params.verifier_params()).unwrap();
    verifier.assert_empty().unwrap();
    assert_eq!(
        ops.len(),
        verifier.ops.len(),
        "prover/verifier operation count"
    );
    for (i, (a, b)) in ops.iter().zip(&verifier.ops).enumerate() {
        assert!(
            a == b,
            "stock verifier transcript mismatch at operation {i}"
        );
    }

    let permutation_columns = cs.permutation().get_columns().len();
    let permutations = permutation_columns.div_ceil(cs.degree() - 2);
    let evaluations = proofs
        * (cs.advice_queries().len()
            + if committed {
                cs.instance_queries().len()
            } else {
                0
            }
            + if permutations == 0 {
                0
            } else {
                3 * permutations - 1
            }
            + 5 * cs.lookups().len()
            + cs.trashcans().len())
        + cs.fixed_queries().len()
        + 1
        + permutation_columns;
    let shape = Shape {
        proofs,
        advice0: cs.advice_column_phase().iter().filter(|p| **p == 0).count(),
        advice1: cs.advice_column_phase().iter().filter(|p| **p == 1).count(),
        advice2: 0,
        phase_challenges1: 0,
        phase_challenges2: 0,
        phase_challenges: cs.num_challenges(),
        lookups: cs.lookups().len(),
        permutations,
        trash: cs.trashcans().len(),
        quotient: pk.get_vk().get_domain().get_quotient_poly_degree(),
        instance_values: 1,
        committed,
        evaluations,
        // Number of q evaluations occurs strictly between multiopen x3 and x4.
        // Cross-check the expected family constant below; never infer event order from it.
        point_sets: if profile > 0 { 4 } else { 1 },
        advice_queries: cs.advice_queries().len(),
        fixed_queries: cs.fixed_queries().len(),
        instance_queries: cs.instance_queries().len(),
        permutation_columns,
    };
    (shape, ops, byte_equal)
}
