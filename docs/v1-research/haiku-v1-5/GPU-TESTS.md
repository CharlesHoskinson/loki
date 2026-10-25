# GPU-TESTS: GPU test and QA gates (haiku-v1-5)

## Existing in Loki
- Harness: `loki-harness/` (k10/k14/k17 cross-verify, ignored k20; placeholder `cuda` feature is CPU-only, Cargo.toml L11-16). Source: design/TEST-PLAN.md "W2 and stock-verifier harness".
- Loki tests: `tests/backend_default_stock.rs`, `tests/backend_cpu_equivalence.rs` (Cargo.toml L123-132), `tests/gpu-compat/`, `tests/ntt-stock/`.
- Fuzz: `fuzz/fuzz_targets/msm_diff.rs`, `ntt_diff.rs`, `proof_parse.rs`, `proof_roundtrip.rs`, `soundness.rs`; scripts `fuzz/gpu-slices.sh`.
- Quint: `quint/gpu_dispatch.qnt`, `gpu_dispatch_test.qnt`, `scheduler.qnt`, `scheduler_test.qnt`, `mutations.qnt`, `pk_cache.qnt`, plus `quint/mbt/`.
- Merge gates: `docs/MERGE-GATES.md` (MERGE-CPU; RECEIPT-NATIVE = `scripts/verify-main.sh --tier native`, heavy.lock then gpu.lock).
- Lean: gpu-spec `LokiLean/BackendMsm.lean:282` and `BackendQuotient.lean:182` are sorry; design/TEST-PLAN.md Lean-to-test table maps contracts to planned tests.

## Planned but absent
- loki-gpu integration tests: srs_registry, ntt_differential, resident_ntt, quotient_differential, quotient_validation, msm_differential, scheduler_contract (TEST-PLAN.md planned targets). Present: runtime_contract (46 fns), ffi_abi (cuda-gated), p2_fix_regressions, review_regressions.
- loki-gpu `crates/loki-gpu/fuzz/` (msm_diff, ntt_diff, quotient_program, residency_state) does not exist.
- W2 ids gpu_k10/k14/k17/k20_cross_verify, default_cpu_path_unchanged, no_lookup_stock_bytes, lookup_matched_order_stock_bytes, stock_default_dependency_audit: not present (grep unverified beyond harness README).

## Proposed artifacts and commands (CODE, not run)
- Dispatch-counter gate: every GPU-required test asserts positive attempted/succeeded counts and zero fallback (TEST-PLAN.md acceptance rule). Command: `cargo test -p loki-gpu --features cuda --test msm_differential` (after tests exist).
- Determinism: fixed RNG, `RAYON_NUM_THREADS=1`, lookup map-order pinned (`loki-deterministic-lookup-order`, Cargo.toml L74); byte equality only for no-lookup and matched-order lookup fixtures.
- Sanitizer: run native lifetime tests under compute-sanitizer memcheck/racecheck/synccheck (docs: https://docs.nvidia.com/compute-sanitizer/ComputeSanitizer/).
- Scheduler: concurrent proof requests under bounded budget, replay cancellation/failure traces (Quint scheduler.qnt MBT).
- Recursion gate: A3 k20 and B3 chain proofs accepted by unmodified stock 0.8.3 verifier (design/TEST-PLAN.md protocol).
- Non-sm_120 / mixed-GPU matrix: TODO.md L144-145 (open).

## Gaps to flag as spec items
- No test currently proves a GPU path ran for any v1 operation, because no arithmetic op is implemented in loki-gpu.
- Lookup byte equality is diagnostic until ordering drift at src/plonk/lookup/prover.rs:427-430 is closed (TEST-PLAN.md; UNVERIFIED whether closed on main).
