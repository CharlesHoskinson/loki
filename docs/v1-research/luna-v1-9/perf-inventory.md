# Perf area inventory and research

Date: 2026-10-25. Scope: read-only inspection of the Loki tree and benchmark documentation. No benchmark, test, or build was run for this roadmap work.

## Current state observed

- `TODO.md:27-29` leaves T06 open and defines the needed gate: repeated matched stock/Loki leaf-parent-decider runs with fixed features, workers, hardware, and shapes; sample counts, dispersion, independent verification, and all CPU phases.
- `README.md:46-63` has a measured-results table but expressly says the W10 A3 comparison is one matched sample, branch-only, and not a recursion-chain result; it calls out end-to-end recursion-chain latency as the target.
- `docs/GPU.md:431-455` records one-sample k20 Sherlock parent prove-plus-verify values and states that they are not a recursion-chain result. `docs/GPU.md:516` says the owner prioritizes chain latency over raw kernel speed.
- `LOKI-CPU.md:105-124` describes W3 phase measurements, three samples per stage, no end-to-end speedup claim, and no production recursion/k20 performance claim.
- `LOKI-MULTIOPEN.md:76-85` documents W11's synthetic `ProfileCircuit`, host/compiler/feature setup, and non-Sherlock limitation. `LOKI-MULTIOPEN.md:99-109` records a sequential stock/feature command pattern and proof comparison.
- `docs/modules/benches.md:1-23` says Criterion targets are inherited from stock, Loki measurements use separate harnesses/examples, shared-host timings are noisy, three-sample medians have been used, and benchmark targets have no tests.
- `loki-harness/README.md:1-20` and `loki-harness/src/bin/bench.rs:1-20` describe a separate stock/Loki synthetic benchmark, not a full Sherlock chain runner.
- More detailed scout inventory: `loki-harness/README.md:64-69` says its current measurements are one cold sample per backend and run Loki first. `examples/cpu-bench.rs:29-54` times setup separately and times proof creation plus transcript finalization, not verification. `examples/loki-profile.rs:76-78` checks profiled phase sum against its total with a 10% tolerance.
- `TODO.md:167-169` leaves T41 open for portability of historical performance evidence and raw scripts, including W10 RESULTS artifacts.
- `scripts/w9-cpu-matrix.sh` provides a historical repeated three-sample CPU matrix, but the scout found its path hardcoded to a W9 worktree (`scripts/w9-cpu-matrix.sh:4`); no stable benchmark CI or baseline store is present.
- The Moriarty benchmark folder has README/SUITE/DEBATE but no `RESULTS.md`; no independent stock prover timing driver is present. The stock harness is a verifier/cross-check, not proof of a clean stock timing path. The scout reports these from the referenced SUITE material; the workspace is read-only for this author.
- W12 maps to the A3 residual-workstream in `/home/charles/repos/loki-wt/w12-a3-residual/LOKI-A3-RESIDUAL.md:1-17`; it makes no Loki-source changes. The W12 note contains CPU and GPU-feature phase timings (`:73-90`). It also describes a PK disk probe, but reports disk loading loses to setup and is not adopted (`:19-27`). `docs/review/luna-r9.md:52` refers specifically to the PK-cache identity proof obligation, and `docs/ARCHITECTURE.md:343` still labels cross-chain cache savings an estimate. Keep W12 A3 rows separately pinned and experimental; never present test-only disk load as Loki default performance.
- The accepted benchmark execution specification is `wiki-llm/loki/benchmark/SUITE.md`, not a results report. Its primary endpoint is the full serial Sherlock chain through stock verification (`:14`), with independent stock and Loki workspaces (`:70-72`), four proof stages (`:34-43`), cold/warm populations and N=4/20 budgets (`:68`, `:114-124`), and a five-run A3 noise pilot (`:159`). The suite rejects small-sample bootstrap significance claims and prescribes median/IQR/min/max plus every observation (`:126-128`). `RESULTS.md` remains absent per its README closing condition.
- Existing code does not expose evidence that a stable native performance CI lane or a mobile/WASM perf matrix currently exists. Cross-target platform owners should own compatibility and budget gates; perf should consume their valid workload IDs and report results.

## Design/release recommendations

- Treat the accepted `wiki-llm/loki/benchmark/SUITE.md` as the campaign methodology source of truth. Its low-n chain policy deliberately uses all raw observations and median/IQR/min/max without bootstrap significance claims; Criterion's bootstrap guidance remains applicable to microbenchmarks, not a reason to override that accepted campaign policy.
- Keep stock `midnight-proofs` 0.8.3 unmodified in its own binary/dependency path and compare against the same immutable fixture, features, worker count, and proof shape.
- Time proof intervals separately from setup and verification, report leaf/parent/decider plus end-to-end chain wall time, and independently verify outputs with stock.
- Alternate paired runs, retain raw samples and metadata, and report dispersion/confidence rather than a bare average. Only a stable pinned runner should gate regressions; shared and heterogeneous hosts should report evidence without gating.
- Make kernel time, transfers, CPU phases, verification, and whole-chain latency distinct columns. Do not infer chain speed from W3/W11 phase gains or W10 one-sample results.
- Publish exact commands, source/lockfile/feature pins, host/compiler details, warmups, samples, variance, raw data, and missing historical inputs. Clearly label historical, branch-only, single-sample, synthetic, and unreproduced rows.
- Perf owns methodology/statistical thresholds and result schema. Platform owners own device matrix and numeric memory/size/time budgets; no phone data was observed, so desktop numbers cannot supply mobile budgets. Astra assigns the host, docs-release wires CI, and perf owns the run manifest/controller contract.

## Outside best practice sources

- Criterion analysis documents sample-based measurement and bootstrap confidence intervals: <https://bheisler.github.io/criterion.rs/book/analysis.html>
- Criterion crate states its goal of statistical confidence in distinguishing improvements/regressions from noise: <https://docs.rs/criterion/latest/criterion/>
- Google Benchmark documents repetitions, random interleaving, manual timing, and memory reporting: <https://github.com/google/benchmark/blob/main/docs/user_guide.md>
- Google Benchmark's random-interleaving guidance gives a concrete mechanism for reducing order effects: <https://github.com/google/benchmark/blob/main/docs/random_interleaving.md>
- Journal article on reproducible benchmarking describes controlling data, code, and environment and retaining metadata such as hardware: <https://pmc.ncbi.nlm.nih.gov/articles/PMC12614554/>
- Principles for automated and reproducible benchmarking (peer-reviewed conference paper/preprint): <https://centaur.reading.ac.uk/114121/1/114121%20VoR.pdf>

These are methodological references, not evidence that Loki currently implements or passes the proposed gates.
