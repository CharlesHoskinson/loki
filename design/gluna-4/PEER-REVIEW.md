# Peer review: gluna-8 build, FFI, and CUDA gating

**Evidence convention:** unless marked **SOURCE**, **MEASURED**, **ESTIMATE**, or **OPEN QUESTION**, statements are review observations or proposed corrections, not claims that a build or GPU execution occurred.

Review scope: read-only review of `crates/loki-gpu/{Cargo.toml,build.rs,src/lib.rs,src/ffi.rs,cuda/include/loki_gpu_ffi.h}`, CUDA stubs, `.github/workflows/rust.yml`, and `design/BUILD.md`. Review is against the assigned skeleton and architecture constraints. No Rust build, CUDA build, or GPU run was performed as part of this review. gluna-8's fixes were reviewed on 2026-10-25.

## Findings

### Resolved — CUDA link search path

- **File/lines:** `crates/loki-gpu/build.rs:41-44,69-73`
- **Original issue:** The script could select `NVCC` with no `CUDA_HOME` and omit the `cudart` link path.
- **Correction reviewed:** `build.rs` now requires and canonicalizes `CUDA_HOME`, resolves `NVCC`, rejects an NVCC path outside that toolkit's `bin` directory, compiles with `cc::Build`, and emits the matching `lib64` search path. The build guide documents `NVCC` only as an override under the selected toolkit. This removes both the missing link path and compiler/runtime mismatch cases from the original finding. A follow-up check against the locked `cc` 1.10 source found that `Build::env("NVCC", ...)` would not select NVCC because compiler selection reads the inherited environment snapshot. The current script correctly calls `env::set_var("NVCC", &nvcc)` after validation and before `Build::new`, so the selected compiler is the exact validated executable.

### Resolved pending commit — CI lockfile

- **Original issue:** CI used `--locked` before a `Cargo.lock` existed.
- **Correction reviewed:** `Cargo.lock` is now present, contains the optional `cc` build dependency, and the CI command remains `cargo check --workspace --locked`.
- **Remaining integration condition:** `Cargo.lock` is untracked in the shared worktree at review time. The coordinator must include it in the repository commit; otherwise a clean checkout will reproduce the original failure.
- **Severity:** No code blocker if the lockfile is committed.

## Confirmed properties

- `default = []`, and `build.rs` returns before CUDA discovery unless the explicit `cuda` feature is enabled. The normal workspace check therefore does not invoke `nvcc` or link CUDA.
- The C header and Rust declarations use fixed-width integer status codes and matching byte-pointer signatures. The native MSM/NTT stubs return `UNSUPPORTED` without dereferencing inputs or touching output.
- The Rust backend reports no capabilities and each operation remains `Unsupported`; compiling the CUDA stubs does not claim that any operation ran on a GPU.
- The API has no CUDA pointer or stream types. The V1 FFI documentation explicitly limits the byte-wire ABI to synchronous stubs and reserves residency/async work for a versioned handle/stream interface.
- CUDA compilation includes a native `sm_120` target. The skeleton does not initialize a device or advertise compute-capability support, so the future runtime probe remains an implementation gate rather than a current claim.

## Limitation

This review covers the fail-closed skeleton only. It does not validate NVCC compilation, static-link behavior, runtime device discovery, CUDA memory safety, kernel correctness, or the Lean obligations. Those require the separately requested compile and future differential/stock-verifier gates.
