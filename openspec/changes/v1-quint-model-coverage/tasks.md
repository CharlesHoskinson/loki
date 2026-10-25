# Tasks

## 1. Coverage inventory and contract

- [ ] 1.1 Create `quint/coverage.yaml` from the exact release-source inventory for every path affecting scheduling, caching, dispatch, fallback, transcript, or lookup order; require model/invariant/witness, current Rust replay, compat-proof fixture ID, fuzz target, Lean contract/status, docs link, and owner for each in-scope row. Make T20 cache identity and T33 retention rows conditional on finding cache code in that source inventory; otherwise record explicit deferrals, without inferring that modeled recl9 or deferred W12 code ships. Also add explicit rows or named blockers for T28 cancellation, T29 temporal liveness, T30 transcript shapes, and T31 NTT fallback/replay. Verify with `python3 quint/ci/check_coverage.py --base "$V1_BASE_SHA" --head "$V1_RELEASE_SHA"`; include positive and negative unmapped-path fixtures.
- [ ] 1.2 Review the manifest against all v1 feature proposals with compat-proof, GPU, fuzz, Lean, WASM/mobile, and docs-release owners; verify every shipped behavior path is mapped and every deferred/unimplemented model is labeled as such in `quint/coverage.yaml`.

## 2. Model safety and witness gate

- [ ] 2.1 Add `python3 quint/ci/run_models.py --release --seed 20261025` to run all in-scope Quint typechecks, unit tests, invariant campaigns, witnesses, and mutation controls; verify it fails on a missing witness, an escaped defect mutation, and any invariant without a registered mutation/counterexample control, using current runner behavior at `quint/check.py:20-56`, `quint/q1/check_q1.py:72-90`, and `quint/q4/check.py:98-128`.
- [ ] 2.2 Extend the models and invariant-to-mutation matrix for each in-scope shipped change, including cancellation no-retry and lookup default/opt-in separation; verify every required mutation is detected by `python3 quint/ci/run_models.py --release --seed 20261025`, building on `quint/q2/dispatch.qnt:1-397` and `quint/q5/lookup.qnt:1-510`.

## 3. Current-source Rust replay

- [ ] 3.1 Refactor release MBT adapters so `CARGO_NET_OFFLINE=true cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1` uses current checkout source and vendored or immutable content-addressed dependencies, not hardcoded machine-local historical repositories; verify in a clean offline CI checkout and record the source SHA, building on `quint/mbt/README.md:7-39`, `quint/mbt/Cargo.toml:7-20`, and `quint/mbt/build.rs:64-74`.
- [ ] 3.2 Add deterministic ITF/model-trace replay crosswalks for every manifest row and consume compat-proof fixture IDs; verify Rust-observed state/output against the model with `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1`, extending `quint/mbt/src/transcript_driver.rs:17-120` and the real-dispatcher replay under `quint/q2/mbt/src/dispatch_driver.rs:1-567`. Classify fake-backend tests as dispatch-policy evidence only; hardware behavior remains under GPU owner gates.
- [ ] 3.3 Add a release-blocker test for any changed shipped behavior without a Rust adapter; verify by removing a replay mapping from a test manifest and confirming `python3 quint/ci/check_coverage.py --base "$V1_BASE_SHA" --head "$V1_RELEASE_SHA"` exits nonzero, with current limitations recorded in `quint/q2/REPORT.md:124-130`.

## 4. Model checking and compatibility boundaries

- [ ] 4.1 Add symbolic bounded finite-safety checks through `python3 quint/ci/run_apalache.py --release`, recording bound, versions and counterexample ITF and claiming exhaustiveness only with a completeness argument; verify expected passing and injected failing models, using `quint/q2/check.py:145-155` and `quint/q4/check.py:28-58` as current patterns.
- [ ] 4.2 For any shipped liveness claim, add a fairness-specified temporal model and a supported temporal checker in `python3 quint/ci/run_temporal.py --release`; verify a starvation mutant is rejected. If no suitable checker supports the fairness condition, keep that shipped liveness claim blocked; the current scheduler remains planned-only, based on `quint/scheduler.qnt:73-83` and `docs/modules/quint.md:12-20`.
- [ ] 4.3 Add a stock/opt-in model compatibility lane using `python3 quint/ci/run_compat_models.py --stock 0.8.3 --release-sha "$V1_RELEASE_SHA"`; verify default lookup remains stock-compatible and deterministic lookup is opt-in, consuming compat-proof fixture IDs and building on `src/plonk/lookup/prover.rs:393-429` and `docs/ARCHITECTURE.md:29,100-104,128-142,158-160`.

## 5. CI, documentation, and release evidence

- [ ] 5.1 Wire manifest, model, MBT, safety, temporal (when claimed), and stock-boundary commands into release CI at the exact candidate SHA; verify CI fails on an omitted report or a deliberately failing fixture. Keep host CPU model jobs independent of GPU and cross-target jobs, based on `quint/mbt/README.md:22-39` and `docs/modules/quint.md:1-28`.
- [ ] 5.2 Update `docs/modules/quint.md` and the release QA docs with exact commands, source pins, coverage semantics, limits, output artifact names, CI owner, WASM/mobile separation, and model-to-fuzz/Lean mappings; verify every manifest docs link resolves and each command is documented from the sources at `docs/modules/quint.md:1-43` and `docs/modules/README.md:1-67`.
- [ ] 5.3 Run `openspec validate v1-quint-model-coverage --strict` and archive the machine-readable gate results at the exact release SHA; verify zero strict findings and that the release evidence lists all required commands, versions, fixture IDs, bounds, witnesses, and replay results.
