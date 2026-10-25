//! P1 default-build gate: Loki with only its default features must behave as
//! the unmodified crates.io `midnight-proofs =0.8.3` package (`stock`).
//!
//! - no-lookup and single-leftover lookup proofs are byte-identical to stock
//!   under the same SRS, VK, proof RNG seed and a one-worker Rayon pool;
//! - many-leftover lookup proofs cross-verify; their bytes follow `HashMap`
//!   order in both crates and are only reported (stock-vs-stock too);
//! - the default MSM and all five domain conversions equal stock exactly;
//! - the default build enables no backend, CUDA, ordering or CPU-budget
//!   feature.

#[path = "support/stock_fixture.rs"]
mod fixture;

use ff::{Field, PrimeField};
use fixture::{
    cross_verify, first_difference, single_thread, MANY_LEFTOVERS, NO_LOOKUP, SINGLE_LEFTOVER,
};
use group::{Curve, Group};
use midnight_curves::{Fq, G1Affine, G1Projective, G2Affine, G2Projective};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

const NON_DEFAULT: &[(&str, bool)] = &[
    ("loki-backend-api", cfg!(feature = "loki-backend-api")),
    ("loki-sppark", cfg!(feature = "loki-sppark")),
    ("loki-gpu-backend", cfg!(feature = "loki-gpu-backend")),
    ("loki-deterministic-lookup-order", cfg!(feature = "loki-deterministic-lookup-order")),
    ("cuda", cfg!(feature = "cuda")),
    ("cuda-ntt", cfg!(feature = "cuda-ntt")),
    ("cuda-srs-registry", cfg!(feature = "cuda-srs-registry")),
    ("loki-cpu-budget", cfg!(feature = "loki-cpu-budget")),
    ("loki-profile", cfg!(feature = "loki-profile")),
    ("loki-multiopen", cfg!(feature = "loki-multiopen")),
    ("loki-harden", cfg!(feature = "loki-harden")),
];

fn is_default_build() -> bool {
    NON_DEFAULT.iter().all(|(_, on)| !on)
}

/// Manifest gate, independent of how this test binary was configured: both
/// manifests must declare exactly stock's `default = ["committed-instances"]`.
/// (A cfg-based check alone cannot catch a feature added to `default`, since
/// that would also change this binary's own cfg.)
#[test]
fn manifest_default_features_are_stock() {
    for name in ["Cargo.toml", "Cargo.toml.orig"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let manifest = std::fs::read_to_string(&path).unwrap();
        let defaults: Vec<_> = manifest
            .lines()
            .map(str::trim)
            .filter(|line| line.split_once('=').is_some_and(|(key, _)| key.trim() == "default"))
            .collect();
        assert_eq!(
            defaults,
            vec![r#"default = ["committed-instances"]"#],
            "{name}: default feature set differs from stock 0.8.3"
        );
    }
}

/// Configuration check: when this binary is built with Cargo's defaults, no
/// backend, CUDA, ordering, profiling or CPU-budget feature is active. Under
/// explicit `--features` it reports and skips; the manifest gate above is the
/// enforced default-set check.
#[test]
fn default_build_configuration_is_stock() {
    assert!(cfg!(feature = "committed-instances"));
    if !is_default_build() {
        let enabled: Vec<_> = NON_DEFAULT.iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
        eprintln!("non-default configuration {enabled:?}: manifest gate enforces the default set");
    }
}

#[test]
fn no_lookup_proof_bytes_equal_stock() {
    for seed in [0x42, 0x07] {
        let (loki, stock) = cross_verify::<NO_LOOKUP>(seed);
        assert_eq!(
            first_difference(&loki, &stock),
            None,
            "no-lookup proof bytes differ from stock (seed {seed:#x})"
        );
    }
}

/// Controlled map order: only one leftover key has a nonzero count, so the
/// stock `HashMap` traversal cannot affect placement.
#[test]
fn single_leftover_lookup_proof_bytes_equal_stock() {
    let (loki, stock) = cross_verify::<SINGLE_LEFTOVER>(0x42);
    assert_eq!(
        first_difference(&loki, &stock),
        None,
        "controlled-order lookup proof bytes differ from stock"
    );
}

#[test]
fn many_leftover_lookup_cross_verifies() {
    let (loki, stock) = cross_verify::<MANY_LEFTOVERS>(0x42);
    // Stock-vs-stock repeatability under the same seed and pool size.
    let (_, stock_again) = cross_verify::<MANY_LEFTOVERS>(0x42);
    eprintln!(
        "many_leftovers loki_vs_stock_first_diff={:?} stock_vs_stock_first_diff={:?} len={}",
        first_difference(&loki, &stock),
        first_difference(&stock, &stock_again),
        loki.len()
    );
    assert_eq!(loki.len(), stock.len(), "proof length differs from stock");
}

fn scalars(n: usize, seed: u8) -> Vec<Fq> {
    let mut rng = ChaCha20Rng::from_seed([seed; 32]);
    (0..n).map(|_| Fq::random(&mut rng)).collect()
}

fn g1_bases(n: usize, seed: u8) -> Vec<G1Projective> {
    let mut rng = ChaCha20Rng::from_seed([seed; 32]);
    (0..n)
        .map(|i| if i % 11 == 0 { G1Projective::identity() } else { G1Projective::random(&mut rng) })
        .collect()
}

/// Scalar patterns that random-only tests miss.
fn scalar_patterns(n: usize) -> Vec<(&'static str, Vec<Fq>)> {
    vec![
        ("random", scalars(n, 1)),
        ("zero", vec![Fq::ZERO; n]),
        ("one", vec![Fq::ONE; n]),
        ("minus_one", vec![-Fq::ONE; n]),
        ("boolean", (0..n).map(|i| Fq::from((i % 2) as u64)).collect()),
        ("small", (0..n).map(|i| Fq::from((i % 16) as u64)).collect()),
        ("sparse", (0..n).map(|i| if i % 7 == 0 { Fq::from(i as u64) } else { Fq::ZERO }).collect()),
    ]
}

#[test]
fn default_msm_equals_stock() {
    for n in [0usize, 1, 2, 31, 257, 1500] {
        let bases = g1_bases(n + 3, 9);
        for (name, coeffs) in scalar_patterns(n) {
            // Equal, shorter-bases and shorter-scalars prefixes.
            for (c, b) in [
                (&coeffs[..], &bases[..n]),
                (&coeffs[..], &bases[..n.saturating_sub(1)]),
                (&coeffs[..n.saturating_sub(2)], &bases[..]),
            ] {
                let loki = midnight_proofs::poly::kzg::msm::msm_specific::<G1Affine>(c, b);
                let stock = stock::poly::kzg::msm::msm_specific::<G1Affine>(c, b);
                assert_eq!(loki.to_affine(), stock.to_affine(), "{name} n={n}");
            }
        }
    }
    // Non-G1 curve takes the generic `msm_best` branch in both crates.
    let mut rng = ChaCha20Rng::from_seed([5; 32]);
    let g2: Vec<_> = (0..40).map(|_| G2Projective::random(&mut rng)).collect();
    let coeffs = scalars(40, 6);
    assert_eq!(
        midnight_proofs::poly::kzg::msm::msm_specific::<G2Affine>(&coeffs, &g2).to_affine(),
        stock::poly::kzg::msm::msm_specific::<G2Affine>(&coeffs, &g2).to_affine()
    );
}

#[test]
fn default_domain_conversions_equal_stock() {
    single_thread(|| {
        for (j, k) in [(3u32, 1u32), (3, 4), (4, 5), (5, 8), (7, 10)] {
            let loki = midnight_proofs::poly::EvaluationDomain::<Fq>::new(j, k);
            let stock = stock::poly::EvaluationDomain::<Fq>::new(j, k);
            assert_eq!(loki.get_omega(), stock.get_omega());
            assert_eq!(loki.get_extended_omega(), stock.get_extended_omega());
            let values = scalars(1 << k, k as u8);

            let a = loki.coeff_to_lagrange(loki.coeff_from_vec(values.clone()));
            let b = stock.coeff_to_lagrange(stock.coeff_from_vec(values.clone()));
            assert_eq!(a[..], b[..], "coeff_to_lagrange k={k}");

            let a = loki.lagrange_to_coeff(loki.lagrange_from_vec(values.clone()));
            let b = stock.lagrange_to_coeff(stock.lagrange_from_vec(values.clone()));
            assert_eq!(a[..], b[..], "lagrange_to_coeff k={k}");

            let a = loki.coeff_to_extended(loki.coeff_from_vec(values.clone()));
            let b = stock.coeff_to_extended(stock.coeff_from_vec(values.clone()));
            assert_eq!(a[..], b[..], "coeff_to_extended k={k}");

            let ea = loki.extended_to_coeff(a.clone());
            let eb = stock.extended_to_coeff(b.clone());
            assert_eq!(ea, eb, "extended_to_coeff k={k}");

            let la = loki.extended_to_lagrange(a);
            let lb = stock.extended_to_lagrange(b);
            assert_eq!(la[..], lb[..], "extended_to_lagrange k={k}");
        }
    });
    // Sanity: the field has the two-adicity the domain tests rely on.
    assert!(Fq::S >= 10);
}

/// Drift detector for the restored stock traversal. Stock places 192 distinct
/// leftover keys in per-map `HashMap` order, so two in-process proofs with the
/// same seed differ (exactly like stock-vs-stock above). The pre-P1 Loki
/// unconditional sort made them identical; that behaviour is now only
/// available under `loki-deterministic-lookup-order`.
#[test]
fn lookup_leftovers_follow_stock_hashmap_order() {
    let proofs: Vec<Vec<u8>> = single_thread(|| {
        let params = fixture::loki::setup();
        (0..2)
            .map(|_| fixture::loki::prove::<MANY_LEFTOVERS>(&params, 0x42).proof)
            .collect()
    });
    let difference = first_difference(&proofs[0], &proofs[1]);
    if cfg!(feature = "loki-deterministic-lookup-order") {
        assert_eq!(difference, None, "deterministic lookup order must be repeatable");
    } else {
        assert!(
            difference.is_some(),
            "default lookup placement is repeatable: leftover keys are not in stock HashMap order"
        );
    }
}
