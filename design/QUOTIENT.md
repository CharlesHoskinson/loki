# Resident quotient numerator interpreter

CODE (proposed contract): this document specifies P5. Unless explicitly SOURCE, MEASURED, ESTIMATE or OPEN QUESTION, normative prose is a proposed CODE contract. The submitted Rust/CUDA implementation returns Unsupported; it does not execute a quotient or claim acceleration.

## Exact boundary

CODE: `QuotientBackend::evaluate_quotient(QuotientProgram)` returns a fresh resident **numerator** vector `nu` in the natural-order extended coset. Despite the method name, it does not divide by the vanishing polynomial, inverse-transform, truncate, split coefficients, commit, draw randomness or write to the transcript. Keeping those boundaries explicit prevents a double division or a duplicated normalization.

CODE: source authority is Loki `877114ddc1da5508230e424ec06d55da1ba00117`: `src/plonk/prover.rs:667-730` (`compute_nu_poly`) supplies advice and instance cosets; `src/plonk/evaluation.rs:305-577` (`Evaluator::evaluate_numerator`) accumulates the identities; `src/poly/domain.rs:345-366` supplies the exact periodic vanishing inverses. W10's separate quotient plan is a handoff input, not merged code at that base.

The complete per-row sequence is circuit-instance order, then each instance's custom gate polynomials, permutation identities and chunks, each lookup's five identities, and each trash identity. The accumulator begins at zero once for the entire instance sequence, not once per instance. Every identity updates `acc = acc*y + term` in stock order. Read-only rotations address original operand columns, never a partially updated numerator. `Fusion.fused_numerator_eq` establishes row fusion from this exact ordered term list; merely evaluating the same unordered set of constraints is insufficient.

## Program semantics and validation

The API has six opcodes, `Add`, `Sub`, `Mul`, `Square`, `Double`, and `Negate`. Operands name an initialized register, a frozen constant, or a resident column with a signed rotation. `Add/Sub/Mul` require a right operand; unary operations require its absence. Both operands are read before the destination is written. Horner evaluation lowers to ordinary multiply and add instructions. Store/copy lowers to an addition with a canonical zero constant if it cannot be eliminated. This avoids an ambiguous ternary Horner opcode.

All per-row registers start uninitialized. A static validator tracks defined registers in program order; every read, including an aliased old destination, must have an earlier definition. Check destination/operand indices against `register_count`, all constants and columns against their table lengths, and the final output register against the final defined set. An empty or malformed program is rejected unless a future explicit constant-output encoding defines its output; v1 needs an initialized output register. Validate before launching and do not assume a GPU out-of-bounds read returns zero.

For base log `k` and extended log `K`, require `k <= K < usize::BITS`, `rows = 2^K`, and `rotation_scale = 2^(K-k)` using checked arithmetic. Every column contains exactly `rows` values in the same field, coset representation and current device generation. Each column lease is validated against backend-owned allocation metadata, not caller IDs alone. Reject unsupported domain, register, instruction, constant or allocation bounds before submission.

Column operand `(c, rotation)` at row `r` reads

`column[c][(r + rotation * rotation_scale) rem_euclid rows]`.

Use arithmetic wide enough for the complete accepted input range, or reduce the signed rotation modulo the domain before multiplication. The API uses `i64` rotations, while current stock expressions use `i32`; either reject out-of-stock-range inputs in the adapter or implement the full checked API range. Do not use wrapping signed multiplication, unsigned casts of negative rotations, or chunk-local row indices. Constants include the exact frozen challenges `beta`, `gamma`, `theta`, trash challenge and `y` after stock squeezes; column/challenge bindings are part of compilation, not a kernel choice.

## Versioned bytecode wire encoding

CODE (P5 ABI specification): never copy the memory of Rust `Operand`, `QuotientOpcode` or `QuotientInstruction` into CUDA. Encode a program envelope with an explicit bytecode version (`u32`, value 1); the private native invocation also binds the validated field/wire ABI, device generation and resident column table. Reject unknown versions before decoding or allocation. The current Unsupported host-byte C entry point does not implement this decoder.

Each version-1 instruction is exactly 40 bytes, encoded little endian with no implicit padding:

| Offset | Width | Value |
|---:|---:|---|
| 0 | 4 | opcode: Add=1, Sub=2, Mul=3, Square=4, Double=5, Negate=6 |
| 4 | 4 | destination register index |
| 8 | 4 | left operand tag |
| 12 | 4 | left operand index |
| 16 | 8 | left signed two's-complement rotation |
| 24 | 4 | right operand tag |
| 28 | 4 | right operand index |
| 32 | 8 | right signed two's-complement rotation |

Operand tags are absent=0, register=1, constant=2 and column=3. Left cannot be absent. Unary instructions require absent right with index/rotation zero; binary instructions forbid absent right. Register/constant operands require rotation zero. Column operands use the signed rotation field. Reject unknown tags, noncanonical unused fields, count-times-40 overflow, short or trailing instruction bytes, and semantic validation failures before launch. Host encoding uses explicit integer serialization; device decoding uses checked offsets or an independently size/offset-asserted native representation. Constants are encoded by the separately validated BLS12-381 field wire conversion. Column references are indices into a backend-owned validated resident table, never arbitrary serialized device pointers.

P5 `quotient_validation` and `ffi_abi` gates include golden byte vectors, encoder/decoder round trips, endian/offset checks, invalid versions/tags/arity and malformed lengths. The Rust and CUDA decoders must agree with the typed instruction semantics; this is a separate implementation obligation from mathematical interpreter equality.

## Compiler and register allocation

The Loki-side compiler lives at proposed `src/backend/quotient_compile.rs`. It lowers `GraphEvaluator` plus the surrounding permutation, lookup and trash formulas into the instruction sequence. It binds selector, fixed, advice, instance, permutation-product and lookup-product operands to their correct circuit instance and extended coset. It binds global-index values such as `omega^row`, active-row selectors, `l0`, `l_last`, and rotation stride exactly as stock.

Initially compile one ordered full numerator program without cross-graph common-subexpression elimination. Reuse local calculation structure only where each consumer reads its own expression result. Keep the compiler's logical registers separate from the physical register allocation map. Reusing a physical register is valid only after every live logical use has completed. Validate this mapping independently, or preserve a liveness certificate checked against the emitted code. A reduction in register count is not evidence that the mapping is correct.

An independent CPU bytecode interpreter is the first executable oracle. Compare each opcode and row against a direct expression evaluator, then compare complete numerator vectors with stock. Keep compiler correctness and interpreter execution correctness as separate hypotheses in Lean and separate test failures. Neither may be replaced by the premise “the final output equals stock.”

## CUDA execution and residency

Start with a grid-stride row interpreter: one worker evaluates one row at a time, reading immutable resident columns and writing one fresh numerator element. A bounded register array is per worker; do not materialize a full-domain vector for every intermediate. Record generated-code register count, spills/local-memory traffic, instruction count, occupancy observations and actual duration. Large instruction/constant tables live in explicit read-only device buffers; do not presume they fit CUDA constant memory.

ESTIMATE: launch width, registers per row, program-size limits, maximum active resident columns and spill thresholds are tuning choices to set from measured sm_120 resource usage. No imported block size, occupancy percentage or kernel time is a default correctness assumption. Reject a program above the configured resource bound with an explicit error so the caller can use a retained CPU checkpoint.

The runtime reserves input leases, program storage, tentative output and scratch before work. Host API completion is blocking. Internal copy/compute streams use explicit dependencies; no source handle or temporary is released until all users retire. Keep the full required extended cosets resident for this first implementation. Stock builds some lookup cosets one at a time; fusing all lookups increases the live set and that cost must be measured. If it does not fit, use explicit ordered pass groups with accumulator residency, each preserving the same Horner order; a different sub-coset/decimation scheme needs its own layout and rotation proof.

After numerator completion, the caller runs the exact periodic vanishing inverse multiplication, inverse coset transform with one normalization, stock truncation and quotient commitment sequence. These may execute through further resident operations without a host round trip. The same stock values and output order are mandatory at each boundary. Do not independently count an NTT transfer saving again as a numerator saving.

## Errors, fallback and proof composition

Any failure leaves logical public inputs and previously published outputs unchanged and publishes no new handle. A CUDA failure may invalidate the device generation. Discard tentative results. A full CPU fallback is allowed only from the caller's complete retained host checkpoint; device-only originals lost with a context terminate the proof. The backend does not select fallback and never repeats transcript writes, challenge squeezes or RNG draws.

`LokiLean/BackendQuotient.lean` specifies instruction evaluation, validated program behavior and connection to the ordered numerator semantics. `runRows` and `runProgram` collect row results through `Option`: one failed row returns no vector. `CompiledQuotient` packages a validated program with its ordered source passes, `SourceTermsMatch` and the explicit `BytecodeImplementsPasses` compiler simulation contract. `compiledBytecodeCall_meetsContract` constructs the transactional call and `compiledBytecodeProofBytes_eq_stock` derives stock-model byte equality. These composition theorems do not depend on the marked interpreter-safety placeholder. `valid_program_eval_some` remains OBLIGATION(quotient-interpreter-safety); discharging compiler lowering, input/challenge binding and native execution is separate P5 work. An abstract theorem is not a proof of the Rust compiler or CUDA interpreter.

## Gates and package ownership

P5 in PLAN.md is authoritative for ownership, features, effort and dependencies: P3 SRS, P4 resident transforms and W10 handoff. Its opt-in feature is `cuda-quotient`, implying `cuda-resident` and `loki-gpu-backend`; crate `cuda` controls native compilation. Exact files are `src/quotient.rs`, `cuda/quotient.cu`, `tests/quotient_validation.rs`, `tests/quotient_differential.rs` under the GPU crate; Loki `src/backend/quotient_compile.rs`, `src/plonk/{evaluation,prover}.rs`, W10 `src/plonk/evaluation/cuda.rs` after handoff; Lean `LokiLean/BackendQuotient.lean`.

Required negative cases include missing operands, wrong arity, register reads before definition (including an uninitialized self-read), out-of-bounds registers/constants/columns, wrong field/generation/length, wrong output register, invalid logs/stride, signed extreme rotations, allocation failure and device loss. Required positive cases include all six opcodes, overwriting an already initialized register, multiple instances without accumulator reset, multi-wrap rotations, all argument families and program/pass chunk boundaries. Compare direct stock arithmetic, independent CPU bytecode and GPU values. GPU tests require successful dispatch counters and zero fallback.

Require W2 cross-verification and actual A3/complete leaf-parent-decider stock acceptance before promotion. ESTIMATE target: at least 3% repeated matched whole-proof/chain median improvement; the historical memo's four-second quotient-pipeline target is only an experiment. Kill promotion on any pointwise mismatch, hidden fallback, excessive live memory/spills or absent whole-proof benefit. See TEST-PLAN.md for full provenance and negative proof controls.

SOURCE: algorithm provenance is our independent lowering of permitted Loki source, the existing `LokiLean/Fusion.lean` and `Plonk.lean` models, and the allowed own-words memo WP2. No external GPU interpreter implementation or prohibited source was consulted or copied. PROVENANCE.md records this algorithm and the separate NTT/runtime sources.
