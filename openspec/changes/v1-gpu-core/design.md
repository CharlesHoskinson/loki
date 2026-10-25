# Design

## Context

See `proposal.md` and the `gpu-arithmetic` / `gpu-chain` contracts. On the checked-out Loki base, P1 typed dispatch exists but `loki-gpu` MSM, NTT, quotient, and SRS methods return `Unsupported` (`loki-gpu/crates/loki-gpu/src/{msm,ntt,quotient,srs}.rs`). The P2 runtime is on loki-gpu main at `eb27233` / `09fd92a`; P3–P7 and W10 integration branches are not merged. The current executable train (`deps.yaml:11-39`) orders P4 before P3, makes P5 depend on P3/P4/W10, and permits P7 to merge after P4; P7 full-chain acceptance still depends on P3/P4/P5. This differs from the conceptual PLAN order table, which lists P3 before P4 and P3 after P1/P2/W13 (`loki-gpu/design/PLAN.md:25-40,75-100`). Use the checked-in train for branch/test-port sequencing, and record the discrepancy as an integration decision until the plan owner reconciles it.

## Goals / Non-Goals

**Goals:**

- Deliver P3, P4, P5, and P7 only after their CPU, Lean, and native evidence is complete.
- Preserve Loki ownership of transcript events, proof encoding, and CPU fallback.
- Keep device memory bounded and every published result transactional.
- Make stock compatibility and whole-chain qualification explicit at feature-enabled boundaries.

**Non-Goals:**

- Admit P6 skew-robust MSM, P8 lookup preparation, or P9 batching/concurrent-heavy/precompute into initial v1 scope. Re-entry requires a separate reviewed scope update and all correctness/Lean/fuzz/Quint/native/performance gates.
- Add a GPU target beyond the qualified CUDA matrix, multi-GPU scheduling, a new public async API, or default acceleration.

## Decisions

1. **Sequence around the actual merge DAG.** Integrate P4 first, then P3, then P5 after both P3/P4 and the W10 handoff. P7 code may be mergeable after P4, but no v1 chain claim passes until P3/P4/P5 are integrated. Keep branch work rebased and use fake-driver tests for merge qualification after P4 changes runtime test seams. This follows `deps.yaml:11-39` and `loki-gpu/design/PLAN.md:75-142`.

2. **Use typed capability checks and per-operation fallback.** The backend advertises only implemented operations for a supported BLS12-381 ABI. Unsupported capability or an error with unchanged host state may use the stock CPU implementation. A poisoned generation is disabled. Cancellation and loss of data without a complete host checkpoint terminate the current proof and never trigger CPU retry. This avoids inferring safety from generic traits or partial device output (`src/backend/dispatch.rs:107-159`; `loki-gpu/design/RUNTIME.md:81-92`).

3. **Keep resident data and reservations authoritative.** Validate exact requested SRS prefixes, basis and device generation; hold leases until work retires; reject over-budget requests before upload or launch. Scheduler admission uses immediate reservation tokens, not stale snapshots. Retain one-heavy-job policy until P9 is separately admitted. The default caps are 4 GiB persistent SRS, 128 MiB pinned staging, 1 GiB retained host checkpoints, and 2 GiB CUDA headroom (`loki-gpu/crates/loki-gpu/src/runtime.rs:64-100`).

4. **Separate value equality from proof interoperability and bytes.** Require exact CPU/GPU field/group outputs and controlled-order proof bytes. Also require proof verification in both directions, equal VK/SRS bytes, and stock default behavior from the area 2 fixtures. Do not treat randomized stock HashMap lookup order as a GPU correctness failure or claim universal byte equality from uncontrolled lookup fixtures (`loki-gpu/design/TEST-PLAN.md:15-17,23-31,33-45`).

5. **Keep CPU merge gates and native receipts separate.** `scripts/verify-main.sh --tier cpu` plus fake-driver checks gate merge without a GPU lock. `scripts/verify-main.sh --tier native` runs post-merge against the exact merged revision and gates release/push promotion (`docs/MERGE-GATES.md:5-20`). GPU-required tests assert positive dispatch and zero fallback.

6. **Defer platform acceleration explicitly.** The v1 GPU feature targets only listed/qualified CUDA combinations. AMD HIP, Intel SYCL/Level Zero, Apple Metal, mobile GPU, WebGPU/WGSL, and multi-GPU remain post-v1. WASM and iOS/Android CPU compatibility jobs must remain independent of GPU features and SDKs (`docs/PLATFORMS.md:1-13,264-337,402-416`).

7. **Keep legacy CUDA as a separate existing opt-in.** Preserve the merged `cuda`, `cuda-ntt`, `cuda-srs-registry`, and `loki-sppark` path and its documented threshold/fallback behavior; the new feature path does not silently replace or combine it. The compile-time guard remains the compatibility boundary (`Cargo.toml:48-73`, `src/lib.rs:11-18`, `docs/GPU.md:48-68,100-219`).

8. **Assign cross-area acceptance contracts once.** Area 2 owns stock proof/key fixture schema and bidirectional commands; area 6 owns fuzz campaign policy; area 7 owns Quint model and MBT trace schema; area 8 owns Lean module merge and release-SHA sync; area 9 owns measured performance thresholds; area 10 owns release CI and docs orchestration. GPU requirements consume those artifacts and do not duplicate their global thresholds.

## Risks / Trade-offs

- [P3 cache identity, leases, or memory accounting is unsound] → Keep exact-content mutation/prefix/generation tests and Lean cache contracts as blockers.
- [P4 root, coset, order, or normalization differs] → Compare every output element to CPU and use direct small DFT negative controls; round-trip alone does not qualify the transform.
- [P5 lowering omits an ordered term or computes the wrong numerator] → Require independent CPU bytecode interpreter, malformed bytecode validation, real circuit differential tests, and closed Lean safety/lowering obligations before enabling the feature.
- [Device fault loses inputs or leaks unpublished allocations] → Require retained checkpoints, failure injection at each async boundary, fake-driver CPU merge gates, and revision-bound native sanitizer receipts.
- [GPU runner scarcity delays merge] → Keep native receipts post-merge and treat a failed/missing receipt as a release/push blocker.
- [Shared or mismatched benchmarks imply a false gain] → Area 9 owns runner/sample/statistic policy; report GPU data as informational until that contract qualifies a stable runner.

## Migration Plan

1. Merge P4, P3, W10/P5, and P7 in the declared DAG with `MERGE-CPU` green after each.
2. Keep all new features opt-in and preserve the legacy sppark feature path as a separate, mutually exclusive choice.
3. Queue `RECEIPT-NATIVE` against the integrated merge SHA and collect stock, differential, sanitizer, hardware, and memory evidence.
4. Enable the new backend only for documented supported CUDA combinations after all receipts pass. Roll back by disabling the opt-in feature; default CPU behavior remains available.

