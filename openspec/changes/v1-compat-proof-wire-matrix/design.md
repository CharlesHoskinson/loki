# Design

## Context

The present independent harness pins registry stock 0.8.3 alongside Loki's path package (`loki-harness/Cargo.toml:18-20`). It compares independent VK bytes and cross-verifies proofs at k=10/14/17. The separate k20/0.8.1 T04 gate is owned by `v1-compat-proof-stock-matrix`. The existing mixed committed/plain multi-proof transcript-order defect is recorded as stock behavior, with an expected counterexample and rejection (`LOKI-QUINT.md:120-166`). These are observed limits, not proposed support claims.

## Goals / Non-Goals

**Goals:**

- Establish a replayable, version-pinned wire and proof compatibility gate for stock 0.8.3.
- Make W2 instances, batches, k values, negative controls, and deterministic behavior explicit in one shared manifest.
- Give fuzz, Quint/Rust MBT, Lean, WASM/mobile, and docs owners a stable fixture/result contract.

**Non-Goals:**

- Change proof protocol ordering to repair T03. Such a change can alter stock-compatible transcript/proof bytes and requires a separately reviewed protocol/version decision.
- Add GPU behavior to the baseline gate, or own acceleration/performance budgets.
- Replace the API compatibility gate owned by compat-api.

## Decisions

1. **Keep stock 0.8.3 independent.** The W2 workspace keeps the unmodified registry dependency separate from the Loki path package. The supplementary 0.8.1 lane and its lockfile are owned by `v1-compat-proof-stock-matrix`.
2. **Use a canonical JSON Lines manifest and result stream.** Publish `loki-harness/fixtures/v1/manifest.jsonl`. Each row records `schema_version`, `fixture_id`, stock version/source pin, Loki revision, `circuit_id`, k, instance mode (`plain`, `committed`, `mixed`), proof count/batch shape, `serde_format`, transcript mode, lookup-placement order mode, worker schedule, features, `seed_id`, `expected_outcome`, transcript vector ID, and relative artifact paths plus SHA-256 digests. The bundle contains serialized SRS/verifier parameters, VK, PK, proof, and public inputs, plus named wrong-input, malformed/truncated, mutated, and appended variants. Transcript JSONL under `loki-harness/fixtures/v1/transcripts/<transcript_vector_id>.jsonl` records ordinal, phase, event kind, artifact digest/value bytes, challenge label, and challenge bytes; it never stores witness secrets. Result rows add command, target/runtime, per-backend outcomes, proof length/difference summary, lockfile digest, and log path. Fuzz, Quint/MBT, Lean, platform, and performance owners consume these identifiers.
3. **Separate equality from interoperability.** No-lookup fixed-seed cases are the byte-deterministic control. Transcript/challenge equality tests fix lookup-placement order and worker schedule. Lookup cases with randomized leftover placement compare and report proof bytes, but differences are not an acceptance equality criterion; they still must satisfy cross-verification and key identity. This follows the existing distinction in `loki-harness/README.md:31-60` and `loki-harness/src/lib.rs:148-153`.
4. **Preserve T03 as an explicit classification.** Keep all-plain, all-committed, one-proof mixed, and multi-proof mixed as distinct cases. Until a separately approved wire-compatible protocol decision, mixed multi-proof cases carry the expected stock-order mismatch/rejection outcome; do not mark them supported and do not hide their rows. Quint and Lean retain the counterexample.
5. **Keep platform execution CPU-only.** The manifest and cryptographic fixtures are target-neutral. WASM/mobile owners provide target runners, CI, budgets, and limitations; this change requires the same compatibility classification without an acceleration feature.
6. **Retain every release artifact.** Check in or archive machine-readable results, source/lock pins, commands and logs, with unrun cases explicitly labeled. Documentation names the artifact location and rerun commands.

## Risks / Trade-offs

- Lookup byte nondeterminism can create false alarms if proof-byte equality is treated as the compatibility oracle; cross-verification and classification remain authoritative.
- T03 is an inherited protocol-order defect. The supported matrix must not claim its mixed multi-proof cases pass until an explicit decision and compatible verification evidence exist.
- The current monolithic harness does not yet expose every matrix shape or target runner. Estimates are intentionally omitted until target owners confirm runner constraints.

## Migration Plan

Introduce the fixture/result schema and stock 0.8.3 W2 extensions additively. Do not alter default production features or wire behavior. First validate the manifest against existing 0.8.3 k10/14/17 evidence, then add instance/batch and negative cases and portable target runners. If a case regresses, block release and keep the last passing revision and logs; do not relax expected outcomes to recover a green gate.
