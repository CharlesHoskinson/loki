# `src/backend`: typed backend API and dispatch

This optional module is compiled with `loki-backend-api`. It connects Loki's MSM and transform call sites to typed backend contracts, with a stock CPU implementation as the fallback. Source references below point to the current checkout at `a256a9a15ff18ae12a8131b86cff576787fa5e3c`.

The seam keeps transcript writes, randomness, and request ordering at the prover call sites. Backend implementations receive arithmetic requests and capabilities, not the transcript or RNG. The full P1 audit, measurements, and historical gates are in [`LOKI-P1-SEAM.md`](../../LOKI-P1-SEAM.md).

## Features

| Feature | Effect |
|---|---|
| `loki-backend-api` | Enables `src/backend` and the typed `loki-backend-api` contracts. |
| `loki-sppark` | Enables the typed API with the legacy sppark CUDA MSM and NTT paths. |
| `loki-gpu-backend` | Enables the separate `loki-gpu` backend path. It cannot be combined with `cuda` or `cuda-ntt`. |

`loki-gpu-backend` remains an integration path under development. The P2 runtime is present in loki-gpu main `d71b7ca`, while the backend still reports arithmetic operations as unsupported and P4 is in progress. The feature can build and dispatch through the API, but it does not establish a GPU arithmetic pass.

## Module contents

| Item | Source | Role |
|---|---|---|
| `api` | [`mod.rs:26`](../../src/backend/mod.rs#L26) | Re-exports `loki_backend_api` contracts for MSM, transforms, resident operations, quotient work, SRS access, errors, and capabilities. |
| `field_abi`, `arithmetic_abi` | [`mod.rs:35`, `:45`](../../src/backend/mod.rs) | Match concrete Rust types by `TypeId`. The native ABI covers the BLS12-381 scalar field `Fq` and G1 affine points, using wire encoding version 1. Other types use the CPU path. |
| `cpu::CpuBackend` | [`cpu.rs:30`](../../src/backend/cpu.rs) | Stock CPU reference for MSM and transforms. Resident, quotient, and SRS operations return `Unsupported`. |
| `dispatch::Dispatcher` | [`dispatch.rs:52`](../../src/backend/dispatch.rs) | Validates requests, checks advertised capabilities and exact ABI support, then selects an accelerator or computes on CPU. |
| `dispatch::DispatchCounters`, `dispatch::global` | [`dispatch.rs:31`, `:231`](../../src/backend/dispatch.rs) | Exposes per-operation CPU, accelerated, fallback, and poisoned-accelerator counts. Production call sites use the process-wide dispatcher; tests can construct an independent `Dispatcher`. |

## Dispatch behavior

For MSM, the request carries `ZipTruncate`, matching the stock call site's `min(scalars.len(), bases.len())` rule. An empty prefix returns the identity, and zero scalars are filtered by the CPU adapter. The dispatcher offers work to an accelerator only when it advertises host MSM and the exact arithmetic ABI.

For transforms, the CPU adapter validates the domain sizes, root order, element order, and coset parameters before changing values. It then runs the stock transform sequence: coset scaling and zero padding before a forward FFT; or an inverse FFT, one `1/n` normalization, truncation, and inverse coset scaling. Current prover call sites use natural element order.

The dispatcher retains host inputs. If an accelerator returns an error without changing observable inputs, Loki can recompute on CPU. An error that poisons the session, a lost or poisoned context, or a transform result with the wrong length disables that accelerator for the rest of the process. Transform outputs replace inputs only after they are complete and have the expected length.

## Call sites and limits

- [`msm_specific`](../../src/poly/kzg/msm.rs#L165) keeps the legacy `cuda` arm first. With `loki-backend-api`, remaining MSM calls go through the dispatcher. Without it, calls continue to the stock CPU routine after any enabled legacy CUDA attempt.
- [`fft` and `ifft`](../../src/poly/domain.rs#L412) use the dispatcher when `loki-backend-api` is enabled. The legacy `cuda-ntt` arm remains first. Whole coset conversions are offered to an accelerator and otherwise run the stock body.
- [`lookup/prover.rs`](../../src/plonk/lookup/prover.rs#L425) uses stock `HashMap` order for lookup leftovers by default. `loki-deterministic-lookup-order` opts into field-order sorting, which changes lookup proof bytes relative to stock.

The backend API defines resident buffers, quotient interpretation, and SRS registry contracts, but these operations are not wired to supported GPU arithmetic in the current status above. The P1 work leaves the CPU quotient evaluator unchanged.

## Tests and references

P1's CPU seam, default behavior, feature exclusivity, and stock-verifier checks are described in [`LOKI-P1-SEAM.md`](../../LOKI-P1-SEAM.md). The gates there are historical results from the listed revisions. No GPU arithmetic pass is claimed for this seam.
