# Quotient interpreter contract handoff

Evidence labels follow astra's `design/QUOTIENT.md`: **SOURCE** is observed code/API,
**LEAN** is a checked abstract statement, **OBLIGATION** remains open, and **OPEN QUESTION** needs
implementation or owner review. This handoff was revised after gluna-9's review to match the frozen
`QuotientProgram` public shape.

## Frozen API reflected in Lean

**SOURCE:** `crates/loki-backend-api/src/lib.rs:467-540` has three operands (`Register`, `Constant`,
`Column { column, rotation }`), six opcodes, an optional right operand, explicit destination,
`register_count`, and `output_register`. Constants bind challenges and other frozen values before
the kernel call. Columns are resident buffers with backend metadata.

**LEAN:** `LokiLean/BackendQuotient.lean` mirrors that shape. `Program.Valid` checks `k ≤ K`, the
word-size log bound, `rows = 2^K`, `rotationScale = 2^(K-k)`, each column's exact row count and
domain/generation metadata, instruction arity, destination/read bounds, read-before-definition,
and an initialized output register. A destination may be reused after an earlier definition; its
operands are read before the write. The negative example rejects a first-instruction self-read.

**SOURCE:** stock `get_rotation_idx` in `src/plonk/evaluation.rs:11-34` uses
`(row + rotation * rotation_scale).rem_euclid(rows)`. **LEAN:** `rotatedRow` uses unbounded `Int`
arithmetic to model that Euclidean index and separately validates the Rust i64 range. The checked
example uses `k=2`, `K=4`, stride 4, including ±1 rotations and multiple wraps.

The six `lower_*_correct` theorems relate each individual opcode to the corresponding source
expression operation. The VM model does not add challenge operands or an arbitrary output operand;
those belonged to an earlier draft and were removed after review.

## Stock quotient path and replacement

**SOURCE:** `src/plonk/evaluation.rs:305-554` processes instances in order and does not reset the
numerator accumulator between them. Each row's identity terms are applied in stock Horner order;
rotated reads use original columns. `src/plonk/prover.rs:667-730` calls that evaluator, then the
caller proceeds with vanishing inversion, inverse transform and quotient construction.

**LEAN:** `SourceTermsMatch` expresses exact flattened identity order. `ordered_source_passes_eq_stock`
and `ordered_source_passes_evaluate_numerator` reuse the proved `Fusion` loop-interchange and
numerator theorem. The latter feeds the existing `Plonk` quotient contract. `BytecodeImplementsPasses`
is an explicit row-by-row compiler simulation contract: each valid VM evaluation equals the source
`Fusion.rowEval` for the corresponding ordered pass. `bytecodeVector_eq_stock_numerator` composes that
contract with the source-pass theorem, and `bytecodeVector_evaluates_numerator` feeds the resulting
vector to the existing Plonk numerator contract. This makes the compiler premise explicit; it does
not claim the Rust bytecode compiler has been verified.

**LEAN:** `bytecodeOutputs_match_stock` binds a validated program factory and that compiler
simulation to the stock `numeratorAt` vector. `callFromInterpreter_meetsContract` constructs the
transactional call from interpreter `Option` outputs: `some` publishes the proved vector, `none`
returns device failure with the original public state. `bytecodeCall_meetsContract` composes these
contracts, so its success equality is derived through bytecode and source-pass equality rather than
assumed as an output premise. `CompiledQuotient` packages the program, ordered source passes,
validation, source-term match and bytecode simulation together;
`compiledBytecodeCall_meetsContract` constructs the call from that artifact family, and
`compiledBytecodeProofBytes_eq_stock` derives proof-byte equality through the existing backend
interface theorem. `runProgram` is the actual modeled transactional row interpreter (`List ℕ →
Option (List F)`), and `bytecodeOutputs` returns its result rather than manufacturing a vector from
`getD` defaults. `runProgram_eq_bytecodeVector_of_simulation` connects a complete successful run to
the vector theorem. `runProgram_valid_eq_bytecodeVector` uses the single marked
`OBLIGATION(quotient-interpreter-safety)` to prove every statically valid program finishes with the
same vector; `runProgram_valid_some` follows immediately.

**LEAN:** `CallContract` requires successful output/publication to match the stock quotient
transition and errors to preserve checkpoint, previous output and transcript. `RuntimeState` is a
separate generation counter, so it may change on device failure. The replacement theorem changes
only `Refinement.Backend.quotientEval`, derives `InterfaceContracts`, and reaches
`Backend.proof_bytes_eq_stock`. Equal encoded bytes imply equal decisions for a verifier modeled as
a function of bytes. Testing the unmodified Rust stock verifier remains a separate gate.

## Open obligations and review

**OBLIGATION:** `quotient-interpreter-safety` must show valid programs return a value in the
initialized output register for each in-range row. Its proof follows the instruction prefix and
static register-initialization check.

**OBLIGATION:** `quotient-compiler-lowering` must establish `BytecodeImplementsPasses` for the
emitted compiler trace against the ordered formulas from `evaluation.rs:305-554`, including
per-instance traversal, permutation chunks, lookup identities, trash identities, challenge
constants, and column/rotation bindings; it must also supply the matching `SourceTermsMatch` field of
the `CompiledQuotient` artifact. Its bytecode simulation conclusion is per-row equality to
`Fusion.rowEval`, not an assumption that final bytecode output equals the numerator. The Rust
compiler and CPU bytecode differential remain separate implementation gates.

**SOURCE:** the public API has typed Rust operands; its separate native wire encoding is specified
in astra's `design/QUOTIENT.md`. Lean does not verify that encoder/decoder, field serialization,
resident-buffer lease identity, CUDA arithmetic, asynchronous ordering or stock-verifier code.

**OPEN QUESTION:** astra/gluna-9 should decide whether to promote the interpreter-safety theorem
from an obligation to a proof before Opus starts P5, and what maximum `wordBits`/register/program
resource bounds the Rust validator will expose. These are not inferred from GPU measurements.
