# Tasks

## 1. Feature and platform matrix

- [ ] 1.1 Publish the complete opt-in feature matrix for neutral API, legacy sppark, new CUDA backend, and P3/P4/P5 features; compile-fail mutually exclusive selections. Acceptance: `cargo tree -e features` confirms no GPU dependency for defaults and `cargo check --features loki-gpu-backend,cuda` plus `cargo check --features loki-gpu-backend,cuda-ntt` fail at the guard.
- [ ] 1.2 Add the exact default/native/WASM/mobile target feature graph checks in the compat-api script. Acceptance: `scripts/compat-api/check.sh` passes with stock 0.8.3 pinned and reports GPU features absent for default, `wasm32-unknown-unknown`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `aarch64-linux-android` CPU configurations.
- [ ] 1.3 Freeze the supported CUDA hardware/toolkit matrix from actual native receipts; initially do not claim beyond measured rows. Acceptance: matrix names GPU, compute capability, driver/toolkit, OS, Rust toolchain, feature set, and test receipt SHA, and unsupported runtime combinations return `Unsupported`.
- [ ] 1.4 Derive and test the SRS/device/staging/checkpoint limits and T07 live-buffer accounting; retain 4 GiB SRS, 2 GiB device headroom, 128 MiB pinned staging, and 1 GiB checkpoints as defaults unless a reviewed change supplies evidence. Acceptance: `cd loki-gpu && cargo test -p loki-gpu --test runtime_contract && cargo test -p loki-gpu --test scheduler_contract` proves over-budget requests fail before publication and records high-water.
- [ ] 1.5 Keep AMD HIP, Intel SYCL/Level Zero, Apple Metal, mobile Metal/Vulkan, WebGPU/WGSL, and multi-GPU as explicit post-v1 TODOs with a documented future acceleration path. Acceptance: `docs/PLATFORMS.md` and `docs/GPU.md` contain a target table with no unreceipted target marked supported.
- [ ] 1.6 Preserve separate behavior notes for legacy `cuda`, `cuda-ntt`, `cuda-srs-registry`, and `loki-sppark`, including MSM threshold and NTT eligibility/fallback; document their coexistence with the new backend only as an exclusive alternative. Acceptance: the feature table matches `Cargo.toml` and legacy-only W2 tests pass without the new runtime.

## 2. CPU merge and native receipt workflow

- [ ] 2.1 Keep `MERGE-CPU` and `RECEIPT-NATIVE` separate in CI configuration and dependency metadata. Acceptance: `scripts/verify-main.sh --tier cpu` passes without a GPU runner or GPU lock; merge records show fake-driver gates.
- [ ] 2.2 Define GPU-required test mode to assert backend attempt/completion counters for every expected operation and fail on any fallback. Acceptance: `cd loki-gpu && cargo test -p loki-gpu --features cuda --test ntt_differential && cargo test -p loki-gpu --features cuda --test quotient_differential && cargo test -p loki-gpu --test scheduler_contract` reports positive dispatch and zero fallback.
- [ ] 2.3 Configure native fault and sanitizer jobs for allocation, transfer, launch, synchronization, context loss, cancellation, and stale-generation cases. Acceptance: `scripts/verify-main.sh --tier native` runs package receipt scripts with compute-sanitizer and records results against the exact merged SHA.
- [ ] 2.4 Make failed or missing native receipts block release/push promotion but not CPU-qualified merge. Acceptance: dependency-graph and release CI tests demonstrate a failed native receipt blocks promotion and leaves MERGE-CPU eligibility unchanged.
- [ ] 2.5 Replace scratch-only native receipt references in `deps.yaml` with version-controlled, reproducible package scripts and stable output paths. Acceptance: `scripts/loki-dag check` resolves each P3/P4/P5/P7 `native_receipt` path in the merged tree and each script emits a receipt bound to the merged SHA.

## 3. Compatibility, documentation, and final validation

- [ ] 3.1 Consume the area-2 stock fixture schema and compat-api feature matrix; ensure GPU config stays opt-in and uses stock 0.8.3 for both-direction verification. Acceptance: area-2 fixtures pass with VK/PK/SRS byte checks and fixed-order proof-byte controls; `cargo tree -e features` confirms no default GPU feature.
- [ ] 3.2 Document the feature matrix, limits, fallback and cancellation semantics, native receipt requirements, supported hardware, and post-v1 target path in GPU/platform docs and rustdoc. Acceptance: `cargo doc --no-deps --features loki-gpu-backend` succeeds and the docs-release coverage table links each feature to tests, fuzz, Quint, Lean, and receipt evidence.
- [ ] 3.3 Run strict OpenSpec validation and incorporate area review. Acceptance: `openspec validate v1-gpu-qualification --strict` passes and the area owner posts the final change IDs and requirement count.

