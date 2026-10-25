# `cuda/` and `build.rs`: the MSM kernel adapter

## Purpose

[`cuda/msm.cu`](../../cuda/msm.cu) is the only CUDA source in the root
crate. It is 94 lines long, instantiates sppark's BLS12-381 Pippenger MSM
templates, and exposes three C functions. The kernels come from sppark 0.1.16
(Apache-2.0), pinned as `=0.1.16` in `Cargo.toml`. Loki builds this file only
with the `cuda` feature and does not implement the kernel arithmetic.

## Public API (C ABI, called from `msm/cuda/backend.rs`)

| Function | Role |
|---|---|
| `loki_gpu_prepare(void** out, const wire_affine* bases, size_t n)` (`msm.cu:26`) | Copies the bases to persistent VRAM and allocates buckets (`resident_t = msm_t<...>`). Synchronizes before Rust drops the host wire array. |
| `loki_gpu_msm(void* resident, wire_affine* out, const wire_scalar* scalars, size_t n)` (`:47`) | Runs one MSM against the resident bases. Writes the affine result; the point at infinity is written as all zeros. |
| `loki_gpu_release(void* resident)` (`:76`) | Synchronizes (surfacing a poisoned context), then frees. |

Wire layout: `wire_affine` is 12 x u64 (x then y, 6 limbs each), `wire_scalar` is 4 x u64. `static_assert`s check size and alignment against sppark's `affine_t` and `scalar_t`. All functions are `noexcept` and convert every C++ exception to a `RustError`.

## Build (`build.rs`)

[`build.rs`](../../build.rs) compiles `cuda/msm.cu` with `cc` and `nvcc`
when `cuda` is enabled.

- The default target is `120`, which emits `sm_120` and `compute_120`. Set
  `LOKI_CUDA_ARCH` to digits only, such as `80`, to select another target.
- The script finds the toolkit through `CUDA_HOME`, `CUDA_PATH`,
  `CUDA_TOOLKIT_PATH`, `NVCC`, or `PATH`, in that order. Cargo reruns it when
  these variables change.
- It copies sppark's `msm_t` header and changes `~msm_t()` to
  `~msm_t() noexcept(false)`. An assertion requires exactly one match. This
  lets errors during free or sync reach the `catch` in `loki_gpu_release`
  instead of terminating the process.
- It defines `__ADX__` when the x86_64 host build supports ADX.
- The build uses `compile_error!` on hosts that are not little-endian
  (`msm/cuda.rs:60`).

## Difference from stock midnight-proofs 0.8.3

Entire directory is new. Stock has no GPU code.

## Invariants and pitfalls

- Targets the RTX 5090 (sm_120) with CUDA 13.4.3 on the reference host. Other architectures need `LOKI_CUDA_ARCH` and have not been measured.
- The ABI uses reduced Montgomery residues. Output must be fully reduced (limb-wise equality; see [plonk](plonk.md)).
- CUDA's active device is thread-local, so every FFI entry calls `select_gpu(0)`. Calls must hold the device lock ([cuda-runtime](cuda-runtime.md)).
- Only device 0 is used. No multi-GPU support exists.
- A repeated-scalar workload is slow on this kernel (2.4 s vs 25 ms at k18).

## Tests

Via [poly-msm](poly-msm.md) GPU tests and the `msm_diff` fuzz target. `cuda_no_device_fallback` checks the CPU path on a host without a device.

## Notes

[`LOKI-GPU-MSM.md`](../../LOKI-GPU-MSM.md). Intel, AMD, Apple, WASM and mobile backends are not implemented here; see [loki-gpu](loki-gpu.md).
