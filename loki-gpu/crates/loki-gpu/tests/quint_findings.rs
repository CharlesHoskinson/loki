//! quint-q3 findings (branch `quint-runtime` 6873b45, `quint/REPORT.md`),
//! adopted as regressions by P4 (fake driver, no GPU).
//!
//! Tests named `bug_*` assert the runtime's documented contract; on
//! `eb27233` they failed (review findings L1/L2) and they pass after the P4
//! corrections. BUG 2 can no longer be written in safe code (`with_driver` is
//! `unsafe` with an exclusivity precondition), so it is restated over the
//! safe `with_fake` constructor. Tests named `known_*` pin the two leaks P2
//! documented, so a future fix shows up as a deliberate change of these
//! expectations (see `design/p4/REVIEW.md`, open risks).
//!
//! Run: `cargo test -p loki-gpu --test quint_findings`.

mod support;

use loki_backend_api::{ErrorKind, FailureEffect, ResidentBackend, ResidentBuffer};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeControl, Fault, Violation};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, LossClass, RuntimeConfig};
use support::{scalars, Toy, ToyPoint};

type Rb = dyn ResidentBackend<Toy, ToyPoint>;

fn config() -> RuntimeConfig {
    RuntimeConfig {
        device_budget_cap: Some(1 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 64,
        checkpoint_budget_bytes: 1 << 20,
        ..RuntimeConfig::default()
    }
}

fn rb(rt: &GpuRuntime) -> &Rb {
    rt
}

/// Rebuild a handle from its (untrusted) metadata; `ResidentBuffer` is not `Clone`.
fn copy_of<T>(h: &ResidentBuffer<T>) -> ResidentBuffer<T> {
    ResidentBuffer::from_untrusted_metadata(h.generation(), h.allocation(), h.len(), h.bytes())
}

/// Poison the runtime's current generation through a sticky H2D failure.
fn poison(fake: &FakeControl, rt: &GpuRuntime) {
    fake.inject(DriverCall::CopyH2D, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let e = rb(rt).upload_scalars(&scalars(3, 1)).unwrap_err();
    assert_eq!(e.effect, FailureEffect::SessionPoisoned);
    assert!(!rt.snapshot().healthy);
}

/// Call `reinitialize` repeatedly, as a scheduler that sees an unhealthy
/// runtime would, and report whether the runtime ever became healthy.
fn try_recover(rt: &GpuRuntime, attempts: usize) -> bool {
    (0..attempts).any(|_| rt.reinitialize().is_ok() && rt.snapshot().healthy)
}

// ---------------------------------------------------------------------------
// BUG 1: a sticky failure while building a context strands the runtime.
// runtime.rs:851/884 (build_context error leaves `current = None`),
// :866-869 (`None` => initialize, which only probes), :849 (probe), :939-947
// (sticky class reported as NotStarted), :1997-2004 (Drop releases the native
// claim when `current` is None; inspection only, needs a real sticky fault).
// ---------------------------------------------------------------------------

#[test]
fn bug_sticky_failure_in_first_setup_is_never_reset() {
    let (fake, rt) = GpuRuntime::with_fake(FakeConfig::default(), config());
    fake.inject(DriverCall::EventCreate, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let first = rt.initialize().unwrap_err();
    // The device is now sticky. The contract says completion/health is
    // uncertain, so the effect must not claim that nothing changed...
    let sticky_reported = first.effect == FailureEffect::SessionPoisoned
        || matches!(first.kind, ErrorKind::Poisoned | ErrorKind::ContextLost);
    // ...and the documented recovery (`reinitialize`, which resets) must work.
    let recovered = try_recover(&rt, 4);
    assert!(
        sticky_reported && recovered,
        "sticky setup failure: first={first:?}, recovered={recovered}, \
         reset calls={} (reinitialize routes to probe-only initialize forever)",
        fake.calls(DriverCall::Reset)
    );
}

#[test]
fn bug_sticky_failure_in_post_reset_setup_is_never_reset_again() {
    let (fake, rt) = GpuRuntime::with_fake(FakeConfig::default(), config());
    rt.initialize().unwrap();
    poison(&fake, &rt);
    // Reset succeeds, then the rebuild hits a sticky failure.
    fake.inject(DriverCall::StreamCreate, 1, Fault::Fail(DriverErrorClass::Poisoned));
    let first = rt.reinitialize().unwrap_err();
    // Scheduler-visible state: a poisoned generation that asks for
    // reinitialization...
    let snap = rt.snapshot();
    assert!(snap.generation.is_some() && !snap.healthy);
    // ...but reinitialization can never reset again.
    let resets_before = fake.calls(DriverCall::Reset);
    let recovered = try_recover(&rt, 4);
    assert!(
        recovered,
        "post-reset sticky setup failure: first={first:?}, later reinitialize={:?}, \
         reset calls {} -> {}",
        rt.reinitialize(),
        resets_before,
        fake.calls(DriverCall::Reset)
    );
}

// ---------------------------------------------------------------------------
// BUG 2: two runtimes over one driver. Original reproduction: two safe
// `with_driver` runtimes shared one fake; one runtime's reset destroyed the
// other's context, which kept reporting Live/healthy on stale handles. Fixed
// by making `with_driver` `unsafe` (sole-coordinator precondition); the safe
// constructor gives each runtime its own driver, and resetting one leaves the
// other untouched.
// ---------------------------------------------------------------------------

#[test]
fn bug_shared_driver_reset_leaves_other_runtime_live_on_stale_handles() {
    let (fa, a) = GpuRuntime::with_fake(FakeConfig::default(), config());
    let (fb, b) = GpuRuntime::with_fake(FakeConfig::default(), config());
    a.initialize().unwrap();
    b.initialize().unwrap();
    let hb = rb(&b).upload_scalars(&scalars(3, 2)).unwrap();
    poison(&fa, &a);
    a.reinitialize().expect("A resets its own driver");
    assert_eq!(fb.calls(DriverCall::Reset), 0, "B's driver was not reset");
    assert_eq!(b.loss_class(&hb), LossClass::Live);
    assert!(b.snapshot().healthy);
    assert_eq!(rb(&b).download_scalars(&hb).unwrap(), scalars(3, 2));
    let stale = |v: &[Violation]| v.iter().any(|v| matches!(v, Violation::StaleEpochUse { .. }));
    assert!(!stale(&fa.violations()) && !stale(&fb.violations()));
}

// ---------------------------------------------------------------------------
// Known leaks documented by P2 (design/p2/REVIEW.md:394-400). They still pass:
// the leaks are open risks, not fixed by P4.
// ---------------------------------------------------------------------------

/// Checkpoint bytes of a poisoned generation stay charged across reinit
/// until `recover_checkpoint` (build_context :1028-1035 never resets
/// `checkpoint_bytes`; release_impl rejects poisoned/stale handles).
#[test]
fn known_checkpoint_bytes_survive_poison_and_reinit_until_recovered() {
    let cfg = RuntimeConfig {
        checkpoint_budget_bytes: 8 * 4,
        ..config()
    };
    let (fake, rt) = GpuRuntime::with_fake(FakeConfig::default(), cfg);
    rt.initialize().unwrap();
    let h = rt.upload_scalars_with_checkpoint(&scalars(4, 3)).unwrap();
    poison(&fake, &rt);
    assert_eq!(rb(&rt).release_scalars(copy_of(&h)).unwrap_err().kind, ErrorKind::Poisoned);
    rt.reinitialize().unwrap();
    assert_eq!(rb(&rt).release_scalars(copy_of(&h)).unwrap_err().kind, ErrorKind::ContextLost);
    // The new healthy generation cannot checkpoint anything: budget leaked.
    let e = rt.upload_scalars_with_checkpoint(&scalars(1, 4)).unwrap_err();
    assert_eq!((e.kind, e.effect), (ErrorKind::Capacity, FailureEffect::NotStarted));
    // Only recovery returns the bytes.
    assert_eq!(rt.recover_checkpoint(h).unwrap().len(), 4);
    rt.upload_scalars_with_checkpoint(&scalars(4, 5)).unwrap();
    assert!(fake.violations().is_empty());
}

/// Pageable staging of a reset context is never `host_free`d: reinitialize
/// moves the old context to `retired` (:881-883) and Drop clears `retired`
/// (:1996) without freeing. NativeDriver::reset forgets every minted handle
/// (ffi.rs `reset`: `self.set().clear()`), so it could not be freed later
/// either; `cudaDeviceReset` does not free `malloc` memory.
#[test]
fn known_pageable_slots_of_a_reset_context_are_never_freed() {
    let cfg = RuntimeConfig {
        pinned_staging: false,
        ..config()
    };
    let fake: FakeControl;
    {
        let (control, rt) = GpuRuntime::with_fake(FakeConfig::default(), cfg);
        fake = control.clone();
        rt.initialize().unwrap();
        poison(&fake, &rt);
        rt.reinitialize().unwrap();
        assert_eq!(rt.staging_capability().unwrap().pageable_slots, 2);
    }
    // Healthy drop freed the 2 slots of the new context only; the 2 pageable
    // slots of the reset context were never freed.
    assert_eq!(fake.calls(DriverCall::HostFree), 2);
    assert_eq!(fake.calls(DriverCall::HostAlloc), 4);
    assert!(fake.violations().is_empty());
}
