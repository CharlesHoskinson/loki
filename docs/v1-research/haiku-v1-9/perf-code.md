# TASK perf-code inventory: haiku-v1-9

Full inventory with file:line refs: ~/repos/loki-wt/v1/docs/v1-research/haiku-v1-9/inventory.md

1. Benchmarks in repo: stock-inherited criterion benches (benches/*.rs, Cargo.toml:135-149) and Loki timing via examples/cpu-bench.rs (per-run timer excludes verify), loki-harness/src/bin/bench.rs (includes verify), ignored manual timers (src/poly/mod.rs:358, src/plonk/lookup/prover.rs:470, CUDA benches).
2. Missing: RESULTS.md (closing condition unmet), stock-only timing driver (bench-driver and stock-verifier do not exist per wiki SUITE.md:33), recursion-chain leaf->parent->decider bench (T06), CI and perf regression gate, stable raw-sample store (T41).
3. Known method problems: loki-harness/README.md:68 says one cold sample per backend, Loki first, so no speed claim; loki-harness mixes stock and Loki in one binary, so it cannot serve as the stock baseline.
