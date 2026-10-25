# Loki module reference

Loki is a fork of midnight-proofs 0.8.3. Add it through `[patch.crates-io]`. Its compatibility contract is that the unmodified stock 0.8.3 verifier accepts proofs for tested, supported cases; the [README](../../README.md) lists those cases and limitations. Each page describes a module, gives public API references as `file:line`, compares it with stock, and lists its pitfalls and tests. Line numbers refer to the tree at `a256a9a`.

For the prover API, start with [plonk](plonk.md). The [examples](examples.md) show runnable circuits, and the [loki-harness](loki-harness.md) checks proofs with the independent stock crate.

## Feature flags (from `Cargo.toml`)

| Flag | Effect |
|---|---|
| `committed-instances` (default) | Committed instance columns in the prover and verifier API. |
| `circuit-params`, `derive_serde`, `dev-curves`, `bench-internal`, `truncated-challenges` | Inherited from stock. `truncated-challenges` must match on prover and verifier. |
| `tabbycat` | Optional upstream dependency for `src/dev/graph.rs`, which is not compiled. |
| `loki-profile` | Phase profiler and arithmetic counters. |
| `loki-multiopen` | Parallel KZG multiopen. |
| `loki-cpu-budget` | Serial BLST (`blst/no-threads`) with MSM chunks on the Rayon pool. |
| `loki-harden` | Checked domain construction and widened rotation arithmetic. |
| `cuda` | sppark MSM on the GPU, CPU fallback. |
| `cuda-ntt` | sppark NTT on the GPU, CPU fallback. |
| `cuda-srs-registry` (implies `cuda`) | Exact-prefix resident SRS registry. |
| `loki-backend-api` | Optional typed backend seam for MSM and host transforms. |
| `loki-sppark` | Typed backend API with the legacy sppark CUDA MSM and NTT path. |
| `loki-gpu-backend` | Opt-in adapter for the separate `loki-gpu` backend; arithmetic support is still pending. |
| `loki-deterministic-lookup-order` | Sorts lookup leftovers instead of using the default HashMap iteration order. |

Always-on CPU changes include parallel inversion, prefix product, and compact rational conversion. Deterministic lookup leftover ordering is opt-in through `loki-deterministic-lookup-order`; the default uses the stock HashMap order.

## Index

| Page | Source | Purpose |
|---|---|---|
| [circuit](circuit.md) | `src/circuit` | Layouters, floor planners, `Value`. Same as stock. |
| [dev](dev.md) | `src/dev` | `MockProver`, cost model, gate and failure reports. Same as stock. |
| [backend](backend.md) | `src/backend` | Typed backend API, stock CPU adapter, dispatch and fallback. Enabled by `loki-backend-api`. |
| [plonk](plonk.md) | `src/plonk` | Prover, verifier, key generation, evaluation. |
| [plonk-arguments](plonk-arguments.md) | `src/plonk/{lookup,permutation,vanishing,trash}` | The four sub-arguments. |
| [poly](poly.md) | `src/poly/{mod,query,commitment}.rs` | Polynomial types, queries, commitment trait. |
| [poly-domain](poly-domain.md) | `src/poly/domain.rs` | Evaluation domain, FFT, GPU NTT dispatch. |
| [poly-kzg](poly-kzg.md) | `src/poly/kzg` | KZG scheme, SRS parameters. |
| [poly-msm](poly-msm.md) | `src/poly/kzg/msm*` | MSM entry, CPU budget, CUDA adapter, SRS registry. |
| [poly-multiopen](poly-multiopen.md) | `src/poly/kzg/multiopen*` | Parallel multiopen. |
| [transcript](transcript.md) | `src/transcript` | Fiat-Shamir transcript. Same as stock. |
| [utils](utils.md) | `src/utils` | Arithmetic helpers, serialization, `Rational`. |
| [profile](profile.md) | `src/profile.rs` | `loki-profile` phase profiler. |
| [cuda-kernels](cuda-kernels.md) | `cuda/`, `build.rs` | MSM kernel adapter and its build. |
| [cuda-runtime](cuda-runtime.md) | `cuda-runtime/` | Device mutex shared by MSM and NTT. |
| [cuda-ntt](cuda-ntt.md) | `cuda-ntt/` | GPU NTT adapter. |
| [loki-harness](loki-harness.md) | `loki-harness/` | W2 differential gate against stock. |
| [tests-stock](tests-stock.md) | `tests-stock/` | Registry-only verifier check. |
| [tests](tests.md) | `tests/` | Integration tests and stock-side checkers. |
| [fuzz](fuzz.md) | `fuzz/` | W7 differential fuzzing. |
| [quint](quint.md) | `quint/` | State models and Rust replay. |
| [probes](probes.md) | `probes/` | Stock verification of the real A3 proof. |
| [benches](benches.md) | `benches/` | Criterion benchmarks. |
| [examples](examples.md) | `examples/` | Runnable examples and fixtures. |
| [scripts](scripts.md) | `scripts/` | W9 validation drivers (host-specific). |
| [loki-gpu](loki-gpu.md) | separate repo | GPU backend modules; P1 call sites use the backend API, but GPU arithmetic remains unsupported. |

## Notes (primary sources)

[`LOKI-CPU.md`](../../LOKI-CPU.md) (W3), [`LOKI-PROFILE.md`](../../LOKI-PROFILE.md) (W1), [`LOKI-GPU-MSM.md`](../../LOKI-GPU-MSM.md) (W4), [`LOKI-GPU-NTT.md`](../../LOKI-GPU-NTT.md) (W5), [`LOKI-QUINT.md`](../../LOKI-QUINT.md) (W8), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md) (W9), [`LOKI-MULTIOPEN.md`](../../LOKI-MULTIOPEN.md) (W11), [`LOKI-FUZZ.md`](../../LOKI-FUZZ.md) (W7), [`LOKI-SRS-REGISTRY.md`](../../LOKI-SRS-REGISTRY.md) (W13), [`LOKI-DONE.md`](../../LOKI-DONE.md) (gates G1-G9), [`CHANGELOG.md`](../../CHANGELOG.md).

## Limits to keep in mind

- Kernel speedups alone barely moved end-to-end time. Most time sits in CPU phases.
- The GPU MSM is slow on repeated scalars (2.4 s versus 25 ms at k18).
- Most measurements are synthetic circuits on one shared host with few samples.
