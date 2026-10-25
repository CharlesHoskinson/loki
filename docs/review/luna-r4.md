# luna-r4 review

## Concerns

- At initial review, `docs/modules/utils.md:18,27` described `utils::multicore` as an exposed module and said its source was unchanged from stock. `src/utils/mod.rs:1-7` declares only `arithmetic`, `helpers`, and `rational`; no `pub mod multicore` appears. `src/utils/multicore.rs` exists but is not compiled or reachable from the crate API. The README index did not list it, while the module page promised a public path that users could not import.
- `README.md:120-128` groups CUDA environment variables across `cuda`, `cuda-ntt`, and `cuda-srs-registry`, and says the build reads `CUDA_HOME`, `CUDA_PATH`, `CUDA_TOOLKIT_PATH`, and `NVCC`. The main `build.rs:15-30` supports those variables for `cuda`; the separate `cuda-ntt/build.rs:3-17` supports only `CUDA_HOME`, `NVCC`, and `PATH`. A `cuda-ntt`-only build cannot use the README's `CUDA_PATH` or `CUDA_TOOLKIT_PATH` alternatives. The architecture variables also differ: root `build.rs:13-14` defaults `LOKI_CUDA_ARCH` to `120` and accepts digits only, while `cuda-ntt/build.rs:32-33` passes `LOKI_NTT_CUDA_ARCH` directly as `-arch={arch}` (for example, `sm_120`). `cargo metadata --offline --no-deps --format-version 1` confirms `cuda-ntt` is independent of `cuda`.
- The original tables at `README.md:84-99` and `docs/modules/README.md:5-18` presented their lists as the feature flags from `Cargo.toml`, but omitted `tabbycat`. Cargo metadata reports `tabbycat: ["dep:tabbycat"]`; `Cargo.toml` declares it optional at lines 169-173. Readers checking those tables for a complete feature list would miss it.
- `docs/modules/README.md` had no recommended reading path. Its index listed the prover API, examples, and independent harness without saying where to start.

## Questions

No open questions remain in this lens. The module docs now identify `multicore.rs` as retained, uncompiled source, and the README distinguishes the CUDA build variables for each adapter.

## Suggestions

- In `docs/modules/utils.md`, remove the `multicore` API row and identify `multicore.rs` as retained but uncompiled source. This documentation correction records the current API without changing source.
- In `README.md`, separate environment setup for `cuda` and `cuda-ntt`; for NTT, document only `CUDA_HOME`, `NVCC`, or `nvcc` on `PATH` unless its build script is changed. Clarify that `LOKI_CUDA_ARCH` configures the MSM build and `LOKI_NTT_CUDA_ARCH` configures the NTT build.
- Add `tabbycat` to both feature tables as an upstream optional feature, or label the tables as a selected subset rather than a full Cargo feature list.
- In `docs/modules/README.md`, add a starting path through the prover API, runnable examples, and independent stock harness.

## Novelty evidence

The Rust integrator lens assigned for this pass did not cover novelty claims.
