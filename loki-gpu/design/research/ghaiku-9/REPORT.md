# ghaiku-9 REPORT: Rust CUDA build and API options for loki-gpu

Access date: 2026-10-25 (UTC). Scope: `cc` + nvcc, `cudarc`, `cust`, plus NVIDIA toolkit linkage and sm_120 facts.
Labels: SOURCE = read from a primary page or file; CODE = result of a command I ran here; MEASURED = host observation; ESTIMATE; OPEN = unverified.

## Summary (3 lines)
1. Recommend `cc` (MIT OR Apache-2.0) to compile our own `.cu` files with nvcc, plus `cudarc` 0.19.10 (MIT OR Apache-2.0) for the driver/runtime API. Do not depend on `cust` or `rustacuda`: both are stale.
2. Use nvcc `-gencode arch=compute_120,code=sm_120` (or `sm_120a` for arch-specific features). CUDA 13.4.3 accepts all three forms (CODE). Link `cudart` statically by default (`cc` default); the NVIDIA EULA allows redistributing `libcudart_static.a` with our applications (SOURCE).
3. Risk: `cudarc` pins its CUDA bindings per feature (`cuda-13040` = CUDA 13.4). Its `dynamic-loading` default avoids link-time CUDA libs, which matters for CI without a GPU (CODE: the probe passed `cargo check`, but this host has a GPU and driver, so the no-driver case is untested).

## 1. Candidate crates

| Crate | Version (crates.io) | Last published | License (Cargo.toml) | Role | Assessment |
|---|---|---|---|---|---|
| `cudarc` | 0.19.10 | 2026-09-24 | MIT OR Apache-2.0 | safe driver/NVRTC/cuBLAS wrapper, dynamic or static link | Recommended for host-side driver API. Active. CUDA 13.0–13.4 feature flags (SOURCE README). |
| `cc` | 1.10.0 | 2026-10-23 | MIT OR Apache-2.0 (MSRV 1.65.0) | build-script compiler driver; `.cuda(true)` invokes nvcc | Recommended for building `.cu` kernels. Has `cudart("static"|"shared"|"none")` (SOURCE lib.rs). |
| `cust` (Rust-GPU/rust-cuda) | 0.3.2 | 2022-02-16 | MIT OR Apache-2.0 | CUDA Driver API bindings | Not recommended. Last release is 2022, so CUDA 13 support is unverified (OPEN). |
| `rustacuda` | 0.1.3 | 2021-07-28 | Apache-2.0 OR MIT | older Driver API wrapper | Not recommended. Unmaintained since 2021 (SOURCE crates.io). |
| `cuda-sys` (termoshtt/accel) | 0.2.0 | 2018-06-13 | MIT | raw FFI | Not recommended. Stale. |

Rust-CUDA project (`github.com/Rust-GPU/Rust-CUDA`): README license is Apache-2.0 OR MIT (SOURCE). Its last commit was 2026-09-30 (SOURCE GitHub API). Its README links a 2025-08-11 status blog. That project is about Rust-to-PTX kernels (`cuda_std`, `nvvm`), which we do not need, because our kernels are CUDA C++ compiled with nvcc.

## 2. cudarc facts (SOURCE: README, `Cargo.toml`, docs.rs)
- Feature table: driver API, NVRTC, cuBLAS, cuBLASLt, NCCL, cuDNN, cuSPARSE and cuFILE support dynamic load, dynamic link and static link. `cuFFT` supports dynamic load only. `cuSOLVER` and `cuSPARSE` are listed as dynamic-load or dynamic-link only.
- CUDA versions: `-F cuda-<major>0<minor>0`, e.g. `cuda-13040` for 13.4. Listed range is 11.4–11.8, 12.0–12.9, 13.0–13.4.
- Default: `dynamic-loading` ("will not require any libraries to be present at build time"). Opt-in: `dynamic-linking` or `static-linking`.
- `driver` module is behind feature `driver` (CODE: the first build failed until I added it).
- Licence: MIT OR Apache-2.0 (SOURCE Cargo.toml).

Implication: with `dynamic-loading`, the Loki build does not link `libcuda`/`libcudart`, so `cargo check` works on a no-GPU CI runner. The GPU driver is found at runtime, which fits our WSL2 host where `libcuda.so` lives in `/usr/lib/wsl/lib` (MEASURED: file present there). Caveat: `cuda-version-from-build-system` needs nvcc, so pin an explicit `cuda-13040` feature in our Cargo.toml.

## 3. CODE: cargo check probe (run here, not in loki-gpu)
Scratch crate at `/tmp/claude-1000/-home-charles/9dfb451b-c903-4483-a9b8-656ad9ac91de/scratchpad/ghaiku9-probe/` (outside the repo, as required):
- `cudarc = { version = "=0.19.10", default-features = false, features = ["cuda-13040", "dynamic-loading", "driver"] }`
- `cargo check` finished with exit 0 (cudarc and libloading compiled, probe checked).
- First attempt failed only because `driver` is feature-gated. That is my usage error, not a cudarc issue.
- Limit: this host has an RTX 5090 and libcuda, so the probe does not prove a build with no driver installed. OPEN: test on a runner with no libcuda.

## 4. nvcc and sm_120 (SOURCE: nvcc docs; CODE: nvcc 13.4.92 at `/home/charles/.local/opt/cuda-13.4.3/bin/nvcc`)
- The nvcc manual's virtual-architecture list includes `compute_120`, `compute_120f` and `compute_120a`, and the real list includes `sm_120`, `sm_120f`, `sm_120a` (SOURCE).
- The manual says arch-specific targets such as `sm_90a` add both arch-specific and generic virtual code (SOURCE).
- CODE: a trivial `.cu` compiled with exit 0 under `-arch=sm_120`, `-gencode arch=compute_120f,code=sm_120`, and `-gencode arch=compute_120a,code=sm_120a`. The last one produced an `sm_120a` cubin.
- OPEN: the meaning of the `f` family suffix (a community gist says it embeds a broader Blackwell family cubin; not checked against NVIDIA's text). For our RTX 5090 (sm_120), `sm_120` is the safe baseline. Use `sm_120a` only if we need arch-specific instructions, and check that they are used.
- Release-note snippets (search results only, not opened): CUDA 13.4 release notes mention a Tile IR compiler crash fix on sm_120. The CUDA 13.0 developer blog says offline compilation was dropped only for architectures below 7.5, so sm_120 is unaffected. Both are OPEN until read.

## 5. Linkage and licence (SOURCE: NVIDIA EULA page, `docs.nvidia.com/cuda/eula/index.html`)
- Attachment A (section 2.6) lists files that "may be distributed with applications developed by you". The list includes `libcudart.so`, `libcudart_static.a` and `libcudadevrt.a` on Linux.
- So statically linking `cudart` into a shipped binary is allowed for our Apache-2.0 crate. ESTIMATE: `libcuda.so` (the driver) is not in that list, so we must load it from the installed driver, not ship it.
- The `cc` default is `cudart("static")`. Keep it, and make the choice explicit in the build script.
- ESTIMATE: the CUDA-runtime sections of Attachment A do not change the Apache-2.0 licence of our own code. A licence lawyer should confirm before we publish binaries.

## 6. Actionable recommendation for loki-gpu
1. Rust side: `cudarc = { version = "=0.19.10", default-features = false, features = ["cuda-13040", "dynamic-loading", "driver"] }`. Pin the exact version and CUDA feature.
2. CUDA side: a `build.rs` using `cc::Build::new().cuda(true)` with `.flag("-gencode").flag("arch=compute_120,code=sm_120")`, and `.cudart("static")`. Keep the `.cu` sources in `loki-gpu/cuda/`.
3. Dependencies, licence and CI: all of `cudarc`, `cc`, `libloading` (dependency of cudarc) are MIT or Apache-2.0 (SOURCE). Record them in `design/PROVENANCE.md` (owner: gluna-10). Add `cargo-deny` or similar to check licences in CI. OPEN: exact licence string of `libloading` 0.9.0 not read.
4. CI: `cargo check` should run without GPU or driver. Real kernel tests need the RTX 5090 runner (gpu-runner-2).

## 7. Risks
- `cudarc` bindings per CUDA version: a 13.5 toolkit needs a new cudarc release. Risk: medium.
- `cust` and `rustacuda`: stale, do not use. Risk of a dead dependency if chosen.
- Dynamic loading hides missing-driver errors until runtime. Our error model must map `CUresult` errors to `Unsupported`/fallback (the design needs this).
- nvcc and CUDA 13.4 are a local install (`~/.local/opt/cuda-13.4.3`). CI needs the same path or `NVCC` env var (SOURCE cc docs: `NVCC` variable).
- The probe used `cudarc` 0.19.10 from crates.io; a git-pinned version may differ.

## 8. Sources (all accessed 2026-10-25)
- cudarc README: https://raw.githubusercontent.com/coreylowman/cudarc/main/README.md ; docs: https://docs.rs/cudarc ; repo https://github.com/chelsea0x3b/cudarc
- cudarc crates.io metadata: https://crates.io/api/v1/crates/cudarc
- cust: https://crates.io/api/v1/crates/cust ; Cargo.toml https://raw.githubusercontent.com/Rust-GPU/Rust-CUDA/main/crates/cust/Cargo.toml
- Rust-CUDA README: https://raw.githubusercontent.com/Rust-GPU/Rust-CUDA/main/README.md
- rustacuda: https://crates.io/api/v1/crates/rustacuda
- cc: https://crates.io/api/v1/crates/cc ; source https://raw.githubusercontent.com/rust-lang/cc-rs/main/src/lib.rs (cuda and cudart APIs)
- nvcc manual: https://docs.nvidia.com/cuda/cuda-compiler-driver-nvcc/index.html
- CUDA 13.4 release notes: https://docs.nvidia.com/cuda/archive/13.4.1/cuda-toolkit-release-notes/index.html (snippet only)
- NVIDIA EULA: https://docs.nvidia.com/cuda/eula/index.html
- Community gist (unverified): https://gist.github.com/agirault/d0d66c12c85dbc2662c95569ef039a1d
- CUDA 13.0 blog (snippet only): https://developer.nvidia.com/blog/navigating-gpu-architecture-support-a-guide-for-nvidia-cuda-developers/

## Limits
- No GPU or driver-free runner tested. No code written in loki-gpu. No git writes.
- Not read: `libloading` licence file, cudarc docs on error mapping, cc-rs docs beyond the API lines.
