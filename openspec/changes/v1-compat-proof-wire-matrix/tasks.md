# Tasks

## 1. Shared fixture and stock 0.8.3 oracle

- [ ] 1.1 Define `loki-harness/fixtures/v1/manifest.jsonl` and transcript vectors at `loki-harness/fixtures/v1/transcripts/` with stock pin, Loki revision, circuit/k, instance/batch, `SerdeFormat`, lookup-order mode, worker schedule, feature/seed, expected outcome, vector ID, artifact paths and SHA-256; verify schema validation with positive/negative fixture corpus and document the exact shared fields in the harness README.
- [ ] 1.2 Confirm the stock 0.8.3 registry crate remains unmodified and independent from the Loki path package; verify the locked dependency graph contains both exact sources and record its lockfile digest and oracle command in the README.

## 2. Proof and serialization interoperability

- [ ] 2.1 Extend differential tests to stock-written/Loki-read and Loki-written/stock-read SRS, VK, PK, verifier parameters, and each `SerdeFormat` (`Processed`, `RawBytes`, `RawBytesUnchecked`) through existing public APIs; verify serial roundtrips, exact VK equality, and checked malformed/truncated cases while recording unchecked stock behavior; document supported formats and API boundaries.
- [ ] 2.2 Record stock-generated transcript event and challenge vectors for supported circuits and instance layouts under matched lookup-order/worker schedules; verify Loki vector equality with `transcript_vectors_match_stock`, the named Quint transcript model, and Rust MBT replay, and document vector provenance and regeneration commands.
- [ ] 2.3 Extend proof negative controls to wrong instances, empty, mutated, truncated, and appended proofs and unsatisfied witnesses, with both MockProvers agreeing on witness validity; verify using the serial differential negative-control test and retain corpus seeds/logs.

## 3. W2 matrix, determinism, and version coverage

- [ ] 3.1 Cover each declared circuit family at k=10/14/17 with plain and committed instances, one-proof and supported multi-proof shapes; verify each JSONL result row and bidirectional 0.8.3 acceptance with `./loki-harness/run.sh 10 14 17` plus the committed/plain matrix command; publish supported and unsupported classifications in the README.
- [ ] 3.2 Add all-plain, all-committed, single mixed, and mixed committed/plain multi-proof T03 cases; verify the recorded stock/Loki classifications and Quint counterexample via `python3 quint/check_instances.py` and Rust MBT replay; document the known mixed multi-proof rejection and require an approved protocol decision before changing it.
- [ ] 3.3 Make fixed-seed, one-worker no-lookup proof equality mandatory while lookup proof differences remain diagnostic and cross-verification/VK identity remain mandatory; verify normal and strict lookup modes with `RAYON_NUM_THREADS=1 ./loki-harness/run.sh 10 14 17` and `LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10`; document the randomized stock lookup placement rationale.
- [ ] 3.4 Differentially qualify default-path permutation and lookup product rewrites against stock 0.8.3 over randomized sizes, fixed seeds, and worker counts declared by the compat-api/perf gates; verify arithmetic outputs, transcript effects, and bidirectional proof acceptance with retained result rows per size/seed/worker, building on `src/plonk/permutation/prover.rs:107-145` and `src/plonk/lookup/prover.rs:198-235`. Do not claim current qualification until results exist.

## 4. QA integrations and release evidence

- [ ] 4.1 Feed W2 manifest cases to differential fuzz targets for proof/key parsing, instances, and transcript mutations; verify with the named differential fuzz campaign against stock 0.8.3 and retain minimized seeds with manifest case IDs.
- [ ] 4.2 Model supported/unsupported instance and batch order plus transcript/challenge events in Quint and replay the same case IDs through Rust MBT; verify `python3 quint/check_instances.py` and the release Quint/MBT replay command, retaining expected T03 counterexample output.
- [ ] 4.3 Map the transcript/proof equality contract and T03 counterexample into Lean with explicit backend-refinement assumptions; verify the Lean release check at the pinned release commit has no `sorry` and no new axioms, and link each theorem/counterexample to W2 case IDs.
- [ ] 4.4 Integrate the CPU-only fixture runner into WASM browser/Node single-threaded and wasm-threads, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `aarch64-linux-android` lanes; verify unmodified stock 0.8.3 and Loki both execute on each target in both proof directions with `wasm:stock-compat` and the four commands `mobile-compat/run.sh --platform ios --runtime simulator`, `mobile-compat/run.sh --platform ios --runtime device`, `mobile-compat/run.sh --platform android --runtime emulator`, and `mobile-compat/run.sh --platform android --runtime device`; retain target/runtime/case records and no GPU features, and document platform-owned budgets/limits.
- [ ] 4.5 Publish the supported matrix, stock 0.8.3 pins, determinism policy, T03 limitation, negative controls, default-rewrite differential results, commands, and verified/unverified status; verify every documented command against the archived release manifest and complete `openspec validate v1-compat-proof-wire-matrix --strict`.
