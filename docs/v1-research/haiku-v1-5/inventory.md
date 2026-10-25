# GPU area inventory (haiku-v1-5, area 5: gpu)

Snapshot taken 2026-10-25 from: Loki `~/repos/loki-proofs` main `eaf09d8`; loki-gpu `~/repos/loki-gpu` main `4a40fe9`; gpu-spec Lean `~/repos/loki-lean-wt/gpu-spec` `ac95fe8`. All citations are file:line in those checkouts. Claims marked UNVERIFIED were not re-run by me.

## 1. What exists on main

**Loki main, P1 seam (CPU-only in practice)**
- Optional features in `Cargo.toml`: `cuda` L49, `cuda-srs-registry` L50, `loki-backend-api` L63, `loki-sppark` L64-68, `loki-gpu-backend` L69-73, `loki-deterministic-lookup-order` L74.
- Optional deps: `loki-backend-api` / `loki-gpu` at `Cargo.toml` L252-259.
- Compile-time guard: `src/lib.rs` L14-17 rejects `loki-gpu-backend` together with `cuda`/`cuda-ntt`.
- Dispatcher: `src/backend/dispatch.rs` `msm` L130, `transform` L165, `try_accelerated_transform` L187 (admission: `host_transform`, `max_transform_log`, field ABI), `global()` L231, `msm_specific` L260 (feature-on arm L261, stock arm L268), `run_transform` L272 (feature-on L273, stock L284), `try_accelerated_transform` accelerator gate L335-L338.
- CPU adapter: `src/backend/cpu.rs` (303 lines), with the stock routines as terminal fallback. Module docs `src/backend/mod.rs` L1-22.
- Feature-on ABI identity: `src/backend/mod.rs` `field_abi` / `arithmetic_abi`, BLS12-381 scalar and G1 only (exact `TypeId` check).

**Loki main, legacy GPU paths (merged, W4 / W5 / W13)**
- W4 sppark MSM: `cuda/msm.cu` plus `src/poly/kzg/msm/cuda/{backend,convert,tests}.rs`. Threshold `LOKI_GPU_MSM_MIN`, default 16384 (`LOKI-GPU-MSM.md`). GPU errors fall back to CPU and disable the backend after the first error. Mutex serializes sppark streams.
- W5 GPU NTT: `cuda-ntt/` (`loki-cuda-ntt`), call sites `src/poly/domain.rs` L415, L425, L459 and `src/plonk/prover.rs` L294, L316, L685 (all `cfg(feature = "cuda-ntt")`). Results: `LOKI-GPU-NTT.md`, `LOKI-GPU-NTT-EQUALITY.csv`, `LOKI-GPU-NTT-BENCH.csv`.
- W13 SRS registry (legacy): `cuda-srs-registry` gates `src/poly/kzg/msm/cuda/backend.rs` L56-L451. Merged (`loki/w13-srs-registry` is an ancestor of main). See `LOKI-SRS-REGISTRY.md`.
- `loki-harness/Cargo.toml` L11-16: `cuda` / `cuda-srs-registry` forward to Loki; `loki-gpu-backend = ["loki/loki-gpu-backend"]`. Per `design/TEST-PLAN.md`, the harness `cuda` feature is a placeholder that builds CPU code, so it is not evidence of GPU execution.

**Not on main (unmerged Loki branches, all ahead of main)**
- `loki/w10-gpu-quotient` (14 commits, 2026-10-24): W10 coefficient / quotient CUDA path, `src/plonk/evaluation/cuda.rs`. Not in main.
- `loki/p3-srs` (10 commits), `loki/p4-ntt` (1), `loki/p5-quotient` (6), `loki/p7-chain` (18). None is an ancestor of main. `design/PLAN.md` puts P3 behind a W13 handoff and P5 behind W10 and P4.

**loki-gpu main (`crates/loki-backend-api`, `crates/loki-gpu`)**
- `loki-backend-api` traits: `MsmBackend` L402, `TransformBackend` L428, `ResidentBackend` L433, `QuotientBackend` L541, `SrsRegistry` L625, `Backend` L658. `MsmLengthPolicy::ZipTruncate` L164, `RequireEqual` L166, length rule L188-192.
- `loki-gpu` features: `default = []`, `cuda = ["dep:cc"]` (`crates/loki-gpu/Cargo.toml` L10-12). `build.rs` requires `CUDA_HOME` (L35-38), checks the nvcc root (L53-56), builds `-gencode=arch=compute_120,code=sm_120` plus `compute_80` PTX (L75-76).
- P2 transactional runtime is present and is the only functioning part: `src/runtime.rs` (3076 lines), `GpuRuntime` L680, `process()` L783, `initialize()` L835, scalar upload L1328, `try_reserve_impl` L1904, `DEFAULT_SRS_BUDGET_BYTES` L83 (4 GiB), `DEFAULT_DEVICE_HEADROOM_BYTES` L87, `DEFAULT_CHECKPOINT_BUDGET_BYTES` L90, env keys L96-99. Native driver `cuda/runtime.cu` (464 lines). FFI `src/ffi.rs` (647 lines; extern decls L151-L213+). Tests: `tests/runtime_contract.rs` (46 test fns), `tests/ffi_abi.rs` (needs `cuda`), `tests/p2_fix_regressions.rs`, `tests/review_regressions.rs`. Commit lineage: `eb27233` merge P2-fix and `09fd92a` fixes D1-D5.
- Arithmetic is still Unsupported everywhere:
  - `src/msm.rs` L20-30: `msm` returns `Err(Unsupported)` after `pair_count()`.
  - `src/ntt.rs` L14-36: `transform` and `transform_resident` return Unsupported.
  - `src/quotient.rs` L12-18: `evaluate_quotient` returns Unsupported.
  - `src/srs.rs` (155 lines): every registry op returns Unsupported (budget env `LOKI_GPU_SRS_BUDGET_BYTES` L16).
  - `cuda/msm.cu` L4-20: `loki_gpu_msm_v1` returns `LOKI_GPU_UNSUPPORTED` (`cuda/ntt.cu` and `cuda/quotient.cu` are the same stubs).
  - `crates/loki-gpu/src/lib.rs` L1-9 doc says arithmetic remains Unsupported; `capabilities()` L38-45 never advertises an arithmetic op.
- Consequence: `loki-gpu-backend` on Loki main is functionally CPU-only. The dispatcher admits no accelerator because `caps.host_msm`/`host_transform` are false, so it cannot be used as evidence of GPU execution.

**Lean spec (gpu-spec worktree)**
- `LokiLean/BackendMsm.lean` L282 `sorry` (`segmentedValue_eq_pippengerList`, OBLIGATION(msm-segmented-partition)).
- `LokiLean/BackendQuotient.lean` L182 `sorry` (`valid_program_eval_some`, OBLIGATION(quotient-interpreter-safety)).
- `LokiLean/Ntt.lean` L41 states all main results proved. Backend-specific modules for NTT/quotient/MSM are referenced in `design/TEST-PLAN.md` but not otherwise checked here.

**Design docs (loki-gpu `design/`)**
- `PLAN.md` (P1-P9 ordering, feature table, kill criteria), `TEST-PLAN.md` (gates and planned test names), `RUNTIME.md`, `QUOTIENT.md`, `MSM.md`, `NTT.md`, `RECURSION.md`, `LOKI-PATCH-PLAN.md`, `REVIEW.md` (latest review at tail: P7 reservation scope proposal, P4 runtime advice 11:47 UTC; "pending the coordinator's draft-freeze decision").

**Loki TODO register** (`TODO.md`): T22 P3 adapter (L91-93), T23 P4 (L95-97), T24 P5 (L99-101), T25 P6 (L103-105), T26 P7 (L107-109), T27 P8/P9 (L111-113), cancellation contract (L116), mixed-GPU / non-sm_120 test matrix (L144-145), Intel Level Zero detection T43 (L175-176), wgpu/native ratios (L364).

## 2. Missing for v1 (state of the GPU area)

1. P3 SRS registry: loki-gpu `srs.rs` is a stub; Loki branch `loki/p3-srs` unmerged.
2. P4 resident NTT: `ntt.rs` stub; `loki/p4-ntt` has one unmerged commit.
3. P5 quotient: `quotient.rs` stub; `loki/p5-quotient` and `loki/w10-gpu-quotient` unmerged; no evaluator/bytecode compiler on main.
4. P6 balanced MSM: not started (conditional in PLAN.md).
5. P7 chain scheduler: `loki/p7-chain` unmerged; `design/REVIEW.md` shows the reservation design still under review.
6. P8/P9: conditional, not started.
7. Feature-matrix gaps: `loki-gpu/crates/loki-gpu/tests/srs_registry.rs`, `ntt_differential.rs`, `resident_ntt.rs`, `quotient_*.rs`, `msm_differential.rs`, `scheduler_contract.rs` are planned in TEST-PLAN.md but absent. `crates/loki-gpu/fuzz/` does not exist; Loki-side `fuzz/fuzz_targets/` has only `msm_diff.rs`, `ntt_diff.rs`, `proof_parse.rs`, `proof_roundtrip.rs`, `soundness.rs`.
8. Determinism gate: `loki-deterministic-lookup-order` exists as a feature (Cargo.toml L74) but the default lookup-order drift at `src/plonk/lookup/prover.rs:427-430` is still noted as open in TEST-PLAN.md. UNVERIFIED whether P1 closed it on main.
9. Platforms: no AMD, Intel or Apple path; Intel detection is a TODO (TODO.md T43). CUDA is sm_120 only (`build.rs` L75-76). Mixed-GPU and non-sm_120 coverage is an open TODO (TODO.md L144).
10. Receipts: `docs/MERGE-GATES.md` defines RECEIPT-NATIVE (`scripts/verify-main.sh --tier native`, heavy.lock then gpu.lock). Source-ACK'd P3/P4/P5/P7 had not reached main as of the doc, because native receipts were blocked.

## 3. Verification run here

- `cd ~/repos/loki-gpu && CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=~/repos/loki-wt/v1/.target-haiku-v1-5 cargo check -p loki-gpu` exited 0 (default features, no CUDA). Log: `/tmp/claude-1000/-home-charles/9dfb451b-c903-4483-a9b8-656ad9ac91de/scratchpad/v1lab/haiku5-check.log`.
- No tests, benches or CUDA builds were run (lab rule: cargo check only).

## 4. Open flags for luna-v1-5

- Branch state: W10 and P3-P7 Loki branches are ahead of main but not merged; any v1 spec that cites "current" GPU code must cite the branch name too.
- The loki-gpu `design/REVIEW.md` says the P7 reservation design is still unfrozen. Specs that depend on it should be marked as depending on an open review.
