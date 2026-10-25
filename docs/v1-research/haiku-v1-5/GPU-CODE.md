# GPU-CODE: code state of loki-gpu P3-P9 and Loki GPU integration (haiku-v1-5)

Verified by reading source at: Loki main eaf09d8, loki-gpu main 4a40fe9. Unverified items are marked.

## Implemented and on main
- loki-gpu P2 transactional runtime (commits eb27233 merge P2-fix, 09fd92a D1-D5 fixes): `crates/loki-gpu/src/runtime.rs` L680 `GpuRuntime`, L783 `process()`, L835 `initialize()`, L1328 `upload_scalars_cancellable`, L1904 `try_reserve_impl`; defaults L83-99 (4 GiB SRS budget, 2 GiB headroom, 1 GiB checkpoint budget, env keys). Native driver `cuda/runtime.cu` (464 lines). Rust FFI `src/ffi.rs`.
- Build: `crates/loki-gpu/build.rs` requires CUDA_HOME (L35-38), checks nvcc root (L53-56), sm_120 SASS + compute_80 PTX (L75-76). `cuda` feature in `crates/loki-gpu/Cargo.toml` L12.
- Loki P1 seam: `src/backend/dispatch.rs` L130 `msm`, L165 `transform`, L187 `try_accelerated_transform`, L231 `global()`, L260 `msm_specific`; feature arms at L261/L273/L337. Admission requires caps.host_msm / host_transform and exact ABI (L142-147 region, `arithmetic_abi` / `field_abi` in `src/backend/mod.rs`).
- Loki feature graph: `Cargo.toml` L48-74 (cuda-ntt, cuda, cuda-srs-registry, loki-backend-api, loki-sppark, loki-gpu-backend, loki-deterministic-lookup-order); guard `src/lib.rs` L14-17.
- Legacy merged GPU code: W4 sppark MSM (`cuda/msm.cu`, `src/poly/kzg/msm/cuda/`), W5 cuda-ntt (`cuda-ntt/`, call sites `src/poly/domain.rs` L415/425/459, `src/plonk/prover.rs` L294/316/685), W13 SRS registry under `cuda-srs-registry`.

## Stubs (Unsupported, no arithmetic)
- `crates/loki-gpu/src/msm.rs` L20-30, `src/ntt.rs` L14-36, `src/quotient.rs` L12-18, `src/srs.rs` (155 lines, budget env L16). All return `BackendError::unsupported`.
- `crates/loki-gpu/cuda/msm.cu` L4-20 returns LOKI_GPU_UNSUPPORTED; ntt.cu and quotient.cu same.
- `capabilities()` (`src/lib.rs` L38-45) advertises no arithmetic op, so the dispatcher never offers work to loki-gpu. `loki-gpu-backend` is CPU-only in practice.

## Not on main
- P3: `loki/p3-srs` (10 commits ahead of main). Blocked on W13 handoff per PLAN.md; W13 is merged on main.
- P4: `loki/p4-ntt` (1 commit). P5: `loki/p5-quotient` (6) and `loki/w10-gpu-quotient` (14, not merged). P7: `loki/p7-chain` (18).
- P6, P8, P9: not started (conditional in design/PLAN.md).

## Design gaps
- design/REVIEW.md tail: P7 reservation scope proposal is unfrozen; open refinements listed (poison idempotence, per-job Dispatcher policy, bounded scope records). Specs depending on it must be marked provisional.
- design/PLAN.md P5 says the quotient returns a numerator and the vanishing division must not be applied twice; this boundary is not yet specified in code.
- Lean gap: gpu-spec `LokiLean/BackendMsm.lean` L282 and `BackendQuotient.lean` L182 contain `sorry`.

## Requirement-relevant facts for v1 specs
- Feature matrix: `cuda` is sm_120 only; no AMD, Intel or Apple path (TODO.md T43 Intel Level Zero detection is open, L175-176).
- Mutual exclusion of loki-gpu-backend vs legacy cuda features is enforced (src/lib.rs L14-17).
- Default features must not activate any CUDA dependency (loki-gpu `default = []`; Loki `Cargo.toml` L48-49 optional).
