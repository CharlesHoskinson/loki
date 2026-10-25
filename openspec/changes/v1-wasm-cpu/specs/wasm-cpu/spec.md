# Spec Delta

## Purpose

Provides a CPU-only WebAssembly target for Loki in browser and Node environments, with an explicit threading mode, stock-prover interoperability gates, and resource limits suitable for release decisions.

## ADDED Requirements

### Requirement: Browser and Node CPU builds
Loki SHALL build and execute the stock-default CPU prover on `wasm32-unknown-unknown` in browser and Node environments without requiring GPU support or changing native default features. Verification: `cargo check --locked --target wasm32-unknown-unknown --lib` and the browser/Node smoke harness; source anchors: `Cargo.toml:45-74, 227-229` and `docs/PLATFORMS.md:320-327`.

#### Scenario: Single-threaded smoke on browser and Node
- **WHEN** CI builds the default CPU-only artifact and runs the smallest canonical proof fixture in each pinned supported browser and Node runtime
- **THEN** both hosts produce a parseable proof/VK/SRS bundle and the gate records runtime and artifact digests without enabling GPU features

### Requirement: WebAssembly entropy source
A WASM build SHALL expose the JS-backed cryptographic entropy source to browser/Node consumers of `OsRng`; proof callers continue to supply their `RngCore` as in the stock API. Verification: `wasm:runtime --rng` exercises both hosts and an unavailable-entropy trap; source anchors: `Cargo.toml:176-182, 227-229` and `loki-harness/src/lib.rs:87-100`.

#### Scenario: Browser and Node cryptographic randomness
- **WHEN** the smallest proof fixture runs under browser `crypto.getRandomValues` and the pinned Node crypto host adapter
- **THEN** the JS-backed host entropy supplies the requested seed, while unavailable host entropy follows rand_core 0.6 `OsRng` failure semantics and the fixture runner exports no proof bundle

### Requirement: Explicit threaded WASM mode
WASM threads SHALL be opt-in and isolated from the default build. Verification: `wasm:threads` runs worker fixtures in browser and Node, and `cargo tree --locked -e features --target wasm32-unknown-unknown` checks default isolation; source anchors: `Cargo.toml:51-74, 184-185` and `docs/PLATFORMS.md:322-327`.

#### Scenario: Default build remains single-threaded
- **WHEN** a downstream crate builds Loki for `wasm32-unknown-unknown` without the opt-in thread feature
- **THEN** it obtains the single-thread artifact without nightly atomics, SharedArrayBuffer, COOP/COEP, or wasm-bindgen-rayon requirements

#### Scenario: Threaded browser and Node workers
- **WHEN** CI builds the explicit threaded artifact with its pinned toolchain and initializes workers in a cross-origin-isolated browser and the pinned Loki-owned Node worker adapter
- **THEN** a proof fixture completes on multiple workers, the worker count is recorded, and unsupported host isolation or worker setup fails before proof generation with a documented error

### Requirement: Bidirectional stock 0.8.3 interoperability
The WASM gate SHALL run stock 0.8.3 and Loki wasm32 verifiers against shared fixtures in both directions. Verification: `wasm:stock-compat` checks proof acceptance, existing VK/PK/SRS encodings, and negative cases; source anchors: `loki-harness/src/lib.rs:1-11, 87-100`, `src/plonk/mod.rs:363-431, 447-464`, and `loki-harness/README.md:1-18`.

#### Scenario: WASM-produced proof accepted by stock
- **WHEN** the WASM prover emits each required shared fixture
- **THEN** the stock 0.8.3 wasm verifier accepts its proof and decodes identical VK, PK, and SRS encodings, while wrong public inputs and malformed or appended proof bytes follow stock rejection behavior

#### Scenario: Stock-produced proof accepted by WASM
- **WHEN** the stock 0.8.3 fixture generator emits the corresponding proof, VK, and parameters
- **THEN** Loki's WASM verifier accepts the valid fixture and rejects wrong public inputs, while malformed or appended proof bytes follow stock behavior

### Requirement: Measured WASM resource budgets
The release SHALL enforce numeric budgets frozen before candidate qualification in `loki-harness/fixtures/v1/wasm-budgets.json` for memory, module-plus-JS size, proof size, and elapsed time per pinned host version, mode, and fixture ID; missing or post-qualification limits SHALL block release. Verification: `wasm:budget` validates the frozen record and enforces it against candidate measurements; source anchor: `docs/PLATFORMS.md:324-326, 351-358` (current unmeasured budget checklist).

#### Scenario: Budget record is absent or incomplete
- **WHEN** a release candidate has no numeric limit and measured value for any required host, mode, or fixture size
- **THEN** the WASM release gate fails closed and reports the missing field

#### Scenario: Budget is frozen before candidate qualification
- **WHEN** the budget record is created or changed after qualification of the candidate begins
- **THEN** `wasm:budget` rejects the run and requires a new baseline review before qualification restarts

#### Scenario: Measured values exceed limits
- **WHEN** peak memory, generated artifact size, proof size, or elapsed time exceeds its committed numeric limit
- **THEN** CI blocks the WASM release gate and records the host, mode, fixture, limit, and observed value

#### Scenario: Theoretical address-space limit is not treated as usable memory
- **WHEN** platform documentation describes wasm32 memory
- **THEN** it states the 4 GiB address-space ceiling separately from measured browser/Node/device allocation limits and does not claim proofs fit solely because they fit below 4 GiB

### Requirement: BLST target baseline
The release SHALL measure BLST's portable no-thread wasm32 path against a matched native reference. Whole-Loki SIMD128 and Rayon worker-count variants SHALL be reported separately. Verification: `wasm:blst-baseline` records pinned inputs, toolchain, worker counts, and median timings without claiming speedup; source anchors: `Cargo.toml:154-155, 184-185` and `docs/PLATFORMS.md:324, 432`.

#### Scenario: BLST baseline matrix is complete
- **WHEN** the WASM baseline job runs on the frozen fixture set
- **THEN** it records the native reference and wasm32 BLST portable path, Loki builds with and without SIMD128, and Rayon worker counts 1, 4, 8, and 16 where supported; it records unsupported counts and keeps BLST thread behavior distinct from Rayon workers

### Requirement: WASM recursion-chain workload
The v1 WASM workload envelope SHALL run the small recursion fixture `sherlock-recursion-chain-smoke-v1` from `loki-harness/fixtures/v1/manifest.jsonl` in every browser/Node host/mode. Perf owns the fixture configuration. Verification: `wasm:chain` emits wall-time, peak-memory, proof-size, and stock-verification results for the required smoke; a missing smoke case blocks release. Source anchor: `docs/PLATFORMS.md:351-358` (T80).

#### Scenario: Recursion-chain target budget
- **WHEN** the release candidate runs `sherlock-recursion-chain-smoke-v1` on each pinned browser/Node × single/threaded lane
- **THEN** it records elapsed time, peak linear memory, proof bytes, and verifier verdict, and the lane fails if its frozen numeric limit is exceeded

#### Scenario: Larger chain exceeds a host workload envelope
- **WHEN** a larger production recursion chain exceeds the measured memory or time envelope on a host/mode
- **THEN** the result records the largest supported fixture and explicit unsupported/timeout limit, while the required smoke fixture remains a passing release gate

### Requirement: CPU-only WASM CI and acceleration boundary
Every v1 WASM CPU release SHALL have separate CI gates for browser and Node single-thread execution, browser and Node threaded-worker execution, stock compatibility, and resource budgets; CPU gates SHALL NOT require CUDA or WebGPU. Verification: the release workflow's WASM job matrix plus `wasm:ci` reports all four host/mode lanes independently; source anchors: `docs/PLATFORMS.md:318-358` and `Cargo.toml:48-50, 63-73`.

#### Scenario: CPU release gate runs without an accelerator
- **WHEN** the WASM release workflow runs on a CPU-only runner
- **THEN** all required single-thread build, runtime, stock-compatibility, and budget checks complete without CUDA or WebGPU availability

#### Scenario: WebGPU remains outside v1 WASM scope
- **WHEN** a release build is selected for v1 WASM
- **THEN** it uses the CPU backend and documents WebGPU/WGSL arithmetic, MSM, and NTT as deferred work until Loki-specific correctness, compatibility, and performance evidence exists

### Requirement: Unsupported WASM features fail at compile time
The WASM target SHALL reject unsupported accelerator and profiling features before native build tools or runtime-only panics occur. Verification: `cargo check --target wasm32-unknown-unknown --features loki-gpu-backend` and the equivalent `loki-sppark` and `loki-profile` checks produce targeted diagnostics; source anchors: `Cargo.toml:59-73`, `build.rs:48-53`, and `src/profile.rs:24, 131, 164, 169`.

#### Scenario: CUDA-backed features are rejected before nvcc
- **WHEN** a WASM consumer enables `loki-gpu-backend` or `loki-sppark`
- **THEN** compilation fails with a target-specific feature diagnostic before invoking `nvcc`

#### Scenario: Non-portable profiling is rejected before runtime
- **WHEN** a WASM consumer enables `loki-profile`
- **THEN** compilation fails with a diagnostic that names unsupported `Instant`/file output, unless a portable implementation replaces those calls

### Requirement: WASM QA and platform documentation
The release SHALL document WASM modes, toolchains, randomness, isolation, worker/completion behavior, budget-record schema, limits, supported JavaScript hosts, and compatibility gates; it SHALL replay shared fuzz vectors and Quint traces and link Lean coverage. Verification: `wasm:docs`, `wasm:fixtures`, and the release Lean gate; source anchors: `docs/PLATFORMS.md:80-82, 318-358`, `loki-harness/README.md:1-18`, and `Cargo.toml:51-52, 227-229`.

#### Scenario: Target guidance and QA trace are published
- **WHEN** the release candidate is assembled
- **THEN** its platform documentation names all four browser/Node × single/threaded lanes, pinned host versions, the limitations of the wasm32 address space, the numeric budget-record schema, links to compatibility, fuzz, Quint/MBT, and Lean evidence at the release commit, states that `loki-profile` is compile-time rejected until portable, states that non-JavaScript `wasm32-unknown-unknown` consumers are unsupported, and explains the calling-thread blocking behavior and worker/message completion handoff

#### Scenario: Mobile support is reported independently
- **WHEN** platform documentation reports target support
- **THEN** it reports iOS and Android CPU gates separately and does not infer mobile support from a passing WASM build
