//! Pointwise differential coverage for the unchanged CPU numerator formula.
//! These tests exercise GPU column transforms, not a GPU numerator kernel.

use std::marker::PhantomData;

use ff::{Field, WithSmallOrderMulGroup};
use midnight_curves::{fft::best_fft, Fq};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

use super::Evaluator;
use crate::{
    plonk::{lookup, permutation, trash, ConstraintSystem, Constraints, Expression, FirstPhase},
    poly::{
        Coeff, EvaluationDomain, ExtendedLagrangeCoeff, Polynomial, PolynomialRepresentation,
        Rotation,
    },
};

// Deliberately never calls EvaluationDomain's optional CUDA FFT dispatcher.
#[derive(Clone, Copy, Debug)]
struct CpuExtended;

impl PolynomialRepresentation for CpuExtended {
    fn len<F: WithSmallOrderMulGroup<3>>(domain: &EvaluationDomain<F>) -> usize {
        domain.extended_len()
    }

    fn omega<F: WithSmallOrderMulGroup<3>>(domain: &EvaluationDomain<F>) -> F {
        domain.get_extended_omega()
    }

    fn k<F: WithSmallOrderMulGroup<3>>(domain: &EvaluationDomain<F>) -> u32 {
        domain.extended_k()
    }

    fn g_coset<F: WithSmallOrderMulGroup<3>>(_: &EvaluationDomain<F>) -> F {
        F::ZETA
    }

    fn coeff_to_self<F: WithSmallOrderMulGroup<3>>(
        domain: &EvaluationDomain<F>,
        poly: Polynomial<F, Coeff>,
    ) -> Polynomial<F, Self> {
        let mut values = poly.values;
        let mut power = F::ONE;
        for value in &mut values {
            *value *= power;
            power *= F::ZETA;
        }
        values.resize(domain.extended_len(), F::ZERO);
        best_fft(
            &mut values,
            domain.get_extended_omega(),
            domain.extended_k(),
        );
        Polynomial {
            values,
            _marker: PhantomData,
        }
    }
}

fn cpu_interpolate(domain: &EvaluationDomain<Fq>, mut values: Vec<Fq>) -> Polynomial<Fq, Coeff> {
    best_fft(&mut values, domain.get_omega_inv(), domain.k());
    let inverse_n = Fq::from(values.len() as u64).invert().unwrap();
    for value in &mut values {
        *value *= inverse_n;
    }
    domain.coeff_from_vec(values)
}

fn random_poly(domain: &EvaluationDomain<Fq>, rng: &mut ChaCha20Rng) -> Polynomial<Fq, Coeff> {
    domain.coeff_from_vec((0..domain.n).map(|_| Fq::random(&mut *rng)).collect())
}

struct Case {
    domain: EvaluationDomain<Fq>,
    cs: ConstraintSystem<Fq>,
    fixed: Vec<Polynomial<Fq, Coeff>>,
    advice: Vec<Vec<Polynomial<Fq, Coeff>>>,
    instance: Vec<Vec<Polynomial<Fq, Coeff>>>,
    permutation_columns: Vec<Polynomial<Fq, Coeff>>,
    permutations: Vec<permutation::prover::Committed<Fq>>,
    lookups: Vec<Vec<lookup::prover::Committed<Fq>>>,
    trashcans: Vec<Vec<trash::prover::Committed<Fq>>>,
    weights: [Polynomial<Fq, Coeff>; 3],
}

fn case(k: u32, shape: usize, rng: &mut ChaCha20Rng) -> Case {
    let n = 1usize << k;
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

    // Exactly three sets, with an uneven one-column final set. Include all
    // three column kinds in the permutation, not just advice columns.
    cs.enable_equality(fixed[0]);
    cs.enable_equality(instance[0]);
    for &column in advice.iter().take(2 * chunk - 1) {
        cs.enable_equality(column);
    }

    // Vary topology, column selection, constants, and rotations with a fixed
    // public seed. Large signed offsets force wraparound at both boundaries.
    let rotations = [0, 1, -1, n as i32 + 3, -(n as i32) - 2];
    for gate in 0..2 + shape {
        let a = advice[(rng.next_u32() as usize) % advice.len()];
        let b = advice[(rng.next_u32() as usize) % advice.len()];
        let f = fixed[gate % fixed.len()];
        let i = instance[gate % instance.len()];
        let offset = rotations[(gate + shape) % rotations.len()];
        let constant = Fq::random(&mut *rng);
        cs.create_gate("argument commit GPU synthetic gate", |cells| {
            let a = cells.query_advice(a, Rotation(offset));
            let b = cells.query_advice(b, Rotation(rotations[(gate + 2) % rotations.len()]));
            let f = cells.query_fixed(f, Rotation(-1));
            let i = cells.query_instance(i, Rotation(n as i32 + 1));
            let c = cells.query_challenge(challenge);
            let expression = if gate % 2 == 0 {
                a.clone() * b + f - i + c * Expression::Constant(constant)
            } else {
                -a.clone() + b.clone() * b + f * i + c
            };
            Constraints::with_selector(
                gate_selector,
                vec![expression, a.clone() * a - Expression::Constant(constant)],
            )
        });
    }

    for lookup in 0..1 + shape {
        cs.lookup_any("argument commit GPU synthetic lookup", |cells| {
            (0..1 + (lookup + shape) % 3)
                .map(|column| {
                    let a = cells.query_advice(advice[column], Rotation(rotations[column]));
                    let i = cells.query_instance(instance[0], Rotation(-1));
                    let f = cells.query_fixed(fixed[column % fixed.len()], Rotation(1));
                    let c = cells.query_challenge(challenge);
                    (
                        a + i + c,
                        f + Expression::Constant(Fq::from((column + 1) as u64)),
                    )
                })
                .collect()
        });
    }

    for trash in 0..1 + shape % 2 {
        cs.create_gate("argument commit GPU synthetic trash", |cells| {
            let a = cells.query_advice(advice[trash], Rotation(-(n as i32) - 1));
            let b = cells.query_advice(advice[trash + 1], Rotation(2));
            let f = cells.query_fixed(fixed[0], Rotation::cur());
            let i = cells.query_instance(instance[0], Rotation::cur());
            let c = cells.query_challenge(challenge);
            Constraints::with_additive_selector(trash_selector, vec![a * b + c, f - i])
        });
    }

    let selector_values = (0..cs.num_selectors())
        .map(|selector| {
            (0..n)
                .map(|row| (row + selector) % 5 != 0 && rng.next_u32() % 3 != 0)
                .collect()
        })
        .collect();
    let (cs, selectors) = cs.directly_convert_selectors_to_fixed(selector_values);
    assert_eq!(
        cs.degree(),
        degree,
        "shape {shape}: unexpected synthetic degree"
    );
    assert_eq!(cs.permutation.columns.len(), 2 * chunk + 1);
    let domain = EvaluationDomain::new(cs.degree() as u32, k);
    assert!(domain.extended_len() >= 1024);
    let mut fixed_coeffs: Vec<_> = fixed.iter().map(|_| random_poly(&domain, rng)).collect();
    fixed_coeffs.extend(selectors.into_iter().map(|v| cpu_interpolate(&domain, v)));
    assert_eq!(fixed_coeffs.len(), cs.num_fixed_columns());

    // Correct Lagrange weights: first row, first inactive row, active row mask.
    let last = n - cs.blinding_factors() - 1;
    let weights = [0, 1, 2].map(|which| {
        let values = (0..n)
            .map(|row| {
                let enabled = match which {
                    0 => row == 0,
                    1 => row == last,
                    _ => row < last,
                };
                if enabled {
                    Fq::ONE
                } else {
                    Fq::ZERO
                }
            })
            .collect();
        cpu_interpolate(&domain, values)
    });
    let instances = 2 + shape % 2;
    // Random, deliberately unconstrained polynomials are valid inputs for raw
    // numerator equality; this is not a proof-generation/satisfaction test.
    Case {
        fixed: fixed_coeffs,
        advice: (0..instances)
            .map(|_| advice.iter().map(|_| random_poly(&domain, rng)).collect())
            .collect(),
        instance: (0..instances)
            .map(|_| instance.iter().map(|_| random_poly(&domain, rng)).collect())
            .collect(),
        permutation_columns: cs
            .permutation
            .columns
            .iter()
            .map(|_| random_poly(&domain, rng))
            .collect(),
        permutations: (0..instances)
            .map(|_| permutation::prover::Committed {
                sets: cs
                    .permutation
                    .columns
                    .chunks(chunk)
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

fn numerator<B: PolynomialRepresentation>(case: &Case, challenges: [Fq; 6]) -> Polynomial<Fq, B> {
    let convert = |polys: &[Polynomial<Fq, Coeff>]| -> Vec<Polynomial<Fq, B>> {
        polys
            .iter()
            .map(|poly| B::coeff_to_self(&case.domain, poly.clone()))
            .collect()
    };
    let fixed = convert(&case.fixed);
    let advice: Vec<_> = case.advice.iter().map(|v| convert(v)).collect();
    let instance: Vec<_> = case.instance.iter().map(|v| convert(v)).collect();
    let permutation_columns = convert(&case.permutation_columns);
    let weights = convert(&case.weights);
    let advice_refs: Vec<_> = advice.iter().map(Vec::as_slice).collect();
    let instance_refs: Vec<_> = instance.iter().map(Vec::as_slice).collect();
    let [y, beta, gamma, theta, trash_challenge, circuit_challenge] = challenges;
    Evaluator::new(&case.cs).evaluate_numerator(
        &case.domain,
        &case.cs,
        &advice_refs,
        &instance_refs,
        &fixed,
        &[circuit_challenge],
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
        &permutation_columns,
    )
}

#[test]
#[ignore = "requires CUDA NTT; parent runs through GPU runner with --ignored --test-threads=1"]
fn argument_commit_gpu_randomized_per_coset() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x6172_6775_6d65_6e74);
    for (shape, k) in [8, 9, 10].into_iter().enumerate() {
        let case = case(k, shape, &mut rng);
        let expected_transforms = case.fixed.len()
            + case.advice.iter().map(Vec::len).sum::<usize>()
            + case.instance.iter().map(Vec::len).sum::<usize>()
            + case.permutation_columns.len()
            + case.weights.len()
            + case
                .permutations
                .iter()
                .map(|p| p.sets.len())
                .sum::<usize>()
            + 3 * case.lookups.iter().map(Vec::len).sum::<usize>()
            + case.trashcans.iter().map(Vec::len).sum::<usize>();
        let random = std::array::from_fn(|_| Fq::random(&mut rng));
        let mixed = [Fq::ONE, Fq::ZERO, -Fq::ONE, random[3], Fq::ONE, random[5]];
        for (challenge_case, challenges) in
            [[Fq::ZERO; 6], [Fq::ONE; 6], [-Fq::ONE; 6], random, mixed]
                .into_iter()
                .enumerate()
        {
            let cpu_calls = loki_cuda_ntt::completed_calls();
            let expected = numerator::<CpuExtended>(&case, challenges);
            assert_eq!(
                loki_cuda_ntt::completed_calls(),
                cpu_calls,
                "shape {shape}: CPU oracle dispatched a GPU FFT; run this test alone"
            );
            let before = loki_cuda_ntt::completed_calls();
            let actual = numerator::<ExtendedLagrangeCoeff>(&case, challenges);
            assert_eq!(
                loki_cuda_ntt::completed_calls() - before,
                expected_transforms as u64,
                "shape {shape} challenge {challenge_case}: incomplete GPU FFT dispatch; run alone"
            );
            assert_eq!(
                actual.len(),
                expected.len(),
                "shape {shape}: numerator lengths"
            );
            let mismatch = actual.iter().zip(expected.iter()).position(|(a, b)| a != b);
            if let Some(index) = mismatch {
                panic!("shape {shape} k {k} challenge {challenge_case}: numerator mismatch at index {index} of {}", actual.len());
            }
        }
    }
}

// A small CPU-only check for the oracle itself: compare explicit FFT output
// with independent Horner evaluations, and interpolation with known weights.
#[test]
fn argument_commit_cpu_oracle_support() {
    let domain = EvaluationDomain::<Fq>::new(4, 3);
    let coefficients: Vec<_> = (0..domain.n).map(|i| Fq::from(i + 1)).collect();
    let result = CpuExtended::coeff_to_self(&domain, domain.coeff_from_vec(coefficients.clone()));
    let mut point = Fq::ZETA;
    for (index, got) in result.iter().enumerate() {
        let want = coefficients
            .iter()
            .rev()
            .fold(Fq::ZERO, |acc, c| acc * point + c);
        assert!(got == &want, "CPU coset oracle mismatch at index {index}");
        point *= domain.get_extended_omega();
    }
    let mut delta = vec![Fq::ZERO; domain.n as usize];
    delta[0] = Fq::ONE;
    let l0 = cpu_interpolate(&domain, delta);
    let inverse_n = Fq::from(domain.n).invert().unwrap();
    assert!(
        l0.iter().all(|c| *c == inverse_n),
        "CPU interpolation oracle mismatch"
    );
}

#[test]
fn argument_commit_cpu_fixture_support() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x6370_755f_6361_7365);
    let before = loki_cuda_ntt::completed_calls();
    for shape in 0..3 {
        let case = case(8, shape, &mut rng);
        let challenges = std::array::from_fn(|_| Fq::random(&mut rng));
        let values = numerator::<CpuExtended>(&case, challenges);
        assert_eq!(
            values.len(),
            case.domain.extended_len(),
            "shape {shape}: CPU fixture length"
        );
        assert!(
            values.iter().any(|value| *value != Fq::ZERO),
            "shape {shape}: synthetic numerator unexpectedly zero"
        );
    }
    assert_eq!(
        loki_cuda_ntt::completed_calls(),
        before,
        "CPU fixture executed a GPU FFT; run alone"
    );
}
