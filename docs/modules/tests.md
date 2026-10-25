# `tests/`: integration tests and verifier checkers

| Path | What it is |
|---|---|
| `tests/plonk_api.rs` | `plonk_api`: the stock end-to-end PLONK API test (keygen, prove, verify) run against Loki. |
| `tests/cpu.rs` | `fixed_rng_lookup_proofs_are_reproducible`: every proof is verified. The test checks byte reproducibility only when `loki-deterministic-lookup-order` is enabled, with a fixed RNG and Rayon pool size. Needs `tests/support/cpu_circuit.rs`. |
| `tests/support/cpu_circuit.rs` | `CpuCircuit { rows }`: public synthetic lookup and copy circuit (3 advice columns, one table column). Shared by `tests/cpu.rs`, `examples/cpu-bench.rs` and the stock checkers. |
| `tests/gpu-compat/` | Package `loki-gpu-stock-verifier`: registry `midnight-proofs =0.8.3` only. `circuit.rs` has `produce(dir)` and `verify(dir)`; `main.rs` verifies a fixture directory. `LOKI-GPU-MSM.md` and `LOKI-SRS-REGISTRY.md` use it as the external stock verifier for a `cuda` proof. |
| `tests/ntt-stock/` | Package `loki-ntt-stock-check` (feature `gpu`, enabling `loki/cuda-ntt` and `loki-cuda-ntt`). Compares the Loki and stock crates, times the `loki_ntt_*` tracing spans, and checks `loki_cuda_ntt::completed_calls` grows only for GPU runs. |
| `tests/stock-profile-verifier/` | Package `loki-profile-stock-verifier`: takes `k` and a proof path and verifies the proof with the registry crate (`LOKI-PROFILE.md`). |
| [`tests/support/stock_fixture.rs`](../../tests/support/stock_fixture.rs) | Shared P1 fixture code for stock cross-verification. Its no-lookup, single-leftover and many-leftover shapes control whether lookup-row order can affect proof bytes. |
| [`tests/backend_default_stock.rs`](../../tests/backend_default_stock.rs) | P1 default-build gate. Checks the default feature set, proof acceptance and byte equality for no-lookup and controlled-order lookup circuits, cross-verification for many leftovers, plus MSM, domain-conversion and lookup-order behavior against stock. |
| [`tests/backend_cpu_equivalence.rs`](../../tests/backend_cpu_equivalence.rs) | P1 CPU seam tests, enabled by `loki-backend-api`. Compares MSM and transforms with stock and naive results, checks request validation and capabilities, and exercises dispatcher routing, fallback and accelerator failure handling with a stub. |

`Cargo.toml` registers `cpu`, `plonk_api`, `backend_default_stock` (requires `committed-instances`) and `backend_cpu_equivalence` (requires `committed-instances` and `loki-backend-api`). The three side directories are separate packages. `gpu-compat` and `ntt-stock` have checked-in lockfiles; `stock-profile-verifier` does not. They must not inherit a Loki patch.

## Difference from stock midnight-proofs 0.8.3

`tests/plonk_api.rs` is the stock test. The other listed files were added in Loki.

## Invariants and pitfalls

- Byte reproducibility for lookup proofs is checked only with the opt-in `loki-deterministic-lookup-order` feature. Each proof must verify in either build. The test uses a fixed Rayon pool size because vanishing randomness is split by worker count. Stock's randomized lookup-row order prevents cross-process byte reproducibility by default.
- Fixtures are synthetic and use the public `unsafe_setup` utility. Its source documentation says it must not be used in production.
- The stock-side packages use an independent verifier. Self-verification would not establish compatibility with the registry verifier.

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md), [`LOKI-GPU-NTT.md`](../../LOKI-GPU-NTT.md), [`LOKI-PROFILE.md`](../../LOKI-PROFILE.md), [`LOKI-P1-SEAM.md`](../../LOKI-P1-SEAM.md). See also [tests-stock](tests-stock.md) and [probes](probes.md).
