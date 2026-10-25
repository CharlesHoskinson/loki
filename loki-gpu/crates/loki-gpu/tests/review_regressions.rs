//! Independent P2 review reproducers. No CUDA or native device access.
mod support;

use std::sync::Arc;
use loki_backend_api::ResidentBackend;
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, RuntimeConfig};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeDriver, Fault};
use support::{scalars, Toy, ToyPoint};

fn runtime() -> (Arc<FakeDriver>, GpuRuntime) {
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    let runtime = GpuRuntime::with_driver(fake.clone(), RuntimeConfig {
        device_budget_cap: Some(1 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 64,
        ..RuntimeConfig::default()
    });
    (fake, runtime)
}

fn rb(runtime: &GpuRuntime) -> &dyn ResidentBackend<Toy, ToyPoint> { runtime }

#[test]
fn foreign_runtime_handle_is_rejected() {
    let (_, a) = runtime();
    let (_, b) = runtime();
    a.initialize().unwrap();
    b.initialize().unwrap();
    let handle_a = rb(&a).upload_scalars(&scalars(4, 1)).unwrap();
    let handle_b = rb(&b).upload_scalars(&scalars(4, 2)).unwrap();
    assert_ne!(rb(&a).download_scalars(&handle_a).unwrap(), rb(&b).download_scalars(&handle_b).unwrap());
    let result = rb(&b).download_scalars(&handle_a);
    assert!(result.is_err(), "foreign handle silently returned another runtime's data: {result:?}");
}

#[test]
fn healthy_drop_stops_destructing_after_new_sticky_sync_error() {
    let (fake, runtime) = runtime();
    runtime.initialize().unwrap();
    rb(&runtime).upload_scalars(&scalars(4, 1)).unwrap();
    fake.inject(DriverCall::StreamSync, 1, Fault::Fail(DriverErrorClass::Poisoned));
    drop(runtime);
    assert!(fake.violations().is_empty(), "teardown touched a newly poisoned context: {:?}", fake.violations());
}

#[test]
fn setup_cleanup_stops_after_sticky_destroy_error() {
    let (fake, runtime) = runtime();
    fake.inject(DriverCall::EventCreate, 3, Fault::Fail(DriverErrorClass::Transient));
    fake.inject(DriverCall::EventDestroy, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert!(runtime.initialize().is_err());
    assert!(fake.violations().is_empty(), "setup cleanup touched a newly poisoned context: {:?}", fake.violations());
}

#[test]
fn sticky_timing_failure_cannot_publish_upload() {
    let (fake, runtime) = runtime();
    runtime.initialize().unwrap();
    fake.inject(DriverCall::EventElapsed, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let result = rb(&runtime).upload_scalars(&scalars(4, 1));
    assert!(result.is_err(), "upload published despite observed sticky timing failure: {result:?}");
    assert!(!runtime.snapshot().healthy, "sticky timing failure left runtime healthy");
}
