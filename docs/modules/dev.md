# `src/dev`: MockProver, cost model, circuit inspection

## Purpose

Developer tooling that does not produce proofs. `MockProver` checks a witness against all constraint families without keys or an SRS. The cost model estimates proof size and verifier cost. The rest renders gates, layouts and failures.

## Public API

| Item | Where | Role |
|---|---|---|
| `MockProver<F>` | [`mod.rs:280`](../../src/dev/mod.rs) | `run` (`:812`) synthesizes a circuit into in-memory columns. `verify` (`:924`), `verify_at_rows` (`:932`), `assert_satisfied` (`:1378`) check gates, lookups, permutations and trash arguments. Accessors: `advice`, `fixed`, `instance`, `selectors`, `usable_rows`, `cs`. |
| `RowSizer`, `min_k` | `mod.rs:628`, `:655` | Smallest `k` that fits a circuit. |
| `CellValue`, `InstanceValue` | `mod.rs:92`, `:312` | Cell state in the mock. |
| `VerifyFailure`, `FailureLocation` | [`failure.rs:127`, `:24`](../../src/dev/failure.rs), `failure/emitter.rs` | Structured constraint failures with region and row context. |
| `cost_model::CircuitModel`, `circuit_model` | [`cost_model.rs:131`, `:255`](../../src/dev/cost_model.rs) | Estimated proof size and verification cost from a circuit's column and rotation profile. `dummy_synthesize_run` (`:635`) drives a synthesis with dummy values. |
| `CircuitGates` | [`gates.rs:98`](../../src/dev/gates.rs) | Text and CSV dump of gates and queries (`queries_to_csv`, `:253`). |
| `graph.rs` (`circuit_dot_graph`) | [`graph.rs:21`](../../src/dev/graph.rs) | Retained in the tree but not declared as a module in `dev/mod.rs`, so it is not compiled. This source is inherited from stock. |
| `TracingFloorPlanner` | [`tfp.rs:93`](../../src/dev/tfp.rs) | Wraps a floor planner and emits `tracing` events per region and assignment. |
| `metadata::{Column, Region, Gate, Constraint, VirtualCell}` | [`metadata.rs`](../../src/dev/metadata.rs) | Names used in failure reports. |

## Difference from stock midnight-proofs 0.8.3

The changes are test-only. They make two textual changes and do not change behavior under any feature:

- `cost_model.rs:757`: the test lookup table is filled with `Fq::from(row)` instead of `Fq::from(row + 1)`, so the table contains zero. This is commit `66a69b7` ("Include zero in the cost-model test lookup table").
- `mod.rs:618`: a doc-test line adds `#[cfg(feature = "circuit-params")]` so the doc-test compiles under that feature.

## Invariants and pitfalls

- Passing `MockProver` does not establish real prover soundness or successful proof creation. Stock never checks that `Z_H` divides the numerator (see `wiki-llm/lessons/correctness.md`, "The prover never checks `Z_H | numerator`"). A bad witness yields a garbage, truncated quotient and a proof the verifier rejects. Use `MockProver` first, then use the stock verifier as the oracle for real proofs.
- `MockProver` has no Fiat-Shamir transcript. It cannot detect the transcript-order defect for multiple circuits with committed instances (see [plonk](plonk.md)).
- Cost-model numbers are estimates, not measured bytes. `examples/proof-size.rs` prints a real size.

## Tests

`mod.rs` contains 4 unit tests. `cost_model.rs` has `cost_model` (`:811`) and `check_correct_computation_k` (`:843`). `loki-harness` uses `MockProver` controls to mutate each of the four constraint families ([loki-harness](loki-harness.md)). `fuzz/diff/tests/soundness_mutation.rs` ports Moriarty's witness-mutation harness ([fuzz](fuzz.md)).

## Notes

No dedicated `LOKI-*.md`. `LOKI-FUZZ.md` covers the mutation port.
