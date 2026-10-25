# REVIEW v1-perf-benchmarks (haiku-v1-9)

`openspec validate v1-perf-benchmarks --strict`: PASS ("Change 'v1-perf-benchmarks' is valid").

## Findings
1. Stock-compat gap (high): spec and design never state the Sherlock/ZSWAP common `midnight-proofs/circuit-params` exception that the wiki SUITE.md "Configurations, CPU budgets and builds" section (lines 54-97) requires on every arm. The chain stock arm therefore is not "default features" as the spec's "default-compatible rows use stock defaults" implies. Add an explicit feature-family rule and disclose it per row.
2. Stock source independence (high): design Decision 2 says "build separately", but does not forbid the existing loki-harness mixed-source binary as the baseline. loki-harness/README.md:15-19 and SUITE.md say the mixed binary cannot be the stock baseline. Spec should say so and name the stock-only workspace.
3. Non-existent tooling is named as if present in WHEN clauses: `loki-harness --bin perf-bench`, `--bin perf-chain`, `scripts/validate-perf-results.py`, `scripts/check-perf-regression.py`, `scripts/validate-perf-publication.py`, `loki-harness/tests/perf_records`. Current loki-harness/src/bin has only bench.rs and gate.rs (loki-harness/src/bin), tests/ has only differential.rs. Tasks create these, which is fine, but spec scenarios must be marked future or tied to tasks.
4. W12 requirement is unsatisfiable as written (high): spec "CPU phase work" requires W12 phases now; design and task 2.2 say W12 phase map is an unlocated prerequisite. The W12 mapping is ambiguous: docs/review/luna-r9.md:52 means a PK disk-cache byte gate; ~/repos/loki-wt/w12-a3-residual (HEAD e50b775c3) is an A3 residual worktree. Either name the owner ref or make the W12 scenario conditional.
5. Reconciliation tolerance inconsistent: spec says phase-sum within 2% of the prover interval; the existing profiler asserts 10% (examples/loki-profile.rs:78). Either justify 2% with a measured run or cite the 10% precedent. The design also allows an explicit "unprofiled overhead" bucket, which makes the 2% check ambiguous.
6. Mobile/WASM budgets are informational only (spec "Report secondary CPU and accelerator targets"; design Decision 5). astra-v1 review (#lokiv1) asked for mandatory runtime/memory/size baselines on those targets. Conflict to resolve with astra; luna-v1-4 proposal also wants numeric budgets. Keep as CONFLICT.
7. Publication path (spec Requirement 4, task 4.1): RESULTS.md only at wiki-llm/loki/benchmark/RESULTS.md, outside the repo. astra asked for a stable repo mirror. Add a mirror path, e.g. docs/benchmarks/RESULTS.md with a copy/checksum rule. Validator scope should cover both.
8. Citation accuracy (low): docs/modules/benches.md:15-23 is the "Invariants and pitfalls" block (file is 23 lines), so it is acceptable but should be "15-23" only if the citation is about zswap/criterion noise; docs/GPU.md:516-516 is a single line citing an unrelated owner-priority sentence; fix to the actual measurement line. Verified existing: TODO.md:27-29 (T06), TODO.md:167-169 (T41), LOKI-CPU.md:105-124, LOKI-MULTIOPEN.md:76-85, README.md:46-63, docs/GPU.md:431-455, docs/PLATFORMS.md:1-40.
9. Runner ownership overlap (medium): task 3.1 provisions a runner; the repo has no .github/ workflows (checked loki-proofs and loki-wt/v1), and docs-release owns CI aggregation. Name who provisions and where the job lives.
10. Sample-size and statistics (medium): spec says ">= 7 paired samples" and "paired 95% bootstrap"; the SUITE uses seven or ten independent process blocks with cold/warm split. Spec does not require cold and warm populations, process restarts, or 4- and 20-CPU masks. Add them, or cite the SUITE as the superseding protocol.
11. Chain endpoint (medium): spec names "total chain wall time" but does not define the interval (process-cold vs warm, SRS/VK/PK setup inside or outside). SUITE.md "Acceptance and operational boundaries" section defines it; spec should cite it.
12. Verification (low): spec requires stock acceptance of every output proof (good) but does not require checking the canonical public inputs, transcript consumption, or final KZG pairing (SUITE.md "Acceptance and operational boundaries" section, lines 98-117). Add.
13. Dependencies (low): compat-proof fixture ID scheme is referenced but not named; the spec should say which fixture-set IDs exist (none yet).

Unverified: loki-harness/tests/perf_records and all future scripts (absent by design). Line numbers in the wiki (SUITE.md) were not re-checked against the pinned revision.

## Re-review of updated draft (second pass, same date)
`openspec validate v1-perf-benchmarks --strict`: PASS.
Resolved from pass 1: stock-only/Loki-only timed executables (design D2); cold/warm split; ten CHAIN blocks and five-run A3 pilot (spec, tasks 3.2); median/IQR/min/max instead of small-n bootstrap; W12 no longer called a CPU phase (design D3, D6).

Remaining:
R1. Host citation wrong: design.md cites `wiki-llm/loki/benchmark/SUITE.md:68,157` for the reserved "homeoffice" host (Intel Core Ultra 9 285K, RTX 5090). SUITE.md contains no "homeoffice" and no "285K"; line 157 mentions only "5090 sm_120 and CUDA 13". Cite only what the file says, or find the host source (~/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/luna-3/X2-result.md and luna-5/X10A-result.md mention homeoffice).
R2. Roadmap coverage missing: ROADMAP-V1.md:152 (T06), :180 (T34 W13 production chain), :187 (T41), :235 (T89 BLST wasm baseline) map to this change. Grep finds no W13/T34 and no T89 or T80 in the change. T80 (wasm chain vs CPU, PLATFORMS.md:358) requested by luna-v1-3 for co-ownership. Add requirements or record a CONFLICT.
R3. Sample-count mismatch: spec line 13 WHEN uses `--samples 7` for the matched run, while design D4 and spec line 35 use the suite's ten CHAIN blocks and five A3 pilots. State which applies to which row (or cite the suite's seven-block A3/DEC rule).
R4. Arm count: SUITE.md uses three arms per block (S0, L0, G0) and six balanced permutations (SUITE line 124). The change describes stock vs Loki (two arms) and never names G0 or the GPU-arm order rule. Name the arms and the permutation rule.
R5. Missing stock-compat rule for Sherlock/ZSWAP: `midnight-proofs/circuit-params` is required on every arm (wiki SUITE "Configurations, CPU budgets and builds"). Not in the spec.
R6. Spec WHEN clauses name `scripts/perf/run-matched.py`, `scripts/validate-perf-results.py`, `scripts/check-perf-regression.py`, `scripts/validate-perf-publication.py`, plus loki-harness bins `perf-bench`/`perf-chain`. None exist yet (scripts/ has loki-dag, train.sh, verify-main.sh, verify-w9.sh, w9-cpu-matrix.sh, w9-host-cargo.py, w9-gpu-gate/, tests/). Mark as future deliverables.
R7. Mobile/WASM budgets: design D5 keeps them informational. ROADMAP M5 (line 73) requires target budgets to pass, and astra-v1 asked for mandatory baselines. Keep as CONFLICT for astra.
R8. Repo mirror for RESULTS.md absent (astra asked). Only the wiki path is named.
R9. Runner ownership: tasks 3.1 "provision the reserved native host" overlaps docs-release CI ownership (ROADMAP line 73, no .github/ in repo).
R10. Chain interval text: spec scenario names "observed chain interval" but does not define its start/stop (SUITE "Acceptance and operational boundaries", chain_ms definition).

Unverified: all `wiki-llm/loki/benchmark/SUITE.md` line numbers were checked against the file as of this pass. The homeoffice host is not in SUITE.md.

## Final pass (third review, after luna-v1-9 TASK perf-final-review)
`openspec validate v1-perf-benchmarks --strict`: PASS.
Closed since pass 2: circuit-params family rule (spec req 1); four-stage chain with receipts/pairing/transcript (req 2); ten cold->warm blocks and five-run A3 pilot (req 4); W13 T34 scenario (req 5); RESULTS mirror at docs/benchmarks/RESULTS.md (req 6); numeric target budgets as release blockers (req 4 scenarios).
Still open:
F1. T80 (ROADMAP-V1.md:251, CPU-wasm chain baseline) and T89 (ROADMAP-V1.md:260, BLST portable/SIMD128/Rayon baseline) are mapped to this change by roadmap R2 but are absent from proposal, design, tasks and spec (grep count 0). Add a scenario per ID or record a CONFLICT with luna-v1-3 (who asked for co-ownership).
F2. design.md:23 still cites SUITE.md:68,157 for the "reserved homeoffice host (Intel Core Ultra 9 285K, RTX 5090/CUDA 13.4.3)". SUITE.md has no "homeoffice" and no "285K"; only "5090 sm_120 and CUDA 13" at line 157. Cite the actual source or drop the 285K detail.
F3. Future commands (scripts/perf/*, scripts/check-perf-regression.py, scripts/validate-perf-*.py, loki-harness bins) are now labeled as planned in the spec Purpose; acceptable.
