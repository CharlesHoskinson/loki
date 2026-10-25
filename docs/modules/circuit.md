# `src/circuit`: layouters, floor planners, values

## Purpose

The circuit-authoring API. A `Circuit` (defined in [`src/plonk/circuit.rs`](../../src/plonk/circuit.rs)) implements `synthesize`. It receives a `Layouter` from this module and assigns cells through it. Floor planners decide which row each region lands on. Both the prover (witness generation), the key generator (fixed columns, copy constraints) and the `MockProver` run synthesis through these types.

## Public API

| Item | Where | Role |
|---|---|---|
| `Chip` trait | [`mod.rs:30`](../../src/circuit/mod.rs) | Marker for a gadget's config and loaded state. |
| `Cell`, `AssignedCell<V, F>` | `mod.rs:93`, `mod.rs:104` | A located cell and an assigned value with its `Value<V>`. |
| `Value<V>` (`known`, `unknown`, `map`, `zip`, ...) | [`value.rs:19`](../../src/circuit/value.rs) | Witness value that is absent during key generation. `Value::unknown()` is `value.rs:31`. |
| `Layouter`, `RegionLayouter`, `RegionShape` | [`layouter.rs`](../../src/circuit/layouter.rs) (`RegionLayouter` at `:49`, `RegionShape` at `:138`) | Region assignment interface and the shape recorded for measurement. |
| `TableLayouter`, `SimpleTableLayouter` | [`table_layouter.rs:21`, `:44`](../../src/circuit/table_layouter.rs) | Lookup-table column assignment. |
| `SimpleFloorPlanner` / `SingleChipLayouter` | [`floor_planner/single_pass.rs:25`, `:40`](../../src/circuit/floor_planner/single_pass.rs) | One pass, regions placed in call order. |
| `V1` / `V1Pass` | [`floor_planner/v1.rs:29`, `:153`](../../src/circuit/floor_planner/v1.rs) | Two passes: measure every region (`MeasurementPass`, `:234`), then pack and assign (`AssignmentPass`, `:263`). `v1/strategy.rs` holds the packing strategy. |

## Difference from stock midnight-proofs 0.8.3

This directory matches the registry `midnight-proofs-0.8.3`. Its only `cfg(feature)` branches are the inherited `circuit-params` ones in `table_layouter.rs` and the two planners.

## Invariants and pitfalls

- Closures passed to region assignment must not be assumed to run once. `V1` replays them across its passes. `LOKI-CPU.md` ("Lookup sorting and region closures") records that Loki deliberately did not change closure invocation semantics in the default build.
- `Value::unknown()` must be handled in `without_witnesses`. Key generation synthesizes with unknown values.
- Rational cells (`Rational<F>`, see [utils](utils.md)) are inverted later in a batch. A cell with a zero denominator evaluates to zero, not an error.

## Tests

`table_layouter.rs` has 4 unit tests. Planner behavior is exercised end to end by `tests/plonk_api.rs`, `MockProver` runs in `src/dev/mod.rs`, and by every circuit in `examples/`.

## Notes

No `LOKI-*.md` note covers this module. See [dev](dev.md) for `TracingFloorPlanner`, which wraps any planner.
