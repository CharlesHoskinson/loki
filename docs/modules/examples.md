# `examples/`: runnable examples

| Example | Features | Role |
|---|---|---|
| `simple-example.rs`, `two-chip.rs`, `vector-mul.rs`, `vector-ops-unblinded.rs`, `serialization.rs`, `proof-size.rs` | none | Stock examples: circuit-authoring patterns, key and proof serialization, and real proof size. `vector-ops-unblinded` builds two circuits and checks that their transcripts have the same inputs. |
| [`cpu-bench.rs`](../../examples/cpu-bench.rs) | none | Args `k` (default 16), `repeats` (3), optional output directory. Proves the synthetic `CpuCircuit` (from `tests/support/cpu_circuit.rs`), verifies each proof and reports timings and hashes. With an output directory it writes `proof.bin`, `vk.bin`, `params.bin` for the stock verifiers. |
| [`loki-profile.rs`](../../examples/loki-profile.rs) with `examples/loki-profile/circuit.rs` | `loki-profile` (required) | Filled-domain synthetic workload with two witness phases, lookup, permutation and trash. Arg `k`, optional output path. Prints the phase report (`LOKI_PROFILE` JSONL optional). It also launches a child test process to populate a profile. |

## Difference from stock midnight-proofs 0.8.3

The first row lists stock examples. `cpu-bench` and `loki-profile` are Loki examples.

## Invariants and pitfalls

- The SRS is `unsafe_setup` from fixed ChaCha20 seeds. It is public test data, never a production setup.
- `loki-profile` is a synthetic CPU workload. It is not a Sherlock recursive parent and not a k20 result.
- Exported fixtures feed [tests-stock](tests-stock.md) and `tests/stock-profile-verifier` ([tests](tests.md)).

## Tests

Examples are compiled by `cargo build --examples`. `loki-profile` is declared with `required-features`, so a plain `cargo build --examples` skips it.

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md), [`LOKI-PROFILE.md`](../../LOKI-PROFILE.md).
