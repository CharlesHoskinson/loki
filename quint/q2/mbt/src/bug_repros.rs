//! Deterministic reproductions of quint-q2 findings against the real
//! `Dispatcher` (src/backend/dispatch.rs @ a256a9a). Each test asserts the
//! *current* behaviour and passes while the defect is present; the assertion
//! that would hold under the contract is stated next to it.
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;

use ff::{Field, PrimeField};
use group::Group;
use midnight_curves::{Fq, G1Affine, G1Projective};
use midnight_proofs::backend::{
    api::{
        ArithmeticAbi, BackendCapabilities, BackendError, CurveAbi, ElementOrder, ErrorKind,
        FailureEffect, FieldAbi, InverseNormalization, MsmBackend, MsmRequest, Operation,
        TransformBackend, TransformDirection, TransformSpec, CURVE_BLS12_381_G1,
        FIELD_BLS12_381_SCALAR, WIRE_ENCODING_V1,
    },
    dispatch::Dispatcher,
};

fn caps() -> BackendCapabilities {
    BackendCapabilities {
        host_msm: true,
        host_transform: true,
        supported_arithmetic: vec![ArithmeticAbi {
            scalar_field: FieldAbi { field_id: FIELD_BLS12_381_SCALAR, encoding_version: WIRE_ENCODING_V1 },
            curve: CurveAbi { curve_id: CURVE_BLS12_381_G1, encoding_version: WIRE_ENCODING_V1 },
        }],
        max_transform_log: Some(8),
        ..BackendCapabilities::default()
    }
}

fn inputs() -> (Vec<Fq>, Vec<G1Projective>, G1Projective) {
    let s: Vec<Fq> = (1..=3u64).map(Fq::from).collect();
    let b: Vec<G1Projective> = (1..=3u64).map(|k| G1Projective::generator() * Fq::from(k)).collect();
    let expect = G1Projective::generator() * Fq::from(1 + 4 + 9);
    (s, b, expect)
}

/// Backend that reports a caller cancellation for every call.
struct Cancelling;
impl MsmBackend<Fq, G1Projective> for Cancelling {
    fn msm(&self, _: MsmRequest<'_, Fq, G1Projective>) -> Result<G1Projective, BackendError> {
        Err(BackendError { operation: Operation::Msm, kind: ErrorKind::Cancelled, effect: FailureEffect::NotStarted })
    }
}
impl TransformBackend<Fq> for Cancelling {
    fn transform(&self, _: &[Fq], _: TransformSpec<Fq>) -> Result<Vec<Fq>, BackendError> {
        Err(BackendError {
            operation: Operation::Transform,
            kind: ErrorKind::Cancelled,
            effect: FailureEffect::InputsPreserved,
        })
    }
}

/// B1: `ErrorKind::Cancelled` "does not request automatic CPU retry of
/// cancelled work" (loki-backend-api, loki-gpu cefecbc; design/BUILD.md:191),
/// yet Dispatcher::failed (dispatch.rs:115-127) accounts it as a fallback and
/// msm()/transform() recompute on CPU. msm() cannot even report cancellation:
/// it returns a bare point.
#[test]
fn b1_cancelled_msm_is_recomputed_on_cpu() {
    let d = Dispatcher::new();
    let caps = caps();
    let (s, b, expect) = inputs();
    let got = d.msm::<G1Affine, Cancelling>(Some((&Cancelling, &caps)), &s, &b);
    let c = d.counters();
    // Current behaviour: the cancelled request ran on CPU and its result was published.
    assert!(got == expect);
    assert_eq!((c.msm_fallback, c.msm_cpu, c.msm_accelerated), (1, 1, 0));
    assert!(!d.accelerator_disabled());
    // Contract: msm_cpu == 0 and the caller observes Cancelled.
}

#[test]
fn b1_cancelled_transform_is_recomputed_on_cpu() {
    let d = Dispatcher::new();
    let caps = caps();
    let mut root = Fq::ROOT_OF_UNITY;
    for _ in 2..Fq::S {
        root = root.square();
    }
    let spec = TransformSpec {
        root,
        source_log_size: 2,
        output_log_size: 2,
        direction: TransformDirection::Forward,
        input_order: ElementOrder::Natural,
        output_order: ElementOrder::Natural,
        inverse_normalization: InverseNormalization::None,
        coset: None,
    };
    let mut v = vec![Fq::ONE, Fq::ZERO, Fq::ZERO, Fq::ZERO];
    let r = d.transform::<Fq, Cancelling>(Some((&Cancelling, &caps)), &mut v, spec);
    // Current behaviour: Ok(()) with the CPU result (delta -> all ones).
    assert!(r.is_ok());
    assert_eq!(v, vec![Fq::ONE; 4]);
    let c = d.counters();
    assert_eq!((c.transform_fallback, c.transform_cpu), (1, 1));
    // Contract: Err(kind == Cancelled) and transform_cpu == 0.
}

/// Backend that blocks until told how to return.
struct Scripted {
    entered: Sender<()>,
    outcome: Mutex<Receiver<Result<(), ErrorKind>>>,
}
impl MsmBackend<Fq, G1Projective> for Scripted {
    fn msm(&self, r: MsmRequest<'_, Fq, G1Projective>) -> Result<G1Projective, BackendError> {
        self.entered.send(()).unwrap();
        match self.outcome.lock().unwrap().recv().unwrap() {
            Ok(()) => Ok(r.scalars.iter().zip(r.bases).fold(G1Projective::identity(), |a, (s, b)| a + *b * *s)),
            Err(kind) => Err(BackendError { operation: Operation::Msm, kind, effect: FailureEffect::InputsPreserved }),
        }
    }
}

/// B3: an in-flight accelerator result is published (msm_accelerated bumped)
/// after a concurrent call has sticky-disabled the accelerator with
/// ContextLost. The gate (dispatch.rs:143-146) is evaluated once before the
/// call (148) and nothing re-checks `disabled` before publication (149-152).
#[test]
fn b3_result_published_after_concurrent_poison() {
    let d = Dispatcher::new();
    let caps = caps();
    let (s, b, expect) = inputs();
    let (ea, a_entered) = channel();
    let (eb, b_entered) = channel();
    let (a_go, ra) = channel();
    let (b_go, rb) = channel();
    let fa = Scripted { entered: ea, outcome: Mutex::new(ra) };
    let fb = Scripted { entered: eb, outcome: Mutex::new(rb) };
    std::thread::scope(|scope| {
        let a = scope.spawn(|| d.msm::<G1Affine, Scripted>(Some((&fa, &caps)), &s, &b));
        a_entered.recv().unwrap();
        let bt = scope.spawn(|| d.msm::<G1Affine, Scripted>(Some((&fb, &caps)), &s, &b));
        b_entered.recv().unwrap();
        // B fails with a lost context and poisons the dispatcher.
        b_go.send(Err(ErrorKind::ContextLost)).unwrap();
        assert!(bt.join().unwrap() == expect);
        assert!(d.accelerator_disabled());
        let before = d.counters();
        assert_eq!((before.msm_accelerated, before.accelerator_poisoned), (0, 1));
        // A's call on the same (now lost) context completes Ok afterwards.
        a_go.send(Ok(())).unwrap();
        assert!(a.join().unwrap() == expect);
    });
    let c = d.counters();
    // Current behaviour: an accelerated result was published after disable.
    assert_eq!(c.msm_accelerated, 1);
    assert!(d.accelerator_disabled());
    // A contract that forbids publication after disable would need a
    // re-check before publication (msm_accelerated == 0, msm_cpu == 2).
}

/// Control: `preserves_observable_state` is true for every FailureEffect, so
/// the assert in Dispatcher::failed (dispatch.rs:116-119) is unreachable.
#[test]
fn preserves_observable_state_is_a_tautology() {
    for effect in [FailureEffect::NotStarted, FailureEffect::InputsPreserved, FailureEffect::SessionPoisoned] {
        assert!(BackendError { operation: Operation::Msm, kind: ErrorKind::Internal, effect }.preserves_observable_state());
    }
}
