//! One-heavy FIFO admission for recursion-chain proof jobs (P7).
//!
//! CODE (P7 contract, RECURSION.md "Admission and shared-device policy"): the
//! scheduler owns inter-proof readiness and admission only. It never sees a
//! transcript, circuit or proof bytes, and it does not change any P2 runtime
//! semantics: device budget is taken exclusively through the public
//! non-blocking [`ResidentBackend::try_reserve`] /
//! [`ResidentBackend::release_reservation`] calls, wrapped by
//! [`ReservationSource`].
//!
//! # Rules
//!
//! * Jobs are submitted as DAG nodes. Unknown dependencies are refused; a job
//!   is *ready* only when every child has been published with the expected VK
//!   identity. A failed or cancelled job makes every descendant unavailable.
//! * FIFO among ready jobs and at most one admitted heavy job. A queued job
//!   holds no reservation, no resident lease and no host lock.
//! * Admission order is fixed: the heavy host lock, then (GPU jobs only) the
//!   GPU host lock, then the immediate device reservation. Every step is
//!   non-blocking. If any step cannot proceed, everything already taken is
//!   released in reverse order and the job keeps its queue position.
//! * The [`HeavyPermit`] releases in reverse order (reservation, GPU, heavy)
//!   on completion, failure, cancellation or drop (including unwinding). A
//!   reservation from an older device generation is reported as
//!   [`ReleaseOutcome::Stale`]; the heavy slot is freed regardless.
//!
//! * [`HeavyPermit::complete`] publishes only if the reservation's device
//!   generation is still current and healthy and the reservation releases
//!   cleanly; otherwise the job is recorded `Failed` and the error returned.
//!
//! # Status: admission prototype (no reservation redemption)
//!
//! The permit's reservation is an admission hold only. The P2 runtime has no
//! API to *redeem* it: uploads and launches inside the admitted job reserve
//! their own allocations independently. A job must therefore NOT request its
//! own workspace through [`JobSpec::gpu`] (that would double-count and can
//! exclude the job's own allocations); request at most the headroom that
//! must stay free for it, or zero. Closing this needs a P4-owned runtime
//! hook (reservation-scoped allocation); until then the scheduler is not a
//! device-memory accountant.
//!
//! The scheduler mutex is the outermost lock. Host locks and the P2 runtime's
//! registry are only taken beneath it, never the other way round.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard};

use loki_backend_api::{
    BackendError, Curve, DeviceGeneration, ErrorKind, Operation, PrimeField, ReservationResult,
    ReservationToken, ResidentBackend, RuntimeSnapshot,
};

// ===========================================================================
// Reservation sources
// ===========================================================================

/// The subset of the backend API the scheduler admits against.
pub trait ReservationSource: Send + Sync {
    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError>;
    /// Must be non-blocking (P2 contract).
    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError>;
    fn release_reservation(&self, token: ReservationToken) -> Result<(), BackendError>;
}

/// Adapts any [`ResidentBackend`] (for example the P2 `GpuRuntime`) into a
/// [`ReservationSource`]. The scalar/curve types only select the trait
/// instance; reservations are untyped byte budgets.
pub struct ResidentReservations<F, G, B> {
    backend: B,
    _types: PhantomData<fn() -> (F, G)>,
}

impl<F, G, B> ResidentReservations<F, G, B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            _types: PhantomData,
        }
    }
}

impl<F, G, B> ReservationSource for ResidentReservations<F, G, B>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
    B: Deref + Send + Sync,
    B::Target: ResidentBackend<F, G>,
{
    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError> {
        self.backend.runtime_snapshot()
    }
    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError> {
        self.backend.try_reserve(bytes)
    }
    fn release_reservation(&self, token: ReservationToken) -> Result<(), BackendError> {
        self.backend.release_reservation(token)
    }
}

/// No device: every GPU job is refused with `Unsupported`. CPU jobs only use
/// the heavy slot.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDevice;

impl ReservationSource for NoDevice {
    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError> {
        Err(BackendError::unsupported(Operation::Runtime))
    }
    fn try_reserve(&self, _bytes: u64) -> Result<ReservationResult, BackendError> {
        Err(BackendError::unsupported(Operation::Reserve))
    }
    fn release_reservation(&self, _token: ReservationToken) -> Result<(), BackendError> {
        Err(BackendError::unsupported(Operation::Release))
    }
}

// ===========================================================================
// Host locks
// ===========================================================================

/// Host-wide locks shared with other processes (DoctorWatson `heavy.lock` and
/// `gpu.lock`). Order is always `Heavy` before `Gpu`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HostLock {
    Heavy,
    Gpu,
}

/// A held host lock; releasing runs the provider's release action.
pub struct HostLockGuard {
    lock: HostLock,
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl HostLockGuard {
    pub fn new(lock: HostLock, release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            lock,
            release: Some(Box::new(release)),
        }
    }
    pub fn lock(&self) -> HostLock {
        self.lock
    }
}

impl Drop for HostLockGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

impl fmt::Debug for HostLockGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostLockGuard")
            .field("lock", &self.lock)
            .finish()
    }
}

/// Provider of non-blocking host lock acquisition. `Ok(None)` means busy.
pub trait HostLocks: Send + Sync {
    fn try_acquire(&self, lock: HostLock) -> std::io::Result<Option<HostLockGuard>>;
}

/// In-process only: host locks are always free (for example when an outer
/// wrapper already holds the DoctorWatson locks for this whole process).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeldByParent;

impl HostLocks for HeldByParent {
    fn try_acquire(&self, lock: HostLock) -> std::io::Result<Option<HostLockGuard>> {
        Ok(Some(HostLockGuard::new(lock, || {})))
    }
}

// ===========================================================================
// Jobs
// ===========================================================================

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JobId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Role {
    Leaf,
    Parent,
    Decider,
}

/// Public identity of a published proof artifact (digests only).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactIdentity {
    pub vk_sha256: String,
    pub proof_sha256: String,
    pub instance_sha256: String,
}

/// A child edge: the child job and the VK digest the dependent was built for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dependency {
    pub child: JobId,
    pub expected_vk_sha256: String,
}

impl Dependency {
    pub fn new(child: JobId, expected_vk_sha256: impl Into<String>) -> Self {
        Self {
            child,
            expected_vk_sha256: expected_vk_sha256.into(),
        }
    }
}

/// Immutable job record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobSpec {
    pub id: JobId,
    pub role: Role,
    /// Device workspace to reserve at admission (GPU jobs only).
    pub device_bytes: u64,
    /// Whether the job needs the GPU lock and a device reservation.
    pub uses_gpu: bool,
    pub dependencies: Vec<Dependency>,
}

impl JobSpec {
    /// A CPU-only heavy job.
    pub fn new(id: JobId, role: Role) -> Self {
        Self {
            id,
            role,
            device_bytes: 0,
            uses_gpu: false,
            dependencies: Vec::new(),
        }
    }
    /// Mark as a GPU job reserving `device_bytes` at admission.
    pub fn gpu(mut self, device_bytes: u64) -> Self {
        self.uses_gpu = true;
        self.device_bytes = device_bytes;
        self
    }
    pub fn depends_on(mut self, dependency: Dependency) -> Self {
        self.dependencies.push(dependency);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Running,
    Completed(ArtifactIdentity),
    Failed(String),
    Cancelled,
    /// A (transitive) child failed or was cancelled.
    Unavailable {
        cause: JobId,
    },
}

impl JobState {
    fn is_dead(&self) -> bool {
        matches!(
            self,
            JobState::Failed(_) | JobState::Cancelled | JobState::Unavailable { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SchedulerConfig {
    /// Fixed CPU thread budget handed to the admitted job (0 = unspecified).
    pub cpu_threads: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaitReason {
    /// A child has not been published yet.
    DependencyPending { child: JobId },
    /// `position` ready jobs are ahead in FIFO order.
    QueuedBehind { position: usize },
    /// Another heavy job holds the slot.
    HeavyBusy { active: JobId },
    /// A host lock is held by another process.
    HostLockBusy(HostLock),
}

#[derive(Debug)]
pub enum ScheduleError {
    DuplicateJob(JobId),
    UnknownDependency {
        job: JobId,
        missing: JobId,
    },
    DependencyUnavailable {
        job: JobId,
        child: JobId,
    },
    /// The job is not in the queue (unknown, running or finished).
    NotQueued(JobId),
    WrongChildIdentity {
        job: JobId,
        child: JobId,
        expected_vk: String,
        actual_vk: String,
    },
    StaleReservation {
        reserved: DeviceGeneration,
        current: Option<DeviceGeneration>,
    },
    Backend(BackendError),
    HostLock(std::io::Error),
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl std::error::Error for ScheduleError {}

/// What happened to the device reservation when a permit was released.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReleaseOutcome {
    /// Reservation (if any) returned to the budget.
    Released,
    /// The reservation belonged to an older device generation; the runtime
    /// already dropped that generation's accounting.
    Stale(BackendError),
    /// The runtime refused the release (for example a poisoned session); its
    /// budget stays conservatively retained by the runtime.
    Retained(BackendError),
}

/// Result of a non-blocking admission request.
pub enum Admission<S: ReservationSource, L: HostLocks> {
    Admitted(HeavyPermit<S, L>),
    Waiting(WaitReason),
    /// The device budget cannot fit the job now. Nothing is held.
    Deferred {
        requested_bytes: u64,
        available_bytes: u64,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SchedulerSnapshot {
    pub active: Option<JobId>,
    /// Queued jobs in submission order.
    pub queued: Vec<JobId>,
    /// Admission order so far.
    pub order: Vec<JobId>,
    pub held_reservations: usize,
    pub held_host_locks: usize,
    pub runtime: Option<RuntimeSnapshot>,
}

// ===========================================================================
// Scheduler
// ===========================================================================

#[derive(Default)]
struct State {
    jobs: HashMap<JobId, (JobSpec, JobState)>,
    queue: VecDeque<JobId>,
    active: Option<JobId>,
    order: Vec<JobId>,
    held_reservations: usize,
    held_host_locks: usize,
}

impl State {
    fn state(&self, id: JobId) -> Option<&JobState> {
        self.jobs.get(&id).map(|(_, s)| s)
    }

    fn set(&mut self, id: JobId, state: JobState) {
        if let Some(entry) = self.jobs.get_mut(&id) {
            entry.1 = state;
        }
    }

    /// Whether every child is published (identity checked separately).
    fn first_pending(&self, spec: &JobSpec) -> Option<JobId> {
        spec.dependencies
            .iter()
            .find(|d| !matches!(self.state(d.child), Some(JobState::Completed(_))))
            .map(|d| d.child)
    }

    fn is_ready(&self, id: JobId) -> bool {
        self.jobs
            .get(&id)
            .is_some_and(|(spec, _)| self.first_pending(spec).is_none())
    }

    /// Mark every queued (transitive) descendant of `cause` unavailable.
    fn propagate(&mut self, cause: JobId) {
        let mut dead: HashSet<JobId> = HashSet::from([cause]);
        loop {
            let newly: Vec<JobId> = self
                .queue
                .iter()
                .copied()
                .filter(|id| {
                    self.jobs[id]
                        .0
                        .dependencies
                        .iter()
                        .any(|d| dead.contains(&d.child))
                })
                .collect();
            if newly.is_empty() {
                break;
            }
            for id in newly {
                self.set(id, JobState::Unavailable { cause });
                self.queue.retain(|q| *q != id);
                dead.insert(id);
            }
        }
    }

    fn finish(&mut self, id: JobId, state: JobState) {
        let dead = state.is_dead();
        self.set(id, state);
        if self.active == Some(id) {
            self.active = None;
        }
        self.queue.retain(|q| *q != id);
        if dead {
            self.propagate(id);
        }
    }
}

struct Inner<S, L> {
    source: S,
    locks: L,
    config: SchedulerConfig,
    state: Mutex<State>,
}

impl<S, L> Inner<S, L> {
    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while holding the state lock leaves plain bookkeeping that is
        // still consistent at every await-free step; recover it.
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// One-heavy FIFO scheduler. Cheap to share by reference or `Arc`.
pub struct HeavyScheduler<S: ReservationSource, L: HostLocks> {
    inner: Arc<Inner<S, L>>,
}

impl<L: HostLocks> HeavyScheduler<NoDevice, L> {
    /// A scheduler for CPU-only jobs.
    pub fn cpu_only(locks: L, config: SchedulerConfig) -> Self {
        Self::new(NoDevice, locks, config)
    }
}

impl<S: ReservationSource, L: HostLocks> HeavyScheduler<S, L> {
    pub fn new(source: S, locks: L, config: SchedulerConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                source,
                locks,
                config,
                state: Mutex::new(State::default()),
            }),
        }
    }

    /// Enqueue a job. Takes no lock and reserves nothing.
    pub fn submit(&self, spec: JobSpec) -> Result<(), ScheduleError> {
        let mut st = self.inner.state();
        if st.jobs.contains_key(&spec.id) {
            return Err(ScheduleError::DuplicateJob(spec.id));
        }
        for dep in &spec.dependencies {
            match st.state(dep.child) {
                None => {
                    return Err(ScheduleError::UnknownDependency {
                        job: spec.id,
                        missing: dep.child,
                    })
                }
                Some(s) if s.is_dead() => {
                    return Err(ScheduleError::DependencyUnavailable {
                        job: spec.id,
                        child: dep.child,
                    })
                }
                Some(_) => {}
            }
        }
        st.queue.push_back(spec.id);
        st.jobs.insert(spec.id, (spec, JobState::Queued));
        Ok(())
    }

    /// Cancel a queued job. Running jobs are cancelled through their permit.
    pub fn cancel(&self, id: JobId) -> Result<(), ScheduleError> {
        let mut st = self.inner.state();
        if st.state(id) != Some(&JobState::Queued) {
            return Err(ScheduleError::NotQueued(id));
        }
        st.finish(id, JobState::Cancelled);
        Ok(())
    }

    pub fn state(&self, id: JobId) -> Option<JobState> {
        self.inner.state().state(id).cloned()
    }

    pub fn snapshot(&self) -> SchedulerSnapshot {
        let st = self.inner.state();
        SchedulerSnapshot {
            active: st.active,
            queued: st.queue.iter().copied().collect(),
            order: st.order.clone(),
            held_reservations: st.held_reservations,
            held_host_locks: st.held_host_locks,
            runtime: self.inner.source.runtime_snapshot().ok(),
        }
    }

    /// Non-blocking admission of `id`. See the module rules.
    pub fn try_admit(&self, id: JobId) -> Result<Admission<S, L>, ScheduleError> {
        let mut st = self.inner.state();
        let spec = match st.jobs.get(&id) {
            Some((spec, JobState::Queued)) => spec.clone(),
            _ => return Err(ScheduleError::NotQueued(id)),
        };

        // DAG readiness and child identity.
        if let Some(child) = st.first_pending(&spec) {
            return Ok(Admission::Waiting(WaitReason::DependencyPending { child }));
        }
        let mut children = Vec::with_capacity(spec.dependencies.len());
        for dep in &spec.dependencies {
            let Some(JobState::Completed(identity)) = st.state(dep.child).cloned() else {
                unreachable!("first_pending checked every child");
            };
            if identity.vk_sha256 != dep.expected_vk_sha256 {
                let error = ScheduleError::WrongChildIdentity {
                    job: id,
                    child: dep.child,
                    expected_vk: dep.expected_vk_sha256.clone(),
                    actual_vk: identity.vk_sha256.clone(),
                };
                st.finish(id, JobState::Failed(format!("{error}")));
                return Err(error);
            }
            children.push((dep.child, identity));
        }

        // FIFO among ready jobs, one heavy job at a time.
        let position = st
            .queue
            .iter()
            .take_while(|q| **q != id)
            .filter(|q| st.is_ready(**q))
            .count();
        if position > 0 {
            return Ok(Admission::Waiting(WaitReason::QueuedBehind { position }));
        }
        if let Some(active) = st.active {
            return Ok(Admission::Waiting(WaitReason::HeavyBusy { active }));
        }

        // Heavy, then GPU, then the immediate reservation.
        let heavy = match self.inner.locks.try_acquire(HostLock::Heavy) {
            Ok(Some(g)) => g,
            Ok(None) => {
                return Ok(Admission::Waiting(WaitReason::HostLockBusy(
                    HostLock::Heavy,
                )))
            }
            Err(e) => return Err(ScheduleError::HostLock(e)),
        };
        let mut guards = vec![heavy];
        let mut token = None;
        if spec.uses_gpu {
            match self.inner.locks.try_acquire(HostLock::Gpu) {
                Ok(Some(g)) => guards.push(g),
                Ok(None) => {
                    release_guards(guards);
                    return Ok(Admission::Waiting(WaitReason::HostLockBusy(HostLock::Gpu)));
                }
                Err(e) => {
                    release_guards(guards);
                    return Err(ScheduleError::HostLock(e));
                }
            }
            match self.inner.source.try_reserve(spec.device_bytes) {
                Ok(ReservationResult::Reserved(t)) => token = Some(t),
                Ok(ReservationResult::Rejected {
                    requested_bytes,
                    available_bytes,
                }) => {
                    release_guards(guards);
                    return Ok(Admission::Deferred {
                        requested_bytes,
                        available_bytes,
                    });
                }
                Err(e) => {
                    release_guards(guards);
                    return Err(ScheduleError::Backend(e));
                }
            }
        }

        st.queue.retain(|q| *q != id);
        st.set(id, JobState::Running);
        st.active = Some(id);
        st.order.push(id);
        st.held_host_locks = guards.len();
        st.held_reservations = usize::from(token.is_some());
        Ok(Admission::Admitted(HeavyPermit {
            inner: self.inner.clone(),
            job: id,
            role: spec.role,
            token,
            guards,
            children,
            finished: false,
        }))
    }
}

/// Release host lock guards in reverse acquisition order.
fn release_guards(mut guards: Vec<HostLockGuard>) {
    while let Some(g) = guards.pop() {
        drop(g);
    }
}

// ===========================================================================
// Permit
// ===========================================================================

/// The single admitted heavy job's resources. Release is guaranteed by drop.
pub struct HeavyPermit<S: ReservationSource, L: HostLocks> {
    inner: Arc<Inner<S, L>>,
    job: JobId,
    role: Role,
    token: Option<ReservationToken>,
    guards: Vec<HostLockGuard>,
    children: Vec<(JobId, ArtifactIdentity)>,
    finished: bool,
}

impl<S: ReservationSource, L: HostLocks> HeavyPermit<S, L> {
    pub fn job(&self) -> JobId {
        self.job
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn cpu_threads(&self) -> usize {
        self.inner.config.cpu_threads
    }
    /// Published children this job was admitted against, in dependency order.
    pub fn children(&self) -> Vec<(JobId, ArtifactIdentity)> {
        self.children.clone()
    }
    pub fn reservation_bytes(&self) -> Option<u64> {
        self.token.as_ref().map(ReservationToken::bytes)
    }
    pub fn reservation_generation(&self) -> Option<DeviceGeneration> {
        self.token.as_ref().map(ReservationToken::generation)
    }

    /// Fails if the reservation's device generation is no longer current or
    /// healthy, so the caller can abort instead of using stale residents.
    pub fn check_current(&self) -> Result<(), ScheduleError> {
        let Some(token) = &self.token else {
            return Ok(());
        };
        let current = self.inner.source.runtime_snapshot().ok();
        match current {
            Some(s) if s.generation == token.generation() && s.context_healthy => Ok(()),
            _ => Err(ScheduleError::StaleReservation {
                reserved: token.generation(),
                current: current.map(|s| s.generation),
            }),
        }
    }

    /// Publish the job's artifact identity and release.
    ///
    /// Completion requires the reservation's device generation to be current
    /// and healthy at completion time, and the reservation to be released
    /// cleanly. Otherwise the job is recorded as `Failed` (descendants become
    /// unavailable), every resource is still released, and the error is
    /// returned: a result produced across a device reset is never published.
    pub fn complete(mut self, identity: ArtifactIdentity) -> Result<ReleaseOutcome, ScheduleError> {
        if let Err(e) = self.check_current() {
            self.release(JobState::Failed(format!("completion refused: {e}")));
            return Err(e);
        }
        let reserved = self.token.as_ref().map(ReservationToken::generation);
        let mut published = Some(JobState::Completed(identity));
        let outcome = self.release_with(|outcome| match outcome {
            ReleaseOutcome::Released => published.take().expect("once"),
            other => JobState::Failed(format!("completion refused: reservation release {other:?}")),
        });
        match outcome {
            ReleaseOutcome::Released => Ok(ReleaseOutcome::Released),
            ReleaseOutcome::Stale(_) => Err(ScheduleError::StaleReservation {
                reserved: reserved.expect("a stale release implies a reservation"),
                current: self.inner.source.runtime_snapshot().ok().map(|s| s.generation),
            }),
            ReleaseOutcome::Retained(e) => Err(ScheduleError::Backend(e)),
        }
    }

    /// Record a failure and release. Descendants become unavailable.
    pub fn fail(mut self, reason: impl Into<String>) -> Result<ReleaseOutcome, ScheduleError> {
        Ok(self.release(JobState::Failed(reason.into())))
    }

    /// Record cancellation and release. Descendants become unavailable.
    pub fn cancel(mut self) -> Result<ReleaseOutcome, ScheduleError> {
        Ok(self.release(JobState::Cancelled))
    }

    fn release(&mut self, state: JobState) -> ReleaseOutcome {
        self.release_with(move |_| state)
    }

    /// Release everything; the final job state may depend on the outcome.
    fn release_with(&mut self, state: impl FnOnce(&ReleaseOutcome) -> JobState) -> ReleaseOutcome {
        self.finished = true;
        let mut st = self.inner.state();
        let outcome = match self.token.take() {
            None => ReleaseOutcome::Released,
            Some(token) => match self.inner.source.release_reservation(token) {
                Ok(()) => ReleaseOutcome::Released,
                Err(e) if e.kind == ErrorKind::ContextLost => ReleaseOutcome::Stale(e),
                Err(e) => ReleaseOutcome::Retained(e),
            },
        };
        release_guards(std::mem::take(&mut self.guards));
        st.held_reservations = 0;
        st.held_host_locks = 0;
        let state = state(&outcome);
        st.finish(self.job, state);
        outcome
    }
}

impl<S: ReservationSource, L: HostLocks> Drop for HeavyPermit<S, L> {
    fn drop(&mut self) {
        if !self.finished {
            let reason = if std::thread::panicking() {
                "permit dropped while unwinding"
            } else {
                "permit dropped without completion"
            };
            self.release(JobState::Failed(reason.into()));
        }
    }
}

impl<S: ReservationSource, L: HostLocks> fmt::Debug for HeavyPermit<S, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeavyPermit")
            .field("job", &self.job)
            .field("role", &self.role)
            .field("reservation_bytes", &self.reservation_bytes())
            .finish()
    }
}
