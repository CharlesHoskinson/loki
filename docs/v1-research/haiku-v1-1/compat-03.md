# compat-03: platform compatibility implications (wasm32 browser/Node, iOS, Android)

Scope: compile feasibility and compatibility implications for the default CPU build of Loki (`~/repos/loki-proofs`, main `eaf09d8`). Nothing here is a runtime result. Targets were probed with `cargo check --lib` only, as the rules allow.

## 1. Official target status (Rust platform support)
Source: https://doc.rust-lang.org/nightly/rustc/platform-support.html
- `wasm32-unknown-unknown`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`, `wasm32-wasip1-threads`, `wasm32-unknown-emscripten` are all Tier 2 without host tools, with std available.
- The page says Tier 2 means "guaranteed to build", and "Automated tests are not always run so it's not guaranteed to produce a working build." Passing a Tier 2 build is not a runtime guarantee. Runtime gates have to be ours.
- Installed locally (`rustup target list --installed`): `wasm32-unknown-unknown`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-linux-android`. Not installed: `wasm32-wasip1-threads`, `x86_64-linux-android`.

## 2. Local feasibility probes (`cargo check --lib --offline`, CARGO_BUILD_JOBS=2, target dir `~/repos/loki-wt/v1/.target-haiku-v1-1`)
Log: `~/repos/loki-wt/v1/docs/v1-research/haiku-v1-1/cargo-check-targets.log` (wasm, Android, iOS first attempt). The Android retry is in `/tmp/claude-1000/-home-charles/9dfb451b-c903-4483-a9b8-656ad9ac91de/scratchpad/android-check.log`.

| Target | Result | Cause |
|---|---|---|
| wasm32-unknown-unknown | PASS, `Finished dev profile` in 20.58 s | Rust code and the `blst` C build both succeed |
| aarch64-linux-android | FAIL in `blst v0.3.17` build script | `failed to find tool "aarch64-linux-android-clang"` (no NDK, `ANDROID_NDK_HOME` unset). A retry with host `clang --target=aarch64-linux-android21` fails compiling `blst/src/server.c` (no bionic sysroot). Environment blocker, not a Rust error. |
| aarch64-apple-ios | FAIL in `blst` build script | `failed to find tool "xcrun"`, no Apple SDK on this Linux/WSL host. Environment blocker. |

Consequence: the Android and iOS compile gates cannot be run on this host. They need an NDK-equipped and a macOS/Xcode runner. Local evidence is only that wasm32 compiles.

## 3. Implications from the code
1. **Entropy on wasm32 (default graph): already explicit, a correction to an earlier note.** Both manifests declare the wasm target dependency `[target.'cfg(all(target_arch = "wasm32", target_os = "unknown"))'.dependencies.getrandom] version = "0.2", features = ["js"]`: stock `Cargo.toml:172-174` and Loki `Cargo.toml:227-229`. `cargo tree --target wasm32-unknown-unknown -i getrandom@0.2.17 -e features` shows `js`, `js-sys` and `wasm-bindgen` enabled, which is that declaration at work. My earlier suggestion that this relied on feature unification was wrong. The `getrandom` docs say wasm32-unknown-unknown is "not automatically supported" without the `wasm_js` (0.3/0.4) or `js` (0.2) backend (https://docs.rs/getrandom/latest/getrandom/#webassembly-support). The remaining gap is the `getrandom 0.3.x`/`0.4.x` lines: they are not in the wasm32 normal graph, so they do not affect the default wasm build, but any future dependency that brings them in would need its own backend. The `getrandom 0.3.4` appearance in the host graph was not traced.
2. **Threads on wasm32.** Stock and Loki both use `rayon::scope` in `parallelize` (stock `src/utils/arithmetic.rs:64-70, 150-165`; Loki the same lines, `src/utils/arithmetic.rs:66-70, 152-165`). A search summary says the global pool falls back to serial execution on wasm32-unknown-unknown, while a custom `ThreadPoolBuilder` pool fails there. The official Rayon docs were not fetched. The only source read is a secondary one (https://github.com/gradium-ai/xn/pull/86), so this is unverified. Loki's production code uses only the global pool. All `ThreadPoolBuilder` uses are inside `#[cfg(test)]` modules (`src/poly/mod.rs:335` in `mod cpu_tests` at line 320; `src/utils/arithmetic.rs:430,444` under `#[cfg(test)]` at line 402; `src/poly/kzg/msm.rs:256,291` in `mod cpu_tests` at line 229). Risk: the new default-on rayon calls (`par_sort` at `src/plonk/lookup/prover.rs:391`, `par_chunks_mut` in `src/utils/arithmetic.rs:184-198`) run on the global pool and need a single-threaded wasm32 test to confirm the fallback. This is a shared stock risk as well.
3. **Wasm threads (`wasm32-wasip1-threads`, browser SharedArrayBuffer).** Threads on `wasm32-unknown-unknown` need a nightly `build-std` rebuild with atomics plus cross-origin isolation headers, according to the wasm-bindgen-rayon docs (https://docs.rs/wasm-bindgen-rayon, search summary; the README flags change between versions, unverified here). This matters for the "wasm-threads" lane, not for single-threaded CPU.
4. **`cfg` gates for mobile and wasm.** `loki-cpu-budget` enables `blst/no-threads` (`Cargo.toml` feature). It is off by default, so the default build still uses the threaded blst. Mobile and wasm budgets that want the non-threaded blst need this feature explicitly.
5. **`blst` version drift.** The direct dependency is `blst = "0.3.16"` (non-optional, added in Loki's manifest), but `Cargo.lock` resolves `blst 0.3.17`. The build-script failures above are from `blst 0.3.17`. The C build is the first platform blocker on iOS and Android, so pin the `blst` version the platform CI uses.
6. **Stock oracle on wasm.** astra's review requires the stock verifier compiled to WASM as well, not only the Loki build. Not checked here. `tests-stock/` and the `loki-harness` cross-verify are native-only (no wasm target found in their manifests; unverified).

## 4. Proposed gates for the platform lanes (not run)
- `cargo check --lib --target wasm32-unknown-unknown` (default features) PASS now. Add the same for `--features committed-instances` explicitly and for the Loki feature sets in scope.
- iOS and Android: `cargo check --lib --target aarch64-apple-ios` and `--target aarch64-linux-android` on a runner with Xcode and an NDK (`CC_*`/`AR_*` set to the NDK clang). Local host cannot run them.
- wasm32 runtime: a Node run of the stock-verifier compat fixtures (single-threaded), with `getrandom` `js` explicit.
- Stock compile for each platform, so the oracle is the same target (astra's requirement).

## 5. Open points (unverified)
- The Rayon fallback behaviour on wasm32 comes from a secondary source (the xn PR), not from Rayon's own docs. Confirm with a test.
- The `getrandom 0.3.4` source in the host graph is not traced.
- No runtime test on any platform has been run.
