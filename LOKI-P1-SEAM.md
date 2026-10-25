# Loki P1: stock CPU plug-in seam

Package P1 of the Loki GPU backend plan (`/home/charles/repos/loki-gpu/design/PLAN.md`).
It does two things:

- adds the optional `loki-backend-api` seam, which leaves the default proof behaviour unchanged;
- audits and corrects the drift that Loki main already had from stock midnight-proofs 0.8.3.

Labels follow the plan's convention: **SOURCE**, **CODE**, **MEASURED**, **OPEN QUESTION**.

## Base and references

- SOURCE: branch `loki/p1-backend-seam` is based on loki-proofs main **`a868ee0`**, which includes W9, W11, W7 and W13. The plan's pin was `877114d`. Line numbers below refer to `a868ee0`.
- SOURCE: the stock reference is the crates.io `midnight-proofs 0.8.3` package. Its `.crate` file in `~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/` has sha256 `19b2e4b1acb581ad741b0cb7c0e9dbdf51142f4e9518886f25fcff40a6702dcc`. That matches the checksum in `tests-stock/Cargo.lock` and `loki-harness/Cargo.lock`. The sha256 of its unpacked `src/` tree is `e2ee92f9a67ce13b4ca055452e8d122258929273e03eac81ebb9111a611c7059`, computed as `find src -type f | sort | xargs sha256sum | sha256sum`.
- SOURCE: the API crate `loki-backend-api` and the `loki-gpu` skeleton come from `/home/charles/repos/loki-gpu` at `df9584f`. P1 depends on them through absolute path dependencies (see open questions). The API was not changed.

## Commits

1. `886a9fb` Restore stock leftover lookup traversal by default.
2. `be63386` Add optional loki-backend-api seam with stock CPU adapter.
3. Record P1 seam audit and gate results (this report).

## Audit: every drift between Loki main (`a868ee0`) and stock 0.8.3 on the default build

Method: run `diff -rq` and `diff -u` against the registry source for every differing file. Each hunk was classified by its feature gate and by its effect on values, RNG, transcript, iteration order, threads, encoding and API. This covers 16 changed files plus the Loki-only `src/profile.rs`, `src/poly/kzg/{msm/,multiopen*}` and `src/poly/domain/`. An independent read-only audit pass over all files except the lookup prover found the same results.

### Fixed by P1

| Location (a868ee0) | Drift | Effect | P1 resolution |
|---|---|---|---|
| `src/plonk/lookup/prover.rs:426-428` | Leftover table keys were sorted unconditionally (`collect` then `sort_unstable_by`). Stock iterates `leftover_table_map.iter()` in `HashMap` order. | **Order-affecting.** It changed lookup column placement and therefore proof bytes. | The default branch is stock `leftover_table_map.iter()` again. The sort now runs only under `loki-deterministic-lookup-order = []`, which no backend feature enables. The stock comment above the map was restored too. |

CODE: `tests/cpu.rs::fixed_rng_lookup_proofs_are_reproducible` only passed because of the sort. Its CpuCircuit leaves 128 distinct leftover keys. The test still requires every proof to verify, but now asserts byte reproducibility only under `loki-deterministic-lookup-order`.

### Unconditional, but value-, RNG- and order-equivalent (recorded, not reverted)

| File (Loki lines) | Change | Why it is equivalent |
|---|---|---|
| `plonk/lookup/prover.rs` ~182-235 | Builds `lookup_product` over `u = n-(b+1)` usable rows instead of `n`. Uses `batch_invert_parallel`, then `prefix_product` and `push(last)`, then `b` random values. | Stock `.take(n-b)` only reads `lookup_product[0..u)`. Each row's inverse is independent and zero stays zero, as in ff's `BatchInvert`. The prefix product is exact field arithmetic for any chunking. The same `b` RNG draws happen in the same position. |
| `plonk/lookup/prover.rs` ~391 | `permuted_input_expression.par_sort()` replaces `sort()`. | Both are stable sorts under a total order, so the output is identical. |
| `plonk/permutation/prover.rs:84-86,107,142-145` | Same pattern: `u` rows, `batch_invert_parallel`, `prefix_product`, `b` randoms. | Same argument as above. RNG draws are unchanged in count and position. |
| `utils/arithmetic.rs:172-221` | New crate-private `batch_invert_parallel` and `prefix_product`. | Exact for any thread count. |
| `poly/mod.rs:290-318` | `batch_invert_rational` skips denominators equal to one. | Values are identical (`Rational::evaluate` semantics). **Note:** it now branches on `denominator == ONE` over witness data, where stock fed everything to batch inversion. That adds a variable-time difference (a side-channel note, not an output change). |
| `poly/domain.rs` `fft`/`ifft` wrappers | The private `Self::fft` and `Self::ifft` helpers now take `&mut Vec`. | With CUDA and profiling off, the bodies are `best_fft` plus the divisor, as in stock. |
| `poly/kzg/msm.rs:165-221` | `msm_specific` delegates to `msm_cpu`. | The `msm_cpu` body is the stock `msm_specific` body. The chunked path exists only under `loki-cpu-budget`. |
| `plonk/keygen.rs:27-50,251` | `create_domain` is `pub(crate)` and returns `Result`. | The default branch still calls `EvaluationDomain::new(degree, k)`. The function is crate-private, so there is no API change. |
| `plonk/evaluation.rs` | Hardening and profiling gates. | The default `RotationIndex` is `i32` with the stock expression. |
| `plonk/prover.rs`, `plonk/vanishing/prover.rs` | Profiling guards and tracing spans; `let result = finalise_proof(..); result`. | No change to squeezes, writes or RNG. |
| `Cargo.toml` direct `blst = 0.3.16` | blst becomes a direct dependency. | Stock already pulls in blst through midnight-curves, with default features only. The lockfile has one blst (0.3.17). The resolved default feature set equals stock exactly (gate below). The only effect is a version floor of ≥0.3.16 for downstream users. |
| `build.rs` | Present unconditionally. | `main()` is empty without `cuda`. |
| `poly/kzg/mod.rs`, `poly/kzg/utils.rs`, `multiopen*` | Gated by `loki-multiopen`. | The default branch is stock code, re-indented. |
| `poly/kzg/msm/cuda/*`, `poly/domain/gpu_tests.rs`, `profile.rs` | Gated by `cuda`, `cuda-ntt` or `loki-profile`; `convert.rs` is compiled in test builds. | Absent from the default library build. |
| `dev/cost_model.rs`, `dev/mod.rs` | Test or doctest only. | None. |

### Manifest drift

SOURCE: the normalized `Cargo.toml` and `Cargo.toml.orig` already disagreed before P1. In `.orig`, blst is optional and `cuda` lists `dep:blst`. In `Cargo.toml`, which Cargo actually uses, blst is unconditional and `cuda` has no `dep:blst`. P1 mirrors its own entries into both files and leaves this pre-existing inconsistency alone.

## CODE: what P1 adds

- **Features**, defined in both manifests: `loki-backend-api = ["dep:loki-backend-api"]`, `loki-sppark = ["loki-backend-api", "cuda", "cuda-ntt"]`, `loki-gpu-backend = ["loki-backend-api", "dep:loki-gpu", "loki-gpu/cuda"]` and `loki-deterministic-lookup-order = []`. `default = ["committed-instances"]` is unchanged. W2 forwards `loki-gpu-backend = ["loki/loki-gpu-backend"]`.
- **Exclusivity:** `src/lib.rs` emits `compile_error!` when `loki-gpu-backend` is combined with `cuda` or `cuda-ntt`, whether directly or through `loki-sppark`.
- **`src/backend/`** (exists only with `loki-backend-api`):
  - `mod.rs` re-exports `api` and maps types to ABIs by exact `TypeId`. Only `midnight_curves::Fq` and `G1Affine` get an ABI.
  - `cpu.rs` holds `CpuBackend`. Its MSM is `msm_cpu` over `MsmRequest::pair_count()`, so `ZipTruncate` gives the stock `min` prefix. Its transform validates the request, then runs the stock sequence: coset scale, zero pad, `best_fft`; or for the inverse, `best_fft` with the inverted root, one `1/n`, truncate, unscale. The cube-root coset path is literally stock `distribute_powers_zeta`. Resident, quotient and SRS operations return `BackendError::unsupported`.
  - `dispatch.rs` holds the dispatcher. It validates every request before any backend sees it. An accelerator is called only when it advertises the operation, the exact ABI and a sufficient `max_transform_log`. An empty MSM never reaches a backend. Any error falls back to CPU, because host inputs are borrowed and retained. `SessionPoisoned`, `ContextLost`, `Poisoned` or a wrong-length result disables the accelerator for the rest of the process. Counters record CPU, accelerated and fallback calls.
- **Call sites**, with every original body kept on the feature-off branch:
  - `poly/kzg/msm.rs::msm_specific`: the legacy `cuda` arm runs first and is unchanged, then dispatch. `msm_cpu` became `pub(crate)`.
  - `poly/domain.rs::{fft, ifft}`: the legacy `cuda-ntt` arm runs first and is unchanged, then dispatch. The `best_fft` import is `cfg(not(loki-backend-api))`.
  - `poly/domain.rs::{coeff_to_extended, extended_to_coeff}`: an accelerator-only coset hook that returns `None` without an accelerator, so the stock body runs.
- **Not touched**: `plonk/prover.rs::compute_nu_poly` and `plonk/evaluation.rs::evaluate_numerator`. In P1 the CPU quotient is the existing evaluator and there is no typed program to route yet. Leaving them alone also avoids conflicts with W10 (`loki/w10-gpu-quotient`). This is a deliberate deviation from the plan's file list. P5 adds that hook.

## Gates (MEASURED, this host, CARGO_BUILD_JOBS=4)

All commands were run from the worktree with `--offline`. Proof runs held `~/.cache/doctorwatson/heavy.lock` via flock. Commands that need `CUDA_HOME` used `CUDA_HOME=$HOME/.local/opt/cuda-13.4.3`. They compile with nvcc but open no device.

| Gate | Command | Result |
|---|---|---|
| Default check | `cargo check --locked` | exit 0 |
| Seam checks | `cargo check --locked --features loki-backend-api`, and the same plus `loki-deterministic-lookup-order` | exit 0, both |
| Accelerator checks | `cargo check --locked --features loki-gpu-backend`; `... --features loki-sppark` | exit 0, both; sppark 0.1.16 compiled |
| Exclusivity guard | `cargo check --locked --features loki-gpu-backend,{cuda-ntt,loki-sppark,cuda}` | exit 101, all three, with the "mutually exclusive" `compile_error!` |
| New default test | `cargo test --release --locked --test backend_default_stock -- --test-threads=1` | 7/7 |
| New seam test | `cargo test --release --locked --features loki-backend-api --test backend_cpu_equivalence -- --test-threads=1` | 14/14 |
| New tests under the new backend | `cargo test --release --locked --features loki-gpu-backend --test backend_cpu_equivalence --test backend_default_stock` | 14/14 and 7/7; skeleton advertises nothing, so 0 accelerated dispatches |
| Opt-in order | `cargo test --release --features loki-deterministic-lookup-order --test backend_default_stock --test cpu` | 7/7 and 1/1; lookup proofs byte-repeatable |
| Full CPU suite, default | `RAYON_NUM_THREADS=4 cargo test --release --locked` | lib 34 passed, 2 ignored; backend_default_stock 7; cpu 1; plonk_api 1; doctests 9; exit 0 |
| Full CPU suite, seam | `RAYON_NUM_THREADS=4 cargo test --release --locked --features loki-backend-api` | lib 34 passed, 2 ignored; backend_cpu_equivalence 14; backend_default_stock 7; cpu 1; plonk_api 1; doctests 9; exit 0 |
| W2 gate, default | `./loki-harness/run.sh` (k10/k14/k17, `RAYON_NUM_THREADS=1`) | exit 0. Every k cross-verifies both ways with byte-equal VKs. No-lookup control is byte-equal (1136 bytes, 0 differing). Lookup bytes differ at offset 192, which is diagnostic: stock HashMap order. |
| W2 gate, forwarded feature | `cargo run --release --locked --features loki-gpu-backend --bin gate` in `loki-harness/` (run.sh env) | exit 0, same results. `cargo tree -i loki-gpu` shows `loki-harness/loki-gpu-backend` → `midnight-proofs/loki-gpu-backend` → `loki-gpu/cuda`. Compile/forwarding gate only, not a GPU pass. |
| W2 tests | `RAYON_NUM_THREADS=1 cargo test --release --locked [--features loki-gpu-backend] -- --test-threads=1` in `loki-harness/` | differential: 7 passed, 1 ignored (k20), in both configs |
| Independent stock verifier | `cargo run --release --locked --example cpu-bench -- 14 1 target/cpu-fixture[-seam]`, then `cargo test --release --locked --manifest-path tests-stock/Cargo.toml` (`LOKI_CPU_FIXTURE` for seam) | 1/1 for the default fixture and 1/1 for the seam fixture. The unmodified 0.8.3 verifier accepts the proof and rejects the corrupted one. |
| Feature graph | `cargo tree --locked -e features,normal,build [--features …]` | **Default:** no loki-backend-api, loki-gpu, sppark, loki-cuda-*, or `blst/no-threads`; the only `cc` is blst's build dependency. The default tree is **identical** to a868ee0's, apart from the root path line. Its set of resolved `crate feature` pairs is **identical** to the stock 0.8.3 crate resolved alone; the only shape difference is blst's pre-existing direct-dependency position. **Seam:** adds only `loki-backend-api` (+ ff/group, already present). **loki-gpu-backend:** adds `loki-gpu` with `cuda` (+ cc). **loki-sppark:** adds sppark, loki-cuda-ntt and loki-cuda-runtime. |

Proof-level byte equality versus crates.io 0.8.3 (same SRS bytes through both readers, same VK and verifier-param bytes, seed-fixed RNG, one-worker Rayon pool):

| Fixture | Default build | Seam build |
|---|---|---|
| No lookup (seeds 0x42, 0x07) | byte-identical | byte-identical |
| Lookup with one nonzero leftover key (controlled map order) | byte-identical | byte-identical |
| Lookup with 192 distinct leftovers | cross-verifies; bytes differ, as stock-vs-stock also does (first difference at 144) | same |

Each fixture also passes negative controls in the stock verifier: a wrong public input, an empty proof, a truncated proof, a trailing byte and a bit flip are all rejected, and the transcript must be fully consumed. `lookup_leftovers_follow_stock_hashmap_order` is the drift detector. On the default build, two in-process proofs with many leftovers must differ, as stock's do. Under `loki-deterministic-lookup-order` they must be identical. The pre-P1 sorted code fails the default assertion.

Matched latency (MEASURED, single host, not a GPU or recursion claim). `cpu-bench 16 7` (CpuCircuit, k16, 4 Rayon threads) ran three interleaved rounds per binary, 21 samples each, under the heavy lock:

| Binary | Median prove ms | Range | Max RSS (median) |
|---|---|---|---|
| base a868ee0 (sorted) | 980.1 | 938.0–1080.8 | 227 492 kB |
| P1 default | 981.6 | 927.8–1031.3 | 226 808 kB |
| P1 seam (`loki-backend-api`) | 988.2 | 960.7–1037.3 | 227 020 kB |

The ranges overlap and there is no significant regression. `equal_first=false` in the P1 builds shows the restored stock behaviour: lookup proofs are not byte-repeatable within a process.

## Open questions and assumptions

- **Path dependencies are absolute** (`/home/charles/repos/loki-gpu/crates/...`). Relative paths cannot resolve from both this worktree and `~/repos/loki-proofs`. `Cargo.lock` records no path, so `--locked` is unaffected. The coordinator should choose the final layout (vendoring, git dependency or a fixed relative layout) before merging.
- `cargo tree` captures use `-e features,normal,build`. They omit dev-dependencies, which now include crates.io `stock` (midnight-proofs `=0.8.3`) for live byte comparison. Dev-dependencies do not affect consumers or the library feature graph.
- Both new tests have `required-features = ["committed-instances"]`, because the stock dev-dependency is built with its own default features. They are not meant to run with `circuit-params`.
- `loki-gpu-backend` builds `loki-gpu/cuda`, which needs `CUDA_HOME` and nvcc. That gate is a compile/forwarding gate with an Unsupported skeleton: there are zero accelerated dispatches, so it is not a GPU pass.
- `ifft` builds its typed request with the canonical forward root, recovered as the inverse of the stock `omega_inv`. The CPU adapter re-inverts it and recomputes `1/n`. Field inverses are unique, so the values are identical. A `debug_assert` checks that the stock divisor is `1/n`.
- The API's `Backend::capabilities` depends on `(F, G)`. For transforms, which have no curve, the dispatcher asks `loki-gpu` with `(Fq, G1Projective)`, the only native pairing. This is not an API change request; it is noted for P2.
