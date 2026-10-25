//! Explicit regression cases requested from the Lean contracts
//! (`loki-lean/LokiLean/{Msm,Ntt,Transcript}.lean`).

use ff::Field;
use loki_diff::{
    circuit::{Case, Shape},
    proof::{build, roundtrip_case, Verdict, CROSS_MISMATCH},
    two_pow, Input,
};
use midnight_curves::{Fq, G1Affine, G1Projective};
use group::{Curve, Group, GroupEncoding};

fn msm_both(s: &[Fq], b: &[G1Projective]) -> G1Projective {
    let l = loki::poly::kzg::msm::msm_specific::<G1Affine>(s, b);
    let r = stock::poly::kzg::msm::msm_specific::<G1Affine>(s, b);
    loki_diff::msm::assert_reduced(&l, "loki");
    assert!(l == r && l.to_affine().to_bytes().as_ref() == r.to_affine().to_bytes().as_ref());
    l
}

/// Signed-window top digit and reduced outputs: scalars r-1, r-2, 2^254+x,
/// 2^255 mod r neighbours, at sizes around the Rayon chunk (1024) and the
/// GPU threshold (2^14 by default, LOKI_GPU_MSM_MIN=1 in these tests).
#[test]
fn msm_top_window_and_sizes() {
    loki_diff::msm::init();
    let g = G1Projective::generator();
    let tops = [
        -Fq::ONE,
        -Fq::from(2),
        two_pow(254),
        two_pow(254) + Fq::ONE,
        two_pow(254) + two_pow(253) + two_pow(252),
        -two_pow(200),
        two_pow(255), // reduced mod r
    ];
    for n in [1usize, 2, 3, 255, 1023, 1024, 1025, 4096, 16383, 16384, 16385] {
        let bases: Vec<G1Projective> = (0..n as u64).map(|i| g * Fq::from(i * 7 + 1)).collect();
        let scalars: Vec<Fq> = (0..n).map(|i| tops[i % tops.len()] - Fq::from(i as u64)).collect();
        let got = msm_both(&scalars, &bases);
        if n <= 1025 {
            let want = scalars.iter().zip(&bases).fold(G1Projective::identity(), |a, (s, b)| a + b * s);
            assert_eq!(got, want, "n={n}");
        }
        // All r-1: Σ -(i*7+1)·G.
        let minus: Vec<Fq> = vec![-Fq::ONE; n];
        assert_eq!(msm_both(&minus, &bases), -bases.iter().fold(G1Projective::identity(), |a, b| a + b));
    }
}

/// msm_specific zips: extra scalars or bases are ignored; zero scalars,
/// identity bases and empty inputs give the identity.
#[test]
fn msm_zip_truncation_and_degenerate() {
    loki_diff::msm::init();
    let g = G1Projective::generator();
    let bases: Vec<G1Projective> = (1..=40u64).map(|i| g * Fq::from(i)).collect();
    let scalars: Vec<Fq> = (1..=40u64).map(Fq::from).collect();
    let full = msm_both(&scalars[..20], &bases[..20]);
    assert_eq!(msm_both(&scalars[..20], &bases), full);
    assert_eq!(msm_both(&scalars, &bases[..20]), full);
    assert!(bool::from(msm_both(&[], &bases).is_identity()));
    assert!(bool::from(msm_both(&scalars, &[]).is_identity()));
    assert!(bool::from(msm_both(&[Fq::ZERO; 40], &bases).is_identity()));
    assert!(bool::from(msm_both(&scalars, &[G1Projective::identity(); 40]).is_identity()));
    // P and -P with equal scalars cancel.
    assert!(bool::from(msm_both(&[Fq::from(5), Fq::from(5)], &[g, -g]).is_identity()));
}

/// Partial commit_lagrange / commit columns against a 2^12 SRS.
#[test]
fn partial_commit_columns() {
    for k in 1..=12u8 {
        let mut data = vec![0u8, k - 1];
        data.extend(std::iter::repeat(k).take(64));
        loki_diff::msm::run(&data); // mode 0 = commitments
    }
}

/// NTT: every k from 1 to 16 and the GPU threshold boundary (n = 2^9 / 2^10,
/// extended 2^10 at k = 9, j = 2/3), elementwise vs stock and naive.
#[test]
fn ntt_all_sizes_and_threshold() {
    let mut input = Input::new(b"ntt sizes");
    for k in 1..=16u32 {
        for j in [1u32, 2, 3, 5] {
            if k + 3 > 18 && j > 1 {
                continue;
            }
            let n = 1usize << k;
            let ext = loki::poly::EvaluationDomain::<Fq>::new(j, k).extended_len();
            let a: Vec<Fq> = (0..n).map(|_| input.scalar()).collect();
            let e: Vec<Fq> = (0..ext).map(|_| input.scalar()).collect();
            loki_diff::ntt::check(k, j, cfg!(feature = "cuda-ntt"), &a, &e);
        }
    }
}

/// With cuda-ntt the GPU really runs at the first eligible size and not below.
#[cfg(feature = "cuda-ntt")]
#[test]
fn ntt_gpu_threshold_dispatch() {
    let a = vec![Fq::ONE; 1 << 9];
    let before = loki_diff::ntt::gpu_calls();
    let d = loki::poly::EvaluationDomain::<Fq>::new(1, 9);
    let _ = d.coeff_to_lagrange(d.coeff_from_vec(a));
    assert_eq!(loki_diff::ntt::gpu_calls(), before, "2^9 must stay on the CPU");
    let d = loki::poly::EvaluationDomain::<Fq>::new(1, 10);
    let _ = d.coeff_to_lagrange(d.coeff_from_vec(vec![Fq::ONE; 1 << 10]));
    assert_eq!(loki_diff::ntt::gpu_calls(), before + 1, "2^10 must use the GPU");
}

fn verdicts(case: Case, seed: u64) -> [Verdict; 4] {
    let b = build(case).expect("keygen");
    let (l, s) = b.prove_both(seed);
    let (l, s) = (l.unwrap(), s.unwrap());
    [
        b.verify_stock(&b.instances, &s),
        b.verify_loki(&b.instances, &l),
        b.verify_stock(&b.instances, &l),
        b.verify_loki(&b.instances, &s),
    ]
}

fn shape(nb_instance: usize, nb_committed: usize) -> Shape {
    Shape {
        phases: false,
        lookup: false,
        cube: false,
        zero_col: false,
        nb_instance,
        nb_committed,
    }
}

/// Transcript order: with several proofs, committed AND plain instance
/// columns, the upstream prover absorbs (C0, P0, C1, P1) while the verifier
/// absorbs (C0, C1, P0, P1), so stock rejects its own proof; Loki must behave
/// identically. Neighbouring shapes must verify in all four directions.
#[test]
fn multi_proof_committed_instance_order() {
    let mut input = Input::new(b"committed-order");
    for (ni, nc, proofs, expect_accept) in [
        (2, 1, 2, false), // the mismatch
        (2, 1, 1, true),
        (2, 0, 2, true),
        (1, 1, 2, true),
        (2, 2, 2, true),
        (1, 0, 2, true),
    ] {
        let case = Case::with_shape(&mut input, 5, shape(ni, nc), proofs);
        assert_eq!(case.order_mismatch(), !expect_accept);
        let v = verdicts(case.clone(), 3);
        eprintln!("instances={ni} committed={nc} proofs={proofs}: {v:?}");
        let cross_ok = !CROSS_MISMATCH;
        assert_eq!(v[0].accepted(), expect_accept, "stock self ({ni},{nc},{proofs})");
        assert_eq!(v[1].accepted(), expect_accept, "loki self ({ni},{nc},{proofs})");
        assert_eq!(v[2].accepted(), expect_accept && cross_ok, "loki->stock ({ni},{nc},{proofs})");
        assert_eq!(v[3].accepted(), expect_accept && cross_ok, "stock->loki ({ni},{nc},{proofs})");
        roundtrip_case(case, 9);
    }
}

/// truncated-challenges parity: run under `--features truncated` (both
/// crates truncate: all accept) and `--features truncated-loki-only` (cross
/// directions must be rejected). This test is the same code in all builds.
#[test]
fn truncated_challenges_parity() {
    let mut input = Input::new(b"truncated");
    let full = Shape {
        phases: true,
        lookup: true,
        cube: true,
        zero_col: true,
        nb_instance: 2,
        nb_committed: 1,
    };
    let v = verdicts(Case::with_shape(&mut input, 6, full, 1), 5);
    eprintln!("truncated feature: loki={} stock={} -> {v:?}", cfg!(feature = "truncated") || CROSS_MISMATCH, cfg!(feature = "truncated"));
    assert_eq!(v[0], Verdict::Accept);
    assert_eq!(v[1], Verdict::Accept);
    assert_eq!(v[2].accepted(), !CROSS_MISMATCH);
    assert_eq!(v[3].accepted(), !CROSS_MISMATCH);
}

/// Upstream (stock and Loki alike) panics on hostile verifying keys, found
/// by `proof_parse`: (1) a k byte near 32 passes the `k <= S` check but
/// `EvaluationDomain::new` asserts `extended_k <= S`; (2) a VK declaring fewer
/// fixed commitments than the constraint system uses is accepted by `read`
/// and the verifier then indexes past the end. Both crates must behave alike.
#[test]
fn upstream_hostile_vk_panics() {
    let fx = loki_diff::parse::fixture();
    let b = &fx.built;
    let layout = b.case.layout.clone();
    let proof = &fx.proofs[0];
    let both = |vk: &[u8]| {
        let l = loki_diff::catch(|| loki_diff::proof::lk::verify(&layout, vk, &b.lvp, &b.instances, proof));
        let s = loki_diff::catch(|| loki_diff::proof::st::verify(&layout, vk, &b.svp, &b.instances, proof));
        (l, s)
    };
    // (1) k = 32.
    let mut vk = b.vk.clone();
    vk[1] = 32;
    let (l, s) = both(&vk);
    assert!(l.is_err() && s.is_err(), "k=32 VK: loki={l:?} stock={s:?}");
    assert!(l.unwrap_err().contains("extended_k <= F::S"));
    // (2) Drop the last fixed commitment and decrement the count.
    let nfixed = u32::from_le_bytes(vk[2..6].try_into().unwrap()) as usize;
    let mut found = false;
    for size in [48usize, 96] {
        let mut v = b.vk.clone();
        v[2..6].copy_from_slice(&((nfixed - 1) as u32).to_le_bytes());
        let start = 6 + (nfixed - 1) * size;
        v.drain(start..start + size);
        let (l, s) = both(&v);
        if let (Err(le), Err(se)) = (&l, &s) {
            assert!(le.contains("index out of bounds") && se.contains("index out of bounds"), "{le} / {se}");
            found = true;
        } else {
            assert_eq!(l.as_ref().map(|v| v.accepted()).ok(), s.as_ref().map(|v| v.accepted()).ok());
        }
    }
    assert!(found, "short fixed-commitment VK did not reproduce the panic");
}
