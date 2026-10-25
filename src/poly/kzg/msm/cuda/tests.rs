use ff::Field;
use group::Group;
use midnight_curves::{Fq, G1Affine, G1Projective};
use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

use super::backend;

fn cpu(scalars: &[Fq], bases: &[G1Projective]) -> G1Projective {
    // Independent stock dispatch, including zero filtering.
    super::super::msm_cpu::<G1Affine>(scalars, bases)
}

#[test]
#[ignore = "requires a GPU; run through gpu-runner with --ignored"]
fn gpu_differential() {
    let mut rng = ChaCha20Rng::from_seed([91; 32]);
    let pool: Vec<_> = (0..1025).map(|_| G1Projective::random(&mut rng)).collect();
    let mut cases = 0;
    for case in 0..10_000 {
        let n = if case < 4 {
            case
        } else {
            1 + (case * 73 % 1025)
        };
        let mut bases = pool[..n].to_vec();
        let scalars: Vec<_> = (0..n)
            .map(|i| match (case + i) % 11 {
                0 => Fq::ZERO,
                1 => Fq::ONE,
                2 => -Fq::ONE,
                _ => Fq::random(&mut rng),
            })
            .collect();
        if n > 0 && case % 3 == 0 {
            bases[n / 2] = G1Projective::identity();
        }
        if case % 31 == 0 {
            bases.fill(G1Projective::identity());
        }
        // Call the backend directly: fallback must never hide a GPU error.
        assert_eq!(
            backend::msm(&scalars, &bases).unwrap(),
            cpu(&scalars, &bases),
            "random case {case}"
        );
        cases += 1;
    }
    for k in 1..=16 {
        for n in [(1 << k) - 1, 1 << k, (1 << k) + 1] {
            let bases: Vec<_> = (0..n).map(|i| pool[i % pool.len()]).collect();
            let scalars: Vec<_> = (0..n).map(|_| Fq::random(&mut rng)).collect();
            assert_eq!(
                backend::msm(&scalars, &bases).unwrap(),
                cpu(&scalars, &bases),
                "size {n}"
            );
            // Hit resident bases with different scalars, including all zeros.
            let zeros = vec![Fq::ZERO; n];
            assert_eq!(
                backend::msm(&zeros, &bases).unwrap(),
                G1Projective::identity()
            );
            cases += 2;
        }
    }
    // Same address/length, changed contents: must upload new bases.
    let mut bases = vec![G1Projective::generator(); 17];
    let scalars = vec![-Fq::ONE; 17];
    let before = backend::msm(&scalars, &bases).unwrap();
    assert_eq!(before, cpu(&scalars, &bases));
    bases[8] = G1Projective::identity();
    let after = backend::msm(&scalars, &bases).unwrap();
    assert_ne!(before, after);
    assert_eq!(after, cpu(&scalars, &bases));
    // Preserve stock zip semantics for mismatched lengths.
    assert_eq!(
        backend::msm(&scalars[..2], &bases).unwrap(),
        cpu(&scalars[..2], &bases)
    );
    let cancellation = [G1Projective::generator(), -G1Projective::generator()];
    assert_eq!(
        backend::msm(&[Fq::ONE; 2], &cancellation).unwrap(),
        G1Projective::identity()
    );
    // Different host threads must select the device and serialize shared streams.
    std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..8)
            .map(|i| {
                scope.spawn(move || {
                    let bases = vec![G1Projective::generator(); 257];
                    let scalars = vec![Fq::from(i + 1); 257];
                    assert_eq!(
                        backend::msm(&scalars, &bases).unwrap(),
                        cpu(&scalars, &bases)
                    );
                })
            })
            .collect();
        for job in jobs {
            job.join().unwrap();
        }
    });
    eprintln!("GPU differential: {} cases passed", cases + 12);
}

#[test]
#[ignore = "requires a GPU; run through gpu-runner with --ignored --nocapture"]
fn gpu_benchmark() {
    use std::time::Instant;
    let mut rng = ChaCha20Rng::from_seed([17; 32]);
    eprintln!("k,n,cpu_ms,cold_ms,warm_ms");
    for k in 16..=22 {
        let n = 1 << k;
        // Distinct subgroup points, generated outside the timed region.
        let mut point = G1Projective::random(&mut rng);
        let step = G1Projective::random(&mut rng);
        let bases: Vec<_> = (0..n)
            .map(|_| {
                let base = point;
                point += step;
                base
            })
            .collect();
        let scalars: Vec<_> = (0..n).map(|_| Fq::random(&mut rng)).collect();
        let start = Instant::now();
        let expected = cpu(&scalars, &bases);
        let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
        backend::clear_cache().unwrap();
        let start = Instant::now();
        assert_eq!(backend::msm(&scalars, &bases).unwrap(), expected);
        let cold_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        for _ in 0..3 {
            assert_eq!(backend::msm(&scalars, &bases).unwrap(), expected);
        }
        let warm_ms = start.elapsed().as_secs_f64() * 1000.0 / 3.0;
        eprintln!("{k},{n},{cpu_ms:.3},{cold_ms:.3},{warm_ms:.3}");
    }
}

#[test]
fn cuda_dispatch_threshold_and_error_fallback() {
    use super::{dispatch, parse_min};
    assert_eq!(parse_min(None), 1 << 14);
    assert_eq!(parse_min(Some("bad")), 1 << 14);
    assert_eq!(parse_min(Some("-1")), 1 << 14);
    assert_eq!(parse_min(Some("0")), 0);
    assert_eq!(parse_min(Some("123")), 123);
    let scalars = vec![Fq::ONE; 2];
    let bases = vec![G1Projective::generator(); 2];
    assert!(dispatch::<G1Affine>(&scalars, &bases, 3, |_, _| panic!("below threshold")).is_none());
    assert!(dispatch::<midnight_curves::G2Affine>(
        &scalars,
        &[midnight_curves::G2Projective::generator(); 2],
        0,
        |_, _| panic!("unsupported group must stay on CPU"),
    )
    .is_none());
    let expected = cpu(&scalars, &bases);
    assert_eq!(
        dispatch::<G1Affine>(&scalars, &bases, 2, |_, _| Ok(expected)),
        Some(expected)
    );
    let result = dispatch::<G1Affine>(&scalars, &bases, 2, |_, _| Err("injected GPU error".into()))
        .unwrap_or_else(|| cpu(&scalars, &bases));
    assert_eq!(result, expected);
    let zeros = vec![Fq::ZERO; 2];
    let result = dispatch::<G1Affine>(&zeros, &bases, 2, |_, _| Err("injected GPU error".into()))
        .unwrap_or_else(|| cpu(&zeros, &bases));
    assert_eq!(result, G1Projective::identity());
}

mod proof_fixture {
    use crate as proofs;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/gpu-compat/circuit.rs"
    ));

    #[cfg(feature = "cuda-ntt")]
    pub fn concurrent_setup() -> (ParamsKZG<Bls12>, plonk::ProvingKey<Fq, Scheme>) {
        let params = ParamsKZG::<Bls12>::unsafe_setup(14, OsRng);
        let vk = plonk::keygen_vk_with_k::<Fq, Scheme, _>(&params, &SquareCircuit, 14).unwrap();
        let pk = plonk::keygen_pk(vk, &SquareCircuit).unwrap();
        (params, pk)
    }

    #[cfg(feature = "cuda-ntt")]
    pub fn concurrent_proof(params: &ParamsKZG<Bls12>, pk: &plonk::ProvingKey<Fq, Scheme>) {
        let values = [Fq::from(49)];
        let instances: &[&[Fq]] = &[&values];
        let mut transcript = CircuitTranscript::<State>::init();
        plonk::create_proof::<Fq, Scheme, _, _>(
            params,
            pk,
            &[SquareCircuit],
            #[cfg(feature = "committed-instances")]
            0,
            &[instances],
            &mut transcript,
            OsRng,
        )
        .unwrap();
        let proof = transcript.finalize();
        let mut transcript = CircuitTranscript::<State>::init_from_bytes(&proof);
        plonk::prepare::<Fq, Scheme, _>(
            pk.get_vk(),
            #[cfg(feature = "committed-instances")]
            &[&[]],
            &[instances],
            &mut transcript,
        )
        .unwrap()
        .verify(&params.verifier_params())
        .unwrap();
    }
}

#[test]
#[ignore = "requires GPU and LOKI_GPU_MSM_MIN=1; run alone through gpu-runner"]
fn gpu_proof() {
    // This test runs in its own process, with no concurrent environment readers.
    std::env::set_var("LOKI_GPU_MSM_MIN", "1");
    let before = backend::gpu_calls();
    proof_fixture::produce(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/gpu-msm-proof"),
    );
    assert!(backend::gpu_calls() > before, "proof must execute GPU MSMs");
    assert!(!backend::failed(), "proof must not silently fall back");
}

#[test]
#[ignore = "requires a GPU-less environment; run alone in the local sandbox"]
fn cuda_no_device_fallback() {
    #[cfg(feature = "loki-profile")]
    let _serial = crate::profile::TEST_REPORTS.lock().unwrap();
    #[cfg(feature = "loki-profile")]
    let proof = crate::profile::ProofGuard::start(997, 1);
    std::env::set_var("LOKI_GPU_MSM_MIN", "1");
    let bases = vec![G1Projective::generator(); 17];
    let scalars: Vec<_> = (0..17)
        .map(|i| match i % 3 {
            0 => Fq::ZERO,
            1 => Fq::ONE,
            _ => -Fq::ONE,
        })
        .collect();
    let expected = cpu(&scalars, &bases);
    assert_eq!(
        super::super::msm_specific::<G1Affine>(&scalars, &bases),
        expected
    );
    assert!(backend::failed(), "test requires no accessible GPU");
    assert_eq!(backend::gpu_calls(), 0);
    // The failed backend stays disabled, and later public calls still work.
    assert_eq!(
        super::super::msm_specific::<G1Affine>(&scalars, &bases),
        expected
    );
    #[cfg(feature = "loki-profile")]
    {
        drop(proof);
        let report = crate::profile::take_report()
            .proofs
            .into_iter()
            .find(|p| p.k == 997)
            .unwrap();
        assert_eq!(report.msm.calls, 2);
        let backends = &report.phases["other"].backends;
        assert_eq!(backends.len(), 1);
        assert_eq!(backends["msm.cpu_fallback"].sizes[&17], 2);
    }
}

#[test]
#[ignore = "requires a GPU; run through gpu-runner with --ignored"]
fn gpu_large_boundaries() {
    let mut rng = ChaCha20Rng::from_seed([63; 32]);
    let pool: Vec<_> = (0..1024).map(|_| G1Projective::random(&mut rng)).collect();
    // These sizes exercise sppark's multi-batch path with a partial final warp
    // and a partial final batch, beyond the smaller randomized matrix.
    for k in 17..=22 {
        for n in [(1 << k) - 1, (1 << k) + 1] {
            let mut bases: Vec<_> = (0..n).map(|i| pool[i % pool.len()]).collect();
            bases[n / 2] = G1Projective::identity();
            let mut scalars: Vec<_> = (0..n).map(|_| Fq::random(&mut rng)).collect();
            scalars[0] = Fq::ZERO;
            scalars[n - 1] = -Fq::ONE;
            assert_eq!(
                backend::msm(&scalars, &bases).unwrap(),
                cpu(&scalars, &bases),
                "size {n}"
            );
            eprintln!("GPU large boundary {n}: passed");
        }
    }
}

#[cfg(feature = "loki-profile")]
#[test]
fn profile_attributes_dispatch_outcomes() {
    let _serial = crate::profile::TEST_REPORTS.lock().unwrap();
    let proof = crate::profile::ProofGuard::start(999, 1);
    let phase = crate::profile::PhaseGuard::start("dispatch_test");
    let scalars = [Fq::ONE; 2];
    let bases = [G1Projective::generator(); 2];
    assert!(super::dispatch::<G1Affine>(&scalars, &bases, 3, |_, _| unreachable!()).is_none());
    assert!(super::dispatch::<midnight_curves::G2Affine>(
        &scalars,
        &[midnight_curves::G2Projective::generator(); 2],
        0,
        |_, _| unreachable!()
    )
    .is_none());
    assert!(
        super::dispatch::<G1Affine>(&scalars, &bases, 0, |_, _| Err("test failure".into()))
            .is_none()
    );
    assert!(super::dispatch::<G1Affine>(&scalars, &bases, 0, |_, _| Ok(bases[0])).is_some());
    assert!(
        super::dispatch::<G1Affine>(&[], &[], 0, |_, _| Ok(G1Projective::identity())).is_some()
    );
    drop(phase);
    drop(proof);
    let report = crate::profile::take_report()
        .proofs
        .into_iter()
        .find(|p| p.k == 999)
        .unwrap();
    let backends = &report.phases["dispatch_test"].backends;
    assert_eq!(backends.len(), 5);
    for name in [
        "msm.cpu_threshold",
        "msm.cpu_type",
        "msm.cpu_fallback",
        "msm.gpu",
    ] {
        assert_eq!(backends[name].sizes[&2], 1, "{name}");
    }
    assert_eq!(backends["msm.cpu_empty"].sizes[&0], 1);
}

#[test]
#[ignore = "requires GPU; run alone through gpu-runner"]
fn gpu_structured_scalars() {
    use std::time::Instant;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        eprintln!(
            "structured MSM rayon_threads={} blst_threads={} loki_cpu_budget={}",
            rayon::current_num_threads(),
            if cfg!(feature = "loki-cpu-budget") {
                "1"
            } else {
                "unknown"
            },
            cfg!(feature = "loki-cpu-budget")
        );
        eprintln!("k,pattern,repetition,cpu_ms,gpu_ms");
        let mut rng = ChaCha20Rng::from_seed([109; 32]);
        for k in [16, 18] {
            let n = 1 << k;
            let mut point = G1Projective::random(&mut rng);
            let step = G1Projective::random(&mut rng);
            let bases: Vec<_> = (0..n)
                .map(|_| {
                    let p = point;
                    point += step;
                    p
                })
                .collect();
            let constant = Fq::random(&mut rng);
            let patterns = [
                (
                    "random",
                    (0..n).map(|_| Fq::random(&mut rng)).collect::<Vec<_>>(),
                ),
                (
                    "small_repeat",
                    (0..n).map(|i| Fq::from((i % 16) as u64)).collect(),
                ),
                ("wide_repeat", vec![constant; n]),
                ("one", vec![Fq::ONE; n]),
                (
                    "zero_one",
                    (0..n).map(|i| Fq::from((i % 2) as u64)).collect(),
                ),
            ];
            let expected: Vec<_> = patterns
                .iter()
                .map(|(_, scalars)| {
                    let started = Instant::now();
                    let result = cpu(scalars, &bases);
                    (result, started.elapsed().as_secs_f64() * 1000.0)
                })
                .collect();
            backend::clear_cache().unwrap();
            assert_eq!(backend::msm(&patterns[0].1, &bases).unwrap(), expected[0].0);
            let calls = backend::gpu_calls();
            for repetition in 0..3 {
                for offset in 0..patterns.len() {
                    let index = (offset + repetition) % patterns.len();
                    let (name, scalars) = &patterns[index];
                    let started = Instant::now();
                    let result = backend::msm(scalars, &bases).unwrap();
                    let gpu_ms = started.elapsed().as_secs_f64() * 1000.0;
                    assert_eq!(result, expected[index].0, "synthetic {name} at k{k}");
                    eprintln!(
                        "{k},{name},{repetition},{:.3},{gpu_ms:.3}",
                        expected[index].1
                    );
                }
            }
            assert_eq!(backend::gpu_calls() - calls, (3 * patterns.len()) as usize);
            assert!(!backend::failed());
        }
    });
}

/// Bounded synthetic comparison of the two bases used by argument commitments.
/// This does not measure an A3 proof or establish a proof-level speedup.
#[cfg(all(feature = "cuda-argument-commit", feature = "cuda-ntt"))]
#[test]
#[ignore = "requires CUDA; parent runs through GPU runner with --ignored --nocapture --test-threads=1"]
fn gpu_argument_basis_small_synthetic() {
    use midnight_curves::{fft::best_fft, Bls12};
    use std::time::Instant;

    use crate::poly::{kzg::params::ParamsKZG, EvaluationDomain};

    // Independent, unfiltered CPU group sum. Neither the GPU dispatcher nor
    // the production CPU MSM implementation computes the expected commitment.
    let oracle = |scalars: &[Fq], bases: &[G1Projective]| {
        scalars
            .iter()
            .zip(bases)
            .fold(G1Projective::identity(), |sum, (scalar, base)| {
                sum + *base * scalar
            })
    };
    let mut rng = ChaCha20Rng::from_seed([0xBA; 32]);
    eprintln!("gpu_argument_basis: public synthetic sizes; milliseconds; not an A3 speed claim");
    eprintln!("k,pattern,conversion_ms,cpu_oracle_ms,lagrange_cold_ms,lagrange_warm_ms,coeff_cold_ms,coeff_warm_ms");
    for k in [8, 10] {
        let domain = EvaluationDomain::<Fq>::new(4, k);
        let params = ParamsKZG::<Bls12>::unsafe_setup(k, &mut rng);
        let n = 1usize << k;
        assert_eq!(params.g.len(), n);
        assert_eq!(params.g_lagrange.len(), n);
        let repeated: Vec<_> = (0..16).map(|_| Fq::random(&mut rng)).collect();
        let edges = [
            Fq::ZERO,
            Fq::ONE,
            -Fq::ONE,
            Fq::from(1u64 << 63),
            Fq::from_raw([u64::MAX; 4]),
        ];
        for pattern in 0..7 {
            let lagrange: Vec<_> = (0..n)
                .map(|i| match pattern {
                    0 => Fq::ZERO, // Identity commitment.
                    1 => Fq::ONE,
                    2 => -Fq::ONE,
                    3 => Fq::from((i % 16) as u64), // Repeated low values.
                    4 => repeated[i % repeated.len()], // Repeated full-width values.
                    5 => Fq::random(&mut rng),
                    _ => edges[i % edges.len()],
                })
                .collect();

            // Independent CPU interpolation (including inverse-FFT scaling).
            let mut cpu_coeff = lagrange.clone();
            best_fft(&mut cpu_coeff, domain.get_omega_inv(), k);
            let inverse_n = Fq::from(n as u64).invert().unwrap();
            for coefficient in &mut cpu_coeff {
                *coefficient *= inverse_n;
            }
            let start = Instant::now();
            let expected = oracle(&cpu_coeff, &params.g);
            let cpu_oracle_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert!(
                oracle(&lagrange, &params.g_lagrange) == expected,
                "k {k} pattern {pattern}: CPU basis identity failed"
            );
            if pattern == 0 {
                assert!(bool::from(expected.is_identity()), "k {k}: zero commitment");
            }

            let before = loki_cuda_ntt::completed_calls();
            let start = Instant::now();
            let coeff = domain.lagrange_to_coeff(domain.lagrange_from_vec(lagrange.clone()));
            let conversion_ms = start.elapsed().as_secs_f64() * 1000.0;
            if k >= 10 {
                assert!(
                    loki_cuda_ntt::completed_calls() > before,
                    "k {k} pattern {pattern}: interpolation did not complete a GPU FFT"
                );
            }
            if let Some(index) = coeff.iter().zip(&cpu_coeff).position(|(a, b)| a != b) {
                panic!("k {k} pattern {pattern}: interpolation mismatch at index {index} of {n}");
            }

            let mut timings = [0.0; 4];
            for (basis, (scalars, bases)) in [
                (lagrange.as_slice(), params.g_lagrange.as_slice()),
                (coeff.as_ref(), params.g.as_slice()),
            ]
            .into_iter()
            .enumerate()
            {
                backend::clear_cache().expect("basis cache reset must succeed");
                let before = backend::gpu_calls();
                let start = Instant::now();
                let actual =
                    backend::msm(scalars, bases).expect("direct GPU basis MSM must succeed");
                timings[basis * 2] = start.elapsed().as_secs_f64() * 1000.0;
                assert!(
                    actual == expected,
                    "k {k} pattern {pattern} basis {basis}: cold commitment mismatch"
                );
                assert!(
                    backend::gpu_calls() > before,
                    "k {k} pattern {pattern} basis {basis}: no cold GPU MSM call"
                );
                let before = backend::gpu_calls();
                let start = Instant::now();
                for repetition in 0..3 {
                    let actual = backend::msm(scalars, bases)
                        .expect("direct warm GPU basis MSM must succeed");
                    assert!(actual == expected,
                        "k {k} pattern {pattern} basis {basis} repetition {repetition}: warm commitment mismatch");
                }
                timings[basis * 2 + 1] = start.elapsed().as_secs_f64() * 1000.0 / 3.0;
                assert!(
                    backend::gpu_calls() >= before + 3,
                    "k {k} pattern {pattern} basis {basis}: missing warm GPU MSM calls"
                );
            }
            assert!(
                !backend::failed(),
                "k {k} pattern {pattern}: GPU backend disabled"
            );
            eprintln!(
                "{k},{pattern},{conversion_ms:.3},{cpu_oracle_ms:.3},{:.3},{:.3},{:.3},{:.3}",
                timings[0], timings[1], timings[2], timings[3]
            );
        }
    }
}

#[cfg(feature = "cuda-ntt")]
#[test]
#[ignore = "requires CUDA; four concurrent provers using MSM and NTT"]
fn gpu_concurrent_proofs() {
    let (params, pk) = proof_fixture::concurrent_setup();
    let msm_reference = backend::gpu_calls();
    let ntt_reference = loki_cuda_ntt::completed_calls();
    proof_fixture::concurrent_proof(&params, &pk);
    let msm_before = backend::gpu_calls();
    let ntt_before = loki_cuda_ntt::completed_calls();
    let msm_per_proof = msm_before - msm_reference;
    let ntt_per_proof = ntt_before - ntt_reference;
    assert!(msm_per_proof > 0 && ntt_per_proof > 0);
    let barrier = std::sync::Barrier::new(4);
    std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let barrier = &barrier;
                let params = &params;
                let pk = &pk;
                scope.spawn(move || {
                    barrier.wait();
                    proof_fixture::concurrent_proof(&params, &pk);
                })
            })
            .collect();
        for job in jobs {
            job.join().unwrap();
        }
    });
    assert_eq!(backend::gpu_calls() - msm_before, 4 * msm_per_proof);
    assert_eq!(
        loki_cuda_ntt::completed_calls() - ntt_before,
        4 * ntt_per_proof
    );
    assert!(
        !backend::failed(),
        "concurrent proofs must not silently fall back"
    );
}

#[cfg(feature = "loki-profile")]
#[test]
#[ignore = "measurement; run alone through gpu-runner with --ignored --nocapture"]
fn gpu_srs_prepare_measurement() {
    std::env::set_var("RAYON_NUM_THREADS", "4");
    let mut rng = ChaCha20Rng::from_seed([113; 32]);
    let pool: Vec<_> = (0..1024).map(|_| G1Projective::random(&mut rng)).collect();
    let n = 1 << 20;
    let bases: Vec<_> = (0..n).map(|i| pool[i % pool.len()]).collect();
    let scalars: Vec<_> = (0..n).map(|_| Fq::random(&mut rng)).collect();
    let expected_full = cpu(&scalars, &bases);
    let expected_prefix = cpu(&scalars[..n - 1], &bases);
    for sample in 0..3 {
        backend::clear_cache().unwrap();
        for (label, len, expected) in [
            ("cold_full", n, expected_full),
            ("prefix", n - 1, expected_prefix),
            ("warm_prefix", n - 1, expected_prefix),
        ] {
            let mut proof = crate::profile::ProofGuard::start(20, 1);
            let start = std::time::Instant::now();
            assert_eq!(backend::msm(&scalars[..len], &bases).unwrap(), expected);
            let elapsed = start.elapsed().as_nanos();
            proof.success = true;
            drop(proof);
            let report = crate::profile::take_report();
            eprintln!("SRS_MEASURE sample={sample} label={label} n={len} wall_ns={elapsed} registry={} report={}",
                cfg!(feature = "cuda-srs-registry"), serde_json::to_string(&report).unwrap());
        }
    }
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "registry regression; run alone through gpu-runner"]
fn gpu_srs_prefix_reuses_upload() {
    backend::clear_cache().unwrap();
    let mut rng = ChaCha20Rng::from_seed([114; 32]);
    let bases: Vec<_> = (0..1025).map(|_| G1Projective::random(&mut rng)).collect();
    let scalars: Vec<_> = (0..1025).map(|_| Fq::random(&mut rng)).collect();
    let mut proof = crate::profile::ProofGuard::start(10, 1);
    // A short first request must still upload the supplied capacity once.
    for n in [1024, 1025, 17, 1, 0, 1024] {
        assert_eq!(
            backend::msm(&scalars[..n], &bases).unwrap(),
            cpu(&scalars[..n], &bases)
        );
    }
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 1);
    assert_eq!(counts["msm.prepare.upload"].calls, 1);
    assert_eq!(counts["msm.prepare.upload_bytes"].elements, 1025 * 96);
    assert_eq!(counts["msm.prepare.hit"].calls, 4);
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "registry mutation gate; run alone through gpu-runner"]
#[allow(unsafe_code)]
fn gpu_srs_mutation() {
    use midnight_curves::Fp;
    backend::clear_cache().unwrap();
    let mut rng = ChaCha20Rng::from_seed([115; 32]);
    let mut bases: Vec<_> = (0..4097).map(|_| G1Projective::random(&mut rng)).collect();
    let scalars: Vec<_> = (0..4097).map(|_| Fq::random(&mut rng)).collect();
    let address = bases.as_ptr();
    let mut proof = crate::profile::ProofGuard::start(12, 1);
    let original = backend::msm(&scalars, &bases).unwrap();
    assert_eq!(original, cpu(&scalars, &bases));
    // A changed unused suffix may hit; consuming it must miss and revalidate.
    bases[4096] = G1Projective::identity();
    assert_eq!(
        backend::msm(&scalars[..17], &bases).unwrap(),
        cpu(&scalars[..17], &bases)
    );
    let changed = backend::msm(&scalars, &bases).unwrap();
    assert_ne!(original, changed);
    assert_eq!(changed, cpu(&scalars, &bases));
    // An in-prefix change at the same address must miss too.
    bases[8] = G1Projective::identity();
    assert_eq!(
        backend::msm(&scalars[..17], &bases).unwrap(),
        cpu(&scalars[..17], &bases)
    );
    assert_eq!(bases.as_ptr(), address);
    // A raw corrupted suffix stays unused by the prefix, but the full request
    // must reject it before upload. A rejected request is not a device fault,
    // so the backend stays enabled (quint-q2 L2).
    let raw = blst::blst_p1 {
        x: Fp::ONE.into(),
        y: Fp::ONE.into(),
        z: Fp::ONE.into(),
    };
    bases[4096] = unsafe { std::mem::transmute::<blst::blst_p1, G1Projective>(raw) };
    assert_eq!(
        backend::msm(&scalars[..17], &bases).unwrap(),
        cpu(&scalars[..17], &bases)
    );
    assert!(backend::msm(&scalars, &bases)
        .unwrap_err()
        .contains("off curve"));
    assert!(!backend::failed(), "request rejections are not sticky");
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.hit"].calls, 2);
    assert_eq!(counts["msm.prepare.miss"].calls, 4);
    assert_eq!(counts["msm.prepare.validation"].calls, 4);
    assert_eq!(counts["msm.prepare.upload"].calls, 3);
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "small-budget LRU gate; run alone through gpu-runner"]
fn gpu_srs_eviction() {
    // One 1025-point resident occupies about 3 MiB including sppark buckets;
    // two fit, but three cannot coexist on the target RTX 5090.
    std::env::set_var("LOKI_GPU_SRS_BUDGET_BYTES", "8388608");
    backend::clear_cache().unwrap();
    let mut rng = ChaCha20Rng::from_seed([116; 32]);
    let a: Vec<_> = (0..1025).map(|_| G1Projective::random(&mut rng)).collect();
    let b: Vec<_> = (0..1025).map(|_| G1Projective::random(&mut rng)).collect();
    let c: Vec<_> = (0..1025).map(|_| G1Projective::random(&mut rng)).collect();
    let scalars: Vec<_> = (0..1025).map(|_| Fq::random(&mut rng)).collect();
    let mut proof = crate::profile::ProofGuard::start(10, 1);
    // Touch A before C arrives: B must be evicted, then C when B returns.
    for bases in [&a, &b, &a, &c, &a, &b] {
        assert_eq!(backend::msm(&scalars, bases).unwrap(), cpu(&scalars, bases));
    }
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 4);
    assert_eq!(counts["msm.prepare.upload"].calls, 4);
    assert_eq!(counts["msm.prepare.hit"].calls, 2);
    assert_eq!(counts["msm.prepare.evict"].calls, 2);
    assert!(!backend::failed());
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "k19/k20 resident coexistence gate; run alone through gpu-runner"]
fn gpu_srs_interleaved() {
    backend::clear_cache().unwrap();
    let mut rng = ChaCha20Rng::from_seed([117; 32]);
    let pool: Vec<_> = (0..1024).map(|_| G1Projective::random(&mut rng)).collect();
    // Four independent arrays represent coefficient/Lagrange at k19 and k20.
    let arrays: Vec<Vec<_>> = [19, 20, 19, 20]
        .into_iter()
        .enumerate()
        .map(|(slot, k)| {
            (0..1 << k)
                .map(|i| pool[(i + slot * 31) % pool.len()])
                .collect()
        })
        .collect();
    let scalars: Vec<_> = (0..1 << 20).map(|_| Fq::random(&mut rng)).collect();
    let mut proof = crate::profile::ProofGuard::start(20, 1);
    for bases in &arrays {
        let n = bases.len();
        assert_eq!(
            backend::msm(&scalars[..n], bases).unwrap(),
            cpu(&scalars[..n], bases)
        );
    }
    for slot in [3, 0, 2, 1] {
        let bases = &arrays[slot];
        for n in [bases.len() - 1, 12345, bases.len()] {
            assert_eq!(
                backend::msm(&scalars[..n], bases).unwrap(),
                cpu(&scalars[..n], bases)
            );
        }
    }
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 4);
    assert_eq!(counts["msm.prepare.upload"].calls, 4);
    assert_eq!(counts["msm.prepare.hit"].calls, 12);
    assert!(!counts.contains_key("msm.prepare.evict"));
    assert!(!backend::failed());
}

#[cfg(feature = "cuda-srs-registry")]
#[test]
#[ignore = "Rayon lock-contention gate; run alone through gpu-runner"]
fn gpu_srs_rayon_contention() {
    use rayon::prelude::*;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let mut rng = ChaCha20Rng::from_seed([118; 32]);
    let bases: Vec<_> = (0..4097).map(|_| G1Projective::random(&mut rng)).collect();
    let scalars: Vec<_> = (0..4097).map(|_| Fq::random(&mut rng)).collect();
    let before = backend::gpu_calls();
    pool.install(|| {
        (0..16).into_par_iter().for_each(|i| {
            let n = 4097 - i;
            assert_eq!(
                backend::msm(&scalars[..n], &bases).unwrap(),
                cpu(&scalars[..n], &bases)
            );
        })
    });
    assert_eq!(backend::gpu_calls() - before, 16);
    assert!(!backend::failed());
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "oversize-budget fallback gate; run alone through gpu-runner"]
fn gpu_srs_budget_rejects_oversize() {
    std::env::set_var("LOKI_GPU_SRS_BUDGET_BYTES", "0");
    std::env::set_var("LOKI_GPU_MSM_MIN", "1");
    let bases = [G1Projective::generator()];
    let scalars = [Fq::ONE];
    let mut proof = crate::profile::ProofGuard::start(1, 1);
    assert!(backend::msm(&scalars, &bases)
        .unwrap_err()
        .contains("budget"));
    assert!(!backend::failed(), "a budget refusal is not sticky");
    // The public path retries the GPU, is refused again, and falls back.
    assert_eq!(
        super::super::msm_specific::<G1Affine>(&scalars, &bases),
        bases[0]
    );
    assert!(!backend::failed());
    assert_eq!(backend::gpu_calls(), 0);
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 2);
    assert!(!counts.contains_key("msm.prepare.upload"));
}

#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "malformed unused-capacity fallback gate; run alone through gpu-runner"]
#[allow(unsafe_code)]
fn gpu_srs_invalid_unused_suffix_fallback() {
    use midnight_curves::Fp;
    std::env::set_var("LOKI_GPU_MSM_MIN", "1");
    let mut bases = vec![G1Projective::generator(); 4097];
    let raw = blst::blst_p1 {
        x: Fp::ONE.into(),
        y: Fp::ONE.into(),
        z: Fp::ONE.into(),
    };
    bases[4096] = unsafe { std::mem::transmute::<blst::blst_p1, G1Projective>(raw) };
    let mut proof = crate::profile::ProofGuard::start(1, 1);
    // Registry rejects the invalid supplied capacity, while public zip semantics
    // still produce the valid consumed prefix through the CPU fallback.
    assert_eq!(
        super::super::msm_specific::<G1Affine>(&[Fq::ONE], &bases),
        bases[0]
    );
    assert!(!backend::failed(), "an invalid SRS is not a device fault");
    assert_eq!(backend::gpu_calls(), 0);
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 1);
    assert_eq!(counts["msm.prepare.validation"].calls, 1);
    assert!(!counts.contains_key("msm.prepare.upload"));
}

/// lean-w1 `KernelPrefixOk` (P3): an MSM with n scalars against a resident
/// handle prepared from |S| >= n points equals the CPU MSM over S[..n].
/// One upload of S serves every prefix; n spans 1, 2^k - 1, 2^k and |S|,
/// under random, boolean and repeated scalar distributions.
#[cfg(all(feature = "cuda-srs-registry", feature = "loki-profile"))]
#[test]
#[ignore = "registry kernel-prefix differential; run alone through gpu-runner"]
fn gpu_srs_kernel_prefix_differential() {
    use rand_chacha::rand_core::RngCore;
    backend::clear_cache().unwrap();
    let mut rng = ChaCha20Rng::from_seed([117; 32]);
    let size = (1 << 15) + 5;
    let bases: Vec<_> = (0..size).map(|_| G1Projective::random(&mut rng)).collect();
    let random: Vec<_> = (0..size).map(|_| Fq::random(&mut rng)).collect();
    let boolean: Vec<_> = (0..size)
        .map(|_| if rng.next_u32() & 1 == 1 { Fq::ONE } else { Fq::ZERO })
        .collect();
    let constant = Fq::random(&mut rng);
    let repeated = vec![constant; size];
    let mut proof = crate::profile::ProofGuard::start(15, 1);
    // The first call prepares the handle from all |S| points.
    assert_eq!(backend::msm(&random, &bases).unwrap(), cpu(&random, &bases));
    let mut cases = 1;
    for (label, scalars) in [("random", &random), ("boolean", &boolean), ("repeated", &repeated)] {
        for n in [1, 2, (1 << 14) - 1, 1 << 14, (1 << 15) - 1, 1 << 15, size] {
            assert_eq!(
                backend::msm(&scalars[..n], &bases).unwrap(),
                cpu(&scalars[..n], &bases[..n]),
                "{label} n={n}"
            );
            cases += 1;
        }
    }
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.upload"].calls, 1);
    assert_eq!(counts["msm.prepare.upload_bytes"].elements, (size * 96) as u64);
    assert_eq!(counts["msm.prepare.hit"].calls, cases - 1);
    assert!(!backend::failed());
    eprintln!("P3 kernel-prefix differential: {cases} MSMs on one resident handle of {size} points");
}
