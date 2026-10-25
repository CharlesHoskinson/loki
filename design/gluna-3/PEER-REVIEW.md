# Peer review: NTT contracts

Review target: `LokiLean/BackendNtt.lean` and `design/NTT.md` at the shared draft state on 2026-10-25.
This is a Lean/API contract review. It does not validate a CUDA kernel or native execution.

## Findings

### High — the kernel contracts do not construct the transactional call contract

**Locations:** `LokiLean/BackendNtt.lean:65-87,124-150` and `design/NTT.md:124-130`.

`Contract call` and `dispatch_value_eq_stock` assume full equality to `stock` for arbitrary
`ℕ → F` outputs. `NaturalForwardKernel`, `NaturalInverseKernel`, and the extended kernel predicates
only constrain indices below `2^k` or `2^K`. The coset lemmas prove equality on valid coset slots.
Nothing builds a `call` from those kernel hypotheses, or converts those range-limited equalities into
the operation fields required by `Backend.OperationContracts`. As written, the theorem that actually
composes with stock assumes a stronger fact than the algorithm-level contracts establish.

**Fix:** model buffers at their finite extent (`Fin n → F`, `Vector F n`, or a list plus an explicit
length), define each operation's exact input/output extent, and give a constructor that turns the
corresponding kernel predicate plus exact root/layout/normalization metadata into the transactional
call contract. Then prove those operation equalities in the shape of
`OperationContracts.forward_ntt_exact` through `inverse_coset_exact`. Preserve separate handling of
invalid lengths: the Rust stock functions assert or panic on the wrong input length, while the backend
API returns a typed error before publishing output.

### Medium — the Lean state cannot express the Rust Vec shape contract

**Locations:** `LokiLean/BackendNtt.lean:41-47,60-69`; API
`crates/loki-backend-api/src/lib.rs:245-259,420-421`; Rust
`loki-proofs/src/poly/domain.rs:249-335`.

The state stores source/output as total functions and has no source or output length. A successful
call can therefore satisfy the current function-level equality without stating that the returned
`Vec<F>` has the requested `2^output_log_size` entries. Conversely, equality outside a finite
transform domain is stronger than the caller can observe and the NTT kernel predicates do not
establish it.

**Fix:** use finite vectors or carry source/output logs in the operation and state, then state both
the expected length and equality at every valid index. Include the inverse-coset prefix case and the
five public `EvaluationDomain` operations in those shape lemmas.

### Low — public transform count is inconsistent

**Locations:** `LokiLean/BackendNtt.lean:7-10,32-40`; `LokiLean/Ntt.lean:473-492`;
`loki-proofs/src/poly/domain.rs:249-335`.

The module says “six stock `EvaluationDomain` transforms,” but its `Operation` has five variants and
the Rust `EvaluationDomain` exposes five corresponding conversion methods. `fft` and `ifft` are
private helpers, not two additional public operations.

**Fix:** change “six” to “five” unless a sixth public operation is intended; if so, name its Rust
call site and add the corresponding Lean operation explicitly.

## Positive notes

- The design correctly requires exact roots, natural-order public results, one inverse normalization,
  global-index coset powers, and preserves the host checkpoint requirement for CPU fallback.
- The text explicitly avoids claiming that abstract Lean equality validates CUDA execution.
- The proof module includes a general kernel contract and a concrete small DFT sanity example, though
  a length-two example would exercise a nontrivial twiddle as well.

## Follow-up after ownership transfer

The current `BackendNtt.lean` revision addresses the findings above:

- The public state stores finite `List F` outputs. `Request F d` carries the exact source-length
  equality for its fixed domain; `stockList_length` and `dispatch_output_length` state exact result
  extents. `validateRequest_rejects_wrong_length` specifies rejection before dispatch.
- `KernelSet.Meets` names the four root- and normalization-pinned range contracts. The five-case
  `kernelList_eq_stock` constructor lifts these to exact finite-list equality; the fifth public
  `extendedToLagrange` operation is explicitly composed from inverse-coset and small forward
  kernels. `kernelCall_contract` builds transactional success from that constructor.
- `backendWithKernels` replaces the four NTT fields in the BLS12-381 stock backend, keeps other
  operations and transcript order stock, and `backendWithKernels_proof_bytes_eq_stock` composes the
  equality through the existing proof-byte theorem.
- The operation list now names five public `EvaluationDomain` methods. Parameterized concrete
  success, unsupported/fallback, and invalid-shape witnesses exercise the contract paths.

The revised module has been locally built; astra-gpu is the independent reviewer for this follow-up.
