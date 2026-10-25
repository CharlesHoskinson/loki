# Proposal

## Why

Loki has useful phase and kernel measurements, but its current Sherlock A3 comparison is a single matched parent sample and does not measure a full leaf-to-parent-to-decider chain. The v1 release condition requires a reproducible head-to-head table against unmodified `midnight-proofs` 0.8.3, with enough repeated evidence to distinguish CPU-phase changes from end-to-end performance.

## What Changes

- **Must-have:** Follow the accepted `wiki-llm/loki/benchmark/SUITE.md` protocol with independently resolved stock 0.8.3 and Loki driver workspaces, shared immutable fixtures, fixed feature/worker/affinity settings, ten process-cold→warm chain blocks per arm at N=4 and N=20, balanced shuffled arm order, five A3 noise-pilot runs, and stock verification.
- **Must-have:** Measure the four-stage Sherlock chain (two leaves, parent, decider) through semantic checks and independent stock verification; report W3/W11 CPU phases and W12 A3 residual-path work separately, labeling unmerged W12 experiments as branch-only and PK disk reuse as test-only unless adopted.
- **Must-have:** Add performance regression CI that qualifies the reserved host/noise floor, preserves full raw samples and correct verifier receipts, and refuses incomplete or invalid comparisons. Timing candidates follow the accepted suite's no-small-sample-significance rule.
- **Must-have:** Publish source/dependency pins, host details, run commands, raw observations, sample counts, median/IQR/min/max, stock/Loki median ratio, verifier receipts, slowdowns, and unavailable/historical evidence in the closing table at `wiki-llm/loki/benchmark/RESULTS.md`, with a checked mirror at Loki `docs/benchmarks/RESULTS.md`.
- **Must-have:** Define the hardware matrix and its eligibility status; a missing or unqualified runner remains NOT READY, never a pass. Cross-target size/memory/time budgets remain gating in their owning platform CI.
- **Must-have:** Close T34 with repeated production-shape A3/chain measurements for the W13 SRS registry, independent stock verification, and separate invocation scratch, total device use, and persistent-budget evidence.
- **Must-have:** Close T89 with a matched native versus wasm32 BLST baseline for single-thread, SIMD128, and threads configurations, reporting it as a kernel/library microbenchmark rather than a prover-chain result.
- **Must-have:** Close T80 with a CPU-only native versus browser/Node WASM Sherlock chain comparison on identical fixtures; report each execution mode's latency, memory, size, and unsupported-size/timeout result without making a kernel-speed claim.
- **Nice-to-have:** Add a multi-host performance trend dashboard and comparable measurements on additional CPU/GPU architectures after stable runners and owner-provided compatible fixtures exist.

## Capabilities

### New Capabilities

- `benchmark-evidence`: Reproducible, independently verified performance measurements and release regression evidence for stock 0.8.3 versus Loki.

### Modified Capabilities

None.

## Impact

This is a planning change for the benchmark harness, scripts, CI workflows, profile output, and publication at `wiki-llm/loki/benchmark/RESULTS.md` plus a checked Loki repository mirror at `docs/benchmarks/RESULTS.md`. It does not alter prover APIs, defaults, proof formats, transcripts, or Loki-only feature gates. The current evidence base includes `TODO.md:27-29` (T06), `README.md:46-63`, `LOKI-CPU.md:105-124`, `LOKI-MULTIOPEN.md:76-85`, and `docs/GPU.md:431-455`; those sources document the present gaps and historical measurements, not completed v1 gates.

Stock 0.8.3 remains the unmodified reference executable and independent verifier; supported fixture outputs are checked in both directions, and stock/Loki VK bytes are compared. Benchmark-only instrumentation stays opt-in. Default workload rows use stock/Loki defaults; Sherlock and ZSWAP rows explicitly share the `circuit-params` feature required by their relation dependencies and do not claim pure-default coverage. CPU compatibility and numeric size/memory/time budgets on WASM and iOS/Android remain release gates owned by their platform areas; performance comparisons stay informational without a pinned baseline. Differential fuzz, Quint MBT replay, and the release-synced Lean contract must use the shared fixture identities supplied by compat-proof. The docs-release area owns release CI aggregation and platform documentation; perf owns performance protocol, benchmark evidence, and the `RESULTS.md` row contract.

Out of scope: changing prover algorithms or enabling new optimizations by default (would endanger stock compatibility); treating kernel or synthetic phase wins as chain speedups (they do not establish end-to-end value); hard gates on shared/heterogeneous machines (noise can cause false results); and requiring GPU acceleration for CPU, WASM, or mobile performance gates (these targets need CPU-only paths).

Risks include runner noise, incompatible historical branch/source pins, unavailable raw logs for older measurements, verification/setup time being accidentally mixed with proof latency, and CI resource cost. Dependencies: compat-proof owns shared fixtures and stock-oracle invocation; the GPU and CPU implementation owners expose phase instrumentation and backend labels; WASM/mobile owners provide target evidence and budgets; fuzz/quint/lean owners consume fixture IDs; docs-release owns CI publication integration.
