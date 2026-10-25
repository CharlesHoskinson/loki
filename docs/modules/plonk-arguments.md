# `src/plonk` arguments: lookup, permutation, vanishing, trash

Each argument is split into `prover.rs`, `verifier.rs` and (for permutation) `keygen.rs`. The prover half commits and evaluates, the verifier half reads commitments, evaluates the identity at `x` and emits queries for the multiopen. See [plonk](plonk.md) for the driver.

## Lookup

[`lookup.rs`](../../src/plonk/lookup.rs): `Argument::new` (`:30`), `input_expressions`, `table_expressions`.
[`lookup/prover.rs`](../../src/plonk/lookup/prover.rs): `commit_permuted` (`:58`), `commit_product` (`:155`), `evaluate` (`:293`), `open` (`:328`), private `permute_expression_pair` (`:374`).
[`lookup/verifier.rs`](../../src/plonk/lookup/verifier.rs): `read_permuted_commitments` (`:31`), `read_product_commitment` (`:52`), `evaluate` (`:69`), `queries` (`:96`).

### Lookup changes
- Input permutation sort is `par_sort` (parallel, stable) at `prover.rs:391`.
- Table counts use a `HashMap`. By default, leftover entries fill unused rows in map iteration order, matching stock. In current source revision `a256a9a15ff18ae12a8131b86cff576787fa5e3c`, `loki-deterministic-lookup-order` sorts leftover entries by value before filling those rows (`src/plonk/lookup/prover.rs`). This feature makes the lookup permutation deterministic.
- The product column is built with `batch_invert_parallel` plus `prefix_product` into a pre-sized buffer.

Table entries and blinding rows interact: the product column has `n - (blinding_factors + 1)` usable rows and the tail is random. By default, Loki iterates leftover rows in `HashMap` order, matching stock. The opt-in `loki-deterministic-lookup-order` feature sorts the input and leftover rows; with a fixed RNG seed and Rayon pool size this makes Loki lookup proofs reproducible. The measured lookup-preparation result and proof-byte behavior in [`LOKI-CPU.md`](../../LOKI-CPU.md) come from source revision `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16`, where leftover sorting was always on. Tests include `time_lookup_preparation_k16` (ignored, benchmark), lookup circuits in `tests/cpu.rs`, the harness and `fuzz`.

## Permutation

[`permutation.rs`](../../src/plonk/permutation.rs): `Argument` (`:33`), `VerifyingKey` (`:95`), `Assembly` (re-exported from `keygen.rs`).
[`permutation/keygen.rs`](../../src/plonk/permutation/keygen.rs): `Assembly` (`:31`, `copy` at `:51`), `build_vk` (`:194`), `build_pk` (`:138`), `compute_polys_and_cosets` (`:254`).
[`permutation/prover.rs`](../../src/plonk/permutation/prover.rs): `commit` (`:35`), `evaluate` (`:181`), `open` (`:231`).
[`permutation/verifier.rs`](../../src/plonk/permutation/verifier.rs): `read_product_commitments` (`:27`), `evaluate` (`:54`, `:72`), `queries` (`:105`, `:146`).

Loki computes the grand-product column `Z` with a parallel prefix product (`utils::arithmetic::prefix_product`) over `usable_rows` entries. It then pushes the final value and appends `blinding_factors` random rows. Stock builds `Z` row by row and overwrites the tail with random values. Both produce the same polynomial for a given RNG stream.

The RNG draw count and order for the blinding rows must match stock for fixed-RNG comparisons. See `permutation/prover.rs:142-145`.

## Vanishing

[`vanishing.rs`](../../src/plonk/vanishing.rs) re-exports.
[`vanishing/prover.rs`](../../src/plonk/vanishing/prover.rs): `commit` (`:37`, random polynomial), `construct` (`:86`, quotient split and commit), `blind_quotient_limbs` (`:148`), `evaluate` (`:165`), `open` (`:193`).
[`vanishing/verifier.rs`](../../src/plonk/vanishing/verifier.rs): `read_commitments_before_y` (`:36`), `read_commitments_after_y` (`:51`), `evaluate_after_x` (`:70`), `verify` (`:88`), `queries` (`:107`).

Loki changes only the instrumentation here. `loki-profile` times `quotient_commitment`; `cuda-ntt` adds the `loki_ntt_quotient_recovery` span around the extended-to-coefficient transform (`:98-106`).

The random polynomial is filled in chunks seeded per Rayon worker (`:49-71`), so proof bytes depend on `current_num_threads()`. This behavior is inherited from stock.

## Trash

[`trash.rs`](../../src/plonk/trash.rs): `Argument::new` (`:19`), `selector`, `constraint_expressions`. Prover `commit`, `evaluate`, `open` ([`trash/prover.rs`](../../src/plonk/trash/prover.rs)); verifier `read_committed`, `evaluate`, `queries` ([`trash/verifier.rs`](../../src/plonk/trash/verifier.rs)). Identical to stock. It carries a selector expression and a list of constraint expressions. The Quint fixtures call this family "additive-selector trash". The `trash` phase appears in the profile report.

## Tests

Exercised through full proofs: `tests/plonk_api.rs`, `tests/cpu.rs`, [loki-harness](loki-harness.md), [fuzz](fuzz.md) `proof_roundtrip` (random circuits with lookup, second phase, copy constraints) and the Quint transcript fixtures (four circuit families, including trash) in [quint](quint.md).

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md), [`LOKI-FUZZ.md`](../../LOKI-FUZZ.md), [`LOKI-QUINT.md`](../../LOKI-QUINT.md).
