//! P5 quotient bytecode: wire ABI golden vectors, decoder rejection, static
//! validation negatives, and the CPU interpreter against direct arithmetic.
//! No CUDA toolkit or device is required.

mod support;

use ff::Field;
use loki_backend_api::{ErrorKind, FailureEffect, Operand, QuotientInstruction, QuotientOpcode};
use loki_gpu::quotient::wire::{self, DecodedProgram, HEADER_BYTES_V1, INSTRUCTION_BYTES_V1};
use loki_gpu::quotient::{
    evaluate_host, rotated_row, validate_program, HostQuotientProgram, ProgramShape, QuotientError,
};
use support::Toy;

use QuotientOpcode::*;

fn reg(r: u32) -> Operand {
    Operand::Register(r)
}
fn cst(c: u32) -> Operand {
    Operand::Constant(c)
}
fn col(c: u32, rotation: i64) -> Operand {
    Operand::Column {
        column: c,
        rotation,
    }
}
fn bin(op: QuotientOpcode, d: u32, l: Operand, r: Operand) -> QuotientInstruction {
    QuotientInstruction {
        opcode: op,
        destination: d,
        left: l,
        right: Some(r),
    }
}
fn un(op: QuotientOpcode, d: u32, l: Operand) -> QuotientInstruction {
    QuotientInstruction {
        opcode: op,
        destination: d,
        left: l,
        right: None,
    }
}

// ---------------------------------------------------------------- wire ABI

#[test]
fn golden_binary_column_instruction() {
    // Sub r7 <- col[3] rot -1, const 258
    let bytes = wire::encode_instruction(&bin(Sub, 7, col(3, -1), cst(258)));
    let expected: [u8; 40] = [
        2, 0, 0, 0, // opcode Sub
        7, 0, 0, 0, // destination
        3, 0, 0, 0, // left tag column
        3, 0, 0, 0, // left index
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, // rotation -1
        2, 0, 0, 0, // right tag constant
        2, 1, 0, 0, // right index 258
        0, 0, 0, 0, 0, 0, 0, 0, // right rotation
    ];
    assert_eq!(bytes, expected);
}

#[test]
fn golden_unary_and_extreme_rotation() {
    let bytes = wire::encode_instruction(&un(Negate, 0x0102_0304, col(0xa0b0_c0d0, i64::MIN)));
    let mut expected = [0u8; 40];
    expected[0..4].copy_from_slice(&[6, 0, 0, 0]);
    expected[4..8].copy_from_slice(&[4, 3, 2, 1]);
    expected[8..12].copy_from_slice(&[3, 0, 0, 0]);
    expected[12..16].copy_from_slice(&[0xd0, 0xc0, 0xb0, 0xa0]);
    expected[16..24].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0x80]);
    // right: absent tag 0, index 0, rotation 0
    assert_eq!(bytes, expected);

    let bytes = wire::encode_instruction(&bin(Mul, 1, reg(2), col(5, i64::MAX)));
    assert_eq!(&bytes[0..4], &[3, 0, 0, 0]);
    assert_eq!(&bytes[8..16], &[1, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(&bytes[16..24], &[0; 8]);
    assert_eq!(&bytes[24..32], &[3, 0, 0, 0, 5, 0, 0, 0]);
    assert_eq!(&bytes[32..40], &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f]);
}

#[test]
fn golden_opcode_and_tag_numbers() {
    for (op, code) in [(Add, 1u8), (Sub, 2), (Mul, 3)] {
        assert_eq!(wire::encode_instruction(&bin(op, 0, reg(0), reg(0)))[0], code);
    }
    for (op, code) in [(Square, 4u8), (Double, 5), (Negate, 6)] {
        assert_eq!(wire::encode_instruction(&un(op, 0, cst(0)))[0], code);
    }
    let b = wire::encode_instruction(&bin(Add, 0, reg(9), cst(9)));
    assert_eq!((b[8], b[24]), (1, 2));
}

#[test]
fn golden_envelope() {
    let program = [un(Double, 0, cst(1)), bin(Add, 1, reg(0), col(0, 2))];
    let bytes = wire::encode_program(4, 1, &program).unwrap();
    assert_eq!(bytes.len(), HEADER_BYTES_V1 + 2 * INSTRUCTION_BYTES_V1);
    assert_eq!(&bytes[0..16], &[1, 0, 0, 0, 4, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]);
    assert_eq!(&bytes[16..56], &wire::encode_instruction(&program[0]));
    assert_eq!(&bytes[56..96], &wire::encode_instruction(&program[1]));
    assert_eq!(
        wire::decode_program(&bytes).unwrap(),
        DecodedProgram {
            register_count: 4,
            output_register: 1,
            instructions: program.to_vec(),
        }
    );
}

fn all_operands() -> Vec<Operand> {
    vec![
        reg(0),
        reg(u32::MAX),
        cst(0),
        cst(u32::MAX),
        col(0, 0),
        col(u32::MAX, i64::MIN),
        col(7, i64::MAX),
        col(1, -3),
    ]
}

#[test]
fn every_instruction_shape_round_trips() {
    let mut count = 0;
    for l in all_operands() {
        for op in [Square, Double, Negate] {
            let i = un(op, 3, l);
            assert_eq!(wire::decode_instruction(&wire::encode_instruction(&i), 0), Ok(i));
            count += 1;
        }
        for r in all_operands() {
            for op in [Add, Sub, Mul] {
                let i = bin(op, u32::MAX, l, r);
                assert_eq!(wire::decode_instruction(&wire::encode_instruction(&i), 0), Ok(i));
                count += 1;
            }
        }
    }
    assert_eq!(count, 8 * 3 + 64 * 3);
}

fn base() -> [u8; 40] {
    wire::encode_instruction(&bin(Add, 0, col(1, 1), reg(2)))
}

fn put32(b: &mut [u8; 40], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(b: &mut [u8; 40], off: usize, v: i64) {
    b[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

#[test]
fn decoder_rejects_unknown_opcodes_and_tags() {
    for opcode in [0u32, 7, 255, u32::MAX] {
        let mut b = base();
        put32(&mut b, 0, opcode);
        assert_eq!(
            wire::decode_instruction(&b, 5),
            Err(QuotientError::UnknownOpcode { instruction: 5, opcode })
        );
    }
    for tag in [4u32, u32::MAX] {
        let mut b = base();
        put32(&mut b, 8, tag);
        assert_eq!(wire::decode_instruction(&b, 1), Err(QuotientError::UnknownTag { instruction: 1, tag }));
        let mut b = base();
        put32(&mut b, 24, tag);
        assert_eq!(wire::decode_instruction(&b, 1), Err(QuotientError::UnknownTag { instruction: 1, tag }));
    }
}

#[test]
fn decoder_rejects_wrong_arity() {
    // Binary op with absent right.
    let mut b = base();
    put32(&mut b, 24, wire::TAG_ABSENT);
    put32(&mut b, 28, 0);
    assert_eq!(wire::decode_instruction(&b, 0), Err(QuotientError::Arity { instruction: 0 }));
    // Unary op with present right.
    let mut b = base();
    put32(&mut b, 0, wire::OP_SQUARE);
    assert_eq!(wire::decode_instruction(&b, 0), Err(QuotientError::Arity { instruction: 0 }));
    // Absent left, for either arity.
    for op in [wire::OP_ADD, wire::OP_NEGATE] {
        let mut b = wire::encode_instruction(&un(Negate, 0, reg(0)));
        put32(&mut b, 0, op);
        put32(&mut b, 8, wire::TAG_ABSENT);
        put32(&mut b, 12, 0);
        assert_eq!(wire::decode_instruction(&b, 0), Err(QuotientError::Arity { instruction: 0 }));
    }
}

#[test]
fn decoder_rejects_noncanonical_unused_fields() {
    // Register and constant operands must carry rotation zero.
    for (tag_off, rot_off) in [(8usize, 16usize), (24, 32)] {
        for tag in [wire::TAG_REGISTER, wire::TAG_CONSTANT] {
            let mut b = base();
            put32(&mut b, tag_off, tag);
            put64(&mut b, rot_off, 1);
            assert_eq!(wire::decode_instruction(&b, 2), Err(QuotientError::NonCanonical { instruction: 2 }));
        }
    }
    // Absent right operand must have zero index and rotation.
    let unary = wire::encode_instruction(&un(Double, 0, reg(0)));
    let mut b = unary;
    put32(&mut b, 28, 1);
    assert_eq!(wire::decode_instruction(&b, 0), Err(QuotientError::NonCanonical { instruction: 0 }));
    let mut b = unary;
    put64(&mut b, 32, -1);
    assert_eq!(wire::decode_instruction(&b, 0), Err(QuotientError::NonCanonical { instruction: 0 }));
}

#[test]
fn envelope_rejects_bad_version_and_lengths() {
    let good = wire::encode_program(2, 0, &[un(Square, 0, cst(0))]).unwrap();
    for version in [0u32, 2, u32::MAX] {
        let mut b = good.clone();
        b[0..4].copy_from_slice(&version.to_le_bytes());
        assert_eq!(wire::decode_program(&b), Err(QuotientError::UnsupportedVersion(version)));
    }
    for len in 0..HEADER_BYTES_V1 {
        assert_eq!(wire::decode_program(&good[..len]), Err(QuotientError::Truncated));
    }
    // Version is checked before anything else, even with a short buffer.
    assert_eq!(wire::decode_program(&[9, 0, 0, 0]), Err(QuotientError::UnsupportedVersion(9)));
    assert_eq!(wire::decode_program(&good[..good.len() - 1]), Err(QuotientError::Truncated));
    let mut long = good.clone();
    long.push(0);
    assert_eq!(wire::decode_program(&long), Err(QuotientError::TrailingBytes));
    // Count claims more instructions than present.
    let mut b = good.clone();
    b[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        wire::decode_program(&b),
        Err(QuotientError::Truncated | QuotientError::LengthOverflow)
    ));
    // Zero-count envelopes decode (the validator rejects them as empty).
    let empty = wire::encode_program(1, 0, &[]).unwrap();
    assert_eq!(wire::decode_program(&empty).unwrap().instructions, vec![]);
    // Instruction errors carry the instruction index.
    let mut b = wire::encode_program(2, 0, &[un(Square, 0, cst(0)), un(Square, 1, cst(0))]).unwrap();
    b[HEADER_BYTES_V1 + INSTRUCTION_BYTES_V1] = 9;
    assert_eq!(
        wire::decode_program(&b),
        Err(QuotientError::UnknownOpcode { instruction: 1, opcode: 9 })
    );
}

#[test]
fn encoder_rejects_wrong_arity() {
    let bad = QuotientInstruction {
        opcode: Add,
        destination: 0,
        left: reg(0),
        right: None,
    };
    assert_eq!(wire::encode_program(1, 0, &[bad]), Err(QuotientError::Arity { instruction: 0 }));
    let bad = QuotientInstruction {
        opcode: Negate,
        destination: 0,
        left: reg(0),
        right: Some(reg(0)),
    };
    assert_eq!(wire::encode_program(1, 0, &[bad]), Err(QuotientError::Arity { instruction: 0 }));
}

// ---------------------------------------------------------------- validator

fn shape(registers: u32, constants: usize, columns: usize, output: u32) -> ProgramShape {
    ProgramShape {
        rows: 16,
        domain_log: 2,
        extended_domain_log: 4,
        rotation_scale: 4,
        register_count: registers,
        constant_count: constants,
        column_count: columns,
        output_register: output,
    }
}

#[test]
fn validator_accepts_minimal_program() {
    assert_eq!(validate_program(&shape(1, 1, 0, 0), &[un(Double, 0, cst(0))]), Ok(()));
}

#[test]
fn validator_rejects_bad_domains() {
    let p = [un(Double, 0, cst(0))];
    let mut s = shape(1, 1, 0, 0);
    s.domain_log = 5;
    assert_eq!(validate_program(&s, &p), Err(QuotientError::InvalidDomainLog));
    let mut s = shape(1, 1, 0, 0);
    s.extended_domain_log = usize::BITS;
    s.domain_log = 0;
    assert_eq!(validate_program(&s, &p), Err(QuotientError::InvalidDomainLog));
    let mut s = shape(1, 1, 0, 0);
    s.rows = 15;
    assert_eq!(validate_program(&s, &p), Err(QuotientError::RowsMismatch));
    let mut s = shape(1, 1, 0, 0);
    s.rotation_scale = 2;
    assert_eq!(validate_program(&s, &p), Err(QuotientError::RotationScaleMismatch));
    // k == K is legal with scale 1.
    let mut s = shape(1, 1, 0, 0);
    s.domain_log = 4;
    s.rotation_scale = 1;
    assert_eq!(validate_program(&s, &p), Ok(()));
}

#[test]
fn validator_rejects_empty_program() {
    assert_eq!(validate_program(&shape(1, 1, 0, 0), &[]), Err(QuotientError::EmptyProgram));
}

#[test]
fn validator_rejects_arity() {
    let p = [QuotientInstruction {
        opcode: Mul,
        destination: 0,
        left: cst(0),
        right: None,
    }];
    assert_eq!(validate_program(&shape(1, 1, 0, 0), &p), Err(QuotientError::Arity { instruction: 0 }));
    let p = [un(Double, 0, cst(0)), QuotientInstruction {
        opcode: Square,
        destination: 0,
        left: cst(0),
        right: Some(cst(0)),
    }];
    assert_eq!(validate_program(&shape(1, 1, 0, 0), &p), Err(QuotientError::Arity { instruction: 1 }));
}

#[test]
fn validator_rejects_reads_before_definition() {
    // Uninitialized self-read.
    let p = [bin(Add, 0, reg(0), cst(0))];
    assert_eq!(
        validate_program(&shape(1, 1, 0, 0), &p),
        Err(QuotientError::UninitializedRegister { instruction: 0, register: 0 })
    );
    // Right-operand self-read.
    let p = [bin(Add, 0, cst(0), reg(0))];
    assert_eq!(
        validate_program(&shape(1, 1, 0, 0), &p),
        Err(QuotientError::UninitializedRegister { instruction: 0, register: 0 })
    );
    // Read of a register defined only later.
    let p = [un(Double, 0, reg(1)), un(Double, 1, cst(0))];
    assert_eq!(
        validate_program(&shape(2, 1, 0, 0), &p),
        Err(QuotientError::UninitializedRegister { instruction: 0, register: 1 })
    );
    // Overwriting an already initialized register is fine, and so is the
    // aliased read of its old value.
    let p = [un(Double, 0, cst(0)), bin(Mul, 0, reg(0), reg(0))];
    assert_eq!(validate_program(&shape(1, 1, 0, 0), &p), Ok(()));
}

#[test]
fn validator_rejects_out_of_bounds() {
    let s = shape(2, 2, 1, 0);
    assert_eq!(
        validate_program(&s, &[un(Double, 2, cst(0))]),
        Err(QuotientError::RegisterOutOfBounds { instruction: 0, register: 2 })
    );
    assert_eq!(
        validate_program(&s, &[un(Double, 0, cst(0)), bin(Add, 1, reg(0), reg(7))]),
        Err(QuotientError::RegisterOutOfBounds { instruction: 1, register: 7 })
    );
    assert_eq!(
        validate_program(&s, &[un(Double, 0, cst(2))]),
        Err(QuotientError::ConstantOutOfBounds { instruction: 0, index: 2 })
    );
    assert_eq!(
        validate_program(&s, &[bin(Add, 0, cst(0), col(1, 0))]),
        Err(QuotientError::ColumnOutOfBounds { instruction: 0, index: 1 })
    );
    // Extreme rotations are in range for the API.
    assert_eq!(
        validate_program(&s, &[bin(Add, 0, col(0, i64::MIN), col(0, i64::MAX))]),
        Ok(())
    );
}

#[test]
fn validator_rejects_bad_output_register() {
    let p = [un(Double, 0, cst(0))];
    assert_eq!(validate_program(&shape(2, 1, 0, 1), &p), Err(QuotientError::OutputUninitialized));
    assert_eq!(validate_program(&shape(2, 1, 0, 2), &p), Err(QuotientError::OutputOutOfBounds));
}

// ---------------------------------------------------------------- interpreter

fn t(v: u64) -> Toy {
    Toy::new(v)
}

fn column(seed: u64, rows: usize) -> Vec<Toy> {
    (0..rows as u64).map(|r| t(seed.wrapping_mul(1_000_003) ^ (r * r + 17 * r + seed))).collect()
}

fn run(
    rows_log: u32,
    domain_log: u32,
    registers: u32,
    instructions: &[QuotientInstruction],
    constants: &[Toy],
    columns: &[Vec<Toy>],
    output: u32,
) -> Result<Vec<Toy>, QuotientError> {
    let refs: Vec<&[Toy]> = columns.iter().map(Vec::as_slice).collect();
    evaluate_host(&HostQuotientProgram {
        rows: 1 << rows_log,
        domain_log,
        extended_domain_log: rows_log,
        rotation_scale: 1 << (rows_log - domain_log),
        register_count: registers,
        instructions,
        constants,
        columns: &refs,
        output_register: output,
    })
}

#[test]
fn rotation_index_matches_i128_reference() {
    for rows in [1usize, 2, 16, 1 << 20] {
        for scale in [1usize, 4, 1 << 20, usize::MAX >> 1] {
            for row in [0, rows - 1, rows / 2] {
                for rot in [i64::MIN, i64::MIN + 1, -17, -1, 0, 1, 3, i64::MAX] {
                    let want = (row as i128 + rot as i128 * scale as i128).rem_euclid(rows as i128) as usize;
                    assert_eq!(rotated_row(row, rot, scale, rows), want, "row {row} rot {rot} scale {scale} rows {rows}");
                }
            }
        }
    }
}

#[test]
fn each_opcode_matches_direct_arithmetic_on_every_row() {
    let rows = 16;
    let a = column(1, rows);
    let b = column(2, rows);
    let k = t(0x1234_5678_9abc);
    let cases: [(QuotientOpcode, fn(Toy, Toy) -> Toy); 6] = [
        (Add, |x, y| x + y),
        (Sub, |x, y| x - y),
        (Mul, |x, y| x * y),
        (Square, |x, _| x.square()),
        (Double, |x, _| x.double()),
        (Negate, |x, _| -x),
    ];
    for (op, f) in cases {
        let ins = if matches!(op, Add | Sub | Mul) {
            bin(op, 0, col(0, 0), col(1, 0))
        } else {
            un(op, 0, col(0, 0))
        };
        let out = run(4, 2, 1, &[ins], &[k], &[a.clone(), b.clone()], 0).unwrap();
        for r in 0..rows {
            assert_eq!(out[r], f(a[r], b[r]), "{op:?} row {r}");
        }
        // Register/constant operand forms of the same op.
        let ins2 = if matches!(op, Add | Sub | Mul) {
            bin(op, 1, reg(0), cst(0))
        } else {
            un(op, 1, reg(0))
        };
        let out = run(4, 2, 2, &[un(Double, 0, col(1, 0)), ins2], &[k], &[a.clone(), b.clone()], 1).unwrap();
        for r in 0..rows {
            assert_eq!(out[r], f(b[r].double(), k), "{op:?} reg/const row {r}");
        }
    }
}

#[test]
fn rotations_scale_and_wrap_multiple_times() {
    // k = 2, K = 4: rotation r reads row + 4r mod 16.
    let rows = 16;
    let a = column(3, rows);
    for rot in [-9i64, -4, -1, 0, 1, 3, 4, 5, 37, i64::MAX, i64::MIN] {
        let out = run(4, 2, 1, &[un(Double, 0, col(0, rot))], &[], &[a.clone()], 0).unwrap();
        for r in 0..rows {
            let src = (r as i128 + rot as i128 * 4).rem_euclid(16) as usize;
            assert_eq!(out[r], a[src].double(), "rot {rot} row {r}");
        }
    }
}

#[test]
fn overwrite_reads_old_value_before_write() {
    // r0 = a; r0 = r0 * r0 + ... Horner-like: acc = acc*y + term.
    let rows = 16;
    let a = column(4, rows);
    let b = column(5, rows);
    let y = t(7);
    let p = [
        bin(Add, 0, col(0, 0), cst(1)),  // r0 = a + 0
        bin(Mul, 0, reg(0), cst(0)),     // r0 = r0 * y
        bin(Add, 0, reg(0), col(1, -1)), // r0 = r0 + b[-1]
        bin(Sub, 1, reg(0), reg(0)),     // r1 = 0
        bin(Add, 0, reg(1), reg(0)),     // r0 = 0 + r0
    ];
    let out = run(4, 2, 2, &p, &[y, Toy::ZERO], &[a.clone(), b.clone()], 0).unwrap();
    for r in 0..rows {
        assert_eq!(out[r], a[r] * y + b[(r + 12) % 16]);
    }
}

#[test]
fn interpreter_rejects_bad_columns_before_evaluating() {
    let p = [un(Double, 0, col(0, 0))];
    assert_eq!(run(4, 2, 1, &p, &[], &[column(1, 15)], 0), Err(QuotientError::ColumnLength { column: 0 }));
    assert_eq!(
        run(4, 2, 1, &p, &[], &[column(1, 16), column(1, 17)], 0),
        Err(QuotientError::ColumnLength { column: 1 })
    );
    // Validation errors surface through the interpreter, too.
    assert_eq!(
        run(4, 2, 1, &[bin(Add, 0, reg(0), cst(0))], &[Toy::ONE], &[], 0),
        Err(QuotientError::UninitializedRegister { instruction: 0, register: 0 })
    );
}

#[test]
fn errors_map_to_side_effect_free_backend_errors() {
    let e = QuotientError::EmptyProgram.backend_error();
    assert_eq!((e.kind, e.effect), (ErrorKind::InvalidInput, FailureEffect::NotStarted));
    let e = QuotientError::ResourceLimit.backend_error();
    assert_eq!((e.kind, e.effect), (ErrorKind::Capacity, FailureEffect::NotStarted));
}
