# Tasks

## 1. Shared fixture and target harness

- [ ] 1.1 Adopt compat-proof's `loki-harness/fixtures/v1/manifest.jsonl` schema and exact stock 0.8.3 source pin; verify each row records schema/version pins, fixture/circuit/k, instance/batch shape, Serde/transcript/features/seed, expected outcome, transcript vector, artifact paths and SHA-256 for SRS, VK, PK, proof, and public inputs, plus wrong-input/malformed/truncated/mutated/appended variants; round-trip PK only through existing `ProvingKey::bytes_length/write/read/to_bytes/from_bytes` at `src/plonk/mod.rs:363-431, 447-464`.
- [ ] 1.2 Build a wasm32 fixture runner for both independent stock and Loki packages; verify `wasm:stock-compat` executes both verifiers in browser and Node and checks valid proof acceptance, wrong-input and malformed-input negatives, plus VK/PK/SRS serialization at every supported fixture size.
- [ ] 1.3 Add browser and Node single-thread smoke commands; verify `cargo check --locked --target wasm32-unknown-unknown --lib` and `wasm:runtime --host browser,node --mode single` build and execute the smallest canonical fixture under pinned host versions.
- [ ] 1.4 Test JS-backed cryptographic randomness in browser and Node, including a test-only unavailable-entropy adapter; verify `wasm:runtime --rng` records host entropy and demonstrates rand_core 0.6 `OsRng` failure/trap before the fixture runner exports a proof bundle.
- [ ] 1.5 Reject unsupported WASM feature combinations before native build tools or runtime-only panic paths; verify checks with `--features loki-gpu-backend`, `loki-sppark`, and `loki-profile` fail with targeted diagnostics naming the unsupported target/feature.

## 2. Opt-in worker threads

- [ ] 2.1 Add a dedicated threaded-WASM build configuration and pinned nightly toolchain for `wasm-bindgen-rayon`; verify `cargo tree --locked -e features --target wasm32-unknown-unknown` shows thread-only dependencies are absent from the default build and present only with the explicit thread mode.
- [ ] 2.2 Implement browser worker integration and a Loki-owned Node `worker_threads` adapter; verify `wasm:threads --host browser,node` initializes Rayon workers and completes the canonical proof/verification fixture, records worker counts, and fails before proving when atomics, shared memory, COOP/COEP, or Node worker setup are unavailable.
- [ ] 2.3 Run the stock 0.8.3 and Loki fixture gates in both threaded host lanes; verify `wasm:stock-compat --mode threads --host browser,node` performs target-side two-way proof checks and VK/PK/SRS round trips without requiring GPU support.

## 3. Budgets, QA, and docs

- [ ] 3.1 Measure baseline peak memory, `.wasm` plus JS size, proof bytes, and elapsed time on pinned browser/Node × single/threaded runners; have perf review and freeze `loki-harness/fixtures/v1/wasm-budgets.json` before candidate qualification with source revision, host version, mode, fixture ID, metric, unit, baseline, and numeric limit; verify `wasm:budget` rejects missing values and any post-freeze record change.
- [ ] 3.2 Benchmark the BLST portable no-thread wasm32 path against a matched native baseline; measure whole-Loki code with and without SIMD128 and Rayon worker counts 1, 4, 8, and 16 where supported; verify `wasm:blst-baseline` records toolchain, fixture, worker count and median without attributing SIMD to BLST C.
- [ ] 3.3 Have perf define the `sherlock-recursion-chain-smoke-v1` small circuit/configuration and compat-proof encode it in `loki-harness/fixtures/v1/manifest.jsonl`; run it on each pinned browser/Node × single/threaded lane and verify `wasm:chain` records stock verdict, wall time, memory, and proof size against the frozen `wasm-budgets.json`, failing release if the required fixture ID is absent. Record larger production-chain attempts as measured envelope outcomes, including explicit unsupported/timeout size where applicable.
- [ ] 3.4 Replay compat-proof fixtures, fuzz-owner deterministic corpus vectors, and Quint-owner target-neutral model traces through each wasm32 host/mode runner; verify `wasm:fixtures` reports fixture IDs, expected/observed verdicts, and source pins. Keep libFuzzer execution host-native unless the fuzz owner adds a supported wasm runner.
- [ ] 3.5 Add a release CI matrix for all four host/mode lanes, target compatibility, fixture replay, BLST baseline, chain workload, and budget enforcement; verify a CPU-only runner completes the required WASM gates without CUDA or WebGPU.
- [ ] 3.6 Document pinned runtime versions, toolchains, JS randomness, non-JS wasm limitation, Loki-owned Node worker requirements, cross-origin isolation, 4 GiB theoretical address-space ceiling versus measured limits, WebGPU deferral, worker/completion handoff, and compile-time rejection of GPU/profile features; verify `wasm:docs` checks platform docs/rustdoc and links to compatibility, fuzz, Quint/MBT, Lean, and budget evidence.
- [ ] 3.7 Coordinate release SHA coverage with fuzz, Quint, and Lean owners; verify `wasm:fixtures` traces shared vectors and the release Lean gate records the same SHA and passes its no-sorry/no-new-custom-axiom audit without claiming WASM compilation as a formal proof.

## 4. Integration and acceptance

- [ ] 4.1 Run all four pinned host/mode lanes against the release candidate and its exact stock 0.8.3 dependency; verify `wasm:ci` has no skipped runtime, stock-compatibility, fixture, or budget jobs and archive the raw logs/measurements.
- [ ] 4.2 Run `openspec validate v1-wasm-cpu --strict` and resolve every validation finding; verify the result is `valid` before release review.
