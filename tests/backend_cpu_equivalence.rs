//! P1 seam gate (`--features loki-backend-api`): the API-enabled CPU path is
//! the stock operation sequence.
//!
//! - proofs routed through the typed dispatcher are byte-identical to the
//!   unmodified crates.io 0.8.3 package (no-lookup and controlled-order lookup)
//!   and the dispatcher really was exercised;
//! - `CpuBackend` MSM / transform results equal stock exactly, including
//!   empty and unequal prefixes, zero/one scalars, roots, cosets and inverse
//!   normalization;
//! - malformed requests are rejected before work starts and resident,
//!   quotient and SRS operations are explicitly `Unsupported`;
//! - the dispatcher falls back to CPU on forced `Unsupported`, sticky-disables
//!   a poisoned accelerator, ignores accelerators that do not advertise the
//!   arithmetic ABI and never calls one for an empty MSM.

#[path = "support/stock_fixture.rs"]
mod fixture;

use std::sync::atomic::{AtomicUsize, Ordering};

use ff::{Field, PrimeField, WithSmallOrderMulGroup};
use fixture::{cross_verify, first_difference, single_thread, NO_LOOKUP, SINGLE_LEFTOVER};
use group::{Curve, Group};
use midnight_curves::{Fq, G1Affine, G1Projective, G2Affine, G2Projective};
use midnight_proofs::backend::{
    api::{
        ArithmeticAbi, Backend, BackendCapabilities, BackendError, CosetSpec, CurveAbi,
        ElementOrder, ErrorKind, FailureEffect, FieldAbi, InverseNormalization, MsmBackend,
        MsmBatchRequest, MsmLengthPolicy, MsmRequest, Operation, QuotientBackend,
        QuotientProgram, ResidentBackend, ResidentBuffer, SrsBasis, SrsCacheKey, SrsRegistry,
        SrsRequest, TransformBackend, TransformDirection, TransformSpec, CURVE_BLS12_381_G1,
        FIELD_BLS12_381_SCALAR, WIRE_ENCODING_V1,
    },
    arithmetic_abi,
    cpu::CpuBackend,
    dispatch::{self, Dispatcher},
    field_abi,
};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

const BLS: ArithmeticAbi = ArithmeticAbi {
    scalar_field: FieldAbi {
        field_id: FIELD_BLS12_381_SCALAR,
        encoding_version: WIRE_ENCODING_V1,
    },
    curve: CurveAbi {
        curve_id: CURVE_BLS12_381_G1,
        encoding_version: WIRE_ENCODING_V1,
    },
};

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

fn naive_msm<G: Group>(coeffs: &[G::Scalar], bases: &[G]) -> G {
    coeffs.iter().zip(bases).fold(G::identity(), |acc, (s, b)| acc + *b * *s)
}

fn plain(root: Fq, log: u32, direction: TransformDirection) -> TransformSpec<Fq> {
    TransformSpec {
        root,
        source_log_size: log,
        output_log_size: log,
        direction,
        input_order: ElementOrder::Natural,
        output_order: ElementOrder::Natural,
        inverse_normalization: match direction {
            TransformDirection::Forward => InverseNormalization::None,
            TransformDirection::Inverse => InverseNormalization::DivideByLengthOnce,
        },
        coset: None,
    }
}

fn zeta_coset() -> CosetSpec<Fq> {
    CosetSpec {
        generator: Fq::ZETA,
        generator_inverse: Fq::ZETA.square(),
    }
}

fn cpu_transform(values: &[Fq], spec: TransformSpec<Fq>) -> Result<Vec<Fq>, BackendError> {
    TransformBackend::<Fq>::transform(&CpuBackend, values, spec)
}

// ---------------------------------------------------------------- proofs

#[test]
fn seam_proofs_equal_stock_and_use_the_dispatcher() {
    let before = dispatch::global().counters();
    for seed in [0x42, 0x07] {
        let (loki, stock) = cross_verify::<NO_LOOKUP>(seed);
        assert_eq!(first_difference(&loki, &stock), None, "no-lookup bytes (seed {seed:#x})");
    }
    let (loki, stock) = cross_verify::<SINGLE_LEFTOVER>(0x42);
    assert_eq!(first_difference(&loki, &stock), None, "controlled-order lookup bytes");
    let after = dispatch::global().counters();
    assert!(after.msm_cpu > before.msm_cpu, "MSM did not route through the dispatcher");
    assert!(
        after.transform_cpu > before.transform_cpu,
        "transforms did not route through the dispatcher"
    );
    // No accelerator advertises support in P1 (loki-gpu is an Unsupported skeleton).
    assert_eq!(after.msm_accelerated, before.msm_accelerated);
    assert_eq!(after.transform_accelerated, before.transform_accelerated);
    assert!(!dispatch::global().accelerator_disabled());
}

// ---------------------------------------------------------------- MSM

#[test]
fn cpu_msm_equals_stock_on_scalar_patterns_and_prefixes() {
    for n in [0usize, 1, 2, 3, 64, 1025] {
        let bases = g1_bases(n + 2, 3);
        let patterns: Vec<(&str, Vec<Fq>)> = vec![
            ("random", scalars(n, 1)),
            ("zero", vec![Fq::ZERO; n]),
            ("one", vec![Fq::ONE; n]),
            ("minus_one", vec![-Fq::ONE; n]),
            ("repeated", vec![Fq::from(0xdead_beef); n]),
            ("boolean", (0..n).map(|i| Fq::from((i % 2) as u64)).collect()),
            ("small", (0..n).map(|i| Fq::from((i % 16) as u64)).collect()),
            ("top_bit", vec![Fq::from(2).pow_vartime([254]); n]),
        ];
        for (name, coeffs) in &patterns {
            for (c, b) in [
                (&coeffs[..], &bases[..n]),
                (&coeffs[..], &bases[..n.saturating_sub(1)]),
                (&coeffs[..n.saturating_sub(2)], &bases[..]),
                (&coeffs[..], &bases[..0]),
            ] {
                let expected = stock::poly::kzg::msm::msm_specific::<G1Affine>(c, b);
                let request = MsmRequest::new(c, b, MsmLengthPolicy::ZipTruncate);
                let got = CpuBackend.msm(request).unwrap();
                assert_eq!(got.to_affine(), expected.to_affine(), "{name} n={n}");
                if c.len().min(b.len()) <= 64 {
                    assert_eq!(got, naive_msm(c, b), "{name} naive n={n}");
                }
                // The routed Loki call site is the same operation.
                let routed = midnight_proofs::poly::kzg::msm::msm_specific::<G1Affine>(c, b);
                assert_eq!(routed.to_affine(), expected.to_affine(), "{name} routed n={n}");
            }
        }
    }
}

#[test]
fn cpu_msm_handles_arbitrary_curves() {
    // G2 is not a native-backend ABI; the CPU adapter still serves it.
    let mut rng = ChaCha20Rng::from_seed([5; 32]);
    let bases: Vec<_> = (0..40).map(|_| G2Projective::random(&mut rng)).collect();
    let coeffs = scalars(39, 6);
    let expected = stock::poly::kzg::msm::msm_specific::<G2Affine>(&coeffs, &bases);
    let got = CpuBackend
        .msm(MsmRequest::new(&coeffs, &bases, MsmLengthPolicy::ZipTruncate))
        .unwrap();
    assert_eq!(got.to_affine(), expected.to_affine());
    assert_eq!(got, naive_msm(&coeffs, &bases));
    assert_eq!(arithmetic_abi::<G2Affine>(), None);
    assert_eq!(arithmetic_abi::<G1Affine>(), Some(BLS));
    assert_eq!(field_abi::<Fq>(), Some(BLS.scalar_field));
}

#[test]
fn msm_require_equal_rejects_mismatch_before_start() {
    let coeffs = scalars(5, 1);
    let bases = g1_bases(4, 2);
    let error = CpuBackend
        .msm(MsmRequest::new(&coeffs, &bases, MsmLengthPolicy::RequireEqual))
        .unwrap_err();
    assert_eq!(
        error,
        BackendError {
            operation: Operation::Msm,
            kind: ErrorKind::InvalidInput,
            effect: FailureEffect::NotStarted,
        }
    );
    let ok = CpuBackend
        .msm(MsmRequest::new(&coeffs[..4], &bases, MsmLengthPolicy::RequireEqual))
        .unwrap();
    assert_eq!(ok, naive_msm(&coeffs[..4], &bases));
}

#[test]
fn msm_batch_preserves_slot_order() {
    let bases = g1_bases(20, 7);
    let a = scalars(20, 1);
    let b = scalars(13, 2);
    let c: Vec<Fq> = Vec::new();
    let batches: Vec<&[Fq]> = vec![&a, &b, &c];
    let results = CpuBackend
        .msm_batch(MsmBatchRequest {
            scalar_batches: &batches,
            bases: &bases,
            length_policy: MsmLengthPolicy::ZipTruncate,
        })
        .unwrap();
    assert_eq!(results, vec![naive_msm(&a, &bases), naive_msm(&b, &bases), G1Projective::identity()]);
}

// ---------------------------------------------------------------- transforms

fn naive_dft(values: &[Fq], root: Fq) -> Vec<Fq> {
    (0..values.len())
        .map(|i| {
            let w = root.pow_vartime([i as u64]);
            values.iter().rev().fold(Fq::ZERO, |acc, v| acc * w + v)
        })
        .collect()
}

#[test]
fn cpu_transforms_equal_stock_domain_conversions() {
    single_thread(|| {
        for (j, k) in [(3u32, 1u32), (3, 4), (4, 5), (5, 8), (7, 10)] {
            let domain = stock::poly::EvaluationDomain::<Fq>::new(j, k);
            let ek = domain.extended_k();
            let (omega, ext) = (domain.get_omega(), domain.get_extended_omega());
            let values = scalars(1 << k, k as u8);

            let forward = cpu_transform(&values, plain(omega, k, TransformDirection::Forward)).unwrap();
            let stock_forward = domain.coeff_to_lagrange(domain.coeff_from_vec(values.clone()));
            assert_eq!(forward[..], stock_forward[..], "forward k={k}");
            if k <= 5 {
                assert_eq!(forward, naive_dft(&values, omega), "naive forward k={k}");
            }

            let inverse = cpu_transform(&values, plain(omega, k, TransformDirection::Inverse)).unwrap();
            let stock_inverse = domain.lagrange_to_coeff(domain.lagrange_from_vec(values.clone()));
            assert_eq!(inverse[..], stock_inverse[..], "inverse k={k}");

            let mut spec = plain(ext, k, TransformDirection::Forward);
            spec.output_log_size = ek;
            spec.coset = Some(zeta_coset());
            let extended = cpu_transform(&values, spec).unwrap();
            let stock_extended = domain.coeff_to_extended(domain.coeff_from_vec(values.clone()));
            assert_eq!(extended[..], stock_extended[..], "coeff_to_extended k={k}");

            let mut spec = plain(ext, ek, TransformDirection::Inverse);
            spec.coset = Some(zeta_coset());
            let back = cpu_transform(&extended, spec).unwrap();
            assert_eq!(back, domain.extended_to_coeff(stock_extended.clone()), "extended_to_coeff k={k}");
            // Exact inverse: coefficients recovered, zero-padded tail.
            assert_eq!(back[..1 << k], values[..]);
            assert!(back[1 << k..].iter().all(|v| v.is_zero_vartime()));

            // extended_to_lagrange = truncated inverse coset, then forward.
            spec.output_log_size = k;
            let truncated = cpu_transform(&extended, spec).unwrap();
            let lagrange = cpu_transform(&truncated, plain(omega, k, TransformDirection::Forward)).unwrap();
            assert_eq!(lagrange[..], domain.extended_to_lagrange(stock_extended)[..], "extended_to_lagrange k={k}");

            // The routed Loki domain (dispatcher CPU path) agrees too.
            let loki = midnight_proofs::poly::EvaluationDomain::<Fq>::new(j, k);
            let routed = loki.coeff_to_extended(loki.coeff_from_vec(values.clone()));
            assert_eq!(routed[..], extended[..], "routed coeff_to_extended k={k}");
            assert_eq!(loki.extended_to_coeff(routed), back, "routed extended_to_coeff k={k}");
        }
    });
}

#[test]
fn cpu_transform_small_domains_and_normalization() {
    // log 0: the only root is one; transforms are the identity.
    let one = [Fq::from(9)];
    assert_eq!(cpu_transform(&one, plain(Fq::ONE, 0, TransformDirection::Forward)).unwrap(), one);
    assert_eq!(cpu_transform(&one, plain(Fq::ONE, 0, TransformDirection::Inverse)).unwrap(), one);
    // log 1: root is -1.
    let two = [Fq::from(3), Fq::from(5)];
    let f = cpu_transform(&two, plain(-Fq::ONE, 1, TransformDirection::Forward)).unwrap();
    assert_eq!(f, vec![Fq::from(8), -Fq::from(2)]);
    // The inverse applies exactly one 1/n; an inverse without it is not an
    // inverse transform and is rejected (API contract).
    let mut spec = plain(-Fq::ONE, 1, TransformDirection::Inverse);
    let normalized = cpu_transform(&f, spec).unwrap();
    assert_eq!(normalized, two);
    spec.inverse_normalization = InverseNormalization::None;
    assert_eq!(cpu_transform(&f, spec).unwrap_err(), invalid(Operation::Transform));
    // A non-cube-root coset generator takes the generic power path.
    let g = Fq::from(7);
    let spec = TransformSpec {
        coset: Some(CosetSpec { generator: g, generator_inverse: g.invert().unwrap() }),
        ..plain(-Fq::ONE, 1, TransformDirection::Forward)
    };
    let scaled = cpu_transform(&two, spec).unwrap();
    assert_eq!(scaled, naive_dft(&[two[0], two[1] * g], -Fq::ONE));
    let back = cpu_transform(&scaled, TransformSpec { direction: TransformDirection::Inverse, inverse_normalization: InverseNormalization::DivideByLengthOnce, ..spec }).unwrap();
    assert_eq!(back, two);
}

fn invalid(operation: Operation) -> BackendError {
    BackendError { operation, kind: ErrorKind::InvalidInput, effect: FailureEffect::NotStarted }
}

#[test]
fn malformed_transform_requests_are_rejected_before_start() {
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 4);
    let omega = domain.get_omega();
    let values = scalars(16, 3);
    let bad = |spec: TransformSpec<Fq>, input: &[Fq]| cpu_transform(input, spec).unwrap_err();
    let fwd = plain(omega, 4, TransformDirection::Forward);

    assert_eq!(bad(fwd, &values[..15]), invalid(Operation::Transform), "short input");
    assert_eq!(bad(TransformSpec { root: omega.square(), ..fwd }, &values), invalid(Operation::Transform), "root order too small");
    assert_eq!(bad(TransformSpec { root: Fq::ONE, ..fwd }, &values), invalid(Operation::Transform), "trivial root");
    assert_eq!(bad(TransformSpec { root: domain.get_extended_omega(), ..fwd }, &values), invalid(Operation::Transform), "root order too large");
    assert_eq!(bad(TransformSpec { output_log_size: 5, ..fwd }, &values), invalid(Operation::Transform), "plain size change");
    assert_eq!(bad(TransformSpec { output_log_size: 3, coset: Some(zeta_coset()), ..fwd }, &values), invalid(Operation::Transform), "forward coset shrink");
    assert_eq!(bad(TransformSpec { inverse_normalization: InverseNormalization::DivideByLengthOnce, ..fwd }, &values), invalid(Operation::Transform), "forward normalization");
    assert_eq!(bad(TransformSpec { inverse_normalization: InverseNormalization::None, ..plain(omega, 4, TransformDirection::Inverse) }, &values), invalid(Operation::Transform), "inverse without normalization");
    let inv = TransformSpec { coset: Some(zeta_coset()), ..plain(omega, 4, TransformDirection::Inverse) };
    assert_eq!(bad(TransformSpec { output_log_size: 5, ..inv }, &values), invalid(Operation::Transform), "inverse coset grow");
    assert_eq!(bad(TransformSpec { coset: Some(CosetSpec { generator: Fq::ZETA, generator_inverse: Fq::ZETA }), ..inv }, &values), invalid(Operation::Transform), "inconsistent coset inverse");
    assert_eq!(bad(TransformSpec { coset: Some(CosetSpec { generator: Fq::ZERO, generator_inverse: Fq::ZERO }), ..inv }, &values), invalid(Operation::Transform), "zero coset generator");
    assert_eq!(bad(TransformSpec { source_log_size: Fq::S + 1, output_log_size: Fq::S + 1, ..fwd }, &values), invalid(Operation::Transform), "beyond two-adicity");
    assert_eq!(bad(TransformSpec { source_log_size: 200, output_log_size: 200, ..fwd }, &values), invalid(Operation::Transform), "shift overflow");
    // Bit-reversed layouts are an explicit CPU-adapter Unsupported.
    let unsupported = BackendError::unsupported(Operation::Transform);
    assert_eq!(bad(TransformSpec { input_order: ElementOrder::BitReversed, ..fwd }, &values), unsupported);
    assert_eq!(bad(TransformSpec { output_order: ElementOrder::BitReversed, ..fwd }, &values), unsupported);
    assert!(unsupported.preserves_observable_state());

    // The dispatcher surfaces CPU validation errors without touching values.
    let dispatcher = Dispatcher::new();
    let mut v = values.clone();
    let err = dispatcher
        .transform::<Fq, CpuBackend>(None, &mut v, TransformSpec { root: Fq::ONE, ..fwd })
        .unwrap_err();
    assert_eq!(err, invalid(Operation::Transform));
    assert_eq!(v, values, "input mutated by a rejected request");
}

#[test]
fn cpu_backend_capabilities_and_unsupported_surfaces() {
    let caps = <CpuBackend as Backend<Fq, G1Projective>>::capabilities(&CpuBackend);
    assert!(caps.host_msm && caps.host_transform);
    assert!(!caps.resident_buffers && !caps.resident_transform);
    assert!(!caps.quotient_interpreter && !caps.srs_registry);
    assert!(caps.supports_arithmetic(BLS));
    assert_eq!(caps.max_transform_log, Some(Fq::S));

    let unsupported = |op| BackendError::unsupported(op);
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::generation(&CpuBackend);
    assert_eq!(r.unwrap_err(), unsupported(Operation::Runtime));
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::runtime_snapshot(&CpuBackend);
    assert_eq!(r.unwrap_err(), unsupported(Operation::Runtime));
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::try_reserve(&CpuBackend, 1);
    assert_eq!(r.err().unwrap(), unsupported(Operation::Reserve));
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::upload_scalars(&CpuBackend, &[Fq::ONE]);
    assert_eq!(r.unwrap_err(), unsupported(Operation::Upload));
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::upload_points(&CpuBackend, &[G1Projective::generator()]);
    assert_eq!(r.unwrap_err(), unsupported(Operation::Upload));

    let generation = midnight_proofs::backend::api::DeviceGeneration { device_ordinal: 0, context_epoch: 0, generation: 0 };
    let forged = || ResidentBuffer::<Fq>::from_untrusted_metadata(generation, midnight_proofs::backend::api::AllocationId(1), 4, 128);
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::download_scalars(&CpuBackend, &forged());
    assert_eq!(r.unwrap_err(), unsupported(Operation::Download));
    let spec = plain(-Fq::ONE, 1, TransformDirection::Forward);
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::transform_resident(&CpuBackend, &forged(), spec);
    assert_eq!(r.unwrap_err(), unsupported(Operation::ResidentTransform));
    let r = <CpuBackend as ResidentBackend<Fq, G1Projective>>::release_scalars(&CpuBackend, forged());
    assert_eq!(r.unwrap_err(), unsupported(Operation::Release));

    let columns = [forged()];
    let program = QuotientProgram {
        rows: 4, domain_log: 2, extended_domain_log: 2, rotation_scale: 1, register_count: 0,
        instructions: &[], constants: &[], columns: &columns, output_register: 0,
    };
    let r = <CpuBackend as QuotientBackend<Fq>>::evaluate_quotient(&CpuBackend, program);
    assert_eq!(r.unwrap_err(), unsupported(Operation::Quotient));

    let key = SrsCacheKey { curve_abi: BLS.curve, basis: SrsBasis::Coefficient, domain_log: 0, point_count: 1, generation, content_identity: [0; 32] };
    let points = [G1Projective::generator()];
    let r = <CpuBackend as SrsRegistry<Fq, G1Projective>>::acquire_srs(&CpuBackend, SrsRequest { key, points: &points });
    assert_eq!(r.err().unwrap(), unsupported(Operation::SrsAcquire));
}

// ---------------------------------------------------------------- dispatcher

/// Accelerator stub: counts calls, then returns `outcome` (or the CPU value).
struct Stub {
    calls: AtomicUsize,
    outcome: Option<BackendError>,
    /// When succeeding, return a transform result of the wrong length.
    short_output: bool,
}

impl Stub {
    fn new(outcome: Option<BackendError>) -> Self {
        Self { calls: AtomicUsize::new(0), outcome, short_output: false }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl MsmBackend<Fq, G1Projective> for Stub {
    fn msm(&self, request: MsmRequest<'_, Fq, G1Projective>) -> Result<G1Projective, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.outcome {
            Some(error) => Err(error),
            None => CpuBackend.msm(request),
        }
    }
}

impl TransformBackend<Fq> for Stub {
    fn transform(&self, values: &[Fq], spec: TransformSpec<Fq>) -> Result<Vec<Fq>, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.outcome {
            Some(error) => Err(error),
            None if self.short_output => Ok(vec![Fq::ONE]),
            None => cpu_transform(values, spec),
        }
    }
}

fn advertised() -> BackendCapabilities {
    BackendCapabilities {
        host_msm: true,
        host_transform: true,
        supported_arithmetic: vec![BLS],
        max_transform_log: Some(20),
        ..BackendCapabilities::default()
    }
}

#[test]
fn forced_unsupported_falls_back_to_cpu() {
    let dispatcher = Dispatcher::new();
    let stub = Stub::new(Some(BackendError::unsupported(Operation::Msm)));
    let caps = advertised();
    let coeffs = scalars(100, 1);
    let bases = g1_bases(90, 2);
    let got = dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases);
    assert_eq!(got, naive_msm(&coeffs, &bases));
    let c = dispatcher.counters();
    assert_eq!((stub.calls(), c.msm_accelerated, c.msm_fallback, c.msm_cpu), (1, 0, 1, 1));
    assert!(!dispatcher.accelerator_disabled(), "Unsupported must not poison");

    let stub = Stub::new(Some(BackendError::unsupported(Operation::Transform)));
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 6);
    let values = scalars(64, 4);
    let mut v = values.clone();
    dispatcher
        .transform(Some((&stub, &caps)), &mut v, plain(domain.get_omega(), 6, TransformDirection::Forward))
        .unwrap();
    assert_eq!(v[..], domain.coeff_to_lagrange(domain.coeff_from_vec(values))[..]);
    let c = dispatcher.counters();
    assert_eq!((stub.calls(), c.transform_accelerated, c.transform_fallback, c.transform_cpu), (1, 0, 1, 1));
}

#[test]
fn successful_accelerator_result_is_published() {
    let dispatcher = Dispatcher::new();
    let stub = Stub::new(None);
    let caps = advertised();
    let coeffs = scalars(50, 1);
    let bases = g1_bases(50, 2);
    assert_eq!(dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases), naive_msm(&coeffs, &bases));
    let c = dispatcher.counters();
    assert_eq!((stub.calls(), c.msm_accelerated, c.msm_fallback, c.msm_cpu), (1, 1, 0, 0));
}

#[test]
fn poisoned_accelerator_is_sticky_disabled() {
    let dispatcher = Dispatcher::new();
    let poison = BackendError { operation: Operation::Msm, kind: ErrorKind::ContextLost, effect: FailureEffect::SessionPoisoned };
    let stub = Stub::new(Some(poison));
    let caps = advertised();
    let coeffs = scalars(10, 1);
    let bases = g1_bases(10, 2);
    for _ in 0..3 {
        assert_eq!(dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases), naive_msm(&coeffs, &bases));
    }
    assert_eq!(stub.calls(), 1, "poisoned accelerator was called again");
    assert!(dispatcher.accelerator_disabled());
    let c = dispatcher.counters();
    assert_eq!((c.msm_fallback, c.msm_cpu, c.accelerator_poisoned), (1, 3, 1));
}

#[test]
fn wrong_length_accelerator_output_is_rejected() {
    let dispatcher = Dispatcher::new();
    let mut stub = Stub::new(None);
    stub.short_output = true;
    let caps = advertised();
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 5);
    let values = scalars(32, 4);
    let mut v = values.clone();
    dispatcher
        .transform(Some((&stub, &caps)), &mut v, plain(domain.get_omega(), 5, TransformDirection::Forward))
        .unwrap();
    assert_eq!(v[..], domain.coeff_to_lagrange(domain.coeff_from_vec(values))[..]);
    assert!(dispatcher.accelerator_disabled(), "contract violation must disable the accelerator");
    assert_eq!(dispatcher.counters().transform_accelerated, 0);
}

#[test]
fn accelerator_skipped_without_capability_or_for_empty_msm() {
    let dispatcher = Dispatcher::new();
    let stub = Stub::new(None);
    let coeffs = scalars(10, 1);
    let bases = g1_bases(10, 2);
    // No arithmetic ABI advertised.
    let caps = BackendCapabilities { host_msm: true, host_transform: true, max_transform_log: Some(20), ..BackendCapabilities::default() };
    dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases);
    // Capability flag off.
    let caps = BackendCapabilities { host_msm: false, ..advertised() };
    dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases);
    // Empty effective prefix: stock identity without entering the backend.
    let caps = advertised();
    assert_eq!(dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases[..0]), G1Projective::identity());
    assert_eq!(dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &[], &bases), G1Projective::identity());
    // Transform beyond advertised max log.
    let caps = BackendCapabilities { max_transform_log: Some(3), ..advertised() };
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 4);
    let mut v = scalars(16, 3);
    dispatcher.transform(Some((&stub, &caps)), &mut v, plain(domain.get_omega(), 4, TransformDirection::Forward)).unwrap();
    assert_eq!(stub.calls(), 0);
    let c = dispatcher.counters();
    assert_eq!((c.msm_cpu, c.msm_fallback, c.transform_cpu, c.transform_fallback), (4, 0, 1, 0));
}

// ------------------------------------------------ P4 transform dispatch fixes

fn coset_spec(domain: &stock::poly::EvaluationDomain<Fq>, direction: TransformDirection) -> TransformSpec<Fq> {
    let (k, ek) = (domain.k(), domain.extended_k());
    TransformSpec {
        root: domain.get_extended_omega(),
        source_log_size: if direction == TransformDirection::Forward { k } else { ek },
        output_log_size: ek,
        direction,
        input_order: ElementOrder::Natural,
        output_order: ElementOrder::Natural,
        inverse_normalization: match direction {
            TransformDirection::Forward => InverseNormalization::None,
            TransformDirection::Inverse => InverseNormalization::DivideByLengthOnce,
        },
        coset: Some(zeta_coset()),
    }
}

#[test]
fn cancelled_transform_is_returned_without_cpu_retry() {
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 5);
    let values = scalars(32, 6);
    let spec = plain(domain.get_omega(), 5, TransformDirection::Forward);
    let caps = advertised();
    for (effect, disables) in [
        (FailureEffect::NotStarted, false),
        (FailureEffect::InputsPreserved, false),
        (FailureEffect::SessionPoisoned, true),
    ] {
        let dispatcher = Dispatcher::new();
        let cancelled = BackendError { operation: Operation::Transform, kind: ErrorKind::Cancelled, effect };
        let stub = Stub::new(Some(cancelled));
        let mut v = values.clone();
        let err = dispatcher.transform(Some((&stub, &caps)), &mut v, spec).unwrap_err();
        assert_eq!(err, cancelled);
        assert_eq!(v, values, "host values changed by a cancelled transform");
        let c = dispatcher.counters();
        assert_eq!((stub.calls(), c.transform_cpu, c.transform_fallback, c.transform_accelerated), (1, 0, 0, 0));
        assert_eq!(dispatcher.accelerator_disabled(), disables, "{effect:?}");
        // Coset conversions propagate the same way.
        let stub = Stub::new(Some(cancelled));
        let d2 = Dispatcher::new();
        let e = d2.coset_transform(Some((&stub, &caps)), &values, coset_spec(&domain, TransformDirection::Forward));
        assert_eq!(e.unwrap_err(), cancelled);
        assert_eq!(d2.counters().transform_cpu, 0);
    }
}

/// Accelerator that, while computing, lets another dispatcher call poison the
/// accelerator, then returns a well-formed (but now untrusted) result.
struct LatePoison<'a> {
    dispatcher: &'a Dispatcher,
    calls: AtomicUsize,
}

impl TransformBackend<Fq> for LatePoison<'_> {
    fn transform(&self, values: &[Fq], spec: TransformSpec<Fq>) -> Result<Vec<Fq>, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let poison = BackendError { operation: Operation::Msm, kind: ErrorKind::ContextLost, effect: FailureEffect::SessionPoisoned };
        let concurrent = Stub::new(Some(poison));
        let caps = advertised();
        self.dispatcher.msm::<G1Affine, _>(Some((&concurrent, &caps)), &scalars(4, 1), &g1_bases(4, 2));
        cpu_transform(values, spec)
    }
}

#[test]
fn transform_result_arriving_after_sticky_disable_is_discarded() {
    let dispatcher = Dispatcher::new();
    let late = LatePoison { dispatcher: &dispatcher, calls: AtomicUsize::new(0) };
    let caps = advertised();
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 5);
    let values = scalars(32, 7);
    let mut v = values.clone();
    dispatcher.transform(Some((&late, &caps)), &mut v, plain(domain.get_omega(), 5, TransformDirection::Forward)).unwrap();
    assert_eq!(v[..], domain.coeff_to_lagrange(domain.coeff_from_vec(values))[..]);
    let c = dispatcher.counters();
    assert!(dispatcher.accelerator_disabled());
    assert_eq!((late.calls.load(Ordering::SeqCst), c.transform_accelerated, c.transform_fallback, c.transform_cpu), (1, 0, 1, 1));
}

#[test]
fn transform_admission_requires_the_exact_arithmetic_abi() {
    let dispatcher = Dispatcher::new();
    let stub = Stub::new(None);
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 4);
    let spec = plain(domain.get_omega(), 4, TransformDirection::Forward);
    // Same scalar field, different curve: not the ABI the caps were asked for.
    let other_curve = ArithmeticAbi { curve: CurveAbi { curve_id: CURVE_BLS12_381_G1 + 100, ..BLS.curve }, ..BLS };
    let other_encoding = ArithmeticAbi { scalar_field: FieldAbi { encoding_version: WIRE_ENCODING_V1 + 1, ..BLS.scalar_field }, ..BLS };
    for abi in [other_curve, other_encoding] {
        let caps = BackendCapabilities { supported_arithmetic: vec![abi], ..advertised() };
        let mut v = scalars(16, 3);
        dispatcher.transform(Some((&stub, &caps)), &mut v, spec).unwrap();
        assert_eq!(dispatcher.coset_transform(Some((&stub, &caps)), &scalars(16, 3), coset_spec(&domain, TransformDirection::Forward)).unwrap(), None);
    }
    assert_eq!(stub.calls(), 0);
    let mut v = scalars(16, 3);
    dispatcher.transform(Some((&stub, &advertised())), &mut v, spec).unwrap();
    assert_eq!(stub.calls(), 1, "exact ABI admitted");
}

#[test]
fn failed_coset_offer_is_settled_once_on_cpu() {
    let domain = stock::poly::EvaluationDomain::<Fq>::new(3, 5);
    let caps = advertised();
    let coeffs = scalars(32, 8);
    let ext = domain.coeff_to_extended(domain.coeff_from_vec(coeffs.clone()));
    let failure = BackendError { operation: Operation::Transform, kind: ErrorKind::Kernel, effect: FailureEffect::InputsPreserved };
    // Forward: one offer, one CPU computation of the whole conversion.
    let dispatcher = Dispatcher::new();
    let stub = Stub::new(Some(failure));
    let got = dispatcher.coset_transform(Some((&stub, &caps)), &coeffs, coset_spec(&domain, TransformDirection::Forward)).unwrap().expect("settled");
    assert_eq!(got[..], ext[..]);
    let c = dispatcher.counters();
    assert_eq!((stub.calls(), c.transform_fallback, c.transform_cpu, c.transform_accelerated), (1, 1, 1, 0));
    // Inverse.
    let back = dispatcher.coset_transform(Some((&stub, &caps)), &ext[..], coset_spec(&domain, TransformDirection::Inverse)).unwrap().expect("settled");
    assert_eq!(back, domain.extended_to_coeff(ext.clone()));
    assert_eq!(stub.calls(), 2);
    // Success publishes the accelerator result; not admitted returns None.
    let ok = Stub::new(None);
    assert_eq!(dispatcher.coset_transform(Some((&ok, &caps)), &coeffs, coset_spec(&domain, TransformDirection::Forward)).unwrap().unwrap()[..], ext[..]);
    let none = BackendCapabilities { host_transform: false, ..advertised() };
    assert_eq!(dispatcher.coset_transform(Some((&ok, &none)), &coeffs, coset_spec(&domain, TransformDirection::Forward)).unwrap(), None);
    assert_eq!(ok.calls(), 1);
}

/// quint-q2 B1 (astra-gpu decision): `try_msm` returns an accelerator
/// `Cancelled` without CPU recomputation or CPU/fallback accounting,
/// sticky-disables only for a poisoning effect, and the non-cancellable
/// value shim unwinds instead of recomputing. Fresh dispatcher per case.
#[test]
fn cancelled_msm_is_returned_not_recomputed() {
    let coeffs = scalars(64, 7);
    let bases = g1_bases(64, 8);
    let caps = advertised();
    for (effect, sticky) in [
        (FailureEffect::NotStarted, false),
        (FailureEffect::InputsPreserved, false),
        (FailureEffect::SessionPoisoned, true),
    ] {
        let cancelled = BackendError {
            operation: Operation::Msm,
            kind: ErrorKind::Cancelled,
            effect,
        };
        let dispatcher = Dispatcher::new();
        let stub = Stub::new(Some(cancelled));
        let got = dispatcher.try_msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases);
        assert_eq!(got.unwrap_err(), cancelled);
        let c = dispatcher.counters();
        assert_eq!((stub.calls(), c.msm_accelerated, c.msm_fallback, c.msm_cpu), (1, 0, 0, 0), "{effect:?}");
        assert_eq!(dispatcher.accelerator_disabled(), sticky, "{effect:?}");
        assert_eq!(c.accelerator_poisoned, u64::from(sticky));

        let dispatcher = Dispatcher::new();
        let stub = Stub::new(Some(cancelled));
        let shim = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatcher.msm::<G1Affine, _>(Some((&stub, &caps)), &coeffs, &bases)
        }));
        assert!(shim.is_err(), "{effect:?}: the value shim must not recompute cancelled work");
        let c = dispatcher.counters();
        assert_eq!((stub.calls(), c.msm_fallback, c.msm_cpu), (1, 0, 0), "{effect:?}");
    }
}
