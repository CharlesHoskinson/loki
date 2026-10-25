# haiku-v1-1 inventory: compat-api (Loki vs stock midnight-proofs 0.8.3)

Snapshot as of 2026-10-25. Sources: `~/repos/loki-proofs` (main `eaf09d8`) and the stock crate at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/midnight-proofs-0.8.3`. All paths below are relative to those two roots. No builds were run.

## Exists
- Package identity: same name `midnight-proofs` and same version `0.8.3`, edition 2021, rust-version 1.76.0 on both sides (`Cargo.toml:9-15` in each). `[patch.crates-io]` drop-in is name/version compatible.
- Public modules: stock `lib.rs:11-17` (`circuit, plonk, poly, transcript, dev, utils`). Loki `src/lib.rs` keeps all six, and adds `pub mod backend` (line gated `loki-backend-api`) and `pub mod profile` (gated `loki-profile`).
- Feature flags: stock declares 7 keys (`bench-internal, circuit-params, committed-instances, default, derive_serde, dev-curves, truncated-challenges`). `default = ["committed-instances"]` is identical on both sides. Loki adds 11: `cuda-ntt, cuda, cuda-srs-registry, loki-profile, loki-multiopen, loki-cpu-budget, loki-harden, loki-backend-api, loki-sppark, loki-gpu-backend, loki-deterministic-lookup-order`. None is in `default`.
- Manifest diff (`diff Cargo.toml` of the two normalized manifests): the only non-feature changes are `build = "build.rs"` (stock `build = false`), new `[[example]]`/`[[test]]` entries, and new deps. The new non-optional dep `[dependencies.blst] version = "0.3.16"` is already transitive through `midnight-curves` (0.3.1 requires blst 0.3.15), so it adds no new crate to the default graph. `build.rs` only acts under `feature = "cuda"`.
- Dev-dep pin: `[dev-dependencies.stock] package = "midnight-proofs" version = "=0.8.3"` (Loki `Cargo.toml`), which lets tests compare against stock.
- Feature-gate exclusivity: `src/lib.rs` contains a `compile_error!` for `loki-gpu-backend` combined with `cuda`/`cuda-ntt`.
- Existing gates: `tests/backend_default_stock.rs`, `tests/backend_cpu_equivalence.rs`, `tests-stock/tests/verify.rs`, `loki-harness/tests/differential.rs`, plus `tests/ntt-stock/` and `tests/stock-profile-verifier/`.
- Stock's `#![deny(missing_docs)]`, `#![deny(missing_debug_implementations)]`, `#![deny(unsafe_code)]` are carried over unchanged in `src/lib.rs`.

## Missing
- No mechanical API-diff gate. `cargo-semver-checks` and `cargo-public-api` are not installed (`which` returned nothing), and no CI file or script in the repo references either tool.
- No CI check that the default feature set produces byte-identical output to stock. `tests/backend_default_stock.rs` covers only part of that.

## Default-on code changes (not behind a feature). These change default-build behaviour and need a wire-level gate. Algebraic equivalence is argued below from reading the code, not verified by test.
- `src/plonk/permutation/prover.rs:107,142`: batch inversion and `z` computation rewritten with `batch_invert_parallel` and `prefix_product` (`src/utils/arithmetic.rs:172,184`). Stock: `permutation/prover.rs` builds `z` with a sequential loop and sets `last_z = z[n-(b+1)]`. Loki's `prefix_product` is exclusive-scan, and it pushes the total, which matches. Unverified by test.
- `src/plonk/lookup/prover.rs:198,231`: same kind of rewrite. Loki builds `usable_rows = n-(b+1)` values, then pushes the last value and `b` random blinding factors. Stock takes `n-b` from `ONE + products`. Lengths both total `n`. Unverified by test.
- `src/plonk/lookup/prover.rs:391,549`: `sort()` changed to `par_sort()`. Field elements that compare equal are bit-identical, so output should match. Unverified by test.
- `src/poly/mod.rs:298-301`: denominators equal to `ONE` are now filtered out before batch inversion (stock keeps all denominators). Checked by reading: `Rational::denominator()` (`src/utils/rational.rs:290`, identical to stock) returns `None` for `Zero` and `Trivial`, and `Rational(_, ONE)` is skipped in the write-back guard (`src/poly/mod.rs:~313`). Consumption of the inverse iterator is therefore aligned with the filter, and the output matches stock. Still unverified by test.
- `src/poly/domain.rs:267,296,356,412`: `best_fft` now sits behind a `Self::fft` wrapper. The default path still calls `best_fft` (`src/poly/domain.rs:422`).
- `src/poly/kzg/mod.rs:140-154,273-288`: point-set sort uses `sort_by_key((len, i))`. Stock has the identical code (`src/poly/kzg/mod.rs:137-143`, `264-270` of stock). No change.
- `src/poly/kzg/msm.rs:165-182`: `msm_specific` dispatches to `backend::dispatch::msm_specific` only under `loki-backend-api`, and otherwise falls through to `msm_cpu` (now `pub(crate)`). Default path is `msm_cpu`, which matches stock.
- `src/plonk/keygen.rs:27-30`: `pub(crate) fn create_domain` changed its return type to `Result<...>` (default-on). Not public API. Callers were updated.

## Dev-only changes
- `src/dev/cost_model.rs:757`: `Fq::from(row + 1)` changed to `Fq::from(row)`. Inside `#[cfg(test)] mod tests` (line 643). No default public effect. Note it when reviewing cost-model expectations.

## Public API additions (all gated; none ungated)
- `src/backend/*` (module gated by `loki-backend-api`): `CpuBackend`, `Dispatcher`, `DispatchCounters`, `global()`, `arithmetic_abi`, `field_abi`.
- `src/profile.rs` (module gated by `loki-profile`): `Report`, `ProofReport`, `OperationCounts`, `PhaseTiming`, `take_report`.
- `src/poly/domain.rs:39-40`: `EvaluationDomain::try_new` (gated `loki-harden`).
- `src/poly/kzg/msm/cuda/tests.rs` pub fns: sit in `#[cfg(any(test, feature = "cuda"))] mod cuda;` (`src/poly/kzg/msm.rs:418-419`), so they are not visible in a default build.
- No stock `pub` line was removed (textual `pub`-line diff: 0 removed lines). Signature-level changes in `pub` items were not checked mechanically; this inventory is textual.

## Open items for compat-api
1. Add a mechanical public-API gate (for example `cargo public-api --diff-against` against the stock crate, or `cargo semver-checks`). Neither is installed or wired into CI.
2. Confirm the default-on changes above with the W2 stock cross-verify matrix (area compat-proof).
3. `rust-version = "1.76.0"` is declared on both sides, but Loki uses `div_ceil` (stable 1.73), so no new MSRV is needed. MSRV was not built or tested. Unverified.
