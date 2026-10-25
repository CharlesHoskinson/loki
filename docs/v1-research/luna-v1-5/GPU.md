# GPU area research — luna-v1-5

## Source snapshot

Read the lab context and astra-v1 KICKOFF on 2026-10-25. The checked code is Loki main in `/home/charles/repos/loki-proofs`, loki-gpu main in `/home/charles/repos/loki-gpu`, and the gpu-spec Lean worktree in `/home/charles/repos/loki-lean-wt/gpu-spec`. Scout reports that `loki-gpu` P2 is on main and arithmetic implementations are still stubs. The v1 workspace itself is planning-only; no production code or tests were changed/run here.

Scout notes:

- `docs/v1-research/haiku-v1-5/inventory.md` — complete state inventory and branch mappings.
- `docs/v1-research/haiku-v1-5/GPU-CODE.md` — code and design references.
- `docs/v1-research/haiku-v1-5/GPU-TESTS.md` — existing/planned QA and test gaps.
- `docs/v1-research/haiku-v1-5/GPU-BESTPRACTICE.md` — external practice research.

## Observed implementation state

- Loki has the P1 typed backend seam and separate legacy GPU flags (`Cargo.toml:45-74`, `src/backend/dispatch.rs:107-159,259-286`). The compile-time guard rejects new backend plus legacy `cuda`/`cuda-ntt` (`src/lib.rs:11-18`). Default features remain `committed-instances` only (`Cargo.toml:45-52`).
- The new `loki-gpu` shell reports runtime capabilities only and does not advertise arithmetic (`loki-gpu/crates/loki-gpu/src/lib.rs:1-45`). MSM returns `Unsupported` (`msm.rs:1-29`), NTT and resident NTT return `Unsupported` (`ntt.rs:1-36`), quotient evaluation returns `Unsupported` (`quotient.rs:1-20`), and SRS registry operations are stubs (`srs.rs:1-20`). Thus the opt-in new backend currently computes on CPU through the dispatcher; feature compilation does not prove GPU execution.
- P2 runtime has generation-tagged opaque handles and transaction semantics (`loki-gpu/design/RUNTIME.md:48-92`), default SRS/device/checkpoint budgets plus a bounded staging ring (`loki-gpu/crates/loki-gpu/src/runtime.rs:64-100`), and operation telemetry (`runtime.rs:398-448`). Existing arithmetic module tests named in TEST-PLAN are proposed future targets, not present.
- Unmerged Loki branches: `loki/p4-ntt`, `loki/p3-srs`, `loki/w10-gpu-quotient`, `loki/p5-quotient`, `loki/p7-chain`. Dependency source `deps.yaml:11-39` orders P4 then P3, P5 after P3/P4/W10, and P7 after P4. Full P7 acceptance still depends on P3/P4/P5. P4 owns runtime changes; after rebase, P3/P7 tests must use fake-driver seams (`deps.yaml:16,23,38`).
- P6 balanced MSM, P8 lookup prep, and P9 batching/concurrency/precompute are conditional in `loki-gpu/design/PLAN.md:116-168`; astra requested these stay out of initial v1 scope unless explicit reviewed re-entry with all gates.
- Lean gpu-spec contains two `sorry`: segmented MSM partition at `LokiLean/BackendMsm.lean:282` and quotient interpreter safety at `LokiLean/BackendQuotient.lean:182`; quotient compiler lowering is an open obligation (`BackendQuotient.lean:387`). P5 safety/lowering closure is a release blocker if P5 ships; segmented MSM only blocks if P6 re-enters.
- Native receipt split: CPU/fake-driver tier gates merge; native sanitizer/device receipts are post-merge and gate release/push (`docs/MERGE-GATES.md:5-22`).
- Platform docs list legacy CUDA (sm_120) and W13 legacy registry as implemented; AMD/HIP, Intel/SYCL-Level Zero, Apple/Metal, mobile, WASM, and WebGPU are TODO (`docs/PLATFORMS.md:1-13,402-416`). v1 GPU scope is CUDA only. WASM and mobile CPU gates remain independent of CUDA.

## Decision and area interfaces

Must-have GPU implementation path: P3 (exact validated SRS lease/registry), P4 (resident NTT/coset), P5 (ordered quotient lowering/interpreter), P7 (resource-bounded chain integration). P6/P8/P9 are not initial v1 implementation scope. Cancellation is terminal and never CPU-retried; fallback applies only to errors that preserve caller-visible state and only with complete host inputs/checkpoints. T07 live-buffer derivation and T19 fault/native sanitizer evidence block release. Existing runtime defaults are 4 GiB SRS, 2 GiB device headroom, 128 MiB pinned staging, 1 GiB checkpoints.

Area ownership confirmed by messages:

- Area 2 owns shared stock 0.8.3 proof/key fixture schema and bidirectional proof checks, including PK/VK/SRS serialization coverage. GPU consumes the schema; deterministic proof bytes require fixed RNG/thread and controlled lookup order.
- Area 6 owns fuzz campaign duration, corpus and crash retention. GPU supplies SRS mutation/lease, NTT/coset, quotient-bytecode, and scheduler resource/cancel cases.
- Area 7 owns `quint/scheduler.qnt`, `quint/gpu_dispatch.qnt`, and Rust MBT trace contract. Actual Rust replay is required for shipped scheduler/dispatch behavior.
- Area 8 owns canonical Lean gpu-spec merge and exact release-SHA sync. GPU supplies P3/P4/P5/P7 contracts; P5 Lean safety and compiler lowering must close before shipment.
- Area 9 owns performance methodology and thresholds. GPU provides feature/host labels, dispatch/fallback counters and runtime telemetry. Current Loki metrics: `loki-profile` exclusive phases, `total_wall_ns`, backend counters (`src/profile.rs:36-45,79-82,230-240`); runtime: `init_ns`, H2D/D2H pinned/pageable bytes, `event_wait_ns`, `gpu_event_ms`, `live_high_water`, quarantined bytes (`runtime.rs:398-422`). GPU data are informational absent qualified stable runner. W12 is PK disk-cache work, not a CPU phase; area 9 will report cold/warm per-stage cache intervals.
- Area 10 owns overall CI orchestration and docs. GPU supplies feature/hardware matrix, operations, limits, fallback, commands and receipts.

## Outside best practice

- [Cargo features](https://doc.rust-lang.org/cargo/reference/features.html): optional dependencies remain absent unless selected; default features are part of compatibility, and features of a shared dependency are unioned. Supports named opt-in lanes, compile-time exclusion of incompatible device runtimes, and explicit `cargo tree -e features` checks. Cargo feature resolver details are at [resolver](https://doc.rust-lang.org/cargo/reference/resolver.html).
- [CUDA C++ Best Practices Guide](https://docs.nvidia.com/cuda/cuda-c-best-practices-guide/index.html): account host-device transfer costs, minimize transfers, retain intermediate data on device when beneficial, batch transfers, and bound pinned-memory use. Supports resident P4/P5 dataflow and separate host-pinned accounting; it does not establish a Loki speedup.
- [CUDA Programming Guide — asynchronous error handling](https://docs.nvidia.com/cuda/cuda-programming-guide/02-basics/intro-to-cuda-cpp.html): asynchronous launch/runtime faults may only become visible at a later synchronization. Supports checked completion events before publishing results and failure injection at sync boundaries.
- [NVIDIA Compute Sanitizer](https://docs.nvidia.com/compute-sanitizer/ComputeSanitizer/): memcheck, racecheck, initcheck, and synccheck cover memory/synchronization defects; use on native receipts. These tools do not prove arithmetic or stock compatibility.
- Scout found sticky-error recovery claims only in [NVIDIA forum discussion](https://forums.developer.nvidia.com/t/cuda-errors-determine-sticky-ness/271625) and [NVIDIA cuda-python issue](https://github.com/NVIDIA/cuda-python/issues/2758); both are non-authoritative. Treat poisoning after uncertain device state as the conservative project design, not a cited CUDA specification fact.

## Validation limits

Only read-only inspection was done. No CUDA build, GPU run, test, fuzz, Lean proof, or Quint model was run by this author. Scout reports a default `cargo check -p loki-gpu` passed; that is not a GPU arithmetic or native receipt. Proposed commands in the OpenSpecs are acceptance gates, not evidence that they have passed.

## Review and roadmap convergence (2026-10-25)

- Haiku reviewed both drafts. Strict validation was PASS on initial drafts. Findings were applied: planned absent test/fuzz targets are explicitly tied to tasks that create them; P5 verification includes no-sorry/axiom audit and T11/T16/T17 closure; P7 scheduler source is marked planned on unmerged `loki/p7-chain`; legacy CUDA behavior is retained as an exclusive opt-in; the quotient backend result is explicitly the numerator before prover vanishing division; qualification tasks replace scratch-only native receipts with version-controlled scripts.
- Astra ROADMAP R1 has the correct GPU scope and IDs, but I posted a CONFLICT pending specific edits: include landed P2 commits `eb27233`/`09fd92a`; disposition legacy W4/W5/W13 flags; name the conflict between `design/PLAN.md` conceptual P3-before-P4 ordering and executable `deps.yaml` P4-before-P3 test-port order; cite Lean sorry/open obligation lines; pin only the evidenced RTX 5090/sm_120/CUDA 13.4.3/WSL2 row pending a new merged-SHA receipt; and promote current scratch/branch receipt references to versioned scripts.
- Lean owner confirms T11 `quot-inputs`, T16 quotient interpreter-safety, and T17 compiler-lowering block P5; T15 segmented MSM partition belongs only to conditional P6. However, the global release Lean tree must contain no sorry/admit/sorryAx, so the P6 sorry must be closed or its module excluded from release source/dependency closure.
- Fuzz owner sets CPU `cargo-fuzz` campaigns to at least 600 seconds per target for weekly and release campaigns, with separate longer soundness campaign; CUDA native tests/sanitizers remain the GPU receipt owner. The GPU task names deterministic native differential/fault suites and does not claim CUDA+ASan.
- Perf confirms GPU figures remain informational absent a stable qualified runner. Current legacy features stay separate from the new backend. W12 is PK disk-cache work; report cold/warm cache intervals rather than call it a CPU phase.
- ROADMAP DRAFT R2 was ACKed for GPU. R2 contains the agreed P2 prerequisite, executable P4→P3 ordering, P5/P7 gates, exact CUDA evidence scope, legacy feature disposition, and native receipt-script promotion task; W12 remains deferred under perf/Lean ownership pending the exact ref.
- Final paired review raised three blockers: T15's MSM proof hole spans a file also needed for shipped P3 dispatch; P7's reservation token is not yet connected to allocation/launch; and native receipt paths need reproducible checked-in scripts. The drafts now require a P3/P6 Lean source split, keep P7 provisional until the reviewed reservation contract is frozen, and task versioned scripts in `deps.yaml`.
- Strict validation after the final edits: `openspec validate v1-gpu-core --strict` and `openspec validate v1-gpu-qualification --strict` both report valid. Requirement count is 20 (5 arithmetic, 5 chain, 7 feature-policy, 4 native-qualification).
