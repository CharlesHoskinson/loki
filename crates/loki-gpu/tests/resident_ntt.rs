//! P4 resident transform contract.
//!
//! Part 1 (always runs, no CUDA): the transactional launch path against the
//! fault-injecting fake device. The fake transform is deliberately not
//! arithmetic; these tests check reservation-before-allocation, immutable
//! source, publication only after completion, scratch retirement, every
//! injected failure class, chains, checkpoints and generation invalidation.
//! Every test ends with no recorded lifetime violation.
//!
//! Part 2 (`cuda` feature, device required unless `LOKI_GPU_ALLOW_NO_DEVICE=1`):
//! resident transform chains on the real device compared with the stock
//! `EvaluationDomain`, with readback only at the chain boundary.

mod ntt_support;

use std::sync::Arc;

use loki_backend_api::{
    BackendError, ErrorKind, FailureEffect, ResidentBackend, ResidentBuffer,
};
use loki_gpu::ntt::{transform_chain, transform_resident};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeDriver, Fault};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, LossClass, RuntimeConfig};
use ntt_support::*;

type G = midnight_curves::G1Projective;
type Rb = dyn ResidentBackend<Fr, G>;

fn rb(rt: &GpuRuntime) -> &Rb {
    rt
}

fn config(budget: u64) -> RuntimeConfig {
    RuntimeConfig {
        device_budget_cap: Some(budget),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 32 * 64,
        checkpoint_budget_bytes: 1 << 24,
        ..RuntimeConfig::default()
    }
}

fn fake_rt(budget: u64) -> (Arc<FakeDriver>, GpuRuntime) {
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    fake.enable_transform();
    let rt = GpuRuntime::with_driver(fake.clone(), config(budget));
    rt.initialize().expect("fake initializes");
    (fake, rt)
}

fn assert_err<T: std::fmt::Debug>(r: Result<T, BackendError>, kind: ErrorKind, effect: FailureEffect) {
    let e = r.expect_err("expected an error");
    assert_eq!((e.kind, e.effect), (kind, effect), "{e:?}");
    assert!(e.preserves_observable_state());
}

fn assert_clean(fake: &FakeDriver) {
    let v = fake.violations();
    assert!(v.is_empty(), "lifetime violations: {v:?}");
}

fn live_reserved(rt: &GpuRuntime) -> (u64, u64) {
    let s = rb(rt).runtime_snapshot().unwrap();
    (s.live_bytes, s.reserved_bytes)
}

/// A small valid domain: n = 16, N = 64.
fn small() -> Stock {
    Stock::new(5, 4)
}

/// Scratch bytes of a launch with extent `log` (consts + twiddles [+ work]).
fn scratch(log: u32, truncated: bool) -> u64 {
    let n = 1u64 << log;
    (72 + n / 2 + if truncated { n } else { 0 }) * 32
}

// ---------------------------------------------------------------------------
// Part 1: fake device
// ---------------------------------------------------------------------------

#[test]
fn success_publishes_fresh_handle_and_keeps_source() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Random, s.n(), 1);
    let src = rb(&rt).upload_scalars(&a).unwrap();
    let allocs_before = fake.live_device_allocations();
    let out = transform_resident(&rt, &src, s.spec_coeff_to_extended()).unwrap();
    assert_eq!(out.len(), s.big_n());
    assert_eq!(out.bytes(), (s.big_n() * 32) as u64);
    assert_ne!(out.allocation(), src.allocation(), "fresh handle");
    // scratch retired: only the output allocation was added
    assert_eq!(fake.live_device_allocations(), allocs_before + 1);
    assert_eq!(fake.transform_launches(), 1);
    assert_eq!(live_reserved(&rt), ((s.n() + s.big_n()) as u64 * 32, 0));
    // source is untouched and still readable
    assert_eq!(rb(&rt).download_scalars(&src).unwrap(), a);
    let t = rt.telemetry();
    assert_eq!(t.transform_launches, 1);
    assert_eq!(t.scratch_high_water, scratch(s.extended_k, false));
    rb(&rt).release_scalars(out).unwrap();
    rb(&rt).release_scalars(src).unwrap();
    assert_eq!(live_reserved(&rt), (0, 0));
    assert_clean(&fake);
}

#[test]
fn reservation_covers_output_and_scratch_before_allocation() {
    let s = small();
    let src_bytes = (s.n() * 32) as u64;
    let need = (s.big_n() * 32) as u64 + scratch(s.extended_k, false);
    // Budget fits the source and the output but not output + scratch.
    let (fake, rt) = fake_rt(src_bytes + need - 1);
    let src = rb(&rt).upload_scalars(&input(Pattern::One, s.n(), 0)).unwrap();
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_extended()),
        ErrorKind::Capacity,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs, "no allocation attempted");
    assert_eq!(fake.calls(DriverCall::Launch), 0);
    assert_eq!(live_reserved(&rt), (src_bytes, 0));
    // with one more byte it fits
    drop(rt);
    let (fake, rt) = fake_rt(src_bytes + need);
    let src2 = rb(&rt).upload_scalars(&input(Pattern::One, s.n(), 0)).unwrap();
    let out = transform_resident(&rt, &src2, s.spec_coeff_to_extended()).unwrap();
    rb(&rt).release_scalars(out).unwrap();
    rb(&rt).release_scalars(src2).unwrap();
    assert_clean(&fake);
    let _ = src;
}

#[test]
fn output_or_scratch_allocation_failure_is_not_started() {
    for nth in [1usize, 2] {
        let (fake, rt) = fake_rt(1 << 20);
        let s = small();
        let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 2)).unwrap();
        let live_allocs = fake.live_device_allocations();
        fake.inject(DriverCall::DeviceAlloc, nth, Fault::Fail(DriverErrorClass::Transient));
        assert_err(
            transform_resident(&rt, &src, s.spec_coeff_to_extended()),
            ErrorKind::Allocation,
            FailureEffect::NotStarted,
        );
        assert_eq!(fake.live_device_allocations(), live_allocs, "partial allocation freed (nth={nth})");
        assert_eq!(fake.calls(DriverCall::Launch), 0);
        assert_eq!(live_reserved(&rt), ((s.n() * 32) as u64, 0));
        // runtime stays healthy and usable
        let out = transform_resident(&rt, &src, s.spec_coeff_to_extended()).unwrap();
        rb(&rt).release_scalars(out).unwrap();
        assert_clean(&fake);
    }
}

#[test]
fn launch_error_after_enqueue_drains_and_preserves_inputs() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Mixed, s.n(), 3);
    let src = rb(&rt).upload_scalars(&a).unwrap();
    let live_allocs = fake.live_device_allocations();
    fake.inject(DriverCall::Launch, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_lagrange()),
        ErrorKind::Kernel,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(fake.live_device_allocations(), live_allocs, "output and scratch freed");
    assert_eq!(live_reserved(&rt), ((s.n() * 32) as u64, 0));
    assert_eq!(rb(&rt).download_scalars(&src).unwrap(), a);
    assert_eq!(rt.loss_class(&src), LossClass::Live);
    assert_clean(&fake);
}

#[test]
fn scratch_free_failure_after_drained_launch_error_poisons() {
    // Regression: a launch error is drained (dst freed, DeviceFree #1), then
    // freeing scratch (DeviceFree #2) fails. The context is untrusted, so the
    // call must report SessionPoisoned, not InputsPreserved.
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Mixed, s.n(), 11);
    let src = rb(&rt).upload_scalars(&a).unwrap();
    let live_allocs = fake.live_device_allocations();
    fake.inject(DriverCall::Launch, 1, Fault::Fail(DriverErrorClass::Transient));
    fake.inject(DriverCall::DeviceFree, 2, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_lagrange()),
        ErrorKind::Internal,
        FailureEffect::SessionPoisoned,
    );
    assert_eq!(fake.live_device_allocations(), live_allocs + 1, "scratch quarantined");
    assert!(!rt.snapshot().healthy);
    assert_eq!(live_reserved(&rt).1, 0, "reservation released");
    assert_eq!(rt.telemetry().transform_launches, 0);
}

#[test]
fn scratch_free_failure_after_success_publishes_nothing() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Random, s.n(), 12);
    let src = rb(&rt).upload_scalars(&a).unwrap();
    let live_allocs = fake.live_device_allocations();
    fake.inject(DriverCall::DeviceFree, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_extended()),
        ErrorKind::Internal,
        FailureEffect::SessionPoisoned,
    );
    // completed output and scratch both quarantined, neither published
    assert_eq!(fake.live_device_allocations(), live_allocs + 2);
    assert!(!rt.snapshot().healthy);
    assert_eq!(live_reserved(&rt).1, 0);
    assert_eq!(rt.telemetry().transform_launches, 0);
}

#[test]
fn sticky_timing_failure_poisons_and_publishes_nothing() {
    // EventRecord #1 orders the copy stream; #2 is the optional timing start.
    // A transient timing failure is ignored, a sticky one poisons.
    for (nth, class, healthy) in [
        (2, DriverErrorClass::Transient, true),
        (2, DriverErrorClass::Poisoned, false),
        (3, DriverErrorClass::Poisoned, false),
    ] {
        let (fake, rt) = fake_rt(1 << 20);
        let s = small();
        let a = input(Pattern::Random, s.n(), 13);
        let src = rb(&rt).upload_scalars(&a).unwrap();
        let live_allocs = fake.live_device_allocations();
        fake.inject(DriverCall::EventRecord, nth, Fault::Fail(class));
        let r = transform_resident(&rt, &src, s.spec_coeff_to_lagrange());
        if healthy {
            let out = r.expect("transient timing failure is optional");
            // the fake launch copies its source (no arithmetic)
            assert_eq!(rb(&rt).download_scalars(&out).unwrap(), a);
            assert!(rt.snapshot().healthy);
        } else {
            let e = r.expect_err("sticky timing failure");
            assert_eq!(e.effect, FailureEffect::SessionPoisoned, "{e:?}");
            assert!(!rt.snapshot().healthy);
            assert_eq!(fake.live_device_allocations(), live_allocs + 2, "out+scratch kept");
            assert_eq!(rt.telemetry().transform_launches, 0);
            assert_eq!(live_reserved(&rt).1, 0);
        }
    }
}

#[test]
fn completion_failure_poisons_and_publishes_nothing() {
    for class in [DriverErrorClass::Poisoned, DriverErrorClass::ContextLost] {
        let (fake, rt) = fake_rt(1 << 20);
        let s = small();
        let a = input(Pattern::Random, s.n(), 4);
        let cp = rt.upload_scalars_with_checkpoint(&a).unwrap();
        let plain_src = rb(&rt).upload_scalars(&a).unwrap();
        let live_allocs = fake.live_device_allocations();
        fake.inject(DriverCall::Launch, 1, Fault::FailAtSync(class));
        let kind = if class == DriverErrorClass::ContextLost {
            ErrorKind::ContextLost
        } else {
            ErrorKind::Synchronization
        };
        assert_err(
            transform_resident(&rt, &cp, s.spec_coeff_to_extended()),
            kind,
            FailureEffect::SessionPoisoned,
        );
        // output and scratch quarantined, not freed through the bad context
        assert_eq!(fake.live_device_allocations(), live_allocs + 2);
        assert!(!rt.snapshot().healthy);
        assert!(rt.telemetry().quarantined_allocations >= 4);
        assert_eq!(rt.telemetry().transform_launches, 0, "nothing published");
        // the checkpointed source is recoverable, the plain one is not
        assert_eq!(rt.loss_class(&cp), LossClass::RecoverableFromHostCheckpoint);
        assert_eq!(rt.loss_class(&plain_src), LossClass::Unrecoverable);
        assert_err(
            transform_resident(&rt, &plain_src, s.spec_coeff_to_extended()),
            ErrorKind::Poisoned,
            FailureEffect::SessionPoisoned,
        );
        assert_eq!(rt.recover_checkpoint(cp).unwrap(), a);
        // a new generation invalidates the old handle
        rt.reinitialize().unwrap();
        assert_err(
            transform_resident(&rt, &plain_src, s.spec_coeff_to_extended()),
            ErrorKind::ContextLost,
            FailureEffect::NotStarted,
        );
        assert_clean(&fake);
    }
}

#[test]
fn event_wait_failure_poisons() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 5)).unwrap();
    fake.inject(DriverCall::EventSync, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_lagrange()),
        ErrorKind::Synchronization,
        FailureEffect::SessionPoisoned,
    );
    assert!(!rt.snapshot().healthy);
    assert_clean(&fake);
}

#[test]
fn record_failure_before_launch_drains_cleanly() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 6)).unwrap();
    let live_allocs = fake.live_device_allocations();
    // first EventRecord of the launch: the copy-stream ordering event
    fake.inject(DriverCall::EventRecord, 1, Fault::Fail(DriverErrorClass::Transient));
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_lagrange()),
        ErrorKind::Synchronization,
        FailureEffect::InputsPreserved,
    );
    assert_eq!(fake.live_device_allocations(), live_allocs);
    assert_eq!(fake.calls(DriverCall::Launch), 0);
    assert!(rt.snapshot().healthy);
    assert_clean(&fake);
}

#[test]
fn driver_without_transform_is_unsupported_without_driver_calls() {
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    let rt = GpuRuntime::with_driver(fake.clone(), config(1 << 20));
    rt.initialize().unwrap();
    let s = small();
    let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 7)).unwrap();
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    assert_err(
        transform_resident(&rt, &src, s.spec_coeff_to_lagrange()),
        ErrorKind::Unsupported,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs);
    let caps = rt.capabilities();
    assert!(caps.resident_buffers && !caps.resident_transform && !caps.host_transform);
    assert!(caps.supported_arithmetic.is_empty());
    assert_clean(&fake);
}

#[test]
fn transform_capability_is_advertised_only_when_healthy() {
    let (fake, rt) = fake_rt(1 << 20);
    let caps = rt.capabilities();
    assert!(caps.host_transform && caps.resident_transform);
    assert_eq!(caps.max_transform_log, Some(loki_gpu::ntt::MAX_TRANSFORM_LOG));
    let s = small();
    let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 8)).unwrap();
    fake.inject(DriverCall::Launch, 1, Fault::FailAtSync(DriverErrorClass::Poisoned));
    let _ = transform_resident(&rt, &src, s.spec_coeff_to_lagrange());
    let caps = rt.capabilities();
    assert!(!caps.host_transform && !caps.resident_transform && !caps.resident_buffers);
    assert_clean(&fake);
}

#[test]
fn malformed_requests_and_handles_touch_no_device() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let src = rb(&rt).upload_scalars(&input(Pattern::Random, s.n(), 9)).unwrap();
    let allocs = fake.calls(DriverCall::DeviceAlloc);
    // wrong length for the spec (source is n, spec expects N)
    assert_err(
        transform_resident(&rt, &src, s.spec_extended_to_coeff()),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    // forged handle metadata
    let forged = ResidentBuffer::<Fr>::from_untrusted_metadata(
        src.generation(),
        src.allocation(),
        src.len(),
        src.bytes() + 32,
    );
    assert_err(
        transform_resident(&rt, &forged, s.spec_coeff_to_lagrange()),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    let unknown = ResidentBuffer::<Fr>::from_untrusted_metadata(
        src.generation(),
        loki_backend_api::AllocationId(u64::MAX),
        src.len(),
        src.bytes(),
    );
    assert_err(
        transform_resident(&rt, &unknown, s.spec_coeff_to_lagrange()),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    assert_eq!(fake.calls(DriverCall::DeviceAlloc), allocs);
    assert_eq!(fake.calls(DriverCall::Launch), 0);
    assert_clean(&fake);
}

#[test]
fn chain_releases_intermediates_and_keeps_source() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Random, s.big_n(), 10);
    let src = rb(&rt).upload_scalars(&a).unwrap();
    let live_allocs = fake.live_device_allocations();
    let specs = [s.spec_extended_to_coeff_prefix(s.k), s.spec_coeff_to_lagrange()];
    let out = transform_chain(&rt, &src, &specs).unwrap();
    assert_eq!(out.len(), s.n());
    assert_eq!(fake.transform_launches(), 2);
    assert_eq!(fake.live_device_allocations(), live_allocs + 1, "intermediate released");
    assert_eq!(live_reserved(&rt), ((s.big_n() + s.n()) as u64 * 32, 0));
    assert_eq!(rt.telemetry().scratch_high_water, scratch(s.extended_k, true));
    assert_eq!(rb(&rt).download_scalars(&src).unwrap(), a);
    rb(&rt).release_scalars(out).unwrap();
    // failure in the second step: intermediate released, nothing published
    fake.inject(DriverCall::Launch, 2, Fault::Fail(DriverErrorClass::Transient));
    assert_err(transform_chain(&rt, &src, &specs), ErrorKind::Kernel, FailureEffect::InputsPreserved);
    assert_eq!(fake.live_device_allocations(), live_allocs);
    assert_eq!(live_reserved(&rt), ((s.big_n() * 32) as u64, 0));
    // an invalid later step is rejected before the first launch
    let launches = fake.calls(DriverCall::Launch);
    // step 2 expects N elements but step 1 produces n
    let bad = [s.spec_extended_to_coeff_prefix(s.k), s.spec_extended_to_coeff()];
    assert_err(transform_chain(&rt, &src, &bad), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_eq!(fake.calls(DriverCall::Launch), launches);
    assert_err(transform_chain(&rt, &src, &[]), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    rb(&rt).release_scalars(src).unwrap();
    assert_eq!(live_reserved(&rt), (0, 0));
    assert_clean(&fake);
}

#[test]
fn checkpointed_chain_source_survives_mid_chain_poison() {
    let (fake, rt) = fake_rt(1 << 20);
    let s = small();
    let a = input(Pattern::Random, s.big_n(), 11);
    let src = rt.upload_scalars_with_checkpoint(&a).unwrap();
    let specs = [s.spec_extended_to_coeff_prefix(s.k), s.spec_coeff_to_lagrange()];
    fake.inject(DriverCall::Launch, 2, Fault::FailAtSync(DriverErrorClass::Poisoned));
    assert_err(
        transform_chain(&rt, &src, &specs),
        ErrorKind::Synchronization,
        FailureEffect::SessionPoisoned,
    );
    // device-only intermediate is gone; the host checkpoint is the
    // recoverable phase, and stock CPU can recompute from it.
    assert_eq!(rt.loss_class(&src), LossClass::RecoverableFromHostCheckpoint);
    let host = rt.recover_checkpoint(src).unwrap();
    assert_eq!(host, a);
    assert_vec_eq("cpu fallback", &s.extended_to_lagrange(&host), &s.extended_to_lagrange(&a));
    assert_clean(&fake);
}

// ---------------------------------------------------------------------------
// Part 2: native resident chains
// ---------------------------------------------------------------------------

#[cfg(feature = "cuda")]
mod native {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static DEVICE: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        DEVICE.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn native() -> Option<GpuRuntime> {
        // Tests hold `DEVICE`, so at most one claim is live; a healthy drop
        // releases it for the next test.
        let rt = GpuRuntime::native(RuntimeConfig::default()).expect("native driver claim");
        match rt.initialize() {
            Ok(_) => Some(rt),
            Err(e) => {
                if std::env::var("LOKI_GPU_ALLOW_NO_DEVICE").as_deref() != Ok("1") {
                    panic!("device required (set LOKI_GPU_ALLOW_NO_DEVICE=1 to skip): {e:?}");
                }
                None
            }
        }
    }

    fn d2h(rt: &GpuRuntime) -> u64 {
        let t = rt.telemetry();
        t.d2h_pinned_bytes + t.d2h_pageable_bytes
    }

    fn h2d(rt: &GpuRuntime) -> u64 {
        let t = rt.telemetry();
        t.h2d_pinned_bytes + t.h2d_pageable_bytes
    }

    /// Lagrange -> coeff -> extended -> (quotient-style pointwise step is P5)
    /// -> coeff -> Lagrange, all resident; one upload and one download.
    #[test]
    fn resident_round_chain_matches_stock_with_boundary_readback_only() {
        let _g = lock();
        let Some(rt) = native() else { return };
        for (j, k) in [(3u32, 6u32), (5, 12), (5, 16)] {
            let s = Stock::new(j, k);
            let lag = input(Pattern::Random, s.n(), 100 + k as u64);
            let (h0, d0) = (h2d(&rt), d2h(&rt));
            let src = rt.upload_scalars_with_checkpoint(&lag).unwrap();
            let coeff = transform_resident(&rt, &src, s.spec_lagrange_to_coeff()).unwrap();
            let ext = transform_resident(&rt, &coeff, s.spec_coeff_to_extended()).unwrap();
            let back = transform_chain(
                &rt,
                &ext,
                &[s.spec_extended_to_coeff_prefix(s.k), s.spec_coeff_to_lagrange()],
            )
            .unwrap();
            // explicit boundary readbacks: the extended values and the result
            let ext_host = rb(&rt).download_scalars(&ext).unwrap();
            let back_host = rb(&rt).download_scalars(&back).unwrap();
            assert_eq!(h2d(&rt) - h0, (s.n() * 32) as u64, "one upload");
            assert_eq!(d2h(&rt) - d0, ((s.big_n() + s.n()) * 32) as u64, "two boundary readbacks");
            let want_coeff = s.lagrange_to_coeff(&lag);
            assert_vec_eq("ext", &ext_host, &s.coeff_to_extended(&want_coeff));
            assert_vec_eq("round", &back_host, &lag);
            assert_vec_eq("stock e2l", &back_host, &s.extended_to_lagrange(&ext_host));
            // source unchanged on the device and its checkpoint retained
            assert_eq!(rt.loss_class(&src), LossClass::Live);
            assert_vec_eq("source", &rb(&rt).download_scalars(&src).unwrap(), &lag);
            for b in [back, ext, coeff, src] {
                rb(&rt).release_scalars(b).unwrap();
            }
            assert_eq!(live_reserved(&rt), (0, 0));
        }
    }

    #[test]
    fn repeated_transforms_have_bounded_resident_state() {
        let _g = lock();
        let Some(rt) = native() else { return };
        let s = Stock::new(5, 14);
        let a = input(Pattern::Random, s.n(), 7);
        let src = rb(&rt).upload_scalars(&a).unwrap();
        let mut high = 0;
        for i in 0..20 {
            let out = transform_resident(&rt, &src, s.spec_coeff_to_extended()).unwrap();
            rb(&rt).release_scalars(out).unwrap();
            let t = rt.telemetry();
            if i == 0 {
                high = t.live_high_water;
            }
            assert_eq!(t.live_high_water, high, "no growth across calls");
            assert_eq!(live_reserved(&rt), ((s.n() * 32) as u64, 0));
        }
        rb(&rt).release_scalars(src).unwrap();
    }

    /// Cost breakdown on synthetic inputs (never private witnesses). Run with
    /// `--release -- --ignored --nocapture measure_`. Reports, per operation:
    /// first and median-of-5 warm full host calls (upload + launch + download),
    /// host canonical conversion (to_repr / from_repr), and GPU-event times
    /// for H2D, the transform launch and D2H.
    #[test]
    #[ignore]
    fn measure_transform_costs_on_synthetic_inputs() {
        use ff::PrimeField;
        use loki_backend_api::TransformSpec;
        use loki_gpu::ntt::transform_host;
        use std::time::Instant;
        let _g = lock();
        let Some(rt) = native() else { return };
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1e3;
        println!("op,k,ext_k,first_ms,warm_med_ms,to_repr_ms,from_repr_ms,h2d_ms,kernel_ms,d2h_ms,scratch_mib");
        for (j, k) in [(3u32, 16u32), (3, 19), (1, 20), (3, 20)] {
            let s = Stock::new(j, k);
            let ops: [(&str, usize, TransformSpec<Fr>); 4] = [
                ("coeff_to_lagrange", s.n(), s.spec_coeff_to_lagrange()),
                ("lagrange_to_coeff", s.n(), s.spec_lagrange_to_coeff()),
                ("coeff_to_extended", s.n(), s.spec_coeff_to_extended()),
                ("extended_to_coeff", s.big_n(), s.spec_extended_to_coeff()),
            ];
            for (name, len, spec) in ops {
                let x = input(Pattern::Random, len, 7 + k as u64);
                let t = Instant::now();
                let reprs: Vec<_> = x.iter().map(|v| v.to_repr()).collect();
                let to_repr = ms(t);
                let t = Instant::now();
                let back: Vec<Fr> = reprs.iter().map(|r| Fr::from_repr(*r).unwrap()).collect();
                let from_repr = ms(t);
                assert_eq!(back.len(), len);
                let t = Instant::now();
                let _ = transform_host(&rt, &x, spec).unwrap();
                let first = ms(t);
                let mut warm = Vec::new();
                for _ in 0..5 {
                    let t = Instant::now();
                    let _ = transform_host(&rt, &x, spec).unwrap();
                    warm.push(ms(t));
                }
                warm.sort_by(|a, b| a.partial_cmp(b).unwrap());
                // resident breakdown (event timings)
                let e0 = rt.telemetry().gpu_event_ms;
                let src = rb(&rt).upload_scalars(&x).unwrap();
                let e1 = rt.telemetry();
                let out = transform_resident(&rt, &src, spec).unwrap();
                let e2 = rt.telemetry();
                let _ = rb(&rt).download_scalars(&out).unwrap();
                let e3 = rt.telemetry().gpu_event_ms;
                rb(&rt).release_scalars(src).unwrap();
                rb(&rt).release_scalars(out).unwrap();
                let kernel = e2.transform_kernel_ms - e1.transform_kernel_ms;
                println!(
                    "{name},{k},{},{first:.3},{:.3},{to_repr:.3},{from_repr:.3},{:.3},{kernel:.3},{:.3},{:.1}",
                    k + j,
                    warm[2],
                    e1.gpu_event_ms - e0,
                    e3 - e2.gpu_event_ms,
                    e2.scratch_high_water as f64 / (1u64 << 20) as f64,
                );
            }
            assert_eq!(live_reserved(&rt), (0, 0));
        }
    }
}
