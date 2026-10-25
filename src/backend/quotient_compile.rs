//! P5 quotient numerator compiler: lowers stock [`Evaluator`] plus the
//! permutation, lookup and trash identities of `Evaluator::evaluate_numerator`
//! into version-1 quotient bytecode (`loki_backend_api::QuotientInstruction`).
//!
//! # Contract
//!
//! The emitted program computes, at every extended-domain row, exactly the
//! value stock `evaluate_numerator::<B>` writes to that row: the numerator
//! `nu`, *before* division by the vanishing polynomial. Term order is stock:
//! circuit instances in order; per instance the custom-gate polynomials
//! (Horner in `y` from the running accumulator), permutation identities
//! (first set, last set, set links, then each chunk), each lookup's five
//! identities, then each trash identity. The accumulator starts at zero once
//! for the whole instance sequence.
//!
//! Field arithmetic is exact, so only the *term sequence* (which identity
//! receives which power of `y`) is semantic; expression association inside a
//! term is free. The compiler nevertheless follows stock's calculation list
//! one-to-one: each `GraphEvaluator` is lowered per use (per instance, per
//! lookup/trash argument) with fresh registers, keeping stock's own
//! within-graph reuse and adding no cross-graph common-subexpression
//! elimination. `Store` calculations are eliminated by operand substitution.
//!
//! Challenges are frozen into the constant table at compile time; the
//! column table ([`ColumnBinding`]) names exactly which stock polynomial
//! (and which circuit instance) each column operand reads. Global-index
//! values are bound as columns: `l0`, `l_last`, `l_active_row` and
//! `omega^row` (the extended generator power used by the permutation's
//! `delta^j * beta * g_coset * omega^row` term).
//!
//! # Registers
//!
//! Lowering produces a single-assignment *logical* program. A linear-scan
//! allocator maps it to physical registers; [`AllocationCertificate`]
//! records the logical program and the map, and [`check_allocation`]
//! re-validates the emitted physical program against it by simulation,
//! independently of the allocator.

// Enum variants and plain data fields are documented at their type.
#![allow(missing_docs)]

use std::{borrow::Cow, collections::HashMap};

use ff::{PrimeField, WithSmallOrderMulGroup};

use super::api::{Operand, QuotientInstruction, QuotientOpcode};
use crate::{
    plonk::{
        evaluation::{Calculation, Evaluator, GraphEvaluator, ValueSource},
        lookup, permutation, trash, Any, ConstraintSystem, Expression,
    },
    poly::{EvaluationDomain, Polynomial, PolynomialRepresentation},
};

/// Which stock polynomial a program column reads. Instance-indexed entries
/// name the circuit instance (outer index of the prover's per-proof vectors).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColumnBinding {
    /// `fixed[column]` (pk fixed cosets), shared by all instances.
    Fixed(usize),
    Advice { instance: usize, column: usize },
    Instance { instance: usize, column: usize },
    /// `permutation_pk_cosets[column]` (sigma), indexed over all
    /// permutation columns.
    PermutationSigma(usize),
    PermutationProduct { instance: usize, set: usize },
    LookupProduct { instance: usize, lookup: usize },
    LookupPermutedInput { instance: usize, lookup: usize },
    LookupPermutedTable { instance: usize, lookup: usize },
    Trash { instance: usize, trash: usize },
    L0,
    LLast,
    LActiveRow,
    /// `omega^row` for the representation's generator `B::omega(domain)`.
    OmegaPower,
}

/// Frozen challenge values, after the stock transcript squeezes.
#[derive(Clone, Copy, Debug)]
pub struct NumeratorChallenges<'a, F> {
    pub challenges: &'a [F],
    pub beta: F,
    pub gamma: F,
    pub theta: F,
    pub trash_challenge: F,
    pub y: F,
}

/// Lowering failures. None of these is reachable from a stock-constructed
/// [`Evaluator`] for the same constraint system; they guard misuse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    /// A graph reads an intermediate no earlier calculation wrote. Stock
    /// would read the previous row's stale value; refuse instead.
    IntermediateBeforeDefinition { graph: &'static str, intermediate: usize },
    /// A graph references a rotation, constant or challenge out of range.
    GraphIndex { graph: &'static str },
    /// The trash selector is not a fixed-column query (stock `unreachable!`).
    TrashSelector { trash: usize },
    /// Evaluator and constraint system disagree in shape.
    EvaluatorShape,
    /// Domain parameters are inconsistent.
    Domain,
    /// More registers/constants/columns than the `u32` encoding allows.
    TooLarge,
    /// Host inputs do not match the compiled column table.
    Binding(&'static str),
    /// The allocation certificate does not validate the physical program.
    Certificate(AllocationError),
}

/// Logical (single-assignment) program and its physical register map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllocationCertificate {
    logical_instructions: Vec<QuotientInstruction>,
    logical_register_count: u32,
    logical_output: u32,
    /// `physical_of_logical[l]` is the physical register holding logical `l`.
    physical_of_logical: Vec<u32>,
}

/// A compiled numerator program with its constant and column tables.
///
/// Fields are private to this module so a caller cannot mutate a program
/// after its certificate was checked; `compile_numerator` checks the
/// certificate before returning and [`CompiledQuotient::verify`] re-checks
/// it at every submission (`bind_host_columns`).
#[derive(Clone, Debug)]
pub struct CompiledQuotient<F> {
    rows: usize,
    domain_log: u32,
    extended_domain_log: u32,
    rotation_scale: usize,
    register_count: u32,
    instructions: Vec<QuotientInstruction>,
    constants: Vec<F>,
    columns: Vec<ColumnBinding>,
    output_register: u32,
    certificate: AllocationCertificate,
    /// Committed-argument shape the program was compiled for; host or
    /// resident inputs must match it exactly.
    shape: ArgumentShape,
}

/// Number of circuit instances and per-instance argument counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArgumentShape {
    pub instances: usize,
    pub permutation_sets: usize,
    pub lookups: usize,
    pub trashcans: usize,
}

impl<F> CompiledQuotient<F> {
    pub fn rows(&self) -> usize {
        self.rows
    }
    pub fn domain_log(&self) -> u32 {
        self.domain_log
    }
    pub fn extended_domain_log(&self) -> u32 {
        self.extended_domain_log
    }
    pub fn rotation_scale(&self) -> usize {
        self.rotation_scale
    }
    pub fn register_count(&self) -> u32 {
        self.register_count
    }
    pub fn instructions(&self) -> &[QuotientInstruction] {
        &self.instructions
    }
    pub fn constants(&self) -> &[F] {
        &self.constants
    }
    pub fn columns(&self) -> &[ColumnBinding] {
        &self.columns
    }
    pub fn output_register(&self) -> u32 {
        self.output_register
    }
    pub fn certificate(&self) -> &AllocationCertificate {
        &self.certificate
    }
    pub fn shape(&self) -> ArgumentShape {
        self.shape
    }

    /// Submission-boundary check: table indices in range and the
    /// allocation certificate validates the physical program.
    pub fn verify(&self) -> Result<(), CompileError> {
        let in_range = |op: &Operand| match *op {
            Operand::Register(r) => r < self.register_count,
            Operand::Constant(c) => (c as usize) < self.constants.len(),
            Operand::Column { column, .. } => (column as usize) < self.columns.len(),
        };
        let ok = self.output_register < self.register_count
            && self.instructions.iter().all(|i| {
                i.destination < self.register_count
                    && in_range(&i.left)
                    && i.right.as_ref().map_or(true, in_range)
            });
        if !ok {
            return Err(CompileError::Certificate(AllocationError::ShapeMismatch {
                instruction: 0,
            }));
        }
        check_allocation(self).map_err(CompileError::Certificate)
    }
}

/// Why an allocation certificate does not match the emitted program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllocationError {
    /// Logical program defines a register twice or reads it undefined.
    NotSingleAssignment { instruction: usize },
    /// Program lengths, opcodes or non-register operands differ.
    ShapeMismatch { instruction: usize },
    /// A map entry is out of range, or an operand is not the mapped register.
    MapMismatch { instruction: usize },
    /// The physical register no longer holds the logical value being read.
    Clobbered { instruction: usize },
    /// The physical output does not hold the logical output at the end.
    Output,
}

/// Compile the full ordered numerator for `instances` circuit instances.
pub(crate) fn compile_numerator<F, B>(
    ev: &Evaluator<F>,
    cs: &ConstraintSystem<F>,
    domain: &EvaluationDomain<F>,
    instances: usize,
    challenges: &NumeratorChallenges<'_, F>,
) -> Result<CompiledQuotient<F>, CompileError>
where
    F: WithSmallOrderMulGroup<3>,
    B: PolynomialRepresentation,
{
    let domain_log = domain.k();
    let extended_domain_log = B::k(domain);
    let rows = B::len(domain);
    if extended_domain_log < domain_log
        || extended_domain_log >= usize::BITS
        || 1usize.checked_shl(extended_domain_log) != Some(rows)
    {
        return Err(CompileError::Domain);
    }
    let rotation_scale = 1usize << (extended_domain_log - domain_log);
    if ev.lookups.len() != cs.lookups.len() || ev.trashcans.len() != cs.trashcans.len() {
        return Err(CompileError::EvaluatorShape);
    }
    if challenges.challenges.len() != cs.num_challenges {
        return Err(CompileError::GraphIndex { graph: "challenges" });
    }

    let mut lw = Lowering::new(challenges);
    let zero = lw.constant(F::ZERO)?;
    let one = lw.constant(F::ONE)?;
    let y = lw.constant(challenges.y)?;
    let beta = lw.constant(challenges.beta)?;
    let gamma = lw.constant(challenges.gamma)?;

    let p = &cs.permutation;
    let chunk_len = cs.degree() - 2;
    let permutation_sets = if p.columns.is_empty() {
        0
    } else {
        p.columns.chunks(chunk_len).count()
    };
    let last_rotation = -((cs.blinding_factors() + 1) as i32);
    // Column j's delta factor beta * g_coset * DELTA^j; stock multiplies it
    // by omega^row (`beta_term`), bound here as the OmegaPower column.
    let mut delta = challenges.beta * B::g_coset(domain);
    let mut delta_constants = Vec::with_capacity(p.columns.len());
    for _ in 0..p.columns.len() {
        delta_constants.push(lw.constant(delta)?);
        delta *= F::DELTA;
    }

    let mut acc = zero;
    for instance in 0..instances {
        // Custom gates: Horner in y starting from the running accumulator.
        acc = lw.graph(&ev.custom_gates, "custom_gates", instance, acc)?;

        // Permutations.
        if permutation_sets > 0 {
            let z = |set| ColumnBinding::PermutationProduct { instance, set };
            let l0 = lw.column(ColumnBinding::L0, 0)?;
            let l_last = lw.column(ColumnBinding::LLast, 0)?;
            let l_active = lw.column(ColumnBinding::LActiveRow, 0)?;
            // l_0 * (1 - z_0)
            let z0 = lw.column(z(0), 0)?;
            let t = lw.bin(QuotientOpcode::Sub, one, z0)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l0)?;
            acc = lw.step(acc, y, t)?;
            // l_last * (z_l^2 - z_l)
            let zl = lw.column(z(permutation_sets - 1), 0)?;
            let t = lw.un(QuotientOpcode::Square, zl)?;
            let t = lw.bin(QuotientOpcode::Sub, t, zl)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l_last)?;
            acc = lw.step(acc, y, t)?;
            // l_0 * (z_i - z_{i-1}(omega^last X))
            for set in 1..permutation_sets {
                let cur = lw.column(z(set), 0)?;
                let prev = lw.column(z(set - 1), last_rotation)?;
                let t = lw.bin(QuotientOpcode::Sub, cur, prev)?;
                let t = lw.bin(QuotientOpcode::Mul, t, l0)?;
                acc = lw.step(acc, y, t)?;
            }
            // l_active * (z_i(omega X) prod (v + beta s + gamma)
            //             - z_i(X) prod (v + delta^j beta X + gamma))
            let omega_row = lw.column(ColumnBinding::OmegaPower, 0)?;
            for (set, columns) in p.columns.chunks(chunk_len).enumerate() {
                let bind = |column: &crate::plonk::Column<Any>| match column.column_type() {
                    Any::Advice(_) => ColumnBinding::Advice {
                        instance,
                        column: column.index(),
                    },
                    Any::Fixed => ColumnBinding::Fixed(column.index()),
                    Any::Instance => ColumnBinding::Instance {
                        instance,
                        column: column.index(),
                    },
                };
                let mut left = lw.column(z(set), 1)?;
                for (offset, column) in columns.iter().enumerate() {
                    let j = set * chunk_len + offset;
                    let v = lw.column(bind(column), 0)?;
                    let sigma = lw.column(ColumnBinding::PermutationSigma(j), 0)?;
                    let t = lw.bin(QuotientOpcode::Mul, beta, sigma)?;
                    let t = lw.bin(QuotientOpcode::Add, v, t)?;
                    let t = lw.bin(QuotientOpcode::Add, t, gamma)?;
                    left = lw.bin(QuotientOpcode::Mul, left, t)?;
                }
                let mut right = lw.column(z(set), 0)?;
                for (offset, column) in columns.iter().enumerate() {
                    let j = set * chunk_len + offset;
                    let v = lw.column(bind(column), 0)?;
                    let d = lw.bin(QuotientOpcode::Mul, delta_constants[j], omega_row)?;
                    let t = lw.bin(QuotientOpcode::Add, v, d)?;
                    let t = lw.bin(QuotientOpcode::Add, t, gamma)?;
                    right = lw.bin(QuotientOpcode::Mul, right, t)?;
                }
                let t = lw.bin(QuotientOpcode::Sub, left, right)?;
                let t = lw.bin(QuotientOpcode::Mul, t, l_active)?;
                acc = lw.step(acc, y, t)?;
            }
        }

        // Lookups: five identities each.
        for (n, graph) in ev.lookups.iter().enumerate() {
            let table = lw.graph(graph, "lookup", instance, zero)?;
            let l0 = lw.column(ColumnBinding::L0, 0)?;
            let l_last = lw.column(ColumnBinding::LLast, 0)?;
            let l_active = lw.column(ColumnBinding::LActiveRow, 0)?;
            let product = ColumnBinding::LookupProduct { instance, lookup: n };
            let input = ColumnBinding::LookupPermutedInput { instance, lookup: n };
            let z = lw.column(product, 0)?;
            let z_next = lw.column(product, 1)?;
            let a = lw.column(input, 0)?;
            let a_prev = lw.column(input, -1)?;
            let s = lw.column(ColumnBinding::LookupPermutedTable { instance, lookup: n }, 0)?;
            let a_minus_s = lw.bin(QuotientOpcode::Sub, a, s)?;
            // l_0 * (1 - z)
            let t = lw.bin(QuotientOpcode::Sub, one, z)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l0)?;
            acc = lw.step(acc, y, t)?;
            // l_last * (z^2 - z)
            let t = lw.un(QuotientOpcode::Square, z)?;
            let t = lw.bin(QuotientOpcode::Sub, t, z)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l_last)?;
            acc = lw.step(acc, y, t)?;
            // l_active * (z(omega X)(a' + beta)(s' + gamma) - z * table)
            let ab = lw.bin(QuotientOpcode::Add, a, beta)?;
            let sg = lw.bin(QuotientOpcode::Add, s, gamma)?;
            let left = lw.bin(QuotientOpcode::Mul, z_next, ab)?;
            let left = lw.bin(QuotientOpcode::Mul, left, sg)?;
            let right = lw.bin(QuotientOpcode::Mul, z, table)?;
            let t = lw.bin(QuotientOpcode::Sub, left, right)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l_active)?;
            acc = lw.step(acc, y, t)?;
            // l_0 * (a' - s')
            let t = lw.bin(QuotientOpcode::Mul, a_minus_s, l0)?;
            acc = lw.step(acc, y, t)?;
            // l_active * (a' - s') * (a' - a'(omega^-1 X))
            let d = lw.bin(QuotientOpcode::Sub, a, a_prev)?;
            let t = lw.bin(QuotientOpcode::Mul, a_minus_s, d)?;
            let t = lw.bin(QuotientOpcode::Mul, t, l_active)?;
            acc = lw.step(acc, y, t)?;
        }

        // Trashcans: compressed - (1 - q) * trash.
        for (n, graph) in ev.trashcans.iter().enumerate() {
            let compressed = lw.graph(graph, "trash", instance, zero)?;
            // Stock reads the selector at the current row, ignoring the
            // query's rotation.
            let q = match cs.trashcans[n].selector() {
                Expression::Fixed(query) => {
                    lw.column(ColumnBinding::Fixed(query.column_index()), 0)?
                }
                _ => return Err(CompileError::TrashSelector { trash: n }),
            };
            let trash_column = lw.column(ColumnBinding::Trash { instance, trash: n }, 0)?;
            let t = lw.bin(QuotientOpcode::Sub, one, q)?;
            let t = lw.bin(QuotientOpcode::Mul, t, trash_column)?;
            let t = lw.bin(QuotientOpcode::Sub, compressed, t)?;
            acc = lw.step(acc, y, t)?;
        }
    }

    // v1 requires an initialized output register.
    if !matches!(acc, Operand::Register(_)) {
        acc = lw.bin(QuotientOpcode::Add, acc, zero)?;
    }
    let Operand::Register(logical_output) = acc else {
        unreachable!()
    };
    let logical_register_count = lw.next;
    let (instructions, physical_of_logical, register_count) =
        allocate(&lw.code, logical_register_count, logical_output);
    let output_register = physical_of_logical[logical_output as usize];
    let compiled = CompiledQuotient {
        rows,
        domain_log,
        extended_domain_log,
        rotation_scale,
        register_count,
        instructions,
        constants: lw.constants,
        columns: lw.columns,
        output_register,
        certificate: AllocationCertificate {
            logical_instructions: lw.code,
            logical_register_count,
            logical_output,
            physical_of_logical,
        },
        shape: ArgumentShape {
            instances,
            permutation_sets,
            lookups: cs.lookups.len(),
            trashcans: cs.trashcans.len(),
        },
    };
    // Translation validation of the allocator on every compile.
    compiled.verify()?;
    Ok(compiled)
}

/// Single-assignment instruction builder with interned constant/column tables.
struct Lowering<'c, F: PrimeField> {
    challenges: &'c NumeratorChallenges<'c, F>,
    code: Vec<QuotientInstruction>,
    next: u32,
    constants: Vec<F>,
    constant_index: HashMap<Vec<u8>, u32>,
    columns: Vec<ColumnBinding>,
    column_index: HashMap<ColumnBinding, u32>,
}

impl<'c, F: PrimeField> Lowering<'c, F> {
    fn new(challenges: &'c NumeratorChallenges<'c, F>) -> Self {
        Self {
            challenges,
            code: Vec::new(),
            next: 0,
            constants: Vec::new(),
            constant_index: HashMap::new(),
            columns: Vec::new(),
            column_index: HashMap::new(),
        }
    }

    fn emit(
        &mut self,
        opcode: QuotientOpcode,
        left: Operand,
        right: Option<Operand>,
    ) -> Result<Operand, CompileError> {
        let destination = self.next;
        self.next = self.next.checked_add(1).ok_or(CompileError::TooLarge)?;
        self.code.push(QuotientInstruction {
            opcode,
            destination,
            left,
            right,
        });
        Ok(Operand::Register(destination))
    }

    fn bin(&mut self, op: QuotientOpcode, l: Operand, r: Operand) -> Result<Operand, CompileError> {
        self.emit(op, l, Some(r))
    }

    fn un(&mut self, op: QuotientOpcode, l: Operand) -> Result<Operand, CompileError> {
        self.emit(op, l, None)
    }

    /// `acc * y + term`, one ordered Horner step.
    fn step(&mut self, acc: Operand, y: Operand, term: Operand) -> Result<Operand, CompileError> {
        let scaled = self.bin(QuotientOpcode::Mul, acc, y)?;
        self.bin(QuotientOpcode::Add, scaled, term)
    }

    /// Interning by canonical value is safe: constants are values, not
    /// computations, so it is not common-subexpression elimination.
    fn constant(&mut self, value: F) -> Result<Operand, CompileError> {
        let key = value.to_repr().as_ref().to_vec();
        if let Some(&index) = self.constant_index.get(&key) {
            return Ok(Operand::Constant(index));
        }
        let index = u32::try_from(self.constants.len()).map_err(|_| CompileError::TooLarge)?;
        self.constants.push(value);
        self.constant_index.insert(key, index);
        Ok(Operand::Constant(index))
    }

    fn column(&mut self, binding: ColumnBinding, rotation: i32) -> Result<Operand, CompileError> {
        let column = match self.column_index.get(&binding) {
            Some(&index) => index,
            None => {
                let index = u32::try_from(self.columns.len()).map_err(|_| CompileError::TooLarge)?;
                self.columns.push(binding);
                self.column_index.insert(binding, index);
                index
            }
        };
        Ok(Operand::Column {
            column,
            rotation: i64::from(rotation),
        })
    }

    /// Lower one use of a graph with fresh registers. Mirrors
    /// `GraphEvaluator::evaluate`: every calculation in order, result is the
    /// last calculation's target (zero for an empty graph).
    fn graph(
        &mut self,
        graph: &GraphEvaluator<F>,
        name: &'static str,
        instance: usize,
        previous: Operand,
    ) -> Result<Operand, CompileError> {
        let constants = graph
            .constants
            .iter()
            .map(|c| self.constant(*c))
            .collect::<Result<Vec<_>, _>>()?;
        let mut intermediates: Vec<Option<Operand>> = vec![None; graph.num_intermediates];
        for info in &graph.calculations {
            let src = |lw: &mut Self, v: &ValueSource, inter: &[Option<Operand>]| {
                lw.source(v, graph, name, &constants, inter, instance, previous)
            };
            let value = match &info.calculation {
                Calculation::Add(a, b) | Calculation::Sub(a, b) | Calculation::Mul(a, b) => {
                    let op = match &info.calculation {
                        Calculation::Add(..) => QuotientOpcode::Add,
                        Calculation::Sub(..) => QuotientOpcode::Sub,
                        _ => QuotientOpcode::Mul,
                    };
                    let a = src(self, a, &intermediates)?;
                    let b = src(self, b, &intermediates)?;
                    self.bin(op, a, b)?
                }
                Calculation::Square(v) | Calculation::Double(v) | Calculation::Negate(v) => {
                    let op = match &info.calculation {
                        Calculation::Square(_) => QuotientOpcode::Square,
                        Calculation::Double(_) => QuotientOpcode::Double,
                        _ => QuotientOpcode::Negate,
                    };
                    let v = src(self, v, &intermediates)?;
                    self.un(op, v)?
                }
                Calculation::Horner(start, parts, factor) => {
                    let factor = src(self, factor, &intermediates)?;
                    let mut value = src(self, start, &intermediates)?;
                    for part in parts {
                        let part = src(self, part, &intermediates)?;
                        value = self.step(value, factor, part)?;
                    }
                    value
                }
                // Eliminated by operand substitution.
                Calculation::Store(v) => src(self, v, &intermediates)?,
            };
            *intermediates
                .get_mut(info.target)
                .ok_or(CompileError::GraphIndex { graph: name })? = Some(value);
        }
        match graph.calculations.last() {
            Some(info) => Ok(intermediates[info.target].expect("written above")),
            None => self.constant(F::ZERO),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn source(
        &mut self,
        value: &ValueSource,
        graph: &GraphEvaluator<F>,
        name: &'static str,
        constants: &[Operand],
        intermediates: &[Option<Operand>],
        instance: usize,
        previous: Operand,
    ) -> Result<Operand, CompileError> {
        let bad = CompileError::GraphIndex { graph: name };
        let rotation = |r: usize| graph.rotations.get(r).copied().ok_or(bad.clone());
        let ch = self.challenges;
        Ok(match *value {
            ValueSource::Constant(i) => *constants.get(i).ok_or(bad.clone())?,
            ValueSource::Intermediate(i) => intermediates.get(i).copied().flatten().ok_or(
                CompileError::IntermediateBeforeDefinition {
                    graph: name,
                    intermediate: i,
                },
            )?,
            ValueSource::Fixed(column, r) => self.column(ColumnBinding::Fixed(column), rotation(r)?)?,
            ValueSource::Advice(column, r) => {
                self.column(ColumnBinding::Advice { instance, column }, rotation(r)?)?
            }
            ValueSource::Instance(column, r) => {
                self.column(ColumnBinding::Instance { instance, column }, rotation(r)?)?
            }
            ValueSource::Challenge(i) => self.constant(*ch.challenges.get(i).ok_or(bad.clone())?)?,
            ValueSource::Beta() => self.constant(ch.beta)?,
            ValueSource::Gamma() => self.constant(ch.gamma)?,
            ValueSource::Theta() => self.constant(ch.theta)?,
            ValueSource::TrashChallenge() => self.constant(ch.trash_challenge)?,
            ValueSource::Y() => self.constant(ch.y)?,
            ValueSource::PreviousValue() => previous,
        })
    }
}

/// Linear-scan allocation of a single-assignment program: an operand's
/// register is freed at its last read (before the destination is chosen,
/// which is sound because operands are read before the write); a value never
/// read is freed right after its definition; the output is never freed.
/// The lowest free register is reused first.
fn allocate(
    logical: &[QuotientInstruction],
    logical_count: u32,
    output: u32,
) -> (Vec<QuotientInstruction>, Vec<u32>, u32) {
    use std::{cmp::Reverse, collections::BinaryHeap};

    let registers = |ins: &QuotientInstruction| {
        let mut regs = [None, None];
        if let Operand::Register(r) = ins.left {
            regs[0] = Some(r);
        }
        if let Some(Operand::Register(r)) = ins.right {
            if regs[0] != Some(r) {
                regs[1] = Some(r);
            }
        }
        regs.into_iter().flatten()
    };
    let mut last_use = vec![None; logical_count as usize];
    for (i, ins) in logical.iter().enumerate() {
        for r in registers(ins) {
            last_use[r as usize] = Some(i);
        }
    }
    last_use[output as usize] = Some(usize::MAX);

    let mut map = vec![u32::MAX; logical_count as usize];
    let mut free = BinaryHeap::new();
    let mut physical_count = 0u32;
    let mut physical = Vec::with_capacity(logical.len());
    let remap = |op: Operand, map: &[u32]| match op {
        Operand::Register(r) => Operand::Register(map[r as usize]),
        other => other,
    };
    for (i, ins) in logical.iter().enumerate() {
        let left = remap(ins.left, &map);
        let right = ins.right.map(|r| remap(r, &map));
        for r in registers(ins) {
            if last_use[r as usize] == Some(i) {
                free.push(Reverse(map[r as usize]));
            }
        }
        let destination = match free.pop() {
            Some(Reverse(p)) => p,
            None => {
                physical_count += 1;
                physical_count - 1
            }
        };
        map[ins.destination as usize] = destination;
        if last_use[ins.destination as usize].is_none() {
            free.push(Reverse(destination));
        }
        physical.push(QuotientInstruction {
            opcode: ins.opcode,
            destination,
            left,
            right,
        });
    }
    (physical, map, physical_count)
}

/// Independently check that the physical program implements the logical
/// single-assignment program under the certificate's register map.
pub fn check_allocation<F>(compiled: &CompiledQuotient<F>) -> Result<(), AllocationError> {
    let cert = &compiled.certificate;
    let logical = &cert.logical_instructions;
    let physical = &compiled.instructions;
    let map = &cert.physical_of_logical;
    // SSA: one logical register per instruction. Checked before any
    // vector sized by the (untrusted) count is allocated.
    if cert.logical_register_count as usize != logical.len() {
        return Err(AllocationError::ShapeMismatch { instruction: 0 });
    }
    let count = cert.logical_register_count as usize;
    if map.len() != count {
        return Err(AllocationError::MapMismatch { instruction: 0 });
    }
    if logical.len() != physical.len() {
        return Err(AllocationError::ShapeMismatch {
            instruction: logical.len().min(physical.len()),
        });
    }

    // 1. The logical program is single-assignment and define-before-use.
    let mut defined = vec![false; count];
    for (i, ins) in logical.iter().enumerate() {
        for op in core::iter::once(&ins.left).chain(ins.right.as_ref()) {
            if let Operand::Register(r) = *op {
                if !defined.get(r as usize).copied().unwrap_or(false) {
                    return Err(AllocationError::NotSingleAssignment { instruction: i });
                }
            }
        }
        match defined.get_mut(ins.destination as usize) {
            Some(slot @ false) => *slot = true,
            _ => return Err(AllocationError::NotSingleAssignment { instruction: i }),
        }
    }
    if !defined.get(cert.logical_output as usize).copied().unwrap_or(false) {
        return Err(AllocationError::Output);
    }
    if map.iter().any(|&p| p >= compiled.register_count) {
        return Err(AllocationError::MapMismatch { instruction: 0 });
    }

    // 2. Simulate which logical value each physical register holds; every
    //    physical read must find exactly the logical value the logical
    //    program reads there.
    let mut holds: Vec<Option<u32>> = vec![None; compiled.register_count as usize];
    let operand = |i: usize, l: &Operand, p: &Operand, holds: &[Option<u32>]| match (*l, *p) {
        (Operand::Register(l), Operand::Register(p)) => {
            if map[l as usize] != p {
                Err(AllocationError::MapMismatch { instruction: i })
            } else if holds[p as usize] != Some(l) {
                Err(AllocationError::Clobbered { instruction: i })
            } else {
                Ok(())
            }
        }
        (Operand::Register(_), _) | (_, Operand::Register(_)) => {
            Err(AllocationError::ShapeMismatch { instruction: i })
        }
        (l, p) if l == p => Ok(()),
        _ => Err(AllocationError::ShapeMismatch { instruction: i }),
    };
    for (i, (l, p)) in logical.iter().zip(physical).enumerate() {
        if l.opcode != p.opcode {
            return Err(AllocationError::ShapeMismatch { instruction: i });
        }
        operand(i, &l.left, &p.left, &holds)?;
        match (&l.right, &p.right) {
            (None, None) => {}
            (Some(lr), Some(pr)) => operand(i, lr, pr, &holds)?,
            _ => return Err(AllocationError::ShapeMismatch { instruction: i }),
        }
        if map[l.destination as usize] != p.destination {
            return Err(AllocationError::MapMismatch { instruction: i });
        }
        holds[p.destination as usize] = Some(l.destination);
    }
    if map[cert.logical_output as usize] != compiled.output_register
        || holds.get(compiled.output_register as usize).copied().flatten()
            != Some(cert.logical_output)
    {
        return Err(AllocationError::Output);
    }
    Ok(())
}

/// The polynomials `evaluate_numerator` receives, in representation `B`
/// (committed argument polynomials stay in coefficient form, as stock).
pub(crate) struct NumeratorInputs<'a, F: PrimeField, B> {
    pub fixed: &'a [Polynomial<F, B>],
    pub advice: &'a [&'a [Polynomial<F, B>]],
    pub instance: &'a [&'a [Polynomial<F, B>]],
    pub lookups: &'a [Vec<lookup::prover::Committed<F>>],
    pub trashcans: &'a [Vec<trash::prover::Committed<F>>],
    pub permutations: &'a [permutation::prover::Committed<F>],
    pub l0: &'a Polynomial<F, B>,
    pub l_last: &'a Polynomial<F, B>,
    pub l_active_row: &'a Polynomial<F, B>,
    pub permutation_pk_cosets: &'a [Polynomial<F, B>],
}

/// Materialize host columns in the compiled column-table order. Borrowed
/// columns are not copied; argument cosets and `omega^row` are computed
/// exactly as stock computes them (`B::coeff_to_self`, repeated `*omega`).
pub(crate) fn bind_host_columns<'a, F, B>(
    compiled: &CompiledQuotient<F>,
    domain: &EvaluationDomain<F>,
    inputs: &NumeratorInputs<'a, F, B>,
) -> Result<Vec<Cow<'a, [F]>>, CompileError>
where
    F: WithSmallOrderMulGroup<3>,
    B: PolynomialRepresentation,
{
    // Domain metadata first, before any conversion: the same extended
    // length can hide a different base k or rotation scale.
    let domain_ok = domain.k() == compiled.domain_log
        && B::k(domain) == compiled.extended_domain_log
        && B::len(domain) == compiled.rows
        && compiled
            .extended_domain_log
            .checked_sub(compiled.domain_log)
            .and_then(|d| 1usize.checked_shl(d))
            == Some(compiled.rotation_scale);
    if !domain_ok {
        return Err(CompileError::Binding("domain"));
    }
    compiled.verify()?;
    let shape = compiled.shape;
    let instances_ok = inputs.advice.len() == shape.instances
        && inputs.instance.len() == shape.instances
        && inputs.lookups.len() == shape.instances
        && inputs.trashcans.len() == shape.instances
        && inputs.permutations.len() == shape.instances;
    if !instances_ok {
        return Err(CompileError::Binding("instance count"));
    }
    if inputs.lookups.iter().any(|l| l.len() != shape.lookups)
        || inputs.trashcans.iter().any(|t| t.len() != shape.trashcans)
        || inputs.permutations.iter().any(|p| p.sets.len() != shape.permutation_sets)
    {
        return Err(CompileError::Binding("argument count"));
    }
    let missing = CompileError::Binding("missing column");
    let coset = |poly: &Polynomial<F, crate::poly::Coeff>| -> Cow<'a, [F]> {
        Cow::Owned(B::coeff_to_self(domain, poly.clone()).values)
    };
    let lookup = |instance: usize, lookup: usize| {
        inputs.lookups.get(instance).and_then(|l| l.get(lookup)).ok_or(missing.clone())
    };
    let mut out = Vec::with_capacity(compiled.columns.len());
    for binding in &compiled.columns {
        let column: Cow<'a, [F]> = match *binding {
            ColumnBinding::Fixed(c) => Cow::Borrowed(&inputs.fixed.get(c).ok_or(missing.clone())?[..]),
            ColumnBinding::Advice { instance, column } => Cow::Borrowed(
                &inputs.advice.get(instance).and_then(|a| a.get(column)).ok_or(missing.clone())?[..],
            ),
            ColumnBinding::Instance { instance, column } => Cow::Borrowed(
                &inputs.instance.get(instance).and_then(|a| a.get(column)).ok_or(missing.clone())?
                    [..],
            ),
            ColumnBinding::PermutationSigma(j) => {
                Cow::Borrowed(&inputs.permutation_pk_cosets.get(j).ok_or(missing.clone())?[..])
            }
            ColumnBinding::PermutationProduct { instance, set } => coset(
                &inputs
                    .permutations
                    .get(instance)
                    .and_then(|p| p.sets.get(set))
                    .ok_or(missing.clone())?
                    .permutation_product_poly,
            ),
            ColumnBinding::LookupProduct { instance, lookup: n } => {
                coset(&lookup(instance, n)?.product_poly)
            }
            ColumnBinding::LookupPermutedInput { instance, lookup: n } => {
                coset(&lookup(instance, n)?.permuted_input_poly)
            }
            ColumnBinding::LookupPermutedTable { instance, lookup: n } => {
                coset(&lookup(instance, n)?.permuted_table_poly)
            }
            ColumnBinding::Trash { instance, trash } => coset(
                &inputs
                    .trashcans
                    .get(instance)
                    .and_then(|t| t.get(trash))
                    .ok_or(missing.clone())?
                    .trash_poly,
            ),
            ColumnBinding::L0 => Cow::Borrowed(&inputs.l0[..]),
            ColumnBinding::LLast => Cow::Borrowed(&inputs.l_last[..]),
            ColumnBinding::LActiveRow => Cow::Borrowed(&inputs.l_active_row[..]),
            ColumnBinding::OmegaPower => {
                let omega = B::omega(domain);
                let mut values = Vec::with_capacity(compiled.rows);
                let mut power = F::ONE;
                for _ in 0..compiled.rows {
                    values.push(power);
                    power *= omega;
                }
                Cow::Owned(values)
            }
        };
        if column.len() != compiled.rows {
            return Err(CompileError::Binding("column length"));
        }
        out.push(column);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "quotient_compile_tests.rs"]
mod tests;
