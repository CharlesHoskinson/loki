# `src/utils`: arithmetic, serialization, rational numbers

## Purpose

Field and curve helpers shared across the prover. This is where the CPU-parallel primitives added by Loki live.

## Public API

| Item | Where |
|---|---|
| `eval_polynomial`, `compute_inner_product`, `kate_division`, `lagrange_interpolate`, `parallelize`, `g_to_lagrange` | [`arithmetic.rs:55`, `:83`, `:97`, `:226`, `:120`, `:34`](../../src/utils/arithmetic.rs) |
| `FftGroup`, `MSM` traits; re-exports of `Field`, `CurveAffine`, `CurveExt` | `arithmetic.rs:21`, `:374`, `:9-16` |
| `batch_invert_parallel` (`pub(crate)`, Loki) | `arithmetic.rs:172`: batch inversion split across `parallelize` chunks for 1024 or more elements, serial `batch_invert` below. Zeros stay zero. |
| `prefix_product` (`pub(crate)`, Loki) | `arithmetic.rs:184`: exclusive prefix product, serial for one chunk, otherwise chunk totals combined serially then applied in parallel. Returns the final total. |
| `powers`, `truncated_powers`, `truncate`, `inner_product`, `msm_inner_product`, `evals_inner_product` | `arithmetic.rs:313`, `:309`, `:301`, `:317`, `:329`, `:360` (all `pub(crate)`) |
| `SerdeFormat`, `ProcessedSerdeObject`, `byte_length`, `pack`, `unpack` | [`helpers.rs:16`, `:31`, `:47`, `:115`, `:125`](../../src/utils/helpers.rs) |
| `Rational<F>` (`Zero`, `Trivial`, `Rational`) | [`rational.rs:11`](../../src/utils/rational.rs): `numerator`, `denominator`, `evaluate`, `invert`, arithmetic ops |

## Difference from stock midnight-proofs 0.8.3

Only `arithmetic.rs` differs in the compiled module tree:
- `batch_invert_parallel` and `prefix_product` are new and always on.
- `g_to_lagrange` records an FFT count under `loki-profile` (`:34` region).
- Two unit tests for the new functions.

`helpers.rs` and `rational.rs` are identical. `multicore.rs` is a retained source file, but `utils/mod.rs` does not declare it, so it is not compiled or importable as `utils::multicore`.

## Invariants and pitfalls

- Batch inversion must skip zeros, as `ff`'s `batch_invert` does. A zero in a parallel chunk must not poison the chunk. Test `parallel_inversion_preserves_zero_denominators` covers 1, 3, 8 workers and boundary lengths 0, 1, 1023, 1024, 1025, 65531, including zero entries.
- `prefix_product` must equal the serial scan including when a zero appears at the start, a chunk boundary or the end. Test `parallel_prefix_product_matches_serial_scan`.
- `parallelize` partitions by `current_num_threads()`. Anything that derives randomness from the partition (the vanishing random polynomial) makes proof bytes depend on the pool size.
- `SerdeFormat::Processed` and raw formats are not interchangeable for keys and SRS. The harness round-trips full SRS bytes in both directions.
- `from_raw_bytes` does no `< r` check (see [plonk](plonk.md)).

## Tests

`arithmetic.rs`: `test_lagrange_interpolate`, `parallel_inversion_preserves_zero_denominators`, `parallel_prefix_product_matches_serial_scan`. `rational.rs` has 6 unit tests. `benches/commit_zk.rs` benchmarks `parallelize`.

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md).
