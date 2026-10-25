//! P5 CPU differential: the compiled bytecode, run by the independent
//! `loki-gpu` CPU interpreter on the *physical* register allocation, must
//! equal `Evaluator::evaluate_numerator` on every extended-domain row.
//!
//! Oracle provenance: the reference is Loki's own
//! `crate::plonk::evaluation::Evaluator` (a stock-derived copy), not the
//! external midnight-proofs 0.8.3 crate, whose `evaluate_numerator` and
//! `compute_nu_poly` are private. A source audit (astra-gpu, 40d267e) found
//! Loki's `evaluation.rs` differs from 0.8.3 only in the `loki-harden`
//! rotation type/branch, the `loki-profile` guard and harden tests; these
//! tests run with default features (`committed-instances`) plus `loki-backend-api`, without `loki-harden`, where the
//! numerator arithmetic is stock-equivalent. "stock" in test names below
//! means that stock-derived evaluator. External stock acceptance is a later
//! whole-proof gate (stock 0.8.3 verifier), not claimed here.
//!
//! Two fixture families:
//! - synthetic constraint systems with random (deliberately unconstrained)
//!   committed polynomials: raw numerator equality does not need a
//!   satisfying witness, and this exercises every argument family, multiple
//!   permutation sets, large wrapping rotations and multiple circuit
//!   instances (accumulator carried, never reset);
//! - the W2 harness circuit, proven by the real `compute_trace` at k10-k14.
//!
//! All witnesses and randomness are synthetic (fixed public seeds).

use std::borrow::Cow;

use ff::Field;
use loki_gpu::quotient::{evaluate_host, HostQuotientProgram};
use midnight_curves::Fq;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use super::*;
use crate::{
    plonk::{
        evaluation::Evaluator, lookup, permutation, trash, ConstraintSystem, Constraints,
        Expression, FirstPhase,
    },
    poly::{Coeff, EvaluationDomain, ExtendedLagrangeCoeff, Polynomial, Rotation},
};

type Ext = ExtendedLagrangeCoeff;

fn random_poly(domain: &EvaluationDomain<Fq>, rng: &mut ChaCha20Rng) -> Polynomial<Fq, Coeff> {
    domain.coeff_from_vec((0..domain.n).map(|_| Fq::random(&mut *rng)).collect())
}

fn lagrange_to_coeff(domain: &EvaluationDomain<Fq>, values: Vec<Fq>) -> Polynomial<Fq, Coeff> {
    domain.lagrange_to_coeff(domain.lagrange_from_vec(values))
}

/// Interpret the compiled program over host columns.
fn interpret(compiled: &CompiledQuotient<Fq>, columns: &[Cow<'_, [Fq]>]) -> Vec<Fq> {
    let refs: Vec<&[Fq]> = columns.iter().map(|c| c.as_ref()).collect();
    evaluate_host(&HostQuotientProgram {
        rows: compiled.rows,
        domain_log: compiled.domain_log,
        extended_domain_log: compiled.extended_domain_log,
        rotation_scale: compiled.rotation_scale,
        register_count: compiled.register_count,
        instructions: &compiled.instructions,
        constants: &compiled.constants,
        columns: &refs,
        output_register: compiled.output_register,
    })
    .expect("compiled program validates")
}

fn assert_rows_equal(label: &str, got: &[Fq], want: &[Fq]) {
    assert_eq!(got.len(), want.len(), "{label}: numerator lengths");
    if let Some(row) = got.iter().zip(want).position(|(a, b)| a != b) {
        panic!("{label}: numerator mismatch at row {row} of {}", want.len());
    }
}

// ------------------------------------------------------------------ synthetic

struct Case {
    domain: EvaluationDomain<Fq>,
    cs: ConstraintSystem<Fq>,
    fixed: Vec<Polynomial<Fq, Coeff>>,
    advice: Vec<Vec<Polynomial<Fq, Coeff>>>,
    instance: Vec<Vec<Polynomial<Fq, Coeff>>>,
    sigma: Vec<Polynomial<Fq, Coeff>>,
    permutations: Vec<permutation::prover::Committed<Fq>>,
    lookups: Vec<Vec<lookup::prover::Committed<Fq>>>,
    trashcans: Vec<Vec<trash::prover::Committed<Fq>>>,
    weights: [Polynomial<Fq, Coeff>; 3],
}

/// Shape-varied synthetic system. `shape` controls degree (hence the
/// permutation chunk length), the number of gates/lookups/trashcans and the
/// number of circuit instances; `with_permutation` / `with_lookups` /
/// `with_trash` toggle families so empty families are covered too.
fn case(
    k: u32,
    shape: usize,
    with_permutation: bool,
    with_lookups: bool,
    with_trash: bool,
    rng: &mut ChaCha20Rng,
) -> Case {
    let n = 1i32 << k;
    let degree = 4 + shape;
    let chunk = degree - 2;
    let mut cs = ConstraintSystem::<Fq>::default();
    let advice: Vec<_> = (0..2 * chunk).map(|_| cs.advice_column()).collect();
    let fixed: Vec<_> = (0..2 + shape).map(|_| cs.fixed_column()).collect();
    let instance: Vec<_> = (0..1 + shape % 2).map(|_| cs.instance_column()).collect();
    let challenge = cs.challenge_usable_after(FirstPhase);
    let gate_selector = cs.selector();
    let trash_selector = cs.complex_selector();
    cs.set_minimum_degree(degree);

    if with_permutation {
        // Three sets with an uneven final set; all three column kinds.
        cs.enable_equality(fixed[0]);
        cs.enable_equality(instance[0]);
        for &column in advice.iter().take(2 * chunk - 1) {
            cs.enable_equality(column);
        }
    }

    // Large signed offsets wrap the base domain more than once.
    let rotations = [0, 1, -1, 2 * n + 3, -3 * n - 2];
    for gate in 0..2 + shape {
        let a = advice[(rng.next_u32() as usize) % advice.len()];
        let b = advice[(rng.next_u32() as usize) % advice.len()];
        let f = fixed[gate % fixed.len()];
        let i = instance[gate % instance.len()];
        let offset = rotations[(gate + shape) % rotations.len()];
        let constant = Fq::random(&mut *rng);
        cs.create_gate("p5 synthetic gate", |cells| {
            let a = cells.query_advice(a, Rotation(offset));
            let b = cells.query_advice(b, Rotation(rotations[(gate + 2) % rotations.len()]));
            let f = cells.query_fixed(f, Rotation(-1));
            let i = cells.query_instance(i, Rotation(n + 1));
            let c = cells.query_challenge(challenge);
            let expression = match gate % 3 {
                0 => a.clone() * b + f - i + c * Expression::Constant(constant),
                1 => -a.clone() + b.clone() * b + f * i + c,
                // Repeated subexpressions exercise within-graph reuse; the
                // constants 0/1/2 hit stock's elimination special cases.
                _ => {
                    (a.clone() + b.clone()) * (a.clone() + b.clone())
                        + Expression::Constant(Fq::from(2)) * f.clone()
                        + Expression::Constant(Fq::ONE) * i
                        + Expression::Constant(Fq::ZERO) * f
                        - (-c)
                }
            };
            Constraints::with_selector(
                gate_selector,
                vec![expression, a.clone() * a - Expression::Constant(constant)],
            )
        });
    }

    if with_lookups {
        for lookup in 0..1 + shape {
            cs.lookup_any("p5 synthetic lookup", |cells| {
                (0..1 + (lookup + shape) % 3)
                    .map(|column| {
                        let a = cells.query_advice(advice[column], Rotation(rotations[column]));
                        let i = cells.query_instance(instance[0], Rotation(-1));
                        let f = cells.query_fixed(fixed[column % fixed.len()], Rotation(1));
                        let c = cells.query_challenge(challenge);
                        (a + i + c, f + Expression::Constant(Fq::from((column + 1) as u64)))
                    })
                    .collect()
            });
        }
    }

    if with_trash {
        for trash in 0..1 + shape % 2 {
            cs.create_gate("p5 synthetic trash", |cells| {
                let a = cells.query_advice(advice[trash], Rotation(-n - 1));
                let b = cells.query_advice(advice[trash + 1], Rotation(2));
                let f = cells.query_fixed(fixed[0], Rotation::cur());
                let i = cells.query_instance(instance[0], Rotation::cur());
                let c = cells.query_challenge(challenge);
                Constraints::with_additive_selector(trash_selector, vec![a * b + c, f - i])
            });
        }
    }

    let rows = n as usize;
    let selector_values = (0..cs.num_selectors())
        .map(|selector| {
            (0..rows)
                .map(|row| (row + selector) % 5 != 0 && rng.next_u32() % 3 != 0)
                .collect()
        })
        .collect();
    let (cs, selectors) = cs.directly_convert_selectors_to_fixed(selector_values);
    let domain = EvaluationDomain::new(cs.degree() as u32, k);
    let mut fixed_coeffs: Vec<_> = fixed.iter().map(|_| random_poly(&domain, rng)).collect();
    fixed_coeffs.extend(selectors.into_iter().map(|v| lagrange_to_coeff(&domain, v)));
    assert_eq!(fixed_coeffs.len(), cs.num_fixed_columns());

    let last = rows - cs.blinding_factors() - 1;
    let weights = [0, 1, 2].map(|which| {
        let values = (0..rows)
            .map(|row| {
                let on = match which {
                    0 => row == 0,
                    1 => row == last,
                    _ => row < last,
                };
                if on {
                    Fq::ONE
                } else {
                    Fq::ZERO
                }
            })
            .collect();
        lagrange_to_coeff(&domain, values)
    });
    let instances = 1 + shape % 3;
    let chunk_len = cs.degree() - 2;
    Case {
        fixed: fixed_coeffs,
        advice: (0..instances)
            .map(|_| advice.iter().map(|_| random_poly(&domain, rng)).collect())
            .collect(),
        instance: (0..instances)
            .map(|_| instance.iter().map(|_| random_poly(&domain, rng)).collect())
            .collect(),
        sigma: cs.permutation.columns.iter().map(|_| random_poly(&domain, rng)).collect(),
        permutations: (0..instances)
            .map(|_| permutation::prover::Committed {
                sets: cs
                    .permutation
                    .columns
                    .chunks(chunk_len)
                    .map(|_| permutation::prover::CommittedSet {
                        permutation_product_poly: random_poly(&domain, rng),
                    })
                    .collect(),
            })
            .collect(),
        lookups: (0..instances)
            .map(|_| {
                cs.lookups
                    .iter()
                    .map(|_| lookup::prover::Committed {
                        product_poly: random_poly(&domain, rng),
                        permuted_input_poly: random_poly(&domain, rng),
                        permuted_table_poly: random_poly(&domain, rng),
                    })
                    .collect()
            })
            .collect(),
        trashcans: (0..instances)
            .map(|_| {
                cs.trashcans
                    .iter()
                    .map(|_| trash::prover::Committed {
                        trash_poly: random_poly(&domain, rng),
                    })
                    .collect()
            })
            .collect(),
        weights,
        cs,
        domain,
    }
}

/// `[y, beta, gamma, theta, trash_challenge, circuit_challenge]`.
fn check_case(label: &str, case: &Case, values: [Fq; 6]) -> CompiledQuotient<Fq> {
    let [y, beta, gamma, theta, trash_challenge, circuit_challenge] = values;
    let to_ext = |p: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, Ext>> {
        p.iter().map(|p| case.domain.coeff_to_extended(p.clone())).collect()
    };
    let fixed = to_ext(&case.fixed);
    let advice: Vec<_> = case.advice.iter().map(|v| to_ext(v)).collect();
    let instance: Vec<_> = case.instance.iter().map(|v| to_ext(v)).collect();
    let sigma = to_ext(&case.sigma);
    let weights = to_ext(&case.weights);
    let advice_refs: Vec<_> = advice.iter().map(Vec::as_slice).collect();
    let instance_refs: Vec<_> = instance.iter().map(Vec::as_slice).collect();
    let challenges = [circuit_challenge];

    let ev = Evaluator::new(&case.cs);
    let want = ev.evaluate_numerator::<Ext>(
        &case.domain,
        &case.cs,
        &advice_refs,
        &instance_refs,
        &fixed,
        &challenges,
        y,
        beta,
        gamma,
        theta,
        trash_challenge,
        &case.lookups,
        &case.trashcans,
        &case.permutations,
        &weights[0],
        &weights[1],
        &weights[2],
        &sigma,
    );

    let compiled = compile_numerator::<Fq, Ext>(
        &ev,
        &case.cs,
        &case.domain,
        case.advice.len(),
        &NumeratorChallenges {
            challenges: &challenges,
            beta,
            gamma,
            theta,
            trash_challenge,
            y,
        },
    )
    .expect("compiles");
    check_allocation(&compiled).expect("allocation certificate");
    let columns = bind_host_columns::<Fq, Ext>(
        &compiled,
        &case.domain,
        &NumeratorInputs {
            fixed: &fixed,
            advice: &advice_refs,
            instance: &instance_refs,
            lookups: &case.lookups,
            trashcans: &case.trashcans,
            permutations: &case.permutations,
            l0: &weights[0],
            l_last: &weights[1],
            l_active_row: &weights[2],
            permutation_pk_cosets: &sigma,
        },
    )
    .expect("binds");
    let got = interpret(&compiled, &columns);
    assert_rows_equal(label, &got, &want);
    compiled
}

fn challenge_sets(rng: &mut ChaCha20Rng) -> Vec<[Fq; 6]> {
    let random: [Fq; 6] = std::array::from_fn(|_| Fq::random(&mut *rng));
    vec![
        [Fq::ZERO; 6],
        [Fq::ONE; 6],
        [-Fq::ONE; 6],
        random,
        [Fq::ONE, Fq::ZERO, -Fq::ONE, random[3], Fq::ONE, random[5]],
    ]
}

#[test]
fn synthetic_all_families_match_stock_on_every_row() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0001);
    for (shape, k) in [(0usize, 4u32), (1, 5), (2, 4), (3, 6)] {
        let case = case(k, shape, true, true, true, &mut rng);
        assert!(case.advice.len() >= 1);
        for (c, values) in challenge_sets(&mut rng).into_iter().enumerate() {
            check_case(&format!("shape {shape} k {k} challenges {c}"), &case, values);
        }
    }
}

#[test]
fn synthetic_multiple_instances_carry_the_accumulator() {
    // shape 2 has three circuit instances.
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0002);
    let case = case(4, 2, true, true, true, &mut rng);
    assert_eq!(case.advice.len(), 3);
    let values = std::array::from_fn(|_| Fq::random(&mut rng));
    check_case("three instances", &case, values);
}

#[test]
fn synthetic_empty_families_match_stock() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0003);
    for bits in 0..8u8 {
        let (p, l, t) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
        let case = case(4, (bits % 3) as usize, p, l, t, &mut rng);
        let values = std::array::from_fn(|_| Fq::random(&mut rng));
        check_case(&format!("families p{p} l{l} t{t}"), &case, values);
    }
}

#[test]
fn compiled_program_round_trips_the_v1_wire_format() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0004);
    let case = case(4, 1, true, true, true, &mut rng);
    let values = std::array::from_fn(|_| Fq::random(&mut rng));
    let compiled = check_case("wire", &case, values);
    let bytes = loki_gpu::quotient::wire::encode_program(
        compiled.register_count,
        compiled.output_register,
        &compiled.instructions,
    )
    .unwrap();
    let decoded = loki_gpu::quotient::wire::decode_program(&bytes).unwrap();
    assert_eq!(decoded.instructions, compiled.instructions);
    assert_eq!(decoded.register_count, compiled.register_count);
    assert_eq!(decoded.output_register, compiled.output_register);
}

#[test]
fn allocation_reuses_registers_and_certificate_catches_corruption() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0005);
    let case = case(4, 1, true, true, true, &mut rng);
    let values = std::array::from_fn(|_| Fq::random(&mut rng));
    let compiled = check_case("alloc", &case, values);
    let cert = &compiled.certificate;
    assert!(
        compiled.register_count < cert.logical_register_count,
        "allocation should reuse registers: {} physical for {} logical",
        compiled.register_count,
        cert.logical_register_count
    );

    // Redirect one register read to a different physical register that is
    // in range: the certificate check must reject it.
    let mut corrupted = compiled.clone();
    let (index, slot) = corrupted
        .instructions
        .iter()
        .enumerate()
        .find_map(|(i, ins)| match ins.left {
            crate::backend::api::Operand::Register(r) => Some((i, r)),
            _ => None,
        })
        .expect("a register read");
    let other = (slot + 1) % corrupted.register_count;
    corrupted.instructions[index].left = crate::backend::api::Operand::Register(other);
    assert!(check_allocation(&corrupted).is_err());

    // Clobbering a live value: retarget one destination onto a register
    // holding a value that is read later.
    let mut corrupted = compiled.clone();
    let victim = corrupted
        .instructions
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, ins)| ins.destination != corrupted.instructions[0].destination)
        .map(|(i, _)| i)
        .unwrap();
    corrupted.instructions[victim].destination = corrupted.instructions[0].destination;
    assert!(check_allocation(&corrupted).is_err());

    // A changed map entry with an unchanged program is also caught.
    let mut corrupted = compiled.clone();
    let l = corrupted.certificate.physical_of_logical.len() / 2;
    corrupted.certificate.physical_of_logical[l] =
        (corrupted.certificate.physical_of_logical[l] + 1) % corrupted.register_count;
    assert!(check_allocation(&corrupted).is_err());
}

/// Remap logical register `l` to physical `q` *consistently* (map, its
/// definition and every read): only the liveness simulation can tell a
/// clobbering remap from a harmless one. Soundness property: whenever the
/// checker accepts, the interpreter output is unchanged.
#[test]
fn certificate_checker_is_sound_under_consistent_remaps() {
    use crate::backend::api::Operand;
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0007);
    let case = case(4, 0, true, true, true, &mut rng);
    let values: [Fq; 6] = std::array::from_fn(|_| Fq::random(&mut rng));
    let compiled = check_case("remap", &case, values);
    let [y, beta, gamma, theta, trash_challenge, circuit_challenge] = values;
    let _ = (y, beta, gamma, theta, trash_challenge, circuit_challenge);
    let to_ext = |p: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, Ext>> {
        p.iter().map(|p| case.domain.coeff_to_extended(p.clone())).collect()
    };
    let fixed = to_ext(&case.fixed);
    let advice: Vec<_> = case.advice.iter().map(|v| to_ext(v)).collect();
    let instance: Vec<_> = case.instance.iter().map(|v| to_ext(v)).collect();
    let sigma = to_ext(&case.sigma);
    let weights = to_ext(&case.weights);
    let advice_refs: Vec<_> = advice.iter().map(Vec::as_slice).collect();
    let instance_refs: Vec<_> = instance.iter().map(Vec::as_slice).collect();
    let columns = bind_host_columns::<Fq, Ext>(
        &compiled,
        &case.domain,
        &NumeratorInputs {
            fixed: &fixed,
            advice: &advice_refs,
            instance: &instance_refs,
            lookups: &case.lookups,
            trashcans: &case.trashcans,
            permutations: &case.permutations,
            l0: &weights[0],
            l_last: &weights[1],
            l_active_row: &weights[2],
            permutation_pk_cosets: &sigma,
        },
    )
    .unwrap();
    let want = interpret(&compiled, &columns);

    let cert = &compiled.certificate;
    let (mut accepted, mut rejected, mut changed) = (0, 0, 0);
    let stride = (cert.logical_register_count as usize / 25).max(1);
    for l in (0..cert.logical_register_count).step_by(stride) {
        for q in 0..compiled.register_count {
            let old = cert.physical_of_logical[l as usize];
            if q == old {
                continue;
            }
            let mut c = compiled.clone();
            c.certificate.physical_of_logical[l as usize] = q;
            for (lg, ph) in cert.logical_instructions.iter().zip(c.instructions.iter_mut()) {
                if lg.destination == l {
                    ph.destination = q;
                }
                if lg.left == Operand::Register(l) {
                    ph.left = Operand::Register(q);
                }
                if lg.right == Some(Operand::Register(l)) {
                    ph.right = Some(Operand::Register(q));
                }
            }
            if cert.logical_output == l {
                c.output_register = q;
            }
            let got = interpret(&c, &columns);
            if got != want {
                changed += 1;
            }
            match check_allocation(&c) {
                Ok(()) => {
                    accepted += 1;
                    assert_eq!(got, want, "checker accepted a remap of {l} to {q} that changes output");
                }
                Err(_) => rejected += 1,
            }
        }
    }
    assert!(rejected > 0 && changed > 0, "no clobbering remap was exercised");
    eprintln!("consistent remaps: {accepted} accepted, {rejected} rejected, {changed} changed output");
}

/// The submission boundary (`bind_host_columns`) re-verifies a compiled
/// program: domain metadata against the actual domain before any coset
/// conversion, and the allocation certificate, so a mutated
/// `CompiledQuotient` cannot be bound merely because a certificate exists.
#[test]
fn binding_rechecks_domain_and_certificate() {
    use crate::backend::api::Operand;
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0008);
    let case = case(4, 0, true, true, true, &mut rng);
    let values: [Fq; 6] = std::array::from_fn(|_| Fq::random(&mut rng));
    let compiled = check_case("rebind", &case, values);
    let to_ext = |p: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, Ext>> {
        p.iter().map(|p| case.domain.coeff_to_extended(p.clone())).collect()
    };
    let fixed = to_ext(&case.fixed);
    let advice: Vec<_> = case.advice.iter().map(|v| to_ext(v)).collect();
    let instance: Vec<_> = case.instance.iter().map(|v| to_ext(v)).collect();
    let sigma = to_ext(&case.sigma);
    let weights = to_ext(&case.weights);
    let advice_refs: Vec<_> = advice.iter().map(Vec::as_slice).collect();
    let instance_refs: Vec<_> = instance.iter().map(Vec::as_slice).collect();
    let inputs = NumeratorInputs {
        fixed: &fixed,
        advice: &advice_refs,
        instance: &instance_refs,
        lookups: &case.lookups,
        trashcans: &case.trashcans,
        permutations: &case.permutations,
        l0: &weights[0],
        l_last: &weights[1],
        l_active_row: &weights[2],
        permutation_pk_cosets: &sigma,
    };
    let bind = |c: &CompiledQuotient<Fq>| bind_host_columns::<Fq, Ext>(c, &case.domain, &inputs);
    assert!(bind(&compiled).is_ok());

    // Same extended length, different base k: must be refused.
    let mut c = compiled.clone();
    c.domain_log -= 1;
    c.rotation_scale *= 2;
    assert_eq!(bind(&c).unwrap_err(), CompileError::Binding("domain"));
    let mut c = compiled.clone();
    c.extended_domain_log += 1;
    assert_eq!(bind(&c).unwrap_err(), CompileError::Binding("domain"));
    let mut c = compiled.clone();
    c.rotation_scale += 1;
    assert_eq!(bind(&c).unwrap_err(), CompileError::Binding("domain"));

    // A corrupted certificate or physical program is refused at binding.
    let mut c = compiled.clone();
    let l = c.certificate.physical_of_logical.len() / 2;
    c.certificate.physical_of_logical[l] =
        (c.certificate.physical_of_logical[l] + 1) % c.register_count;
    assert!(matches!(bind(&c).unwrap_err(), CompileError::Certificate(_)));
    let mut c = compiled.clone();
    let i = c.instructions.iter().position(|x| matches!(x.left, Operand::Register(_))).unwrap();
    if let Operand::Register(r) = c.instructions[i].left {
        c.instructions[i].left = Operand::Register((r + 1) % c.register_count);
    }
    assert!(matches!(bind(&c).unwrap_err(), CompileError::Certificate(_)));
}

#[test]
fn certificate_register_count_is_checked_before_allocating() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0009);
    let case = case(4, 0, true, true, true, &mut rng);
    let values: [Fq; 6] = std::array::from_fn(|_| Fq::random(&mut rng));
    let compiled = check_case("count", &case, values);
    // SSA: exactly one logical register per instruction.
    assert_eq!(
        compiled.certificate.logical_register_count as usize,
        compiled.certificate.logical_instructions.len()
    );
    let mut c = compiled.clone();
    c.certificate.logical_register_count = u32::MAX;
    assert_eq!(check_allocation(&c), Err(AllocationError::ShapeMismatch { instruction: 0 }));
}

#[test]
fn binding_rejects_mismatched_committed_shapes() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x7035_7175_6f74_0006);
    let case = case(4, 0, true, true, true, &mut rng);
    let ev = Evaluator::new(&case.cs);
    let ch = NumeratorChallenges {
        challenges: &[Fq::ONE],
        beta: Fq::ONE,
        gamma: Fq::ONE,
        theta: Fq::ONE,
        trash_challenge: Fq::ONE,
        y: Fq::ONE,
    };
    let compiled = compile_numerator::<Fq, Ext>(&ev, &case.cs, &case.domain, 1, &ch).unwrap();
    let to_ext = |p: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, Ext>> {
        p.iter().map(|p| case.domain.coeff_to_extended(p.clone())).collect()
    };
    let fixed = to_ext(&case.fixed);
    let advice = to_ext(&case.advice[0]);
    let instance = to_ext(&case.instance[0]);
    let sigma = to_ext(&case.sigma);
    let weights = to_ext(&case.weights);
    let no_lookups: Vec<Vec<lookup::prover::Committed<Fq>>> = vec![vec![]];
    let advice_refs = [advice.as_slice()];
    let instance_refs = [instance.as_slice()];
    let result = bind_host_columns::<Fq, Ext>(
        &compiled,
        &case.domain,
        &NumeratorInputs {
            fixed: &fixed,
            advice: &advice_refs,
            instance: &instance_refs,
            lookups: &no_lookups,
            trashcans: &case.trashcans[..1],
            permutations: &case.permutations[..1],
            l0: &weights[0],
            l_last: &weights[1],
            l_active_row: &weights[2],
            permutation_pk_cosets: &sigma,
        },
    );
    assert!(result.is_err());
    // A challenge vector shorter than the circuit's challenges is rejected.
    let short = NumeratorChallenges { challenges: &[], ..ch };
    assert!(compile_numerator::<Fq, Ext>(&ev, &case.cs, &case.domain, 1, &short).is_err());
}

// ------------------------------------------------------------- W2 harness

mod harness {
    use blake2b_simd::State;
    use ff::Field;
    use midnight_curves::{Bls12, Fq};
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;

    use super::*;
    use crate::{
        circuit::{Layouter, SimpleFloorPlanner, Value},
        plonk::{
            compute_trace, keygen_pk, keygen_vk_with_k, Advice, Challenge, Circuit,
            Column, ConstraintSystem, Constraints, Error, FirstPhase, Fixed, Instance,
            SecondPhase, TableColumn,
        },
        poly::{kzg::params::ParamsKZG, kzg::KZGCommitmentScheme, Rotation},
        transcript::{CircuitTranscript, Transcript},
    };

    // The W2 harness circuit (loki-harness/src/backend.rs): a nonlinear
    // challenge-dependent gate, copies on every row, a public input, an 8-bit
    // lookup and a second-phase column.
    #[derive(Clone, Debug)]
    pub struct Config {
        x: Column<Advice>,
        copy: Column<Advice>,
        late: Column<Advice>,
        active: Column<Fixed>,
        public: Column<Instance>,
        theta: Challenge,
        table: TableColumn,
    }

    #[derive(Clone)]
    pub struct Fixture {
        rows: usize,
        witness: bool,
    }

    impl Circuit<Fq> for Fixture {
        type Config = Config;
        type FloorPlanner = SimpleFloorPlanner;
        #[cfg(feature = "circuit-params")]
        type Params = ();

        fn without_witnesses(&self) -> Self {
            Self {
                witness: false,
                ..self.clone()
            }
        }

        fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
            let x = meta.advice_column();
            let copy = meta.advice_column();
            let late = meta.advice_column_in(SecondPhase);
            let theta = meta.challenge_usable_after(FirstPhase);
            let active = meta.fixed_column();
            let public = meta.instance_column();
            for column in [x, copy] {
                meta.enable_equality(column);
            }
            meta.enable_equality(public);
            meta.create_gate("nonlinear challenge-dependent gate", |meta| {
                let q = meta.query_fixed(active, Rotation::cur());
                let x = meta.query_advice(x, Rotation::cur());
                let copied = meta.query_advice(copy, Rotation::cur());
                let late = meta.query_advice(late, Rotation::cur());
                let theta = meta.query_challenge(theta);
                Constraints::without_selector(vec![q * (late - x.clone() * x - theta * copied)])
            });
            let table = meta.lookup_table_column();
            meta.lookup("8-bit range", |meta| {
                let q = meta.query_fixed(active, Rotation::cur());
                let x = meta.query_advice(x, Rotation::cur());
                vec![(q * x, table)]
            });
            Config {
                x,
                copy,
                late,
                active,
                public,
                theta,
                table,
            }
        }

        fn synthesize(&self, config: Config, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
            layouter.assign_table(
                || "duplicated 0..255",
                |mut table| {
                    for row in 0..512 {
                        table.assign_cell(
                            || "range value",
                            config.table,
                            row,
                            || Value::known(Fq::from((row % 256) as u64)),
                        )?;
                    }
                    Ok(())
                },
            )?;
            let theta = layouter.get_challenge(config.theta);
            let first = layouter.assign_region(
                || "dense arithmetic",
                |mut region| {
                    let mut first = None;
                    for row in 0..self.rows {
                        region.assign_fixed(|| "active", config.active, row, || Value::known(Fq::ONE))?;
                        let x = Fq::from(((row * 17 + 7) % 256) as u64);
                        let value = |x| if self.witness { Value::known(x) } else { Value::unknown() };
                        let cell = region.assign_advice(|| "x", config.x, row, || value(x))?;
                        let copy = region.assign_advice(|| "copy", config.copy, row, || value(x))?;
                        region.constrain_equal(cell.cell(), copy.cell())?;
                        let late = theta.map(|theta| x.square() + theta * x);
                        region.assign_advice(|| "second phase", config.late, row, || late)?;
                        if row == 0 {
                            first = Some(cell.cell());
                        }
                    }
                    Ok(first.expect("fixture has rows"))
                },
            )?;
            layouter.constrain_instance(first, config.public, 0)
        }
    }

    /// Prove `circuits` copies of the harness at `k` up to the trace, then
    /// compare stock `compute_nu_poly`'s numerator with the compiled program.
    pub fn check(k: u32, circuits: usize) {
        check_timed(k, circuits, None)
    }

    /// As [`check`]; with `dump`, also time stock `evaluate_numerator`
    /// (rayon), the argument-coset binding, and write the program shape
    /// (v1 envelope, constants, rows/scale logs; no column data) for a GPU
    /// replay with synthetic columns.
    pub fn check_timed(k: u32, circuits: usize, dump: Option<&std::path::Path>) {
        let params = ParamsKZG::<Bls12>::unsafe_setup(k, ChaCha20Rng::from_seed([0x53; 32]));
        let circuit = Fixture {
            rows: (1usize << k) - 32,
            witness: true,
        };
        let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(
            &params,
            &circuit.without_witnesses(),
            k,
        )
        .unwrap();
        let pk = keygen_pk(vk, &circuit.without_witnesses()).unwrap();
        let public = [Fq::from(7)];
        let instances: &[&[Fq]] = &[&public];
        let all_instances = vec![instances; circuits];
        let mut transcript = CircuitTranscript::<State>::init();
        let trace = compute_trace(
            &params,
            &pk,
            &vec![circuit; circuits],
            #[cfg(feature = "committed-instances")]
            0,
            &all_instances,
            &mut transcript,
            &mut ChaCha20Rng::from_seed([0x42; 32]),
        )
        .unwrap();

        // Exactly stock `compute_nu_poly` (pub(super) in plonk::prover):
        // advice/instance extended cosets, then `evaluate_numerator`.
        let domain = pk.get_vk().get_domain();
        let to_ext = |polys: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, Ext>> {
            polys.iter().map(|p| domain.coeff_to_extended(p.clone())).collect()
        };
        let advice: Vec<_> = trace.advice_polys.iter().map(|v| to_ext(v)).collect();
        let instance: Vec<_> = trace.instance_polys.iter().map(|v| to_ext(v)).collect();
        let advice_refs: Vec<_> = advice.iter().map(Vec::as_slice).collect();
        let instance_refs: Vec<_> = instance.iter().map(Vec::as_slice).collect();
        let started = std::time::Instant::now();
        let want = pk.ev.evaluate_numerator::<Ext>(
            domain,
            pk.get_vk().cs(),
            &advice_refs,
            &instance_refs,
            &pk.fixed_cosets,
            &trace.challenges,
            trace.y,
            trace.beta,
            trace.gamma,
            trace.theta,
            trace.trash_challenge,
            &trace.lookups,
            &trace.trashcans,
            &trace.permutations,
            &pk.l0,
            &pk.l_last,
            &pk.l_active_row,
            &pk.permutation.cosets,
        );
        let stock_eval = started.elapsed();
        let compiled = compile_numerator::<Fq, Ext>(
            &pk.ev,
            pk.get_vk().cs(),
            domain,
            circuits,
            &NumeratorChallenges {
                challenges: &trace.challenges,
                beta: trace.beta,
                gamma: trace.gamma,
                theta: trace.theta,
                trash_challenge: trace.trash_challenge,
                y: trace.y,
            },
        )
        .unwrap();
        check_allocation(&compiled).unwrap();
        let started = std::time::Instant::now();
        let columns = bind_host_columns::<Fq, Ext>(
            &compiled,
            domain,
            &NumeratorInputs {
                fixed: &pk.fixed_cosets,
                advice: &advice_refs,
                instance: &instance_refs,
                lookups: &trace.lookups,
                trashcans: &trace.trashcans,
                permutations: &trace.permutations,
                l0: &pk.l0,
                l_last: &pk.l_last,
                l_active_row: &pk.l_active_row,
                permutation_pk_cosets: &pk.permutation.cosets,
            },
        )
        .unwrap();
        let bind = started.elapsed();
        if let Some(path) = dump {
            use std::io::Write;
            let program = loki_gpu::quotient::wire::encode_program(
                compiled.register_count,
                compiled.output_register,
                &compiled.instructions,
            )
            .unwrap();
            let mut f = std::fs::File::create(path).unwrap();
            // [rows_log u32][scale_log u32][columns u32][constants u32][program..][constants..]
            for v in [
                compiled.extended_domain_log,
                compiled.extended_domain_log - compiled.domain_log,
                compiled.columns.len() as u32,
                compiled.constants.len() as u32,
            ] {
                f.write_all(&v.to_le_bytes()).unwrap();
            }
            f.write_all(&(program.len() as u32).to_le_bytes()).unwrap();
            f.write_all(&program).unwrap();
            for c in &compiled.constants {
                f.write_all(ff::PrimeField::to_repr(c).as_ref()).unwrap();
            }
            eprintln!(
                "harness k{k} x{circuits} timing: stock evaluate_numerator {:?} (rayon {} threads); bind_host_columns (argument cosets + omega) {:?}",
                stock_eval,
                rayon::current_num_threads(),
                bind
            );
        }
        let started = std::time::Instant::now();
        let got = interpret(&compiled, &columns);
        let interp = started.elapsed();
        if dump.is_some() {
            eprintln!("harness k{k} x{circuits}: single-thread CPU bytecode interpreter {interp:?}");
        }
        assert_rows_equal(&format!("harness k{k} x{circuits}"), &got, &want);
        eprintln!(
            "harness k{k} x{circuits}: rows {} instructions {} physical registers {} (logical {}) constants {} columns {}",
            compiled.rows,
            compiled.instructions.len(),
            compiled.register_count,
            compiled.certificate.logical_register_count,
            compiled.constants.len(),
            compiled.columns.len()
        );
    }
}

#[test]
fn harness_k10_matches_stock_compute_nu_poly() {
    harness::check(10, 1);
}

#[test]
fn harness_k10_two_circuits_match_stock_compute_nu_poly() {
    harness::check(10, 2);
}

/// Timing/shape probe for the P5 whole-proof estimate. Run explicitly with
/// `LOKI_P5_DUMP=<file>` under heavy.lock; writes no witness-derived column
/// data (only program, frozen synthetic challenges and sizes).
#[test]
#[ignore = "timing probe: LOKI_P5_DUMP=<path> cargo test --release -- --ignored harness_timing_probe"]
fn harness_timing_probe() {
    // A blanket `--ignored` run (e.g. for k11-k14) skips the probe.
    let Some(path) = std::env::var_os("LOKI_P5_DUMP") else {
        eprintln!("harness_timing_probe: LOKI_P5_DUMP unset, skipping");
        return;
    };
    let k: u32 = std::env::var("LOKI_P5_K").ok().and_then(|v| v.parse().ok()).unwrap_or(19);
    harness::check_timed(k, 1, Some(std::path::Path::new(&path)));
}

#[test]
#[ignore = "k11-k14 differential: run with --release --ignored"]
fn harness_k11_to_k14_match_stock_compute_nu_poly() {
    for k in 11..=14 {
        harness::check(k, 1);
    }
    harness::check(14, 2);
}
