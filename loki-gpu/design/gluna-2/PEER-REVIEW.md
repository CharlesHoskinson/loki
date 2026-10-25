# Peer review: `LokiLean/BackendMsm.lean`

Review target: gluna-3's MSM model; correction checks through 2026-10-25 05:16 UTC. Read-only
review; findings and follow-ups were sent over AgentBus. The unsigned digit decomposition and sorted,
flattened segmentation invariants are mathematically aligned with the `pippengerList` reference. The
segmented-value theorem is explicitly marked `OBLIGATION(msm-segmented-partition)` and is not treated
as proved.

## Findings

### Resolved — shared call API snapshot was stale

- **Location:** `LokiLean/BackendMsm.lean:134-152`.
- **Finding:** The first draft used `Backend.CallResult State G` and
  `Backend.dispatch_eq_reference`, while the shared API defines `CallResult PublicState RuntimeState
  Output` and `dispatch_public_eq_reference`.
- **Correction:** The current snapshot adds the runtime-state type and calls the updated theorem.

### Resolved — public and runtime state are now distinguished

- **Location:** `LokiLean/BackendMsm.lean:134-136`.
- **Finding:** `PublicState` is described as holding handles and outstanding effects. The approved
  contract preserves caller-visible input/checkpoint values, output publication, and transcript state;
  the backend may invalidate resident handles or poison its session on error.
- **Correction:** The current docstring identifies caller-visible inputs, published outputs and
  transcript state as `PublicState`, and puts handles, resident allocations, pending events, cache
  generations and poison state in `RuntimeState`. It does not explicitly name the host checkpoint;
  this remains a documentation improvement, not a contract defect, because fallback receives the
  preserved public state.

### Resolved — batched dispatcher now preserves output slot order

- **Location:** `LokiLean/BackendMsm.lean:168-194` and `LokiLean/Backend.lean:173`.
- **Finding:** The first snapshot proved only one request's output.
- **Correction:** The current snapshot adds `BatchRequest` and a transactional batch adapter with
  one ordered `List G` output. `dispatchBatch_msmBatch_exact` states the required operation equality
  given `hsp`, the adapter between `SpecOps.msm` and `msmSpecific`.

### Resolved — segmented kernel correctness is tied to its modeled plan

- **Location:** `LokiLean/BackendMsm.lean:282-330` in the current snapshot.
- **Finding:** The first draft assumed in `hk` that every successful kernel output already equals
  `pippengerList`, making the result circular.
- **Correction:** The current `segmentedKernel_success_correct` requires a `SegmentedPlan` witness and
  equality to `segmentedValue`; it derives `pippengerList` using the separately marked
  `OBLIGATION(msm-segmented-partition)`. The transactional adapter then composes that result with
  `Loki.pippenger_meetsMsmSpecific` to establish the required request-level equality. This last
  composition was checked in the current snapshot; it has not been independently built by this
  reviewer, per owner/build boundaries.

## Evidence boundary

The abstract unsigned Pippenger reference is tied to Loki's truncating `zip` and zero filtering at
`loki-proofs/src/poly/kzg/msm.rs:163-190`. The sort/RLE/segment relation is a meaningful kernel
contract, but the partition theorem remains open and neither Lean statement verifies CUDA or unsafe
FFI. Keep real-distribution differentials and stock-verifier acceptance as separate implementation
gates.

## Quotient review response (2026-10-25)

The transferred quotient module was separately reviewed by gluna-9 against the frozen API. I replaced
the append-only draft with `QuotientProgram`-shaped registers, operands, destinations, optional-right
arity, initialized output, column metadata and scaled signed rotations. The current
`BackendQuotient.lean` builds on its own; its concrete nonempty sample includes all six opcodes and
initialized in-place reuse, and a separate example rejects first-use self-read. The initial
quotient-specific findings are addressed in the new snapshot and `design/gluna-2/QUOTIENT.md`.

Still open: `quotient-interpreter-safety` is a marked Lean theorem obligation, and the full Rust
compiler-to-source-pass simulation is documented as `OBLIGATION(quotient-compiler-lowering)` rather
than claimed as proved. After astra requested an explicit compiler-to-source bridge and interpreter
call constructor, I added `BytecodeImplementsPasses`: a valid program's per-row result equals the
source `Fusion.rowEval`. `bytecodeVector_eq_stock_numerator` composes this with ordered source-pass
equality, then `bytecodeOutputs_match_stock` derives output equality from the compiler contract.
`callFromInterpreter_meetsContract` proves that `some` publishes the matching result and `none`
preserves public state; `bytecodeCall_meetsContract` combines both. These additions build in the
owner's module gate. To make that chain a single compiler artifact contract, I added
`CompiledQuotient`, which stores the program, ordered passes, validation, `SourceTermsMatch` proof and
bytecode simulation; `compiledBytecodeCall_meetsContract` builds the concrete interpreter call, and
`compiledBytecodeProofBytes_eq_stock` composes it with the replacement interface. The
`valid_program_eval_some` theorem is the only axiom-audit result containing `sorryAx`, matching its
marked interpreter-safety obligation; the new bridge and call theorems use only standard Lean
axioms. Native interpreter, wire decoder, CUDA and actual stock-verifier gates remain external
evidence. After the subsequent review requested a concrete VM `Option` result, I added transactional
`runRows`/`runProgram`; `bytecodeOutputs` now returns that result rather than a vector synthesized
with total defaults. `runProgram_valid_some` connects static validation to complete execution via
the marked safety obligation, `runProgram_valid_eq_bytecodeVector` proves the valid-path vector
equality, and `runProgram_eq_bytecodeVector_of_simulation` connects the compiler simulation to the
concrete interpreter result without using that safety `sorry`. The composed proof-byte theorem still
builds.

gluna-9's final re-review confirmed the current shared snapshot implements the fail-closed `Option`
traversal and closes the VM/source/backend/proof-byte bridge under the explicitly stated compiler and
interpreter-safety obligations. The earlier P1 findings are resolved in that review report.
