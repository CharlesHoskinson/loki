//! P4 differential tests: GPU transforms against the naive DFT and the
//! unmodified stock `midnight-proofs 0.8.3` `EvaluationDomain`.
//!
//! Layout:
//! * oracle self-checks (pinned constants, naive vs stock) — no device;
//! * request validation (rejected shapes, wrong roots, unsupported field) —
//!   pure, plus the same rejections through a live runtime with zero launches;
//! * device differentials: every domain conversion at small sizes and at the
//!   k19/k20 base and extended domains, zero/one/max/impulse/mixed inputs,
//!   bit-reversed boundary orders, both inverse normalizations, rotations;
//! * negative control: a wrong (but valid-order) root round-trips perfectly
//!   on the GPU yet fails the direct differential.
//!
//! A missing device fails the gate; `LOKI_GPU_ALLOW_NO_DEVICE=1` skips device
//! tests on a toolkit-only host (same convention as `ffi_abi`). Run device
//! commands under the shared GPU lock.

mod ntt_support;
#[allow(dead_code)]
mod support;

use std::sync::{Mutex, MutexGuard};

use ff::{Field, PrimeField, WithSmallOrderMulGroup};
use loki_backend_api::{
    ElementOrder, ErrorKind, FailureEffect, InverseNormalization, Operation,
    TransformDirection, TransformSpec,
};
use loki_gpu::ntt::{
    transform_chain, transform_host, validate_transform, wire_field_supported,
    DomainTransforms, MAX_TRANSFORM_LOG,
};
use loki_gpu::runtime::{GpuRuntime, RuntimeConfig};
use ntt_support::*;

// ---------------------------------------------------------------------------
// Oracle self-checks (no device)
// ---------------------------------------------------------------------------

/// Stock `ZETA` (order three), big-endian hex. Same value as
/// `LokiLean.Ntt.zetaVal`.
const PINNED_ZETA_HEX: &str = "00000000000000000000000000000000ac45a4010001a40200000000ffffffff";

#[test]
fn pinned_root_and_zeta_match_stock() {
    assert_eq!(fr_to_hex(&Fr::ROOT_OF_UNITY), PINNED_ROOT_HEX);
    assert_eq!(Fr::S, 32);
    assert!(has_exact_order(Fr::ROOT_OF_UNITY, 32));
    let zeta = <Fr as WithSmallOrderMulGroup<3>>::ZETA;
    assert_eq!(fr_to_hex(&zeta), PINNED_ZETA_HEX);
    assert_ne!(zeta, Fr::ONE);
    assert_eq!(zeta.square() * zeta, Fr::ONE);
    assert_eq!(zeta.invert().unwrap(), zeta.square());
    assert_eq!(fr_to_hex(&max_fr()).len(), 64);
    assert_eq!(fr_from_hex(PINNED_ROOT_HEX), Fr::ROOT_OF_UNITY);
    // the modulus string the wire check relies on
    assert_eq!(Fr::MODULUS.trim_start_matches("0x"), MODULUS_HEX);
}

#[test]
fn stock_domain_roots_are_the_pinned_roots() {
    for (j, k) in [(3, 1), (3, 10), (5, 12), (3, 19), (5, 19), (3, 20), (5, 20), (9, 20)] {
        let s = Stock::new(j, k);
        assert_eq!(s.omega, stock_root(s.k), "omega j={j} k={k}");
        assert_eq!(s.extended_omega, stock_root(s.extended_k), "omega_e j={j} k={k}");
        assert_eq!(pow2(s.extended_omega, s.extended_k - s.k), s.omega);
        assert!(has_exact_order(s.omega, s.k));
        assert!(has_exact_order(s.extended_omega, s.extended_k));
        assert_eq!(s.zeta, <Fr as WithSmallOrderMulGroup<3>>::ZETA);
    }
    // Extension factors used below: j=3 -> +1, j=5 -> +2, j=9 -> +3.
    assert_eq!(Stock::new(3, 20).extended_k, 21);
    assert_eq!(Stock::new(5, 20).extended_k, 22);
    assert_eq!(Stock::new(9, 20).extended_k, 23);
}

#[test]
fn naive_dft_matches_stock_for_every_conversion() {
    for (j, k) in [(3u32, 1u32), (3, 2), (5, 3), (3, 5), (5, 6), (9, 4)] {
        let s = Stock::new(j, k);
        for pattern in ALL_PATTERNS {
            let a = input(pattern, s.n(), 11);
            let e = input(pattern, s.big_n(), 12);
            assert_vec_eq("c2l", &s.coeff_to_lagrange(&a), &naive_dft(&a, s.omega));
            assert_vec_eq("l2c", &s.lagrange_to_coeff(&a), &naive_idft(&a, s.omega));
            assert_vec_eq(
                "c2e",
                &s.coeff_to_extended(&a),
                &naive_coset_eval(&a, s.zeta, s.extended_omega, s.big_n()),
            );
            let mut want = naive_idft(&e, s.extended_omega);
            let zinv = s.zeta.invert().unwrap();
            let mut z = Fr::ONE;
            for x in want.iter_mut() {
                *x *= z;
                z *= zinv;
            }
            assert_vec_eq("e2c", &s.extended_to_coeff(&e), &want);
            want.truncate(s.n());
            assert_vec_eq("e2l", &s.extended_to_lagrange(&e), &naive_dft(&want, s.omega));
        }
    }
}

#[test]
fn domain_transforms_helper_builds_the_hand_written_specs() {
    for (j, k) in [(3, 4), (5, 19), (5, 20)] {
        let s = Stock::new(j, k);
        let d = DomainTransforms {
            k: s.k,
            extended_k: s.extended_k,
            omega: s.omega,
            extended_omega: s.extended_omega,
            zeta: s.zeta,
        };
        assert_eq!(d.coeff_to_lagrange(), s.spec_coeff_to_lagrange());
        assert_eq!(d.lagrange_to_coeff(), s.spec_lagrange_to_coeff());
        assert_eq!(d.coeff_to_extended().unwrap(), s.spec_coeff_to_extended());
        assert_eq!(d.extended_to_coeff().unwrap(), s.spec_extended_to_coeff());
        let [a, b] = d.extended_to_lagrange().unwrap();
        assert_eq!(a, s.spec_extended_to_coeff_prefix(s.k));
        assert_eq!(b, s.spec_coeff_to_lagrange());
    }
}

// ---------------------------------------------------------------------------
// Validation: rejected shapes (pure; no device)
// ---------------------------------------------------------------------------

fn expect_reject(len: usize, spec: TransformSpec<Fr>, kind: ErrorKind, label: &str) {
    let e = validate_transform(len, &spec, Operation::Transform).expect_err(label);
    assert_eq!(e.kind, kind, "{label}");
    assert_eq!(e.effect, FailureEffect::NotStarted, "{label}");
}

#[test]
fn rejected_shapes_and_roots() {
    let s = Stock::new(5, 6); // n = 64, N = 256
    let (n, big) = (s.n(), s.big_n());
    let inv = ErrorKind::InvalidInput;
    // accepted baselines
    for (len, spec) in [
        (n, s.spec_coeff_to_lagrange()),
        (n, s.spec_lagrange_to_coeff()),
        (n, s.spec_coeff_to_extended()),
        (big, s.spec_extended_to_coeff()),
        (big, s.spec_extended_to_coeff_prefix(s.k)),
    ] {
        validate_transform(len, &spec, Operation::Transform).expect("valid baseline");
    }
    // exact source length
    expect_reject(n - 1, s.spec_coeff_to_lagrange(), inv, "short");
    expect_reject(n + 1, s.spec_coeff_to_lagrange(), inv, "long");
    expect_reject(0, s.spec_coeff_to_lagrange(), inv, "empty");
    expect_reject(big, s.spec_coeff_to_extended(), inv, "c2e given extended length");
    expect_reject(n, s.spec_extended_to_coeff(), inv, "e2c given base length");
    // exactly one 1/n on inverses, none on forward transforms
    for (len, mut sp) in [(n, s.spec_lagrange_to_coeff()), (big, s.spec_extended_to_coeff())] {
        sp.inverse_normalization = InverseNormalization::None;
        expect_reject(len, sp, inv, "inverse without normalization");
    }
    for (len, mut sp) in [(n, s.spec_coeff_to_lagrange()), (n, s.spec_coeff_to_extended())] {
        sp.inverse_normalization = InverseNormalization::DivideByLengthOnce;
        expect_reject(len, sp, inv, "normalized forward");
    }
    // plain transforms keep the log
    let mut sp = s.spec_coeff_to_lagrange();
    sp.output_log_size += 1;
    expect_reject(n, sp, inv, "plain pad");
    // forward coset may only pad, inverse coset may only truncate
    let mut sp = s.spec_coeff_to_extended();
    sp.source_log_size = s.extended_k + 1;
    expect_reject(big * 2, sp, inv, "forward coset shrink");
    let mut sp = s.spec_extended_to_coeff();
    sp.output_log_size = s.extended_k + 1;
    expect_reject(big, sp, inv, "inverse coset grow");
    // coset boundaries are natural order
    for (input_order, output_order) in [
        (ElementOrder::BitReversed, ElementOrder::Natural),
        (ElementOrder::Natural, ElementOrder::BitReversed),
    ] {
        let mut sp = s.spec_coeff_to_extended();
        sp.input_order = input_order;
        sp.output_order = output_order;
        expect_reject(n, sp, inv, "coset bitrev");
    }
    // forward transforms carry no normalization
    let mut sp = s.spec_coeff_to_lagrange();
    sp.inverse_normalization = InverseNormalization::DivideByLengthOnce;
    expect_reject(n, sp, inv, "forward normalized");
    // root must have exact order 2^extent (forward root, never pre-inverted
    // is not detectable by order; see wrong_root tests), and the extent is the
    // output log for forward coset, the source log for inverse coset.
    for (root, label) in [
        (Fr::ZERO, "zero root"),
        (Fr::ONE, "one root"),
        (-Fr::ONE, "order-2 root"),
        (stock_root(s.k + 1), "order 2n root"),
        (stock_root(s.k - 1), "order n/2 root"),
        (s.zeta, "zeta as root"),
    ] {
        let mut sp = s.spec_coeff_to_lagrange();
        sp.root = root;
        expect_reject(n, sp, inv, label);
    }
    let mut sp = s.spec_coeff_to_extended();
    sp.root = s.omega; // small root on the extended extent
    expect_reject(n, sp, inv, "c2e with small root");
    let mut sp = s.spec_extended_to_coeff_prefix(s.k);
    sp.root = s.omega;
    expect_reject(big, sp, inv, "e2c prefix with small root");
    // coset constants must be inverse to each other
    let mut sp = s.spec_coeff_to_extended();
    sp.coset.as_mut().unwrap().generator_inverse = s.zeta;
    expect_reject(n, sp, inv, "zeta inverse wrong");
    let mut sp = s.spec_coeff_to_extended();
    let c = sp.coset.as_mut().unwrap();
    c.generator = Fr::ZERO;
    c.generator_inverse = Fr::ZERO;
    expect_reject(n, sp, inv, "zero generator");
    // log 0 transform: root must be exactly one
    let mut sp = plain(Fr::ONE, 0, TransformDirection::Forward);
    validate_transform(1, &sp, Operation::Transform).expect("log0 ok");
    sp.root = -Fr::ONE;
    expect_reject(1, sp, inv, "log0 root -1");
    // beyond the supported maximum: Unsupported (CPU fallback allowed)
    let big_log = MAX_TRANSFORM_LOG + 1;
    let sp = plain(stock_root(big_log), big_log, TransformDirection::Forward);
    expect_reject(1usize << big_log, sp, ErrorKind::Unsupported, "too large");
    // a log that does not fit usize
    let mut sp = plain(Fr::ONE, 0, TransformDirection::Forward);
    sp.source_log_size = 64;
    sp.output_log_size = 64;
    expect_reject(1, sp, inv, "log 64");
}

#[test]
fn wire_field_check_identifies_bls12_381_only() {
    assert!(wire_field_supported::<Fr>());
    assert!(!wire_field_supported::<support::Toy>());
    assert!(!wire_field_supported::<midnight_curves::Fp>());
    // N2: same modulus, width and sample encodings, different bijective
    // representation elsewhere: refused by type before any device work.
    use ntt_support::impostor::Impostor;
    for x in [Fr::ZERO, Fr::ONE, Fr::from(u64::MAX) + Fr::ONE, -Fr::ONE] {
        assert_eq!(Impostor(x).to_repr().0.as_slice(), x.to_repr().as_ref(), "samples agree");
    }
    let v = Fr::from(256);
    assert_ne!(Impostor(v).to_repr().0.as_slice(), v.to_repr().as_ref(), "differs elsewhere");
    assert_eq!(Impostor::from_repr(Impostor(v).to_repr()).unwrap(), Impostor(v));
    assert!(!wire_field_supported::<Impostor>());
    let s = Stock::new(1, 3);
    let spec = s.spec_coeff_to_lagrange();
    let spec = TransformSpec {
        root: Impostor(spec.root),
        source_log_size: spec.source_log_size,
        output_log_size: spec.output_log_size,
        direction: spec.direction,
        input_order: spec.input_order,
        output_order: spec.output_order,
        inverse_normalization: spec.inverse_normalization,
        coset: None,
    };
    let rt = GpuRuntime::unavailable();
    let e = transform_host(&rt, &vec![Impostor::ONE; 8], spec).expect_err("refused");
    assert_eq!(e.kind, ErrorKind::Unsupported, "{e:?}");
}

// ---------------------------------------------------------------------------
// Device harness
// ---------------------------------------------------------------------------

static DEVICE: Mutex<()> = Mutex::new(());

fn device_lock() -> MutexGuard<'static, ()> {
    DEVICE.lock().unwrap_or_else(|p| p.into_inner())
}

/// A fresh native runtime (the process-unique driver claim; tests hold
/// `DEVICE`, and a healthy drop frees its resources and releases the claim),
/// or `None` when skipping is explicitly allowed. The trait entry point,
/// which uses the never-dropped process runtime, is tested in
/// `ntt_process_entry.rs`.
fn native() -> Option<GpuRuntime> {
    let rt = GpuRuntime::native(RuntimeConfig::default()).expect("native driver claim");
    match rt.initialize() {
        Ok(_) => Some(rt),
        Err(e) => {
            if std::env::var("LOKI_GPU_ALLOW_NO_DEVICE").as_deref() != Ok("1") {
                panic!("device required (set LOKI_GPU_ALLOW_NO_DEVICE=1 to skip): {e:?}");
            }
            eprintln!("skipping device test: {e:?}");
            None
        }
    }
}

/// Run one host transform and check that exactly one launch happened, that
/// the input slice is unchanged, and that nothing is left resident.
fn gpu(rt: &GpuRuntime, values: &[Fr], spec: TransformSpec<Fr>) -> Vec<Fr> {
    let before = rt.telemetry();
    let copy = values.to_vec();
    let out = transform_host(rt, values, spec).expect("gpu transform");
    let after = rt.telemetry();
    assert_eq!(after.transform_launches, before.transform_launches + 1, "one launch");
    assert_eq!(values, &copy[..], "input slice unchanged");
    let snap = rt.snapshot();
    assert_eq!(snap.live_bytes, Some(0), "no resident leftovers");
    assert_eq!(snap.reserved_bytes, Some(0), "no reservation leftovers");
    out
}

/// GPU `extended_to_lagrange`: inverse coset truncated to n, then forward n,
/// as one device chain with one upload and one download.
fn gpu_e2l(rt: &GpuRuntime, s: &Stock, values: &[Fr]) -> Vec<Fr> {
    let before = rt.telemetry();
    let src = rt
        .upload_scalars_cancellable(values, &Default::default())
        .expect("upload");
    let specs = [s.spec_extended_to_coeff_prefix(s.k), s.spec_coeff_to_lagrange()];
    let out = transform_chain(rt, &src, &specs).expect("chain");
    let host = rt
        .download_scalars_cancellable(&out, &Default::default())
        .expect("download");
    rt_release(rt, src);
    rt_release(rt, out);
    let after = rt.telemetry();
    assert_eq!(after.transform_launches, before.transform_launches + 2);
    let d2h = (after.d2h_pinned_bytes + after.d2h_pageable_bytes)
        - (before.d2h_pinned_bytes + before.d2h_pageable_bytes);
    assert_eq!(d2h, (s.n() * 32) as u64, "only the chain result is read back");
    assert_eq!(rt.snapshot().live_bytes, Some(0));
    host
}

fn rt_release(rt: &GpuRuntime, b: loki_backend_api::ResidentBuffer<Fr>) {
    loki_backend_api::ResidentBackend::<Fr, midnight_curves::G1Projective>::release_scalars(rt, b)
        .expect("release");
}

/// All five stock conversions on one domain, one input pattern.
fn check_domain(rt: &GpuRuntime, s: &Stock, pattern: Pattern, seed: u64, naive: bool) {
    let label = |op: &str| format!("{op} k={} K={} {pattern:?}", s.k, s.extended_k);
    let a = input(pattern, s.n(), seed);
    let e = input(pattern, s.big_n(), seed ^ 0x55);

    let c2l = gpu(rt, &a, s.spec_coeff_to_lagrange());
    assert_vec_eq(&label("coeff_to_lagrange"), &c2l, &s.coeff_to_lagrange(&a));
    let l2c = gpu(rt, &a, s.spec_lagrange_to_coeff());
    assert_vec_eq(&label("lagrange_to_coeff"), &l2c, &s.lagrange_to_coeff(&a));
    let c2e = gpu(rt, &a, s.spec_coeff_to_extended());
    assert_vec_eq(&label("coeff_to_extended"), &c2e, &s.coeff_to_extended(&a));
    let e2c = gpu(rt, &e, s.spec_extended_to_coeff());
    assert_vec_eq(&label("extended_to_coeff"), &e2c, &s.extended_to_coeff(&e));
    let e2l = gpu_e2l(rt, s, &e);
    assert_vec_eq(&label("extended_to_lagrange"), &e2l, &s.extended_to_lagrange(&e));

    if naive {
        assert_vec_eq(&label("naive c2l"), &c2l, &naive_dft(&a, s.omega));
        assert_vec_eq(&label("naive l2c"), &l2c, &naive_idft(&a, s.omega));
        assert_vec_eq(
            &label("naive c2e"),
            &c2e,
            &naive_coset_eval(&a, s.zeta, s.extended_omega, s.big_n()),
        );
    }
}

// ---------------------------------------------------------------------------
// Device differentials
// ---------------------------------------------------------------------------

#[test]
fn small_domains_every_conversion_every_pattern() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    for k in 1..=12u32 {
        for j in [3u32, 5, 9] {
            let s = Stock::new(j, k);
            for pattern in ALL_PATTERNS {
                check_domain(&rt, &s, pattern, 1000 + k as u64, s.big_n() <= 256);
            }
        }
    }
    assert_eq!(rt.snapshot().live_bytes, Some(0));
}

#[test]
fn log_zero_transform_is_identity_with_single_scale() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    for x in [Fr::ZERO, Fr::ONE, max_fr(), Fr::from(7)] {
        let f = gpu(&rt, &[x], plain(Fr::ONE, 0, TransformDirection::Forward));
        assert_eq!(f, vec![x]);
        let i = gpu(&rt, &[x], plain(Fr::ONE, 0, TransformDirection::Inverse));
        assert_eq!(i, vec![x]);
    }
}

#[test]
fn field_boundary_values_are_fully_reduced() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    // Sums that wrap: every element r-1 (sum n(r-1)), alternating 1/r-1,
    // and r-1 with one 1 (sum hits zero exactly).
    let k = 10;
    let s = Stock::new(3, k);
    let n = s.n();
    let cases: Vec<Vec<Fr>> = vec![
        vec![max_fr(); n],
        (0..n).map(|i| if i % 2 == 0 { Fr::ONE } else { max_fr() }).collect(),
        (0..n).map(|i| if i == 0 { Fr::ONE } else { Fr::ZERO }).collect(),
        (0..n).map(|i| if i == n - 1 { max_fr() } else { Fr::ZERO }).collect(),
        {
            let mut v = vec![Fr::ZERO; n];
            v[0] = max_fr();
            v[1] = Fr::ONE;
            v
        },
    ];
    for v in &cases {
        for (spec, want) in [
            (s.spec_coeff_to_lagrange(), s.coeff_to_lagrange(v)),
            (s.spec_lagrange_to_coeff(), s.lagrange_to_coeff(v)),
            (s.spec_coeff_to_extended(), s.coeff_to_extended(v)),
        ] {
            let got = gpu(&rt, v, spec);
            // equality of decoded values already implies canonical bytes,
            // because download decodes with `from_repr` (rejects >= r).
            assert_vec_eq("boundary", &got, &want);
        }
    }
}

#[test]
fn bit_reversed_boundary_orders() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    for k in [1u32, 2, 5, 10, 13] {
        let s = Stock::new(3, k);
        let a = input(Pattern::Random, s.n(), 77 + k as u64);
        let natural = s.coeff_to_lagrange(&a);
        for (i_ord, o_ord) in [
            (ElementOrder::BitReversed, ElementOrder::Natural),
            (ElementOrder::Natural, ElementOrder::BitReversed),
            (ElementOrder::BitReversed, ElementOrder::BitReversed),
        ] {
            let mut spec = s.spec_coeff_to_lagrange();
            spec.input_order = i_ord;
            spec.output_order = o_ord;
            // physical slot p holds logical bitrev(p)
            let phys_in = if i_ord == ElementOrder::BitReversed { bitrev_permute(&a) } else { a.clone() };
            let got = gpu(&rt, &phys_in, spec);
            let want = if o_ord == ElementOrder::BitReversed { bitrev_permute(&natural) } else { natural.clone() };
            assert_vec_eq(&format!("bitrev k={k} {i_ord:?}->{o_ord:?}"), &got, &want);
        }
        // inverse with bit-reversed output
        let mut spec = s.spec_lagrange_to_coeff();
        spec.output_order = ElementOrder::BitReversed;
        let got = gpu(&rt, &natural, spec);
        assert_vec_eq("inverse bitrev out", &got, &bitrev_permute(&a));
    }
}

#[test]
fn inverse_normalization_is_applied_exactly_once() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    for (j, k) in [(3u32, 4u32), (5, 11)] {
        let s = Stock::new(j, k);
        let v = input(Pattern::Random, s.n(), 5);
        let once = gpu(&rt, &v, s.spec_lagrange_to_coeff());
        assert_vec_eq("once == stock", &once, &s.lagrange_to_coeff(&v));
        // An unnormalized inverse is refused before any device work.
        let mut raw = s.spec_lagrange_to_coeff();
        raw.inverse_normalization = InverseNormalization::None;
        let launches = rt.telemetry().transform_launches;
        let e = transform_host(&rt, &v, raw).expect_err("inverse without 1/n");
        assert_eq!((e.kind, e.effect), (ErrorKind::InvalidInput, FailureEffect::NotStarted));
        assert_eq!(rt.telemetry().transform_launches, launches);
        // No hidden scaling: the forward transform of the inverse's output
        // is the input itself (exactly one 1/n was applied).
        assert_vec_eq("forward(inverse(v)) == v", &gpu(&rt, &once, s.spec_coeff_to_lagrange()), &v);
        let n = Fr::from(s.n() as u64);
        // twice-normalized would be once / n: not what we get
        let n_inv = n.invert().unwrap();
        let twice: Vec<Fr> = once.iter().map(|x| *x * n_inv).collect();
        assert_ne!(once, twice);
        // extended inverse coset also scales by exactly 1/N
        let e = input(Pattern::Random, s.big_n(), 6);
        let e2c = gpu(&rt, &e, s.spec_extended_to_coeff());
        assert_vec_eq("e2c once", &e2c, &s.extended_to_coeff(&e));
    }
}

#[test]
fn inverse_coset_prefixes_match_truncated_stock() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    let s = Stock::new(9, 8); // N = 2^11
    let e = input(Pattern::Random, s.big_n(), 9);
    let full = s.extended_to_coeff(&e);
    for out_log in [0u32, 1, 5, s.k, s.extended_k - 1, s.extended_k] {
        let got = gpu(&rt, &e, s.spec_extended_to_coeff_prefix(out_log));
        assert_vec_eq(&format!("prefix {out_log}"), &got, &full[..1 << out_log]);
    }
    // the high coefficients of a genuine degree-<n extension are zero
    let a = input(Pattern::Random, s.n(), 10);
    let ext = s.coeff_to_extended(&a);
    let back = gpu(&rt, &ext, s.spec_extended_to_coeff());
    assert_vec_eq("low", &back[..s.n()], &a);
    assert!(back[s.n()..].iter().all(|x| *x == Fr::ZERO));
}

#[test]
fn multi_wrap_rotations_of_natural_order_outputs() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    let s = Stock::new(5, 9);
    let (n, big) = (s.n() as i64, s.big_n() as i64);
    let stride = big / n;
    let a = input(Pattern::Random, s.n(), 21);
    let lag = gpu(&rt, &a, s.spec_coeff_to_lagrange());
    let ext = gpu(&rt, &a, s.spec_coeff_to_extended());
    for r in [1i64, -1, 3, n - 1, n + 3, -(2 * n + 5), 3 * n - 1, -7 * n] {
        // rotated polynomial p(omega^r X): coefficient i times omega^(r i)
        let w = if r >= 0 {
            s.omega.pow_vartime([r as u64])
        } else {
            s.omega.invert().unwrap().pow_vartime([(-r) as u64])
        };
        let mut rot = a.clone();
        let mut wi = Fr::ONE;
        for c in rot.iter_mut() {
            *c *= wi;
            wi *= w;
        }
        let lag_rot = s.coeff_to_lagrange(&rot);
        let ext_rot = s.coeff_to_extended(&rot);
        for i in 0..n {
            let idx = (i + r).rem_euclid(n) as usize;
            assert_eq!(lag[idx], lag_rot[i as usize], "lagrange r={r} i={i}");
        }
        for i in 0..big {
            let idx = (i + r * stride).rem_euclid(big) as usize;
            assert_eq!(ext[idx], ext_rot[i as usize], "extended r={r} i={i}");
        }
        // stock Lagrange rotation helper agrees (it only accepts |r| < n)
        if r.abs() >= n {
            continue;
        }
        let poly = s.domain.lagrange_from_vec(lag.clone());
        let rotated = poly.rotate(midnight_proofs::poly::Rotation(r as i32));
        for i in 0..n as usize {
            assert_eq!(rotated[i], lag[(i as i64 + r).rem_euclid(n) as usize]);
        }
    }
}

#[test]
fn k19_k20_base_and_extended_domains() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    for (j, k) in [(3u32, 19u32), (5, 19), (3, 20), (5, 20)] {
        let s = Stock::new(j, k);
        for pattern in [Pattern::Random, Pattern::Max] {
            check_domain(&rt, &s, pattern, 4242 + k as u64, false);
        }
    }
}

#[test]
fn rejected_requests_never_launch() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    let s = Stock::new(5, 6);
    let a = input(Pattern::Random, s.n(), 1);
    let before = rt.telemetry();
    let mut bad_root = s.spec_coeff_to_lagrange();
    bad_root.root = stock_root(s.k + 1);
    let mut pad = s.spec_coeff_to_lagrange();
    pad.output_log_size += 1;
    for (len, spec) in [
        (s.n() - 1, s.spec_coeff_to_lagrange()),
        (s.n(), bad_root),
        (s.n(), pad),
    ] {
        let e = transform_host(&rt, &a[..len], spec).unwrap_err();
        assert_eq!((e.kind, e.effect), (ErrorKind::InvalidInput, FailureEffect::NotStarted));
    }
    // unsupported field: Unsupported before any transfer
    let toy = vec![support::Toy::new(1); 4];
    let e = transform_host(
        &rt,
        &toy,
        TransformSpec {
            root: support::Toy::ROOT_OF_UNITY.pow_vartime([1u64 << (support::Toy::S - 2)]),
            source_log_size: 2,
            output_log_size: 2,
            direction: TransformDirection::Forward,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: InverseNormalization::None,
            coset: None,
        },
    )
    .unwrap_err();
    assert_eq!((e.kind, e.effect), (ErrorKind::Unsupported, FailureEffect::NotStarted));
    let after = rt.telemetry();
    assert_eq!(after.transform_launches, before.transform_launches);
    assert_eq!(after.transfers, before.transfers, "no upload happened");
}

// ---------------------------------------------------------------------------
// Negative control: a round trip does not pin the root.
// ---------------------------------------------------------------------------

#[test]
fn wrong_root_round_trip_is_only_a_negative_control() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    let s = Stock::new(3, 10);
    let a = input(Pattern::Random, s.n(), 31);
    // omega^3 also has exact order n (3 is odd), so it passes validation.
    let wrong = s.omega.pow_vartime([3u64]);
    assert!(has_exact_order(wrong, s.k));
    let mut fwd = s.spec_coeff_to_lagrange();
    fwd.root = wrong;
    let mut inv = s.spec_lagrange_to_coeff();
    inv.root = wrong;
    let evals = gpu(&rt, &a, fwd);
    let back = gpu(&rt, &evals, inv);
    // The wrong-root pair round-trips perfectly ...
    assert_vec_eq("wrong-root round trip", &back, &a);
    // ... yet the direct comparison against stock catches it.
    assert!(
        first_mismatch(&evals, &s.coeff_to_lagrange(&a)).is_some(),
        "direct differential must reject a wrong root"
    );
    // and the impulse input pins the root: out[j] = omega^j exactly.
    let imp = input(Pattern::Impulse, s.n(), 0);
    let got = gpu(&rt, &imp, s.spec_coeff_to_lagrange());
    let mut wj = Fr::ONE;
    for x in &got {
        assert_eq!(*x, wj);
        wj *= s.omega;
    }
}

// ---------------------------------------------------------------------------
// Canonical <-> Montgomery boundaries and the forward-root convention
// ---------------------------------------------------------------------------

/// Values at canonical limb boundaries and values whose Montgomery form
/// (`x * 2^256 mod r`) is at a boundary (0, 1, r-1, 2^255 mod r ...).
fn boundary_values() -> Vec<Fr> {
    let two = Fr::from(2);
    let r_inv_mont = two.pow_vartime([256u64]).invert().unwrap(); // R^-1: Montgomery form 1
    let mut v = vec![
        Fr::ZERO,
        Fr::ONE,
        max_fr(),
        max_fr() - Fr::ONE,
        Fr::from(u32::MAX as u64),
        Fr::from(u64::MAX),
        Fr::from(u64::MAX) + Fr::ONE,
        two.pow_vartime([128u64]) - Fr::ONE,
        two.pow_vartime([192u64]),
        two.pow_vartime([254u64]),
        -two.pow_vartime([32u64]),
        r_inv_mont,              // Montgomery limbs = 1
        -r_inv_mont,             // Montgomery limbs = r - 1
        r_inv_mont * Fr::from(2), // Montgomery limbs = 2
        two.pow_vartime([255u64]) * r_inv_mont, // Montgomery limbs = 2^255 mod r
        two.pow_vartime([256u64]),             // canonical R mod r
        two.pow_vartime([512u64]),             // canonical R^2 mod r
        fr_from_hex(PINNED_ROOT_HEX),
    ];
    v.push(<Fr as WithSmallOrderMulGroup<3>>::ZETA);
    v
}

#[test]
fn canonical_montgomery_boundaries_round_trip_exactly() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    // log 0: load (canonical -> Montgomery) then finish (Montgomery ->
    // canonical) with scale one: the exact identity on every value.
    for x in boundary_values() {
        let got = gpu(&rt, &[x], plain(Fr::ONE, 0, TransformDirection::Forward));
        assert_eq!(got, vec![x], "identity on {}", fr_to_hex(&x));
    }
    // and inside real transforms, against stock
    let mut v = boundary_values();
    v.resize(32, Fr::ZERO);
    let s = Stock::new(5, 5);
    assert_vec_eq("c2l boundary", &gpu(&rt, &v, s.spec_coeff_to_lagrange()), &s.coeff_to_lagrange(&v));
    assert_vec_eq("l2c boundary", &gpu(&rt, &v, s.spec_lagrange_to_coeff()), &s.lagrange_to_coeff(&v));
    assert_vec_eq("c2e boundary", &gpu(&rt, &v, s.spec_coeff_to_extended()), &s.coeff_to_extended(&v));
    let mut e = boundary_values();
    e.resize(s.big_n(), max_fr());
    assert_vec_eq("e2c boundary", &gpu(&rt, &e, s.spec_extended_to_coeff()), &s.extended_to_coeff(&e));
}

#[test]
fn spec_root_is_always_the_forward_root() {
    let _g = device_lock();
    let Some(rt) = native() else { return };
    let s = Stock::new(3, 7);
    let v = input(Pattern::Random, s.n(), 13);
    let omega_inv = s.omega.invert().unwrap();
    // Correct convention: inverse spec carries omega; backend inverts once.
    let ok = gpu(&rt, &v, s.spec_lagrange_to_coeff());
    assert_vec_eq("inverse with forward root", &ok, &s.lagrange_to_coeff(&v));
    // A caller that pre-inverts gets (1/n) * DFT(omega): detectably wrong.
    let mut pre = s.spec_lagrange_to_coeff();
    pre.root = omega_inv;
    let wrong = gpu(&rt, &v, pre);
    let n_inv = Fr::from(s.n() as u64).invert().unwrap();
    let want_wrong: Vec<Fr> = naive_dft(&v, s.omega).into_iter().map(|x| x * n_inv).collect();
    assert_vec_eq("pre-inverted root", &wrong, &want_wrong);
    assert!(first_mismatch(&wrong, &ok).is_some());
    // Forward with omega^-1 is the unnormalized inverse, not the forward NTT.
    let mut fwd = s.spec_coeff_to_lagrange();
    fwd.root = omega_inv;
    let back = gpu(&rt, &v, fwd);
    assert_vec_eq("forward with omega^-1", &back, &naive_dft(&v, omega_inv));
    assert!(first_mismatch(&back, &s.coeff_to_lagrange(&v)).is_some());
}
