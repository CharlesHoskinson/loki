# TASK perf-ci inventory and research: haiku-v1-9

## Repo state
- No CI of any kind for perf: no .github/ in ~/repos/loki-proofs or ~/repos/loki-wt/v1. Benchmarks are run by hand (scripts/w9-cpu-matrix.sh, loki-harness/run.sh with a 292 s timeout gate).
- scripts/w9-cpu-matrix.sh hardcodes /home/charles/repos/loki-wt/w9-harden (line 4), so it is not portable to a runner.
- Sample counts: cpu-bench default 3 repeats (examples/cpu-bench.rs), w9 matrix 3 samples with alternating order at k16, harness 1 sample.

## External practice (sources)
- Criterion.rs analysis: outliers classified by modified Tukey fences and kept, not dropped; change detected by bootstrap plus t-test; noise threshold is "configurable" with example +-1% (no default stated in the page I read). https://github.com/bheisler/criterion.rs/blob/27642b476837753cbb539f269fbbcbefa815bf00/book/src/analysis.md . A 2026 arXiv paper quotes a +-2% default (https://arxiv.org/pdf/2607.02744), not verified against source.
- Bencher and similar tools: baseline-based statistical boundaries, relative thresholds, or change-point detection; no independent head-to-head evaluation found. https://bencher.dev/docs/how-to/track-benchmarks
- benchdiff-rs: Welch's t-test against a saved baseline; notes fixed thresholds are flaky when noise is near the gate. https://download.plaud.ai/JSLEEKR/benchdiff-rs (low-authority source; UNVERIFIED)
- Gate on a low-noise metric where available: a single unreviewed project (tak) reports instruction counts stable across contention while wall time moved about 150%. https://tak.jdx.dev/guide/methodology (pre-v1, unreviewed; treat as anecdote)

## Recommendations for Loki
1. Use criterion (or equivalent) only for micro kernels; gate end-to-end runs on paired, interleaved samples with a Welch or bootstrap test against a pinned stock 0.8.3 build on the same runner.
2. Do not gate wall-time absolute thresholds on shared runners. Report, do not gate, unless the runner is dedicated and its noise floor is measured first.
3. Record the noise floor per runner (repeat the same binary N times) before setting any threshold.
4. Add instruction-count or CPU-seconds gating only after validating it against wall time on the target hardware (UNVERIFIED for Loki).
