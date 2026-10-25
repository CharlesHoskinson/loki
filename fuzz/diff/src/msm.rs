//! `msm_diff`: Loki `msm_specific` (CPU chunked Pippenger, or the GPU path
//! with `--features cuda`) == stock `msm_specific` == naive sum, plus partial
//! `commit_lagrange` / `commit` columns through both commitment schemes.
//!
//! Lean: `Msm.MeetsMsmSpecific` (zip truncation, zero filter, identity on
//! empty), `pippengerFr_eq_msm` (top window near r), fully reduced outputs.

use std::sync::OnceLock;

use ff::Field;
use group::{Curve, Group, GroupEncoding};
use midnight_curves::{Bls12, Fq, G1Affine, G1Projective};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

use crate::{limbs_lt, Input, P_LIMBS};

/// Largest MSM length exercised (also the SRS size for commitments).
pub const MAX_LOG: u32 = 12;
pub const MAX_N: usize = 1 << MAX_LOG;
/// Naive Σ sᵢ·Pᵢ is computed up to this many terms.
const NAIVE_MAX: usize = 96;

fn pool() -> &'static [G1Projective] {
    static POOL: OnceLock<Vec<G1Projective>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut rng = ChaCha20Rng::from_seed([0x31; 32]);
        let g = G1Projective::generator();
        (0..MAX_N + 64).map(|_| g * Fq::random(&mut rng)).collect()
    })
}

struct Srs {
    loki: loki::poly::kzg::params::ParamsKZG<Bls12>,
    stock: stock::poly::kzg::params::ParamsKZG<Bls12>,
}

fn srs() -> &'static Srs {
    static SRS: OnceLock<Srs> = OnceLock::new();
    SRS.get_or_init(|| {
        let params = stock::poly::kzg::params::ParamsKZG::<Bls12>::unsafe_setup(
            MAX_LOG,
            ChaCha20Rng::from_seed([0x53; 32]),
        );
        let mut bytes = Vec::new();
        params
            .write_custom(&mut bytes, stock::utils::SerdeFormat::RawBytes)
            .unwrap();
        let loki = loki::poly::kzg::params::ParamsKZG::<Bls12>::read_custom(
            &mut &bytes[..],
            loki::utils::SerdeFormat::RawBytes,
        )
        .unwrap();
        Srs { loki, stock: params }
    })
}

/// Process setup shared by all MSM entry points.
pub fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Route every MSM to the GPU (the default threshold is 2^14 terms);
        // read once by Loki's dispatcher, so set it before the first call.
        if std::env::var_os("LOKI_GPU_MSM_MIN").is_none() {
            std::env::set_var("LOKI_GPU_MSM_MIN", "1");
        }
        pool();
    });
}

/// Every coordinate is a fully reduced Montgomery residue, and the point is
/// on the curve.
pub fn assert_reduced(p: &G1Projective, what: &str) {
    use midnight_curves::serde::SerdeObject;
    for (name, c) in [("x", p.x()), ("y", p.y()), ("z", p.z())] {
        assert!(
            limbs_lt(&c.to_raw_bytes(), &P_LIMBS),
            "{what}: coordinate {name} is not fully reduced"
        );
    }
    assert!(bool::from(p.is_on_curve()), "{what}: result is not on the curve");
}

fn naive(scalars: &[Fq], bases: &[G1Projective]) -> G1Projective {
    scalars
        .iter()
        .zip(bases)
        .fold(G1Projective::identity(), |acc, (s, b)| acc + b * s)
}

fn same(a: &G1Projective, b: &G1Projective) -> bool {
    a == b && a.to_affine().to_bytes().as_ref() == b.to_affine().to_bytes().as_ref()
}

/// Generate scalars and bases with zeros, identities, duplicates and negations.
pub fn case(input: &mut Input) -> (Vec<Fq>, Vec<G1Projective>) {
    let pool = pool();
    let n = match input.u8() % 8 {
        0 => input.range(0, 4),
        1..=3 => input.range(0, 64),
        4 => input.range(0, 512),
        // Rayon chunk (1024) and window boundaries.
        5 => [1023, 1024, 1025, 2047, 2048, 2049, 3072, 4095, 4096][input.range(0, 8)],
        _ => input.range(0, MAX_N),
    };
    let dense = input.u8();
    let offset = input.range(0, 64);
    let mut scalars: Vec<Fq> = Vec::with_capacity(n);
    let mut bases: Vec<G1Projective> = Vec::with_capacity(n);
    for i in 0..n {
        // After the explicit bytes run out, keep a dense stream of edge cases.
        let tag = if input.exhausted() && dense < 128 { 255 } else { input.u8() };
        let base = match tag % 16 {
            0 => G1Projective::identity(),
            1 if i > 0 => bases[input.range(0, i - 1)],
            2 if i > 0 => -bases[input.range(0, i - 1)],
            3 => G1Projective::generator(),
            _ => pool[offset + i],
        };
        let scalar = match (tag >> 4) % 16 {
            0 if i > 0 => scalars[input.range(0, i - 1)],
            1 if i > 0 => -scalars[input.range(0, i - 1)],
            2..=9 => input.scalar(),
            _ => input.rng_scalar(),
        };
        bases.push(base);
        scalars.push(scalar);
    }
    // Zip truncation: msm_specific uses min(len) terms.
    match input.u8() % 8 {
        0 => {
            let extra = input.range(1, 64);
            bases.extend_from_slice(&pool[..extra]);
        }
        1 => {
            let extra = input.range(1, 64);
            for _ in 0..extra {
                scalars.push(input.scalar());
            }
        }
        _ => {}
    }
    (scalars, bases)
}

pub fn run(data: &[u8]) {
    init();
    let mut input = Input::new(data);
    match input.u8() % 4 {
        0 => commit(&mut input),
        _ => msm(&mut input),
    }
}

fn msm(input: &mut Input) {
    let (scalars, bases) = case(input);
    let loki = loki::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &bases);
    let stock = stock::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &bases);
    assert_reduced(&loki, "loki msm_specific");
    assert!(
        same(&loki, &stock),
        "msm_specific mismatch: n_scalars={} n_bases={}",
        scalars.len(),
        bases.len()
    );
    let terms = scalars.len().min(bases.len());
    if terms <= NAIVE_MAX {
        assert!(same(&loki, &naive(&scalars, &bases)), "msm_specific != naive (n={terms})");
    }
    if terms == 0 || scalars[..terms].iter().all(|s| bool::from(s.is_zero())) {
        assert!(bool::from(loki.is_identity()), "empty/zero MSM is not the identity");
    }
    // Same scalars again on the same bases must hit any resident-base cache
    // and still agree; then on a different base slice of the same length.
    let again = loki::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &bases);
    assert!(same(&again, &loki), "repeated MSM differs");
    if terms > 0 && input.bool() {
        let mut moved = bases.clone();
        let i = input.range(0, terms - 1);
        moved[i] += G1Projective::generator();
        let loki = loki::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &moved);
        let stock = stock::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &moved);
        assert!(same(&loki, &stock), "MSM after base change differs (stale bases?)");
    }
}

/// Partial `commit_lagrange` and `commit`: a column of 2^k' values against the
/// 2^12 SRS uses the first 2^k' bases (Lean `commit_lagrange` size < n).
fn commit(input: &mut Input) {
    use loki::poly::commitment::PolynomialCommitmentScheme as LokiPcs;
    use stock::poly::commitment::PolynomialCommitmentScheme as StockPcs;
    let srs = srs();
    let k = input.range(1, MAX_LOG as usize) as u32;
    let n = 1usize << k;
    let values: Vec<Fq> = (0..n)
        .map(|_| if input.exhausted() { input.rng_scalar() } else { input.scalar() })
        .collect();
    let ld = loki::poly::EvaluationDomain::<Fq>::new(1, k);
    let sd = stock::poly::EvaluationDomain::<Fq>::new(1, k);
    let lagrange = input.bool();
    let (l, s) = if lagrange {
        (
            <loki::poly::kzg::KZGCommitmentScheme<Bls12> as LokiPcs<Fq>>::commit_lagrange(
                &srs.loki,
                &ld.lagrange_from_vec(values.clone()),
            ),
            <stock::poly::kzg::KZGCommitmentScheme<Bls12> as StockPcs<Fq>>::commit_lagrange(
                &srs.stock,
                &sd.lagrange_from_vec(values.clone()),
            ),
        )
    } else {
        (
            <loki::poly::kzg::KZGCommitmentScheme<Bls12> as LokiPcs<Fq>>::commit(
                &srs.loki,
                &ld.coeff_from_vec(values.clone()),
            ),
            <stock::poly::kzg::KZGCommitmentScheme<Bls12> as StockPcs<Fq>>::commit(
                &srs.stock,
                &sd.coeff_from_vec(values.clone()),
            ),
        )
    };
    assert_reduced(&l, "loki commitment");
    assert!(same(&l, &s), "commit(lagrange={lagrange}) mismatch at k={k}");
    // Reference: the first 2^k entries of the full-size Lagrange basis.
    if lagrange && n <= NAIVE_MAX {
        let basis = &srs.stock.g_lagrange()[..n];
        assert!(same(&l, &naive(&values, basis)), "commit_lagrange != naive prefix");
    }
}
