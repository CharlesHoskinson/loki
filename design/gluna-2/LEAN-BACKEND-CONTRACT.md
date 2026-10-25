# Lean backend contract notes

Status: draft for owner review. Scope: `LokiLean/Backend.lean` and its root import only.

Evidence convention: normative design statements below are **CODE (proposed contract)** unless
explicitly labeled SOURCE, MEASURED, ESTIMATE, or OPEN QUESTION; they do not assert implementation
results.

## Contract choices

- **CODE:** Loki's optional MSM dispatch returns a GPU result only on success and otherwise runs
  `msm_cpu` (`loki-proofs/src/poly/kzg/msm.rs:169-175`). NTT similarly falls back to `best_fft`;
  the CUDA adapter is required to preserve the mutable input on error
  (`loki-proofs/src/poly/domain.rs:395-447`).
- **CODE:** Quotient evaluation receives fixed-order advice, instance, permutation, lookup, trash,
  and selector inputs (`loki-proofs/src/plonk/prover.rs:667-720`,
  `loki-proofs/src/plonk/evaluation.rs:305-577`).
- `Loki.Backend.Contract` requires successful output and logical-state equality with the stock
  transition. An error preserves the full `PublicState`, which represents host inputs/checkpoints,
  published outputs, and transcript events. `RuntimeState` is separate and may mark a device
  generation poisoned or invalidate a cache after a failed call.
- `dispatch` uses the stock transition after an error, on the original `PublicState`. Its theorem
  proves equality of the returned value and logical state while retaining the backend's updated
  runtime metadata. A failure has no output constructor, so a partial result cannot be published.
- Transcript writes remain in shared prover code. `InterfaceContracts` carries an independent
  `SameOrder` obligation, so asynchronous device completion cannot set commitment order
  (`loki-proofs/src/plonk/prover.rs:292-328`).
- The `ord` parameter is the selected stock lookup map traversal. The design's stock-default build
  must preserve current stock traversal; deterministic sorted leftovers remain an explicit,
  separately gated behavior (`loki-proofs/src/plonk/lookup/prover.rs:399-431`).
- `OperationContracts` names the separate MSM, base/coset transform, quotient, inversion, and lookup
  equalities. `toRefines` constructs the existing `Refinement.Refines` record. The final theorem then
  applies the existing `loki_proof_eq_stock` result; this does not assert that native CUDA satisfies
  any kernel contract.

## Evidence and remaining obligations

- **PROVED IN LEAN:** transactional dispatch returns the reference public transition on success and
  on error; successful calls may update runtime state. Concrete `Nat` examples exercise both a real
  success and an `Unsupported` CPU fallback.
- **PROVED IN LEAN:** operation-level contracts construct `Refinement.Refines`; with transcript
  `SameOrder`, proof events, encoded bytes, and any verifier function of events or encoded bytes
  makes the same decision as the stock model.
- **AXIOMS:** a targeted `#print axioms` pass found `dispatch_public_eq_reference` axiom-free;
  `OperationContracts.toRefines`, `proof_events_eq_stock`, `proof_bytes_eq_stock`, and
  `verifier_decision_eq_stock` and `byte_verifier_decision_eq_stock` depend only on standard
  `[propext, Classical.choice, Quot.sound]`.
- **OPEN (`quot-inputs`):** equality of a quotient kernel with `SpecOps.quotientEval` does not prove
  the Rust adapter supplied the exact extended cosets and challenge tuple. Keep the existing
  `Refinement.quotient_via_backend` assumption and W2 cross-verifier test.
- **OPEN (native implementation):** Lean models the Rust API contract and mathematical operation
  semantics; it does not verify CUDA, unsafe FFI, stream lifetime, device-generation poisoning, or
  resource bounds. Differential tests, error injection, sanitizers, and unmodified stock-verifier
  acceptance remain implementation gates.
- **OPEN (fallback after device loss):** the host checkpoint must retain all values needed to rerun
  the stock operation. Device cache/session invalidation belongs in `RuntimeState`, not the logical
  preservation theorem.

## Shared names

`Loki.Backend.Error`, `CallResult PublicState RuntimeState Output`, `Reference`, `Contract`,
`dispatch`, and `dispatch_public_eq_reference` are the shared transactional API. `OperationContracts`
has one field per existing `Refines` operation; algorithm modules should provide those equalities from
their own correctness theorems plus this dispatcher contract.
