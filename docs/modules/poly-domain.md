# `src/poly/domain.rs`: evaluation domain and FFT dispatch

## Purpose

`EvaluationDomain<F>` holds `omega`, the extended domain and the coset generator, and converts between coefficient, Lagrange and extended-Lagrange form. Every conversion funnels into one internal `fft`/`ifft`. That is the single point where the GPU NTT can take over.

## Public API

[`domain.rs`](../../src/poly/domain.rs):

- `EvaluationDomain::new(j, k)` (`:62`) and the checked `try_new(j, k)` (`:39`, `loki-harden` only).
- Constructors: `empty_coeff`, `empty_lagrange`, `empty_extended`, `constant_lagrange`, `constant_extended`, `lagrange_from_vec` (`:172`), `coeff_from_vec` (`:184`).
- Conversions: `lagrange_to_coeff` (`:249`), `coeff_to_lagrange` (`:263`), `coeff_to_extended` (`:276`), `extended_to_coeff` (`:297`), `extended_to_lagrange` (`:320`), `divide_by_vanishing_poly` (`:346`).
- Accessors: `k` (`:450`), `extended_k` (`:455`), `extended_len` (`:460`), `get_omega` (`:465`), `get_omega_inv` (`:471`).

## Difference from stock midnight-proofs 0.8.3

| Change | Feature |
|---|---|
| Stock called `best_fft` directly. Loki calls `Self::fft`/`Self::ifft`, which first record `profile::record_fft` | `loki-profile` |
| `gpu_fft` tries the CUDA NTT first: `Vec<F>` is downcast to `Vec<Fq>` through `Any` (other fields stay on CPU), requires `1 << 10 <= len <= 1 << 28`, and is skipped when `LOKI_NTT_CPU` is set. On error it logs one `tracing::warn!` and returns to `best_fft` | `cuda-ntt` |
| Current main can route host FFT/iFFT and whole coset conversions through the typed backend dispatcher. The optional GPU is used only when it advertises the operation and arithmetic; otherwise the stock CPU adapter runs. Current `loki-gpu` does not advertise arithmetic yet. | `loki-backend-api`, `loki-gpu-backend` |
| `try_new` rejects `j` over `u32`, zero degree, `2^k` and the extended size overflowing `u64`, extended size over the field's 2-adicity `F::S` or over `isize::MAX / size_of::<F>()` | `loki-harden` |

The ZETA coset distribution, zero padding, truncation and rotation stride are unchanged. The GPU does plain (not coset) NTTs in natural order.

## Invariants and pitfalls

- Pin the root and coset generator. A wrong root of unity or coset generator can pass round-trip tests. `ntt_diff` (fuzz) compares against a naive DFT for extended sizes up to 256. The CUDA wrapper also compares the passed root with sppark's root table and returns an error on mismatch (see [cuda-ntt](cuda-ntt.md)).
- Apply 1/n exactly once. sppark's inverse transform already scales. The GPU branch returns early only after success, so the CPU divisor is not applied twice. If the GPU fails, the input remains intact because the adapter uses a temporary limb buffer; the CPU path then applies its own divisor.
- Outputs must be fully reduced Montgomery residues, since `Fq` equality is limb-wise.
- `try_new` is a compile-time-gated safety net. Without `loki-harden`, `new` keeps stock panics.

## Tests

- `domain/gpu_tests.rs` (`cuda-ntt`): `gpu_ntt_dispatch`, `equality`, `equality_cases`, `gpu_ntt_reject_wrong_root`, `gpu_ntt_forced_error_falls_back`, and the ignored `gpu_ntt_benchmark`.
- `domain.rs`: `test_rotate`, `test_l_i`, `checked_domain_rejects_invalid_parameters` (`loki-harden`).
- [fuzz](fuzz.md) `ntt_diff` and `tests/ntt-stock` ([tests](tests.md)).

## Notes

[`LOKI-GPU-NTT.md`](../../LOKI-GPU-NTT.md), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md); data in `LOKI-GPU-NTT-BENCH.csv` and `LOKI-GPU-NTT-EQUALITY.csv`.
