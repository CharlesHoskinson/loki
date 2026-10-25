# WASM area research notes

Reference repository: `/home/charles/repos/loki-proofs`, revision `eaf09d8`. These are planning observations; successful compile checks are not runtime or compatibility evidence.

## Current code state

- `Cargo.toml:13-16`: crate is Rust 2021 / MSRV 1.76.0.
- `Cargo.toml:45-74`: native default is `committed-instances`; Loki features are opt-in. `cuda`, `cuda-ntt`, `loki-sppark`, and `loki-gpu-backend` are feature-gated, but the GPU backend feature is not target-gated.
- `Cargo.toml:154-185`: BLST 0.3.16 requirement (currently resolves to 0.3.17), `rand_core` 0.6 with `getrandom`, Rayon 1.10 requirement.
- `Cargo.toml:227-229`: target-specific `getrandom` 0.2 with `js` is already enabled for wasm32-unknown-unknown.
- `build.rs:48-53` is entered by the CUDA build path and requires CUDA toolkit and BLST/Sppark headers; WASM CPU gates must leave accelerator features disabled.
- `loki-harness/src/lib.rs:1-11, 87-153`: native differential harness offers stock/Loki cross-verification, equal VK and verifier-parameter checks, wrong-input/empty/appended-byte controls, and no-lookup deterministic byte comparison. It is not a WASM runner.
- `loki-harness/README.md:1-18`: the independent registry stock 0.8.3 oracle currently records native k10/k14/k17, and explicitly says it has no recorded k20 run; mixed committed/plain multi-proof is inherited unsupported T03.
- Scout haiku-v1-3 reports `tests-stock` (unmodified stock 0.8.3 dev dependency) passes `cargo check --target wasm32-unknown-unknown --tests`; this is compile-only and does not execute the stock verifier on a wasm runtime.
- Stock and Loki `src/plonk/mod.rs:363-431` expose `ProvingKey::bytes_length`, `write`, and `read`; the compatibility fixture owner should use these existing APIs and must not invent a new PK encoding.
- `docs/PLATFORMS.md:318-358` and `TODO.md:295-324, 359-368` list wasm compile, browser proof, workers, memory, WebGPU, chain, and verifier questions as open work.
- `src/profile.rs:16-25, 98-115, 169` uses `std::time::Instant` and file I/O behind `loki-profile`. Rust's unsupported wasm32 `Instant` backend panics at runtime; this feature is not qualified and must not be listed as supported without a portable clock/storage backend.
- BLST has no Cargo feature named `no-asm` in the resolved 0.3.17 manifest. Its build script has target-specific portable/no-thread/freestanding handling (`blst-0.3.17/build.rs:47-53, 223-226`); record this actual build behavior rather than specifying a nonexistent feature.

## Permitted feasibility probe

Ran from the reference repository:

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/home/charles/repos/loki-wt/v1/.target-luna-v1-3 cargo check --target wasm32-unknown-unknown --lib
```

Result: passed (`Finished dev profile`, 20.24 seconds) on the local toolchain. `rustup target list --installed` listed `wasm32-unknown-unknown` and `x86_64-unknown-linux-gnu`. The temporary target directory was cleaned with `cargo clean` afterward. This probe did not run browser/Node code, test Rayon workers, compile the stock harness, validate RNG at runtime, or measure memory/module/proof/time budgets.

Scout haiku-v1-3 reports additional permitted checks passed for default features, `--no-default-features`, `loki-cpu-budget,loki-profile`, and `loki-backend-api`; `loki-gpu-backend` failed because the `loki-gpu` build script invokes `nvcc` (`loki-proofs/Cargo.toml:69-73`, `loki-gpu/crates/loki-gpu/build.rs:1-40`). Those results are reported by the scout and were not repeated by this author.

## Outside best practice and constraints

- Rust's official target guide calls `wasm32-unknown-unknown` Tier 2, notes it does not have a browser-vs-Node cfg, and documents that `std::thread::spawn` panics and `std::fs` fails on the target. A browser/Node runner must be explicit; native thread assumptions cannot be inferred. https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html
- `getrandom` 0.2.17 explicitly lists browser entropy via `Crypto.getRandomValues` and Node via `crypto.randomFillSync`; this is why the existing target-scoped `js` feature is appropriate. https://docs.rs/getrandom/0.2.17/getrandom/#webassembly-support
- The getrandom 0.2 README discourages enabling `js` in libraries because it adds wasm-bindgen dependencies and breaks non-JavaScript wasm builds. Loki already scopes it to wasm32-unknown-unknown; v1 support is specifically for browser/Node JavaScript hosts and must document that boundary. https://github.com/rust-random/getrandom/tree/v0.2.17
- `wasm-bindgen-rayon` describes itself as a Web adapter, requires nightly thread support, a `--target web` style output, a rebuilt stdlib with atomics, and `SharedArrayBuffer` enabled by cross-origin isolation. The upstream README does not claim Node worker_threads support, so Node threaded support requires its own proven adapter and test lane, not an assumption based on browser support. https://github.com/RReverser/wasm-bindgen-rayon#usage
- Rayon-core's current wasm32 path falls back to sequential execution without a worker-pool adapter; Rust's target guide also states that `std::thread::spawn` panics on `wasm32-unknown-unknown`. A worker pool is a separate opt-in product. https://github.com/rayon-rs/rayon#usage-with-webassembly and https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html
- The wasm-bindgen guide likewise documents `+atomics`, worker restrictions, and required security headers for shared memory. https://wasm-bindgen.github.io/wasm-bindgen/examples/raytrace.html
- WebAssembly memory32 addresses use 32-bit indices; the 4 GiB address-space ceiling is theoretical. Engines, browsers, devices, allocation patterns, and available memory can impose lower limits, so release budgets require measured host-specific limits. https://www.w3.org/TR/wasm-core/#memories
- Rust docs advise choosing target features deliberately because wasm engines must understand every emitted instruction. SIMD128 should be a separately measured artifact/mode, not a silent default change. https://doc.rust-lang.org/stable/rustc/platform-support/wasm32-unknown-unknown.html#enabled-webassembly-features

## Roadmap position

Must-have: CPU-only `wasm32-unknown-unknown`; browser and Node, each in a default single-thread lane and a separate threaded lane; stock 0.8.3 and Loki verifier builds running on wasm; bidirectional shared proof/key/SRS fixtures and negative controls; numeric memory/module+JS/proof/time budgets by pinned host and mode; CPU-only CI; runtime and platform documentation.

WebGPU/WGSL arithmetic, MSM, NTT, and worker-hosted GPU lifecycle are excluded from v1. `docs/PLATFORMS.md:329-358` and `TODO.md:311-321` describe these as open work; no Loki-compatible BLS12-381 Halo2/KZG implementation or measurements establish a v1 gate. Native stock verification is useful additional evidence but does not replace the required wasm-built stock verifier.

Dependencies: luna-v1-2 owns canonical fixture schema/source pins and compatibility interpretation; luna-v1-4 owns native iOS/Android lanes; luna-v1-6 owns fuzz generation (WASM replays deterministic vectors, not libFuzzer); luna-v1-7 owns target-neutral Quint traces (WASM owns replay); luna-v1-8 owns Lean wire/arithmetic trace at release SHA; luna-v1-9 owns measurement method; luna-v1-10 owns release workflow aggregation and shared platform docs; luna-v1-5 keeps GPU optional and outside CPU gates.
