use super::*;
use crate::{poly::commitment::PolynomialCommitmentScheme, transcript::CircuitTranscript};
use blake2b_simd::State;
use midnight_curves::{Bls12, Fq};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

#[test]
fn product_division_matches_successive_kate_division() {
    let mut rng = ChaCha20Rng::from_seed([83; 32]);
    let r = Fq::random(&mut rng);
    let i = Fq::ROOT_OF_UNITY.pow_vartime([1u64 << (Fq::S - 2)]);
    let shapes = [
        vec![],
        vec![Fq::ZERO],
        vec![Fq::from(7)],
        vec![Fq::ZERO, Fq::ZERO, Fq::ZERO],
        vec![r, r, r],
        vec![r, r * i, r * i.square(), r * i.square() * i],
        (0..5).map(|_| Fq::random(&mut rng)).collect(),
    ];
    for n in [0, 1, 2, 3, 4, 5, 6, 64, 4097] {
        for zero in [false, true] {
            let mut poly: Vec<_> = (0..n)
                .map(|_| if zero { Fq::ZERO } else { Fq::random(&mut rng) })
                .collect();
            // Preserve vector length even when the polynomial degree is lower.
            for trailing_zeros in [false, true] {
                if trailing_zeros {
                    for coefficient in poly.iter_mut().rev().take(3) {
                        *coefficient = Fq::ZERO;
                    }
                }
                for roots in &shapes {
                    // The stock kernel underflows when dividing an empty vector.
                    if roots.len() <= n {
                        let expected = roots
                            .iter()
                            .fold(poly.clone(), |p, x| kate_division(&p, *x));
                        assert_eq!(divide_by_roots(&poly, roots), expected);
                    } else {
                        assert!(divide_by_roots(&poly, roots).is_empty());
                    }
                }
            }
        }
    }
    // Nonzero remainder: (X^2 + 1) / ((X - 1)(X - 2)) has quotient 1.
    assert_eq!(
        divide_by_roots(&[Fq::ONE, Fq::ZERO, Fq::ONE], &[Fq::ONE, Fq::from(2)]),
        vec![Fq::ONE]
    );
}

#[test]
fn prover_grouping_skips_only_unused_evaluations() {
    let mut rng = ChaCha20Rng::from_seed([97; 32]);
    let polys: Vec<_> = (0..3)
        .map(|_| Polynomial {
            values: (0..16).map(|_| Fq::random(&mut rng)).collect(),
            _marker: PhantomData,
        })
        .collect();
    let queries = [
        ProverQuery::new(Fq::from(3), &polys[0]),
        ProverQuery::new(Fq::from(7), &polys[0]),
        ProverQuery::new(Fq::from(7), &polys[1]),
        ProverQuery::new(Fq::from(3), &polys[2]),
        ProverQuery::new(Fq::from(7), &polys[2]),
    ];
    let (expected, points) = construct_intermediate_sets(&queries).unwrap();
    let (actual, actual_points) = super::super::utils::construct_prover_sets(&queries).unwrap();
    assert_eq!(actual_points, points);
    assert!(expected
        .iter()
        .flat_map(|data| &data.evals)
        .any(|e| *e != Fq::ZERO));
    for (a, b) in actual.iter().zip(&expected) {
        assert_eq!(a.commitment, b.commitment);
        assert_eq!(a.commitment_label, b.commitment_label);
        assert_eq!(a.set_index, b.set_index);
        assert_eq!(a.point_indices, b.point_indices);
        assert_eq!(a.evals, vec![Fq::ZERO; b.evals.len()]);
    }
    assert_eq!(actual.len(), expected.len());
    let duplicates = [queries[0], queries[0]];
    assert!(matches!(
        super::super::utils::construct_prover_sets(&duplicates),
        Err(Error::DuplicatedQuery)
    ));
}

// Independent copy of the pre-feature polynomial path, including its clones.
fn serial(
    queries: &[ProverQuery<Fq>],
    x1: Fq,
    x2: Fq,
    k: u32,
) -> Result<(Vec<Polynomial<Fq, Coeff>>, Polynomial<Fq, Coeff>), Error> {
    let (map, sets) = construct_intermediate_sets(queries)?;
    let mut groups = vec![vec![]; sets.len()];
    for data in map {
        groups[data.set_index].push(data.commitment.poly.clone());
    }
    let q: Vec<_> = groups
        .iter()
        .map(|polys| {
            #[cfg(feature = "truncated-challenges")]
            let scalars = truncated_powers(x1);
            #[cfg(not(feature = "truncated-challenges"))]
            let scalars = powers(x1);
            inner_product(polys, scalars)
        })
        .collect();
    let mut order: Vec<_> = (0..sets.len()).collect();
    order.sort_by_key(|&i| (sets[i].len(), i));
    let q: Vec<_> = order.iter().map(|&i| q[i].clone()).collect();
    let f: Vec<_> = order
        .iter()
        .zip(&q)
        .map(|(&i, poly)| {
            let mut values = sets[i]
                .iter()
                .fold(poly.values.clone(), |p, x| kate_division(&p, *x));
            values.resize(1 << k, Fq::ZERO);
            Polynomial {
                values,
                _marker: PhantomData,
            }
        })
        .collect();
    Ok((q, inner_product(&f, powers(x2))))
}

fn serial_proof(params: &ParamsKZG<Bls12>, queries: &[ProverQuery<Fq>]) -> Vec<u8> {
    let mut t = CircuitTranscript::<State>::init();
    let x1 = t.squeeze_challenge();
    let x2 = t.squeeze_challenge();
    let (mut q, f) = serial(queries, x1, x2, params.max_k()).unwrap();
    t.write(&KZGCommitmentScheme::<Bls12>::commit(params, &f))
        .unwrap();
    let x3: Fq = t.squeeze_challenge();
    #[cfg(feature = "truncated-challenges")]
    let x3 = truncate(x3);
    for p in &q {
        t.write(&eval_polynomial(p, x3)).unwrap();
    }
    let x4 = t.squeeze_challenge();
    q.push(f);
    #[cfg(feature = "truncated-challenges")]
    let scalars = truncated_powers(x4);
    #[cfg(not(feature = "truncated-challenges"))]
    let scalars = powers(x4);
    let p = inner_product(&q, scalars);
    let v = eval_polynomial(&p, x3);
    let pi = Polynomial {
        values: kate_division(&(&p - v).values, x3),
        _marker: PhantomData,
    };
    t.write(&KZGCommitmentScheme::<Bls12>::commit(params, &pi))
        .unwrap();
    t.finalize()
}

#[test]
fn differential_random_sets_and_transcripts() {
    let mut rng = ChaCha20Rng::from_seed([71; 32]);
    // Single set, repeated points across polys, equal-cardinality ties, nested
    // sets, zero polynomials, and sizes on both sides of the parallel threshold.
    for k in [3, 12] {
        let params = ParamsKZG::<Bls12>::unsafe_setup(k, &mut rng);
        for shape in [
            vec![vec![0]],
            vec![vec![0], vec![0], vec![0]],
            vec![vec![1, 2], vec![0], vec![2, 1], vec![3]],
            vec![vec![0, 1, 2], vec![0], vec![1, 2], vec![0, 1]],
        ] {
            let polys: Vec<_> = (0..shape.len())
                .map(|i| Polynomial {
                    values: (0..1 << k)
                        .map(|_| {
                            if i == 0 {
                                Fq::ZERO
                            } else {
                                Fq::random(&mut rng)
                            }
                        })
                        .collect(),
                    _marker: PhantomData,
                })
                .collect();
            let queries: Vec<_> = shape
                .iter()
                .zip(&polys)
                .flat_map(|(points, poly)| {
                    points.iter().map(move |&point| ProverQuery {
                        point: Fq::from(point),
                        poly,
                    })
                })
                .collect();
            for threads in [1, 4, 20] {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build()
                    .unwrap()
                    .install(|| {
                        let x1 = Fq::from(17);
                        let x2 = Fq::from(29);
                        let (q, f) = prepare(&queries, x1, x2, k).unwrap();
                        let (expected_q, expected_f) = serial(&queries, x1, x2, k).unwrap();
                        assert_eq!(q.len(), expected_q.len());
                        for (a, b) in q.iter().zip(&expected_q) {
                            assert_eq!(a.values, b.values);
                        }
                        assert_eq!(f.values, expected_f.values);
                        let mut t = CircuitTranscript::<State>::init();
                        KZGCommitmentScheme::<Bls12>::multi_open(&params, &queries, &mut t)
                            .unwrap();
                        assert_eq!(t.finalize(), serial_proof(&params, &queries));
                    });
            }
        }
        let zero = Polynomial {
            values: vec![Fq::ZERO; 1 << k],
            _marker: PhantomData,
        };
        let repeated = [ProverQuery {
            point: Fq::ONE,
            poly: &zero,
        }; 2];
        assert!(matches!(
            prepare(&repeated, Fq::ONE, Fq::ONE, k),
            Err(Error::DuplicatedQuery)
        ));
        assert!(matches!(
            serial(&repeated, Fq::ONE, Fq::ONE, k),
            Err(Error::DuplicatedQuery)
        ));
    }
}

#[test]
fn differential_coefficient_chunks() {
    let mut rng = ChaCha20Rng::from_seed([19; 32]);
    for n in [1, 1023, 4095, 4096, 4097, 8193] {
        let polys: Vec<_> = (0..5)
            .map(|_| Polynomial {
                values: (0..n).map(|_| Fq::random(&mut rng)).collect(),
                _marker: PhantomData,
            })
            .collect();
        let refs: Vec<_> = polys.iter().collect();
        for x in [Fq::ZERO, Fq::ONE, Fq::from(43)] {
            let expected = inner_product(&polys, powers(x));
            assert_eq!(combine(&refs, powers(x)).values, expected.values);
            #[cfg(feature = "truncated-challenges")]
            assert_eq!(
                combine(&refs, truncated_powers(x)).values,
                inner_product(&polys, truncated_powers(x)).values
            );
        }
    }
}

#[test]
fn padded_rhs_keeps_first_polynomial_length() {
    let polys: Vec<_> = [4097, 4097, 8192]
        .into_iter()
        .map(|n| Polynomial {
            values: vec![Fq::ONE; n],
            _marker: PhantomData,
        })
        .collect();
    let refs: Vec<_> = polys.iter().collect();
    let expected = inner_product(&polys, powers(Fq::from(7)));
    let actual = combine(&refs, powers(Fq::from(7)));
    assert_eq!(actual.values.len(), 4097);
    assert_eq!(actual.values, expected.values);
}

#[test]
fn short_polynomial_with_larger_srs_preserves_proof_bytes() {
    let mut rng = ChaCha20Rng::from_seed([37; 32]);
    let params = ParamsKZG::<Bls12>::unsafe_setup(4, &mut rng);
    let poly = Polynomial {
        values: (0..8).map(|_| Fq::random(&mut rng)).collect(),
        _marker: PhantomData,
    };
    let queries = [ProverQuery::new(Fq::from(3), &poly)];
    let mut t = CircuitTranscript::<State>::init();
    KZGCommitmentScheme::<Bls12>::multi_open(&params, &queries, &mut t).unwrap();
    assert_eq!(t.finalize(), serial_proof(&params, &queries));
}
