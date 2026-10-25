# `cuda-ntt/`: the GPU NTT adapter (`loki-cuda-ntt`)

## Purpose

This crate is a serialized adapter to sppark's BLS12-381 scalar-field NTT.
Its unsafe FFI keeps `unsafe` out of the proofs crate's NTT path. The root
feature `cuda-ntt` enables it independently of `cuda`.

## Public API

[`cuda-ntt/src/lib.rs`](../../cuda-ntt/src/lib.rs):
- `fft(values: &mut [Fq], root: Fq, inverse: bool) -> Result<(), String>` (`:29`). Natural-order standard NTT (not a coset transform) on reduced Montgomery elements. Inverse includes the `1/n` factor. Requires a power-of-two length up to `2^28`. Errors leave the caller's slice unchanged.
- `completed_calls() -> u64` (`:20`): count of successful GPU transforms. Tests and fuzz use it to prove the GPU ran.
- `LOKI_NTT_FORCE_ERROR` (environment): injects an error for fallback tests.

[`cuda-ntt/cuda/ntt.cu`](../../cuda-ntt/cuda/ntt.cu) exports
`loki_ntt(fr_t*, log_n, inverse, root)`. Before launching, it compares the
provided root byte-for-byte with sppark's `forward_roots_of_unity[log_n]` or
`inverse_roots_of_unity[log_n]`. A mismatch returns an error. It runs
`NTT::Base` with `NN` order on device 0. [`build.rs`](../../cuda-ntt/build.rs)
finds the toolkit using `CUDA_HOME`, `NVCC`, or `PATH`, then adds the runtime
library path. It passes `LOKI_NTT_CUDA_ARCH` through as `nvcc -arch=<target>`;
use an NVCC target spelling such as `sm_120` or `compute_80`.

Dispatch is in the root crate: `poly/domain.rs` `gpu_fft` (see [poly-domain](poly-domain.md)): 2^10 up to 2^28 elements, `Fq` only, skipped with `LOKI_NTT_CPU`.

## Difference from stock midnight-proofs 0.8.3

New crate. Transforms below 2^10 stay on the CPU. The adapter converts through the public `Fq` / `blst_fr` conversions rather than a layout cast. Each transform is copied to a temporary limb buffer and back, so values do not remain on the device. Resident NTT work is in progress in loki-gpu P4 (see [loki-gpu](loki-gpu.md)).

## Invariants and pitfalls

- Pin the root and the coset generator: round trips do not detect a wrong root. Apply `1/n` exactly once (sppark's inverse already scales).
- Hold the device lock (`loki_cuda_runtime::lock()`) for the whole call.
- Failure falls back to CPU with one `tracing::warn!` per process.
- Measured on the reference RTX 5090 (synthetic k17, `LOKI-GPU-NTT.md`): numerator 349.8 to 176.3 ms (1.98x), vanishing division and coset recovery 28.1 to 10.6 ms, whole `create_proof` 2172.8 to 1950.6 ms (1.11x). Per-call: `coeff_to_lagrange` k20 122.5 to 46.6 ms. Three samples each, shared host: indicative only.

## Tests

`domain/gpu_tests.rs` (equality with CPU and naive oracles, wrong-root rejection, forced-error fallback), `tests/ntt-stock` (GPU proof checked by the stock verifier), fuzz `ntt_diff` ([fuzz](fuzz.md)). Raw data: `LOKI-GPU-NTT-EQUALITY.csv`, `LOKI-GPU-NTT-BENCH.csv`.

## Notes

[`LOKI-GPU-NTT.md`](../../LOKI-GPU-NTT.md), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md).
