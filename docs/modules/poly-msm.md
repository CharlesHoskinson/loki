# `src/poly/kzg/msm.rs` and `msm/cuda`: multi-scalar multiplication

## Purpose

Every commitment is an MSM over the SRS. `msm_specific` is the single entry. It tries the optional legacy CUDA adapter, then the typed backend dispatcher when enabled, and otherwise uses the stock CPU path. The `msm/cuda` directory is the Rust side of the sppark MSM; the kernels are on [cuda-kernels](cuda-kernels.md). The current backend adapter is described on [loki-gpu](loki-gpu.md).

## Public API

[`msm.rs`](../../src/poly/kzg/msm.rs):

- `msm_specific<C: CurveAffine>(coeffs, bases)` (`:165`). Records `profile::record_msm` (`loki-profile`), tries `cuda::try_msm` (`cuda`), else `msm_cpu`.
- `MSMKZG<E>` (`:32`: `init`, `from_many`, `from_base`, `collapse`) and `DualMSM<E>` (`:351`+: `new`, `split`, `scale`, `add_msm`, `check`) for the verifier-side pairing check.

[`msm/cuda.rs`](../../src/poly/kzg/msm/cuda.rs) (`cuda`):

- `try_msm` (`:23`): reads `LOKI_GPU_MSM_MIN` once (default 16384, an unparsable value gives the default, 0 is allowed), then `dispatch`.
- `dispatch` (`:34`): GPU only if `min(coeffs.len(), bases.len()) >= min` and the curve is exactly `midnight_curves::G1Affine` (checked by `TypeId`, then an unsafe slice cast). On any error it emits one `tracing::warn!("GPU MSM failed; using CPU MSM")` and returns `None`, so the caller falls back to the CPU.

[`msm/cuda/backend.rs`](../../src/poly/kzg/msm/cuda/backend.rs): `msm` (`:392`), `validate` (`:162`), `same_prefix` (`:151`), `Cache::run` (`:265`), FFI to `loki_gpu_prepare` / `loki_gpu_msm` / `loki_gpu_release` (`:48-55`). [`convert.rs`](../../src/poly/kzg/msm/cuda/convert.rs): `affine`, `scalar`, `decode` between `midnight_curves` and the 96-byte / 32-byte wire layout.

## Difference from stock midnight-proofs 0.8.3

| Change | Feature |
|---|---|
| `msm_specific` dispatches to `cuda::try_msm` first | `cuda` |
| `msm_cpu` is the unchanged stock body, split out | none |
| BLST built `no-threads`; MSM is split into chunks of `max(ceil(len/threads), 1024)`, each a serial BLST `multi_exp`, run on the active Rayon pool | `loki-cpu-budget` (opt-in; default keeps stock BLST threading) |
| Exact-prefix, address-keyed, LRU resident SRS registry bounded by `LOKI_GPU_SRS_BUDGET_BYTES` (default 4294967296) | `cuda-srs-registry` (implies `cuda`) |
| Current main routes MSM through `src/backend/dispatch.rs` when `loki-backend-api` is enabled. The optional `loki-gpu-backend` is offered only for advertised operations and exact arithmetic; unsupported requests use the stock CPU adapter. Current `loki-gpu` does not advertise arithmetic yet. | `loki-backend-api`, `loki-gpu-backend` |

Without `cuda-srs-registry` the cache holds at most two SRS arrays with their bucket workspaces, keyed by source pointer plus length, with LRU eviction.

## Invariants and pitfalls

- A cache hit compares retained coordinates with the request, so address reuse or in-place mutation cannot reuse stale bases. This takes an O(n) host scan on every warm call. Upload validates curve membership and subgroup membership before FFI. The `gpu_rejects_corrupted_resident_base` test rejects an off-curve point and an order-three point.
- The registry keeps a validated projective snapshot (144 bytes per host point; device wire stays 96). A hit compares all x/y/z limbs of the requested prefix; unused suffix bytes may differ. The cache does not rely on a digest or pointer generation.
- Dispatch happens before zero filtering to keep SRS identity stable.
- `loki_cuda_runtime::lock()` is taken before the cache lock (see [cuda-runtime](cuda-runtime.md)). If registry release fails, the entry remains and the backend is disabled for the process.
- Repeated scalars are slow on the GPU: 2.4 s versus 25 ms at k18 (`LOKI-GPU-MSM.md`). Kernel speedups barely changed end-to-end time because most time sits in CPU phases.
- Signed-digit Pippenger needs one extra window beyond `ceil(255/c)` or it silently drops points. `msm_specific` also zips silently. The `msm_diff` fuzz target covers both cases.
- Outputs must be fully reduced. `msm_diff` checks every output coordinate is a reduced residue and on the curve.
- `loki-cpu-budget` applies `blst/no-threads` to every user of the same resolved BLST through Cargo feature unification (G2 MSMs and aggregates lose BLST parallelism). Opt in knowingly. At four Rayon workers it was 56-61% slower than stock BLST on a 20-CPU host because it caps the budget; at 20 workers it was 1.4-3.7% slower.

## Tests

- CPU: `msm.rs` `cpu_tests`: `msm_matches_individual_multiplications` (workers 1, 2, 4; lengths 0..4097); `msm_uses_only_the_active_rayon_pool` (Linux, `loki-cpu-budget`).
- GPU, [`msm/cuda/tests.rs`](../../src/poly/kzg/msm/cuda/tests.rs): `gpu_differential`, `gpu_large_boundaries`, `gpu_proof`, `gpu_concurrent_proofs`, `cuda_dispatch_threshold_and_error_fallback`, `cuda_no_device_fallback`, and the `gpu_srs_*` set (`prefix_reuses_upload`, `mutation`, `eviction`, `interleaved`, `rayon_contention`, `budget_rejects_oversize`, `invalid_unused_suffix_fallback`). Most are `#[ignore]` and run under the host GPU lock.
- [fuzz](fuzz.md) `msm_diff`; Quint `gpu_dispatch.qnt` ([quint](quint.md)).

## Notes

[`LOKI-GPU-MSM.md`](../../LOKI-GPU-MSM.md), [`LOKI-SRS-REGISTRY.md`](../../LOKI-SRS-REGISTRY.md), [`LOKI-CPU.md`](../../LOKI-CPU.md) ("One CPU budget"), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md).
