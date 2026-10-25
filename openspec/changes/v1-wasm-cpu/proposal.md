# Proposal

## Why

The reference prover is still native-first, and its platform guide marks WASM work as TODO. Loki v1 requires usable CPU prover builds on `wasm32-unknown-unknown` in both browser and Node environments, with compatibility evidence and published limits rather than an unsupported target label.

## What Changes

### Must-have for v1

- Define a first-class CPU-only WASM build and runtime gate for browser and Node, using a portable single-threaded configuration as the default.
- Add a separately built, opt-in `wasm-threads` path backed by Rayon/Web Workers for both browser and Node; keep its toolchain flags, host adapters, and limits separate from default builds.
- Provide bidirectional interoperability evidence with both Loki and unmodified stock `midnight-proofs` 0.8.3 compiled to wasm32, including proof acceptance, VK/PK/SRS encodings, and negative verification controls. Native stock verification is additional evidence.
- Add target build, runtime, stock-compatibility, and resource-budget checks to v1 CI. Require a checked-in budget record with numeric linear-memory, module-size, proof-size, and elapsed-time limits; missing or exceeded limits block release.
- Measure the portable BLST wasm32 path against a matched native reference, with scalar, SIMD128, and Rayon worker-count configurations reported separately; run a required small recursion-chain smoke fixture in every host/mode lane and report larger-chain outcomes against each measured workload envelope.
- Reject unsupported CUDA-backed and `loki-profile` WASM feature combinations at compile time unless those features gain portable implementations.
- Document browser/Node setup, random-source requirements, thread isolation, limits, tested fixtures, and v1 acceleration boundaries.

### Nice-to-have

- Measure a SIMD128 artifact and worker scaling beyond the required correctness matrix; retain the portable scalar artifact as the baseline.
- Revisit memory64 only after the supported browser/runtime matrix can run the required compatibility gate.

### Out of scope

- WebGPU/WGSL arithmetic, MSM, NTT, and a GPU-backed browser prover are deferred because no Loki-compatible BLS12-381 Halo2/KZG implementation or measurements establish a safe v1 release scope.
- Memory64 is not a v1 release target because browser support is incomplete; measure and document within wasm32's 4 GiB theoretical address-space ceiling and lower host limits.
- A public JavaScript package/API contract is not introduced; the required deliverable is a target runner and fixture gate, avoiding an unreviewed new public API surface.

## Capabilities

### New Capabilities
- `wasm-cpu`: CPU-only WASM build, execution, optional threads, stock interoperability, and resource-budget behavior.

### Modified Capabilities
- None.

## Impact

This change plans target-specific Cargo configuration and build features, a browser/Node × single/threaded test harness, shared stock-compatibility fixtures, CI jobs, and platform documentation. Source anchors below are repo-relative to the reference Loki code repository `~/repos/loki-proofs` at `eaf09d8`. Its manifest already enables `getrandom` 0.2's `js` backend for `wasm32-unknown-unknown` at `Cargo.toml:227-229`; the crate uses Rayon at `Cargo.toml:184-185`, BLST at `Cargo.toml:154-155`, and defaults to `committed-instances` at `Cargo.toml:51-52`. The current native harness cross-verifies proof bytes but does not run either verifier on wasm (`loki-harness/src/lib.rs:87-100`; `loki-harness/README.md:1-18`). A scoped wasm32 library `cargo check` passed on reference revision `eaf09d8`; this is compile-only evidence. Current platform gaps and candidate gates are recorded at `docs/PLATFORMS.md:318-358` and `TODO.md:295-324`.

Stock 0.8.3 remains the API, default-feature, serialization, transcript, and proof-acceptance oracle. WASM configuration must be additive and target-scoped, with `wasm-threads` opt-in. No native default behavior or public API changes are proposed. Mobile CPU builds remain a separate first-class area owned by luna-v1-4; this proposal does not infer iOS or Android support from WASM results. GPU acceleration is not needed by any WASM CPU gate.

QA includes target `cargo check`, all four browser/Node × single/threaded runtime lanes, wasm32 builds of both stock and Loki for bidirectional verification, resource measurements, deterministic shared differential-fuzz vector replay, Quint-owned target-neutral model-trace replay, and Lean coverage of unchanged arithmetic/wire invariants. Fuzzing and Quint model generation remain owned by their respective areas; WASM owns fixture consumption. Documentation belongs in the platform guide, user-facing setup notes, and rustdoc for any public wrapper.

Risks include BLST's portable C/no-assembly path, Rayon worker assumptions on `wasm32-unknown-unknown`, differing Node/browser host APIs, nightly and cross-origin-isolation requirements for threaded WASM, and browser/device limits below the 4 GiB wasm32 address-space ceiling. The `getrandom` 0.2 `js` feature is presently enabled for the WASM target, but dependency feature unification, both runtime entropy paths, and threaded Node support still need runtime verification. `OsRng` failures follow rand_core 0.6 panic/trap behavior rather than a fallible proof API; the runtime gate must verify no proof bytes are exported. `loki-profile` uses `Instant` and file I/O and must be compile-time rejected on WASM until portable (`src/profile.rs:16-25, 98-115, 169`). The opt-in CUDA features currently fail on wasm because their build path is not target-gated; WASM release checks must reject them before attempting `nvcc` (`Cargo.toml:64-73`).

Dependencies: luna-v1-2 owns shared 0.8.3 fixture schema and compatibility oracle; luna-v1-4 owns native mobile lanes and shares platform build conventions; luna-v1-6, luna-v1-7, and luna-v1-8 own fuzz, Quint/MBT, and Lean mappings; luna-v1-9 owns performance measurement methodology and budget evidence; luna-v1-10 owns release CI integration and cross-platform docs; luna-v1-5 owns GPU scope and must keep GPU feature forwarding isolated from CPU WASM gates.
