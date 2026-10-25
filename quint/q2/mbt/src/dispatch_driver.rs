//! quint-connect driver: quint/q2/dispatch.qnt (`mbtStep`) against the real
//! `midnight_proofs::backend::dispatch::Dispatcher` (src/backend/dispatch.rs).
//!
//! Each model thread is a real OS thread calling the real dispatcher with a
//! scripted fake accelerator. The fake blocks inside `msm`/`transform` until
//! the trace says how the backend call returns, so model interleavings of two
//! in-flight requests are executed for real. Every published value is
//! compared against an independent naive MSM / DFT oracle.
use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

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
    dispatch::{DispatchCounters, Dispatcher},
};
use quint_connect::{Driver, Result, State, Step};
use serde::{Deserialize, Deserializer};

// ------------------------------------------------------------------ model types

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
pub enum Op {
    OMsm,
    OTransform,
    OCoset,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
pub enum Kind {
    Unsupported,
    InvalidInput,
    Capacity,
    Allocation,
    Transfer,
    Kernel,
    Synchronization,
    ContextLost,
    Poisoned,
    Internal,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
pub enum Effect {
    NotStarted,
    InputsPreserved,
    SessionPoisoned,
}

/// Observable phase. The model's Admitted (gate passed) and InAccel (inside
/// the backend) are one observable state: the caller thread is in the fake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Busy,
    Done,
}

impl<'de> Deserialize<'de> for Phase {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Tagged {
            tag: String,
        }
        let t = Tagged::deserialize(d)?;
        Ok(match t.tag.as_str() {
            "Idle" => Phase::Idle,
            "Admitted" | "InAccel" => Phase::Busy,
            "Done" => Phase::Done,
            other => return Err(serde::de::Error::custom(format!("phase {other}"))),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
pub enum Src {
    SNone,
    SCpu,
    SAccel,
    SReject,
    SCancel,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReqView {
    phase: Phase,
    result: i64,
    source: Src,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    msm_cpu: i64,
    msm_acc: i64,
    msm_fb: i64,
    tr_cpu: i64,
    tr_acc: i64,
    tr_fb: i64,
    poisoned: i64,
}

impl From<DispatchCounters> for Counters {
    fn from(c: DispatchCounters) -> Self {
        Self {
            msm_cpu: c.msm_cpu as i64,
            msm_acc: c.msm_accelerated as i64,
            msm_fb: c.msm_fallback as i64,
            tr_cpu: c.transform_cpu as i64,
            tr_acc: c.transform_accelerated as i64,
            tr_fb: c.transform_fallback as i64,
            poisoned: c.accelerator_poisoned as i64,
        }
    }
}

/// Projection of the model state that the real code determines.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    disabled: bool,
    ctr: Counters,
    reqs: BTreeMap<i64, ReqView>,
}

// ------------------------------------------------------------------ fake backend

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Ok,
    WrongLen,
    Err(ErrorKind, FailureEffect),
}

enum Event {
    Entered(i64),
    Finished(i64),
}

struct Fake {
    t: i64,
    events: Sender<Event>,
    script: Mutex<Receiver<Outcome>>,
}

impl Fake {
    fn wait(&self) -> Outcome {
        self.events.send(Event::Entered(self.t)).unwrap();
        self.script.lock().unwrap().recv().expect("trace scripted the backend outcome")
    }
}

fn naive_msm(scalars: &[Fq], bases: &[G1Projective]) -> G1Projective {
    scalars
        .iter()
        .zip(bases)
        .fold(G1Projective::identity(), |acc, (s, b)| acc + *b * *s)
}

/// Independent O(n^2) DFT with the API's inverse convention.
fn naive_transform(values: &[Fq], spec: &TransformSpec<Fq>) -> Vec<Fq> {
    let n = values.len();
    let w = match spec.direction {
        TransformDirection::Forward => spec.root,
        TransformDirection::Inverse => spec.root.invert().unwrap(),
    };
    let mut out = vec![Fq::ZERO; n];
    for (k, o) in out.iter_mut().enumerate() {
        let wk = w.pow_vartime([k as u64]);
        let mut p = Fq::ONE;
        for v in values {
            *o += *v * p;
            p *= wk;
        }
    }
    if spec.direction == TransformDirection::Inverse
        && spec.inverse_normalization == InverseNormalization::DivideByLengthOnce
    {
        let inv = Fq::from(n as u64).invert().unwrap();
        out.iter_mut().for_each(|o| *o *= inv);
    }
    out
}

fn api_error(op: Operation, kind: ErrorKind, effect: FailureEffect) -> BackendError {
    BackendError { operation: op, kind, effect }
}

impl MsmBackend<Fq, G1Projective> for Fake {
    fn msm(&self, request: MsmRequest<'_, Fq, G1Projective>) -> std::result::Result<G1Projective, BackendError> {
        match self.wait() {
            Outcome::Ok => {
                let n = request.pair_count()?;
                Ok(naive_msm(&request.scalars[..n], &request.bases[..n]))
            }
            Outcome::WrongLen => unreachable!("MSM has no wrong-length outcome"),
            Outcome::Err(k, e) => Err(api_error(Operation::Msm, k, e)),
        }
    }
}

impl TransformBackend<Fq> for Fake {
    fn transform(&self, values: &[Fq], spec: TransformSpec<Fq>) -> std::result::Result<Vec<Fq>, BackendError> {
        match self.wait() {
            Outcome::Ok => Ok(naive_transform(values, &spec)),
            Outcome::WrongLen => Ok(vec![Fq::from(99); values.len() + 1]),
            Outcome::Err(k, e) => Err(api_error(Operation::Transform, k, e)),
        }
    }
}

// ------------------------------------------------------------------ concrete requests

const LOG_N: u32 = 2;

fn root_of_order(log: u32) -> Fq {
    let mut r = Fq::ROOT_OF_UNITY;
    for _ in log..Fq::S {
        r = r.square();
    }
    r
}

fn abi_g1() -> ArithmeticAbi {
    ArithmeticAbi {
        scalar_field: FieldAbi { field_id: FIELD_BLS12_381_SCALAR, encoding_version: WIRE_ENCODING_V1 },
        curve: CurveAbi { curve_id: CURVE_BLS12_381_G1, encoding_version: WIRE_ENCODING_V1 },
    }
}

/// A different curve ABI over the same field (the transform gate only checks
/// the field, the MSM gate the exact pair).
fn abi_other() -> ArithmeticAbi {
    ArithmeticAbi {
        scalar_field: FieldAbi { field_id: 77, encoding_version: WIRE_ENCODING_V1 },
        curve: CurveAbi { curve_id: 77, encoding_version: WIRE_ENCODING_V1 },
    }
}

fn caps(op: Op, abi_ok: bool, cap_ok: bool, size_ok: bool) -> BackendCapabilities {
    BackendCapabilities {
        host_msm: op == Op::OMsm && cap_ok,
        host_transform: op != Op::OMsm && cap_ok,
        supported_arithmetic: vec![if abi_ok { abi_g1() } else { abi_other() }],
        max_transform_log: Some(if size_ok { LOG_N } else { LOG_N - 1 }),
        ..BackendCapabilities::default()
    }
}

/// MSM inputs with an effective prefix of `pairs`: unequal slice lengths
/// exercise ZipTruncate, and input 2 places a zero scalar first.
fn msm_inputs(pairs: usize, input: i64) -> (Vec<Fq>, Vec<G1Projective>) {
    let extra_s = (input % 2) as usize;
    let scalars = (0..pairs + extra_s)
        .map(|k| if input == 2 && k == 0 { Fq::ZERO } else { Fq::from(input as u64 + k as u64) })
        .collect();
    let bases = (0..pairs + 1 - extra_s)
        .map(|k| G1Projective::generator() * Fq::from(k as u64 + 1))
        .collect();
    (scalars, bases)
}

fn transform_inputs(valid: bool, input: i64) -> (Vec<Fq>, TransformSpec<Fq>) {
    let direction = if input == 3 { TransformDirection::Inverse } else { TransformDirection::Forward };
    let spec = TransformSpec {
        root: if valid { root_of_order(LOG_N) } else { Fq::ONE },
        source_log_size: LOG_N,
        output_log_size: LOG_N,
        direction,
        input_order: ElementOrder::Natural,
        output_order: ElementOrder::Natural,
        inverse_normalization: match direction {
            TransformDirection::Forward => InverseNormalization::None,
            TransformDirection::Inverse => InverseNormalization::DivideByLengthOnce,
        },
        coset: None,
    };
    let values = (0..1u64 << LOG_N).map(|k| Fq::from(input as u64 * 5 + k * k + 1)).collect();
    (values, spec)
}

enum Answer {
    Msm(G1Projective),
    Transform(std::result::Result<Vec<Fq>, BackendError>),
}

struct Inflight {
    op: Op,
    pairs: usize,
    input: i64,
    valid: bool,
    handle: JoinHandle<Answer>,
    script: Sender<Outcome>,
    before: DispatchCounters,
}

pub struct DispatchDriver {
    dispatcher: Arc<Dispatcher>,
    events: Receiver<Event>,
    events_tx: Sender<Event>,
    inflight: BTreeMap<i64, Inflight>,
    views: BTreeMap<i64, ReqView>,
}

impl Default for DispatchDriver {
    fn default() -> Self {
        let (events_tx, events) = channel();
        Self {
            dispatcher: Arc::new(Dispatcher::new()),
            events,
            events_tx,
            inflight: BTreeMap::new(),
            views: BTreeMap::new(),
        }
    }
}

impl State<DispatchDriver> for Snapshot {
    fn from_driver(d: &DispatchDriver) -> Result<Self> {
        Ok(Snapshot {
            disabled: d.dispatcher.accelerator_disabled(),
            ctr: d.dispatcher.counters().into(),
            reqs: d.views.clone(),
        })
    }
}

/// Abstract model value: 10 * input + opcode, identity 0, WRONG -99.
fn abstract_value(op: Op, input: i64, pairs: usize) -> i64 {
    if op == Op::OMsm && pairs == 0 {
        0
    } else {
        10 * input + if op == Op::OMsm { 1 } else { 2 }
    }
}

impl DispatchDriver {
    fn idle() -> ReqView {
        ReqView { phase: Phase::Idle, result: 0, source: Src::SNone }
    }

    fn admit(&mut self, t: i64, op: Op, pairs: usize, abi_ok: bool, cap_ok: bool, size_ok: bool, valid: bool, input: i64) {
        assert_ne!(op, Op::OCoset, "mbtStep excludes the crate-private coset hook");
        let (script, script_rx) = channel();
        let fake = Fake { t, events: self.events_tx.clone(), script: Mutex::new(script_rx) };
        let caps = caps(op, abi_ok, cap_ok, size_ok);
        let d = Arc::clone(&self.dispatcher);
        let done = self.events_tx.clone();
        let before = d.counters();
        let handle = std::thread::spawn(move || {
            let answer = if op == Op::OMsm {
                let (s, b) = msm_inputs(pairs, input);
                Answer::Msm(d.msm::<G1Affine, Fake>(Some((&fake, &caps)), &s, &b))
            } else {
                let (mut v, spec) = transform_inputs(valid, input);
                let r = d.transform::<Fq, Fake>(Some((&fake, &caps)), &mut v, spec);
                Answer::Transform(r.map(|()| v))
            };
            done.send(Event::Finished(t)).unwrap();
            answer
        });
        self.inflight.insert(t, Inflight { op, pairs, input, valid, handle, script, before });
        match self.events.recv().unwrap() {
            Event::Entered(u) => {
                assert_eq!(u, t);
                self.views.insert(t, ReqView { phase: Phase::Busy, result: 0, source: Src::SNone });
            }
            Event::Finished(u) => {
                assert_eq!(u, t);
                self.collect(t);
            }
        }
    }

    fn resolve(&mut self, t: i64, outcome: Outcome) {
        let f = self.inflight.get(&t).expect("request in flight");
        // A fresh baseline: other threads may have completed since admission.
        let before = self.dispatcher.counters();
        let script = f.script.clone();
        script.send(outcome).unwrap();
        match self.events.recv().unwrap() {
            Event::Finished(u) => assert_eq!(u, t),
            Event::Entered(u) => panic!("unexpected backend entry by {u}"),
        }
        self.inflight.get_mut(&t).unwrap().before = before;
        self.collect(t);
    }

    /// Join a finished caller, check the value against the naive oracle and
    /// derive the source from this call's counter delta.
    fn collect(&mut self, t: i64) {
        let f = self.inflight.remove(&t).unwrap();
        let answer = f.handle.join().expect("caller thread");
        let after = self.dispatcher.counters();
        let (result, source) = match answer {
            Answer::Msm(point) => {
                let (s, b) = msm_inputs(f.pairs, f.input);
                let ok = point == naive_msm(&s, &b);
                let src = if after.msm_accelerated > f.before.msm_accelerated {
                    Src::SAccel
                } else {
                    assert!(after.msm_cpu > f.before.msm_cpu, "msm neither cpu nor accelerated");
                    Src::SCpu
                };
                (if ok { abstract_value(f.op, f.input, f.pairs) } else { -99 }, src)
            }
            Answer::Transform(Err(e)) => {
                assert!(!f.valid, "valid transform rejected: {e:?}");
                assert_eq!(e.kind, ErrorKind::InvalidInput);
                assert_eq!(after, f.before, "rejected request changed counters");
                (0, Src::SReject)
            }
            Answer::Transform(Ok(values)) => {
                let (v0, spec) = transform_inputs(f.valid, f.input);
                let ok = values == naive_transform(&v0, &spec);
                let src = if after.transform_accelerated > f.before.transform_accelerated {
                    Src::SAccel
                } else {
                    assert!(after.transform_cpu > f.before.transform_cpu);
                    Src::SCpu
                };
                (if ok { abstract_value(f.op, f.input, f.pairs) } else { -99 }, src)
            }
        };
        self.views.insert(t, ReqView { phase: Phase::Done, result, source });
    }
}

fn kind(k: Kind) -> ErrorKind {
    match k {
        Kind::Unsupported => ErrorKind::Unsupported,
        Kind::InvalidInput => ErrorKind::InvalidInput,
        Kind::Capacity => ErrorKind::Capacity,
        Kind::Allocation => ErrorKind::Allocation,
        Kind::Transfer => ErrorKind::Transfer,
        Kind::Kernel => ErrorKind::Kernel,
        Kind::Synchronization => ErrorKind::Synchronization,
        Kind::ContextLost => ErrorKind::ContextLost,
        Kind::Poisoned => ErrorKind::Poisoned,
        Kind::Internal => ErrorKind::Internal,
        Kind::Cancelled => ErrorKind::Cancelled,
    }
}

fn effect(e: Effect) -> FailureEffect {
    match e {
        Effect::NotStarted => FailureEffect::NotStarted,
        Effect::InputsPreserved => FailureEffect::InputsPreserved,
        Effect::SessionPoisoned => FailureEffect::SessionPoisoned,
    }
}

impl Driver for DispatchDriver {
    type State = Snapshot;
    fn step(&mut self, step: &Step) -> Result {
        quint_connect::switch!(step {
            init => {
                // Driver is reused across traces; Drop releases leftover callers.
                *self = DispatchDriver::default();
                self.views = (0..2).map(|t| (t, DispatchDriver::idle())).collect();
            },
            admit(t: i64, op: Op, pairs: i64, abiOk: bool, capOk: bool, sizeOk: bool, valid: bool, input: i64) => {
                self.admit(t, op, pairs as usize, abiOk, capOk, sizeOk, valid, input);
            },
            enter(t: i64) => {
                // The caller entered the fake during `admit`; nothing to do.
                assert!(self.inflight.contains_key(&t));
            },
            accelOk(t: i64) => { self.resolve(t, Outcome::Ok); },
            accelWrongLen(t: i64) => { self.resolve(t, Outcome::WrongLen); },
            accelErr(t: i64, kind: Kind, effect: Effect) => {
                self.resolve(t, Outcome::Err(self::kind(kind), self::effect(effect)));
            },
            recycle(t: i64) => { self.views.insert(t, DispatchDriver::idle()); }
        })
    }
}

impl Drop for DispatchDriver {
    fn drop(&mut self) {
        // Release callers blocked in the fake at trace end so threads exit.
        for (_, f) in std::mem::take(&mut self.inflight) {
            let _ = f.script.send(Outcome::Err(ErrorKind::Internal, FailureEffect::NotStarted));
            let _ = f.handle.join();
        }
    }
}

#[quint_connect::quint_run(
    spec = "../dispatch.qnt",
    main = "q2_dispatch",
    step = "mbtStep",
    max_samples = 2000,
    max_steps = 30,
    seed = "20261025"
)]
fn replay_p1_dispatcher() -> DispatchDriver {
    DispatchDriver::default()
}
