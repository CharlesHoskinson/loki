# ghaiku-2 REPORT: sppark reuse, licence and build interface

Scope: sppark (Supranational, Apache-2.0) only. Access date 2026-10-25 (UTC).
Not consulted, per the clean-room rules: open-icicle source, lab-haiku-1/H21.md and its sim scripts.
No GPU was run. No git writes. Label key: SOURCE (web), CODE (read locally), MEASURED (none), ESTIMATE, OPEN QUESTION.

## 1. Exact package versions

- CODE: two sppark crates are in the local registry cache, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`: `sppark-0.1.14` and `sppark-0.1.16`.
- SOURCE (crates.io API, read 2026-10-25): latest is 0.1.16, published 2026-10-20T10:35:02Z, license Apache-2.0, not yanked. Earlier: 0.1.15 (2026-06-04), 0.1.14 (2025-09-06), 0.1.13 (yanked). All versions are listed as Apache-2.0.
- CODE: `/home/charles/repos/loki-proofs/Cargo.lock` pins `sppark 0.1.16`. Its `Cargo.toml` line 209 has `version = "=0.1.16"`, `features = ["cuda"]`, optional behind the `cuda` feature.
- CODE: 0.1.14 and 0.1.16 differ in `ff/mont_t.cuh`, `ff/bls12-381-fp2.hpp`, `ec/affine_t.hpp`, `ec/jacobian_t.hpp`, `ntt/kernels*.cu`, `ntt/parameters.cuh`, `util/gpu_t.cuh`, `util/cuda2hip.hpp` and `polynomial/div_by_x_minus_z.cuh`. 0.1.16 adds two `#![allow(...)]` lines to `src/lib.rs`. The MSM header `msm/pippenger.cuh` is identical in both. Any reuse should be pinned to 0.1.16 and diffed again on upgrade.

## 2. Rust-facing API (what the crate actually exports)

- CODE, `src/lib.rs`: the Rust surface is FFI plumbing only:
  - `Error { code: i32, str }`, a `#[repr(C)]` error with `Drop` that calls libc `free`, and `Display`/`String` conversions;
  - `Gpu_Ptr<T>`, an opaque device-pointer handle with `Drop`/`Clone` that call `drop_gpu_ptr_t` and `clone_gpu_ptr_t` (C symbols, cfg `cuda` or `rocm`);
  - `Slice<U>`, a `#[repr(C)] {ptr, len}` view with `From` impls for slices and `Slice::new`;
  - `NTTInputOutputOrder` (NN/NR/RN/RR), `NTTDirection` (Forward/Inverse) and `NTTType` (Standard/Coset) as `#[repr(C)]` enums;
  - an optional `build` module behind the `build` feature.
- CODE: there is no Rust MSM or NTT function. The algorithms are C++ templates that the consumer must instantiate in its own `.cu` file and expose through `extern "C"`:
  - `msm/pippenger.cuh`: `class msm_t<bucket_t, point_t, affine_t, scalar_t, ...>` with `invoke(point_t&, points, npoints, scalars, bool mont = true, size_t ffi_affine_sz)`, and the free function `mult_pippenger<bucket_t, point_t, affine_t, scalar_t>(out, points, npoints, scalars, mont, ffi_sz)` returning `RustError`. It also has a destructor `~msm_t()`.
  - `ntt/ntt.cuh`: `class NTT` with protected static helpers (`bit_rev`, `LDE_powers`) and public `enum class InputOutputOrder`, `Direction`, `Type`, `Algorithm { GS, CT }`.
  - `polynomial/*.cuh`: evaluate, prefix-op, and div-by-(x−z) kernels.
  - `util/rusterror.h`: `RustError { int code; const char* str }`, returned by value.
- SOURCE (README, read 2026-10-25): the README says the repo has a `rust/` directory and that the Rust and Go interfaces are "[TBD]". It does not document a Rust MSM or NTT API.
- Implication: sppark is a header-only C++/CUDA template library. The Rust crate contributes only the link and FFI glue. Loki already works this way (see section 4).

## 3. Curve and field representation

- CODE, `ff/bls12-381.hpp`: `fp_mont = mont_t<381, BLS12_381_P, M0, RR, one, Px8>` and `fr_mont = mont_t<255, ...>`. The Montgomery form uses 32-bit limbs on device: Fp has 12 limbs (`BLS12_381_P[12]`) and Fr has 8 limbs (`BLS12_381_r[8]`). The constants are device-side `__constant__`.
- CODE, `ff/mont_t.cuh`: the generic `mont_t<N, ...>` is `static const size_t nbits = N`. Other fields in the same directory are alt_bn128, bls12-377, pasta, goldilocks, baby_bear, mersenne31 and gl64. `mont32_t` is a separate 32-bit variant.
- CODE, `ec/affine_t.hpp`, `ec/jacobian_t.hpp`, `ec/xyzz_t.hpp`: point types, with `__CUDA_ARCH__` guarded code paths.
- CODE, `ntt/parameters/bls12_381.h`: twiddle and root tables for BLS12-381 Fr.
- OPEN QUESTION: whether the 32-bit limb layout is best for sm_120 is a performance question for later. It is not a correctness question here. Loki's BLS12-381 representation must be checked for bit-compatibility against the Montgomery form that sppark expects (this depends on Loki's `Fp`/`Fr` layout, which I did not read in this task).

## 4. Build interface and sm_120

- CODE, sppark `build.rs` (0.1.16): it locates `nvcc` (honouring `NVCC`), requires CUDA >= 11.4 (parsed as f32 from `release X.Y,`), compiles a probe `src/cuda_available.cpp` to detect a working GPU, then `cc::Build` with `.cuda(true)` compiles `util/all_gpus.cpp` into `libsppark_cuda.a` and emits `cargo:rustc-cfg=feature="cuda"`. It also sets `cargo:ROOT` (exported as `DEP_SPPARK_ROOT` because of `links = "sppark"`). A ROCm path also exists.
- CODE: the sppark build sets no explicit `-arch`/`-gencode` flag for the CUDA path. The toolchain default applies. ESTIMATE: this is not sm_120-specific. Any sm_120 build needs the consumer to compile its own kernels with an explicit gencode.
- CODE, `/home/charles/repos/loki-proofs/build.rs` (the working consumer pattern):
  - reads `DEP_SPPARK_ROOT` and `DEP_BLST_C_SRC` for headers;
  - asserts that `msm/pippenger.cuh` contains exactly one `~msm_t()` and writes a patched copy into `OUT_DIR` with `~msm_t() noexcept(false)`, so release-time CUDA errors can reach the C ABI catch. This is a string patch against a pinned upstream file, and it is fragile on upgrade;
  - compiles `cuda/msm.cu` with `nvcc`: `cudart("static")`, `-std=c++17`, `-gencode=arch=compute_{A},code=[sm_{A},compute_{A}]` where `A = LOKI_CUDA_ARCH` (default 120), `FEATURE_BLS12_381`, `TAKE_RESPONSIBILITY_FOR_ERROR_MESSAGE`, and `__ADX__` when the host has ADX.
- CODE: the CUDA toolkit is at `~/.local/opt/cuda-13.4.3` (per goal.md). sppark's version regex accepts 13.x. MEASURED: none. Whether sppark 0.1.16 compiles cleanly with CUDA 13.4 for sm_120 has not been verified here. OPEN QUESTION, to be checked on the host via gpu-runner-2 when a build is approved.
- Consequence for the design: the legacy path should keep the sppark-crate build for its headers, and the consumer owns the kernel build with an explicit arch. Do not rely on sppark's own `all_gpus.cpp` compile for sm_120 correctness.

## 5. Licence and NOTICE obligations

- CODE: every sppark source file I read carries `// Copyright Supranational LLC` and `// Licensed under the Apache License, Version 2.0, see LICENSE for details.` with `SPDX-License-Identifier: Apache-2.0`.
- CODE: the crate tarball has no LICENSE or NOTICE file. Its `include` list (`/Cargo.toml`, `/build.rs`, `/src/**`, `/sppark/{ec,ff,msm,ntt,polynomial,util}/**`) leaves them out. `find` over the unpacked 0.1.16 tree shows neither file.
- SOURCE (WebFetch of `raw.githubusercontent.com/supranational/sppark/main/LICENSE`): the text is the full Apache-2.0 licence, version 2.0, January 2004, with the standard appendix placeholder. The fetch did not name a copyright holder. A WebFetch summary is not a byte-level check, so treat it as UNVERIFIED-BYTE.
- SOURCE: the README does not mention a NOTICE file or extra attribution. The Apache 4(d) reference to NOTICE is generic licence text, and I found no sppark NOTICE file to reproduce. OPEN QUESTION: confirm there is no upstream NOTICE with `git ls-files` on a clone (no clone was made).
- Actionable: when copying any sppark code into `loki-gpu`, keep the Supranational copyright line and the Apache-2.0 header in each copied file. In `loki-gpu/NOTICE`, record "sppark, Copyright Supranational LLC, Apache-2.0, pinned 0.1.16" with the source path and the tarball checksum, and cite it in `design/PROVENANCE.md`. Do not copy the licence text into the source tree unless the copies are redistributed without the upstream repo.
- Reuse safety: calling sppark through FFI, as loki-proofs does, needs only the attribution above. Copying code triggers the header and NOTICE rules.

## 6. What can be reused safely, and what stays legacy

- Reuse as a legacy backend (safe now): link the pinned `sppark =0.1.16` headers and instantiate `msm_t` / `NTT` from a consumer `.cu` with an explicit sm_120 gencode, exactly as loki-proofs does. This needs attribution only. The required header patch on `~msm_t()` must stay checked by assertion.
- Reuse by copying (needs review first): the BLS12-381 Montgomery constants and the generic `mont_t` (32-bit limbs) are candidates for copying into loki-gpu. Each copied file keeps its header and gets a PROVENANCE entry. Not decided here.
- Stays legacy: the `~msm_t` destructor hack, the `all_gpus.cpp` build path and the ROCm path. They are not needed for loki-gpu's sm_120 target.
- Do not reuse without a fresh review: any code from `open-icicle` (excluded), and any ICICLE-derived code (not examined).

## 7. Limits and checks still needed

- No compile has been run. The sm_120 + CUDA 13.4 build of sppark 0.1.16 is unverified (OPEN QUESTION, needs host run via gpu-runner-2).
- The SOURCE facts about the upstream LICENSE rely on a WebFetch summary, not on a byte comparison with the local crate, which has no LICENSE file at all.
- The sppark README describes Rust and Go interfaces as TBD, so the upstream Rust API is not a stable public contract. Pin the version and treat the C++ templates as the real interface.
- The Loki-side BLS12-381 field representation (`Fp`/`Fr` limb layout and Montgomery R) was not compared to sppark's `mont_t` here. Owner: whoever does the field-layout compatibility test (ESTIMATE: gluna).

## 8. Actionable implications

1. Pin `sppark =0.1.16` as the legacy MSM/NTT backend; keep its headers and the `~msm_t` assertion.
2. Do the sm_120 gencode in the consumer build (the loki-gpu crate), never through sppark's own build path.
3. Add the sppark NOTICE and PROVENANCE entries before any copy. Keep the Supranational header lines on copied files.
4. Schedule one host build of sppark 0.1.16 under CUDA 13.4 for sm_120 (via gpu-runner-2, coordinated with astra-gpu) to turn the OPEN QUESTION into a MEASURED result.
5. Check the BLS12-381 Montgomery layout against sppark `mont_t` before any reuse of field code.

## Sources

- crates.io API, sppark: https://crates.io/api/v1/crates/sppark (read 2026-10-25)
- sppark GitHub README: https://github.com/supranational/sppark (read 2026-10-25)
- sppark LICENSE: https://raw.githubusercontent.com/supranational/sppark/main/LICENSE (read 2026-10-25, WebFetch summary)
- Local: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sppark-0.1.16/` and `sppark-0.1.14/`
- Local: `/home/charles/repos/loki-proofs/Cargo.toml` (line 209), `build.rs`, `cuda/msm.cu`, `Cargo.lock`
