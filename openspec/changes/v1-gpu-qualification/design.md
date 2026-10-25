# Design

## Context

See `proposal.md` and the `gpu-feature-policy` / `gpu-native-qualification` contracts. Loki has legacy `cuda`, `cuda-ntt`, and `cuda-srs-registry` features plus the mutually exclusive `loki-gpu-backend` feature (`Cargo.toml:45-74`, `src/lib.rs:11-18`). The latter currently exposes the P2 runtime but no arithmetic capabilities. The repository has an explicit CPU merge tier and post-merge native receipt tier (`docs/MERGE-GATES.md:5-20`).

## Goals / Non-Goals

**Goals:**

- Give consumers a stable distinction between legacy sppark CUDA and the new backend.
- Prevent GPU features and SDK dependencies from entering default, WASM, and mobile CPU builds.
- Make resource budgets and supported CUDA hardware/toolkit combinations visible and testable.
- Preserve CPU merge progress while requiring native evidence before release or push.

**Non-Goals:**

- Change stock 0.8.3 API/default/proof behavior, establish a general cross-vendor backend API, or qualify unimplemented hardware.
- Make `cargo test --all-features` a valid CI command when mutually exclusive runtime features are set.

## Decisions

1. **Keep explicit feature lanes.** `loki-backend-api` is the neutral seam; `loki-sppark` selects the legacy runtime; `loki-gpu-backend` selects the new CUDA runtime. The new and legacy lanes fail at compile time when unified together. The published feature matrix distinguishes existing legacy flags from proposed new-backend flags:

   | Feature | Lane / status | Role |
   |---|---|---|
   | `cuda`, `cuda-ntt`, `cuda-srs-registry` | Legacy, existing, opt-in | sppark MSM, NTT, and W13 registry |
   | `loki-backend-api` | Neutral, existing, opt-in | typed backend seam and CPU reference |
   | `loki-sppark` | Legacy selector, existing, opt-in | selects legacy `cuda` + `cuda-ntt` |
   | `loki-gpu-backend` | New CUDA runtime, existing, opt-in | P2 runtime shell; arithmetic not advertised until packages qualify |
   | `loki-gpu-srs-registry` | New lane, planned P3 | exact-content SRS lease registry |
   | `cuda-resident` | New lane, planned P4 | resident transforms/cosets |
   | `loki-argument-commit` | backend-neutral W10 strategy | coefficient-basis commitment selection |
   | `cuda-quotient` | New lane, planned P5 | ordered quotient interpreter |
   | `cuda-msm-balanced`, `cuda-lookup-prepare` | Conditional P6/P8, excluded initially | separate review and measured re-entry required |
   | P9 experimental configuration | Conditional P9, excluded initially | separate review; no default or release promise |

   Cargo feature unification makes silent runtime mixing unsafe; Cargo's feature rules document optional dependency behavior and feature union ([Cargo features](https://doc.rust-lang.org/cargo/reference/features.html)).

2. **Do not expand beyond the currently qualified CUDA set by inference.** Publish an explicit hardware/toolkit matrix and initially qualify only recorded combinations; runtime capability queries reject unsupported devices. Tests that use only the fake driver remain device-independent. AMD, Intel, Apple, WebGPU, mobile acceleration, and multi-GPU require a later reviewed scope with target-specific kernels, memory/error semantics, docs, and native receipts.

   The only currently evidenced new-backend row is RTX 5090 / sm_120 with CUDA 13.4.3 on WSL2. Treat it as historical evidence until RECEIPT-NATIVE re-runs it against the merged release SHA. PTX output or a compatible compute-capability query alone does not add a supported row (`loki-gpu/crates/loki-gpu/build.rs:75-76`; `docs/GPU.md:18-31`).

3. **Make memory limits numeric and visible.** Enforce the current defaults: SRS 4 GiB, device headroom 2 GiB, pinned staging 128 MiB maximum, and host checkpoints 1 GiB. Device reservations cover persistent and scratch allocations. These values are not permission to promise fit for a particular proof; qualification records actual high-water and rejects requests beyond the configured hard budget (`runtime.rs:64-100,439-448`).

4. **Use separate acceptance tiers.** MERGE-CPU runs static/API/default/fake-driver checks with no GPU lock. RECEIPT-NATIVE runs after merge on a fixed revision with hardware, sanitizer, differential, dispatch, stock-verifier, and resource evidence. Native failure blocks release/push, not the already CPU-qualified merge (`docs/MERGE-GATES.md:5-20`). NVIDIA Compute Sanitizer's memcheck, racecheck, and synccheck are selected for native lifetime/kernel checks ([Compute Sanitizer](https://docs.nvidia.com/compute-sanitizer/ComputeSanitizer/)).

5. **Keep optimization thresholds with the performance owner.** GPU reports expose operation dispatch/fallback counters, transfers, event/wall timing, and memory high-water. Area 9 owns chain comparison method and acceptance statistics. P6/P8/P9 remain out of initial v1 scope pending explicit re-entry; no kernel-only result can admit them.

6. **Preserve CPU-only target lanes.** WASM browser/Node and iOS/Android must compile and execute their stock CPU compatibility fixtures independently of CUDA availability. Metal, Vulkan, WebGPU, HIP, and SYCL are documented as post-v1 paths rather than implied by `loki-backend-api`.

## Risks / Trade-offs

- [Cargo feature unification activates incompatible runtimes] → Use named feature lanes, compile-fail combination checks, and a valid feature matrix rather than all-features CI.
- [The fixed CUDA matrix is too narrow] → Add a device only after an explicit target row passes native receipts and docs; do not claim broader portability from PTX compilation alone.
- [Configured budgets do not represent real peak usage] → Derive T07 live-buffer accounting and capture host/device high-water; treat memory accounting gaps as release blockers.
- [GPU failure evidence is unavailable before merge] → Keep MERGE-CPU runnable on fake drivers and bind native receipts to merged revisions.
- [Toolchain documentation is stale] → Have docs/release CI update the matrix with observed toolkit, driver, GPU, and supported compute capability at each receipt.

## Migration Plan

1. Preserve current default feature set and existing legacy flags.
2. Add/validate any new-backend subfeatures only with their implementation packages.
3. Run `scripts/verify-main.sh --tier cpu` before merge.
4. Run `scripts/verify-main.sh --tier native` after merge and record the exact merged SHA before v1 release/push promotion.

