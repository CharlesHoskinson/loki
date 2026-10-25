# Quint QA inventory (area 7, haiku-v1-7)

Scope: current state of Quint models, MBT/replay, and gates in loki-proofs (main `eaf09d8`). Read-only; no production edits. Unverified items are marked.

## 1. Top-level models and gates (quint/)

| Model | Test file | Safety invariant (file:line) | Witnesses | Source correspondence |
|---|---|---|---|---|
| transcript.qnt (171 lines) | transcript_test.qnt (6 defs) | `safety` line 116 (orderedPrefix, absorbedSequence, challengesBound) | 5 in check.py | prover.rs compute_instances:449, compute_trace:57, finalise_proof:266 |
| gpu_dispatch.qnt (126) | gpu_dispatch_test.qnt | `safety` line 119 (resultEqualsCpu, staleNeverUsed, fallbackTotal) | 5 | no GPU kernel run; W4 replay only |
| pk_cache.qnt (85) | pk_cache_test.qnt | `safety` line 73 (bindingCorrect, keygenAtMostOnce, noPartialUse) | 5 | W6 cache, recl9 commit abe9701 (fenrir-loki) |
| scheduler.qnt (83) | scheduler_test.qnt | `safety` line 72 (vramBudget, slotLimit, noLostOrDuplicated, checkpointIntegrity) | 5 | no Rust scheduler exists; BUDGET=10 abstract units (line 7) |
| mutations.qnt (62) | none | 5 mutants, each `mutation_*` main | n/a | not run by any script (manual command in LOKI-QUINT.md:104-110) |
| transcript.qnt `transcript_instances` module (lines ~130-170) | transcript_instances_test.qnt | `mismatchExactlyMixedBatch`, `proverEqualsVerifier` (expected to FAIL, exit 1) | allPlain, allCommitted, singleMixed, multiMixed | driven by quint/check_instances.py |

All five top-level .qnt files plus the 4 `*_test.qnt` files typecheck with quint 0.32.0 (`quint typecheck`, exit silent; run 2026-10-25). Not run: `quint run` campaigns (too heavy for this lab).

## 2. Gate wiring (what actually runs)

- `quint/check.py` (64 lines). MODELS dict lines 12-17: transcript 180 steps, gpu_dispatch 120, pk_cache 120, scheduler 1000. Per model: typecheck, `quint test ... --match '.*Test$'`, `quint run --backend rust --n-threads 4 --seed 20261024 --max-samples 10000 --invariant safety --witnesses ...`. Missing witness aborts (line ~54). Then calls `quint/check_instances.py`.
- check.py does NOT run: mutations.qnt, the Rust MBT crate, Apalache, or the q1/q2/q4/q5 campaigns.
- Rust MBT: `quint/mbt/` (crate `loki-quint-mbt`, Cargo.toml line 1-20). Command in quint/mbt/README.md: `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features w4-source,w6-source -- --nocapture --test-threads=1`. Manual only.
- Repo has no `.github/` directory, and no CI workflow file. `grep -rn quint` over scripts/ and tests/ found no gate. Current Quint gating is therefore manual (`python3 quint/check.py`) with results recorded in quint/results.json (JSON recorded manually).
- Ad hoc campaign directories with their own runners: quint/q1 (check_q1.py, results.json), quint/q2 (check.py, REPORT.md; includes Apalache), quint/q4 (check.py, srs_w13 / srs_p3, REPORT.md, evidence/apalache-p3.log), quint/q5 (check.py, argcommit / lookup, results.json). LOKI-QUINT.md (W8) does not cover them; their status is UNVERIFIED in this inventory.

## 3. Rust replay (quint/mbt/src)

- transcript_driver.rs: `random_real_transcripts` line 181; step impl line 151. Replays 24 traces, 1,022 operations (quint/results.json).
- pk_driver.rs: step line 135; test `public_cache_api_reuses_key` line 201.
- w4_driver.rs: test `concurrent_host_calls_match_cpu` line 264. Pinned W4 commit fdef4c4 exists in this repo (verified by git cat-file).
- instance_order.rs: test `diagnose_stock_mixed_instance_order` line 16 (expected-failure diagnostic; stock rejects mixed multi-proof cases).
- build.rs line 64-67: reads recl9 source from `LOKI_RECL9_REPO`, default hard-coded `/home/charles/repos/fenrir-loki` at commit abe9701 (verified present). Not hermetic for CI.
- Cargo.toml line 20: `quint-connect` git dep pinned at rev 4f018f5 (requires network at build time).
- No Rust driver exists for: scheduler (none in source), NTT (T31 open), mutations.

## 4. Source-side branch points that Quint is supposed to cover (verify with file:line)

- Lookup leftover order: src/plonk/lookup/prover.rs:393-396 (HashMap build), :425-429 (default stock HashMap iteration vs `loki-deterministic-lookup-order` feature). The feature is declared Cargo.toml:74 and changes proof bytes relative to stock.
- Committed-instance split: src/plonk/prover.rs:19, :32, :57-69, :83-84 (`committed-instances` feature; nb_committed_instances=0 when off).
- Verifier parse: src/plonk/verifier.rs:16 parse_trace; :44-46, :208, :269-272 (committed columns handled before plain).
- GPU routing/fallback: loki-gpu-backend feature Cargo.toml:69-72; not covered by a Rust replay of the MSM dispatch beyond W4 CPU-backed FFI.

## 5. Known gaps (from LOKI-QUINT.md and this scan)

1. No CI gate for any Quint model, MBT replay, or mutation. (no .github/)
2. Scheduler: no Rust implementation; Apalache liveness fails: `Handling fairness is not supported yet!` (LOKI-QUINT.md:280, T29 open).
3. Stock mixed-instance multi-proof rejection (CRITICAL, LOKI-QUINT.md "Implementation mismatches" item 1; T03 open). Expected-fail controls exist but are not a release-blocking gate.
4. Fixed-RNG byte oracle not universal (lookup-rich traces unequal; item 2; default HashMap order).
5. W5 NTT fail-stop vs total fallback; NTT not modeled in a live replay (item 3; T31).
6. Transcript fixtures cover only 2 of 3 advice phases and no arbitrary query rotations (T30).
7. Mutations run manually only; not wired to check.py.
8. Models sample safety (10,000 samples, bounded steps), not proofs; witnesses reach-only.
9. Replay depends on non-hermetic external repo (fenrir-loki) and a network git dep.
10. Quint 0.32.0 pinned in docs; check.py does not pin or verify the quint version at run time (UNVERIFIED).

Notes path: /home/charles/repos/loki-wt/v1/docs/v1-research/haiku-v1-7/Q7-CODE.md (same content).
