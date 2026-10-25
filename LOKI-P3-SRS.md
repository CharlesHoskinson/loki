# P3 — legacy sppark adapter and new-backend SRS codec (Loki side)

Branch `loki/p3-srs` on `1c8615f` (P4 merge). The loki-gpu half (registry, runtime seam, contract
tests) is recorded in `loki-gpu/design/p3/REVIEW.md`. Labels: CODE / MEASURED / OPEN
QUESTION. All inputs are synthetic.

## Features

| Feature | Meaning |
|---|---|
| `loki-sppark` | the W13/legacy sppark MSM, now reached through the P1 dispatcher by the typed `backend::sppark::SpparkBackend` adapter |
| `loki-gpu-srs-registry` | implies `loki-gpu-backend`; builds `backend::gpu_srs` (BLS12-381 G1 codec and process registry for `loki_gpu::srs::GpuSrsRegistry`) |

The two never coexist: `src/lib.rs` rejects `loki-gpu-backend` together with `cuda` /
`cuda-ntt`, so all mixed combinations fail to compile (exit 101).

## What changed (CODE)

- `src/poly/kzg/msm/srs_validate.rs`: W13's validation (`validate`, `same_prefix`,
  `same_coordinates`, `parallel_chunks`) moved out of the legacy backend, unchanged in
  behaviour and messages, so the legacy registry and the new codec share one check.
- `src/backend/sppark.rs`: the adapter. Admission reproduces the legacy threshold
  (`LOKI_GPU_MSM_MIN`, default 2^14) and BLS12-381-only type check; admitted calls run the same
  `backend::msm` on the same untruncated bases, so arithmetic is unchanged. A device point is
  published only when it is on the curve and in the G1 subgroup. Legacy failures are
  classified: device faults poison (`SessionPoisoned`); invalid SRS, budget refusal and host
  worker failure fall back once without disabling (`InputsPreserved`).
- `src/poly/kzg/msm/cuda/backend.rs`: only device faults set the sticky disable flag;
  eviction and cache clearing remove an entry only after its release succeeded; a handle
  returned together with a preparation error is retained, never destroyed on the faulted
  context.
- `src/backend/dispatch.rs` (MSM arm): `msm_specific` selects the adapter under
  `loki-sppark`; a result that returns after a concurrent poison is discarded and recomputed
  on the CPU.
- `src/backend/gpu_srs.rs`: `MidnightG1Codec` — 96 bytes per point, `x[6] ‖ y[6]` LE `u64`
  BLST Montgomery limbs (R = 2^384), identity `(0, 0)`; equal to the legacy
  `convert::affine` encoding and to an independent BigUint computation in tests.
- `src/poly/domain/gpu_tests.rs`: missing `best_fft` import (pre-existing break under
  `loki-sppark`).

## Gates (MEASURED)

CPU (final source commit `05a3c7a`; MERGE-CPU on the rebased branch): `loki-sppark` lib 49 passed / 41 GPU-ignored;
`loki-gpu-srs-registry` lib 38 / 3 ignored; default lib 34; `backend_cpu_equivalence` 15
(`--features loki-backend-api`); W2 harness k10/k14/k17 cross-verified with equal VKs; the
independent stock verifier (`tests-stock`) accepts the k14 CPU fixture. Each new rule (output
check, failure classification, post-return recheck, Cancelled return) has a test that fails on
its mutant.

GPU, measured before the final fixes: the adapter equals the legacy call and the CPU MSM
(prefixes, unequal lengths, same-address mutation); `gpu_proof`, `gpu_differential` (10,108
cases) and `gpu_large_boundaries` pass under `loki-sppark`; the `tests/gpu-compat` stock
verifier accepts the GPU proof and rejects a mutated one.

RECEIPT-NATIVE (post-merge, per `docs/MERGE-GATES.md`): the GPU rows on the merged sha, the
W13 `gpu_srs_*` set including `gpu_srs_kernel_prefix_differential`, and
`gpu_srs_registry_k19_k20_on_device`.

## Open questions

- `msm_specific` keeps the stock value signature and is non-cancellable: an accelerator
  `Cancelled` unwinds there. Cancellable callers must use `Dispatcher::try_msm` (quint-q2 B1).
- The legacy W13 `parse_budget` still treats an unparsable budget as 4 GiB (its W13 test pins
  this); the new registry rejects it.
- The NTT legacy arm (`domain.rs`) and the dispatcher transform arm belong to P4 (quint-q2 L3,
  B3 for transforms).
