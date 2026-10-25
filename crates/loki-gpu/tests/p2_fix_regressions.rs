//! P2-fix coverage beyond the review reproducers (no CUDA, fake driver only):
//! D1 process-unique identity for every token kind, D2 every teardown and
//! setup-cleanup boundary, D3 sticky optional-telemetry failures on every
//! transfer/launch path.
mod support;


use loki_backend_api::{ErrorKind, FailureEffect, ReservationResult, ResidentBackend};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeControl, Fault};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, RuntimeConfig};
use support::{scalars, Toy, ToyPoint};

fn config() -> RuntimeConfig {
    RuntimeConfig {
        device_budget_cap: Some(1 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 64,
        checkpoint_budget_bytes: 1 << 16,
        ..RuntimeConfig::default()
    }
}

fn runtime() -> (FakeControl, GpuRuntime) {
    let (fake, rt) = GpuRuntime::with_fake(FakeConfig::default(), config());
    (fake, rt)
}

fn rb(rt: &GpuRuntime) -> &dyn ResidentBackend<Toy, ToyPoint> {
    rt
}

// --------------------------------------------------------------------------
// D1: identity is process-unique; foreign tokens of every kind are rejected
// --------------------------------------------------------------------------

#[test]
fn identities_are_unique_across_runtimes() {
    let (_, a) = runtime();
    let (_, b) = runtime();
    a.initialize().unwrap();
    b.initialize().unwrap();
    let ha = rb(&a).upload_scalars(&scalars(4, 1)).unwrap();
    let hb = rb(&b).upload_scalars(&scalars(4, 2)).unwrap();
    assert_ne!(ha.generation(), hb.generation());
    assert_ne!(ha.allocation(), hb.allocation());
}

#[test]
fn foreign_release_and_duplicate_are_rejected_without_effect() {
    let (_, a) = runtime();
    let (_, b) = runtime();
    a.initialize().unwrap();
    b.initialize().unwrap();
    let values = scalars(4, 3);
    let ha = rb(&a).upload_scalars(&values).unwrap();
    let hb = rb(&b).upload_scalars(&scalars(4, 4)).unwrap();
    let forged = loki_backend_api::ResidentBuffer::<Toy>::from_untrusted_metadata(
        ha.generation(),
        ha.allocation(),
        ha.len(),
        ha.bytes(),
    );
    assert!(b.duplicate_scalars(&forged).is_err());
    assert!(rb(&b).release_scalars(forged).is_err());
    assert_eq!(rb(&b).runtime_snapshot().unwrap().live_bytes, 32);
    assert_eq!(rb(&a).download_scalars(&ha).unwrap(), values);
    rb(&a).release_scalars(ha).unwrap();
    rb(&b).release_scalars(hb).unwrap();
}

#[test]
fn foreign_reservation_is_rejected() {
    let (_, a) = runtime();
    let (_, b) = runtime();
    a.initialize().unwrap();
    b.initialize().unwrap();
    let ta = match rb(&a).try_reserve(100).unwrap() {
        ReservationResult::Reserved(t) => t,
        ReservationResult::Rejected { .. } => panic!("fits"),
    };
    let tb = match rb(&b).try_reserve(100).unwrap() {
        ReservationResult::Reserved(t) => t,
        ReservationResult::Rejected { .. } => panic!("fits"),
    };
    let forged = loki_backend_api::ReservationToken::from_untrusted_metadata(
        ta.id(),
        ta.generation(),
        ta.bytes(),
    );
    assert!(rb(&b).release_reservation(forged).is_err());
    assert_eq!(rb(&b).runtime_snapshot().unwrap().reserved_bytes, 100);
    rb(&a).release_reservation(ta).unwrap();
    rb(&b).release_reservation(tb).unwrap();
}

#[test]
fn foreign_checkpoint_cannot_be_recovered() {
    let (_, a) = runtime();
    let (_, b) = runtime();
    a.initialize().unwrap();
    b.initialize().unwrap();
    let ha = a.upload_scalars_with_checkpoint::<Toy>(&scalars(4, 5)).unwrap();
    let _hb = b.upload_scalars_with_checkpoint::<Toy>(&scalars(4, 6)).unwrap();
    let forged = loki_backend_api::ResidentBuffer::<Toy>::from_untrusted_metadata(
        ha.generation(),
        ha.allocation(),
        ha.len(),
        ha.bytes(),
    );
    assert!(b.recover_checkpoint(forged).is_err());
}

// --------------------------------------------------------------------------
// D2: every teardown / setup-cleanup boundary stops at the first failure
// --------------------------------------------------------------------------

const TEARDOWN_CALLS: [DriverCall; 5] = [
    DriverCall::StreamSync,
    DriverCall::DeviceFree,
    DriverCall::EventDestroy,
    DriverCall::HostFree,
    DriverCall::StreamDestroy,
];

#[test]
fn teardown_stops_at_every_failure_boundary() {
    for call in TEARDOWN_CALLS {
        for nth in 1..=8 {
            for class in [DriverErrorClass::Poisoned, DriverErrorClass::ContextLost, DriverErrorClass::Transient] {
                let (fake, rt) = runtime();
                rt.initialize().unwrap();
                let _a = rb(&rt).upload_scalars(&scalars(4, 1)).unwrap();
                let _b = rb(&rt).upload_scalars(&scalars(4, 2)).unwrap();
                fake.inject(call, nth, Fault::Fail(class));
                drop(rt);
                assert!(
                    fake.violations().is_empty(),
                    "{call:?}#{nth} {class:?}: {:?}",
                    fake.violations()
                );
                assert_eq!(fake.double_frees(), 0);
            }
        }
    }
}

#[test]
fn healthy_teardown_without_faults_still_frees_everything() {
    let (fake, rt) = runtime();
    rt.initialize().unwrap();
    let _a = rb(&rt).upload_scalars(&scalars(4, 1)).unwrap();
    drop(rt);
    assert!(fake.violations().is_empty());
    assert_eq!(
        (fake.live_device_allocations(), fake.live_host_buffers(), fake.live_streams(), fake.live_events()),
        (0, 0, 0, 0)
    );
}

#[test]
fn setup_cleanup_stops_at_every_failure_boundary() {
    let creates = [DriverCall::StreamCreate, DriverCall::EventCreate, DriverCall::HostAlloc];
    let destroys = [DriverCall::HostFree, DriverCall::EventDestroy, DriverCall::StreamDestroy];
    for create in creates {
        for nth in 1..=6 {
            for destroy in destroys {
                for class in [DriverErrorClass::Poisoned, DriverErrorClass::ContextLost, DriverErrorClass::Transient] {
                    let (fake, rt) = runtime();
                    fake.inject(create, nth, Fault::Fail(DriverErrorClass::Transient));
                    fake.inject(destroy, 1, Fault::Fail(class));
                    let _ = rt.initialize();
                    drop(rt);
                    assert!(
                        fake.violations().is_empty(),
                        "{create:?}#{nth} then {destroy:?} {class:?}: {:?}",
                        fake.violations()
                    );
                    assert_eq!(fake.double_frees(), 0);
                }
            }
        }
    }
}

// --------------------------------------------------------------------------
// D3: sticky optional-telemetry failures poison and never publish
// --------------------------------------------------------------------------

fn assert_poisoned<T: std::fmt::Debug>(rt: &GpuRuntime, fake: &FakeControl, r: Result<T, loki_backend_api::BackendError>) {
    let e = r.expect_err("sticky telemetry failure must not publish");
    assert_eq!(e.effect, FailureEffect::SessionPoisoned, "{e:?}");
    assert!(!rt.snapshot().healthy);
    assert!(fake.violations().is_empty(), "{:?}", fake.violations());
}

#[test]
fn sticky_timing_failures_poison_upload_download_and_launch() {
    for call in [DriverCall::EventRecord, DriverCall::EventElapsed] {
        for class in [DriverErrorClass::Poisoned, DriverErrorClass::ContextLost] {
            // Upload: the first EventRecord is the timing start.
            let (fake, rt) = runtime();
            rt.initialize().unwrap();
            fake.inject(call, 1, Fault::Fail(class));
            assert_poisoned(&rt, &fake, rb(&rt).upload_scalars(&scalars(16, 1)));
            // Nothing was published: the upload's memory is quarantined, not live.
            assert_eq!(fake.calls(DriverCall::DeviceFree), 0);

            // Download.
            let (fake, rt) = runtime();
            rt.initialize().unwrap();
            let h = rb(&rt).upload_scalars(&scalars(16, 2)).unwrap();
            fake.inject(call, 1, Fault::Fail(class));
            assert_poisoned(&rt, &fake, rb(&rt).download_scalars(&h));

            // Launch (resident duplicate).
            let (fake, rt) = runtime();
            rt.initialize().unwrap();
            let h = rb(&rt).upload_scalars(&scalars(16, 3)).unwrap();
            // Launch records copy_done (mandatory) first; timing start is #2.
            let nth = if call == DriverCall::EventRecord { 2 } else { 1 };
            fake.inject(call, nth, Fault::Fail(class));
            assert_poisoned(&rt, &fake, rt.duplicate_scalars(&h));
        }
    }
}

#[test]
fn transient_timing_failures_are_ignored() {
    for call in [DriverCall::EventRecord, DriverCall::EventElapsed] {
        for class in [DriverErrorClass::Transient, DriverErrorClass::Unsupported] {
            let (fake, rt) = runtime();
            rt.initialize().unwrap();
            let values = scalars(16, 4);
            fake.inject(call, 1, Fault::Fail(class));
            let h = rb(&rt).upload_scalars(&values).unwrap();
            fake.inject(call, 1, Fault::Fail(class));
            assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
            let nth = if call == DriverCall::EventRecord { 2 } else { 1 };
            fake.inject(call, nth, Fault::Fail(class));
            let d = rt.duplicate_scalars(&h).unwrap();
            assert_eq!(rb(&rt).download_scalars(&d).unwrap(), values);
            assert!(rt.snapshot().healthy);
            assert!(fake.violations().is_empty());
        }
    }
}

#[test]
fn cancellation_reports_the_cancelled_kind() {
    let (_, rt) = runtime();
    rt.initialize().unwrap();
    let token = loki_gpu::runtime::CancelToken::new();
    token.cancel();
    let e = rt.upload_scalars_cancellable::<Toy>(&scalars(4, 1), &token).unwrap_err();
    assert_eq!((e.kind, e.effect), (ErrorKind::Cancelled, FailureEffect::NotStarted));
}
