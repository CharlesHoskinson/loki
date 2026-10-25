# Peer review: gluna-4 MSM design and skeleton

Review basis: read-only review of `design/MSM.md`,
`crates/loki-gpu/src/msm.rs`, `crates/loki-gpu/cuda/msm.cu`,
`crates/loki-backend-api/src/lib.rs`, and the current `design/ARCHITECTURE.md`.
No native kernel was run or reviewed as implemented; the current CUDA and Rust
entry points are explicit `Unsupported` stubs.

## Finding

- **P2 — clarify the production resident FFI seam** —
  `design/MSM.md:61`. The current v1 CUDA function accepts host byte slices and
  returns `Unsupported`; the public API's resident SRS lease is an opaque,
  generation-tagged allocation handle. The text says a future resident/async
  implementation may either add an ABI carrying a device context/stream and
  resident handle, **or** document a safe synchronous invocation contract in
  the API crate. The latter documents completion semantics but does not explain
  how the CUDA translation unit resolves an opaque lease to the backend-owned
  allocation, so implementers could accidentally route every resident MSM
  through host bytes and lose the residency property.

  **Concrete fix:** require a private resident-call seam for the CUDA module
  that passes validated allocation identities/context generation (and an
  internal device pointer only inside the private FFI boundary), even when calls
  are synchronous. Keep the byte-wire v1 symbol only for host-slice operations.
  Alternatively, state that the Rust runtime resolves the lease and invokes a
  private kernel wrapper directly, and document that wrapper's validation and
  completion contract. A stream parameter is optional for v1 if the wrapper
  blocks until completion.

## Checks that passed at design level

- The unsigned 256-bit window decomposition covers a partial top window without
  signed carry handling; the top window is initialized before lower-window
  doublings.
- Bucket segments are specified as disjoint half-open ranges, with a bounded
  segment reduction and a deterministic fold. The chunk correction in the
  reverse running-sum formula restores coefficient `d` from its local
  coefficient `d-a+1`.
- The host Rust stub validates the selected length policy before returning
  `Unsupported`; the CUDA stub returns without reading or writing its pointers.
  This is consistent with explicit fallback and zip-truncation semantics.
- The document correctly labels kernel performance and whole-chain benefits as
  unmeasured/conditional and does not treat the old W10 regression as new gain.

No blocker found in the current stub's functional behavior. The Lean/MSM
contract and native CUDA implementation remain separate correctness gates; this
review does not establish kernel correctness.
