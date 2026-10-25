# Spec Delta

## Purpose

Defines reproducible evidence for comparing Loki with stock `midnight-proofs` 0.8.3 and for measuring the complete Sherlock recursion chain. It makes performance claims traceable to matched inputs, independent verification, and published raw measurements.

Commands named in scenarios are required v1 deliverables created by this change's tasks; they do not exist in the inspected source tree yet.

## ADDED Requirements

### Requirement: Stock head-to-head benchmark uses matched inputs
The benchmark suite SHALL compare stock 0.8.3 and Loki on the same pinned fixture, proof shape, thread count, and verification policy, and SHALL record both source revisions and feature sets. Sherlock and ZSWAP circuit rows SHALL enable the same `circuit-params` feature on every arm and SHALL be labeled as a circuit feature family.

#### Scenario: Run matched stock and Loki cases
- **WHEN** the planned `python3 scripts/perf/run-matched.py --fixture-set <id> --families default,circuits` command (task 1.1) runs the stock-only and Loki-only executables in paired processes on the same pinned fixture
- **THEN** every paired result identifies stock 0.8.3, the Loki revision, fixture digest, proof shape, worker count, feature set and family, records the shared `circuit-params` exception, bidirectional proof verification on supported fixtures, stock/Loki VK-byte comparison, balanced arm order, and the speedup ratio `median(stock chain_ms)/median(Loki chain_ms)` with slowdowns visible; verify the record schema with `python3 scripts/validate-perf-results.py`, building on `loki-harness/README.md:64-69` and `loki-harness/src/bin/bench.rs:1-20`.

### Requirement: Recursion chain latency is measured end to end
The performance suite SHALL measure the complete ordered Sherlock chain of both leaf proofs, the parent proof, and the decider proof, preserving stage timings and the observed chain interval through independent stock verification and semantic checks.

#### Scenario: Measure and verify one complete chain
- **WHEN** the planned `python3 scripts/perf/run-chain.py` command (task 2.1) runs the pinned four-stage fixture for ten process-cold→process-warm blocks per stock/Loki arm and CPU budget
- **THEN** both leaf stages, parent, decider, and total-chain result are recorded; `chain_ms` includes independent stock verification and semantic checks; every receipt matches canonical public inputs, consumes the complete transcript, completes the final KZG pairing, and verifies each supported stock/Loki proof in both directions; verify with `python3 scripts/perf/run-chain.py --fixture-set <id> --blocks 10 --cpu-budgets 4,20 --cache-sequence cold,warm --verify-stock --verify-both-directions`, building on `TODO.md:27-29`, `docs/GPU.md:431-455`, and `wiki-llm/loki/benchmark/SUITE.md:14,100-120`.

### Requirement: CPU workstream effects are attributed without conflating intervals
The benchmark record SHALL attribute W3 and W11 CPU phases and W12 A3 residual-path work separately from setup, verification, accelerator execution, and whole-chain latency; unmerged W12 rows SHALL remain labeled experimental and SHALL NOT represent the Loki default.

#### Scenario: Reconcile named phases and W12 experiment intervals with timed proof work
- **WHEN** the phase-enabled matched benchmark runs on the pinned fixture and the W12 A3 residual candidate is listed in the pre-registered row manifest
- **THEN** its report identifies W3/W11 CPU phases, W12 A3 residual-path phases, any PK disk-load probe, and unprofiled overhead; it labels W12 branch/source and test-only cache paths without representing them as Loki default behavior, and reconciles profiled phases to profiler total with overhead reported separately; verify with the planned `python3 scripts/perf/run-chain.py --profile-phases --a3-residual <source-sha> --check-phase-sum`, building on `LOKI-CPU.md:105-124`, `LOKI-MULTIOPEN.md:76-85`, `examples/loki-profile.rs:76-78`, `docs/ARCHITECTURE.md:343-343`, and `LOKI-A3-RESIDUAL.md:10-25,73-90`.

### Requirement: Regression results use a declared hardware matrix and noise policy
The performance gate SHALL designate a pinned, qualified native CPU host and matched worker/affinity budgets as its required baseline, and SHALL classify other hardware and target-specific measurements by runner, CPU/GPU, OS, compiler, and execution mode.

#### Scenario: Evaluate paired samples on the baseline runner
- **WHEN** the regression job evaluates a pre-registered run manifest on the pinned host and CPU masks
- **THEN** it requires the five-run same-seed A3 noise pilot to have `(max-min)/median ≤5%`, preserves all ten independent cold→warm chain blocks per arm and CPU budget, reports median/IQR/min/max and every raw sample without a small-sample significance claim, and fails closed on missing or correctness-invalid rows; after baseline freeze it fails if Loki median `chain_ms` is more than 5% slower than matched stock at either N=4 or N=20, while a slowdown at or below 5% passes this timing decision only if every other gate passes; verify with `python3 scripts/check-perf-regression.py --manifest <id> --require-a3-noise-pilot --chain-blocks 10 --cpu-budgets 4,20 --regression-threshold 0.05`, building on `docs/modules/benches.md:15-23`, `TODO.md:27-29`, and `wiki-llm/loki/benchmark/SUITE.md:118-128,159-160`.

#### Scenario: Report secondary CPU and accelerator targets
- **WHEN** benchmark evidence is collected on aarch64 CPU, WASM, mobile, or GPU runners
- **THEN** each row names hardware, runtime, worker mode, feature set, artifact size and measured peak memory when supported, references the frozen native baseline runner manifest and its N=4/N=20 CPU masks when a native comparison is reported, and separates performance comparison status from owner-gated numeric target budgets; verify with `python3 scripts/validate-perf-results.py --hardware-matrix`, building on `docs/PLATFORMS.md:1-40`, `docs/GPU.md:431-455`, and `wiki-llm/loki/benchmark/SUITE.md:68,157`.

#### Scenario: Require target budget evidence before release
- **WHEN** the release hardware matrix is evaluated for wasm32 browser single-thread/threads, Node single-thread/threads, iOS device/simulator, Android device/emulator, and native CPU
- **THEN** each required target row has a named runtime/device, fixture, and frozen numeric execution-time, peak-memory, and artifact-size budget plus a measured result; a missing runner, budget, or result is NOT READY and cannot pass by omission; verify with `python3 scripts/validate-perf-results.py --require-target-budgets wasm-browser,wasm-node,ios,android,native-cpu`, building on `docs/PLATFORMS.md:311-325,351-358,415-424` and `TODO.md:271-288`.

#### Scenario: Measure W13 on production chain shapes
- **WHEN** a qualified GPU arm with the W13 SRS registry is included in the registered A3 and CHAIN workload rows
- **THEN** repeated stock-verified production-shape measurements report invocation scratch, total device use, and persistent registry budget separately and disclose unsupported/failed rows; verify with `python3 scripts/perf/run-chain.py --gpu-arm G0 --report-srs-memory --stock-receipts`, building on `TODO.md:139-141` and `LOKI-SRS-REGISTRY.md:14-16`.

#### Scenario: Compare portable BLST native and WebAssembly modes
- **WHEN** the BLST microbenchmark runs the same fixture against native CPU and wasm32's portable no-thread BLST configuration
- **THEN** every row records runtime, compiler/target flags, worker mode, artifact size, peak memory, ten raw timing samples, median/IQR/min/max, and a native/WASM ratio, and is labeled as a BLST library microbenchmark; verify with `python3 scripts/perf/bench-blst-targets.py --samples 10 --matrix native,wasm-portable`, building on `TODO.md:359-361` and `docs/PLATFORMS.md:324,432`.

#### Scenario: Measure whole-Loki SIMD128 and Rayon variants separately
- **WHEN** the whole-Loki target benchmark runs the same supported prover fixture with WASM SIMD128 disabled/enabled and with named Rayon worker counts
- **THEN** rows identify the Loki source, target flags, runtime, fixture, worker count, artifact size, peak memory, and ten raw timings; they are labeled whole-prover target variants and are not merged with portable BLST microbenchmark results or described as browser/Node chain qualification; verify with `python3 scripts/perf/bench-prover-targets.py --samples 10 --matrix wasm-simd128,rayon-workers`, building on `TODO.md:359-361` and `docs/PLATFORMS.md:324,432`.

#### Scenario: Compare CPU-only WASM and native recursion-chain rows
- **WHEN** the WASM owner executes compat-proof fixture `sherlock-recursion-chain-smoke-v1` from `loki-harness/fixtures/v1/manifest.jsonl` on native CPU and wasm32 browser/Node single-thread and threaded targets, using perf's pinned circuit/config and matched methodology
- **THEN** ten raw chain samples per supported mode report native/WASM latency ratio, named runtime/host, measured memory and artifact size, the target owner's frozen numeric budget and pass/fail outcome, plus any timeout or unsupported-size status; each supported stage retains stock-verifier receipts on target, and production-size chains outside the target envelope are reported as unsupported rather than inferred from the smoke fixture; verify with `python3 scripts/perf/run-chain.py --fixture-set sherlock-recursion-chain-smoke-v1 --targets native,wasm-browser-single,wasm-browser-threads,wasm-node-single,wasm-node-threads --samples 10 --cpu-only`, building on `TODO.md:323-324` and `docs/PLATFORMS.md:351-358`.

### Requirement: Published results are reproducible and disclose limits
The v1 performance closing evidence SHALL publish a stock 0.8.3 versus Loki head-to-head table with raw samples, runnable commands, source and dependency pins, hardware details, and explicit limitations at `wiki-llm/loki/benchmark/RESULTS.md` and a checked repository mirror at `docs/benchmarks/RESULTS.md`.

#### Scenario: Audit the release benchmark publication
- **WHEN** the release publication check processes `RESULTS.md` and its referenced evidence bundle
- **THEN** it confirms both results files have the same row IDs and provenance, every row links to raw records and a command, includes sample count, median/IQR/min/max and stock/Loki median ratio, reports stock-verifier outcomes, and labels historical, single-sample, synthetic, or unavailable evidence without presenting it as a fresh chain result; verify with `python3 scripts/validate-perf-publication.py wiki-llm/loki/benchmark/RESULTS.md docs/benchmarks/RESULTS.md`, building on `README.md:46-63`, `TODO.md:167-169`, and `docs/GPU.md:431-455`.
