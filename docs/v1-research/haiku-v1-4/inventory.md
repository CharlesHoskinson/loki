# Area 4 (mobile) inventory: current state in loki-proofs

Scope: `~/repos/loki-proofs` at the state read on 2026-10-25. Read only. Nothing was built or run beyond inspection. Items marked UNVERIFIED were not executed.

## Present

- **Crate target and cfg.** No mobile target cfg exists. `Cargo.toml:14` sets `rust-version = "1.76.0"`, and `Cargo.toml:13` sets edition 2021. The only target-gated dependency is `Cargo.toml:227-229`, which gates `getrandom` with `features = ["js"]` on `cfg(all(target_arch = "wasm32", target_os = "unknown"))`. `src/` has no `target_os`/`target_arch` branch for ios, android or wasm32 (grep, no matches).
- **Unconditional deps that matter on mobile.** `rayon` at `Cargo.toml:184-186` (version 1.10, no target gate). `blst` 0.3.16 at `Cargo.toml:154-156`. `loki-cpu-budget = ["blst/no-threads"]` at `Cargo.toml:61`. `getrandom` at `Cargo.toml:179-182` uses `default-features = false` with `features = ["getrandom"]` on `rand_core`.
- **build.rs.** `build.rs:70-74` contains only an x86_64 ADX probe. There is no aarch64, iOS, Android or wasm branch. The `cuda` path at `build.rs:2-6` is feature-gated.
- **Feature flags.** `Cargo.toml:45-74` lists the flags. `default = ["committed-instances"]` at `Cargo.toml:52`. Opt-in flags include `loki-harden`, `loki-multiopen`, `loki-profile` and `loki-deterministic-lookup-order`. GPU flags are `cuda`, `cuda-ntt`, `cuda-srs-registry`, `loki-backend-api`, `loki-sppark`, and `loki-gpu-backend` (`Cargo.toml:63-73`).
- **Backend seam.** `loki-backend-api` is an optional dependency (`Cargo.toml:253-256`). `loki-gpu` is an optional dependency (`Cargo.toml:258`). The GPU crates live in `loki-gpu/crates/{loki-backend-api,loki-gpu}`. The GPU C ABI is `loki-gpu/crates/loki-gpu/src/ffi.rs` and `cuda/include/loki_gpu_ffi.h`. That is CUDA-only.
- **Compatibility harness.** The stock oracle is `midnight-proofs =0.8.3`, pinned as a dev-dependency at `Cargo.toml:217-219` (`package = "midnight-proofs"`). The W2 gate is `loki-harness/` (`loki-harness/README.md`, `loki-harness/run.sh:1-9`). It runs natively on the x86_64 host with `RAYON_NUM_THREADS=1`, a 292 s timeout, and k10/k14/k17. The k20 test is ignored by default. The harness is host-only. Nothing in it targets mobile.
- **Stock 0.8.3 target gates.** Checked at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/midnight-proofs-0.8.3/Cargo.toml:172-178`. It gates `getrandom` `js` on `wasm32-unknown-unknown` and `midnight-curves` `portable` on `cfg(ci_build)`. Stock has no ios or android gate, so any mobile behaviour is new work.
- **Docs.** `docs/PLATFORMS.md:264-317` is the mobile section (section 8, status TODO). It covers API options (Metal for iOS, Vulkan/wgpu for Android), limits (thermal, memory, backgrounding, battery), client-side candidates (proposal, not measured) and a checklist. Lines 309-316 hold the checklist. `docs/PLATFORMS.md:13` marks mobile as TODO. `docs/PLATFORMS.md:27-43` holds the rules and gates G1-G6. `docs/PLATFORMS.md:72-82` holds the API questions O1-O3 (unified memory, device budget, async readback).
- **TODO tracking.** `TODO.md:271-292` holds T67-T72: smallest leaf proof on a mid-range phone (CPU, aarch64), client-side candidates (O8), iOS Metal G1-G3, Android Vulkan `shaderInt64` survey (O6), thermal soak, and memory-kill test (G4). `TODO.md:347-352` holds T86 (O6) and a mobile/browser max-k measurement. `TODO.md:295-324` holds the WASM items T73-T80 that the wasm area also covers.
- **Top-level README.** `README.md:68` and `README.md:218` list mobile as TODO. `README.md:260` notes that Rayon needs the WASM fallback documented in its own README.

## Missing

- **No CI for any mobile or wasm target.** `loki-proofs` has no `.github/` directory (`ls -a` shows none). The only workflow is `loki-gpu/.github/workflows/rust.yml:9` (`runs-on: ubuntu-latest`), and it runs only `cargo check --workspace --locked` at lines 13-14, CPU-only.
- **No FFI or bindings.** No uniffi, JNI, Swift or `wasm_bindgen` code in `src/`, `loki-gpu/crates/loki-backend-api` or `loki-gpu/crates/loki-gpu` (grep, apart from the CUDA C ABI above). No `cdylib`/`staticlib` crate-type is declared in `Cargo.toml:76-80` (`[lib]` section, path only; UNVERIFIED that crate-type is absent in every crate, but none appears in the `[lib]` table).
- **No memory, thermal or size budget** exists in code or docs. The only budget is the API field `device_memory_budget: Option<u64>` (`docs/PLATFORMS.md:80`). No measured number exists for a mobile phone.
- **No Metal or Vulkan code.** `docs/PLATFORMS.md:218-248` is prior-art and plans. `loki-gpu` has no Metal or Vulkan backend (`loki-gpu/crates/loki-gpu` contains CUDA sources only).
- **Toolchain on this host.** Installed rustup targets: `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown` (`rustup target list --installed`). Not installed: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`, `x86_64-linux-android`. Android NDK is not installed (`ANDROID_NDK_HOME` unset, no `~/Android`). Xcode is not present (`xcodebuild` not found). `/usr/bin/clang` exists.

## Implications for the v1 mobile area

1. The mobile CPU build has no pre-existing gate. The first gate must be created: add `aarch64-apple-ios`, `aarch64-apple-ios-sim` and `aarch64-linux-android` to the rustup set, then `cargo check` each (a `cargo check` probe does not link).
2. iOS builds and any iOS device or simulator run need macOS and Xcode. This WSL2 host can only check, not link or run, Apple targets (UNVERIFIED for check-only on this host; will run as a probe).
3. `blst` 0.3.16 build script handles `target_os` `unknown` and `uefi` specially (`blst build.rs:41-53, 92`). It has no iOS or Android branch in the grep. Its C and assembly path will need an NDK or Apple clang at build time (UNVERIFIED until probed).
4. The stock oracle has no mobile gate, so the stock-compat gate on phones must be a Loki-built native-side verifier on the produced bytes, or a stock-built binary cross-compiled to the device target (UNVERIFIED which of these works).
5. Mobile budgets (memory, thermal, size) are unmeasured. TODO T67 needs a physical device.

Probe results and recommendations are in the scout notes that follow this inventory once written.
