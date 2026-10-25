# Proposal

## Why

Loki currently exposes several overlapping CUDA feature paths and describes AMD, Intel, Apple, WebGPU, and mobile acceleration as future work, while the P2 `loki-gpu` arithmetic skeleton has no operations advertised. A release needs an unambiguous opt-in feature matrix, resource limits, platform boundary, and native-receipt policy so acceleration cannot leak into stock defaults or block CPU merge qualification.

## What Changes

- Specify the new-backend/legacy-sppark feature matrix, compile-time mutual exclusion, per-operation capability negotiation, resource budgets, and fallback behavior.
- Preserve CUDA as the only v1 GPU implementation target; mark AMD HIP, Intel SYCL/Level Zero, Apple Metal, WebGPU, and mobile acceleration as post-v1 TODOs unless later evidence admits a scoped target.
- Define MERGE-CPU and post-merge RECEIPT-NATIVE gates, including GPU-required mode that fails on fallback and receipts bound to merged source revisions.
- Require platform-neutral CPU compatibility gates on WASM and iOS/Android without CUDA dependencies.

## Capabilities

### New Capabilities

- `gpu-feature-policy`: opt-in accelerator selection, per-operation support, resource limits, platform scope, and fallback isolation.
- `gpu-native-qualification`: merge and native receipt gates for GPU-dependent operations.

### Modified Capabilities

None.

## Impact

The feature declarations and compile-time guard in Loki `Cargo.toml`/`src/lib.rs`, `loki-gpu` runtime capability reporting, `scripts/verify-main.sh`, `deps.yaml`, W2 tests, CI configuration, and `docs/GPU.md`/`docs/PLATFORMS.md` are affected. The proposal preserves stock midnight-proofs 0.8.3 API, features, defaults, proof/VK/PK serialization and transcript semantics; compat-api and compat-proof own the mechanical comparison and bidirectional interoperability checks. WASM/browser/Node and iOS/Android CPU gates remain independent of GPU tooling. GPU-specific docs must state supported hardware, limits, fallback policy, and explicit post-v1 platform TODOs.
