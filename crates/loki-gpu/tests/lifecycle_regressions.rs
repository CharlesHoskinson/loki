//! Review finding L1 (failed setup loses uncertainty), fake driver only.
//!
//! A setup attempt that ends uncertain (sticky cause, or a destroy failing
//! while cleaning up) must be persisted: `initialize` refuses without
//! touching the driver, `reinitialize` resets before rebuilding, and a failed
//! rebuild after a reset is persisted again. A clean (transient, fully
//! cleaned) setup failure leaves no state and may simply be retried.
mod support;


use loki_backend_api::{ErrorKind, FailureEffect, ResidentBackend};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeControl, Fault};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, RuntimeConfig};
use support::{scalars, Toy, ToyPoint};

fn runtime() -> (FakeControl, GpuRuntime) {
    let config = RuntimeConfig {
        device_budget_cap: Some(1 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 64,
        ..RuntimeConfig::default()
    };
    GpuRuntime::with_fake(FakeConfig::default(), config)
}

fn rb(rt: &GpuRuntime) -> &dyn ResidentBackend<Toy, ToyPoint> {
    rt
}

fn usable(rt: &GpuRuntime) {
    let h = rb(rt).upload_scalars(&scalars(4, 9)).expect("fresh generation usable");
    assert_eq!(rb(rt).download_scalars(&h).unwrap(), scalars(4, 9));
    rb(rt).release_scalars(h).unwrap();
}

fn assert_refused_without_driver_calls(fake: &FakeControl, rt: &GpuRuntime) {
    let (probes, streams) = (fake.calls(DriverCall::Probe), fake.calls(DriverCall::StreamCreate));
    let e = rt.initialize().expect_err("initialize must refuse after uncertain setup");
    assert_eq!((e.kind, e.effect), (ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
    assert_eq!(fake.calls(DriverCall::Probe), probes, "no re-probe");
    assert_eq!(fake.calls(DriverCall::StreamCreate), streams, "no rebuild on the old context");
    assert!(!rt.snapshot().healthy, "no healthy generation is advertised");
}

#[test]
fn sticky_setup_failure_is_persisted_until_reset() {
    let (fake, rt) = runtime();
    fake.inject(DriverCall::StreamCreate, 2, Fault::Fail(DriverErrorClass::Poisoned));
    let e = rt.initialize().expect_err("sticky setup failure");
    assert_eq!(e.effect, FailureEffect::SessionPoisoned, "{e:?}");
    assert_refused_without_driver_calls(&fake, &rt);
    assert_eq!(fake.calls(DriverCall::Reset), 0);
    rt.reinitialize().expect("reset then rebuild");
    assert_eq!(fake.calls(DriverCall::Reset), 1, "recovery reset the context");
    usable(&rt);
    assert!(fake.violations().is_empty(), "{:?}", fake.violations());
}

#[test]
fn failed_cleanup_is_persisted_with_its_own_kind() {
    // EventCreate #1 fails transiently; cleanup then fails destroying the
    // first stream. The context is uncertain; the destroy failure's kind is
    // reported.
    let (fake, rt) = runtime();
    fake.inject(DriverCall::EventCreate, 1, Fault::Fail(DriverErrorClass::Transient));
    fake.inject(DriverCall::StreamDestroy, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let e = rt.initialize().expect_err("setup failure");
    assert_eq!((e.kind, e.effect), (ErrorKind::Internal, FailureEffect::SessionPoisoned), "{e:?}");
    assert_refused_without_driver_calls(&fake, &rt);
    rt.reinitialize().expect("reset then rebuild");
    assert_eq!(fake.calls(DriverCall::Reset), 1);
    usable(&rt);
    assert!(fake.violations().is_empty(), "{:?}", fake.violations());
}

#[test]
fn failed_rebuild_after_reset_is_persisted_again() {
    let (fake, rt) = runtime();
    fake.inject(DriverCall::StreamCreate, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert!(rt.initialize().is_err());
    // The rebuild after the first reset fails sticky too.
    fake.inject(DriverCall::StreamCreate, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let e = rt.reinitialize().expect_err("failed rebuild");
    assert_eq!(e.effect, FailureEffect::SessionPoisoned, "{e:?}");
    assert_eq!(fake.calls(DriverCall::Reset), 1);
    assert_refused_without_driver_calls(&fake, &rt);
    rt.reinitialize().expect("second reset then rebuild");
    assert_eq!(fake.calls(DriverCall::Reset), 2);
    usable(&rt);
}

#[test]
fn poisoned_context_rebuild_failure_requires_another_reset() {
    let (fake, rt) = runtime();
    rt.initialize().unwrap();
    // Poison the live generation.
    fake.inject(DriverCall::EventSync, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert!(rb(&rt).upload_scalars(&scalars(4, 1)).is_err());
    assert!(!rt.snapshot().healthy);
    fake.inject(DriverCall::EventCreate, 2, Fault::Fail(DriverErrorClass::Poisoned));
    assert!(rt.reinitialize().is_err());
    assert_refused_without_driver_calls(&fake, &rt);
    rt.reinitialize().expect("reset again");
    assert_eq!(fake.calls(DriverCall::Reset), 2);
    usable(&rt);
}

#[test]
fn clean_transient_setup_failure_leaves_no_state() {
    let (fake, rt) = runtime();
    fake.inject(DriverCall::StreamCreate, 2, Fault::Fail(DriverErrorClass::Transient));
    let e = rt.initialize().expect_err("transient setup failure");
    assert_eq!(e.effect, FailureEffect::NotStarted, "{e:?}");
    rt.initialize().expect("plain retry succeeds");
    assert_eq!(fake.calls(DriverCall::Reset), 0, "no reset needed");
    usable(&rt);
    assert!(fake.violations().is_empty(), "{:?}", fake.violations());
}

#[test]
fn probe_failures_are_classified() {
    // No device / unusable device: Unsupported, nothing persisted.
    let (fake, rt) = runtime();
    fake.inject(DriverCall::Probe, 1, Fault::Fail(DriverErrorClass::Unsupported));
    let e = rt.initialize().expect_err("no device");
    assert_eq!((e.kind, e.effect), (ErrorKind::Unsupported, FailureEffect::NotStarted));
    rt.initialize().expect("device appears: plain retry");
    // Transient query failure: NotStarted, retryable without reset.
    let (fake, rt) = runtime();
    fake.inject(DriverCall::Probe, 1, Fault::Fail(DriverErrorClass::Transient));
    let e = rt.initialize().expect_err("transient probe");
    assert_eq!(e.effect, FailureEffect::NotStarted, "{e:?}");
    rt.initialize().expect("retry");
    assert_eq!(fake.calls(DriverCall::Reset), 0);
    // Sticky or lost context while probing: persisted, reset required.
    for class in [DriverErrorClass::Poisoned, DriverErrorClass::ContextLost] {
        let (fake, rt) = runtime();
        fake.inject(DriverCall::Probe, 1, Fault::Fail(class));
        let e = rt.initialize().expect_err("uncertain probe");
        assert_eq!(e.effect, FailureEffect::SessionPoisoned, "{e:?}");
        assert_refused_without_driver_calls(&fake, &rt);
        rt.reinitialize().expect("reset then rebuild");
        assert_eq!(fake.calls(DriverCall::Reset), 1);
        usable(&rt);
    }
}

#[test]
fn failed_reset_keeps_setup_uncertainty() {
    let (fake, rt) = runtime();
    fake.inject(DriverCall::StreamCreate, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert!(rt.initialize().is_err());
    fake.inject(DriverCall::Reset, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let e = rt.reinitialize().expect_err("reset fails");
    assert_eq!((e.kind, e.effect), (ErrorKind::ContextLost, FailureEffect::SessionPoisoned));
    assert_refused_without_driver_calls(&fake, &rt);
    rt.reinitialize().expect("second reset succeeds");
    assert_eq!(fake.calls(DriverCall::Reset), 2);
    usable(&rt);
}
