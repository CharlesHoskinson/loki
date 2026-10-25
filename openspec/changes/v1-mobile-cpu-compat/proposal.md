# Proposal

## Why

Loki currently documents iOS and Android as TODO targets, with no mobile CI or phone evidence. v1 needs CPU builds and a stock 0.8.3 compatibility gate on actual simulator/emulator or device artifacts, plus measured limits, before mobile can be called supported.

## What Changes

### Must-have

- Add a mobile CPU support contract for `aarch64-apple-ios` device and simulator, and `aarch64-linux-android` via the Android NDK.
- Add CI build and smoke-test jobs at Rust 1.76 that compile the stock-default feature set and exercise CPU proving and verification on an Apple Silicon iOS simulator host and an ARM64 Android emulator host (or named physical-device runner). A cross-compile alone will not count as runtime compatibility.
- Build both Loki and stock 0.8.3 for all mobile target triples. Add mandatory iOS simulator, Android emulator, and physical-device compatibility runs, with Loki and stock proving and verifying in both directions using compat-proof-owned fixtures; include PK/VK/SRS/verifier-parameter serialization checks and wrong-input/malformed-proof negatives.
- Require a named low- and mid-range phone matrix and evidence-based numeric peak-RSS, library-size, proof-time, and sustained-throughput budgets. Missing or unmeasured values block release; this proposal does not invent measurements.
- Add a minimal Rust smoke-consumer artifact named `loki-mobile-compat-smoke` to exercise the existing crate API and provide a stable linked artifact for size measurements.
- Document supported SDK/NDK setup, tested OS/device matrix, commands, resource limits, and the phone shapes known to complete successfully.

### Nice-to-have

- Add a minimal downstream Rust app embedding example after target runner and fixture contracts stabilize; this does not add a public Swift/Kotlin ABI.

### Out of scope

- Defer Metal and Vulkan/wgpu arithmetic, GPU-required compatibility, shader capability surveys, and a new app-facing FFI/UniFFI ABI. Keep CPU builds independent of accelerator SDKs and features.
- Reason: no mobile GPU arithmetic exists, Android `shaderInt64` is optional, and a new FFI would require a separate compatibility contract for ownership, errors, cancellation, panic containment, and proving-key memory.

## Capabilities

### New Capabilities

- `mobile-cpu-support`: CPU compilation, stock compatibility, runtime CI, and measured resource limits for iOS and Android.

### Modified Capabilities

None.

## Impact

- Current package facts: `Cargo.toml:13-16` declares edition 2021, Rust 1.76, and package 0.8.3; `Cargo.toml:45-74` shows the stock default and opt-in accelerator features. `src/lib.rs:14-18` rejects combining the newer GPU backend with legacy CUDA. `src/utils/multicore.rs:1-7` exposes Rayon parallelism relevant to mobile resource usage.
- Platform status: `docs/PLATFORMS.md:9-13,264-316,412-415` marks mobile and phone GPU work TODO. `TODO.md:271-291` already calls for a CPU leaf baseline, circuit selection, a 10-proof thermal soak, and a memory-kill test; their release evidence remains open.
- Compatibility foundation: `loki-harness/README.md:3-10,49-55` defines the independent registry oracle, VK/SRS checks, cross-verification, and negative controls. `tests-stock/Cargo.toml:7-19` isolates the exact stock 0.8.3 dependency. Existing harness limitations at `loki-harness/README.md:15-19` remain visible.
- Dependencies: compat-api owns API/default-feature invariants; compat-proof owns shared fixture schema and stock cross-verification; WASM area owns browser/Node execution and shares only portable fixture semantics; fuzz owns the differential corpus; perf owns shared budget methodology; GPU area confirms Metal/Vulkan deferral; docs-release owns release CI orchestration and cross-target publication. Mobile owns the target matrix and device evidence.
- No current mobile binding layer is present: `src/lib.rs:20-30` exposes Rust modules and `src/lib.rs:9` denies unsafe code. UniFFI or a versioned C ABI remains a separate design decision, not a new compatibility promise in this change.
- QA: target builds and simulator/emulator proof/runtime checks are release gates; stock compatibility consumes the compat-proof shared bundle; fuzz adds deterministic seed replay on both mobile runners; Quint records mobile resource/lifecycle limitations without adding a fake native model; Lean records no new arithmetic contract and syncs existing cited contracts at the frozen release SHA. No QA result is claimed by this proposal.
- Documentation per gate: build/runtime lanes document pinned toolchains and command output; device qualification publishes exact phone/OS/fixture and measured limits; FFI and GPU deferrals plus interruption behavior appear in the platform guide and rustdoc.
- Risks: BLST C cross-compilation or SDK drift may block target builds; Rayon can amplify phone memory use; emulators do not establish phone thermal or memory limits; OS process termination cannot resume an in-flight synchronous proof. No numeric baseline is currently available.
