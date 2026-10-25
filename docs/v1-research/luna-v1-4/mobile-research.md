# Mobile area research notes

Date: 2026-10-25. Prepared for `v1-mobile-cpu-compat`. The lab inspected source and documentation only; the OpenSpec acceptance commands are planned gates, not results.

## Current code and evidence

- `Cargo.toml:13-16` declares edition 2021, Rust 1.76, and `midnight-proofs` 0.8.3. `Cargo.toml:45-74` keeps `committed-instances` as the default and accelerator/backend flags opt-in.
- No iOS, Android, JNI, Swift, UniFFI, or mobile FFI target path was found. The library manifest at `Cargo.toml:76-80` names its Rust library but does not set `cdylib`/`staticlib` crate types. The GPU C ABI is CUDA-specific and not a mobile API (`loki-gpu/crates/loki-gpu/src/ffi.rs`).
- `build.rs:1-6` only enters CUDA build logic when `cuda` is enabled. `build.rs:70-74` has an x86_64 ADX conditional; no mobile target branch was found.
- Rayon is exposed through `src/utils/multicore.rs:1-7`; CPU MSM uses Rayon and includes a thread-count-dependent chunk path (`src/poly/kzg/msm.rs:211-224`). Phone memory and sustained throughput therefore need measurement, not a host extrapolation.
- The stock harness uses an independent registry `midnight-proofs =0.8.3` package and documents cross-verification, serialization, and negative controls (`loki-harness/README.md:3-10,43-55`). It has no mobile target runner; known shape limitation is documented at `loki-harness/README.md:15-19`.
- `tests-stock/Cargo.toml:7-19` isolates its registry dependency. No iOS/Android target gate appears in the stock 0.8.3 manifest inspected by Haiku (`~/.cargo/registry/src/.../midnight-proofs-0.8.3/Cargo.toml:172-178`).
- `docs/PLATFORMS.md:264-316` calls phone support TODO and considers Metal on iOS, Vulkan/wgpu on Android, thermal and memory behavior, and client-side proof candidates. `TODO.md:271-291` requires a CPU leaf baseline, circuit selection, thermal soak, and memory-kill evidence. No numeric mobile budget is recorded.
- Installed target inventory during this lab initially contained only `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown`; Haiku added the three requested mobile Rust targets, but this host has no Xcode or Android NDK. Its permitted `cargo check` probe failed during BLST's C build because the Android clang wrapper was unavailable (`docs/v1-research/haiku-v1-4/probe-android-check.txt`). This is a toolchain-missing result, not a source incompatibility result.

## Mobile compatibility recommendation

Use separate iOS simulator and Android emulator runtime lanes in addition to target compile jobs. Compile and run both Loki and the unmodified stock 0.8.3 oracle on simulator/emulator and physical iOS/Android targets, with compat-proof's portable fixture schema. Require proof verification in both directions, PK/VK/SRS/verifier-parameter serialization checks, and negative controls. A native-host stock verification of bytes produced by a phone is useful supplemental evidence, not a substitute for target runtime coverage.

Use simulators/emulators for deterministic compatibility. Require named physical low- and mid-range phones to establish numeric per-device peak RSS, installed library size, proof wall time, and ten-consecutive-proof thermal throughput. Release remains blocked until actual numeric thresholds are frozen from those observations. Do not report thresholds as evidence before handset runs.

Defer Metal and Vulkan arithmetic. The Android `shaderInt64` feature is optional; selecting a portable mobile GPU path therefore requires a device capability survey and 32-bit arithmetic path. No Loki Metal/Vulkan code is present. Keep CPU mobile builds independent of GPU dependencies.

Defer a stable C ABI/UniFFI layer. If the product needs Swift/Kotlin entry points, define a narrow separate interface for inputs, proof/key ownership, errors, cancellation, and panic containment, then version it and test it independently. Do not bind the crate's broad Rust API by default.

WASM and native phones share stock fixture bytes and QA contracts where practical, but have separate runtime gates and budgets. Browser/Node threading, linear memory, and HTTP isolation constraints do not establish iOS/Android behavior.

## Outside sources

- Android NDK's [other build systems guide](https://developer.android.com/ndk/guides/other_build_systems) names `aarch64-linux-android` as the arm64 Android triple. Its Clang target includes an API-level suffix (example: `aarch64-linux-android21`); toolchain variables are needed for non-CMake/non-ndk-build native dependencies. This supports pinning NDK and API-level details in CI. It does not establish that Loki's BLST dependency already cross-builds. **Source status: snippet-level** in the scout's practice inventory; validate the current guide during implementation.
- Apple's [Xcode simulator and physical-device guide](https://developer.apple.com/documentation/Xcode/running-your-app-on-simulated-or-physical-devices) search result describes both simulator and paired-device destinations. The page was JS-backed and its Markdown fetch was unsupported, so this is a **snippet-level** pointer only; no target-specific build claim is made. The build/runtime gate should retain both target and OS/runtime identity.
- Android's [emulator acceleration guide](https://developer.android.com/studio/run/emulator-acceleration) says hardware-accelerated emulation requires a system image matching the host CPU architecture. **Source status: snippet-level** in the scout's practice inventory. The mobile plan therefore names ARM64 Linux for an ARM64 Android emulator, or uses a physical Android device. Android's [emulator command-line guide](https://developer.android.com/studio/run/emulator-commandline) also identifies host hypervisors by OS; **source status: snippet-level** in the scout's practice inventory. Emulator host architecture must be part of the retained CI evidence.
- Mozilla's official [UniFFI project](https://github.com/mozilla/uniffi-rs) source is cited at **snippet-level** in the scout's practice inventory; it reports shared-library compilation and generated Kotlin and Swift bindings. This makes UniFFI a viable future binding option, but its versioning and FFI interface would create a separate compatibility surface. Confirm the current project status and supported bindings before selecting a version.
- Khronos' [Vulkan `VkPhysicalDeviceFeatures` reference](https://registry.khronos.org/vulkan/specs/latest/man/html/VkPhysicalDeviceFeatures.html) is cited at **snippet-level** in the scout's practice inventory. It identifies `shaderInt64` as the feature controlling 64-bit signed and unsigned integer use in shaders. Android Vulkan availability is not evidence that this optional feature is present on every phone; verify the current reference and query device support before a future acceleration claim.

## Ownership contracts

- compat-proof (luna-v1-2) owns the canonical fixture schema and stock verification oracle; mobile consumes it and runs both targets.
- docs-release (luna-v1-10) owns workflow integration and consolidated platform publication; mobile supplies target commands, evidence and device matrices.
- fuzz (luna-v1-6) owns generated corpus/seeds; mobile performs deterministic portable corpus replay, not on-device libFuzzer campaigns.
- perf (luna-v1-9) owns methodology; mobile owns handset metrics and per-device limits.
- WASM (luna-v1-3) owns browser/Node execution; mobile owns native Xcode/NDK execution. The only shared runtime artifact is portable compatibility fixture data.

