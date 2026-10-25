# Spec Delta

## Purpose

Defines the user-visible feature, capability, resource, and platform policy for Loki GPU acceleration so optional device support cannot alter CPU defaults or silently exceed runtime limits.

## ADDED Requirements

### Requirement: Existing legacy CUDA features retain their opt-in behavior
The v1 feature matrix SHALL retain the existing `cuda`, `cuda-ntt`, `cuda-srs-registry`, and `loki-sppark` opt-in lanes as the compatibility path while documenting their thresholds and fallback behavior separately from the new backend.

#### Scenario: Select the existing legacy CUDA path
- **WHEN** a consumer enables one or both legacy CUDA operation features without `loki-gpu-backend`
- **THEN** Loki retains the documented legacy sppark MSM/NTT/SRS behavior and CPU fallback without selecting or linking a second `loki-gpu` device runtime. Verification: `cargo tree -e features` and `cargo test --features loki-sppark`; source artifact: `Cargo.toml:48-50,64-73` and `docs/GPU.md:48-68,100-219`.

### Requirement: GPU acceleration remains opt-in and mutually exclusive
The stock default feature set SHALL remain unchanged and free of GPU dependencies; the legacy sppark and new `loki-gpu` runtimes MUST NOT be selected together.

#### Scenario: Build stock defaults
- **WHEN** a consumer builds the crate with its default features
- **THEN** only the stock default feature set is enabled and no CUDA, sppark, or `loki-gpu` dependency is activated. Verification: `cargo tree -e features` and `cargo test --test backend_default_stock`; source artifact: `Cargo.toml:45-74,252-260` and `src/backend/mod.rs:1-22`.

#### Scenario: Select incompatible runtimes
- **WHEN** a consumer enables `loki-gpu-backend` together with legacy `cuda` or `cuda-ntt`
- **THEN** compilation fails with the explicit accelerator exclusivity guard. Verification: `cargo check --features loki-gpu-backend,cuda` and `cargo check --features loki-gpu-backend,cuda-ntt` are expected-failure gates; source artifact: `src/lib.rs:14-17` and `Cargo.toml:48-73`.

### Requirement: Capability advertisement matches implemented healthy operations
The backend SHALL advertise only operations that are implemented for the current arithmetic ABI and healthy device generation; every other operation MUST return `Unsupported` and remain eligible for safe CPU fallback.

#### Scenario: Runtime has no supported device operation
- **WHEN** the runtime is uninitialized, unhealthy, or lacks an implemented operation
- **THEN** capability reporting does not claim acceleration and Loki uses the CPU reference path. Verification: `cargo test -p loki-gpu --test runtime_contract` and Loki `cargo test --test backend_cpu_equivalence`; source artifact: `loki-gpu/crates/loki-gpu/src/lib.rs:33-44`, `loki-gpu/crates/loki-gpu/src/msm.rs:17-29`, and `src/backend/dispatch.rs:128-159`.

#### Scenario: Operation is unsupported by this arithmetic ABI
- **WHEN** a request uses a field, curve, domain, or operation not advertised by the backend
- **THEN** it returns a typed `Unsupported` result before mutating caller data and the CPU route remains available. Verification: `cargo test -p loki-backend-api` and `cargo test --test backend_cpu_equivalence`; source artifact: `src/backend/mod.rs:24-53`, `src/backend/dispatch.rs:140-159,165-224`, and `loki-gpu/crates/loki-gpu/src/msm.rs:17-29`.

### Requirement: Conditional optimizations require reviewed re-entry
P6 skew-robust MSM, P8 lookup preparation, and P9 batching, precomputation, or concurrent-heavy scheduling MUST remain outside the initial v1 feature set unless a separate reviewed scope change admits the feature with complete correctness, Lean, fuzz, Quint, resource, native, documentation, and performance gates.

#### Scenario: Inspect the initial v1 feature set
- **WHEN** the v1 feature matrix is generated before any approved re-entry
- **THEN** `cuda-msm-balanced`, `cuda-lookup-prepare`, and P9 experimental configuration are labeled conditional and are not enabled by default or represented as required v1 operations. Verification: `cargo tree -e features` and `scripts/verify-main.sh --tier cpu`; source artifact: `loki-gpu/design/PLAN.md:116-168` and `Cargo.toml:45-74`.

### Requirement: Device, pinned-host, checkpoint, and SRS budgets are bounded
The GPU runtime SHALL reserve and account allocations before launch, enforce a shared device budget, cap pinned staging at 128 MiB, cap retained host checkpoints at 1 GiB, and default persistent SRS residency to at most 4 GiB.

#### Scenario: Reserve within configured budgets
- **WHEN** an operation requests SRS, input/output, scratch, staging, or checkpoint memory
- **THEN** it obtains a generation-valid reservation before allocation, reports live/reserved high-water, and never exceeds the relevant hard cap. Verification: `cargo test -p loki-gpu --test runtime_contract --test scheduler_contract` plus `cargo fuzz run residency_state`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:64-100,398-448` and `loki-gpu/crates/loki-gpu/src/srs.rs:15-20,27-70`.

#### Scenario: Reject an over-budget request
- **WHEN** an SRS entry, staging request, checkpoint, or device allocation would exceed its configured budget
- **THEN** the request fails before launch and no partial resident handle is published. Verification: `cargo test -p loki-gpu --test runtime_contract --test srs_registry`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:64-100,439-448` and `loki-gpu/crates/loki-gpu/src/srs.rs:27-70`.

### Requirement: GPU platform scope is explicit
The v1 acceleration path SHALL support only the CUDA target matrix qualified by native receipts; AMD HIP, Intel SYCL/Level Zero, Apple Metal, mobile GPU, WebGPU/WGSL, and multi-GPU execution MUST remain documented post-v1 TODOs unless an explicit reviewed scope change adds their complete correctness and receipt gates.

#### Scenario: Run on a qualified CUDA device
- **WHEN** the runtime is initialized on a listed CUDA hardware/toolkit combination
- **THEN** it advertises only implemented capabilities for that device and unsupported combinations fail closed to the CPU path. Verification: `cargo test -p loki-gpu --features cuda --test runtime_contract` and `scripts/verify-main.sh --tier native`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:81-99` and `docs/PLATFORMS.md:1-13,402-416`.

#### Scenario: Build CPU-only WASM and mobile targets
- **WHEN** the CPU-only compatibility jobs target wasm32 browser/Node or iOS/Android
- **THEN** they do not require CUDA, Metal, Vulkan, or WebGPU SDKs, and the acceleration feature is not enabled. Verification: `cargo tree -e features --target wasm32-unknown-unknown`, `cargo tree -e features --target aarch64-apple-ios`, and `cargo tree -e features --target aarch64-linux-android`; platform-owned compatibility jobs additionally run these targets. Source artifact: `Cargo.toml:45-74,252-260` and `docs/PLATFORMS.md:264-337,402-416`.

### Requirement: GPU documentation maps every shipped feature to QA evidence
The GPU release documentation SHALL map each shipped feature to its supported operation, resource and failure limits, differential fuzz targets, Quint/MBT coverage, Lean contract and status, compatibility fixture, native receipt, and usage guidance.

#### Scenario: Review a shipped GPU feature row
- **WHEN** an in-scope P3, P4, P5, or P7 feature is listed for release
- **THEN** the documentation links its feature flag to its test/fuzz target, Quint model/replay, Lean contract and proof status, stock fixture, native receipt command, fallback behavior, and resource cap. Verification: `scripts/verify-main.sh --tier native` plus the docs-release feature coverage audit; source artifact: `docs/GPU.md:45-74,553-580` and `loki-gpu/design/TEST-PLAN.md:47-105`.

