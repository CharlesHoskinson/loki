//! P7 scheduler contract: one-heavy FIFO admission over the P2 reservation API.
//!
//! These tests drive [`loki_gpu::schedule::HeavyScheduler`] against the
//! fault-injecting fake device runtime (`GpuRuntime::with_driver`) and against
//! recording fakes for host locks and reservations. No CUDA toolkit or GPU is
//! required.
//!
//! Contract under test (RECURSION.md "Admission and shared-device policy"):
//! * FIFO among ready heavy jobs, at most one admitted heavy job.
//! * Queued jobs hold no device reservation, lease or host lock.
//! * Admission takes the heavy host lock, then the GPU host lock, then the
//!   non-blocking device reservation; release runs in reverse.
//! * Budget rejection defers immediately, releases everything and keeps the
//!   job's queue position.
//! * Completion, failure, cancellation and unwinding all release; a
//!   reservation from an older device generation is rejected as stale but
//!   still frees the heavy slot.
//! * DAG readiness: unknown dependencies are refused, pending dependencies
//!   wait, a wrong child VK fails the dependent, and a failed or cancelled
//!   child makes descendants unavailable.

mod support;

use std::sync::{Arc, Mutex};
use std::thread;

use loki_backend_api::{
    BackendError, DeviceGeneration, ErrorKind, FailureEffect, ReservationId, ReservationResult,
    ReservationToken, ResidentBackend, RuntimeSnapshot,
};
use loki_gpu::runtime::fake::{DriverCall, FakeDriver, Fault};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, RuntimeConfig};
use loki_gpu::schedule::{
    Admission, ArtifactIdentity, Dependency, HeavyScheduler, HostLock, HostLockGuard, HostLocks,
    JobId, JobSpec, JobState, ReleaseOutcome, ReservationSource, ResidentReservations, Role,
    ScheduleError, SchedulerConfig, WaitReason,
};
use support::{scalars, Toy, ToyPoint};

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

fn fake_runtime() -> (Arc<FakeDriver>, Arc<GpuRuntime>) {
    let fake = Arc::new(FakeDriver::new(Default::default()));
    let rt = GpuRuntime::with_driver(
        fake.clone(),
        RuntimeConfig {
            device_budget_cap: Some(1 << 20),
            device_headroom_bytes: 0,
            staging_slots: 2,
            staging_slot_bytes: 64,
            checkpoint_budget_bytes: 1 << 20,
            ..RuntimeConfig::default()
        },
    );
    rt.initialize().expect("fake device initializes");
    (fake, Arc::new(rt))
}

type Source = ResidentReservations<Toy, ToyPoint, Arc<GpuRuntime>>;

fn device_source(rt: &Arc<GpuRuntime>) -> Source {
    ResidentReservations::new(rt.clone())
}

fn reserved(rt: &GpuRuntime) -> u64 {
    ResidentBackend::<Toy, ToyPoint>::runtime_snapshot(rt)
        .unwrap()
        .reserved_bytes
}

/// Host locks that are always free and record every acquire/release.
#[derive(Clone, Default)]
struct RecordingLocks {
    log: Arc<Mutex<Vec<String>>>,
    busy: Arc<Mutex<Vec<HostLock>>>,
}

impl RecordingLocks {
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
    fn set_busy(&self, lock: HostLock, busy: bool) {
        let mut b = self.busy.lock().unwrap();
        b.retain(|l| *l != lock);
        if busy {
            b.push(lock);
        }
    }
}

impl HostLocks for RecordingLocks {
    fn try_acquire(&self, lock: HostLock) -> std::io::Result<Option<HostLockGuard>> {
        if self.busy.lock().unwrap().contains(&lock) {
            self.log.lock().unwrap().push(format!("busy {lock:?}"));
            return Ok(None);
        }
        self.log.lock().unwrap().push(format!("acquire {lock:?}"));
        let log = self.log.clone();
        Ok(Some(HostLockGuard::new(lock, move || {
            log.lock().unwrap().push(format!("release {lock:?}"));
        })))
    }
}

/// Reservation source that records calls into the same log as the locks.
struct RecordingSource {
    log: Arc<Mutex<Vec<String>>>,
    available: Mutex<u64>,
    next: Mutex<u64>,
}

impl RecordingSource {
    fn new(log: Arc<Mutex<Vec<String>>>, available: u64) -> Self {
        Self {
            log,
            available: Mutex::new(available),
            next: Mutex::new(1),
        }
    }
}

const GEN: DeviceGeneration = DeviceGeneration {
    device_ordinal: 0,
    context_epoch: 1,
    generation: 1,
};

impl ReservationSource for RecordingSource {
    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError> {
        Ok(RuntimeSnapshot {
            generation: GEN,
            context_healthy: true,
            budget_bytes: 1 << 20,
            live_bytes: 0,
            reserved_bytes: 0,
        })
    }
    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError> {
        self.log.lock().unwrap().push(format!("reserve {bytes}"));
        let available = *self.available.lock().unwrap();
        if bytes > available {
            return Ok(ReservationResult::Rejected {
                requested_bytes: bytes,
                available_bytes: available,
            });
        }
        let mut next = self.next.lock().unwrap();
        *next += 1;
        Ok(ReservationResult::Reserved(
            ReservationToken::from_untrusted_metadata(ReservationId(*next), GEN, bytes),
        ))
    }
    fn release_reservation(&self, token: ReservationToken) -> Result<(), BackendError> {
        self.log
            .lock()
            .unwrap()
            .push(format!("release-reservation {}", token.bytes()));
        Ok(())
    }
}

fn gpu_job(id: u64, bytes: u64) -> JobSpec {
    JobSpec::new(JobId(id), Role::Leaf).gpu(bytes)
}

fn cpu_job(id: u64) -> JobSpec {
    JobSpec::new(JobId(id), Role::Leaf)
}

fn identity(vk: &str) -> ArtifactIdentity {
    ArtifactIdentity {
        vk_sha256: vk.into(),
        proof_sha256: format!("proof-of-{vk}"),
        instance_sha256: format!("instance-of-{vk}"),
    }
}

fn admitted<S: ReservationSource, L: HostLocks>(
    a: Result<Admission<S, L>, ScheduleError>,
) -> loki_gpu::schedule::HeavyPermit<S, L> {
    match a {
        Ok(Admission::Admitted(p)) => p,
        Ok(Admission::Waiting(r)) => panic!("expected admission, waiting: {r:?}"),
        Ok(Admission::Deferred {
            requested_bytes,
            available_bytes,
        }) => panic!("expected admission, deferred {requested_bytes}/{available_bytes}"),
        Err(e) => panic!("expected admission, error {e:?}"),
    }
}

fn waiting<S: ReservationSource, L: HostLocks>(
    a: Result<Admission<S, L>, ScheduleError>,
) -> WaitReason {
    match a {
        Ok(Admission::Waiting(r)) => r,
        Ok(Admission::Admitted(_)) => panic!("expected waiting, got admission"),
        Ok(Admission::Deferred { .. }) => panic!("expected waiting, got deferred"),
        Err(e) => panic!("expected waiting, error {e:?}"),
    }
}

// --------------------------------------------------------------------------
// One-heavy FIFO admission
// --------------------------------------------------------------------------

#[test]
fn admission_is_fifo_and_admits_at_most_one_heavy_job() {
    let (fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 64)).unwrap();
    s.submit(gpu_job(2, 64)).unwrap();
    s.submit(gpu_job(3, 64)).unwrap();

    // Out-of-order requests wait behind the head.
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::QueuedBehind { position: 1 }
    );
    assert_eq!(
        waiting(s.try_admit(JobId(3))),
        WaitReason::QueuedBehind { position: 2 }
    );

    let p1 = admitted(s.try_admit(JobId(1)));
    assert_eq!(s.snapshot().active, Some(JobId(1)));
    // Next in line still waits: one heavy job at a time.
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::HeavyBusy { active: JobId(1) }
    );
    assert_eq!(
        waiting(s.try_admit(JobId(3))),
        WaitReason::QueuedBehind { position: 1 }
    );

    p1.complete(identity("leaf")).unwrap();
    assert_eq!(s.snapshot().active, None);
    assert_eq!(
        waiting(s.try_admit(JobId(3))),
        WaitReason::QueuedBehind { position: 1 }
    );
    let p2 = admitted(s.try_admit(JobId(2)));
    p2.complete(identity("leaf2")).unwrap();
    admitted(s.try_admit(JobId(3)))
        .complete(identity("leaf3"))
        .unwrap();
    assert_eq!(s.snapshot().order, vec![JobId(1), JobId(2), JobId(3)]);
    assert_eq!(reserved(&rt), 0);
    assert!(fake.violations().is_empty());
}

#[test]
fn queued_jobs_hold_no_reservation_or_lease() {
    let (_fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    for id in 1..=4 {
        s.submit(gpu_job(id, 1000 * id)).unwrap();
    }
    assert_eq!(
        reserved(&rt),
        0,
        "submission must not reserve device memory"
    );
    let snap = s.snapshot();
    assert_eq!(snap.queued, vec![JobId(1), JobId(2), JobId(3), JobId(4)]);
    assert_eq!(snap.held_reservations, 0);
    assert_eq!(snap.held_host_locks, 0);

    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(
        reserved(&rt),
        1000,
        "only the admitted head holds a reservation"
    );
    let snap = s.snapshot();
    assert_eq!(snap.held_reservations, 1);
    assert_eq!(snap.queued, vec![JobId(2), JobId(3), JobId(4)]);
    for id in 2..=4 {
        waiting(s.try_admit(JobId(id)));
    }
    assert_eq!(reserved(&rt), 1000, "waiting requests never reserve");
    assert_eq!(p.reservation_bytes(), Some(1000));
    drop(p);
    assert_eq!(reserved(&rt), 0);
}

#[test]
fn snapshot_reports_runtime_generation_and_budget() {
    let (_fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    let snap = s.snapshot();
    let runtime = snap.runtime.expect("fake runtime is initialized");
    assert!(runtime.context_healthy);
    assert_eq!(runtime.budget_bytes, 1 << 20);
    assert_eq!(runtime.reserved_bytes, 0);
}

// --------------------------------------------------------------------------
// Lock order
// --------------------------------------------------------------------------

#[test]
fn admission_takes_heavy_then_gpu_then_reservation_and_releases_in_reverse() {
    let locks = RecordingLocks::default();
    let src = RecordingSource::new(locks.log.clone(), 1 << 20);
    let s = HeavyScheduler::new(src, locks.clone(), SchedulerConfig::default());
    s.submit(gpu_job(1, 4096)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(
        locks.log(),
        ["acquire Heavy", "acquire Gpu", "reserve 4096"]
    );
    locks.clear();
    p.complete(identity("leaf")).unwrap();
    assert_eq!(
        locks.log(),
        ["release-reservation 4096", "release Gpu", "release Heavy"]
    );
}

#[test]
fn cpu_only_job_takes_only_the_heavy_lock() {
    let locks = RecordingLocks::default();
    let src = RecordingSource::new(locks.log.clone(), 1 << 20);
    let s = HeavyScheduler::new(src, locks.clone(), SchedulerConfig::default());
    s.submit(cpu_job(1)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(locks.log(), ["acquire Heavy"]);
    assert_eq!(p.reservation_bytes(), None);
    locks.clear();
    drop(p);
    assert_eq!(locks.log(), ["release Heavy"]);
}

#[test]
fn busy_gpu_lock_releases_heavy_and_keeps_queue_position() {
    let locks = RecordingLocks::default();
    let src = RecordingSource::new(locks.log.clone(), 1 << 20);
    let s = HeavyScheduler::new(src, locks.clone(), SchedulerConfig::default());
    s.submit(gpu_job(1, 64)).unwrap();
    s.submit(gpu_job(2, 64)).unwrap();
    locks.set_busy(HostLock::Gpu, true);
    assert_eq!(
        waiting(s.try_admit(JobId(1))),
        WaitReason::HostLockBusy(HostLock::Gpu)
    );
    assert_eq!(
        locks.log(),
        ["acquire Heavy", "busy Gpu", "release Heavy"],
        "a busy GPU lock must not leave heavy held, and must not reserve"
    );
    assert_eq!(s.snapshot().active, None);
    assert_eq!(s.snapshot().queued, vec![JobId(1), JobId(2)]);
    locks.set_busy(HostLock::Gpu, false);
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::QueuedBehind { position: 1 }
    );
    admitted(s.try_admit(JobId(1)));
}

#[test]
fn busy_heavy_lock_is_reported_without_touching_gpu() {
    let locks = RecordingLocks::default();
    let src = RecordingSource::new(locks.log.clone(), 1 << 20);
    let s = HeavyScheduler::new(src, locks.clone(), SchedulerConfig::default());
    s.submit(gpu_job(1, 64)).unwrap();
    locks.set_busy(HostLock::Heavy, true);
    assert_eq!(
        waiting(s.try_admit(JobId(1))),
        WaitReason::HostLockBusy(HostLock::Heavy)
    );
    assert_eq!(locks.log(), ["busy Heavy"]);
}

// --------------------------------------------------------------------------
// Budget deferral
// --------------------------------------------------------------------------

#[test]
fn budget_rejection_defers_immediately_and_releases_host_locks() {
    let (_fake, rt) = fake_runtime();
    let locks = RecordingLocks::default();
    let s = HeavyScheduler::new(
        device_source(&rt),
        locks.clone(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 2 << 20)).unwrap(); // larger than the 1 MiB fake budget
    s.submit(gpu_job(2, 64)).unwrap();
    match s.try_admit(JobId(1)).unwrap() {
        Admission::Deferred {
            requested_bytes,
            available_bytes,
        } => {
            assert_eq!(requested_bytes, 2 << 20);
            assert!(available_bytes < requested_bytes);
        }
        _ => panic!("oversized job must be deferred"),
    }
    assert_eq!(
        locks.log(),
        [
            "acquire Heavy",
            "acquire Gpu",
            "release Gpu",
            "release Heavy"
        ]
    );
    assert_eq!(reserved(&rt), 0);
    let snap = s.snapshot();
    assert_eq!(snap.active, None);
    assert_eq!(
        snap.queued,
        vec![JobId(1), JobId(2)],
        "deferred head keeps FIFO position"
    );
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::QueuedBehind { position: 1 }
    );
}

#[test]
fn unhealthy_runtime_refuses_admission_without_holding_locks() {
    let (fake, rt) = fake_runtime();
    // Poison the generation through an injected sticky failure.
    fake.inject(
        DriverCall::DeviceAlloc,
        1,
        Fault::Fail(DriverErrorClass::Poisoned),
    );
    assert!(ResidentBackend::<Toy, ToyPoint>::upload_scalars(&*rt, &scalars(4, 1)).is_err());
    let locks = RecordingLocks::default();
    let s = HeavyScheduler::new(
        device_source(&rt),
        locks.clone(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 64)).unwrap();
    match s.try_admit(JobId(1)) {
        Err(ScheduleError::Backend(e)) => {
            assert_eq!(e.kind, ErrorKind::Poisoned);
            assert_eq!(e.effect, FailureEffect::SessionPoisoned);
        }
        other => panic!("poisoned runtime must refuse admission: {:?}", other.err()),
    }
    assert_eq!(
        locks.log(),
        [
            "acquire Heavy",
            "acquire Gpu",
            "release Gpu",
            "release Heavy"
        ]
    );
    assert_eq!(s.snapshot().active, None);
    assert_eq!(s.snapshot().queued, vec![JobId(1)]);
}

// --------------------------------------------------------------------------
// Release on cancellation, error and unwinding
// --------------------------------------------------------------------------

#[test]
fn cancelling_a_queued_job_removes_it_without_any_reservation() {
    let (_fake, rt) = fake_runtime();
    let locks = RecordingLocks::default();
    let s = HeavyScheduler::new(
        device_source(&rt),
        locks.clone(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 64)).unwrap();
    s.submit(gpu_job(2, 64)).unwrap();
    s.cancel(JobId(1)).unwrap();
    assert_eq!(s.state(JobId(1)), Some(JobState::Cancelled));
    assert!(
        locks.log().is_empty(),
        "cancelling a queued job touches no lock"
    );
    assert_eq!(reserved(&rt), 0);
    admitted(s.try_admit(JobId(2)));
    assert!(matches!(
        s.try_admit(JobId(1)),
        Err(ScheduleError::NotQueued(JobId(1)))
    ));
}

#[test]
fn cancelling_the_active_job_releases_reservation_and_locks() {
    let (fake, rt) = fake_runtime();
    let locks = RecordingLocks::default();
    let s = HeavyScheduler::new(
        device_source(&rt),
        locks.clone(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 512)).unwrap();
    s.submit(gpu_job(2, 64)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(reserved(&rt), 512);
    locks.clear();
    assert_eq!(p.cancel().unwrap(), ReleaseOutcome::Released);
    assert_eq!(locks.log(), ["release Gpu", "release Heavy"]);
    assert_eq!(reserved(&rt), 0);
    assert_eq!(s.state(JobId(1)), Some(JobState::Cancelled));
    admitted(s.try_admit(JobId(2)));
    assert!(fake.violations().is_empty());
}

#[test]
fn failing_the_active_job_releases_and_marks_it_failed() {
    let (_fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 256)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(
        p.fail("prove: synthetic error").unwrap(),
        ReleaseOutcome::Released
    );
    assert_eq!(reserved(&rt), 0);
    assert_eq!(
        s.state(JobId(1)),
        Some(JobState::Failed("prove: synthetic error".into()))
    );
    assert_eq!(s.snapshot().active, None);
}

#[test]
fn a_panicking_proof_worker_releases_its_permit() {
    let (_fake, rt) = fake_runtime();
    let locks = RecordingLocks::default();
    let s = Arc::new(HeavyScheduler::new(
        device_source(&rt),
        locks.clone(),
        SchedulerConfig::default(),
    ));
    s.submit(gpu_job(1, 300)).unwrap();
    s.submit(gpu_job(2, 64)).unwrap();
    let worker = {
        let s = s.clone();
        thread::spawn(move || {
            let _p = admitted(s.try_admit(JobId(1)));
            panic!("synthetic prover panic while holding the heavy permit");
        })
    };
    assert!(worker.join().is_err());
    assert_eq!(
        reserved(&rt),
        0,
        "unwinding must release the device reservation"
    );
    assert!(matches!(s.state(JobId(1)), Some(JobState::Failed(_))));
    assert!(locks
        .log()
        .ends_with(&["release Gpu".to_string(), "release Heavy".to_string()]));
    admitted(s.try_admit(JobId(2)));
}

#[test]
fn reservation_from_an_older_generation_is_rejected_as_stale_but_frees_the_slot() {
    let (fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 128)).unwrap();
    s.submit(gpu_job(2, 128)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    let old = p.reservation_generation().unwrap();
    // Context loss during the proof, then the runtime's only recovery path.
    fake.inject(
        DriverCall::DeviceAlloc,
        1,
        Fault::Fail(DriverErrorClass::ContextLost),
    );
    assert!(ResidentBackend::<Toy, ToyPoint>::upload_scalars(&*rt, &scalars(4, 2)).is_err());
    let new = rt.reinitialize().unwrap();
    assert_ne!(old, new);
    assert!(matches!(
        p.check_current(),
        Err(ScheduleError::StaleReservation { .. })
    ));
    match p.fail("context lost").unwrap() {
        ReleaseOutcome::Stale(e) => assert_eq!(e.kind, ErrorKind::ContextLost),
        other => panic!("stale reservation must be reported, got {other:?}"),
    }
    assert_eq!(
        reserved(&rt),
        0,
        "new generation starts with no reservation"
    );
    let p2 = admitted(s.try_admit(JobId(2)));
    assert_eq!(p2.reservation_generation(), Some(new));
    p2.check_current().unwrap();
}

#[test]
fn completion_after_a_device_reset_is_refused_and_descendants_become_unavailable() {
    let (fake, rt) = fake_runtime();
    let s = HeavyScheduler::new(
        device_source(&rt),
        RecordingLocks::default(),
        SchedulerConfig::default(),
    );
    s.submit(gpu_job(1, 128)).unwrap();
    s.submit(JobSpec::new(JobId(2), Role::Parent).depends_on(Dependency::new(JobId(1), "child-vk")))
        .unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    fake.inject(
        DriverCall::DeviceAlloc,
        1,
        Fault::Fail(DriverErrorClass::ContextLost),
    );
    assert!(ResidentBackend::<Toy, ToyPoint>::upload_scalars(&*rt, &scalars(4, 2)).is_err());
    rt.reinitialize().unwrap();
    // The caller does not call check_current(): completion itself must refuse.
    match p.complete(identity("child-vk")) {
        Err(ScheduleError::StaleReservation { .. }) => {}
        other => panic!("completion on a stale generation must be refused, got {other:?}"),
    }
    assert!(matches!(s.state(JobId(1)), Some(JobState::Failed(_))));
    assert!(matches!(s.state(JobId(2)), Some(JobState::Unavailable { .. })));
    assert_eq!(s.snapshot().held_reservations, 0);
    assert!(s.snapshot().active.is_none());
}

#[test]
fn forged_reservation_metadata_is_rejected_by_the_runtime_source() {
    let (_fake, rt) = fake_runtime();
    let src = device_source(&rt);
    let real = match src.try_reserve(64).unwrap() {
        ReservationResult::Reserved(t) => t,
        _ => panic!("fits"),
    };
    let forged = ReservationToken::from_untrusted_metadata(real.id(), real.generation(), 1);
    let e = src.release_reservation(forged).unwrap_err();
    assert_eq!(e.kind, ErrorKind::InvalidInput);
    assert_eq!(
        reserved(&rt),
        64,
        "forged release must not drop the real reservation"
    );
    src.release_reservation(real).unwrap();
    assert_eq!(reserved(&rt), 0);
}

// --------------------------------------------------------------------------
// Dependency DAG readiness
// --------------------------------------------------------------------------

fn chain(s: &HeavyScheduler<impl ReservationSource, impl HostLocks>, parent_expects: &str) {
    s.submit(cpu_job(1)).unwrap();
    s.submit(
        JobSpec::new(JobId(2), Role::Parent).depends_on(Dependency::new(JobId(1), parent_expects)),
    )
    .unwrap();
    s.submit(
        JobSpec::new(JobId(3), Role::Decider).depends_on(Dependency::new(JobId(2), "parent-vk")),
    )
    .unwrap();
}

#[test]
fn unknown_dependency_is_refused_at_submission() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    let err = s
        .submit(JobSpec::new(JobId(9), Role::Parent).depends_on(Dependency::new(JobId(42), "x")))
        .unwrap_err();
    assert!(matches!(
        err,
        ScheduleError::UnknownDependency {
            job: JobId(9),
            missing: JobId(42)
        }
    ));
    assert_eq!(s.state(JobId(9)), None, "a refused job is not queued");
    assert!(matches!(s.submit(cpu_job(1)), Ok(())));
    assert!(matches!(
        s.submit(cpu_job(1)),
        Err(ScheduleError::DuplicateJob(JobId(1)))
    ));
}

#[test]
fn dependent_waits_until_child_is_published_then_runs_in_dag_order() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    chain(&s, "leaf-vk");
    // The parent was submitted before... but is not ready: it must not jump ahead.
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::DependencyPending { child: JobId(1) }
    );
    let leaf = admitted(s.try_admit(JobId(1)));
    assert_eq!(
        waiting(s.try_admit(JobId(2))),
        WaitReason::DependencyPending { child: JobId(1) }
    );
    leaf.complete(identity("leaf-vk")).unwrap();
    let parent = admitted(s.try_admit(JobId(2)));
    assert_eq!(parent.children(), vec![(JobId(1), identity("leaf-vk"))]);
    parent.complete(identity("parent-vk")).unwrap();
    admitted(s.try_admit(JobId(3)))
        .complete(identity("decider-vk"))
        .unwrap();
    assert_eq!(s.snapshot().order, vec![JobId(1), JobId(2), JobId(3)]);
}

#[test]
fn ready_independent_job_is_not_blocked_by_a_waiting_dependent() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    s.submit(cpu_job(1)).unwrap();
    s.submit(JobSpec::new(JobId(2), Role::Parent).depends_on(Dependency::new(JobId(1), "v")))
        .unwrap();
    s.submit(cpu_job(3)).unwrap();
    let p1 = admitted(s.try_admit(JobId(1)));
    p1.complete(identity("v")).unwrap();
    // FIFO among *ready* jobs: 2 is ready now and ahead of 3.
    assert_eq!(
        waiting(s.try_admit(JobId(3))),
        WaitReason::QueuedBehind { position: 1 }
    );
    admitted(s.try_admit(JobId(2)))
        .complete(identity("p"))
        .unwrap();
    admitted(s.try_admit(JobId(3)));
}

#[test]
fn wrong_child_vk_fails_the_dependent_and_its_descendants() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    chain(&s, "expected-leaf-vk");
    admitted(s.try_admit(JobId(1)))
        .complete(identity("other-leaf-vk"))
        .unwrap();
    match s.try_admit(JobId(2)) {
        Err(ScheduleError::WrongChildIdentity {
            job,
            child,
            expected_vk,
            actual_vk,
        }) => {
            assert_eq!((job, child), (JobId(2), JobId(1)));
            assert_eq!(expected_vk, "expected-leaf-vk");
            assert_eq!(actual_vk, "other-leaf-vk");
        }
        other => panic!("wrong child VK must be refused: {:?}", other.err()),
    }
    assert!(matches!(s.state(JobId(2)), Some(JobState::Failed(_))));
    assert_eq!(
        s.state(JobId(3)),
        Some(JobState::Unavailable { cause: JobId(2) })
    );
    assert!(matches!(
        s.try_admit(JobId(3)),
        Err(ScheduleError::NotQueued(JobId(3)))
    ));
    assert_eq!(s.snapshot().held_host_locks, 0);
}

#[test]
fn cancelled_child_makes_descendants_unavailable() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    chain(&s, "leaf-vk");
    let leaf = admitted(s.try_admit(JobId(1)));
    leaf.cancel().unwrap();
    assert_eq!(
        s.state(JobId(2)),
        Some(JobState::Unavailable { cause: JobId(1) })
    );
    assert_eq!(
        s.state(JobId(3)),
        Some(JobState::Unavailable { cause: JobId(1) })
    );
    assert!(s.snapshot().queued.is_empty());
}

#[test]
fn failed_child_makes_descendants_unavailable() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    chain(&s, "leaf-vk");
    admitted(s.try_admit(JobId(1)))
        .fail("leaf verify failed")
        .unwrap();
    assert_eq!(
        s.state(JobId(2)),
        Some(JobState::Unavailable { cause: JobId(1) })
    );
    assert_eq!(
        s.state(JobId(3)),
        Some(JobState::Unavailable { cause: JobId(1) })
    );
}

#[test]
fn dependency_on_an_already_failed_job_is_refused_at_submission() {
    let s = HeavyScheduler::cpu_only(RecordingLocks::default(), SchedulerConfig::default());
    s.submit(cpu_job(1)).unwrap();
    admitted(s.try_admit(JobId(1))).fail("boom").unwrap();
    let err = s
        .submit(JobSpec::new(JobId(2), Role::Parent).depends_on(Dependency::new(JobId(1), "v")))
        .unwrap_err();
    assert!(matches!(
        err,
        ScheduleError::DependencyUnavailable {
            job: JobId(2),
            child: JobId(1)
        }
    ));
}

#[test]
fn cpu_budget_is_fixed_and_reported_by_the_permit() {
    let s = HeavyScheduler::cpu_only(
        RecordingLocks::default(),
        SchedulerConfig { cpu_threads: 4 },
    );
    s.submit(cpu_job(1)).unwrap();
    let p = admitted(s.try_admit(JobId(1)));
    assert_eq!(p.cpu_threads(), 4);
    assert_eq!(p.role(), Role::Leaf);
    assert_eq!(p.job(), JobId(1));
}
