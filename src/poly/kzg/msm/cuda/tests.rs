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
