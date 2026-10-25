# Spec Delta

## Purpose

Defines the release evidence required to connect Quint models and model-based Rust replays to every shipped Loki change that affects stateful or ordered behavior.

## ADDED Requirements

### Requirement: Stateful behavior changes have complete model traceability
The release SHALL map every changed source path that affects scheduling, caching, dispatch, fallback, transcript order, or lookup order to a Quint model, its observable Rust replay, compatibility fixture IDs, and the owning fuzz and Lean contract status in `quint/coverage.yaml`.

#### Scenario: Release diff includes a covered behavior change
- **WHEN** `python3 quint/ci/check_coverage.py --base "$V1_BASE_SHA" --head "$V1_RELEASE_SHA"` inspects a release diff containing an in-scope source path
- **THEN** it SHALL pass only if `quint/coverage.yaml` names the path and its model, invariant, witness, Rust replay, fixture IDs, fuzz entry, Lean status, and docs entry; the checked-in baseline areas are transcript and instance order at `quint/transcript.qnt:6-116,124-170`, dispatch at `src/backend/dispatch.rs:29-47,113-159,185-224`, and lookup at `src/plonk/lookup/prover.rs:393-429`.

#### Scenario: Release diff has an unmapped behavior change
- **WHEN** the same command finds a changed in-scope source path without a complete mapping
- **THEN** it SHALL exit nonzero and identify that source path as a release blocker, using the model areas documented at `docs/modules/quint.md:8-20` and the existing manual gate boundary in `docs/modules/quint.md:1-28`.

### Requirement: Safety invariants, witnesses, and mutation controls gate release
Each in-scope model SHALL define safety invariants and required reachable witnesses, and each invariant SHALL have a mutation or counterexample control that demonstrates the invariant can detect a relevant defect.

#### Scenario: Model gate records expected passing and failing cases
- **WHEN** `python3 quint/ci/run_models.py --release --seed 20261025` runs the coverage manifest
- **THEN** it SHALL typecheck each model, run its unit tests and bounded campaign, require every listed witness to occur, require every in-scope invariant to map to at least one mutation or counterexample control, fail when any invariant is unmapped, require registered defect mutations to violate their named invariant, and retain per-run seed, command, result, and log; the contract builds on `quint/check.py:20-56`, `quint/q1/check_q1.py:48-90`, and `quint/q4/check.py:76-129`.

### Requirement: Shipped behavior changes replay against real Rust
For each changed shipped behavior in the covered categories, the release SHALL replay model-derived traces against the corresponding Rust code at the exact release SHA and compare observable state or outputs; a missing replay seam SHALL remain an unmet release blocker. A fake backend MAY drive the real dispatcher policy, but it SHALL NOT count as evidence for GPU arithmetic or device behavior.

#### Scenario: MBT replay uses current release code and shared fixtures
- **WHEN** `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1` runs at `V1_RELEASE_SHA`
- **THEN** every manifest replay SHALL consume compat-proof-owned stock 0.8.3 fixture IDs and a deterministic ITF trace, compare model and current-release Rust observations, and write a replay result keyed to that SHA; the existing adapter baseline is `quint/mbt/README.md:7-39` and `quint/mbt/src/transcript_driver.rs:17-120`.

#### Scenario: A CPU surrogate models an accelerator result
- **WHEN** a test supplies scripted accelerator success or failure to exercise a real dispatcher or host adapter
- **THEN** that replay SHALL count only for the Rust routing, publication, counter, and fallback contract at that seam; GPU arithmetic/device coverage SHALL come from the GPU owner’s separate differential and device gates, building on `quint/q2/REPORT.md:90-122` and `quint/mbt/README.md:22-33`.

#### Scenario: A covered shipped path cannot be replayed
- **WHEN** the Rust replay runner has no injectable seam for a changed shipped behavior
- **THEN** `python3 quint/ci/run_models.py --release --seed 20261025` SHALL fail the release gate with the path, missing adapter, and owner recorded in `quint/coverage.yaml`, building on current replay limitations documented in `quint/q2/REPORT.md:124-130`.

### Requirement: Release replays are source-pinned and reproducible
The release SHALL pin the Quint toolchain, Apalache version, Rust MBT dependencies, and stock oracle version, and SHALL make those pinned dependencies available in clean offline CI while running Rust adapters against exact-release source without machine-local paths or stale snapshots.

#### Scenario: Replay runs in a clean checkout
- **WHEN** `python3 quint/ci/run_models.py --release --seed 20261025` and `CARGO_NET_OFFLINE=true cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1` run in a clean CI checkout at `V1_RELEASE_SHA`
- **THEN** both SHALL complete using checked-in toolchain/lock pins and vendored or immutable content-addressed dependencies, the 0.8.3 oracle, and checkout source, while recording versions and SHA; the existing non-hermetic assumption to replace is `quint/mbt/build.rs:64-74` and the current git dependency pin is described at `quint/mbt/Cargo.toml:7-20`.

### Requirement: Finite safety and liveness claims use suitable verification
Finite-state safety properties SHALL receive symbolic bounded verification with Apalache where feasible, and results SHALL state the bound. A result SHALL be called exhaustive only when completeness is established; any shipped liveness claim SHALL use a temporal verifier with explicit fairness assumptions, and sampled completion witnesses SHALL NOT count as liveness proof.

#### Scenario: Finite dispatch safety is checked to a recorded bound
- **WHEN** `python3 quint/ci/run_apalache.py --release` checks a finite safety model within its declared state bound
- **THEN** the run SHALL record the bound, invariant, expected result, counterexample artifact when present, tool version, and exit status; the current finite dispatch campaign is `quint/q2/check.py:145-155` and its model invariants are in `quint/q2/dispatch.qnt:1-397`.

#### Scenario: Scheduler liveness lacks a supported temporal check
- **WHEN** a release change claims eventual completion under weak fairness but the selected checker cannot process that fairness condition
- **THEN** the shipped liveness claim SHALL remain blocked and SHALL NOT be inferred from sampled witnesses if the selected checker lacks required fairness support; the current planned-only model and fairness boundary are `quint/scheduler.qnt:73-83` and `docs/modules/quint.md:12-20`.

### Requirement: Stock behavior and opt-in Loki behavior remain distinct
Quint models and Rust replays SHALL distinguish stock 0.8.3 default behavior from each Loki-only opt-in and SHALL NOT use model results to authorize a default behavior or proof-format change.

#### Scenario: Default and deterministic lookup ordering are compared separately
- **WHEN** the release model campaign covers leftover lookup placement
- **THEN** its stock-default branch SHALL preserve unspecified randomized `HashMap` iteration and its deterministic branch SHALL be enabled only by `loki-deterministic-lookup-order`; compatibility SHALL be verified by `python3 quint/ci/run_compat_models.py --stock 0.8.3 --release-sha "$V1_RELEASE_SHA"` and the source mapping at `src/plonk/lookup/prover.rs:393-429` plus `docs/ARCHITECTURE.md:29,100-104,128-142,158-160`.

#### Scenario: Quint and stock fixture outcomes disagree
- **WHEN** a model-derived fixture has a different accept/reject result or observable transcript event order than unmodified stock 0.8.3 on the default configuration
- **THEN** the compatibility model gate SHALL fail and retain the counterexample for compat-proof review; the current oracle boundary is `quint/mbt/README.md:13-23` and the stock mixed-instance diagnostic is `quint/transcript.qnt:124-170`.

#### Scenario: Stock rejects its inherited mixed-instance multiproof
- **WHEN** the shared T03 fixture uses multiple proofs with both committed and plain instance columns
- **THEN** the compatibility gate SHALL record the expected unmodified-stock rejection and require Loki to match that result until compat-proof records an explicit protocol decision; it SHALL NOT treat the inherited rejection as support or silently fix the transcript order, building on `loki-harness/README.md:13-19` and `quint/transcript.qnt:124-170`.

#### Scenario: Stock and Loki verify the shared proof fixtures in both directions
- **WHEN** `./loki-harness/run.sh` and `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1` run with the manifest fixture IDs
- **THEN** Loki-produced supported proofs SHALL pass the unmodified stock verifier and stock-produced fixtures SHALL pass Loki, while T03 mixed-instance rejection and lookup-order nondeterminism remain classified as documented stock outcomes; the existing harness gates and limitations are `loki-harness/README.md:1-19,49-74`.

### Requirement: Host model checks remain separate from target runtime gates
The release SHALL run Quint and Apalache as host-side CPU-only checks and SHALL report WASM/mobile runtime compatibility separately. Each covered feature SHALL link its verification evidence, limits, and module documentation in the coverage manifest.

#### Scenario: CPU-only release checks run without acceleration
- **WHEN** `python3 quint/ci/run_models.py --release --seed 20261025` runs on host CI
- **THEN** it SHALL require no CUDA, WebGPU, Metal, or Vulkan and SHALL leave wasm32 browser/Node and iOS/Android execution to platform-owned fixture compatibility jobs; the current host-only replay limits are at `quint/mbt/README.md:22-39` and the model documentation boundary is `docs/modules/quint.md:1-28`.

#### Scenario: Feature docs and external QA mappings are complete
- **WHEN** the release coverage audit checks a feature row
- **THEN** it SHALL require a module documentation link, fuzz entry, Lean contract identifier or explicit owner-approved N/A, and the precise result artifact for the current SHA; cache rows SHALL identify the current Rust source or remain blocked if their only replay source is historical, building on `quint/pk_cache.qnt:1-84`, `quint/mbt/src/pk_driver.rs:1-219`, and `docs/modules/quint.md:1-28`.

### Requirement: Cache coverage follows the release-source inventory
The release SHALL determine cache obligations from its source inventory and SHALL NOT infer that modeled recl9 or deferred W12 code ships.

#### Scenario: Cache obligations are inventoried
- **WHEN** `python3 quint/ci/check_coverage.py --base "$V1_BASE_SHA" --head "$V1_RELEASE_SHA"` evaluates the immutable release source inventory
- **THEN** it SHALL require T20 identity and T33 retention model, Rust replay, Lean and documentation mappings for cache code actually included in that tree, or explicit deferrals when none is present, building on `quint/pk_cache.qnt:1-84`, `quint/mbt/src/pk_driver.rs:1-219`, and the source-snapshot caveat at `quint/mbt/README.md:22-39`.

### Requirement: Known stateful QA obligations have explicit coverage status
The release manifest SHALL map or explicitly block the known cancellation, liveness, transcript-shape, and NTT fallback obligations.

#### Scenario: The coverage gate checks known obligations
- **WHEN** `python3 quint/ci/check_coverage.py --base "$V1_BASE_SHA" --head "$V1_RELEASE_SHA"` checks the release manifest
- **THEN** it SHALL report T28 cancellation no-retry, T29 temporal liveness, T30 transcript shapes, and T31 NTT fallback/replay as covered or as named release blockers, building on `quint/scheduler.qnt:73-83`, `quint/transcript.qnt:124-170`, `quint/q2/dispatch.qnt:1-397`, and `quint/q2/REPORT.md:124-130`.
