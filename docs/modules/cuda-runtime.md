# `cuda-runtime/`: the shared device lock (`loki-cuda-runtime`)

## Purpose

This crate exposes one process-wide mutex that serializes sppark calls. MSM
and NTT share sppark streams and allocate device memory, so the calls must not
run concurrently. The shared dependency also covers direct `loki-cuda-ntt`
callers.

## Public API

[`cuda-runtime/src/lib.rs`](../../cuda-runtime/src/lib.rs): `pub fn lock() -> Result<MutexGuard<'static, ()>, String>`. A poisoned mutex returns `"CUDA device lock poisoned"`. The crate has no dependencies, `publish = false`, version 0.1.0.

## Difference from stock midnight-proofs 0.8.3

New crate. Optional dependency of the root crate under `cuda`; also pulled by `loki-cuda-ntt`.

## Invariants and pitfalls

- Lock order: take the device lock before any adapter-local cache lock. `msm/cuda/backend.rs:398-402` follows this order. Resident allocation, invocation, and release all hold the device lock.
- Hold the lock across every sppark call, including allocation and destruction.
- Never wait on Rayon while holding it. The SRS registry validates uploads on four native scoped threads for that reason.
- Concurrent provers are safe but queue on the device. The W9 test with four concurrent provers requires exactly four times the successful dispatches of one proof.
- The lock cannot see GPU memory. Bounded residency is enforced by the MSM cache ([poly-msm](poly-msm.md)).

## Tests

No unit tests in the crate. Covered by `gpu_concurrent_proofs` and `registry_workers_join_after_errors_and_panics`, and abstracted by `quint/gpu_dispatch.qnt` ([quint](quint.md)).

## Notes

[`LOKI-HARDEN.md`](../../LOKI-HARDEN.md) ("GPU behavior").
