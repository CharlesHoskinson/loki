# `src/poly/kzg/multiopen.rs`: parallel KZG multiopen (`loki-multiopen`)

## Purpose

Replaces the prover's serial grouping and division inside `KZGCommitmentScheme::multi_open` with clone-free, ordered, parallel polynomial work. Off by default.

## Public API

All items are `pub(super)` in [`multiopen.rs`](../../src/poly/kzg/multiopen.rs):

- `prepare(queries, x1, x2, k)` (`:102`) returns `(q_polys, f_poly)`. It groups borrowed polynomials by point set, combines each group with powers of `x1` (truncated powers under `truncated-challenges`), sorts sets by `(cardinality, index)`, divides each by its root product, pads to `2^k`, then combines with powers of `x2`.
- `combine(polys, scalars)` (`:57`): one output allocation, disjoint coefficient chunks (at least 1024 coefficients each), polynomial order preserved, output length equals the first polynomial's length (so `f` may be padded longer than the `q` polynomials).
- `divide_by_roots(poly, roots)` (`:18`): quotient by the monic product of `X - root`, remainder dropped, exactly `poly.len() - roots.len()` coefficients kept including zeros. Ported from midnight-zk #565.

`combine` parallelizes coefficient chunks when the first polynomial has at least 4096 coefficients (`PARALLEL_THRESHOLD`), including a single-set call. `prepare` parallelizes group combination and root-product division when there is more than one set and total input work is at least 4096 coefficients.

## Difference from stock midnight-proofs 0.8.3

Entire module is feature-gated by `loki-multiopen` (declared in `poly/kzg/mod.rs:21`). Transcript writes, evaluations, commitments, the final `pi` division and the verifier are unchanged. The final `pi` synthetic division stays serial. Run 2 (current) replaces the chain of `kate_division` calls with root-product division and skips evaluations stored only for grouping.

Measured (`LOKI-MULTIOPEN.md`, synthetic CPU workload, not Sherlock): multiopen phase 10-20% faster, 0.7-3.9% end to end. Run 2 at k17 with 20 workers: 451.0 ms serial to 365.3 ms multiopen phase.

## Invariants and pitfalls

- Set and polynomial order feed the transcript. Indexed Rayon collection preserves that order. The recorded k16 and k17 runs matched Loki's serial and feature-enabled proof bytes for all eight pairs at 4 and 20 Rayon threads (SHA-256 hashes are in [`LOKI-MULTIOPEN.md`](../../LOKI-MULTIOPEN.md)). This byte-equality evidence covers those runs only; it does not cover registry stock, other thread counts, k20, recursive-parent circuits or GPU runs.
- Root-product division requires distinct roots in a set; duplicate queries are rejected earlier (`Error::DuplicatedQuery`).
- Zero scalars and `scalar == ONE` are special-cased in `combine`. The result must still equal the generic product.
- The Lean contract is `LokiLean/Multiopen.lean` in `~/repos/loki-lean`. The Rust code cites it, but that citation does not establish that the implementation follows the contract. The proofs remain a separate obligation and are not complete.
- Output with the feature enabled must be verified by the stock verifier (`W2` gate, `W11-validation`).

## Tests

[`multiopen/tests.rs`](../../src/poly/kzg/multiopen/tests.rs): `product_division_matches_successive_kate_division`, `prover_grouping_skips_only_unused_evaluations`, `differential_random_sets_and_transcripts`, `differential_coefficient_chunks`, `padded_rhs_keeps_first_polynomial_length`, `short_polynomial_with_larger_srs_preserves_proof_bytes`. Plus the harness with `--features loki-multiopen` ([loki-harness](loki-harness.md)).

## Notes

[`LOKI-MULTIOPEN.md`](../../LOKI-MULTIOPEN.md).
