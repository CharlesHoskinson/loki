//! CPU profiling fixture: two witness phases, lookup, permutation and trash.
use blake2b_simd::State;
use midnight_curves::{Bls12, Fq};
use midnight_proofs::{
    plonk::{create_proof, keygen_pk, keygen_vk_with_k, prepare},
    poly::{
        commitment::Guard,
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
    },
    profile::{take_report, ProofReport},
    transcript::{CircuitTranscript, Transcript},
};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};
use std::time::Instant;

#[path = "loki-profile/circuit.rs"]
mod circuit;
use circuit::ProfileCircuit;

type Scheme = KZGCommitmentScheme<Bls12>;

fn run(k: u32, proof_output: Option<&std::path::Path>) -> ProofReport {
    let circuit = ProfileCircuit {
        rows: (1 << k) - 16,
    };
    let mut rng = ChaCha20Rng::from_seed([42; 32]);
    let params = ParamsKZG::<Bls12>::unsafe_setup(k, &mut rng);
    let vk = keygen_vk_with_k::<Fq, Scheme, _>(&params, &circuit, k).unwrap();
    let pk = keygen_pk(vk, &circuit).unwrap();
    assert!(
        take_report().proofs.is_empty(),
        "keygen must not be profiled"
    );
    let mut transcript = CircuitTranscript::<State>::init();
    let started = Instant::now();
    create_proof::<Fq, Scheme, _, _>(
        &params,
        &pk,
        &[circuit],
        #[cfg(feature = "committed-instances")]
        0,
        &[&[]],
        &mut transcript,
        &mut rng,
    )
    .unwrap();
    let elapsed = started.elapsed().as_nanos() as u64;
    let report = take_report();
    assert_eq!(report.proofs.len(), 1);
    assert!(take_report().proofs.is_empty());
    let proof_report = report.proofs.into_iter().next().unwrap();
    assert!(proof_report.success);
    assert_eq!(proof_report.k, k);
    for phase in [
        "witness_synthesis.phase_0",
        "witness_synthesis.phase_1",
        "advice_commitments",
        "lookup_prepare",
        "lookup_products",
        "permutation_products",
        "trash",
        "compute_nu_poly",
        "quotient_evaluation",
        "quotient_commitment",
        "evaluations",
        "multiopen",
        "kzg_multiopen",
        "other",
    ] {
        let timing = proof_report
            .phases
            .get(phase)
            .unwrap_or_else(|| panic!("missing {phase}"));
        assert!(timing.calls > 0 && timing.wall_ns > 0, "empty {phase}");
    }
    let phase_sum: u64 = proof_report.phases.values().map(|p| p.wall_ns).sum();
    #[cfg(feature = "cuda-argument-commit")]
    {
        let lookup = &proof_report.phases["lookup_prepare"].backends;
        let coeff = lookup
            .get("argument_commit.coeff")
            .expect("missing lookup coefficient strategy");
        assert_eq!(coeff.calls, 2);
        assert_eq!(coeff.sizes.get(&(1 << k)), Some(&2));
        let permutation = &proof_report.phases["permutation_products"].backends;
        let coeff = permutation
            .get("argument_commit.coeff")
            .expect("missing permutation coefficient strategy");
        assert!(coeff.calls > 0);
        assert_eq!(coeff.sizes.get(&(1 << k)), Some(&coeff.calls));
        assert!(proof_report
            .phases
            .values()
            .all(|phase| !phase.backends.contains_key("argument_commit.lagrange")));
    }
    assert_eq!(phase_sum, proof_report.total_wall_ns);
    assert!((phase_sum as f64 / elapsed as f64 - 1.0).abs() < 0.1);
    for counts in [&proof_report.msm, &proof_report.fft] {
        assert!(counts.calls > 0);
        assert_eq!(counts.calls, counts.sizes.values().sum::<u64>());
        assert_eq!(
            counts.elements,
            counts
                .sizes
                .iter()
                .map(|(size, calls)| *size as u64 * calls)
                .sum::<u64>()
        );
    }
    // Dispatch attribution must account for every arithmetic entry exactly once.
    for (prefix, total) in [("msm.", &proof_report.msm), ("ntt.", &proof_report.fft)] {
        let mut sizes = std::collections::BTreeMap::new();
        for phase in proof_report.phases.values() {
            for (_, counts) in phase.backends.iter().filter(|(name, _)| name.starts_with(prefix)) {
                for (size, calls) in &counts.sizes {
                    *sizes.entry(*size).or_insert(0_u64) += calls;
                }
            }
        }
        assert_eq!(sizes, total.sizes, "unattributed {prefix} calls");
    }
    // Verify with the unchanged CPU verifier.
    let proof = transcript.finalize();
    if let Some(path) = proof_output {
        std::fs::write(path, &proof).unwrap();
    }
    let mut transcript = CircuitTranscript::<State>::init_from_bytes(&proof);
    let guard = prepare::<Fq, Scheme, _>(
        pk.get_vk(),
        #[cfg(feature = "committed-instances")]
        &[&[]],
        &[&[]],
        &mut transcript,
    )
    .unwrap();
    transcript.assert_empty().unwrap();
    guard.verify(&params.verifier_params()).unwrap();
    assert!(
        take_report().proofs.is_empty(),
        "verification must not be profiled"
    );
    println!("{}", serde_json::to_string(&proof_report).unwrap());
    proof_report
}

fn main() {
    let k = std::env::args()
        .nth(1)
        .map(|k| k.parse().unwrap())
        .unwrap_or(16);
    let output = std::env::args().nth(2).map(std::path::PathBuf::from);
    run(k, output.as_deref());
}

#[test]
fn small_proof_populates_profile() {
    run(6, None);
}

#[test]
fn jsonl_append_and_output_failure() {
    let path =
        std::env::temp_dir().join(format!("loki-profile-proof-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let child = |output_path: &std::path::Path| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "small_proof_populates_profile", "--nocapture"])
            .env("LOKI_PROFILE", output_path)
            .env("RAYON_NUM_THREADS", "4")
            .output()
            .unwrap();
        assert!(output.status.success(), "child proof failed: {output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        let line = stdout.lines().find(|line| line.starts_with('{')).unwrap();
        serde_json::from_str::<ProofReport>(line).unwrap()
    };
    let first = child(&path);
    let second = child(&path);
    let lines = std::fs::read_to_string(&path).unwrap();
    assert_eq!(lines.lines().count(), 2);
    for (line, report) in lines.lines().zip([first, second]) {
        assert!(report.success && report.jsonl_error.is_none());
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(line).unwrap(),
            serde_json::to_value(report).unwrap()
        );
    }
    let failed_output = child(&path.join("cannot-create-under-a-file.jsonl"));
    assert!(failed_output.success);
    assert!(failed_output.jsonl_error.is_some());
    std::fs::remove_file(path).unwrap();
}
