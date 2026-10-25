//! P2 runtime contract: injected-failure tests against the fault-injecting
//! fake device driver. No CUDA toolkit or GPU is required.
//!
//! The fake executes enqueued copies/launches lazily, only when an event or
//! stream is synchronized, and it reads/writes backend-owned staging buffers at
//! execution time. A runtime that reused a staging slot, freed an allocation,
//! or published an output before the governing event retired would therefore
//! observe corrupted data or a recorded `Violation`. Every test ends by
//! asserting that no violation was recorded.

mod support;

use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use loki_backend_api::{
    AllocationId, BackendError, DeviceGeneration, ErrorKind, FailureEffect, ReservationId,
    ReservationResult, ReservationToken, ResidentBackend, ResidentBuffer,
};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeDriver, Fault};
use loki_gpu::runtime::{
    CancelToken, DriverErrorClass, GpuRuntime, LossClass, RuntimeConfig, MAX_PINNED_SLOTS,
};
use support::{scalars, Toy, ToyPoint};

type Rb = dyn ResidentBackend<Toy, ToyPoint>;

const SLOT: usize = 64; // 8 Toy elements per staging chunk

fn config() -> RuntimeConfig {
    RuntimeConfig {
        device_budget_cap: Some(1 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: SLOT,
        checkpoint_budget_bytes: 1 << 20,
        ..RuntimeConfig::default()
    }
}

fn setup_with(cfg: RuntimeConfig, fake_cfg: FakeConfig) -> (Arc<FakeDriver>, GpuRuntime) {
    let fake = Arc::new(FakeDriver::new(fake_cfg));
    let rt = GpuRuntime::with_driver(fake.clone(), cfg);
    rt.initialize().expect("fake device initializes");
    (fake, rt)
}

fn setup() -> (Arc<FakeDriver>, GpuRuntime) {
    setup_with(config(), FakeConfig::default())
}

fn rb(rt: &GpuRuntime) -> &Rb {
    rt
}

fn assert_err<T>(result: Result<T, BackendError>, kind: ErrorKind, effect: FailureEffect) {
    match result {
        Ok(_) => panic!("expected {kind:?}/{effect:?}, got success"),
        Err(e) => {
            assert_eq!(e.kind, kind, "error kind ({e:?})");
            assert_eq!(e.effect, effect, "failure effect ({e:?})");
            assert!(e.preserves_observable_state());
        }
    }
}

fn assert_clean(fake: &FakeDriver) {
    let v = fake.violations();
    assert!(v.is_empty(), "fake driver recorded lifetime violations: {v:?}");
}

fn live_and_reserved(rt: &GpuRuntime) -> (u64, u64) {
    let s = rb(rt).runtime_snapshot().expect("snapshot");
    (s.live_bytes, s.reserved_bytes)
}

// --------------------------------------------------------------------------
// Initialization and capability
// --------------------------------------------------------------------------

#[test]
fn uninitialized_runtime_fabricates_no_generation() {
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    let rt = GpuRuntime::with_driver(fake.clone(), config());
    assert_err(rb(&rt).generation(), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(rb(&rt).runtime_snapshot(), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(rb(&rt).try_reserve(8), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(
        rb(&rt).upload_scalars(&scalars(4, 1)),
        ErrorKind::Unsupported,
        FailureEffect::NotStarted,
    );
    assert!(rt.snapshot().generation.is_none());
    assert!(!rt.capabilities().resident_buffers);
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), 0);
}

#[test]
fn driverless_runtime_is_unsupported() {
    let rt = GpuRuntime::unavailable();
    assert_err(rt.initialize(), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(rb(&rt).generation(), ErrorKind::Unsupported, FailureEffect::NotStarted);
}

#[test]
fn non_sm120_device_is_unsupported_and_allocates_nothing() {
    let fake = Arc::new(FakeDriver::new(FakeConfig {
        compute_capability: (8, 9),
        ..FakeConfig::default()
    }));
    let rt = GpuRuntime::with_driver(fake.clone(), config());
    assert_err(rt.initialize(), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(rb(&rt).generation(), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_eq!(fake.calls(DriverCall::StreamCreate), 0);
    assert_eq!(fake.calls(DriverCall::HostAlloc), 0);
}

#[test]
fn invalid_config_is_rejected_before_probe() {
    for bad in [
        RuntimeConfig { staging_slots: 0, ..config() },
        RuntimeConfig { staging_slots: MAX_PINNED_SLOTS + 1, ..config() },
        RuntimeConfig { staging_slot_bytes: 0, ..config() },
        RuntimeConfig { staging_slot_bytes: 64 * 1024 * 1024, ..config() },
    ] {
        let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
        let rt = GpuRuntime::with_driver(fake.clone(), bad);
        assert_err(rt.initialize(), ErrorKind::InvalidInput, FailureEffect::NotStarted);
        assert_eq!(fake.calls(DriverCall::Probe), 0);
    }
}

#[test]
fn budget_is_lower_of_cap_and_free_minus_headroom() {
    let (_f, rt) = setup_with(
        RuntimeConfig {
            device_budget_cap: Some(10_000),
            device_headroom_bytes: 0,
            ..config()
        },
        FakeConfig::default(),
    );
    assert_eq!(rb(&rt).runtime_snapshot().unwrap().budget_bytes, 10_000);

    let (_f, rt) = setup_with(
        RuntimeConfig {
            device_budget_cap: None,
            device_headroom_bytes: 1_000,
            ..config()
        },
        FakeConfig {
            free_bytes: 5_000,
            ..FakeConfig::default()
        },
    );
    assert_eq!(rb(&rt).runtime_snapshot().unwrap().budget_bytes, 4_000);

    // Headroom larger than free memory yields a zero budget, never underflow.
    let (_f, rt) = setup_with(
        RuntimeConfig {
            device_budget_cap: None,
            device_headroom_bytes: 10_000,
            ..config()
        },
        FakeConfig {
            free_bytes: 5_000,
            ..FakeConfig::default()
        },
    );
    assert_eq!(rb(&rt).runtime_snapshot().unwrap().budget_bytes, 0);
}

#[test]
fn healthy_runtime_advertises_resident_buffers_only() {
    let (_f, rt) = setup();
    let caps = rt.capabilities();
    assert!(caps.resident_buffers);
    assert!(!caps.host_msm && !caps.host_transform && !caps.resident_transform);
    assert!(!caps.quotient_interpreter && !caps.srs_registry);
    assert_eq!(caps.device_memory_budget, Some(1 << 20));
}

// --------------------------------------------------------------------------
// Staging pool: bounded, probed, pageable fallback
// --------------------------------------------------------------------------

#[test]
fn pinned_staging_probe_reports_capability() {
    let (_f, rt) = setup();
    let cap = rt.staging_capability().expect("initialized");
    assert_eq!(cap.pinned_slots, 2);
    assert_eq!(cap.pageable_slots, 0);
    assert_eq!(cap.slot_bytes, SLOT);
    assert!(cap.async_overlap_available);
    assert_eq!(rt.telemetry().pinned_bytes, 2 * SLOT as u64);
}

#[test]
fn pinned_cap_falls_back_to_pageable_and_still_round_trips() {
    // Model the WSL2 pinned limit: only one slot of pinned memory available.
    let (fake, rt) = setup_with(
        config(),
        FakeConfig {
            pinned_limit_bytes: Some(SLOT as u64),
            ..FakeConfig::default()
        },
    );
    let cap = rt.staging_capability().unwrap();
    assert_eq!((cap.pinned_slots, cap.pageable_slots), (1, 1));
    assert!(cap.async_overlap_available);
    let t = rt.telemetry();
    assert_eq!(t.pinned_probe_failures, 1);

    let values = scalars(37, 2);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    let t = rt.telemetry();
    assert!(t.h2d_pinned_bytes > 0 && t.h2d_pageable_bytes > 0);
    assert_eq!(t.h2d_pinned_bytes + t.h2d_pageable_bytes, 37 * 8);
    rb(&rt).release_scalars(h).unwrap();
    assert_clean(&fake);
}

#[test]
fn no_pinned_memory_disables_overlap_capability() {
    let (fake, rt) = setup_with(
        config(),
        FakeConfig {
            pinned_limit_bytes: Some(0),
            ..FakeConfig::default()
        },
    );
    let cap = rt.staging_capability().unwrap();
    assert_eq!((cap.pinned_slots, cap.pageable_slots), (0, 2));
    assert!(!cap.async_overlap_available);
    let values = scalars(20, 3);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    assert_eq!(rt.telemetry().h2d_pinned_bytes, 0);
    assert_clean(&fake);
}

#[test]
fn multi_chunk_round_trip_never_reuses_a_live_slot() {
    let (fake, rt) = setup();
    // 100 elements = 800 bytes = 13 chunks through 2 slots.
    let values = scalars(100, 4);
    let before = values.clone();
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(values, before, "upload must not mutate the caller slice");
    assert_eq!(h.len(), 100);
    assert_eq!(h.bytes(), 800);
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    assert_eq!(live_and_reserved(&rt), (800, 0));
    rb(&rt).release_scalars(h).unwrap();
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(rt.telemetry().live_high_water, 800);
    assert_clean(&fake);
}

#[test]
fn empty_upload_is_rejected_without_allocation() {
    let (fake, rt) = setup();
    assert_err(
        rb(&rt).upload_scalars(&[]),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), 0);
}

// --------------------------------------------------------------------------
// Allocation and budget
// --------------------------------------------------------------------------

#[test]
fn device_allocation_failure_is_transient_and_not_started() {
    let (fake, rt) = setup();
    let gen = rb(&rt).generation().unwrap();
    fake.inject(DriverCall::DeviceAlloc, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        rb(&rt).upload_scalars(&scalars(10, 5)),
        ErrorKind::Allocation,
        FailureEffect::NotStarted,
    );
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(rb(&rt).generation().unwrap(), gen, "transient errors keep the generation");
    assert!(rb(&rt).runtime_snapshot().unwrap().context_healthy);
    let values = scalars(10, 6);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    assert_clean(&fake);
}

#[test]
fn budget_exhaustion_rejects_before_device_allocation() {
    let (fake, rt) = setup_with(
        RuntimeConfig {
            device_budget_cap: Some(100),
            ..config()
        },
        FakeConfig::default(),
    );
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    assert_err(
        rb(&rt).upload_scalars(&scalars(13, 7)), // 104 bytes > 100
        ErrorKind::Capacity,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs);
    let h = rb(&rt).upload_scalars(&scalars(12, 7)).unwrap(); // 96 bytes fits
    assert_err(
        rb(&rt).upload_scalars(&scalars(1, 7)), // 96 + 8 > 100
        ErrorKind::Capacity,
        FailureEffect::NotStarted,
    );
    rb(&rt).release_scalars(h).unwrap();
    assert_clean(&fake);
}

#[test]
fn reservations_are_checked_authoritative_and_single_use() {
    let (fake, rt) = setup_with(
        RuntimeConfig {
            device_budget_cap: Some(1_000),
            ..config()
        },
        FakeConfig::default(),
    );
    let r = rb(&rt);
    let token = match r.try_reserve(600).unwrap() {
        ReservationResult::Reserved(t) => t,
        ReservationResult::Rejected { .. } => panic!("600 of 1000 must fit"),
    };
    assert_eq!(live_and_reserved(&rt), (0, 600));
    match r.try_reserve(500).unwrap() {
        ReservationResult::Rejected {
            requested_bytes,
            available_bytes,
        } => assert_eq!((requested_bytes, available_bytes), (500, 400)),
        ReservationResult::Reserved(_) => panic!("over budget"),
    }
    // Overflowing requests are rejected, never wrapped.
    assert!(matches!(
        r.try_reserve(u64::MAX).unwrap(),
        ReservationResult::Rejected { .. }
    ));
    // Reserved bytes count against uploads too.
    assert_err(
        r.upload_scalars(&scalars(60, 8)), // 480 > 400 remaining
        ErrorKind::Capacity,
        FailureEffect::NotStarted,
    );

    // Forged tokens: wrong byte count, unknown id, wrong generation.
    let forged_bytes = ReservationToken::from_untrusted_metadata(token.id(), token.generation(), 1);
    assert_err(
        r.release_reservation(forged_bytes),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    let forged_id =
        ReservationToken::from_untrusted_metadata(ReservationId(9_999), token.generation(), 600);
    assert_err(
        r.release_reservation(forged_id),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    let g = token.generation();
    let forged_gen = ReservationToken::from_untrusted_metadata(
        token.id(),
        DeviceGeneration {
            generation: g.generation + 7,
            ..g
        },
        600,
    );
    assert_err(
        r.release_reservation(forged_gen),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    assert_eq!(live_and_reserved(&rt), (0, 600), "forgeries change nothing");

    let (id, gen, bytes) = (token.id(), token.generation(), token.bytes());
    r.release_reservation(token).unwrap();
    assert_eq!(live_and_reserved(&rt), (0, 0));
    // Duplicate release of an identical (reconstructed) token is rejected.
    assert_err(
        r.release_reservation(ReservationToken::from_untrusted_metadata(id, gen, bytes)),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Copy failures
// --------------------------------------------------------------------------

#[test]
fn transient_h2d_failure_after_partial_work_drains_and_frees() {
    let (fake, rt) = setup();
    let gen = rb(&rt).generation().unwrap();
    let values = scalars(50, 9);
    let before = values.clone();
    // Third chunk enqueue fails; two chunks are already in flight.
    fake.inject(DriverCall::CopyH2D, 3, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        rb(&rt).upload_scalars(&values),
        ErrorKind::Transfer,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(values, before);
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(rb(&rt).generation().unwrap(), gen);
    assert_eq!(fake.live_device_allocations(), 0, "drained allocation was freed");
    assert_clean(&fake);
    // Runtime remains usable.
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    assert_clean(&fake);
}

#[test]
fn poisoned_h2d_failure_poisons_generation_and_quarantines() {
    let (fake, rt) = setup();
    let keep = rb(&rt).upload_scalars(&scalars(5, 10)).unwrap();
    let allocs_before = fake.live_device_allocations();
    fake.inject(DriverCall::CopyH2D, 2, Fault::Fail(DriverErrorClass::Poisoned));
    assert_err(
        rb(&rt).upload_scalars(&scalars(40, 11)),
        ErrorKind::Transfer,
        FailureEffect::SessionPoisoned,
    );
    let snap = rb(&rt).runtime_snapshot().unwrap();
    assert!(!snap.context_healthy);
    // The failed allocation and the older live one are quarantined, not freed.
    assert_eq!(fake.live_device_allocations(), allocs_before + 1);
    assert!(rt.telemetry().quarantined_bytes >= 40 * 8);
    // A poisoned generation accepts no further device work.
    assert_err(rb(&rt).download_scalars(&keep), ErrorKind::Poisoned, FailureEffect::SessionPoisoned);
    assert_err(
        rb(&rt).upload_scalars(&scalars(1, 12)),
        ErrorKind::Poisoned,
        FailureEffect::SessionPoisoned,
    );
    assert_err(rb(&rt).try_reserve(8), ErrorKind::Poisoned, FailureEffect::SessionPoisoned);
    drop(rt);
    // Teardown must not destruct through the poisoned context.
    assert_eq!(fake.live_device_allocations(), allocs_before + 1);
    assert_clean(&fake);
}

#[test]
fn transient_d2h_failure_publishes_nothing_and_keeps_source() {
    let (fake, rt) = setup();
    let values = scalars(30, 13);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    fake.inject(DriverCall::CopyD2H, 2, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        rb(&rt).download_scalars(&h),
        ErrorKind::Transfer,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    rb(&rt).release_scalars(h).unwrap();
    assert_clean(&fake);
}

#[test]
fn corrupt_device_bytes_fail_decode_without_publication() {
    let (fake, rt) = setup();
    let h = rb(&rt).upload_scalars(&scalars(9, 14)).unwrap();
    fake.corrupt_device_memory(0xff); // 0xffff_ffff_ffff_ffff >= p
    assert_err(
        rb(&rt).download_scalars(&h),
        ErrorKind::Internal,
        FailureEffect::InputsPreserved,
    );
    assert!(rb(&rt).runtime_snapshot().unwrap().context_healthy);
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Launch failures (runtime self-test kernel)
// --------------------------------------------------------------------------

#[test]
fn launch_success_publishes_fresh_handle_after_completion() {
    let (fake, rt) = setup();
    let values = scalars(25, 15);
    let src = rb(&rt).upload_scalars(&values).unwrap();
    let dup = rt.duplicate_scalars(&src).unwrap();
    assert_ne!(dup.allocation(), src.allocation());
    assert_eq!(rb(&rt).download_scalars(&dup).unwrap(), values);
    assert_eq!(live_and_reserved(&rt), (400, 0));
    rb(&rt).release_scalars(dup).unwrap();
    rb(&rt).release_scalars(src).unwrap();
    assert_clean(&fake);
}

#[test]
fn transient_launch_failure_frees_scratch_and_keeps_source() {
    let (fake, rt) = setup();
    let values = scalars(25, 16);
    let src = rb(&rt).upload_scalars(&values).unwrap();
    fake.inject(DriverCall::Launch, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(rt.duplicate_scalars(&src), ErrorKind::Kernel, FailureEffect::InputsPreserved);
    assert_eq!(live_and_reserved(&rt), (200, 0));
    assert_eq!(rb(&rt).download_scalars(&src).unwrap(), values);
    assert_clean(&fake);
}

#[test]
fn asynchronous_kernel_fault_surfaces_at_sync_and_poisons() {
    let (fake, rt) = setup();
    let src = rb(&rt).upload_scalars(&scalars(25, 17)).unwrap();
    fake.inject(DriverCall::Launch, 1, Fault::FailAtSync(DriverErrorClass::Poisoned));
    assert_err(
        rt.duplicate_scalars(&src),
        ErrorKind::Synchronization,
        FailureEffect::SessionPoisoned,
    );
    assert!(!rb(&rt).runtime_snapshot().unwrap().context_healthy);
    assert_eq!(fake.live_device_allocations(), 2, "source and scratch quarantined");
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Event failures
// --------------------------------------------------------------------------

#[test]
fn event_sync_failure_is_uncertain_completion_and_poisons() {
    // Even a non-sticky event error leaves completion uncertain.
    for class in [DriverErrorClass::Transient, DriverErrorClass::Poisoned] {
        let (fake, rt) = setup();
        fake.inject(DriverCall::EventSync, 1, Fault::Fail(class));
        assert_err(
            rb(&rt).upload_scalars(&scalars(30, 18)),
            ErrorKind::Synchronization,
            FailureEffect::SessionPoisoned,
        );
        assert!(!rb(&rt).runtime_snapshot().unwrap().context_healthy);
        let pinned = fake.live_host_buffers();
        drop(rt);
        assert_eq!(fake.live_host_buffers(), pinned, "staging retained after poison");
        assert_eq!(fake.live_device_allocations(), 1);
        assert_clean(&fake);
    }
}

#[test]
fn event_record_failure_drains_stream_and_preserves() {
    let (fake, rt) = setup();
    fake.inject(DriverCall::EventRecord, 2, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        rb(&rt).upload_scalars(&scalars(30, 19)),
        ErrorKind::Synchronization,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(fake.live_device_allocations(), 0);
    assert!(rb(&rt).runtime_snapshot().unwrap().context_healthy);
    assert_clean(&fake);
}

#[test]
fn event_record_failure_with_failed_drain_poisons() {
    let (fake, rt) = setup();
    fake.inject(DriverCall::EventRecord, 2, Fault::Fail(DriverErrorClass::Transient));
    fake.inject(DriverCall::StreamSync, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        rb(&rt).upload_scalars(&scalars(30, 20)),
        ErrorKind::Synchronization,
        FailureEffect::SessionPoisoned,
    );
    assert_eq!(fake.live_device_allocations(), 1, "uncertain allocation retained");
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Cancellation
// --------------------------------------------------------------------------

#[test]
fn cancellation_drains_retires_and_publishes_nothing() {
    let (fake, rt) = setup();
    let token = CancelToken::new();
    fake.inject(DriverCall::CopyH2D, 4, Fault::Cancel(token.clone()));
    let values = scalars(80, 21);
    assert_err(
        rt.upload_scalars_cancellable::<Toy>(&values, &token),
        ErrorKind::Internal,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(fake.live_device_allocations(), 0);
    assert!(rb(&rt).runtime_snapshot().unwrap().context_healthy);
    assert_clean(&fake); // nothing freed while copies were pending

    // Cancelled download: source survives, no vector returned.
    let h = rb(&rt).upload_scalars(&values).unwrap();
    let token = CancelToken::new();
    fake.inject(DriverCall::CopyD2H, 3, Fault::Cancel(token.clone()));
    assert_err(
        rt.download_scalars_cancellable(&h, &token),
        ErrorKind::Internal,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    assert_clean(&fake);
}

#[test]
fn pre_cancelled_token_does_not_start() {
    let (fake, rt) = setup();
    let token = CancelToken::new();
    token.cancel();
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    assert_err(
        rt.upload_scalars_cancellable::<Toy>(&scalars(4, 22), &token),
        ErrorKind::Internal,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs);
}

#[test]
fn cancellation_with_failed_drain_poisons_and_retains() {
    let (fake, rt) = setup();
    let token = CancelToken::new();
    fake.inject(DriverCall::CopyH2D, 3, Fault::Cancel(token.clone()));
    fake.inject(DriverCall::StreamSync, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert_err(
        rt.upload_scalars_cancellable::<Toy>(&scalars(80, 23), &token),
        ErrorKind::Synchronization,
        FailureEffect::SessionPoisoned,
    );
    assert_eq!(fake.live_device_allocations(), 1);
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Stale generation, context loss, checkpoints
// --------------------------------------------------------------------------

#[test]
fn context_loss_then_reinitialize_rejects_stale_handles() {
    let (fake, rt) = setup();
    let old_gen = rb(&rt).generation().unwrap();
    let values = scalars(12, 24);
    let old = rb(&rt).upload_scalars(&values).unwrap();
    let old_token = match rb(&rt).try_reserve(16).unwrap() {
        ReservationResult::Reserved(t) => t,
        _ => panic!(),
    };
    fake.inject(DriverCall::DeviceAlloc, 1, Fault::Fail(DriverErrorClass::ContextLost));
    assert_err(
        rb(&rt).upload_scalars(&scalars(3, 25)),
        ErrorKind::ContextLost,
        FailureEffect::SessionPoisoned,
    );
    // Reinitialize is the only path to a new generation.
    let new_gen = rt.reinitialize().unwrap();
    assert_ne!(new_gen, old_gen);
    assert!(new_gen.generation > old_gen.generation);
    assert!(new_gen.context_epoch > old_gen.context_epoch);
    assert!(rb(&rt).runtime_snapshot().unwrap().context_healthy);
    assert_eq!(live_and_reserved(&rt), (0, 0), "old accounting does not leak forward");

    assert_err(rb(&rt).download_scalars(&old), ErrorKind::ContextLost, FailureEffect::NotStarted);
    assert_err(rt.duplicate_scalars(&old), ErrorKind::ContextLost, FailureEffect::NotStarted);
    assert_err(
        rb(&rt).release_reservation(old_token),
        ErrorKind::ContextLost,
        FailureEffect::NotStarted,
    );
    assert_eq!(rt.loss_class(&old), LossClass::Unrecoverable);
    assert_err(rt.recover_checkpoint(old), ErrorKind::ContextLost, FailureEffect::NotStarted);

    // New generation works.
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(h.generation(), new_gen);
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    drop(rt);
    assert_clean(&fake);
}

#[test]
fn reinitialize_of_healthy_generation_is_refused() {
    let (_fake, rt) = setup();
    let gen = rb(&rt).generation().unwrap();
    assert_err(rt.reinitialize(), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_eq!(rb(&rt).generation().unwrap(), gen);
}

#[test]
fn host_checkpoint_distinguishes_recoverable_from_resident_only_loss() {
    let (fake, rt) = setup();
    let values = scalars(16, 26);
    let checkpointed = rt.upload_scalars_with_checkpoint::<Toy>(&values).unwrap();
    let resident_only = rb(&rt).upload_scalars(&scalars(16, 27)).unwrap();
    assert_eq!(rt.loss_class(&checkpointed), LossClass::Live);
    assert_eq!(rt.loss_class(&resident_only), LossClass::Live);

    fake.inject(DriverCall::CopyD2H, 1, Fault::Fail(DriverErrorClass::ContextLost));
    assert_err(
        rb(&rt).download_scalars(&resident_only),
        ErrorKind::ContextLost,
        FailureEffect::SessionPoisoned,
    );
    assert_eq!(rt.loss_class(&checkpointed), LossClass::RecoverableFromHostCheckpoint);
    assert_eq!(rt.loss_class(&resident_only), LossClass::Unrecoverable);
    // The checkpoint is host data retained by the backend, not a device result.
    assert_eq!(rt.recover_checkpoint(checkpointed).unwrap(), values);
    assert_err(
        rt.recover_checkpoint(resident_only),
        ErrorKind::ContextLost,
        FailureEffect::NotStarted,
    );
    assert_clean(&fake);
}

#[test]
fn checkpoint_budget_is_bounded() {
    let (fake, rt) = setup_with(
        RuntimeConfig {
            checkpoint_budget_bytes: 64,
            ..config()
        },
        FakeConfig::default(),
    );
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    assert_err(
        rt.upload_scalars_with_checkpoint::<Toy>(&scalars(9, 28)), // 72 > 64
        ErrorKind::Capacity,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs);
    let h = rt.upload_scalars_with_checkpoint::<Toy>(&scalars(8, 28)).unwrap();
    rb(&rt).release_scalars(h).unwrap();
    // Release returns checkpoint budget.
    let h = rt.upload_scalars_with_checkpoint::<Toy>(&scalars(8, 29)).unwrap();
    rb(&rt).release_scalars(h).unwrap();
    assert_clean(&fake);
}

// --------------------------------------------------------------------------
// Metadata forgery and duplicate release
// --------------------------------------------------------------------------

#[test]
fn forged_handle_metadata_is_rejected_before_device_work() {
    let (fake, rt) = setup();
    let values = scalars(10, 30);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    let (g, a, n, b) = (h.generation(), h.allocation(), h.len(), h.bytes());
    let copies = fake.calls(DriverCall::CopyD2H);

    let forgeries: Vec<ResidentBuffer<Toy>> = vec![
        ResidentBuffer::from_untrusted_metadata(g, a, n + 1, b),
        ResidentBuffer::from_untrusted_metadata(g, a, n, b + 8),
        ResidentBuffer::from_untrusted_metadata(g, a, n - 1, b - 8),
        ResidentBuffer::from_untrusted_metadata(g, AllocationId(a.0 + 1_000), n, b),
        ResidentBuffer::from_untrusted_metadata(DeviceGeneration { device_ordinal: 3, ..g }, a, n, b),
        ResidentBuffer::from_untrusted_metadata(DeviceGeneration { context_epoch: g.context_epoch + 1, ..g }, a, n, b),
        ResidentBuffer::from_untrusted_metadata(DeviceGeneration { generation: g.generation + 1, ..g }, a, n, b),
    ];
    for forged in forgeries {
        assert_err(
            rb(&rt).download_scalars(&forged),
            ErrorKind::InvalidInput,
            FailureEffect::NotStarted,
        );
        assert_err(rt.duplicate_scalars(&forged), ErrorKind::InvalidInput, FailureEffect::NotStarted);
        assert_err(rb(&rt).release_scalars(forged), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    }
    // Element-type forgery: a scalar allocation presented as points.
    let as_points: ResidentBuffer<ToyPoint> = ResidentBuffer::from_untrusted_metadata(g, a, n, b);
    assert_err(
        rb(&rt).release_points(as_points),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::CopyD2H), copies, "no device work for forgeries");
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values, "original still valid");
    rb(&rt).release_scalars(h).unwrap();
    assert_clean(&fake);
}

#[test]
fn duplicate_release_is_rejected_and_accounted_once() {
    let (fake, rt) = setup();
    let h = rb(&rt).upload_scalars(&scalars(10, 31)).unwrap();
    let (g, a, n, b) = (h.generation(), h.allocation(), h.len(), h.bytes());
    rb(&rt).release_scalars(h).unwrap();
    assert_eq!(live_and_reserved(&rt), (0, 0));
    let again = ResidentBuffer::<Toy>::from_untrusted_metadata(g, a, n, b);
    assert_err(rb(&rt).release_scalars(again), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    let used = ResidentBuffer::<Toy>::from_untrusted_metadata(g, a, n, b);
    assert_err(rb(&rt).download_scalars(&used), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_eq!(fake.double_frees(), 0, "no double free reached the driver");
    assert_clean(&fake);
}

#[test]
fn allocation_ids_are_never_reused() {
    let (fake, rt) = setup();
    let mut seen = std::collections::HashSet::new();
    for i in 0..20 {
        let h = rb(&rt).upload_scalars(&scalars(3, 40 + i)).unwrap();
        assert!(seen.insert(h.allocation()));
        rb(&rt).release_scalars(h).unwrap();
    }
    assert_clean(&fake);
}

#[test]
fn points_have_no_generic_wire_encoding_and_are_unsupported() {
    let (fake, rt) = setup();
    assert_err(
        rb(&rt).upload_points(&[ToyPoint::default()]),
        ErrorKind::Unsupported,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), 0);
}

// --------------------------------------------------------------------------
// Lock order / admission liveness / teardown
// --------------------------------------------------------------------------

#[test]
fn admission_and_snapshot_do_not_wait_for_device_work() {
    let (fake, rt) = setup();
    let rt = Arc::new(rt);
    let (reached_tx, reached_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    fake.inject(
        DriverCall::CopyH2D,
        2,
        Fault::Pause {
            reached: reached_tx,
            resume: Arc::new(std::sync::Mutex::new(resume_rx)),
        },
    );
    let worker = {
        let rt = rt.clone();
        thread::spawn(move || {
            let h = rb(&rt).upload_scalars(&scalars(40, 50)).unwrap();
            rb(&rt).release_scalars(h).unwrap();
        })
    };
    reached_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("upload reached the paused copy");
    // The device lock is held by the paused upload; admission must still run.
    let snap = rb(&rt).runtime_snapshot().unwrap();
    assert_eq!(snap.live_bytes, 0);
    assert_eq!(snap.reserved_bytes, 320, "in-flight upload holds its reservation");
    match rb(&rt).try_reserve(8).unwrap() {
        ReservationResult::Reserved(t) => rb(&rt).release_reservation(t).unwrap(),
        ReservationResult::Rejected { .. } => panic!("budget available"),
    }
    resume_tx.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(live_and_reserved(&rt), (0, 0));
    assert_clean(&fake);
}

#[test]
fn teardown_frees_every_healthy_resource() {
    let (fake, rt) = setup();
    let _leaked_handle = rb(&rt).upload_scalars(&scalars(10, 60)).unwrap();
    assert!(fake.live_streams() >= 3, "one copy + two compute streams");
    assert!(fake.live_events() > 0);
    drop(rt);
    assert_eq!(fake.live_device_allocations(), 0);
    assert_eq!(fake.live_host_buffers(), 0);
    assert_eq!(fake.live_streams(), 0);
    assert_eq!(fake.live_events(), 0);
    assert_clean(&fake);
}

#[test]
fn streams_and_events_are_long_lived() {
    let (fake, rt) = setup();
    let streams = fake.calls(DriverCall::StreamCreate);
    let events = fake.calls(DriverCall::EventCreate);
    for i in 0..5 {
        let h = rb(&rt).upload_scalars(&scalars(30, 70 + i)).unwrap();
        let d = rt.duplicate_scalars(&h).unwrap();
        rb(&rt).download_scalars(&d).unwrap();
        rb(&rt).release_scalars(d).unwrap();
        rb(&rt).release_scalars(h).unwrap();
    }
    assert_eq!(fake.calls(DriverCall::StreamCreate), streams);
    assert_eq!(fake.calls(DriverCall::EventCreate), events);
    assert!(fake.streams_created_non_blocking());
    assert_clean(&fake);
}

#[test]
fn telemetry_separates_paths_and_reports_waits() {
    let (fake, rt) = setup();
    let h = rb(&rt).upload_scalars(&scalars(20, 80)).unwrap();
    rb(&rt).download_scalars(&h).unwrap();
    let t = rt.telemetry();
    assert_eq!(t.h2d_pinned_bytes, 160);
    assert_eq!(t.d2h_pinned_bytes, 160);
    assert_eq!(t.h2d_pageable_bytes + t.d2h_pageable_bytes, 0);
    assert!(t.transfers >= 2);
    assert!(t.init_ns > 0);
    assert_eq!(t.poison_events, 0);
    assert_clean(&fake);
}
