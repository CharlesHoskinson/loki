# cp-code: proof and serialization inventory (haiku-v1-2)

Read-only. No commands were run beyond file reads, diffs and agentbus. Per the lab rules (no proofs, no `cargo test`), the only verified state is the recorded 2026-10-24 W2 run described in `loki-harness/README.md`.

## Exact commands (from `loki-harness/README.md:21-30` and `loki-harness/run.sh`)

- `./loki-harness/run.sh` (wraps `cargo run --release --locked --bin gate` under `timeout 292s`, `RAYON_NUM_THREADS=1`, `CARGO_BUILD_JOBS=4`).
- `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml -- --test-threads=1 --nocapture`
- `... -- --ignored --test-threads=1 --nocapture` for k20.
- `... --bin bench -- 10 14 17` for JSON Lines timings.
- `LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10` is expected to fail on the lookup byte difference.

## Where the compatibility logic lives

- Compare and gates: `loki-harness/src/lib.rs:88-172` (`compare<LOOKUPS>`), `lib.rs:174-197` (`determinism`, `negative_controls`).
- Backend prove/verify, both sides: `loki-harness/src/backend.rs:198-304` (`setup`, `roundtrip_params`, `prove`, `verify`), `backend.rs:306-324` (MockProver mutants). Macro instantiated as `backend!(fork, loki)` and `backend!(reference, stock)`.
- Cross-side integration tests: `loki-harness/tests/differential.rs:1-32`.
- Stock-only verifier check: `tests-stock/tests/verify.rs:1-57` (reads `target/cpu-fixture`, uses `SerdeFormat::Processed`, flips the last proof byte to confirm rejection).
- Shared fixture and shapes: `tests/support/stock_fixture.rs:60-340` (`Fixture<SHAPE>`, `NO_LOOKUP`, `SINGLE_LEFTOVER`, `MANY_LEFTOVERS`), `cross_verify` at `stock_fixture.rs:300-340` with wrong-PI, empty, truncated, trailing and bit-flip rejection.
- Default-build parity: `tests/backend_default_stock.rs:38-239`.

## Verified state (recorded, not re-run)

- k10, k14, k17 cross-verify in both directions; VK bytes equal; SRS RawBytes roundtrip equal (`loki-harness/README.md`, "Recorded validation").
- Lookup proofs: 1,280 B, first difference at offset 192 (stock HashMap order). No-lookup control: 1,136 B, byte-identical.
- No k20 run. No 0.8.1 run. (T04, `TODO.md:19-21`.)

## Gaps (not exercised by any recorded test)

- PK serialization is never read back cross-side.
- Serde formats: only `RawBytes` (harness) and `Processed` (`tests-stock/tests/verify.rs:30`). `RawBytesUnchecked` is not used anywhere.
- Transcript hash: only `blake2b_simd::State`. Stock's `Blake2b256` (`midnight-proofs-0.8.3/src/transcript/implementors.rs:41-45`, sampling at `:191`) has no cross test.
- Committed instances: harness and fixture always pass committed `&[&[]]`, so nb_committed = 0 (`backend.rs`, `stock_fixture.rs:197-240`).
- Batch shape: single circuit, single proof only.
- Mixed committed/plain multi-proof shape: stock rejects its own proof (T03, `LOKI-QUINT.md:14-15,124-125,161-163`); not in the harness.

## Loki prover diffs touching proof bytes (see inventory.md section 3)

- `src/plonk/lookup/prover.rs` and `src/plonk/permutation/prover.rs`: ungated default-path rewrites (`prefix_product`, `batch_invert_parallel`, `par_sort`). Equality with stock is asserted only by the W2 harness at k10/14/17.
- Opt-in only (intentional wire change when enabled): `loki-deterministic-lookup-order`, `loki-multiopen`.

## Unverified

- Equivalence of the ungated rewrites under randomized sizes (no targeted test found).
- Vanishing, domain and arithmetic diffs not reviewed line by line.
