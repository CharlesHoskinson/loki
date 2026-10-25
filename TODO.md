# Unfinished work

This register contains implementation, evidence and release follow-ups from the documentation review. Current source status is Loki main `a256a9a` and loki-gpu main `d71b7ca`; historical measurements retain their original pins. Completed documentation corrections are recorded in [the review](docs/review/REVIEW.md).

Every item below remains open. Owners are roles; an item closes only when its acceptance gate has a reviewable artifact. Platform G1-G6 refer to [the platform gates](docs/PLATFORMS.md), which are distinct from the repository-wide gates in `LOKI-DONE.md`.

- [ ] T01: Publish canonical checkout URLs and revision policy
  Owner: Release maintainer. Acceptance gate: Publish reachable Loki, loki-gpu and Lean URLs; verify all checkout and cross-repository documentation links from a clean checkout.
  Source: README.md; docs/PLATFORMS.md.

- [ ] T02: Validate a minimal downstream consumer
  Owner: Rust integration maintainer. Acceptance gate: From a clean sibling-layout checkout, resolve the patch, create a CPU proof and verify it with unmodified 0.8.3; record command, toolchain, revision and output.
  Source: README.md; loki-harness/README.md.

- [ ] T03: Publish and enforce the supported instance/batch matrix
  Owner: Protocol compatibility maintainer. Acceptance gate: Cover mixed committed/plain columns with multiple proofs; retain the expected stock-rejection diagnostic, document unsupported shapes and require an explicit compatibility decision before any protocol fix.
  Source: LOKI-QUINT.md.

- [ ] T04: Complete the additional G2 verifier cases
  Owner: Compatibility test maintainer. Acceptance gate: Run an independent unmodified 0.8.1 verifier and k20 cases with bidirectional acceptance, equal VK bytes and negative controls; archive source pins, commands and logs.
  Source: LOKI-DONE.md; loki-harness/README.md.

- [ ] T05: Establish exact-target novelty priority, if claimed
  Owner: Prior-art researcher. Acceptance gate: Publish a dated comparison of target version, curve, verifier and output conditions for close public implementations; make a priority claim only with affirmative evidence.
  Source: docs/NOVELTY.md; docs/review/luna-r8.md.

- [ ] T06: Measure complete recursion-chain performance
  Owner: Performance engineer. Acceptance gate: Repeat matched stock/Loki leaf-parent-decider chain runs with fixed features, worker counts, hardware and proof shapes; report sample counts, dispersion, independent verification and all CPU phases.
  Source: docs/GPU.md; docs/PLATFORMS.md.

- [ ] T07: Derive the quotient memory budget
  Owner: Quotient implementation owner. Acceptance gate: Count every live buffer and element size, specify k22 and domain-expansion assumptions, independently recompute the formerly unsupported 26 GB estimate and publish a labeled estimate.
  Source: loki-gpu docs/OVERVIEW.md; design/QUOTIENT.md.

- [ ] T08: Review witness-dependent timing in rational inversion
  Owner: Side-channel reviewer. Acceptance gate: Analyze denominator-equals-one branches in batch_invert_rational under the deployment threat model; document leakage, measurements and any required mitigation before making constant-time claims.
  Source: src/poly/mod.rs; LOKI-P1-SEAM.md.

- [ ] T09: Synchronize the Lean source pin with current Loki
  Owner: Formalization maintainer. Acceptance gate: Update the model/source correspondence from SYNCED-LOKI-COMMIT 877114d to the released source revision, list every changed assumption and archive the Lean check output.
  Source: docs/ARCHITECTURE.md; docs/review/luna-r9.md.

- [ ] T10: Close stock-trace-faithful
  Owner: Formalization maintainer. Acceptance gate: Prove correspondence between the structural prove model and complete stock event trace, including trash/permutation, quotient carry, evaluations and real multiopen rounds.
  Source: LokiLean/Refinement.lean.

- [ ] T11: Close quot-inputs
  Owner: Prover/formalization maintainer. Acceptance gate: Prove that concrete adapters supply exactly the intended extended cosets and challenges to the quotient evaluator; bind the statement to a source revision.
  Source: LokiLean/Refinement.lean; Backend.lean.

- [ ] T12: Establish stock-complete-any-order
  Owner: Proof-system formalization maintainer. Acceptance gate: Prove stock verifier completeness for each admissible lookup permutation used by the deterministic-order option, with all protocol assumptions explicit.
  Source: LokiLean/Refinement.lean.

- [ ] T13: Discharge keygen-deterministic
  Owner: Key-cache formalization maintainer. Acceptance gate: State and prove the key-generation purity premise used by the cache model; retain source checks and VK-byte regression evidence separately.
  Source: LokiLean/Refinement.lean.

- [ ] T14: Prove shipped exact-prefix registry refinement
  Owner: SRS/formalization maintainer. Acceptance gate: Connect pair_count=min(lengths), requested prefix, retained capacity and exact coordinates to resident MSM; check same-address mutation and prefix/zip regressions.
  Source: LOKI-SRS-REGISTRY.md.

- [ ] T15: Prove msm-segmented-partition
  Owner: GPU MSM formalization maintainer. Acceptance gate: Replace segmentedValue_eq_pippengerList sorry with a checked proof and record a clean build and theorem assumptions.
  Source: gpu-spec LokiLean/BackendMsm.lean.

- [ ] T16: Prove quotient-interpreter-safety
  Owner: GPU quotient formalization maintainer. Acceptance gate: Replace valid_program_eval_some sorry with a checked proof for every validated program and record assumptions/build output.
  Source: gpu-spec LokiLean/BackendQuotient.lean.

- [ ] T17: Prove quotient-compiler-lowering
  Owner: Quotient compiler/formalization maintainer. Acceptance gate: Show every ordered source term, rotation, challenge and column lowers to validated bytecode evaluating the same numerator.
  Source: gpu-spec LokiLean/BackendQuotient.lean.

- [ ] T18: Relate P2 runtime code to its Lean transition model
  Owner: Runtime/formalization maintainer. Acceptance gate: Define a relation for Rust state and native stream/event completion and check transition preservation; keep implementation tests distinct from model proofs.
  Source: gpu-spec LokiLean/Backend.lean; loki-gpu runtime.rs.

- [ ] T19: Close runtime-lifetime evidence
  Owner: GPU runtime maintainer. Acceptance gate: Run fault injection and compute-sanitizer for staging reuse, retirement, cancellation, context loss and release; publish source pins/logs and state which lifetime properties remain outside Lean.
  Source: gpu-spec LokiLean/Backend.lean.

- [ ] T20: Specify digest-binds and disk integrity
  Owner: Key-cache security maintainer. Acceptance gate: State the collision-resistance assumption and bind circuit/config/VK identity and file integrity to retained cache entries; test corruption and mismatched-relation cases.
  Source: LokiLean/Pipeline.lean; LOKI-QUINT.md.

- [ ] T21: Resolve future digest-keyed SRS identity
  Owner: SRS API designer. Acceptance gate: Before replacing exact coordinate comparisons, establish immutable identity binding with explicit cryptographic assumptions and mutation tests. This is not required by the shipped exact-compare registry.
  Source: LokiLean/GpuSystem.lean; LOKI-SRS-REGISTRY.md.

- [ ] T22: Complete P3 legacy adapter and SRS integration
  Owner: GPU backend maintainer. Acceptance gate: Implement the planned adapter/registry boundary; pass exact-prefix, mutation, failure and stock-verifier tests with recorded revisions.
  Source: docs/modules/loki-gpu.md.

- [ ] T23: Complete P4 resident NTT/coset pipeline
  Owner: GPU NTT maintainer. Acceptance gate: Pass forward/inverse/coset equality, normalization, memory/error and GPU-required stock-verifier gates; publish resident-path timings.
  Source: docs/modules/loki-gpu.md.

- [ ] T24: Complete P5 quotient compiler and interpreter integration
  Owner: GPU quotient maintainer. Acceptance gate: Implement ordered bytecode evaluation, pass differential and stock-verifier gates, close input/lowering obligations and publish memory/chain results.
  Source: docs/modules/loki-gpu.md.

- [ ] T25: Decide and implement P6 balanced MSM
  Owner: GPU MSM maintainer. Acceptance gate: Measure repeated-scalar skew, decide the conditional package using recorded evidence, and pass arithmetic/skew/failure/stock-verifier gates if implemented.
  Source: docs/modules/loki-gpu.md.

- [ ] T26: Implement P7 chain integration and scheduling
  Owner: Scheduler maintainer. Acceptance gate: Validate measured memory peaks, one-heavy-job admission, checkpoint persistence and cross-backend locking on a full verified recursion chain.
  Source: docs/modules/loki-gpu.md; LOKI-QUINT.md.

- [ ] T27: Decide P8/P9 lookup batching and concurrency
  Owner: GPU/scheduler maintainer. Acceptance gate: Record profile-based go/no-go decisions; if pursued, implement lookup preparation/batching/concurrency and pass arithmetic, failure and complete-chain gates.
  Source: docs/modules/loki-gpu.md.

- [ ] T28: Define cancellation before scheduling uses it
  Owner: Runtime/API maintainer. Acceptance gate: Ensure Cancelled work is not automatically retried by CPU fallback; add an explicit cancellation contract and fault-injection regression before P7 uses cancellation.
  Source: LOKI-P1-SEAM.md.

- [ ] T29: Prove scheduler liveness
  Owner: Formal methods maintainer. Acceptance gate: Check admittedEventuallyFinish under explicit service/fairness assumptions with a tool supporting them; retain sampled completion evidence as a separate result.
  Source: LOKI-QUINT.md.

- [ ] T30: Expand transcript replay fixtures
  Owner: Model-based testing maintainer. Acceptance gate: Cover arbitrary query rotations, more circuit shapes and third advice phase; record trace-to-source correspondence and unchanged stock-verifier results.
  Source: LOKI-QUINT.md.

- [ ] T31: Replay current dispatch/NTT implementation
  Owner: Model-based testing maintainer. Acceptance gate: Update historical W4/W6 source replay to the current P1/W9 paths, including NTT failures and shared locking; archive immutable source pins and model-state checks.
  Source: LOKI-QUINT.md.

- [ ] T32: Improve proof and soundness fuzz input coverage
  Owner: Fuzzing maintainer. Acceptance gate: Use structured inputs beyond the recorded 4-6-byte seeds; publish corpus shape, campaigns and MockProver-conditioned rejection/acceptance results.
  Source: LOKI-FUZZ.md.

- [ ] T33: Document key-cache retention and caller assumptions
  Owner: Key-cache maintainer. Acceptance gate: Test or explicitly exclude crashes, failed publication, deletion, eviction, hash collisions and same-type wrong-relation misuse; bind each guarantee to its prerequisites.
  Source: LOKI-QUINT.md.

- [ ] T34: Measure W13 on production chain shapes
  Owner: SRS/performance maintainer. Acceptance gate: Run repeated real A3/chain stock-verifier and latency gates with the registry; report invocation scratch and total device usage separately from persistent budget.
  Source: LOKI-SRS-REGISTRY.md.

- [ ] T35: Validate multi-device and additional hardware behavior
  Owner: GPU validation maintainer. Acceptance gate: Test mixed-GPU accounting and selected non-sm_120/toolkit combinations; record device selection, arithmetic equality, fallback and stock-verifier results.
  Source: LOKI-SRS-REGISTRY.md; docs/GPU.md.

- [ ] T36: Integrate W10 with P1/W13
  Owner: Prover integration maintainer. Acceptance gate: Reconcile shared call sites and profile counters, apply the authorized msm.prepare exclusion, then run combined arithmetic, stock-verifier and measured chain gates on the merged revision.
  Source: LOKI-SRS-REGISTRY.md; LOKI-P1-SEAM.md.

- [ ] T37: Validate the declared Rust minimum
  Owner: Rust release maintainer. Acceptance gate: Build and run appropriate CPU/integration checks on declared Rust 1.76.0 or raise the manifest requirement with evidence; distinguish validated Rust 1.99.1 in docs.
  Source: README.md.

- [ ] T38: Repair standalone locked-workspace reproducibility
  Owner: Rust test infrastructure maintainer. Acceptance gate: Resolve the existing fuzz lock mismatch and decide lockfile policy for stock-profile-verifier, cuda-ntt and cuda-runtime; documented --locked commands must work from a clean checkout.
  Source: LOKI-P1-SEAM.md.

- [ ] T39: Audit backend provenance and attribution
  Owner: Release/provenance maintainer. Acceptance gate: Publish the clean-room provenance register, permitted-source boundary, license notices and third-party attributions for each shipped backend revision.
  Source: docs/NOVELTY.md; loki-gpu design references.

- [ ] T40: Complete narrower formal/runtime novelty comparison
  Owner: Prior-art researcher. Acceptance gate: Compare the exact conditional refinement statement (whose contracts currently imply extensional equality with StockBackend), runtime contract and prover-specific trace corpus against cited formalization and testing literature; claim priority only with affirmative evidence.
  Source: docs/NOVELTY.md; docs/review/luna-r9.md.

- [ ] T41: Validate complete historical performance evidence portability
  Owner: Performance/release maintainer. Acceptance gate: Publish referenced raw samples and scripts at stable paths, including W10 RESULTS/RESULTS-DETAILS; identify unavailable local-only artifacts without implying fresh reproduction.
  Source: docs/GPU.md; LOKI-*.md.

- [ ] T42: Decide fused k15 terminal proposal
  Owner: Protocol/product owner. Acceptance gate: Record explicit approval or rejection of the new-key Track B design and, if accepted, publish compatibility and complete-chain gates.
  Source: LOKI-DONE.md.

- [ ] T43: Intel: Detect Intel GPUs through Level Zero; fill `BackendCapabilities`.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Detect Intel GPUs through Level Zero; fill `BackendCapabilities`.
  Source: docs/PLATFORMS.md.

- [ ] T44: Intel: 32-bit-limb Fr and Fp Montgomery mul, add, sub in SYCL; G1.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: 32-bit-limb Fr and Fp Montgomery mul, add, sub in SYCL; G1.
  Source: docs/PLATFORMS.md.

- [ ] T45: Intel: Microbenchmark 32-bit vs 64-bit limb throughput; record numbers in this file.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Microbenchmark 32-bit vs 64-bit limb throughput; record numbers in this file.
  Source: docs/PLATFORMS.md.

- [ ] T46: Intel: NTT, natural-order output, up to the CUDA-equal `max_transform_log`; G1, G2.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: NTT, natural-order output, up to the CUDA-equal `max_transform_log`; G1, G2.
  Source: docs/PLATFORMS.md.

- [ ] T47: Intel: MSM with the same digit and bucket scheme as `MSM.md`; G1, G2, G5.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: MSM with the same digit and bucket scheme as `MSM.md`; G1, G2, G5.
  Source: docs/PLATFORMS.md.

- [ ] T48: Intel: Resident buffers, SRS lease path; G4.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Resident buffers, SRS lease path; G4.
  Source: docs/PLATFORMS.md.

- [ ] T49: Intel: Quotient interpreter; G2.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Quotient interpreter; G2.
  Source: docs/PLATFORMS.md.

- [ ] T50: Intel: Stock-verifier run on a real k17 to k19 proof; G3.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Stock-verifier run on a real k17 to k19 proof; G3.
  Source: docs/PLATFORMS.md.

- [ ] T51: Intel: Chain run on Arc and on a Max card, if available; G6.
  Owner: Intel platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Chain run on Arc and on a Max card, if available; G6.
  Source: docs/PLATFORMS.md.

- [ ] T52: AMD: Run hipify-clang over a copy of `cuda/`; record what fails to compile.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Run hipify-clang over a copy of `cuda/`; record what fails to compile.
  Source: docs/PLATFORMS.md.

- [ ] T53: AMD: Replace PTX asm with portable primitives; G1.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Replace PTX asm with portable primitives; G1.
  Source: docs/PLATFORMS.md.

- [ ] T54: AMD: Parameterise wavefront size (32 and 64); test both on RDNA.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Parameterise wavefront size (32 and 64); test both on RDNA.
  Source: docs/PLATFORMS.md.

- [ ] T55: AMD: NTT and MSM parity: G1, G2, G5.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: NTT and MSM parity: G1, G2, G5.
  Source: docs/PLATFORMS.md.

- [ ] T56: AMD: SRS lease, error mapping (HIP error codes to `ErrorKind`); G4.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: SRS lease, error mapping (HIP error codes to `ErrorKind`); G4.
  Source: docs/PLATFORMS.md.

- [ ] T57: AMD: Stock verifier; G3.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Stock verifier; G3.
  Source: docs/PLATFORMS.md.

- [ ] T58: AMD: Chain run on one CDNA and one RDNA card; G6.
  Owner: AMD platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Chain run on one CDNA and one RDNA card; G6.
  Source: docs/PLATFORMS.md.

- [ ] T59: Apple: Metal device discovery; `recommendedMaxWorkingSetSize` into `device_memory_budget`.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Metal device discovery; `recommendedMaxWorkingSetSize` into `device_memory_budget`.
  Source: docs/PLATFORMS.md.

- [ ] T60: Apple: Decide the unified-memory capability in the API (O1) before writing buffers.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Decide the unified-memory capability in the API (O1) before writing buffers.
  Source: docs/PLATFORMS.md.

- [ ] T61: Apple: 32-bit-limb Fr/Fp Montgomery in MSL; G1.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: 32-bit-limb Fr/Fp Montgomery in MSL; G1.
  Source: docs/PLATFORMS.md.

- [ ] T62: Apple: 32 vs 64-bit limb microbenchmark on M-series; record here.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: 32 vs 64-bit limb microbenchmark on M-series; record here.
  Source: docs/PLATFORMS.md.

- [ ] T63: Apple: NTT, then MSM (sort-based buckets); G1, G2, G5.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: NTT, then MSM (sort-based buckets); G1, G2, G5.
  Source: docs/PLATFORMS.md.

- [ ] T64: Apple: Quotient interpreter; G2.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Quotient interpreter; G2.
  Source: docs/PLATFORMS.md.

- [ ] T65: Apple: Stock verifier; G3. Chain run on the largest-memory M-series Mac available; G6.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Stock verifier; G3. Chain run on the largest-memory M-series Mac available; G6.
  Source: docs/PLATFORMS.md.

- [ ] T66: Apple: Thermal behaviour on fanless parts (MacBook Air) documented.
  Owner: Apple platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Thermal behaviour on fanless parts (MacBook Air) documented.
  Source: docs/PLATFORMS.md.

- [ ] T67: Mobile phones: Measure the smallest Loki leaf proof on a mid-range phone on CPU only (aarch64 build). This is the real baseline.
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Measure the smallest Loki leaf proof on a mid-range phone on CPU only (aarch64 build). This is the real baseline.
  Source: docs/PLATFORMS.md.

- [ ] T68: Mobile phones: Decide which circuits are client-side candidates (O8).
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Decide which circuits are client-side candidates (O8).
  Source: docs/PLATFORMS.md.

- [ ] T69: Mobile phones: iOS: reuse the Metal backend; run G1 to G3 on an iPhone.
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: iOS: reuse the Metal backend; run G1 to G3 on an iPhone.
  Source: docs/PLATFORMS.md.

- [ ] T70: Mobile phones: Android: Vulkan `shaderInt64` survey across Adreno and Mali; 32-bit-limb kernels; G1 to G3.
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Android: Vulkan `shaderInt64` survey across Adreno and Mali; 32-bit-limb kernels; G1 to G3.
  Source: docs/PLATFORMS.md.

- [ ] T71: Mobile phones: Thermal soak test: 10 consecutive proofs, record per-proof time; G6 reported as sustained throughput.
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Thermal soak test: 10 consecutive proofs, record per-proof time; G6 reported as sustained throughput.
  Source: docs/PLATFORMS.md.

- [ ] T72: Mobile phones: Memory-kill test: confirm clean `ContextLost` handling; G4.
  Owner: Mobile platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Memory-kill test: confirm clean `ContextLost` handling; G4.
  Source: docs/PLATFORMS.md.

- [ ] T73: WebAssembly: Audit workspace crates for `wasm32-unknown-unknown` compilation (BLST, rayon, getrandom, std::time).
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Audit workspace crates for `wasm32-unknown-unknown` compilation (BLST, rayon, getrandom, std::time).
  Source: docs/PLATFORMS.md.

- [ ] T74: WebAssembly: CPU build in a browser; prove the smallest circuit; G3 using the stock verifier compiled to wasm or run natively on the produced bytes.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: CPU build in a browser; prove the smallest circuit; G3 using the stock verifier compiled to wasm or run natively on the produced bytes.
  Source: docs/PLATFORMS.md.

- [ ] T75: WebAssembly: Add `wasm-bindgen-rayon`; measure scaling to 4, 8, 16 workers.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Add `wasm-bindgen-rayon`; measure scaling to 4, 8, 16 workers.
  Source: docs/PLATFORMS.md.

- [ ] T76: WebAssembly: Measure the memory ceiling: largest k that proves in Chrome, Firefox and Safari.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Measure the memory ceiling: largest k that proves in Chrome, Firefox and Safari.
  Source: docs/PLATFORMS.md.

- [ ] T77: WebAssembly: WGSL 32-bit-limb Fr/Fp Montgomery; G1.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: WGSL 32-bit-limb Fr/Fp Montgomery; G1.
  Source: docs/PLATFORMS.md.

- [ ] T78: WebAssembly: WGSL NTT, then MSM; G1, G2, G5.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: WGSL NTT, then MSM; G1, G2, G5.
  Source: docs/PLATFORMS.md.

- [ ] T79: WebAssembly: Worker-hosted backend, `ContextLost` mapping; G4.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Worker-hosted backend, `ContextLost` mapping; G4.
  Source: docs/PLATFORMS.md.

- [ ] T80: WebAssembly: Chain-level comparison against CPU wasm; G6.
  Owner: WebAssembly platform maintainer. Acceptance gate: Complete this check and attach source revision, command/output and the cited platform gates from docs/PLATFORMS.md: Chain-level comparison against CPU wasm; G6.
  Source: docs/PLATFORMS.md.

- [ ] T81: Platform decision O1 unified-memory API
  Owner: Backend API designer. Acceptance gate: Record a memory-kind/zero-copy decision with accounting semantics and a tested discrete/unified example.
  Source: docs/PLATFORMS.md.

- [ ] T82: Platform decision O2 changing device budgets
  Owner: Backend API designer. Acceptance gate: Define budget-change handling and demonstrate safe admission/eviction under a shrinking budget.
  Source: docs/PLATFORMS.md.

- [ ] T83: Platform decision O3 browser completion model
  Owner: WebGPU/API designer. Acceptance gate: Choose worker-blocking or leak-safe async completion and test device loss plus forgotten handles.
  Source: docs/PLATFORMS.md.

- [ ] T84: Platform decision O4 limb throughput
  Owner: Platform performance engineer. Acceptance gate: Publish comparable 32-bit/64-bit limb microbenchmarks for Arc, RDNA, CDNA, Apple, Adreno and Mali.
  Source: docs/PLATFORMS.md.

- [ ] T85: Platform decision O5 public platform prior art
  Owner: Prior-art researcher. Acceptance gate: Publish a dated, source-linked BLS12-381 MSM/NTT survey for Intel, HIP and Metal; distinguish algorithms, curves and licenses.
  Source: docs/PLATFORMS.md.

- [ ] T86: Platform decision O6 Android capabilities
  Owner: Android backend maintainer. Acceptance gate: Publish shaderInt64 and compute-throughput measurements for named Adreno/Mali devices.
  Source: docs/PLATFORMS.md.

- [ ] T87: Platform decision O7 device memory ceilings
  Owner: Mobile/browser validation engineer. Acceptance gate: Measure maximum successful k by device/browser and confirm recoverable allocation/context-loss behavior.
  Source: docs/PLATFORMS.md.

- [ ] T88: Platform decision O8 client-side circuits and data
  Owner: Protocol/privacy owner. Acceptance gate: Approve named leaf circuits and witness-data handling rules against measured client resource limits.
  Source: docs/PLATFORMS.md.

- [ ] T89: Platform decision O9 wasm BLST baseline
  Owner: WASM performance engineer. Acceptance gate: Measure native versus wasm32 BLST with/without SIMD128 and threads using matched fixtures.
  Source: docs/PLATFORMS.md.

- [ ] T90: Platform decision O10 portable/native comparison
  Owner: Platform performance engineer. Acceptance gate: Measure wgpu/native ratios at k17-k19 on the same cards and use the data for backend prioritization.
  Source: docs/PLATFORMS.md.

- [ ] T91: Platform decision O11 browser verifier gate
  Owner: Compatibility test owner. Acceptance gate: Record whether browser-produced bytes require a wasm verifier or native stock verification, then implement the agreed gate.
  Source: docs/PLATFORMS.md.

- [ ] T92: Platform decision O12 kernel reuse notices
  Owner: Release/provenance maintainer. Acceptance gate: Check provenance and required attribution/NOTICE for every reused kernel fragment before distributing a new port.
  Source: docs/PLATFORMS.md.

- [x] T93: Complete the coordinator's final diagram screenshot check
  Owner: Documentation coordinator, with diagram owners r2 and r3. Acceptance gate: render and inspect the latest `docs/diagrams/pipeline.svg`, `layers.svg`, `correctness.svg`, `recursion.svg`, `features.svg`, `gpu-fallback.svg` and `platforms.svg`; confirm readable labels, no covered edges and current P1/P2/W10 semantics. Record the reviewed revisions and fix any findings before publication. VERIFY-REQUEST messages have been sent for all seven files, including the final W10 dependency correction in `features.svg`.
  Source: docs/review/REVIEW.md; docs/review/coordinator-haiku-arch.md.

