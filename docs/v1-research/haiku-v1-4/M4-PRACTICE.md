# M4-PRACTICE: mobile Rust practice for v1 (area 4)

Status: based on WebSearch result snippets only. No page was fetched in full. Items marked UNVERIFIED need a primary-source read before a spec cites them.

## Build and toolchain
- Android: cargo-ndk automates NDK env setup; ANDROID_NDK_HOME overrides auto-detection. Triple aarch64-linux-android; the NDK clang wrapper carries the API level suffix (for example aarch64-linux-android21-clang). Source: https://developer.android.com/ndk/guides/other_build_systems and https://github.com/rib/cargo-ndk
- cc-based C deps (blst) need CC_aarch64_linux_android and AR_aarch64_linux_android set to the NDK wrapper and llvm-ar. UNVERIFIED for blst 0.3.16 specifically (snippet did not cover it).
- Local probe (this host, 2026-10-25): `cargo check --lib --target aarch64-linux-android` with CARGO_BUILD_JOBS=2 failed in blst's cc-rs step: `failed to find tool "aarch64-linux-android-clang"`. Log: ~/repos/loki-wt/v1/docs/v1-research/haiku-v1-4/probe-android-check.txt. Cause: NDK not installed. The probe confirms the NDK is a hard prerequisite for any Android cargo check of loki-proofs.
- rustup targets added: aarch64-linux-android, aarch64-apple-ios, aarch64-apple-ios-sim.

## FFI
- UniFFI (Mozilla) generates Swift and Kotlin bindings; current version reported 0.32.0 (index snippet). Needs crate-type cdylib and staticlib, `#[uniffi::export]`. Used in production by Firefox mobile, Ferrostar and Bitwarden SDK. Source: https://github.com/mozilla/uniffi-rs and https://mobilesystemdesign.substack.com/p/multiplatform-with-rust-on-ios-2c4 (tutorial targets UniFFI 0.29, so pin a version).
- Recommendation for v1: UniFFI for a small surface (verify, prove leaf, key ids) and a C ABI as fallback. Keep the proving API off the FFI surface until the compat gate passes on device.

## Metal and Vulkan
- Metal: 64-bit integer math in MSL is listed as available from Metal 3 and the Apple3 GPU family (A9/A10) upward. Source: https://developer.apple.com/metal/feature-sets and https://developer.apple.com/metal/limits (snippet only). Forum note on uint64 buffers (https://developer.apple.com/forums/thread/655127) is secondary. UNVERIFIED against the current MSL 4.1 spec text.
- Vulkan: shaderInt64 is optional (https://registry.khronos.org/vulkan/specs/latest/man/html/VkPhysicalDeviceFeatures.html, already cited in docs/PLATFORMS.md:275). Survey per device is TODO T70/T86.

## Budgets
- No published numbers were found in this pass. Thermal, memory-kill and battery limits remain measurement items (TODO T71, T72, O7). Do not claim numbers.

## v1 recommendation (mobile)
1. v1 mobile = CPU build for aarch64-apple-ios and aarch64-linux-android, with the NDK pinned, behind a cfg-free feature flag so default stock behaviour is unchanged.
2. Metal and Vulkan acceleration: out of v1. Document as post-v1 (matches docs/PLATFORMS.md section 8 options; no code exists).
3. FFI: UniFFI surface limited to verify and leaf prove; compat gate runs off-device (see M4-STOCK).
4. CI: add GitHub Actions jobs for cargo check on both triples plus an NDK-installed Android job. iOS build job needs a macOS runner (UNVERIFIED runner availability).
