# haiku-v1-9 inventory: perf area (state as of 2026-10-25)

Repo: ~/repos/loki-proofs (main). Unverified items are marked UNVERIFIED. No builds were run.

## Exists
- Criterion benches inherited from stock, unchanged in Loki: benches/plonk.rs (groups plonk-keygen, plonk-prover, verifier), benches/commit_zk.rs (Blinder_poly), benches/zswap_output.rs (needs feature bench-internal). Registered in Cargo.toml:135-149; bench-internal feature at Cargo.toml:46; criterion 0.7 dev-dep at Cargo.toml:208-209. docs/modules/benches.md says these are not Loki timings.
- examples/cpu-bench.rs: fixed-seed synthetic CPU prove timing. Setup timed separately (lines 29-36); per-run timer is create_proof plus transcript finalize only (lines 41-54), so verify and prepare are excluded. Default 3 repeats, default k=16.
- examples/loki-profile.rs: phase profiler; asserts phase sum within 10% of total (line 78). Doc: LOKI-PROFILE.md:16-36, 151-175.
- loki-harness/src/bin/bench.rs: JSON records for loki and stock at k=10,14,17 via cross_verify (lib.rs:167). Timings in loki-harness/src/backend.rs:222-252 (keygen, prove, verify; Instant-based).
- loki-harness/README.md:60-70: documents timer boundaries, states ONE cold sample per backend, Loki first, and that the timings do not establish a speed difference.
- scripts/w9-cpu-matrix.sh: repeated (3-sample) CPU matrix with threads 20/4, alternating order for k=16, flock on heavy.lock, sha256 of binaries, sched affinity logged. Hardcodes /home/charles/repos/loki-wt/w9-harden (line 4).
- loki-harness/run.sh: 292 s timeout gate, RAYON_NUM_THREADS=1.
- Ignored manual timers: src/poly/mod.rs:358-359 (time_rational_conversion_k16), src/plonk/lookup/prover.rs:470-471 (time_lookup_preparation_k16); CUDA benches src/poly/kzg/msm/cuda/tests.rs:107-108 (gpu_benchmark), src/poly/domain/gpu_tests.rs:269-270 (gpu_ntt_benchmark). CSVs: LOKI-GPU-NTT-BENCH.csv, LOKI-GPU-NTT-EQUALITY.csv.
- Wiki (not in repo): ~/repos/Moriarty-factory/wiki-llm/loki/benchmark/{README.md,SUITE.md,DEBATE.md}. README states the closing condition and names RESULTS.md as the table file.
- Owner TODO: TODO.md:27 T06 (recursion-chain perf, matched stock/Loki); TODO.md:140 T34 (W13 latency with registry); TODO.md:168 T41 (publish raw samples and scripts at stable paths); TODO.md:108 (memory peaks / full chain).

## Missing
- RESULTS.md: does NOT exist (`~/repos/Moriarty-factory/wiki-llm/loki/benchmark/` has no RESULTS.md). Closing condition is therefore unmet.
- Any stock-vs-Loki head-to-head bench that is a stock-only, independently resolved build. loki-harness mixes both sources in one binary, and SUITE.md (Statistics/build sections) forbids timing that as the stock baseline. tests/stock-profile-verifier (LOKI-PROFILE.md:151-159) is the only independent stock crate; it is a verifier, not a prover-timing driver.
- bench-driver and stock-verifier named in SUITE.md:33 do not exist yet (SUITE says so explicitly).
- No recursion-chain bench (leaf -> parent -> decider). Sherlock fixtures live in ~/repos/Moriarty-sherlock, not loki-proofs. P7 `bench/` is planned (SUITE.md:33), not present in ~/repos/loki-wt/p7-chain (UNVERIFIED, not inspected).
- No CI: no .github/ directory in loki-proofs or ~/repos/loki-wt/v1. No criterion regression tracking, no baseline storage, no perf gate in CI.
- No stable-path raw sample store (T41). Raw CSVs exist only for NTT.
- Hardware matrix: none in repo; WSL2 host (20 CPUs, 90 GiB) per SUITE.md:29.
- WASM and mobile timing: none (perf area owns budget evidence for WASM/mobile per lab brief, nothing exists yet).

## Gaps and risks
- Single cold sample, Loki first (loki-harness README:68): order bias and n=1. Not publishable.
- Criterion benches in repo are stock inherited; criterion's noise on shared hosts is noted in docs/modules/benches.md but no protocol is enforced.
- Timers exclude verify in cpu-bench; loki-harness includes verify. Two different endpoints in the repo; neither is the recursion-chain endpoint SUITE.md:69-71 requires.
- scripts/w9-cpu-matrix.sh hardcodes a worktree path; not portable to CI or other hosts.
- Sherlock timers in ~/repos/Moriarty-sherlock tests print negative-control or artifact-write time (SUITE.md:12-14, citing parent.rs:974-1028 and sherlock_terminal.rs:293-350). Do not reuse as endpoints.

## Verification for this inventory
- All file:line references above were read in this session. Line numbers for ~/repos/Moriarty-factory and ~/repos/Moriarty-sherlock are cited from SUITE.md and were not re-checked by haiku-v1-9.

## Workstream map (added after lookup)
- W3 CPU work: LOKI-CPU.md (header "CPU work (W3)"; measured at 1db6ab8, lines 1-3).
- W11 root-product multiopen: LOKI-MULTIOPEN.md; README.md:53 gives medians of three at k17 (multiopen 12.83% / 19.00% lower at 4 / 20 threads; create_proof 2.74% lower / 0.11% higher; end-to-end "not established").
- W12 PK disk cache: fresh-vs-cached byte gate, cited in docs/review/luna-r9.md:52. Its timing evidence is not located yet (UNVERIFIED).
