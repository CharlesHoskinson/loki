# `src/poly/kzg`: KZG commitment scheme

## Purpose

`KZGCommitmentScheme<E: Engine>` implements `PolynomialCommitmentScheme` for BLS12-381. It provides commit, Lagrange commit, the multi-point opening (`multi_open`) and the pairing check behind `Guard`/`DualMSM`. The MSM backend is on [poly-msm](poly-msm.md), the opt-in multiopen on [poly-multiopen](poly-multiopen.md).

## Public API

| Item | Where |
|---|---|
| `KZGCommitmentScheme<E>` | [`mod.rs:55`](../../src/poly/kzg/mod.rs). Trait impl `:59`: `commit` (`:77`), `commit_lagrange` (`:86`), `multi_open` (`:97`). |
| `ParamsKZG<E>` | [`params.rs:19`](../../src/poly/kzg/params.rs): public `unsafe_setup` (`:53`, draws toxic secret; MUST NOT be used in production), `from_parts` (`:102`), `downsize` (`:40`), `g_lagrange`, `write_custom` / `read_custom` (`:136`, `:154`), `verifier_params` (`:273`). |
| `ParamsVerifierKZG<E>` | `params.rs:235`; `read`/`write`, `s_g2`. |
| `construct_intermediate_sets` | [`utils.rs:37`](../../src/poly/kzg/utils.rs): groups queries by commitment and point set. |
| `MSMKZG`, `DualMSM`, `msm_specific` | [`msm.rs`](../../src/poly/kzg/msm.rs) (see [poly-msm](poly-msm.md)). |

## Difference from stock midnight-proofs 0.8.3

| Change | Feature | Where |
|---|---|---|
| `multi_open` calls `multiopen::prepare` instead of the serial inline code. The stock body stays under `cfg(not(feature = "loki-multiopen"))` | `loki-multiopen` | `mod.rs:~110` |
| `kzg_multiopen` phase timer | `loki-profile` | `mod.rs:104` |
| `construct_prover_sets` and `construct_intermediate_sets_with_evals`: grouping that borrows queries and skips evaluations the prover does not need | `loki-multiopen` | `utils.rs` |
| `params.rs` | none | identical to stock |

The stock `verify`, the `DualMSM::check` pairing and the transcript code are unchanged from 0.8.3. The cross-verifier evidence is scoped to the tested supported inputs and configurations in [loki-harness](loki-harness.md); it does not cover every multi-proof instance shape. The mixed committed/plain multi-proof limitation is described in [plonk](plonk.md).

## Invariants and pitfalls

- `msm_specific` zips silently and truncates mismatched lengths. `from_raw_bytes` does no `< r` check. Do not feed unvalidated SRS bytes.
- The challenge order `x1, x2, (f commit), x3, (q evals), x4` must stay as in stock. Quint's `transcript.qnt` pins all four multiopen challenges, and `pi` last.
- `truncated-challenges` truncates `x1` powers and `x3`. Both prover and verifier must agree.
- `unsafe_setup` is a public utility that draws a toxic secret and MUST NOT be used in production. Repository fixtures use synthetic parameters.
- Sets are sorted by `(cardinality, index)` before opening. Any change in set order changes the transcript.

## Tests

`multiopen/tests.rs` (see [poly-multiopen](poly-multiopen.md)), `kzg/mod.rs` `verify` helper at `:420`, and the whole-proof tests listed on [plonk](plonk.md).

## Notes

[`LOKI-MULTIOPEN.md`](../../LOKI-MULTIOPEN.md), [`LOKI-SRS-REGISTRY.md`](../../LOKI-SRS-REGISTRY.md).
