# haiku-v1-3 inventory: WASM (area 3) current state

Scope: Loki worktree `~/repos/loki-wt/v1` (branch `loki/v1-roadmap`), derived from `~/repos/loki-proofs` main. Read-only; no production edits. Probes: `rustup target add wasm32-unknown-unknown` (added by me), `cargo check --target wasm32-unknown-unknown --lib` with CARGO_BUILD_JOBS=2 and CARGO_TARGET_DIR=`~/repos/loki-wt/v1/.target-haiku-v1-3`, `cargo tree`.

## 1. Measured probe results (wasm32-unknown-unknown, host rustc 1.99.1, cargo 1.99.1)

| Invocation | Result |
|---|---|
| `cargo check --target wasm32-unknown-unknown --lib` (default features: `committed-instances`) | PASS, exit 0, 0 warnings |
| same with `--no-default-features` | PASS |
| same with `--features loki-cpu-budget,loki-profile` | PASS (compile only; see runtime hazard R2) |
| same with `--features loki-backend-api` | PASS |
| same with `--features loki-gpu-backend` | FAIL: loki-gpu build script runs nvcc (`loki-gpu/crates/loki-gpu/build.rs`, host-only `cc`/nvcc path) and dies with `features.h not found` under the wasm target. See R1. |

Logs: `~/repos/loki-wt/ids/haiku-v1-3` is the identity dir; probe logs are in `/tmp/claude-1000/-home-charles/9dfb451b-c903-4483-a9b8-656ad9ac91de/scratchpad/haiku-v1-3/` (`check-default.log`, `check-nodef.log`, `check-budget-profile.log`, `check-backend-api.log`, `check-gpu.log`).
Not run (unverified): `cargo check --tests` on wasm (dev-deps include `stock = midnight-proofs =0.8.3`, Cargo.toml:202-208); any wasm-threads build (needs nightly, `+atomics,+bulk-memory`, build-std); any run under a wasm runtime (no wasm-bindgen-test, no node runner probed); `wasm-bindgen-rayon` (not in Cargo.toml).

## 2. Target and dependency wiring (file:line)

- `Cargo.toml:14` `rust-version = "1.76.0"`; `Cargo.toml:13` edition 2021.
- `Cargo.toml:45-73` `[features]`: `loki-cpu-budget = ["blst/no-threads"]` (line 61); `loki-profile = []` (line 59); `loki-backend-api` (63); `loki-sppark` = backend-api + cuda + cuda-ntt (64-68); `loki-gpu-backend = ["loki-backend-api", "dep:loki-gpu", "loki-gpu/cuda"]` (69-73); `cuda = ["dep:sppark", "dep:cc", "dep:loki-cuda-runtime"]` (49); `cuda-ntt` (48); `default = ["committed-instances"]` (52).
- `Cargo.toml:154-155` `blst = "0.3.16"` (resolves to blst 0.3.17 in Cargo.lock:115-116). On wasm32 `blst/build.rs` sets `target_no_std` and `no-threads` automatically, compiles portable C with `-ffreestanding` (blst build.rs:47-53, 223-226), so `loki-cpu-budget` is redundant on wasm (matches docs/PLATFORMS.md:327).
- `Cargo.toml:181-182` `rand_core` 0.6 with `getrandom` feature; `Cargo.toml:184-185` `rayon = "1.10"` (resolves rayon-core 1.13.0, Cargo.lock:1144).
- `Cargo.toml:227-229` `[target.'cfg(all(target_arch = "wasm32", target_os = "unknown"))'.dependencies.getrandom] version="0.2" features=["js"]`. This is the correct wasm RNG wiring. Lockfile has getrandom 0.2.17 (Cargo.lock:552), 0.3.4 (565) and 0.4.3 (577); `cargo tree --target wasm32-unknown-unknown` shows only 0.2.17 is reached (midnight-curves, midnight-proofs, rand_core 0.6.4). 0.3/0.4 are native-only. Good, no `getrandom_backend` cfg needed today.
- `Cargo.toml:202-208` dev-dep `stock = midnight-proofs =0.8.3`: wasm compile of dev/test graph is unverified.
- `Cargo.toml` has no `[target.wasm32...]` block for rayon, blst or threads, and no `wasm-bindgen` dependency. No wasm test harness.
- `Cargo.toml:154` blst is a non-optional, unconditional dependency; that is fine for the CPU wasm path.

## 3. Runtime hazards found by reading code (not yet executed)

- R1 (BLOCKER for GPU-on-wasm isolation): `loki-gpu-backend` and `loki-sppark` enable `cuda` unconditionally with no `target_os`/`target_arch` gate (`Cargo.toml:64-73`; `loki-gpu/crates/loki-gpu/Cargo.toml:12,36`; `loki-gpu/crates/loki-gpu/build.rs:1-40`). The build script is gated on `feature = "cuda"` only, not on the target, so cross builds to wasm (and mobile) try nvcc. Breaks the requirement that WASM/mobile CPU gates cannot depend on GPU (`openspec/config.yaml`, KICKOFF). Fix belongs to compat-api/gpu owners, but wasm needs a `cfg(not(target_arch="wasm32"))` gate or a `cargo:` guard.
- R2 (runtime panic on wasm32-unknown-unknown): `src/profile.rs:24,104-205` uses `std::time::Instant::now()`; module is gated by `loki-profile` (`src/lib.rs:25-26`). `Instant::now()` is unsupported on wasm32-unknown-unknown (std panics at runtime); this only matters when `loki-profile` is enabled on wasm. Also `src/profile.rs:169` writes a JSONL file via `OpenOptions` (`:1-30` imports), which has no filesystem on wasm. Needs a no-op or `performance.now()` backend under cfg. Verify with a runtime test (unverified).
- R3 (OsRng in non-test code): `src/poly/kzg/mod.rs:28,70` `ParamsKZG::unsafe_setup(k, OsRng)` is used on the public `gen_params` path. On wasm this works only through getrandom 0.2 `js` (Cargo.toml:227-229). Good, but a CSPRNG check (crypto.getRandomValues) is still needed for the threat model (docs/PLATFORMS.md:326). Tests also use OsRng (`src/poly/domain.rs:639-697`, `src/plonk/prover.rs:1008-1070`).
- R4 (rayon): `src/utils/multicore.rs:1-` re-exports rayon; 49 rayon call sites in `src/` (grep). rayon-core 1.13.0 `src/lib.rs:20-30` documents that on `wasm32-unknown-unknown` the implicit global pool falls back to sequential execution (join runs serially). So a non-threaded wasm build is functionally correct but single-threaded. Real parallelism needs `wasm-bindgen-rayon` plus atomics (docs/PLATFORMS.md:323, TODO.md:303). Not yet done.
- R5 (process/proc in tests only): `src/poly/kzg/msm.rs:273-292` (`#[cfg(all(target_os="linux", feature="loki-cpu-budget"))]`) spawns a child process and reads `/proc/self/task`. Test-only, not on wasm. `src/poly/domain/gpu_tests.rs:374-375` and `src/poly/kzg/msm/cuda/tests.rs` similarly spawn processes (GPU tests). Wasm test runs must exclude these.
- R6 (env vars): `src/poly/domain.rs:431` (`LOKI_NTT_CPU`) and `src/poly/kzg/msm/cuda.rs:28` read `std::env::var`. On wasm these return Err/None, so the CPU fallback is the default. Harmless, but document it.
- R7 (threads in non-test code): `std::thread` appears only in `src/profile.rs:320` (test) and `src/poly/kzg/msm/cuda/backend.rs:116` (cuda path). No thread spawn on the CPU path, so the CPU path compiles and runs single-threaded on wasm without atomics.
- R8 (memory): wasm32 linear memory is capped at 4 GiB (docs/PLATFORMS.md:325). No code-level memory accounting exists for proof size. Unmeasured.

## 4. Existing docs and TODOs (status)

- `docs/PLATFORMS.md:13` "WASM (TODO)"; section 9 `:320-358`: 9.1 CPU wasm build (target, wasm-bindgen, wasm-bindgen-rayon, simd128, 4 GiB, RNG via getrandom js); 9.2 WebGPU (`:360-400`, wgpu/WGSL, 32-bit limbs, no blocking readback, device loss to ContextLost). Checklist `:351-358` is all unchecked. Platform matrix row `:415` "WASM CPU (wasm32) | TODO" and WebGPU row for MSM/NTT/quotient TODO.
- `docs/PLATFORMS.md:432-434` open questions O9 (wasm BLST baseline) and O11 (stock verifier in wasm vs native).
- `TODO.md:295-304` T73 (audit crates for wasm32 compile), T74 (CPU build in a browser, smallest circuit, G3), T75 (wasm-bindgen-rayon, scaling 4/8/16 workers); `:323` T80 (chain comparison, G6); `:359-360` T89 (O9 blst baseline); `:368` (browser bytes vs native verifier). All open.
- `README.md:68` and `:218` (WASM TODO), `README.md:260` (rayon WASM fallback with link).
- `docs/modules/loki-gpu.md:29` and `docs/modules/cuda-kernels.md:57` say WASM has no code.
- `openspec/config.yaml:9-10` and `:26`, `:39` require WASM/mobile impact in every change (already in config).

## 5. CI, test and fuzz coverage relevant to wasm

- No `.github/` directory in `~/repos/loki-wt/v1` (`ls -a`), so no repository CI config exists for any target. Gates live in `scripts/verify-main.sh`, `scripts/verify-w9.sh`, `scripts/w9-cpu-matrix.sh`; grep for `wasm` in `scripts/` returns nothing.
- Fuzz: `fuzz/fuzz_targets/` = msm_diff, ntt_diff, proof_parse, proof_roundtrip, soundness. None build for wasm32; no `wasm` in `fuzz/` except Cargo.lock.
- Stock-compat harness: `loki-harness/` (rayon dep `loki-harness/Cargo.toml:26`, env `RAYON_NUM_THREADS` at `loki-harness/src/lib.rs:174`), and `tests-stock/`. Neither has a wasm target.
- No `wasm-bindgen-test`, no `wasm-pack`, no node runner in repo (grep of Cargo.toml and scripts).
- MSRV 1.76 (`Cargo.toml:14`) vs. wasm rustup: wasm32 target is installed here on rustc 1.99.1 only; MSRV parity on wasm32 unverified.

## 6. Gaps (what is missing for v1 wasm)

1. No wasm CI job or script; no stated wasm test runner (wasm-bindgen-test or node).
2. R1: GPU features are not target-gated; wasm `loki-gpu-backend` build fails (measured).
3. R2: `Instant` and file I/O in `loki-profile` are unsupported on wasm32-unknown-unknown (code reading).
4. No `wasm-bindgen-rayon` dependency, no atomics build, no `cross-origin isolation` story; threads path unverified.
5. No stated memory/proof-size/time budgets for wasm (T73-T80 open; O9/O11 open).
6. Stock-compat gate (loki-harness) never run in wasm (proposal: run native verifier on wasm-produced bytes, O11).
7. WebGPU path: no code, only design text (PLATFORMS.md:360-400).

## 7. Branches / mappings

Unmerged `loki/p3-srs`, `p4-ntt`, `p5-quotient`, `p7-chain`, `w10-gpu-quotient` (from `~/repos/loki-proofs` per context.md) are not inspected for wasm; if they add CUDA or Instant use they will hit R1/R2 again. Not checked.

## 8. External URLs cited so far (to be confirmed by the wasm-best-practice and wasm-webgpu tasks)

- https://github.com/rayon-rs/rayon#usage-with-webassembly (cited in README.md:260)
- https://github.com/RReverser/wasm-bindgen-rayon (cited in docs/PLATFORMS.md:323)
- https://github.com/WebAssembly/memory64 (cited in docs/PLATFORMS.md:325)
- https://www.w3.org/TR/webgpu/ and https://www.w3.org/TR/WGSL/ (cited in docs/PLATFORMS.md:361-364)

Verified in code (no URL needed): rayon-core 1.13.0 fallback text at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rayon-core-1.13.0/src/lib.rs:20-30`; blst build wasm branches at `.../blst-0.3.17/build.rs:47-53, 223-226`.

## 9. Addendum: stock 0.8.3 on wasm32 (answers astra REVIEW wasm challenge)

- `cd ~/repos/loki-wt/v1/tests-stock && cargo check --target wasm32-unknown-unknown --tests` PASSES (exit 0; log `/tmp/claude-1000/-home-charles/9dfb451b-c903-4483-a9b8-656ad9ac91de/scratchpad/haiku-v1-3/stock-wasm-tests.log`). The `tests-stock` crate pulls `midnight-proofs v0.8.3` as a dev-dependency (`cargo tree` shows it under `loki-stock-verifier-check`). So the unmodified stock verifier type-checks on wasm32-unknown-unknown.
- Not executed: no wasm runtime ran the stock verifier. Unverified whether `tests-stock` contains runnable wasm-compatible tests.
- `loki-harness` wasm check not run.
