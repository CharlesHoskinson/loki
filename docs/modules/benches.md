# `benches/`: criterion benchmarks

| Bench | Registered as | What it measures |
|---|---|---|
| [`benches/plonk.rs`](../../benches/plonk.rs) | `[[bench]] plonk`, `harness = false` | Criterion groups `plonk-keygen`, `plonk-prover` and a verifier group for a small multiply/add PLONK circuit. |
| [`benches/commit_zk.rs`](../../benches/commit_zk.rs) | `[[bench]] commit_zk` | Criterion group `Blinder_poly`: serial (`rand_poly_serial`) versus parallel (`rand_poly_par`, using `utils::arithmetic::parallelize`) generation of a random blinding polynomial. |
| [`benches/zswap_output.rs`](../../benches/zswap_output.rs) | `[[bench]] zswap_output`, requires feature `bench-internal` | Prover and verifier on the Zswap-output circuit built with `midnight-circuits`. Uses `plonk::bench::prover` internals. |

`bench-internal` enables the optional `criterion` dependency and `plonk::bench`.

## Difference from stock midnight-proofs 0.8.3

These three benchmark files are inherited from stock. Loki's timings come from `examples/loki-profile.rs`, `examples/cpu-bench.rs`, `loki-harness --bin bench` and the ignored manual-benchmark tests (`time_lookup_preparation_k16`, `time_rational_conversion_k16`, `gpu_ntt_benchmark`, `gpu_benchmark`).

## Invariants and pitfalls

- `zswap_output` imports `midnight_circuits`, but `Cargo.toml` declares no such dependency in this tree. It is gated behind `bench-internal` and was not built for this page, so its description remains unverified.
- Criterion timings on a shared host are noisy. The project notes use three-sample medians with alternating order and say so.
- These benchmark targets do not enable GPU features.

## Tests

These benchmark targets have no tests.
