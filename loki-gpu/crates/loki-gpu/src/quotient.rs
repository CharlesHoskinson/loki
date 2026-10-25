//! Resident quotient numerator evaluation (P5).
//!
//! This module holds the CUDA-neutral parts of the P5 contract:
//!
//! - [`validate_program`]: the static program validator (shape, arity,
//!   bounds, define-before-use register tracking, output register);
//! - [`wire`]: the explicit version-1 bytecode envelope and 40-byte
//!   little-endian instruction encoding of `design/QUOTIENT.md`;
//! - [`evaluate_host`]: an independent CPU bytecode interpreter over host
//!   slices. It is the oracle for the native interpreter and for the Loki
//!   compiler; it is **not** wired behind [`QuotientBackend`], so a failing or
//!   absent native path can never be hidden as a CPU success.
//!
//! The native resident path is not implemented here yet and still returns
//! `Unsupported` without inspecting handles.

use ff::{Field, PrimeField};
use loki_backend_api::{
    BackendError, ErrorKind, FailureEffect, Operand, Operation, QuotientBackend,
    QuotientInstruction, QuotientOpcode, QuotientProgram, ResidentBuffer,
};

use crate::LokiGpu;

impl<F: PrimeField> QuotientBackend<F> for LokiGpu {
    fn evaluate_quotient(
        &self,
        _program: QuotientProgram<'_, F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        // Do not inspect handles or return a partially initialized result.
        Err(BackendError::unsupported(Operation::Quotient))
    }
}

/// Machine-readable reason a program, envelope or column set was rejected.
/// Every variant is detected before any row is evaluated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotientError {
    /// `domain_log <= extended_domain_log < usize::BITS` does not hold.
    InvalidDomainLog,
    /// `rows != 2^extended_domain_log`.
    RowsMismatch,
    /// `rotation_scale != 2^(extended_domain_log - domain_log)`.
    RotationScaleMismatch,
    /// v1 needs at least one instruction defining the output register.
    EmptyProgram,
    /// `Add/Sub/Mul` without a right operand, or a unary op with one.
    Arity { instruction: usize },
    RegisterOutOfBounds { instruction: usize, register: u32 },
    /// A register read before any earlier instruction defined it, including
    /// a read of the instruction's own (not yet defined) destination.
    UninitializedRegister { instruction: usize, register: u32 },
    ConstantOutOfBounds { instruction: usize, index: u32 },
    ColumnOutOfBounds { instruction: usize, index: u32 },
    OutputOutOfBounds,
    OutputUninitialized,
    /// A column does not contain exactly `rows` values.
    ColumnLength { column: usize },
    /// The program exceeds a configured resource bound.
    ResourceLimit,
    // ---- wire decoding ----
    UnsupportedVersion(u32),
    /// Fewer bytes than the header or instruction count requires.
    Truncated,
    /// More bytes than the header and instruction count describe.
    TrailingBytes,
    /// `instruction_count * 40` (plus header) overflows `usize`.
    LengthOverflow,
    UnknownOpcode { instruction: usize, opcode: u32 },
    UnknownTag { instruction: usize, tag: u32 },
    /// An unused index/rotation field is nonzero.
    NonCanonical { instruction: usize },
}

impl QuotientError {
    /// Every validation failure is an input error with no side effect.
    pub const fn backend_error(self) -> BackendError {
        BackendError {
            operation: Operation::Quotient,
            kind: match self {
                QuotientError::ResourceLimit => ErrorKind::Capacity,
                _ => ErrorKind::InvalidInput,
            },
            effect: FailureEffect::NotStarted,
        }
    }
}

impl From<QuotientError> for BackendError {
    fn from(e: QuotientError) -> Self {
        e.backend_error()
    }
}

/// Storage-independent description of a program's tables and domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProgramShape {
    pub rows: usize,
    pub domain_log: u32,
    pub extended_domain_log: u32,
    pub rotation_scale: usize,
    pub register_count: u32,
    pub constant_count: usize,
    pub column_count: usize,
    pub output_register: u32,
}

impl ProgramShape {
    pub fn of_resident<F>(program: &QuotientProgram<'_, F>) -> Self {
        Self {
            rows: program.rows,
            domain_log: program.domain_log,
            extended_domain_log: program.extended_domain_log,
            rotation_scale: program.rotation_scale,
            register_count: program.register_count,
            constant_count: program.constants.len(),
            column_count: program.columns.len(),
            output_register: program.output_register,
        }
    }
}

/// Statically validate a program against its shape. Returns `Ok` only when
/// every row evaluation is guaranteed to read initialized registers and
/// in-bounds constants/columns and to define the output register.
pub fn validate_program(
    shape: &ProgramShape,
    instructions: &[QuotientInstruction],
) -> Result<(), QuotientError> {
    validate_domain(shape)?;
    if instructions.is_empty() {
        return Err(QuotientError::EmptyProgram);
    }
    let registers = shape.register_count as usize;
    let mut defined = vec![false; registers];
    for (index, instruction) in instructions.iter().enumerate() {
        if instruction_is_binary(instruction.opcode) != instruction.right.is_some() {
            return Err(QuotientError::Arity { instruction: index });
        }
        // Operands are read before the destination is written: check every
        // read against the set defined by *earlier* instructions.
        let operands = core::iter::once(&instruction.left).chain(instruction.right.as_ref());
        for operand in operands {
            match *operand {
                Operand::Register(register) => {
                    match defined.get(register as usize) {
                        None => {
                            return Err(QuotientError::RegisterOutOfBounds {
                                instruction: index,
                                register,
                            })
                        }
                        Some(false) => {
                            return Err(QuotientError::UninitializedRegister {
                                instruction: index,
                                register,
                            })
                        }
                        Some(true) => {}
                    }
                }
                Operand::Constant(c) => {
                    if c as usize >= shape.constant_count {
                        return Err(QuotientError::ConstantOutOfBounds {
                            instruction: index,
                            index: c,
                        });
                    }
                }
                // Every i64 rotation is accepted: row indexing is exact in i128.
                Operand::Column { column, .. } => {
                    if column as usize >= shape.column_count {
                        return Err(QuotientError::ColumnOutOfBounds {
                            instruction: index,
                            index: column,
                        });
                    }
                }
            }
        }
        match defined.get_mut(instruction.destination as usize) {
            Some(slot) => *slot = true,
            None => {
                return Err(QuotientError::RegisterOutOfBounds {
                    instruction: index,
                    register: instruction.destination,
                })
            }
        }
    }
    match defined.get(shape.output_register as usize) {
        None => Err(QuotientError::OutputOutOfBounds),
        Some(false) => Err(QuotientError::OutputUninitialized),
        Some(true) => Ok(()),
    }
}

fn validate_domain(shape: &ProgramShape) -> Result<(), QuotientError> {
    if shape.domain_log > shape.extended_domain_log || shape.extended_domain_log >= usize::BITS {
        return Err(QuotientError::InvalidDomainLog);
    }
    if 1usize.checked_shl(shape.extended_domain_log) != Some(shape.rows) {
        return Err(QuotientError::RowsMismatch);
    }
    if 1usize.checked_shl(shape.extended_domain_log - shape.domain_log) != Some(shape.rotation_scale) {
        return Err(QuotientError::RotationScaleMismatch);
    }
    Ok(())
}

const fn instruction_is_binary(opcode: QuotientOpcode) -> bool {
    matches!(opcode, QuotientOpcode::Add | QuotientOpcode::Sub | QuotientOpcode::Mul)
}

/// Euclidean rotated row index `(row + rotation * scale) mod rows`, computed
/// in `i128` so every `usize` row, `i64` rotation and `usize` scale is exact.
/// `rows` must be nonzero.
pub fn rotated_row(row: usize, rotation: i64, scale: usize, rows: usize) -> usize {
    // |row| < 2^64, |rotation * scale| < 2^127: the sum fits in i128.
    (row as i128 + rotation as i128 * scale as i128).rem_euclid(rows as i128) as usize
}

/// A program whose columns are host slices, for the CPU oracle.
pub struct HostQuotientProgram<'a, F> {
    pub rows: usize,
    pub domain_log: u32,
    pub extended_domain_log: u32,
    pub rotation_scale: usize,
    pub register_count: u32,
    pub instructions: &'a [QuotientInstruction],
    pub constants: &'a [F],
    pub columns: &'a [&'a [F]],
    pub output_register: u32,
}

impl<F> HostQuotientProgram<'_, F> {
    pub fn shape(&self) -> ProgramShape {
        ProgramShape {
            rows: self.rows,
            domain_log: self.domain_log,
            extended_domain_log: self.extended_domain_log,
            rotation_scale: self.rotation_scale,
            register_count: self.register_count,
            constant_count: self.constants.len(),
            column_count: self.columns.len(),
            output_register: self.output_register,
        }
    }
}

/// Independent CPU bytecode interpreter: validates, then evaluates every row
/// in order and returns the fresh output vector (one value per row).
pub fn evaluate_host<F: Field>(program: &HostQuotientProgram<'_, F>) -> Result<Vec<F>, QuotientError> {
    let shape = program.shape();
    validate_program(&shape, program.instructions)?;
    if let Some(column) = program.columns.iter().position(|c| c.len() != program.rows) {
        return Err(QuotientError::ColumnLength { column });
    }
    let mut registers = vec![F::ZERO; program.register_count as usize];
    let mut output = Vec::with_capacity(program.rows);
    for row in 0..program.rows {
        for instruction in program.instructions {
            let read = |operand: &Operand, registers: &[F]| -> F {
                match *operand {
                    Operand::Register(r) => registers[r as usize],
                    Operand::Constant(c) => program.constants[c as usize],
                    Operand::Column { column, rotation } => {
                        program.columns[column as usize]
                            [rotated_row(row, rotation, program.rotation_scale, program.rows)]
                    }
                }
            };
            let left = read(&instruction.left, &registers);
            let right = instruction.right.as_ref().map(|r| read(r, &registers));
            let value = match (instruction.opcode, right) {
                (QuotientOpcode::Add, Some(right)) => left + right,
                (QuotientOpcode::Sub, Some(right)) => left - right,
                (QuotientOpcode::Mul, Some(right)) => left * right,
                (QuotientOpcode::Square, None) => left.square(),
                (QuotientOpcode::Double, None) => left.double(),
                (QuotientOpcode::Negate, None) => -left,
                // Excluded by validate_program.
                _ => unreachable!("validated arity"),
            };
            registers[instruction.destination as usize] = value;
        }
        output.push(registers[program.output_register as usize]);
    }
    Ok(output)
}

/// Version-1 bytecode wire format.
///
/// Envelope (little endian, no padding):
///
/// | Offset | Width | Value |
/// |---:|---:|---|
/// | 0 | 4 | bytecode version, `1` |
/// | 4 | 4 | register count |
/// | 8 | 4 | output register |
/// | 12 | 4 | instruction count `n` |
/// | 16 | 40 n | instructions, encoded by [`encode_instruction`] |
///
/// The envelope carries no constants or column pointers: constants use the
/// separately validated field wire conversion and columns are indices into a
/// backend-owned resident table.
pub mod wire {
    use super::*;

    pub const BYTECODE_VERSION_V1: u32 = 1;
    pub const HEADER_BYTES_V1: usize = 16;
    pub const INSTRUCTION_BYTES_V1: usize = 40;

    pub const OP_ADD: u32 = 1;
    pub const OP_SUB: u32 = 2;
    pub const OP_MUL: u32 = 3;
    pub const OP_SQUARE: u32 = 4;
    pub const OP_DOUBLE: u32 = 5;
    pub const OP_NEGATE: u32 = 6;

    pub const TAG_ABSENT: u32 = 0;
    pub const TAG_REGISTER: u32 = 1;
    pub const TAG_CONSTANT: u32 = 2;
    pub const TAG_COLUMN: u32 = 3;

    /// A decoded envelope. Decoding checks wire canonicality only; callers
    /// must still run [`validate_program`] against the table sizes.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DecodedProgram {
        pub register_count: u32,
        pub output_register: u32,
        pub instructions: Vec<QuotientInstruction>,
    }

    const fn opcode_code(opcode: QuotientOpcode) -> u32 {
        match opcode {
            QuotientOpcode::Add => OP_ADD,
            QuotientOpcode::Sub => OP_SUB,
            QuotientOpcode::Mul => OP_MUL,
            QuotientOpcode::Square => OP_SQUARE,
            QuotientOpcode::Double => OP_DOUBLE,
            QuotientOpcode::Negate => OP_NEGATE,
        }
    }

    /// `(tag, index, rotation)` of an optional operand.
    const fn operand_fields(operand: Option<&Operand>) -> (u32, u32, i64) {
        match operand {
            None => (TAG_ABSENT, 0, 0),
            Some(Operand::Register(r)) => (TAG_REGISTER, *r, 0),
            Some(Operand::Constant(c)) => (TAG_CONSTANT, *c, 0),
            Some(Operand::Column { column, rotation }) => (TAG_COLUMN, *column, *rotation),
        }
    }

    /// Encode one instruction. The bytes are canonical only when the
    /// instruction's arity matches its opcode; [`encode_program`] checks
    /// that, and [`decode_instruction`] rejects a mismatch.
    pub fn encode_instruction(instruction: &QuotientInstruction) -> [u8; INSTRUCTION_BYTES_V1] {
        let mut out = [0u8; INSTRUCTION_BYTES_V1];
        let (lt, li, lr) = operand_fields(Some(&instruction.left));
        let (rt, ri, rr) = operand_fields(instruction.right.as_ref());
        out[0..4].copy_from_slice(&opcode_code(instruction.opcode).to_le_bytes());
        out[4..8].copy_from_slice(&instruction.destination.to_le_bytes());
        out[8..12].copy_from_slice(&lt.to_le_bytes());
        out[12..16].copy_from_slice(&li.to_le_bytes());
        out[16..24].copy_from_slice(&lr.to_le_bytes());
        out[24..28].copy_from_slice(&rt.to_le_bytes());
        out[28..32].copy_from_slice(&ri.to_le_bytes());
        out[32..40].copy_from_slice(&rr.to_le_bytes());
        out
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&bytes[offset..offset + 4]);
        u32::from_le_bytes(b)
    }

    fn i64_at(bytes: &[u8], offset: usize) -> i64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&bytes[offset..offset + 8]);
        i64::from_le_bytes(b)
    }

    fn decode_operand(
        tag: u32,
        index: u32,
        rotation: i64,
        instruction: usize,
    ) -> Result<Option<Operand>, QuotientError> {
        let zero_rotation = |operand| {
            if rotation == 0 {
                Ok(Some(operand))
            } else {
                Err(QuotientError::NonCanonical { instruction })
            }
        };
        match tag {
            TAG_ABSENT => {
                if index == 0 && rotation == 0 {
                    Ok(None)
                } else {
                    Err(QuotientError::NonCanonical { instruction })
                }
            }
            TAG_REGISTER => zero_rotation(Operand::Register(index)),
            TAG_CONSTANT => zero_rotation(Operand::Constant(index)),
            TAG_COLUMN => Ok(Some(Operand::Column {
                column: index,
                rotation,
            })),
            tag => Err(QuotientError::UnknownTag { instruction, tag }),
        }
    }

    /// Decode one instruction; `index` is used only for error reporting.
    pub fn decode_instruction(
        bytes: &[u8; INSTRUCTION_BYTES_V1],
        index: usize,
    ) -> Result<QuotientInstruction, QuotientError> {
        let opcode = match u32_at(bytes, 0) {
            OP_ADD => QuotientOpcode::Add,
            OP_SUB => QuotientOpcode::Sub,
            OP_MUL => QuotientOpcode::Mul,
            OP_SQUARE => QuotientOpcode::Square,
            OP_DOUBLE => QuotientOpcode::Double,
            OP_NEGATE => QuotientOpcode::Negate,
            opcode => return Err(QuotientError::UnknownOpcode { instruction: index, opcode }),
        };
        let destination = u32_at(bytes, 4);
        let left = decode_operand(u32_at(bytes, 8), u32_at(bytes, 12), i64_at(bytes, 16), index)?;
        let right = decode_operand(u32_at(bytes, 24), u32_at(bytes, 28), i64_at(bytes, 32), index)?;
        let Some(left) = left else {
            return Err(QuotientError::Arity { instruction: index });
        };
        if instruction_is_binary(opcode) != right.is_some() {
            return Err(QuotientError::Arity { instruction: index });
        }
        Ok(QuotientInstruction {
            opcode,
            destination,
            left,
            right,
        })
    }

    /// Encode a version-1 envelope. Rejects arity mismatches, which have no
    /// canonical encoding, and counts that do not fit the `u32` field.
    pub fn encode_program(
        register_count: u32,
        output_register: u32,
        instructions: &[QuotientInstruction],
    ) -> Result<Vec<u8>, QuotientError> {
        let count = u32::try_from(instructions.len()).map_err(|_| QuotientError::LengthOverflow)?;
        let len = instructions
            .len()
            .checked_mul(INSTRUCTION_BYTES_V1)
            .and_then(|n| n.checked_add(HEADER_BYTES_V1))
            .ok_or(QuotientError::LengthOverflow)?;
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(&BYTECODE_VERSION_V1.to_le_bytes());
        out.extend_from_slice(&register_count.to_le_bytes());
        out.extend_from_slice(&output_register.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        for (index, instruction) in instructions.iter().enumerate() {
            if instruction_is_binary(instruction.opcode) != instruction.right.is_some() {
                return Err(QuotientError::Arity { instruction: index });
            }
            out.extend_from_slice(&encode_instruction(instruction));
        }
        Ok(out)
    }

    /// Decode a version-1 envelope. The version is checked first, before any
    /// length computation or allocation.
    pub fn decode_program(bytes: &[u8]) -> Result<DecodedProgram, QuotientError> {
        if bytes.len() >= 4 {
            let version = u32_at(bytes, 0);
            if version != BYTECODE_VERSION_V1 {
                return Err(QuotientError::UnsupportedVersion(version));
            }
        }
        if bytes.len() < HEADER_BYTES_V1 {
            return Err(QuotientError::Truncated);
        }
        let register_count = u32_at(bytes, 4);
        let output_register = u32_at(bytes, 8);
        let count = u32_at(bytes, 12) as usize;
        let body = count
            .checked_mul(INSTRUCTION_BYTES_V1)
            .and_then(|n| n.checked_add(HEADER_BYTES_V1))
            .ok_or(QuotientError::LengthOverflow)?;
        if bytes.len() < body {
            return Err(QuotientError::Truncated);
        }
        if bytes.len() > body {
            return Err(QuotientError::TrailingBytes);
        }
        let instructions = bytes[HEADER_BYTES_V1..]
            .chunks_exact(INSTRUCTION_BYTES_V1)
            .enumerate()
            .map(|(index, chunk)| {
                let mut b = [0u8; INSTRUCTION_BYTES_V1];
                b.copy_from_slice(chunk);
                decode_instruction(&b, index)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DecodedProgram {
            register_count,
            output_register,
            instructions,
        })
    }
}
