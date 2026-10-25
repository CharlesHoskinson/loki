# Peer review: backend API and Loki patch plan

Reviewer: gluna-8. Scope: read-only review of `crates/loki-backend-api/src/lib.rs`
and `design/LOKI-PATCH-PLAN.md`. Review target is the current draft in the shared
workspace; line numbers below are from the reviewed revision.

## Findings

### Major: supported field/curve identity is not represented in capabilities

- **Location:** `crates/loki-backend-api/src/lib.rs:96-113`; the patch plan
  promises pre-dispatch type validation at `design/LOKI-PATCH-PLAN.md:120-123`.
- **Issue:** `BackendCapabilities` reports operation booleans and transform
  limits, but no supported scalar-field or curve identifiers. The proposed GPU
  module is specifically for BLS12-381, while the public traits are generic over
  every `PrimeField` and `Curve`. The patch plan therefore promises a validation
  decision that the API cannot express for a generic dispatcher.
- **Fix:** add stable field and curve/ABI identifiers to the capability or
  operation query, or define a type-specific backend registration that can
  reject unsupported `F,G` before any device call. Do not infer support from
  `BackendCapabilities::default()` or a type name. Tie any accepted identifier
  to the exact scalar/point wire encoding.
- **Status:** resolved in the current draft. `BackendCapabilities` now includes
  `supported_arithmetic: Vec<ArithmeticAbi>` and
  `supports_arithmetic(ArithmeticAbi)`; the patch plan requires the dispatcher
  to map concrete Rust types to stable IDs before GPU dispatch.

### Major: an SRS lease is publicly constructible metadata

- **Location:** `crates/loki-backend-api/src/lib.rs:545-550`.
- **Issue:** every `SrsLease` field is public, including the allocation handle,
  generation, and lease ID. A caller can construct or alter a lease value. The
  backend must not treat those fields alone as authority to use or release a
  resident allocation.
- **Fix:** make lease fields private and provide read-only accessors plus an
  explicitly untrusted metadata constructor if downstream backend
  implementations need to construct the value. On every SRS operation and
  release, validate lease ID, generation, key/content identity, element type,
  length, and allocation against the live registry before passing any handle
  to native code. A forged or stale lease must leave the registry unchanged.
- **Status:** resolved in the current draft. `SrsLease` fields are private,
  accessor methods are provided, and `from_untrusted_metadata` explicitly
  does not establish a live lease. The API docs require registry validation.

### Moderate: transform root meaning is underspecified

- **Location:** `crates/loki-backend-api/src/lib.rs:236-257`.
- **Issue:** `TransformSpec` carries both `root` and `direction`, but the API
  docs do not say whether `root` is always the canonical forward root or the
  root already selected for the requested direction. An inverse implementation
  could invert twice or use the forward root. This is material because the
  existing NTT contracts require pinning the exact domain root and scaling once.
- **Fix:** specify one interpretation in the type docs and examples. Recommended:
  `root` is the canonical forward root for `source_log_size`; inverse mode uses
  its inverse, and any output-size/coset extension has its explicitly documented
  root derivation. Validate that the root has the required order before work.
- **Status:** resolved in the current draft. The API defines `root` as the
  canonical forward root, states which extent applies by direction, and requires
  exact order.

### Moderate: the W10 commitment seam is outside the stated base revision

- **Location:** `design/LOKI-PATCH-PLAN.md:40-43,62-73`.
- **Issue:** the heading says the exact seams target Loki `877114d`, but the
  argument-commit feature and its call-site cfgs exist only in the W10 worktree
  at this review; the current `loki-proofs` main has the `commit_lagrange`
  call sites without those feature-gated branches. The section is correctly
  labeled W10, but an implementer following the stated base revision could
  search for or patch the wrong state.
- **Fix:** label this as a follow-up against the precise W10 commit/branch, list
  its source revision and exact lines, and state whether it is conditional on
  W10 merging or will be rebased as part of this integration.
- **Status:** resolved in the current draft. The patch plan now pins the W10
  commit and states the merge/rebase condition explicitly.

## Resolved during review

- `ResidentBackend::generation` now returns `Result`, so a CUDA-free backend
  does not invent a valid device generation.
- The public v1 resident API is blocking. Uploads stage owned data before DMA;
  downloads return a completed vector. This avoids making async safety depend
  on `Drop` running or on a borrowed caller slice remaining alive.
- `FailureEffect` now represents only caller-visible preservation and poisoned
  generation state. The patch plan separately requires a retained host
  checkpoint before CPU fallback.
- `ResidentBuffer` metadata is private and explicitly untrusted at construction;
  implementations must validate handles against their allocation registry.
- The patch plan identifies the existing feature-off lookup-order drift in
  `877114d` and requires restoring stock traversal before claiming byte identity.

The abstract Lean bridge must not be presented as proof that any native CUDA
kernel, wire encoding, or asynchronous lifetime is correct. Those remain
implementation obligations checked by differential tests and stock-verifier
acceptance.
