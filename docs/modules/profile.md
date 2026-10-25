# `src/profile.rs`: phase profiler (`loki-profile`)

## Purpose

Per-proof exclusive wall time per prover phase, plus MSM and FFT call counters. It is the measurement tool the whole project uses to decide where time goes. The `LOKI-PROFILE.md` note records that, in the k16 synthetic workload, quotient evaluation, quotient commitment and KZG multiopen each take 17-19% of proof time.

The module exists only with the `loki-profile` feature (`lib.rs:14-15`). Without it, the guards, counters and the `LOKI_PROFILE` environment read are compiled out.

## Public API

[`profile.rs`](../../src/profile.rs):

- `take_report() -> Report` (`:98`): drains completed proofs. Active proofs stay private until they finish. Reports accumulate until drained.
- `Report { proofs: Vec<ProofReport> }` (`:31`). `ProofReport` (`:72`): `k`, `circuits`, `success`, `total_wall_ns`, `phases: BTreeMap<String, PhaseTiming>`, `msm`, `fft`, `jsonl_error`. `PhaseTiming` (`:38`): `calls`, `wall_ns`, `backends`. `OperationCounts` (`:50`): `calls`, `elements`, `sizes` histogram, `wall_ns`. All derive serde `Serialize`/`Deserialize`.
- `LOKI_PROFILE=/absolute/path.jsonl` appends one JSON object per proof attempt (`:169`). An output error becomes `jsonl_error` and does not change proof success.
- Crate-internal: `ProofGuard::start` (`:130`), `PhaseGuard::start` (`:184`), `record_msm` (`:213`), `record_fft` (`:221`), `record_backend_timing` (`:230`, used by the CUDA backend for prepare, upload and run timing).

Phase names: `witness_synthesis.phase_N`, `advice_commitments`, `lookup_prepare`, `permutation_products`, `lookup_products`, `trash`, `compute_nu_poly`, `quotient_evaluation`, `quotient_construct`, `quotient_commitment`, `evaluations`, `multiopen`, `kzg_multiopen`, `other`.

## Difference from stock midnight-proofs 0.8.3

The file is new. Hooks are `cfg(feature = "loki-profile")` blocks in `plonk/prover.rs`, `plonk/evaluation.rs`, `plonk/vanishing/prover.rs`, `poly/domain.rs`, `poly/kzg/mod.rs`, `poly/kzg/msm.rs` and `utils/arithmetic.rs`. No dependency was added.

## Invariants and pitfalls

- Phase times are exclusive, and their sum equals `total_wall_ns`. `other` records time outside the named phases. Publication and JSONL I/O happen after the measurement.
- MSM lengths are counted before zero filtering. FFT counts include forward and inverse. Keygen and verification are not measured.
- Phases are exclusive, so `multiopen` (0.033 ms in the k16 report) holds only the time outside the nested `kzg_multiopen` phase (211 ms). Read the two together.
- The output contains no witness, SRS, key or transcript values.
- Concurrent provers have independent records. Use separate files for separate processes.

## Tests

`profile.rs` test `isolate_threads_drain_completed_and_serialize`. `examples/loki-profile.rs` runs a filled-domain synthetic workload ([examples](examples.md)). Phase-sum equality is checked in the W11 validation (`LOKI-MULTIOPEN.md`).

## Notes

[`LOKI-PROFILE.md`](../../LOKI-PROFILE.md).
