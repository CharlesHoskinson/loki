# compat-proof inventory (haiku-v1-2)

Scope: wire-level compatibility with stock midnight-proofs 0.8.3 (and 0.8.1 for the version matrix). Read-only inventory; nothing was built or run beyond recorded artifacts and file reads. Verified state = the recorded 2026-10-24 harness run only.

## 1. Existing W2 harness (what is covered)

- Harness compare path: `loki-harness/src/lib.rs:88-172` (`compare<LOOKUPS>`): SRS roundtrip both ways, prove on each side, VK byte equality, verifier-params equality, cross-verify both directions, wrong public input rejected, empty proof rejected, trailing byte rejected, no-lookup proof bytes required identical.
- Test cases: `loki-harness/tests/differential.rs:19-26` runs k10, k14, k17. k20 is `#[ignore]` at `differential.rs:28-31`.
- Stock pin: `loki-harness/Cargo.toml:20` `stock = { package = "midnight-proofs", version = "=0.8.3" }`.
- Stock backend: `loki-harness/src/backend.rs` (`reference` module, `backend!(reference, stock)` at line ~327). Negative controls via MockProver at `backend.rs:306-324`.
- Recorded run (`loki-harness/README.md`, "Recorded validation (2026-10-24)"): all k10/k14/k17 cross-verified, VK bytes equal, SRS roundtrip equal. Lookup proofs 1,280 B first differ at offset 192 (documented stock HashMap-order nondeterminism); no-lookup control 1,136 B byte-identical. No k20 run, no 0.8.1 run.
- Open under T04 (`TODO.md:19-21`): 0.8.1 verifier and k20 cases.

## 2. Gaps in the recorded test coverage (mark: not exercised by any recorded test)

- PK serialization: not cross-read. Only VK and verifier params are compared.
- Serde formats: harness uses `SerdeFormat::RawBytes` only (`backend.rs` setup/prove/verify). `tests-stock/tests/verify.rs:30` uses `Processed`. `RawBytesUnchecked` appears nowhere.
- Transcript hash: only `CircuitTranscript<blake2b_simd::State>` is used (`backend.rs` prove/verify, `tests/support/stock_fixture.rs:197-240`). Stock's `Blake2b256` (`midnight-proofs-0.8.3/src/transcript/mod.rs:6`, `implementors.rs:41`, challenge sampling at `implementors.rs:191`) is never cross-tested.
- Committed instances: harness always passes `&[&[]]` as committed (`backend.rs` prove/verify, `prepare` call), so nb_committed_instances = 0. Non-empty committed columns are not exercised.
- Batch shape: harness and `tests/support/stock_fixture.rs` use a single circuit, `&[circuit]` with one instance set. Multi-proof batches are not exercised.
- Mixed committed/plain multi-proof: stock rejects its own proof for this shape (T03; `LOKI-QUINT.md:14-15`, `124-125`, `161-163`). Not in the harness.
- Malformed multi-proof lengths: 0.8.2+ rejects with `Error::InvalidInstances` (see cp-stock). Not exercised.

## 3. Loki-vs-stock prover deltas that can touch proof bytes

Diffed `src/` against `~/.cargo/registry/src/*/midnight-proofs-0.8.3/src/`: 22 differing entries.

- `src/plonk/lookup/prover.rs`: ungated default-path change. `prefix_product` and `batch_invert_parallel` replace the stock `batch_invert` + scan; `par_sort` replaces `sort` for `permuted_input_expression` (equal field elements are byte-identical, so output should be unchanged, but this is unverified by a test). Leftover table order is stock `HashMap` order by default; `loki-deterministic-lookup-order` (opt-in, `Cargo.toml`) changes lookup bytes intentionally.
- `src/plonk/permutation/prover.rs`: ungated default-path change, same `prefix_product` pattern. Claimed to keep RNG draw count identical (blinding factors drawn in the same order); not checked by any test beyond the W2 harness.
- `src/poly/kzg/mod.rs:114-117`: `loki-multiopen` (opt-in) replaces the prover q-poly path via `multiopen::prepare`. Default path, `mod.rs:117-200`, is the stock code. The point-set sort `(len, i)` is kept at `src/poly/kzg/multiopen.rs:127` and `mod.rs:150`, matching stock `mod.rs:139`.
- `src/poly/kzg/msm.rs`: default path follows stock 0.8.3 (no msm_best cutover). Loki additions are cfg-gated (`cuda`, `loki-backend-api`, `loki-cpu-budget`). Default-path MSM output is asserted equal to stock in `tests/backend_default_stock.rs:141` (`default_msm_equals_stock`).
- `src/plonk/keygen.rs`: `create_domain` returns `Result` only under `loki-harden`; default path is `Ok((domain, cs, config))`. `create_domain` is `pub(crate)` in both trees, so no public API change.
- `src/plonk/evaluation.rs`: `RotationIndex` type alias is gated on `loki-harden`; default path keeps `i32`.
- `src/plonk/prover.rs`: transcript-order diffs are `loki-profile` instrumentation only (`PhaseGuard`, `ProofGuard`); no absorb or squeeze order change found.
- `src/plonk/verifier.rs`: identical to stock 0.8.3 (not in the diff list).
- `src/plonk/vanishing/prover.rs`, `src/plonk/mod.rs`, `src/utils/arithmetic.rs`, `src/poly/domain.rs`, `src/poly/mod.rs`: domain/arithmetic helpers and tracing spans; not reviewed line by line (flagged unverified).

Default-feature parity is asserted at `tests/backend_default_stock.rs:47-70` (manifest default = `["committed-instances"]`, same as stock).

## 4. Loki-only Cargo features (all additive, all default-off)

From `Cargo.toml` diff against stock 0.8.3: `cuda-ntt`, `cuda`, `cuda-srs-registry`, `loki-profile`, `loki-multiopen`, `loki-cpu-budget`, `loki-harden`, `loki-backend-api`, `loki-sppark`, `loki-gpu-backend`, `loki-deterministic-lookup-order`. Default feature set unchanged. Note `loki-cpu-budget = ["blst/no-threads"]` changes the blst threading model and is relevant to WASM (compat-proof does not own the WASM gate; flag to luna-v1-3).

## 5. Branch and TODO map (from astra's kickoff)

- T02, T03, T04 map to compat-proof.
- Merged into main per branch list: w2-harness, w7-fuzz, w8-quint, w11-multiopen, w9-harden.
- Unmerged, all touch prover paths and have no recorded harness run: `loki/p3-srs`, `loki/p4-ntt`, `loki/p5-quotient`, `loki/p7-chain`, `loki/w10-gpu-quotient`.

## 6. Unverified items

- Equality of the ungated lookup/permutation rewrites with stock: asserted by the W2 harness at k10/14/17 for one seed set, not by a targeted randomized-size test.
- `par_sort` vs `sort` equivalence on lookup permuted input: argued, not tested.
- Vanishing, domain, arithmetic diffs not reviewed in full.
