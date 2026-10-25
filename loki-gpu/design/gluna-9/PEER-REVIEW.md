# Peer review: quotient design and Lean contract

Reviewer: gluna-9

Scope: read-only review against the owner brief, `design/ARCHITECTURE.md`, `design/PLAN.md`, the permitted Loki 877114d source and the shared Lean modules.
Status: design wire/self-read findings and the three Lean P1 findings are resolved. The VM-to-source-pass/backend-call bridge is present under explicit compiler/source hypotheses, and the transactional output adapter fails closed on row evaluation failure. gluna-2 reports the module build passed; I did not rerun another owner's build.

## Findings

### P2 — Quotient bytecode wire encoding is not specified

**Location:** `design/QUOTIENT.md:37` and P5's FFI handoff at `design/QUOTIENT.md:53`.

The CUDA interpreter consumes `QuotientInstruction` records, but Rust enum and struct layout are not a stable wire format. The document requires an explicit versioned C ABI generally, yet P5 does not say how six opcode tags, optional right operands, register/constant/column indices, and signed rotations cross that boundary. Copying the public Rust structs directly would make decoding depend on unspecified layout and alignment.

**Concrete fix:** before native quotient execution, define a versioned fixed-width wire encoding for each instruction and tagged operand, with explicit widths/endianness, checked instruction/constant/column counts, and rejection of unknown opcode/tag/reserved values. Keep the Rust API representation separate from the C/CUDA wire representation. Add golden layout and malformed-wire tests to `ffi_abi` / `quotient_validation`.

**Disposition: RESOLVED.** `design/QUOTIENT.md:27-46` now specifies a version-1 fixed-width, little-endian 40-byte instruction, operand tags, canonical unused fields, checked lengths, unknown-tag rejection, and golden/malformed `ffi_abi` / `quotient_validation` gates. `loki_gpu_ffi.h` also states that Rust/C++ layouts are not the wire format.

### P3 — Negative self-read case needs to distinguish first definition from in-place update

**Location:** `design/QUOTIENT.md:55`.

The prose groups “uninitialized/self-read registers” as a negative case, while line 17 permits an instruction to read an already initialized destination and the positive cases explicitly include overwriting an initialized register. A test author could incorrectly reject all destination/operand aliasing.

**Concrete fix:** phrase the negative case as “read-before-definition, including a first-instruction destination alias”; retain the positive case for an initialized in-place update.

**Disposition: RESOLVED.** `design/QUOTIENT.md:76` now states that the negative is a read before definition, including an uninitialized self-read, while the positive case keeps overwriting an initialized register.

## Lean review

### P1 — Extended-domain rotation scale is missing from the Lean model

**Location:** `/home/charles/repos/loki-lean-wt/gpu-spec/LokiLean/BackendQuotient.lean:52-64, 76-80`.

The API and quotient design compute a source-column index as `(row + rotation * rotation_scale) mod rows`; stock `get_rotation_idx` receives `rot_scale = 2^(K-k)`. `Environment` stores only `domainSize`, and `rotatedRow` adds the signed rotation directly. This agrees only when `rotation_scale = 1`. On an extended domain such as k20→K22, a rotation by one base row must advance four extended rows, but this model advances one, so the Lean expression compiler does not model the promised stock query semantics.

**Concrete fix:** add the checked base/extended-domain relation or an explicit `rotationScale` to `Environment`, and calculate modulo using `rotation * rotationScale`. Add a small example with scale greater than one and positive, negative, and multi-wrap rotations; connect it to the existing `get_rotation_idx` definition.

### P1 — Program validation differs from the public API shape

**Location:** `BackendQuotient.lean:93-108`; compare `crates/loki-backend-api/src/lib.rs:495-527`.

The Rust v1 request has an `output_register` and requires an initialized output register, but the Lean `Program.output : Read` accepts a literal, column, or challenge and `Program.Valid` permits an empty instruction list with a literal output. That describes a broader interface than the actual backend API and omits the output-register invariant. `Environment.Valid` also checks only `0 < domainSize`; it does not state the request's exact row count, base/extended log and rotation scale, or column-length/generation consistency. This leaves the promised malformed-shape/error contract outside the model.

**Concrete fix:** make the Lean output a register index and require a nonempty program whose final output register is initialized, or explicitly broaden the Rust API to support arbitrary output operands and align the design. Model the validated domain/column metadata (at minimum by proof-carrying hypotheses) and require `row < domainSize`, exact column lengths and generation/domain agreement before a success contract can apply.

**Disposition: RESOLVED.** The revised `BackendQuotient.lean` models `registerCount` and `outputRegister`, rejects uninitialized outputs (including empty programs), validates `domainLog ≤ extendedDomainLog < wordBits`, rows and rotation scale, and checks every resident column's exact length, domain logs, and generation. `valid_program_eval_some` is an explicitly marked `OBLIGATION(quotient-interpreter-safety)` for in-range rows.

### Lean rotation-scale finding disposition

**Disposition: RESOLVED.** The revised `rotatedRow` at `BackendQuotient.lean:77` uses `row + rotation * rotationScale` with Euclidean reduction. Its sanity example checks scale 4, positive/negative rotations, and multiple wraps. The API `rotationFitsI64` bound is checked in `Operand.Valid`.

### P1 — The VM-backed output adapter must fail closed on interpreter errors

**Location:** `BackendQuotient.lean:360-367`.

An earlier snapshot returned `some (bytecodeVector ...)` after validation, where a total proof helper replaces `evalRow = none` with zero via `getD 0`. That could publish a vector containing zero after an unexpected runtime failure.

The latest version packages `program`, `orderedPasses`, validity, `sourceTerms`, and `bytecodeSimulation` in `CompiledQuotient`; it adds `bytecodeVector_eq_stock_numerator`, `compiledBytecodeCall_meetsContract`, and `compiledBytecodeProofBytes_eq_stock`. These connect VM row execution to source pass values and stock proof bytes under explicit compiler/source hypotheses. The source compiler simulation remains an explicit obligation. This closes the prior formal-composition finding at the abstract level; it does not prove Rust or CUDA execution.

**Disposition: RESOLVED.** The current snapshot defines `runRows` as an `Option` traversal, so any row failure aborts the whole vector; `runProgram` executes every row and `bytecodeOutputs` returns `runProgram` only for a valid program. `callFromInterpreter` maps `none` to failure with the original public state. `runProgram_eq_bytecodeVector_of_simulation` derives successful-vector equality, and `compiledBytecodeCall_meetsContract` / `compiledBytecodeProofBytes_eq_stock` compose that concrete call through the stock refinement theorem. The valid-program totality argument relies on the single marked `OBLIGATION(quotient-interpreter-safety)`.

### P1 disposition

**Status: RESOLVED under explicit obligations.** The formal VM/source/backend-call chain and fail-closed adapter are present. `quotient-compiler-lowering` and `quotient-interpreter-safety` remain explicit obligations; no native CUDA correctness claim is made.

### OBLIGATION status

Across the current Lean modules, the two marked `sorry` declarations are `BackendMsm.segmentedValue_eq_pippengerList` (`OBLIGATION(msm-segmented-partition)`) and `BackendQuotient.valid_program_eval_some` (`OBLIGATION(quotient-interpreter-safety)`). The source-to-bytecode lowering remains an explicit `OPEN OBLIGATION(quotient-compiler-lowering)` without a `sorry`; the compiled VM-to-source-pass/backend-call/proof-byte composition is present under that compiler contract. gluna-2 reports `lake build LokiLean.BackendQuotient` passed; this review does not claim native CUDA correctness.
