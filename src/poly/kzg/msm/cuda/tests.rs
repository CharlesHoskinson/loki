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
    // must reject it before upload. This runs last: errors disable the backend.
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
    assert!(backend::failed());
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
    assert!(backend::failed());
    assert_eq!(
        super::super::msm_specific::<G1Affine>(&scalars, &bases),
        bases[0]
    );
    assert_eq!(backend::gpu_calls(), 0);
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 1);
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
    assert!(backend::failed());
    assert_eq!(backend::gpu_calls(), 0);
    proof.success = true;
    drop(proof);
    let report = crate::profile::take_report();
    let counts = &report.proofs.last().unwrap().phases["other"].backends;
    assert_eq!(counts["msm.prepare.miss"].calls, 1);
    assert_eq!(counts["msm.prepare.validation"].calls, 1);
    assert!(!counts.contains_key("msm.prepare.upload"));
}
