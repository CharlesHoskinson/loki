# Loki P1: stock CPU plug-in seam

Package P1 of the Loki GPU backend plan (`/home/charles/repos/loki-gpu/design/PLAN.md`).
P1 adds the optional `loki-backend-api` seam, which leaves default proof behavior unchanged. It also audits and corrects drift between Loki main and stock midnight-proofs 0.8.3.

The plan labels claims `SOURCE`, `CODE`, `MEASURED`, or `OPEN QUESTION`.

## Base and references

- SOURCE: branch `loki/p1-backend-seam` is based on loki-proofs main `a868ee0`, which includes W9, W11, W7 and W13. The plan's pin was `877114d`. Line numbers below refer to `a868ee0`.
- SOURCE: the stock reference is the crates.io `midnight-proofs 0.8.3` package. Its `.crate` file in `~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/` has sha256 `19b2e4b1acb581ad741b0cb7c0e9dbdf51142f4e9518886f25fcff40a6702dcc`. That matches the checksum in `tests-stock/Cargo.lock` and `loki-harness/Cargo.lock`. The sha256 of its unpacked `src/` tree is `e2ee92f9a67ce13b4ca055452e8d122258929273e03eac81ebb9111a611c7059`, computed as `find src -type f | sort | xargs sha256sum | sha256sum`.
- SOURCE: the API crate `loki-backend-api` and the `loki-gpu` skeleton come from `/home/charles/repos/loki-gpu` at `df9584f`. P1 depends on them through absolute path dependencies (see open questions). The API was not changed.

Later path update: commit `fbd923a92c2400b5ed36474ce16bbe407bc7ff1d` changed both manifests to sibling-relative dependencies: `../loki-gpu/crates/loki-backend-api` and `../loki-gpu/crates/loki-gpu`. The absolute-path questions below describe the earlier state and were resolved by this commit.

## Commits

1. `886a9fb` Restore stock leftover lookup traversal by default.
2. `be63386` Add optional loki-backend-api seam with stock CPU adapter.
3. Record P1 seam audit and gate results (this report).

## Audit: every drift between Loki main (`a868ee0`) and stock 0.8.3 on the default build

Method: `diff -rq` and `diff -u` were run against the registry source for every differing file. Each hunk was classified by feature gate and effect on values, RNG, transcript, iteration order, threads, encoding, and API. The audit covers 16 changed files plus the Loki-only `src/profile.rs`, `src/poly/kzg/{msm/,multiopen*}`, and `src/poly/domain/`. A separate read-only pass over every file except the lookup prover found the same results.

### Fixed by P1

| Location (a868ee0) | Drift | Effect | P1 resolution |
|---|---|---|---|
| `src/plonk/lookup/prover.rs:426-428` | Leftover table keys were sorted unconditionally (`collect` then `sort_unstable_by`). Stock iterates `leftover_table_map.iter()` in `HashMap` order. | Order-affecting. This changed lookup column placement and proof bytes. | The default branch again uses stock `leftover_table_map.iter()`. Sorting now runs only under `loki-deterministic-lookup-order = []`, which no backend feature enables. The stock comment above the map was also restored. |

CODE: `tests/cpu.rs::fixed_rng_lookup_proofs_are_reproducible` passed only because of the sort. Its CpuCircuit leaves 128 distinct leftover keys. The test still requires every proof to verify. It asserts byte reproducibility only under `loki-deterministic-lookup-order`.

### Unconditional changes that preserve values, RNG, and order

| File (Loki lines) | Change | Why it is equivalent |
|---|---|---|
| `plonk/lookup/prover.rs` ~182-235 | Builds `lookup_product` over `u = n-(b+1)` usable rows instead of `n`. Uses `batch_invert_parallel`, then `prefix_product` and `push(last)`, then `b` random values. | Stock `.take(n-b)` only reads `lookup_product[0..u)`. Each row's inverse is independent and zero stays zero, as in ff's `BatchInvert`. The prefix product is exact field arithmetic for any chunking. The same `b` RNG draws happen in the same position. |
| `plonk/lookup/prover.rs` ~391 | `permuted_input_expression.par_sort()` replaces `sort()`. | Both are stable sorts under a total order, so the output is identical. |
| `plonk/permutation/prover.rs:84-86,107,142-145` | Same pattern: `u` rows, `batch_invert_parallel`, `prefix_product`, `b` randoms. | Same argument as above. RNG draws are unchanged in count and position. |
| `utils/arithmetic.rs:172-221` | New crate-private `batch_invert_parallel` and `prefix_product`. | Exact for any thread count. |
| `poly/mod.rs:290-318` | `batch_invert_rational` skips denominators equal to one. | Values match `Rational::evaluate` semantics. The branch on `denominator == ONE` examines witness data; stock sent every value to batch inversion. This changes timing but not output. |
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

SOURCE: the normalized `Cargo.toml` and `Cargo.toml.orig` already disagreed before P1. In `.orig`, blst is optional and `cuda` lists `dep:blst`. Cargo uses `Cargo.toml`, where blst is unconditional and `cuda` has no `dep:blst`. P1 mirrors its entries into both files and leaves this existing inconsistency unchanged.

## Code: what P1 adds

- Features in both manifests are `loki-backend-api = ["dep:loki-backend-api"]`, `loki-sppark = ["loki-backend-api", "cuda", "cuda-ntt"]`, `loki-gpu-backend = ["loki-backend-api", "dep:loki-gpu", "loki-gpu/cuda"]`, and `loki-deterministic-lookup-order = []`. `default = ["committed-instances"]` is unchanged. W2 forwards `loki-gpu-backend = ["loki/loki-gpu-backend"]`.
- The compile-time guard in `src/lib.rs` rejects `loki-gpu-backend` combined with `cuda` or `cuda-ntt`, directly or through `loki-sppark`.
- `src/backend/` exists only with `loki-backend-api`:
  - `mod.rs` re-exports `api` and maps types to ABIs by exact `TypeId`. Only `midnight_curves::Fq` and `G1Affine` get an ABI.
  - `cpu.rs` holds `CpuBackend`. Its MSM is `msm_cpu` over `MsmRequest::pair_count()`, so `ZipTruncate` gives the stock `min` prefix. Its transform validates the request, then runs the stock sequence: coset scale, zero pad, `best_fft`; or for the inverse, `best_fft` with the inverted root, one `1/n`, truncate, unscale. The cube-root coset path is literally stock `distribute_powers_zeta`. Resident, quotient and SRS operations return `BackendError::unsupported`.
  - `dispatch.rs` holds the dispatcher. It validates every request before any backend sees it. An accelerator is called only when it advertises the operation, the exact ABI and a sufficient `max_transform_log`. An empty MSM never reaches a backend. Any error falls back to CPU, because host inputs are borrowed and retained. `SessionPoisoned`, `ContextLost`, `Poisoned` or a wrong-length result disables the accelerator for the rest of the process. Counters record CPU, accelerated and fallback calls.
- Call sites keep each original body on the feature-off branch:
  - `poly/kzg/msm.rs::msm_specific`: the legacy `cuda` arm runs first and is unchanged, then dispatch. `msm_cpu` became `pub(crate)`.
  - `poly/domain.rs::{fft, ifft}`: the legacy `cuda-ntt` arm runs first and is unchanged, then dispatch. The `best_fft` import is `cfg(not(loki-backend-api))`.
  - `poly/domain.rs::{coeff_to_extended, extended_to_coeff}`: an accelerator-only coset hook that returns `None` without an accelerator, so the stock body runs.
- P1 does not touch `plonk/prover.rs::compute_nu_poly` or `plonk/evaluation.rs::evaluate_numerator`. The CPU quotient remains the existing evaluator, with no typed program to route. This also avoids conflicts with W10 (`loki/w10-gpu-quotient`) and departs from the plan's file list. P5 adds that hook.

## Gates measured on this host (CARGO_BUILD_JOBS=4)

All commands were run from the worktree with `--offline`. Proof runs held `~/.cache/doctorwatson/heavy.lock` via flock. Commands that need `CUDA_HOME` used `CUDA_HOME=$HOME/.local/opt/cuda-13.4.3`. They compile with nvcc but open no device.

| Gate | Command | Result |
|---|---|---|
| Default check | `cargo check --locked` | exit 0 |
| Seam checks | `cargo check --locked --features loki-backend-api`, and the same plus `loki-deterministic-lookup-order` | exit 0, both |
| Accelerator checks | `cargo check --locked --features loki-gpu-backend`; `... --features loki-sppark` | exit 0, both; sppark 0.1.16 compiled |
| Exclusivity guard | `cargo check --locked --features loki-gpu-backend,{cuda-ntt,loki-sppark,cuda}` | exit 101, all three, with the "mutually exclusive" `compile_error!` |
| New default test | `cargo test --release --locked --test backend_default_stock -- --test-threads=1` | 9/9 final (default and seam builds); 7/7 before the review fix and the single-leftover check |
| New seam test | `cargo test --release --locked --features loki-backend-api --test backend_cpu_equivalence -- --test-threads=1` | 14/14 |
| New tests under the new backend | `cargo test --release --locked --features loki-gpu-backend --test backend_cpu_equivalence --test backend_default_stock` | 14/14 and 7/7 (pre-fix); skeleton advertises nothing, so 0 accelerated dispatches |
| Opt-in order | `cargo test --release --features loki-deterministic-lookup-order --test backend_default_stock --test cpu` | 7/7 (pre-fix) and 1/1; lookup proofs byte-repeatable |
| Full CPU suite, default | `RAYON_NUM_THREADS=4 cargo test --release --locked` | lib 34 passed, 2 ignored; backend_default_stock 7 (pre-fix); cpu 1; plonk_api 1; doctests 9; exit 0 |
| Full CPU suite, seam | `RAYON_NUM_THREADS=4 cargo test --release --locked --features loki-backend-api` | lib 34 passed, 2 ignored; backend_cpu_equivalence 14; backend_default_stock 7 (pre-fix); cpu 1; plonk_api 1; doctests 9; exit 0 |
| W2 gate, default | `./loki-harness/run.sh` (k10/k14/k17, `RAYON_NUM_THREADS=1`) | exit 0. Every k cross-verifies both ways with byte-equal VKs. No-lookup control is byte-equal (1136 bytes, 0 differing). Lookup bytes differ at offset 192, which is diagnostic: stock HashMap order. |
| W2 gate, forwarded feature | `cargo run --release --locked --features loki-gpu-backend --bin gate` in `loki-harness/` (run.sh env) | exit 0, same results. `cargo tree -i loki-gpu` shows `loki-harness/loki-gpu-backend` → `midnight-proofs/loki-gpu-backend` → `loki-gpu/cuda`. Compile/forwarding gate only, not a GPU pass. |
| W2 tests | `RAYON_NUM_THREADS=1 cargo test --release --locked [--features loki-gpu-backend] -- --test-threads=1` in `loki-harness/` | differential: 7 passed, 1 ignored (k20), in both configs |
| Independent stock verifier | `cargo run --release --locked --example cpu-bench -- 14 1 target/cpu-fixture[-seam]`, then `cargo test --release --locked --manifest-path tests-stock/Cargo.toml` (`LOKI_CPU_FIXTURE` for seam) | 1/1 for the default fixture and 1/1 for the seam fixture. The unmodified 0.8.3 verifier accepts the proof and rejects the corrupted one. |
| Feature graph | `cargo tree --locked -e features,normal,build [--features …]` | Default: no loki-backend-api, loki-gpu, sppark, loki-cuda-*, or `blst/no-threads`; the only `cc` is blst's build dependency. The default tree matches a868ee0's, apart from the root path line. Its resolved `crate feature` pairs also match the stock 0.8.3 crate resolved alone; the only shape difference is blst's pre-existing direct-dependency position. Seam: adds only `loki-backend-api` (+ ff/group, already present). `loki-gpu-backend`: adds `loki-gpu` with `cuda` (+ cc). `loki-sppark`: adds sppark, loki-cuda-ntt, and loki-cuda-runtime. |

Proof-level byte equality versus crates.io 0.8.3 (same SRS bytes through both readers, same VK and verifier-param bytes, seed-fixed RNG, one-worker Rayon pool):

| Fixture | Default build | Seam build |
|---|---|---|
| No lookup (seeds 0x42, 0x07) | byte-identical | byte-identical |
| Lookup with one nonzero leftover key (controlled map order) | byte-identical | byte-identical |
| Lookup with 192 distinct leftovers | cross-verifies; bytes differ, as stock-vs-stock also does (first difference at 144) | same |

Each fixture also passes negative controls in the stock verifier: a wrong public input, an empty proof, a truncated proof, a trailing byte and a bit flip are all rejected, and the transcript must be fully consumed. `lookup_leftovers_follow_stock_hashmap_order` is the drift detector. On the default build, two in-process proofs with many leftovers must differ, as stock's do. Under `loki-deterministic-lookup-order` they must be identical. The pre-P1 sorted code fails the default assertion.

Review fix for `be63386`: the original `default_feature_set_is_stock` skipped itself whenever a non-default flag was on. A feature accidentally added to `default` would therefore have skipped the check too. The test was replaced by:

- `manifest_default_features_are_stock`, which runs regardless of build flags and requires exactly `default = ["committed-instances"]` in both `Cargo.toml` and `Cargo.toml.orig`. MEASURED: it fails when `loki-backend-api` is temporarily added to `default`.
- `default_build_configuration_is_stock`, an explicitly labeled cfg report.

The only change is to this test file. "(pre-fix)" counts above came from runs before this fix.

Matched CPU latency (MEASURED on one host; no GPU or recursion measurement). `cpu-bench 16 7` (CpuCircuit, k16, 4 Rayon threads) ran three interleaved rounds per binary, 21 samples each, under the heavy lock:

| Binary | Median prove ms | Range | Max RSS (median) |
|---|---|---|---|
| base a868ee0 (sorted) | 980.1 | 938.0 to 1080.8 | 227 492 kB |
| P1 default | 981.6 | 927.8 to 1031.3 | 226 808 kB |
| P1 seam (`loki-backend-api`) | 988.2 | 960.7 to 1037.3 | 227 020 kB |

The ranges overlap, with no significant regression. `equal_first=false` in the P1 builds shows the restored stock behavior: lookup proofs are not byte-repeatable within a process.

## Follow-ups and historical assumptions

- Historical path-layout question, resolved by `fbd923a92c2400b5ed36474ce16bbe407bc7ff1d`: Cargo reads path-dependency manifests and locks them during resolution even when every backend feature is off. At the time, every Loki build and consumer patched onto Loki needed `/home/charles/repos/loki-gpu/crates/{loki-backend-api,loki-gpu}` present on disk, and consumers' `Cargo.lock` files gained the two entries on their next resolve. That absolute layout worked on this host but was not portable. The later commit set the sibling-relative layout described above.
- Cargo metadata for sibling projects passes in `tests-stock`, `tests/gpu-compat`, `tests/ntt-stock`, and `loki-harness` (updated in `be63386`). It fails in `fuzz` (lock lacks `loki-cuda-runtime`) and in `tests/stock-profile-verifier`, `cuda-ntt`, and `cuda-runtime` (no lockfile committed). All four fail identically at `a868ee0`; these issues predate P1 and are outside its file ownership.
- P1 shares only `src/plonk/lookup/prover.rs` with W10. P1's hunk is the leftover traversal (~L423-437); W10's argument-commit region is ~L116-134. A clean rebase was expected. P1 leaves `prover.rs` and `evaluation.rs` untouched. Rebase remained pending coordinator direction.
- At the time of this report, `loki-backend-api` had an uncommitted additive `ErrorKind::Cancelled` for P2. The seam compiled against it, and its error handling was generic. For P2/P7, the dispatcher fell back to CPU on any accelerator error. P1 did not cancel requests, so `Cancelled` could not arise. Once cancellation is supported, cancelled work must not be retried automatically.
- The full-suite, harness, and stock-verifier runs used `be63386`. After them, only `tests/backend_default_stock.rs` (and this report) changed, and that file was re-run at 9/9 in the default and seam builds. `loki-gpu` advanced to `d71b7ca` (P2 merged) during P1; `cargo check --locked --features loki-gpu-backend --tests` was re-run against it, exit 0.

- Historical path detail, resolved by `fbd923a92c2400b5ed36474ce16bbe407bc7ff1d`: dependencies used absolute paths (`/home/charles/repos/loki-gpu/crates/...`), which could not resolve from both this worktree and `~/repos/loki-proofs`. `Cargo.lock` records no path, so `--locked` was unaffected. The commit changed both manifests to sibling-relative dependencies.
- `cargo tree` captures use `-e features,normal,build`. They omit dev-dependencies, which now include crates.io `stock` (midnight-proofs `=0.8.3`) for live byte comparison. Dev-dependencies do not affect consumers or the library feature graph.
- Both new tests have `required-features = ["committed-instances"]`, because the stock dev-dependency is built with its own default features. They are not meant to run with `circuit-params`.
- `loki-gpu-backend` builds `loki-gpu/cuda`, which needs `CUDA_HOME` and nvcc. That gate is a compile/forwarding gate with an Unsupported skeleton: there are zero accelerated dispatches, so it is not a GPU pass.
- `ifft` builds its typed request with the canonical forward root, recovered as the inverse of the stock `omega_inv`. The CPU adapter re-inverts it and recomputes `1/n`. Field inverses are unique, so the values are identical. A `debug_assert` checks that the stock divisor is `1/n`.
- The API's `Backend::capabilities` depends on `(F, G)`. For transforms, which have no curve, the dispatcher asks `loki-gpu` with `(Fq, G1Projective)`, the only native pairing. This is a P2 implementation note and does not request an API change.
