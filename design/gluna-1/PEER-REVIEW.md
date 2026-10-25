# Peer review: gluna-7 runtime and SRS draft

Review scope: `crates/loki-gpu/src/runtime.rs`, `src/srs.rs`, and
`design/RUNTIME.md` as read at 2026-10-24. The CUDA implementation is explicitly
stubbed, so findings distinguish current contract mismatches from gates for a
future implementation.

## Findings

### MEDIUM — runtime design assigns allocation ownership to the wrong layer

`design/RUNTIME.md:44-55` says resident buffers own allocations and are released
after the last operation completes. The public API in
`crates/loki-backend-api/src/lib.rs` defines `ResidentBuffer<T>` as a non-owning
typed handle with no `Drop`; lifetime is managed by the backend registry and
released with explicit `release_scalars`, `release_points`, or `release_srs`.
The implementation should not imply that dropping a handle frees the device
allocation. That ambiguity can cause either leaks or premature frees when a
buffer remains referenced by an SRS lease or queued internal stream work.

**Status: RESOLVED.** Re-read current `design/RUNTIME.md`: it now says that
`ResidentBuffer` is a non-owning opaque handle, the runtime registry owns
allocations, dropping a handle does not free the allocation, and explicit
release makes it eligible for deferred destruction after operations and SRS
leases complete. This matches the API's explicit release methods and internal
completion lifetime.

**Fix:** say the runtime owns allocations; API buffer values are non-owning
handles. Require explicit release only after all operations using the handle
have retired. Keep the backend's allocation registry as the authority for
generation, element type, length, byte count, and in-flight-use validation.

### MEDIUM — unavailable snapshot encodes unknown live memory as zero

`crates/loki-gpu/src/runtime.rs:36-56` documents that unknown memory remains
`None`, but `RuntimeSnapshot::unavailable()` sets `live_bytes` and
`reserved_bytes` to zero. The type is exposed through
`GpuSrsRegistry::runtime_snapshot()` at `src/srs.rs:76-80`. Although
`healthy=false` and `hard_budget_bytes=None` must prevent admission, a caller
could still interpret the two zero counters as measured empty usage.

**Status: RESOLVED.** Re-read current `crates/loki-gpu/src/runtime.rs`:
`RuntimeSnapshot::{live_bytes,reserved_bytes}` are `Option<u64>`, and
`unavailable()` sets both to `None`. The returned unknown state no longer
claims zero live or reserved memory.

**Fix:** make live and reserved bytes optional in this internal snapshot, or
represent unavailable separately from a healthy snapshot. Before mapping it to
`loki_backend_api::RuntimeSnapshot`, require a healthy initialized generation
and fully accounted values; otherwise return `Operation::Runtime` unsupported.

## Implementation gates, not current stub defects

- `GpuSrsRegistry::acquire_srs` currently returns `Unsupported`, so it cannot
  produce an unsound hit. Before implementing it, verify generation, curve and
  wire ABI, basis, length, and actual point contents against any cache key;
  validate curve/subgroup membership before publishing a resident entry. The
  current `validate_metadata` helper explicitly checks only count and budget.
- Keep active SRS leases pinned until all dependent internal events retire.
  Evict only zero-lease entries, and retain uncertain allocations on context
  loss as `design/RUNTIME.md:145-156` requires.
- The one-copy/two-compute stream plan and 4 GiB persistent-SRS budget are
  design assumptions, not measured concurrency or total-device-memory limits.
  Keep the one-heavy-proof admission rule until full k19/k20 memory and latency
  measurements justify overlap.

## Disposition

The runtime lock ordering, no borrowed async DMA boundary, generation tagging,
owned staging, nonblocking reservation surface, and fail-closed unsupported
stubs align with the API and clean-room requirements. Both medium findings
are resolved in the current shared draft. No native CUDA behavior was run or
verified in this review.

## Lean backend review

Reviewed read-only: `/home/charles/repos/loki-lean-wt/gpu-spec/LokiLean/Backend.lean`
and the referenced contracts in `LokiLean/Refinement.lean`. Ran
`PATH=/home/charles/.elan/bin:$PATH lake build LokiLean.Backend` from the
`gpu-spec` worktree; it exited 0. Lean reported warnings in imported modules
and a short copyright-header warning for `Backend.lean`; no build errors.

**Disposition: no blocking findings for the requested contract review.**

- Error preservation is stated on every failure in `Contract`, then consumed
  by `dispatch_public_eq_reference` and `error_preserves_public_state`.
  `PublicState` is documented to include input/checkpoint values, outputs, and
  transcript state; `RuntimeState` is explicitly allowed to change. The
  dispatcher falls back using the preserved public state.
- Successful calls must match both reference output and public-state
  transition. The small `successfulCall` example exercises this branch; the
  `unsupportedCall` example exercises fallback, and `mutatingFailureCall`
  demonstrates that fallback can diverge without the preservation contract.
  `OperationContracts.toRefines` builds the component record, while
  `InterfaceContracts` separately requires `SameOrder`; the event, byte, and
  modeled-verifier equality results use those hypotheses. The file also
  includes `cpuBackend_refines` as a concrete witness for the operation/order
  contract, so the composition is not merely an uninhabited implication.
- Rust/transcript scope is explicit in the module header: Loki owns transcript
  writes and ordering, async calls are complete only after effects are known,
  and the module does not prove the Rust adapter, FFI, CUDA kernel, wire
  encoding, or stock verifier implementation. `OBLIGATION(quot-inputs)` states
  the exact coset/challenge binding needed to connect the abstract numerator
  function to the actual proof; differential and independent stock-verifier
  gates remain external evidence.

No Lean files were changed by this review. The API's quotient result is the
numerator vector consumed before the existing division and inverse-coset tail,
which matches the `Refinement.prove` and `quotient_via_backend` boundary.
