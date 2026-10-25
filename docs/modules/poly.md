# `src/poly`: polynomials, queries, commitment trait

## Purpose

Polynomial types in three bases, the query types the multiopen consumes, and the `PolynomialCommitmentScheme` abstraction. The FFT domain is on [poly-domain](poly-domain.md), KZG on [poly-kzg](poly-kzg.md).

## Public API

| Item | Where |
|---|---|
| `Polynomial<F, B>` with bases `Coeff`, `LagrangeCoeff`, `ExtendedLagrangeCoeff` (trait `PolynomialRepresentation`) | [`mod.rs:173`, `:50`, `:87`, `:115`, `:144`](../../src/poly/mod.rs) |
| `Rotation(i32)` with `cur`, `prev`, `next` | `mod.rs:541` |
| `Polynomial::rotate` | `mod.rs:486` |
| `batch_invert_rational` (`pub(crate)`) | `mod.rs:290` |
| `ProverQuery`, `VerifierQuery`, `CommitmentLabel`, `Query` trait | [`query.rs:52`, `:185`, `:12`, `:40`](../../src/poly/query.rs) |
| `PolynomialCommitmentScheme<F>`, `Guard`, `Params` traits | [`commitment.rs:16`, `:75`, `:95`](../../src/poly/commitment.rs) |

## Difference from stock midnight-proofs 0.8.3

`mod.rs` differs in `batch_invert_rational`. Always on, no feature. Stock collected every denominator, batch-inverted them serially and multiplied by `unwrap_or(ONE)`. Loki keeps only denominators that need inversion (`Rational::Zero`, `Trivial` and denominators equal to one are skipped), runs `batch_invert_parallel` over the compacted vector and rebuilds each column. `Polynomial::invert` was removed with it. Measured effect: 1.83x at 10% rational cells, 1.38x at 100% (`LOKI-CPU.md`, "Rational conversion"). `query.rs` and `commitment.rs` are identical to stock.

## Invariants and pitfalls

- A zero denominator evaluates to zero, as in `ff`'s `batch_invert`. The unit test `rational_batch_matches_individual_evaluation` pins this at thread counts 1 and 4 and lengths up to 65531.
- `Polynomial` equality is limb-wise (see the `Fq` pitfall on [plonk](plonk.md)).

## Tests

`poly/mod.rs` `cpu_tests`: `rational_batch_matches_individual_evaluation`, plus `time_rational_conversion_k16` (ignored benchmark). `domain.rs` `test_rotate`, `test_l_i`.

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md).
