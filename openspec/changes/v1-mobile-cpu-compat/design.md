# Design

## Context

The crate has no mobile-specific source path, CI workflow, FFI surface, or phone budget. Its Rust 1.76/edition 2021 identity and default feature set are declared in `Cargo.toml:13-16,45-74`; CUDA compilation is isolated behind `cuda` in `build.rs:1-6`. Rayon is part of CPU execution (`src/utils/multicore.rs:1-7`). The existing W2 harness compares Loki with an independent registry stock dependency, but its current commands run natively (`loki-harness/README.md:3-10,21-41`). The local machine lacks Xcode and Android NDK, so this lab has not established mobile build or runtime success.

## Goals / Non-Goals

**Goals:**

- Make native CPU builds and simulator/emulator compatibility checks mandatory release evidence for iOS and Android.
- Establish a repeatable handset matrix for memory, library size, proof time, and sustained thermal throughput.
- Preserve stock defaults and isolate all optional acceleration.

**Non-Goals:**

- Implement Metal, Vulkan, WebGPU, or any mobile GPU arithmetic.
- Promise that every circuit or recursion chain can be proved on a phone.
- Add or stabilize a Swift/Kotlin FFI or UniFFI API in this change.

## Decisions

1. **Keep mobile support on the existing Rust crate surface.** Build both Loki and the stock-default independent package for `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `aarch64-linux-android`. No target-specific behavior or feature flag is needed for CPU arithmetic. Pin and record Xcode/iOS SDK, Android NDK, Android API level, Rust 1.76, host architecture, and runner images in the eventual workflow. Android's official NDK guidance uses the `aarch64-linux-android` target with an API-level suffix for clang; that is an NDK toolchain detail, not a change to Rust's target triple ([Android NDK cross-compilation guide](https://developer.android.com/ndk/guides/other_build_systems)).

   Alternatives considered: add a `mobile` feature now, which would duplicate target selection and risk changing feature resolution; introduce a UniFFI layer, which would create a new public ABI before ownership, error mapping, panic handling, and exposed proof/key types have a compatibility contract. Both are deferred. UniFFI can generate Swift and Kotlin bindings from a deliberately designed Rust interface, but it is a separate app-facing API decision ([Mozilla UniFFI](https://github.com/mozilla/uniffi-rs)).

2. **Run the stock oracle on simulator, emulator, and physical phones.** Use unmodified `midnight-proofs =0.8.3` with its default `committed-instances` feature alongside Loki's same default feature in an isolated compatibility runner; do not use `default-features = false` as `tests-stock/Cargo.toml:16` does for its specialized verifier package. The current W2 harness uses stock defaults (`loki-harness/Cargo.toml:18-25`; Loki default is `Cargo.toml:51-52`). Consume compat-proof's proposed `loki-harness/fixtures/v1/manifest.jsonl` bundle with fixture ID, stock source pin, target/runtime identity, proof/key artifacts, and SHA-256 digests. Run both prover directions and verify each proof with the opposite implementation; round-trip proof, VK, PK, SRS, and verifier-parameter bytes with malformed and wrong-public-input negatives. Loki's existing PK methods are in `impl ProvingKey` at `src/plonk/mod.rs:354-370` (`bytes_length`) and `src/plonk/mod.rs:372-465` (`write`, `read`, `to_bytes`, `from_bytes`); do not create a mobile-only serialization contract. Native-host verification of phone-produced files is useful supplemental evidence, but does not replace target runtime checks. Current stock and Loki harness details and known fixture limits are in `loki-harness/README.md:3-19,43-55`.

   Alternatives considered: only compile the Loki library and verify on the host (insufficient runtime/target evidence); cross-compile stock without executing it (also insufficient); build a target-compatible stock runner and execute both packages under simulator/emulator (selected). Exact runner commands follow the compat-proof fixture contract.

3. **Separate reproducible CI lanes from physical-device evidence.** Compile iOS device/simulator and Android targets at Rust 1.76; run compatibility on iOS simulator, Android emulator, and physical devices. Run an ARM64 Android emulator only on an ARM64 host, or use a physical Android device; Android documents that accelerated emulation requires a system image matching host architecture ([Android emulator acceleration](https://developer.android.com/studio/run/emulator-acceleration)). Apple Xcode distinguishes simulator and paired physical-device destinations ([Apple Xcode device execution](https://developer.apple.com/documentation/Xcode/running-your-app-on-simulated-or-physical-devices)). Physical named low- and mid-range phones supply peak RSS, stripped `loki-mobile-compat-smoke` artifact size, per-proof time, and a ten-proof soak. That Loki-only Rust consumer avoids baking stock's additional code into the size metric. The first measurement run freezes numeric thresholds; a later run must qualify against them. No measured phone numbers exist today.

   Alternatives considered: set a guessed universal memory/time cap (not defensible across phone classes); make budgets informational (does not create a release gate); allow a baseline run to qualify itself against thresholds generated during that same run (circular); freeze measured per-device limits then pass a later candidate against them (selected).

4. **Keep Metal/Vulkan out of v1 mobile acceptance.** The Android Vulkan `shaderInt64` capability is optional, so a portable Android acceleration claim would require 32-bit arithmetic and device surveys; `docs/PLATFORMS.md:270-284` and `TODO.md:279-291` mark this as unfinished. iOS Metal has no Loki implementation. CPU compatibility and device evidence do not depend on these paths. Preserve Metal/Vulkan as documented post-v1 candidates. Vulkan feature support is exposed through device feature queries, not guaranteed across devices ([Khronos Vulkan feature reference](https://registry.khronos.org/vulkan/specs/latest/man/html/VkPhysicalDeviceFeatures.html)).

5. **Treat process termination as incomplete work, not `ContextLost`.** A CPU proof call has no GPU context to restore, but mobile OS suspension or process termination can interrupt an in-flight operation. The runner must not publish an artifact unless the call completes; the current synchronous API does not promise suspend/resume or proof-job checkpoint recovery. T72's `ContextLost` wording at `TODO.md:291` applies to a device-context path and must not be copied as a CPU API promise. Defer FFI until the project defines how borrowed inputs, long-running proving, cancellation, errors, panic containment, and proving-key memory ownership cross the language boundary.

## Risks / Trade-offs

- [Cross toolchain or BLST C build fails on a target] → Pin SDK/NDK and add target build lanes before claiming support; report each target independently.
- [Emulator memory/thermal data differs from phones] → Require named physical-device compatibility in addition to simulator/emulator CI; use physical runs for budgets and document OS revisions.
- [Large circuits exceed app memory or are killed] → Publish measured maximum supported shape and require explicit application-level admission; do not claim universal phone support.
- [Stock fixture schema is not runner-friendly] → Resolve through compat-proof's shared producer/schema before implementation; treat absent fixture output as a blocker.
- [New FFI accidentally freezes unstable internals] → Keep all bindings outside this change and require a separate versioned interface specification.

## Migration Plan

No migration or public API change is planned. Add the target lanes, shared compatibility runner, device budget record, and platform guide before release. Roll back a target support claim if its compile, runtime, or budget gate fails; keep CPU use available on other passing targets.
