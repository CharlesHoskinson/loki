//! Transactional device runtime: one process coordinator for every GPU module.
//!
//! CODE (P2): this module owns the device context/generation, the checked
//! shared device budget and reservations, the allocation registry, the bounded
//! staging pool, long-lived streams/events, and generation poisoning. It is the
//! only code that talks to a [`DeviceDriver`]; the native CUDA driver lives in
//! `ffi.rs` behind the `cuda` feature, and [`fake::FakeDriver`] is a
//! fault-injecting in-memory device used by `tests/runtime_contract.rs`.
//!
//! # Transaction rules
//!
//! * Public v1 calls block through checked completion. Upload copies the caller
//!   slice into backend-owned staging, download returns a fresh vector, and a
//!   launch returns a fresh handle; nothing is published before the governing
//!   completion event has been synchronized successfully. Caller slices are
//!   never DMA sources or targets.
//! * Every device allocation is reserved against the shared budget for its full
//!   size before allocation and stays reserved until publication or retirement.
//! * A failure before any enqueue attempt returns `NotStarted`. A failure after
//!   an enqueue attempt drains the affected stream; if the drain succeeds, all
//!   private resources retire and the error is `InputsPreserved`. If the drain
//!   fails, an event wait fails, or the driver reports a sticky error,
//!   completion is uncertain: the generation is poisoned, every allocation of
//!   that generation is quarantined (never freed through the poisoned context),
//!   and the error is `SessionPoisoned`.
//! * A staging slot is rewritten or read only after its completion event has
//!   been synchronized, and allocations are freed only after the work that
//!   references them has retired (every v1 call retires its own work before
//!   returning, including internal cancellation).
//! * Handles are untrusted metadata. Every use and release checks the
//!   backend-owned record: allocation identity, generation, element type,
//!   length and byte count. Allocation and reservation identities are never
//!   reused. A handle from an older generation fails with `ContextLost`.
//! * Only [`GpuRuntime::reinitialize`] creates a new generation, and only after
//!   the old one is poisoned and the driver reset succeeds.
//!
//! # Lock order
//!
//! 1. `device` — device/context guard; serializes all driver work.
//! 2. `registry` — budget, reservations and allocation metadata. Held only for
//!    bookkeeping, never across a driver call, so scheduler admission
//!    (`try_reserve`, snapshots) never waits for device work.
//! 3. `telemetry` — counters.
//!
//! [`OrderedMutex`] enforces this order at run time on every acquisition: a
//! thread that already holds an equal or higher level panics instead of
//! risking deadlock.

use std::any::TypeId;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use loki_backend_api::{
    AllocationId, BackendCapabilities, BackendError, Curve, DeviceGeneration, ErrorKind,
    FailureEffect, Operation, PrimeField, ReservationId, ReservationResult, ReservationToken,
    ResidentBackend, ResidentBuffer, RuntimeSnapshot as ApiRuntimeSnapshot, TransformSpec,
};

use crate::LokiGpu;

/// WSL2-friendly pinned transfer slot size. A failed pinned allocation uses
/// backend-owned pageable staging; the pool never grows beyond its bounded
/// configuration.
pub const PINNED_SLOT_BYTES: usize = 32 * 1024 * 1024;

/// Start with two slots; increase only after measuring overlap and reviewing
/// the host-memory budget.
pub const INITIAL_PINNED_SLOTS: usize = 2;

/// Hard cap on staging slots. WSL2 may refuse pinned allocations near
/// 500 MB process-wide, so the pool stays far below that.
pub const MAX_PINNED_SLOTS: usize = 4;

/// Maximum host memory reserved by the pinned transfer ring. This is separate
/// from the CUDA device-allocation budget.
pub const PINNED_HOST_BUDGET_BYTES: usize = MAX_PINNED_SLOTS * PINNED_SLOT_BYTES;

/// Conservative starting limit for persistent SRS residency. Runtime scratch
/// is accounted separately against the shared device budget.
pub const DEFAULT_SRS_BUDGET_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Device memory left unbudgeted for the CUDA context, other processes and
/// allocator overhead when no explicit cap is configured.
pub const DEFAULT_DEVICE_HEADROOM_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Default bound on backend-retained host checkpoints.
pub const DEFAULT_CHECKPOINT_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;

/// The sm_120 native implementation requires compute capability 12.0.
pub const REQUIRED_COMPUTE_CAPABILITY: (u32, u32) = (12, 0);

/// Environment overrides read by [`RuntimeConfig::from_environment`].
pub const DEVICE_BUDGET_ENV: &str = "LOKI_GPU_DEVICE_BUDGET_BYTES";
pub const DEVICE_HEADROOM_ENV: &str = "LOKI_GPU_DEVICE_HEADROOM_BYTES";
pub const STAGING_SLOTS_ENV: &str = "LOKI_GPU_STAGING_SLOTS";
pub const DEVICE_ORDINAL_ENV: &str = "LOKI_GPU_DEVICE_ORDINAL";

// ===========================================================================
// Driver seam
// ===========================================================================

/// Driver entry point, used for fault injection and error classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DriverCall {
    Probe,
    Reset,
    StreamCreate,
    StreamDestroy,
    EventCreate,
    EventDestroy,
    DeviceAlloc,
    DeviceFree,
    HostAlloc,
    HostFree,
    CopyH2D,
    CopyD2H,
    Launch,
    EventRecord,
    StreamWait,
    EventSync,
    EventQuery,
    EventElapsed,
    StreamSync,
}

/// Whether a driver failure leaves the context usable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DriverErrorClass {
    /// The call failed without corrupting the context (for example a
    /// non-sticky out-of-memory). Work enqueued earlier may still be pending.
    Transient,
    /// Sticky device error: the context can no longer be trusted.
    Poisoned,
    /// The context or device is gone; resident data is lost.
    ContextLost,
    /// The request is not supported by this device/driver.
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverError {
    pub call: DriverCall,
    pub class: DriverErrorClass,
    /// Native status / CUDA error code for diagnostics; 0 for the fake.
    pub code: i32,
}

/// Properties reported by a driver probe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub ordinal: u32,
    pub compute_capability: (u32, u32),
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Native context epoch; increases on every successful reset.
    pub context_epoch: u64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum HandleKind {
    DeviceMemory = 1,
    HostPinned = 2,
    HostPageable = 3,
    Stream = 4,
    Event = 5,
}

/// Opaque driver handle: kind, context epoch, driver bits and extent. The
/// fields are crate-private so no caller can mint one.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DriverHandle {
    pub(crate) kind: HandleKind,
    pub(crate) epoch: u64,
    pub(crate) raw: u64,
    pub(crate) bytes: u64,
}

/// Device allocation handle (never exposed outside the runtime).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceMemory(pub(crate) DriverHandle);
/// Long-lived non-blocking stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamId(pub(crate) DriverHandle);
/// Long-lived event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventId(pub(crate) DriverHandle);

/// Backend-owned host staging allocation (pinned or pageable).
#[derive(Debug)]
pub struct HostBuffer {
    pub(crate) handle: DriverHandle,
    pub(crate) ptr: *mut u8,
    pub(crate) len: usize,
}

// SAFETY: a HostBuffer is exclusively owned by the runtime (it is not Clone);
// the pointed-to allocation is only accessed by the thread holding the device
// lock, after the driver has confirmed no pending operation references it.
unsafe impl Send for HostBuffer {}

impl HostBuffer {
    pub fn pinned(&self) -> bool {
        self.handle.kind == HandleKind::HostPinned
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Device driver used by the coordinator.
///
/// # Safety
///
/// Implementations must guarantee that a [`HostBuffer`] returned by
/// `host_alloc` points to `len` writable bytes that stay valid until
/// `host_free` (or a context reset that the implementation documents), that
/// copies read/write exactly the requested ranges, and that a successful
/// `event_synchronize`/`stream_synchronize` means every earlier operation on
/// the governing stream has completed and released its host-buffer access.
pub unsafe trait DeviceDriver: Send + Sync {
    fn probe(&self, ordinal: u32) -> Result<DeviceInfo, DriverError>;
    /// Destroy the current context (and every resource it owned) and create a
    /// new one with a larger epoch.
    fn reset(&self, ordinal: u32) -> Result<DeviceInfo, DriverError>;
    fn create_stream(&self, non_blocking: bool) -> Result<StreamId, DriverError>;
    fn destroy_stream(&self, stream: StreamId) -> Result<(), DriverError>;
    fn create_event(&self, timing: bool) -> Result<EventId, DriverError>;
    fn destroy_event(&self, event: EventId) -> Result<(), DriverError>;
    fn device_alloc(&self, bytes: u64) -> Result<DeviceMemory, DriverError>;
    fn device_free(&self, memory: DeviceMemory) -> Result<(), DriverError>;
    fn host_alloc(&self, bytes: usize, pinned: bool) -> Result<HostBuffer, DriverError>;
    fn host_free(&self, buffer: HostBuffer) -> Result<(), DriverError>;
    fn copy_h2d(
        &self,
        dst: DeviceMemory,
        dst_offset: u64,
        src: &HostBuffer,
        src_offset: usize,
        len: usize,
        stream: StreamId,
    ) -> Result<(), DriverError>;
    fn copy_d2h(
        &self,
        dst: &HostBuffer,
        dst_offset: usize,
        src: DeviceMemory,
        src_offset: u64,
        len: usize,
        stream: StreamId,
    ) -> Result<(), DriverError>;
    /// Runtime self-test kernel: device-to-device byte copy.
    fn launch_copy(
        &self,
        dst: DeviceMemory,
        src: DeviceMemory,
        bytes: u64,
        stream: StreamId,
    ) -> Result<(), DriverError>;
    /// P4 transform pipeline (see [`TransformLaunch`]): read `src` (never
    /// written), use `scratch` privately and write all of `dst`. Asynchronous
    /// on `stream`, like the copies. The default refuses before enqueueing.
    fn launch_transform(
        &self,
        dst: DeviceMemory,
        src: DeviceMemory,
        scratch: Option<DeviceMemory>,
        launch: &TransformLaunch,
        stream: StreamId,
    ) -> Result<(), DriverError> {
        let _ = (dst, src, scratch, launch, stream);
        Err(DriverError {
            call: DriverCall::Launch,
            class: DriverErrorClass::Unsupported,
            code: 0,
        })
    }
    /// Whether [`launch_transform`](Self::launch_transform) is implemented.
    /// The runtime refuses transform work (`Unsupported`, `NotStarted`) and
    /// advertises no transform capability when this is false.
    fn supports_transform(&self) -> bool {
        false
    }
    fn record_event(&self, event: EventId, stream: StreamId) -> Result<(), DriverError>;
    fn stream_wait_event(&self, stream: StreamId, event: EventId) -> Result<(), DriverError>;
    fn event_synchronize(&self, event: EventId) -> Result<(), DriverError>;
    fn event_query(&self, event: EventId) -> Result<bool, DriverError>;
    fn event_elapsed_ms(&self, start: EventId, end: EventId) -> Result<f32, DriverError>;
    fn stream_synchronize(&self, stream: StreamId) -> Result<(), DriverError>;
    /// Called before the runtime reads or writes a staging buffer on the host.
    /// The fake driver uses it to detect access while DMA is pending.
    fn before_host_access(&self, _buffer: &HostBuffer) {}
}

/// Bytes per scalar in the P4 transform wire format (canonical little-endian
/// `to_repr` bytes of a 255-bit field element).
pub const TRANSFORM_ELEM_BYTES: usize = 32;

/// Maximum number of per-bit constants carried by a [`TransformLaunch`].
pub const TRANSFORM_MAX_POW_TABLE: usize = 32;

/// Scratch elements holding the launch constants in Montgomery form (32 root
/// powers, 32 coset powers, scale, padding to a 256-byte multiple).
pub const TRANSFORM_CONST_SLOTS: u64 = 72;

/// Descriptor of one P4 transform launch, built and validated by `ntt.rs`.
///
/// CODE (P4): the device pipeline is, for `N = 2^ntt_log`:
///
/// 1. twiddles `w^t` (`t < N/2`) from `root_powers[j] = w^(2^j)`, where `w`
///    is the exact root for this direction (already inverted for inverse);
/// 2. load: work slot `q` receives the logical input `x[i]`,
///    `i = q` when `input_bitrev` and `bitrev(q)` otherwise, multiplied by
///    `g^i` (`g^(2^j) = coset_powers[j]`) when `pre_coset`; logical indices
///    `i >= source_len` read as zero (padding);
/// 3. in-place radix-2 decimation-in-time stages, producing natural order;
/// 4. finish: output slot `p < output_len` receives work slot `bitrev(p)`
///    when `output_bitrev` and `p` otherwise, multiplied by `scale` and, when
///    `post_coset`, by `g^p`; fully reduced canonical bytes.
///
/// All constants are canonical little-endian bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransformLaunch {
    pub ntt_log: u32,
    pub source_len: u64,
    pub output_len: u64,
    pub input_bitrev: bool,
    pub output_bitrev: bool,
    pub pre_coset: bool,
    pub post_coset: bool,
    pub root_powers: [[u8; TRANSFORM_ELEM_BYTES]; TRANSFORM_MAX_POW_TABLE],
    pub coset_powers: [[u8; TRANSFORM_ELEM_BYTES]; TRANSFORM_MAX_POW_TABLE],
    pub scale: [u8; TRANSFORM_ELEM_BYTES],
}

impl TransformLaunch {
    /// Private scratch, in elements: [`TRANSFORM_CONST_SLOTS`] Montgomery
    /// constants, `N/2` twiddles, plus an `N`-element work buffer when the
    /// output is shorter than the transform (truncated inverse coset). Must
    /// equal the native `loki_gpu_ntt_scratch_bytes_v1`.
    pub fn scratch_bytes(&self) -> Option<u64> {
        if self.ntt_log >= 63 {
            return None;
        }
        let n = 1u64 << self.ntt_log;
        let elem = TRANSFORM_ELEM_BYTES as u64;
        let work = if self.output_len < n { n } else { 0 };
        TRANSFORM_CONST_SLOTS
            .checked_add(n / 2)?
            .checked_add(work)?
            .checked_mul(elem)
    }
}

// ===========================================================================
// Configuration, cancellation, telemetry
// ===========================================================================

/// Runtime configuration. Validated by [`GpuRuntime::initialize`] before the
/// device is probed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub device_ordinal: u32,
    /// Operator cap on the shared device budget. The usable budget is the
    /// lower of this cap and `free - device_headroom_bytes`.
    pub device_budget_cap: Option<u64>,
    pub device_headroom_bytes: u64,
    /// Number of staging slots, `1..=MAX_PINNED_SLOTS`.
    pub staging_slots: usize,
    /// Bytes per staging slot, `1..=PINNED_SLOT_BYTES`.
    pub staging_slot_bytes: usize,
    /// Bound on backend-retained host checkpoints.
    pub checkpoint_budget_bytes: u64,
    /// Try pinned staging first (default). `false` forces backend-owned
    /// pageable staging, e.g. to exercise the WSL2 fallback path.
    pub pinned_staging: bool,
    /// Set when environment parsing failed; initialization then refuses.
    pub invalid_environment: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            device_ordinal: 0,
            device_budget_cap: None,
            device_headroom_bytes: DEFAULT_DEVICE_HEADROOM_BYTES,
            staging_slots: INITIAL_PINNED_SLOTS,
            staging_slot_bytes: PINNED_SLOT_BYTES,
            checkpoint_budget_bytes: DEFAULT_CHECKPOINT_BUDGET_BYTES,
            pinned_staging: true,
            invalid_environment: false,
        }
    }
}

impl RuntimeConfig {
    /// Defaults plus validated environment overrides. Malformed values are
    /// rejected rather than silently ignored.
    pub fn from_environment() -> Result<Self, BackendError> {
        fn parse<T: std::str::FromStr>(key: &str) -> Result<Option<T>, BackendError> {
            match std::env::var(key) {
                Ok(raw) => raw
                    .trim()
                    .parse::<T>()
                    .map(Some)
                    .map_err(|_| err(Operation::Runtime, ErrorKind::InvalidInput, FailureEffect::NotStarted)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(std::env::VarError::NotUnicode(_)) => Err(err(
                    Operation::Runtime,
                    ErrorKind::InvalidInput,
                    FailureEffect::NotStarted,
                )),
            }
        }
        let mut cfg = Self::default();
        if let Some(v) = parse::<u64>(DEVICE_BUDGET_ENV)? {
            cfg.device_budget_cap = Some(v);
        }
        if let Some(v) = parse::<u64>(DEVICE_HEADROOM_ENV)? {
            cfg.device_headroom_bytes = v;
        }
        if let Some(v) = parse::<usize>(STAGING_SLOTS_ENV)? {
            cfg.staging_slots = v;
        }
        if let Some(v) = parse::<u32>(DEVICE_ORDINAL_ENV)? {
            cfg.device_ordinal = v;
        }
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), BackendError> {
        let ok = !self.invalid_environment
            && (1..=MAX_PINNED_SLOTS).contains(&self.staging_slots)
            && (1..=PINNED_SLOT_BYTES).contains(&self.staging_slot_bytes);
        if ok {
            Ok(())
        } else {
            Err(err(
                Operation::Runtime,
                ErrorKind::InvalidInput,
                FailureEffect::NotStarted,
            ))
        }
    }
}

/// Cooperative cancellation for internal transfers. Checked at chunk
/// boundaries; a cancelled operation drains and retires its in-flight work
/// before returning and never writes caller output.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Staging-pool capability recorded at initialization. `async_overlap_available`
/// is false when every slot fell back to pageable memory; pageable
/// `cudaMemcpyAsync` is not evidence of overlap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StagingCapability {
    pub pinned_slots: usize,
    pub pageable_slots: usize,
    pub slot_bytes: usize,
    pub async_overlap_available: bool,
}

/// Runtime telemetry. Byte counters are split by pinned/pageable path; wall
/// wait time is reported separately from GPU event time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeTelemetry {
    pub init_ns: u64,
    pub pinned_slots: usize,
    pub pageable_slots: usize,
    pub pinned_bytes: u64,
    pub pinned_probe_failures: u32,
    pub h2d_pinned_bytes: u64,
    pub h2d_pageable_bytes: u64,
    pub d2h_pinned_bytes: u64,
    pub d2h_pageable_bytes: u64,
    /// Completed public transfer/launch operations.
    pub transfers: u64,
    pub launches: u64,
    pub live_high_water: u64,
    pub quarantined_bytes: u64,
    pub quarantined_allocations: u64,
    pub poison_events: u64,
    /// Host wall time spent inside event/stream synchronization.
    pub event_wait_ns: u64,
    /// Sum of GPU event durations of completed operations.
    pub gpu_event_ms: f64,
    /// P4: completed transform launches (subset of `launches`).
    pub transform_launches: u64,
    /// P4: CUDA event time of transform launches, milliseconds.
    pub transform_kernel_ms: f64,
    /// P4: host wall time from first enqueue to completion of transform
    /// launches (excludes validation and publication), nanoseconds.
    pub transform_wall_ns: u64,
    /// P4: largest private transform scratch allocation, bytes.
    pub scratch_high_water: u64,
}

/// What remains recoverable for a resident handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LossClass {
    /// The handle is live in a healthy generation.
    Live,
    /// Device data is lost or untrusted, but the backend retained a complete
    /// host checkpoint (see [`GpuRuntime::recover_checkpoint`]).
    RecoverableFromHostCheckpoint,
    /// Device data is lost and only existed on the device: the current proof
    /// must abort unless the caller kept its own host copy.
    Unrecoverable,
    /// The handle does not match any backend record.
    Unknown,
}

/// Read-only status used by the scheduler and diagnostics. Unknown values
/// remain `None`; a CUDA-free build never invents a free-memory estimate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeSnapshot {
    pub generation: Option<DeviceGeneration>,
    pub healthy: bool,
    pub hard_budget_bytes: Option<u64>,
    pub live_bytes: Option<u64>,
    pub reserved_bytes: Option<u64>,
}

impl RuntimeSnapshot {
    const fn unavailable() -> Self {
        Self {
            generation: None,
            healthy: false,
            hard_budget_bytes: None,
            live_bytes: None,
            reserved_bytes: None,
        }
    }
}

const fn err(operation: Operation, kind: ErrorKind, effect: FailureEffect) -> BackendError {
    BackendError {
        operation,
        kind,
        effect,
    }
}

// ===========================================================================
// Ordered locks
// ===========================================================================

const LEVEL_DEVICE: u8 = 1;
const LEVEL_REGISTRY: u8 = 2;
const LEVEL_TELEMETRY: u8 = 3;

thread_local! {
    static HELD_LEVELS: Cell<u8> = const { Cell::new(0) };
}

/// Mutex with a fixed position in the runtime lock order.
#[derive(Debug)]
pub(crate) struct OrderedMutex<T> {
    level: u8,
    inner: Mutex<T>,
}

pub(crate) struct OrderedGuard<'a, T> {
    level: u8,
    guard: MutexGuard<'a, T>,
}

impl<T> OrderedMutex<T> {
    const fn new(level: u8, value: T) -> Self {
        Self {
            level,
            inner: Mutex::new(value),
        }
    }

    /// Acquire in lock order. Panics if this thread already holds a lock at an
    /// equal or higher level. A std-poisoned lock (a panic while held) is
    /// reported as `Err` so the runtime treats its state as untrusted.
    fn lock(&self) -> Result<OrderedGuard<'_, T>, ()> {
        let level = self.level;
        HELD_LEVELS.with(|held| {
            let mask = held.get();
            let not_below = !((1u8 << level) - 1);
            assert!(
                mask & not_below == 0,
                "loki-gpu runtime lock-order violation: acquiring level {level} while holding mask {mask:#b}"
            );
            held.set(mask | (1 << level));
        });
        match self.inner.lock() {
            Ok(guard) => Ok(OrderedGuard { level, guard }),
            Err(_) => {
                HELD_LEVELS.with(|held| held.set(held.get() & !(1 << level)));
                Err(())
            }
        }
    }

    fn get_mut(&mut self) -> &mut T {
        match self.inner.get_mut() {
            Ok(v) => v,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl<T> Drop for OrderedGuard<'_, T> {
    fn drop(&mut self) {
        let level = self.level;
        HELD_LEVELS.with(|held| held.set(held.get() & !(1 << level)));
    }
}

impl<T> std::ops::Deref for OrderedGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}
impl<T> std::ops::DerefMut for OrderedGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

// ===========================================================================
// Coordinator state
// ===========================================================================

#[derive(Debug)]
struct StagingSlot {
    buffer: HostBuffer,
    event: EventId,
    /// Chunk `(byte offset, length)` of an enqueued transfer that has not been
    /// confirmed complete. The slot may not be touched while `Some`.
    pending: Option<(usize, usize)>,
}

/// Device-level state (lock level 1). Raw driver handles live only here.
#[derive(Debug)]
struct Context {
    info: DeviceInfo,
    generation: DeviceGeneration,
    poisoned: bool,
    copy_stream: StreamId,
    compute_streams: [StreamId; 2],
    slots: Vec<StagingSlot>,
    copy_done: EventId,
    compute_done: EventId,
    timing_start: EventId,
    timing_end: EventId,
    memory: HashMap<AllocationId, DeviceMemory>,
    /// In-flight allocations whose completion became uncertain. Never freed.
    quarantined: Vec<DeviceMemory>,
}

#[derive(Debug, Default)]
struct DeviceState {
    current: Option<Context>,
    /// Poisoned contexts superseded by a reset. Their resources were destroyed
    /// by the reset (or are deliberately leaked); they are never freed here.
    retired: Vec<Context>,
    /// A setup attempt that ended with the context's health uncertain (a
    /// sticky driver error, or a failed destroy while cleaning up). There is
    /// no `Context`, but the driver context must not be trusted: `initialize`
    /// refuses, `reinitialize` must reset first, and `Drop` keeps the native
    /// claim. Holds the error the failed attempt reported.
    setup_failed: Option<BackendError>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Form {
    /// Canonical `PrimeField::to_repr` bytes, one element per `elem_size`.
    ScalarCanonicalRepr,
}

#[derive(Debug)]
struct AllocRecord {
    generation: DeviceGeneration,
    element: TypeId,
    elem_size: usize,
    form: Form,
    len: usize,
    bytes: u64,
    quarantined: bool,
    checkpoint: Option<Box<[u8]>>,
}

#[derive(Debug)]
struct ReservationRecord {
    generation: DeviceGeneration,
    bytes: u64,
}

/// Bookkeeping state (lock level 2). Pure metadata: no driver handles.
#[derive(Debug, Default)]
struct Registry {
    generation: Option<DeviceGeneration>,
    healthy: bool,
    budget: u64,
    live: u64,
    reserved: u64,
    checkpoint_bytes: u64,
    allocations: HashMap<AllocationId, AllocRecord>,
    reservations: HashMap<ReservationId, ReservationRecord>,
}

impl Registry {
    fn available(&self) -> u64 {
        self.budget
            .saturating_sub(self.live)
            .saturating_sub(self.reserved)
    }

    /// Require an initialized, healthy generation for new device work.
    fn require_healthy(&self, op: Operation) -> Result<DeviceGeneration, BackendError> {
        match self.generation {
            None => Err(BackendError::unsupported(op)),
            Some(_) if !self.healthy => Err(err(
                op,
                ErrorKind::Poisoned,
                FailureEffect::SessionPoisoned,
            )),
            Some(g) => Ok(g),
        }
    }

    /// Validate untrusted handle metadata against the backend-owned record.
    fn validate<T: 'static>(
        &self,
        op: Operation,
        handle: &ResidentBuffer<T>,
    ) -> Result<&AllocRecord, BackendError> {
        let invalid = err(op, ErrorKind::InvalidInput, FailureEffect::NotStarted);
        if self.generation.is_none() {
            return Err(BackendError::unsupported(op));
        }
        let record = self.allocations.get(&handle.allocation()).ok_or(invalid)?;
        if record.generation != handle.generation()
            || record.element != TypeId::of::<T>()
            || record.len != handle.len()
            || record.bytes != handle.bytes()
            || record.form != Form::ScalarCanonicalRepr
        {
            return Err(invalid);
        }
        if Some(record.generation) != self.generation {
            return Err(err(op, ErrorKind::ContextLost, FailureEffect::NotStarted));
        }
        if !self.healthy || record.quarantined {
            return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
        }
        Ok(record)
    }
}

/// Shared process runtime. All GPU modules use this coordinator so that MSM,
/// NTT, quotient work, allocations and destruction share one device
/// generation, budget and lock order.
pub struct GpuRuntime {
    driver: Option<Arc<dyn DeviceDriver>>,
    config: RuntimeConfig,
    device: OrderedMutex<DeviceState>,
    registry: OrderedMutex<Registry>,
    telemetry: OrderedMutex<RuntimeTelemetry>,
    /// Set when this runtime holds the process-unique native driver claim.
    owns_native: bool,
}

/// Process-wide identity counters. Allocation, reservation and generation
/// identities are unique across every `GpuRuntime` in the process, so a handle
/// or token minted by one runtime can never match another runtime's records.
/// Counters never wrap: exhaustion fails instead of recycling an identity.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Return the next value of `counter`, or `None` once it would wrap.
pub(crate) fn checked_next(counter: &AtomicU64) -> Option<u64> {
    let mut current = counter.load(Ordering::SeqCst);
    loop {
        let next = current.checked_add(1)?;
        match counter.compare_exchange_weak(current, next, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(v) => return Some(v),
            Err(v) => current = v,
        }
    }
}

/// Only one runtime may own the native CUDA driver at a time: the native
/// context, epoch and device reset are process-global.
#[cfg(feature = "cuda")]
static NATIVE_CLAIMED: AtomicBool = AtomicBool::new(false);

impl std::fmt::Debug for GpuRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuRuntime")
            .field("has_driver", &self.driver.is_some())
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for GpuRuntime {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// Internal failure while an operation holds private resources.
struct Abort {
    error: BackendError,
}

impl GpuRuntime {
    /// A runtime without a device driver: every device call is `Unsupported`.
    pub fn unavailable() -> Self {
        Self {
            driver: None,
            config: RuntimeConfig::default(),
            device: OrderedMutex::new(LEVEL_DEVICE, DeviceState::default()),
            registry: OrderedMutex::new(LEVEL_REGISTRY, Registry::default()),
            telemetry: OrderedMutex::new(LEVEL_TELEMETRY, RuntimeTelemetry::default()),
            owns_native: false,
        }
    }

    /// A runtime over an explicit driver. Nothing is probed or allocated until
    /// [`initialize`](Self::initialize). For tests, use the safe
    /// [`with_fake`](Self::with_fake); native code uses
    /// [`native`](Self::native) or [`process`](Self::process).
    ///
    /// # Safety
    ///
    /// The runtime must be the driver's only coordinator for the driver's
    /// whole lifetime: no other `GpuRuntime` may be built over the same
    /// driver (or over one sharing its device context), and no retained
    /// alias may call any `DeviceDriver` method (in particular `reset`,
    /// `device_free`, `host_free`, `destroy_*` or a launch) while this
    /// runtime or any handle it minted is alive. The runtime's quarantine,
    /// generation and publication guarantees, and the memory safety of
    /// native drivers, depend on this exclusivity; violating it can free or
    /// reset memory that queued device work still references.
    ///
    /// Raw injection is not callable from safe code (review finding L2):
    ///
    /// ```compile_fail,E0133
    /// use std::sync::Arc;
    /// use loki_gpu::runtime::fake::{FakeConfig, FakeDriver};
    /// use loki_gpu::runtime::{DeviceDriver, GpuRuntime, RuntimeConfig};
    /// let driver: Arc<dyn DeviceDriver> = Arc::new(FakeDriver::new(FakeConfig::default()));
    /// let _rt = GpuRuntime::with_driver(driver, RuntimeConfig::default());
    /// ```
    ///
    /// The same call inside `unsafe`, with the precondition upheld, compiles:
    ///
    /// ```
    /// use std::sync::Arc;
    /// use loki_gpu::runtime::fake::{FakeConfig, FakeDriver};
    /// use loki_gpu::runtime::{DeviceDriver, GpuRuntime, RuntimeConfig};
    /// let driver: Arc<dyn DeviceDriver> = Arc::new(FakeDriver::new(FakeConfig::default()));
    /// // SAFETY: `driver` is fresh and moved in; no alias remains.
    /// let _rt = unsafe { GpuRuntime::with_driver(driver, RuntimeConfig::default()) };
    /// ```
    pub unsafe fn with_driver(driver: Arc<dyn DeviceDriver>, config: RuntimeConfig) -> Self {
        let mut rt = Self::unavailable();
        rt.driver = Some(driver);
        rt.config = config;
        rt
    }

    /// Construct the runtime that exclusively owns the native CUDA driver.
    /// The native driver is not otherwise reachable from safe code, so no
    /// caller can reset the context or submit foreign handles behind the
    /// coordinator's back. A second live claim is refused (`InvalidInput`).
    /// The claim is released when the runtime is dropped after a healthy
    /// teardown (or without ever initializing); a runtime dropped with an
    /// uncertain context keeps the claim for the rest of the process.
    #[cfg(feature = "cuda")]
    pub fn native(config: RuntimeConfig) -> Result<Self, BackendError> {
        if NATIVE_CLAIMED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(err(
                Operation::Runtime,
                ErrorKind::InvalidInput,
                FailureEffect::NotStarted,
            ));
        }
        // SAFETY: the native driver is not reachable from safe code and the
        // `NATIVE_CLAIMED` claim above makes this the only live coordinator.
        let mut rt = unsafe { Self::with_driver(crate::ffi::native_driver(), config) };
        rt.owns_native = true;
        Ok(rt)
    }

    /// Return the process-wide runtime coordinator shared by every operation
    /// module. This does not initialize CUDA or allocate device memory. With
    /// the `cuda` feature it uses the native driver; otherwise it has none.
    pub fn process() -> &'static Self {
        static RUNTIME: OnceLock<GpuRuntime> = OnceLock::new();
        RUNTIME.get_or_init(|| {
            #[cfg(feature = "cuda")]
            {
                let config = RuntimeConfig::from_environment().unwrap_or(RuntimeConfig {
                    invalid_environment: true,
                    ..RuntimeConfig::default()
                });
                Self::native(config).unwrap_or_else(|_| Self::unavailable())
            }
            #[cfg(not(feature = "cuda"))]
            {
                Self::unavailable()
            }
        })
    }

    /// Construct the uniform error used by currently unsupported operations.
    pub(crate) const fn unsupported(operation: Operation) -> BackendError {
        BackendError::unsupported(operation)
    }

    fn lock_device(&self, op: Operation) -> Result<OrderedGuard<'_, DeviceState>, BackendError> {
        self.device
            .lock()
            .map_err(|()| err(op, ErrorKind::Internal, FailureEffect::SessionPoisoned))
    }

    fn lock_registry(&self, op: Operation) -> Result<OrderedGuard<'_, Registry>, BackendError> {
        self.registry
            .lock()
            .map_err(|()| err(op, ErrorKind::Internal, FailureEffect::SessionPoisoned))
    }

    fn with_telemetry(&self, f: impl FnOnce(&mut RuntimeTelemetry)) {
        if let Ok(mut t) = self.telemetry.lock() {
            f(&mut t);
        }
    }

    fn fresh_id(&self, op: Operation) -> Result<u64, BackendError> {
        checked_next(&NEXT_ID).ok_or(err(op, ErrorKind::Capacity, FailureEffect::NotStarted))
    }

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Probe the device, create long-lived streams/events, probe the bounded
    /// staging pool and establish the first generation. Idempotent while
    /// healthy; refuses (`Poisoned`) once poisoned — use `reinitialize`.
    pub fn initialize(&self) -> Result<DeviceGeneration, BackendError> {
        let op = Operation::Runtime;
        let driver = self.driver.as_ref().ok_or(BackendError::unsupported(op))?;
        self.config.validate()?;
        let mut device = self.lock_device(op)?;
        if let Some(ctx) = &device.current {
            return if ctx.poisoned {
                Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned))
            } else {
                Ok(ctx.generation)
            };
        }
        if device.setup_failed.is_some() {
            // An earlier setup left the context uncertain: retrying without a
            // reset could build on it. Only `reinitialize` may recover.
            return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
        }
        let started = Instant::now();
        let info = match driver.probe(self.config.device_ordinal) {
            Ok(info) => info,
            // No device, or a device this build cannot use.
            Err(e) if e.class == DriverErrorClass::Unsupported => {
                return Err(BackendError::unsupported(op));
            }
            // The query failed without corrupting anything: retryable.
            Err(e) if e.class == DriverErrorClass::Transient => {
                return Err(err(op, Self::kind_for(&e), FailureEffect::NotStarted));
            }
            // A sticky or lost context observed while probing: the native
            // session is uncertain, so recovery must reset (L1).
            Err(e) => {
                let failure = err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned);
                device.setup_failed = Some(failure);
                return Err(failure);
            }
        };
        let ctx = Self::record_setup(&mut device, self.build_context(driver.as_ref(), info, started))?;
        let generation = ctx.generation;
        device.current = Some(ctx);
        Ok(generation)
    }

    /// Persist an uncertain setup failure (effect `SessionPoisoned`) so that
    /// it survives the call; a clean `NotStarted` failure leaves no state.
    fn record_setup(
        device: &mut DeviceState,
        built: Result<Context, BackendError>,
    ) -> Result<Context, BackendError> {
        built.inspect_err(|e| {
            if e.effect == FailureEffect::SessionPoisoned {
                device.setup_failed = Some(*e);
            }
        })
    }

    /// Replace a poisoned generation: reset the driver context, then build a
    /// fresh context and generation. Old allocations stay quarantined and
    /// their handles fail with `ContextLost`; old reservations are void.
    pub fn reinitialize(&self) -> Result<DeviceGeneration, BackendError> {
        let op = Operation::Runtime;
        let driver = self.driver.as_ref().ok_or(BackendError::unsupported(op))?;
        self.config.validate()?;
        let mut device = self.lock_device(op)?;
        match &device.current {
            None if device.setup_failed.is_none() => {
                drop(device);
                return self.initialize();
            }
            // Failed, uncertain setup: no context to retire, but reset first.
            None => {}
            Some(ctx) if !ctx.poisoned => {
                return Err(err(op, ErrorKind::InvalidInput, FailureEffect::NotStarted));
            }
            Some(_) => {}
        }
        let started = Instant::now();
        // A failed reset leaves whatever uncertainty existed in place: the
        // poisoned context stays current, or `setup_failed` stays set.
        let info = driver
            .reset(self.config.device_ordinal)
            .map_err(|_| err(op, ErrorKind::ContextLost, FailureEffect::SessionPoisoned))?;
        // The reset destroyed every resource of the old context (and of any
        // failed setup); never free them again. Keep the record only for
        // diagnostics.
        if let Some(old) = device.current.take() {
            device.retired.push(old);
        }
        device.setup_failed = None;
        let ctx = Self::record_setup(&mut device, self.build_context(driver.as_ref(), info, started))?;
        let generation = ctx.generation;
        device.current = Some(ctx);
        Ok(generation)
    }

    fn build_context(
        &self,
        driver: &dyn DeviceDriver,
        info: DeviceInfo,
        started: Instant,
    ) -> Result<Context, BackendError> {
        let op = Operation::Runtime;
        if info.compute_capability != REQUIRED_COMPUTE_CAPABILITY {
            return Err(BackendError::unsupported(op));
        }

        let generation_value = checked_next(&NEXT_GENERATION)
            .ok_or(err(op, ErrorKind::Capacity, FailureEffect::NotStarted))?;

        // Track partially created resources so a failed setup releases them.
        let mut streams: Vec<StreamId> = Vec::new();
        let mut events: Vec<EventId> = Vec::new();
        let mut buffers: Vec<HostBuffer> = Vec::new();
        let kind_of = |e: &DriverError| match e.class {
            DriverErrorClass::ContextLost => ErrorKind::ContextLost,
            DriverErrorClass::Unsupported => ErrorKind::Unsupported,
            _ => match e.call {
                DriverCall::HostAlloc | DriverCall::DeviceAlloc => ErrorKind::Allocation,
                _ => ErrorKind::Internal,
            },
        };
        // Clean failure: every partial resource destroyed, effect NotStarted.
        // Uncertain failure (sticky cause, or a destroy failing during
        // cleanup): effect SessionPoisoned, which the caller persists as
        // `setup_failed` so that recovery must reset. A failed destroy
        // reports its own kind.
        let cleanup = |streams: Vec<StreamId>,
                       events: Vec<EventId>,
                       buffers: Vec<HostBuffer>,
                       e: DriverError| {
            if e.class == DriverErrorClass::Transient || e.class == DriverErrorClass::Unsupported
            {
                // Stop at the first failed destroy: the context may have just
                // become sticky, and nothing more may be destructed through it.
                let failed: Option<DriverError> = 'cleanup: {
                    for b in buffers {
                        if let Err(d) = driver.host_free(b) {
                            break 'cleanup Some(d);
                        }
                    }
                    for ev in events {
                        if let Err(d) = driver.destroy_event(ev) {
                            break 'cleanup Some(d);
                        }
                    }
                    for s in streams {
                        if let Err(d) = driver.destroy_stream(s) {
                            break 'cleanup Some(d);
                        }
                    }
                    None
                };
                match failed {
                    None => err(op, kind_of(&e), FailureEffect::NotStarted),
                    Some(d) => err(op, kind_of(&d), FailureEffect::SessionPoisoned),
                }
            } else {
                // Sticky failure during setup: leak rather than destruct
                // through an untrusted context. HostBuffer has no Drop, so
                // dropping the Rust handles does not free the allocations.
                drop(buffers);
                err(op, kind_of(&e), FailureEffect::SessionPoisoned)
            }
        };

        macro_rules! attempt {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(e) => return Err(cleanup(streams, events, buffers, e)),
                }
            };
        }

        let copy_stream = attempt!(driver.create_stream(true));
        streams.push(copy_stream);
        let c0 = attempt!(driver.create_stream(true));
        streams.push(c0);
        let c1 = attempt!(driver.create_stream(true));
        streams.push(c1);
        let copy_done = attempt!(driver.create_event(false));
        events.push(copy_done);
        let compute_done = attempt!(driver.create_event(false));
        events.push(compute_done);
        let timing_start = attempt!(driver.create_event(true));
        events.push(timing_start);
        let timing_end = attempt!(driver.create_event(true));
        events.push(timing_end);

        // Probe the bounded pinned budget slot by slot. A pinned failure falls
        // back to backend-owned pageable staging and is recorded.
        let mut probe_failures = 0u32;
        let mut slot_events = Vec::new();
        for _ in 0..self.config.staging_slots {
            let pinned = if self.config.pinned_staging {
                driver.host_alloc(self.config.staging_slot_bytes, true)
            } else {
                Err(DriverError {
                    call: DriverCall::HostAlloc,
                    class: DriverErrorClass::Unsupported,
                    code: 0,
                })
            };
            let buffer = match pinned {
                Ok(b) => b,
                Err(e) if e.class == DriverErrorClass::Transient
                    || e.class == DriverErrorClass::Unsupported =>
                {
                    if self.config.pinned_staging {
                        probe_failures += 1;
                    }
                    attempt!(driver.host_alloc(self.config.staging_slot_bytes, false))
                }
                Err(e) => return Err(cleanup(streams, events, buffers, e)),
            };
            buffers.push(buffer);
            let ev = attempt!(driver.create_event(false));
            events.push(ev);
            slot_events.push(ev);
        }

        let slots: Vec<StagingSlot> = buffers
            .into_iter()
            .zip(slot_events)
            .map(|(buffer, event)| StagingSlot {
                buffer,
                event,
                pending: None,
            })
            .collect();
        let pinned_slots = slots.iter().filter(|s| s.buffer.pinned()).count();
        let pageable_slots = slots.len() - pinned_slots;

        let generation = DeviceGeneration {
            device_ordinal: info.ordinal,
            context_epoch: info.context_epoch,
            generation: generation_value,
        };
        let budget = info
            .free_bytes
            .saturating_sub(self.config.device_headroom_bytes)
            .min(self.config.device_budget_cap.unwrap_or(u64::MAX));

        {
            let mut reg = self.lock_registry(op)?;
            reg.generation = Some(generation);
            reg.healthy = true;
            reg.budget = budget;
            reg.live = 0;
            reg.reserved = 0;
        }
        let slot_bytes = self.config.staging_slot_bytes as u64;
        let init_ns = started.elapsed().as_nanos().max(1) as u64;
        self.with_telemetry(|t| {
            t.init_ns = init_ns;
            t.pinned_slots = pinned_slots;
            t.pageable_slots = pageable_slots;
            t.pinned_bytes = pinned_slots as u64 * slot_bytes;
            t.pinned_probe_failures += probe_failures;
        });

        Ok(Context {
            info,
            generation,
            poisoned: false,
            copy_stream,
            compute_streams: [c0, c1],
            slots,
            copy_done,
            compute_done,
            timing_start,
            timing_end,
            memory: HashMap::new(),
            quarantined: Vec::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Observation
    // -----------------------------------------------------------------------

    /// Report the current runtime state. A poisoned metadata lock is reported
    /// as unavailable rather than making capabilities look better.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        match self.registry.lock() {
            Ok(reg) => match reg.generation {
                None => RuntimeSnapshot::unavailable(),
                Some(g) => RuntimeSnapshot {
                    generation: Some(g),
                    healthy: reg.healthy,
                    hard_budget_bytes: Some(reg.budget),
                    live_bytes: Some(reg.live),
                    reserved_bytes: Some(reg.reserved),
                },
            },
            Err(()) => RuntimeSnapshot::unavailable(),
        }
    }

    /// Advertise only what this runtime can currently guarantee: resident
    /// scalar buffers on a healthy generation, plus (P4) BLS12-381 scalar
    /// transforms when the driver implements them. Other arithmetic
    /// operations remain unadvertised until their packages pass their gates.
    pub fn capabilities(&self) -> BackendCapabilities {
        let snap = self.snapshot();
        if snap.generation.is_some() && snap.healthy {
            let transform = self.driver_supports_transform();
            BackendCapabilities {
                resident_buffers: true,
                host_transform: transform,
                resident_transform: transform,
                supported_arithmetic: if transform {
                    vec![crate::ntt::TRANSFORM_ARITHMETIC]
                } else {
                    Vec::new()
                },
                max_transform_log: transform.then_some(crate::ntt::MAX_TRANSFORM_LOG),
                device_memory_budget: snap.hard_budget_bytes,
                ..BackendCapabilities::default()
            }
        } else {
            BackendCapabilities::default()
        }
    }

    pub fn telemetry(&self) -> RuntimeTelemetry {
        self.telemetry
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    /// Staging capability of the current context, if initialized.
    pub fn staging_capability(&self) -> Option<StagingCapability> {
        let device = self.device.lock().ok()?;
        let ctx = device.current.as_ref()?;
        let pinned = ctx.slots.iter().filter(|s| s.buffer.pinned()).count();
        Some(StagingCapability {
            pinned_slots: pinned,
            pageable_slots: ctx.slots.len() - pinned,
            slot_bytes: self.config.staging_slot_bytes,
            async_overlap_available: pinned > 0,
        })
    }

    /// Device properties of the current context, if initialized.
    pub fn device_info(&self) -> Option<DeviceInfo> {
        let device = self.device.lock().ok()?;
        device.current.as_ref().map(|c| c.info.clone())
    }

    /// Classify what is recoverable for `handle` (untrusted metadata).
    pub fn loss_class<T: 'static>(&self, handle: &ResidentBuffer<T>) -> LossClass {
        let Ok(reg) = self.registry.lock() else {
            return LossClass::Unknown;
        };
        let Some(record) = reg.allocations.get(&handle.allocation()) else {
            return LossClass::Unknown;
        };
        if record.generation != handle.generation()
            || record.element != TypeId::of::<T>()
            || record.len != handle.len()
            || record.bytes != handle.bytes()
        {
            return LossClass::Unknown;
        }
        let live = Some(record.generation) == reg.generation && reg.healthy && !record.quarantined;
        if live {
            LossClass::Live
        } else if record.checkpoint.is_some() {
            LossClass::RecoverableFromHostCheckpoint
        } else {
            LossClass::Unrecoverable
        }
    }

    // -----------------------------------------------------------------------
    // Poisoning
    // -----------------------------------------------------------------------

    /// Poison the current generation: no further device work, every
    /// allocation quarantined, nothing freed through this context. `extra`
    /// covers an in-flight allocation that has no record yet.
    fn poison(&self, ctx: &mut Context, extra: Option<(DeviceMemory, u64)>) {
        let first = !ctx.poisoned;
        ctx.poisoned = true;
        let mut quarantined_bytes = 0u64;
        let mut quarantined_allocations = 0u64;
        if let Some((memory, bytes)) = extra {
            // Keep the raw handle so it is never reused or freed.
            ctx.quarantined.push(memory);
            quarantined_bytes += bytes;
            quarantined_allocations += 1;
        }
        if let Ok(mut reg) = self.registry.lock() {
            if Some(ctx.generation) == reg.generation {
                reg.healthy = false;
            }
            let generation = ctx.generation;
            for record in reg.allocations.values_mut() {
                if record.generation == generation && !record.quarantined {
                    record.quarantined = true;
                    quarantined_bytes += record.bytes;
                    quarantined_allocations += 1;
                }
            }
        }
        self.with_telemetry(|t| {
            if first {
                t.poison_events += 1;
            }
            t.quarantined_bytes += quarantined_bytes;
            t.quarantined_allocations += quarantined_allocations;
        });
    }

    fn kind_for(e: &DriverError) -> ErrorKind {
        match e.class {
            DriverErrorClass::ContextLost => ErrorKind::ContextLost,
            DriverErrorClass::Unsupported => ErrorKind::Unsupported,
            _ => match e.call {
                DriverCall::DeviceAlloc | DriverCall::HostAlloc => ErrorKind::Allocation,
                DriverCall::CopyH2D | DriverCall::CopyD2H => ErrorKind::Transfer,
                DriverCall::Launch => ErrorKind::Kernel,
                DriverCall::EventRecord
                | DriverCall::StreamWait
                | DriverCall::EventSync
                | DriverCall::EventQuery
                | DriverCall::EventElapsed
                | DriverCall::StreamSync => ErrorKind::Synchronization,
                _ => ErrorKind::Internal,
            },
        }
    }

    /// Whether a driver error leaves completion of earlier work uncertain.
    fn is_uncertain(e: &DriverError) -> bool {
        matches!(
            e.class,
            DriverErrorClass::Poisoned | DriverErrorClass::ContextLost
        ) || matches!(
            e.call,
            DriverCall::EventSync | DriverCall::EventQuery | DriverCall::StreamSync
        )
    }

    #[allow(clippy::too_many_arguments)]
    /// Handle a driver error after an enqueue attempt on `stream`: poison on
    /// uncertainty, otherwise drain the stream and free `memory` (if any).
    fn fail_after_enqueue(
        &self,
        driver: &dyn DeviceDriver,
        ctx: &mut Context,
        op: Operation,
        kind: ErrorKind,
        cause: Option<DriverError>,
        stream: StreamId,
        memory: Option<(DeviceMemory, u64)>,
    ) -> Abort {
        if let Some(e) = cause {
            if Self::is_uncertain(&e) {
                self.poison(ctx, memory);
                return Abort {
                    error: err(op, kind, FailureEffect::SessionPoisoned),
                };
            }
        }
        let started = Instant::now();
        let drained = driver.stream_synchronize(stream);
        self.with_telemetry(|t| t.event_wait_ns += started.elapsed().as_nanos() as u64);
        match drained {
            Ok(()) => {
                for slot in &mut ctx.slots {
                    slot.pending = None;
                }
                if let Some((m, _)) = memory {
                    if let Err(e) = driver.device_free(m) {
                        self.poison(ctx, Some((m, 0)));
                        return Abort {
                            error: err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned),
                        };
                    }
                }
                Abort {
                    error: err(op, kind, FailureEffect::InputsPreserved),
                }
            }
            Err(_) => {
                // The drain itself failed: completion is uncertain.
                self.poison(ctx, memory);
                Abort {
                    error: err(op, ErrorKind::Synchronization, FailureEffect::SessionPoisoned),
                }
            }
        }
    }

    /// Optional (telemetry) driver call. A transient or unsupported failure is
    /// ignored, but a sticky failure is a mandatory health transition: the
    /// generation is poisoned and the operation must not publish.
    fn optional<T>(
        &self,
        ctx: &mut Context,
        op: Operation,
        result: Result<T, DriverError>,
        memory: Option<(DeviceMemory, u64)>,
    ) -> Result<Option<T>, Abort> {
        match result {
            Ok(v) => Ok(Some(v)),
            Err(e) if matches!(e.class, DriverErrorClass::Transient | DriverErrorClass::Unsupported) => {
                Ok(None)
            }
            Err(e) => {
                self.poison(ctx, memory);
                Err(Abort {
                    error: err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned),
                })
            }
        }
    }

    /// Wait for `event` with uncertainty handling: any failure poisons.
    fn wait_event(
        &self,
        driver: &dyn DeviceDriver,
        ctx: &mut Context,
        op: Operation,
        event: EventId,
        memory: Option<(DeviceMemory, u64)>,
    ) -> Result<(), Abort> {
        let started = Instant::now();
        let r = driver.event_synchronize(event);
        self.with_telemetry(|t| t.event_wait_ns += started.elapsed().as_nanos() as u64);
        r.map_err(|e| {
            self.poison(ctx, memory);
            Abort {
                error: err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned),
            }
        })
    }

    fn release_internal_reservation(&self, bytes: u64, checkpoint: u64) {
        if let Ok(mut reg) = self.registry.lock() {
            reg.reserved = reg.reserved.saturating_sub(bytes);
            reg.checkpoint_bytes = reg.checkpoint_bytes.saturating_sub(checkpoint);
        }
    }

    // -----------------------------------------------------------------------
    // Upload
    // -----------------------------------------------------------------------

    /// Upload with cooperative cancellation. See [`ResidentBackend::upload_scalars`].
    pub fn upload_scalars_cancellable<F: PrimeField>(
        &self,
        values: &[F],
        cancel: &CancelToken,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        self.upload_impl(values, cancel, false)
    }

    /// Upload and retain a backend-owned host checkpoint of the canonical
    /// bytes, bounded by `checkpoint_budget_bytes`. After context loss the
    /// checkpoint can be returned by [`recover_checkpoint`](Self::recover_checkpoint).
    pub fn upload_scalars_with_checkpoint<F: PrimeField>(
        &self,
        values: &[F],
    ) -> Result<ResidentBuffer<F>, BackendError> {
        self.upload_impl(values, &CancelToken::new(), true)
    }

    fn upload_impl<F: PrimeField>(
        &self,
        values: &[F],
        cancel: &CancelToken,
        checkpoint: bool,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        let op = Operation::Upload;
        let driver = self.driver.as_deref().ok_or(BackendError::unsupported(op))?;
        let not_started = |kind| err(op, kind, FailureEffect::NotStarted);
        // Admission checks before any resource is touched.
        {
            let reg = self.lock_registry(op)?;
            reg.require_healthy(op)?;
        }
        if values.is_empty() {
            return Err(not_started(ErrorKind::InvalidInput));
        }
        let elem_size = F::Repr::default().as_ref().len();
        let chunk_elems = self.config.staging_slot_bytes / elem_size.max(1);
        if elem_size == 0 || chunk_elems == 0 {
            return Err(not_started(ErrorKind::Unsupported));
        }
        let bytes = (values.len() as u64)
            .checked_mul(elem_size as u64)
            .ok_or(not_started(ErrorKind::Capacity))?;
        let host_bytes: usize = usize::try_from(bytes).map_err(|_| not_started(ErrorKind::Capacity))?;
        if cancel.is_cancelled() {
            return Err(not_started(ErrorKind::Cancelled));
        }
        // Identity is allocated before any device work so exhaustion can
        // never strand a completed transfer.
        let id = AllocationId(self.fresh_id(op)?);

        let mut device = self.lock_device(op)?;
        let ctx = device
            .current
            .as_mut()
            .ok_or(BackendError::unsupported(op))?;
        let checkpoint_bytes = if checkpoint { bytes } else { 0 };
        let generation = {
            let mut reg = self.lock_registry(op)?;
            let generation = reg.require_healthy(op)?;
            if ctx.poisoned {
                return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
            }
            if bytes > reg.available() {
                return Err(not_started(ErrorKind::Capacity));
            }
            let cp_total = reg
                .checkpoint_bytes
                .checked_add(checkpoint_bytes)
                .ok_or(not_started(ErrorKind::Capacity))?;
            if cp_total > self.config.checkpoint_budget_bytes {
                return Err(not_started(ErrorKind::Capacity));
            }
            reg.reserved += bytes;
            reg.checkpoint_bytes = cp_total;
            generation
        };

        let memory = match driver.device_alloc(bytes) {
            Ok(m) => m,
            Err(e) => {
                self.release_internal_reservation(bytes, checkpoint_bytes);
                if Self::is_uncertain(&e) {
                    self.poison(ctx, None);
                    return Err(err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned));
                }
                return Err(not_started(Self::kind_for(&e)));
            }
        };

        let mut snapshot: Option<Vec<u8>> = if checkpoint {
            Some(Vec::with_capacity(host_bytes))
        } else {
            None
        };
        let result = self.upload_chunks(
            driver, ctx, op, values, elem_size, chunk_elems, memory, bytes, cancel,
            snapshot.as_mut(),
        );
        match result {
            Ok(()) => {}
            Err(abort) => {
                self.release_internal_reservation(bytes, checkpoint_bytes);
                return Err(abort.error);
            }
        }

        // Publication: the completion event retired successfully.
        ctx.memory.insert(id, memory);
        let live = {
            let mut reg = self.lock_registry(op)?;
            reg.reserved = reg.reserved.saturating_sub(bytes);
            reg.live += bytes;
            reg.allocations.insert(
                id,
                AllocRecord {
                    generation,
                    element: TypeId::of::<F>(),
                    elem_size,
                    form: Form::ScalarCanonicalRepr,
                    len: values.len(),
                    bytes,
                    quarantined: false,
                    checkpoint: snapshot.map(Vec::into_boxed_slice),
                },
            );
            reg.live
        };
        self.with_telemetry(|t| {
            t.transfers += 1;
            t.live_high_water = t.live_high_water.max(live);
        });
        Ok(ResidentBuffer::from_untrusted_metadata(
            generation,
            id,
            values.len(),
            bytes,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn upload_chunks<F: PrimeField>(
        &self,
        driver: &dyn DeviceDriver,
        ctx: &mut Context,
        op: Operation,
        values: &[F],
        elem_size: usize,
        chunk_elems: usize,
        memory: DeviceMemory,
        bytes: u64,
        cancel: &CancelToken,
        mut snapshot: Option<&mut Vec<u8>>,
    ) -> Result<(), Abort> {
        let stream = ctx.copy_stream;
        let nslots = ctx.slots.len();
        let mut enqueued = false;
        let mem = Some((memory, bytes));
        let r = driver.record_event(ctx.timing_start, stream);
        let mut timing = self.optional(ctx, op, r, mem)?.is_some();
        for (index, chunk) in values.chunks(chunk_elems).enumerate() {
            if cancel.is_cancelled() {
                let kind = ErrorKind::Cancelled;
                return Err(if enqueued {
                    self.fail_after_enqueue(driver, ctx, op, kind, None, stream, Some((memory, bytes)))
                } else {
                    // Nothing enqueued yet; the allocation is unreferenced.
                    match driver.device_free(memory) {
                        Ok(()) => Abort {
                            error: err(op, kind, FailureEffect::NotStarted),
                        },
                        Err(e) => {
                            self.poison(ctx, Some((memory, bytes)));
                            Abort {
                                error: err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned),
                            }
                        }
                    }
                });
            }
            let s = index % nslots;
            if ctx.slots[s].pending.is_some() {
                let event = ctx.slots[s].event;
                self.wait_event(driver, ctx, op, event, Some((memory, bytes)))?;
                ctx.slots[s].pending = None;
            }
            let len = chunk.len() * elem_size;
            let offset = index * chunk_elems * elem_size;
            {
                let slot = &ctx.slots[s];
                driver.before_host_access(&slot.buffer);
                debug_assert!(len <= slot.buffer.len);
                // SAFETY: the slot buffer is backend-owned, `len` bytes fit in
                // it (chunk_elems * elem_size <= slot bytes), and its last
                // transfer has retired (pending == None), so no DMA reads it.
                let dst = unsafe { std::slice::from_raw_parts_mut(slot.buffer.ptr, len) };
                for (i, value) in chunk.iter().enumerate() {
                    let repr = value.to_repr();
                    let src = repr.as_ref();
                    if src.len() != elem_size {
                        return Err(self.fail_after_enqueue(
                            driver, ctx, op, ErrorKind::InvalidInput, None, stream,
                            Some((memory, bytes)),
                        ));
                    }
                    dst[i * elem_size..(i + 1) * elem_size].copy_from_slice(src);
                }
                if let Some(cp) = snapshot.as_deref_mut() {
                    cp.extend_from_slice(dst);
                }
            }
            enqueued = true;
            let pinned = ctx.slots[s].buffer.pinned();
            if let Err(e) = driver.copy_h2d(memory, offset as u64, &ctx.slots[s].buffer, 0, len, stream) {
                return Err(self.fail_after_enqueue(
                    driver, ctx, op, Self::kind_for(&e), Some(e), stream, Some((memory, bytes)),
                ));
            }
            if let Err(e) = driver.record_event(ctx.slots[s].event, stream) {
                return Err(self.fail_after_enqueue(
                    driver, ctx, op, Self::kind_for(&e), Some(e), stream, Some((memory, bytes)),
                ));
            }
            ctx.slots[s].pending = Some((offset, len));
            self.with_telemetry(|t| {
                if pinned {
                    t.h2d_pinned_bytes += len as u64;
                } else {
                    t.h2d_pageable_bytes += len as u64;
                }
            });
        }
        let r = driver.record_event(ctx.timing_end, stream);
        timing &= self.optional(ctx, op, r, mem)?.is_some();
        let done = ctx.copy_done;
        if let Err(e) = driver.record_event(done, stream) {
            return Err(self.fail_after_enqueue(
                driver, ctx, op, Self::kind_for(&e), Some(e), stream, Some((memory, bytes)),
            ));
        }
        self.wait_event(driver, ctx, op, done, Some((memory, bytes)))?;
        for slot in &mut ctx.slots {
            slot.pending = None;
        }
        if timing {
            let r = driver.event_elapsed_ms(ctx.timing_start, ctx.timing_end);
            if let Some(ms) = self.optional(ctx, op, r, mem)? {
                self.with_telemetry(|t| t.gpu_event_ms += f64::from(ms));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Download
    // -----------------------------------------------------------------------

    /// Download with cooperative cancellation. See [`ResidentBackend::download_scalars`].
    pub fn download_scalars_cancellable<F: PrimeField>(
        &self,
        source: &ResidentBuffer<F>,
        cancel: &CancelToken,
    ) -> Result<Vec<F>, BackendError> {
        let op = Operation::Download;
        let driver = self.driver.as_deref().ok_or(BackendError::unsupported(op))?;
        if cancel.is_cancelled() {
            return Err(err(op, ErrorKind::Cancelled, FailureEffect::NotStarted));
        }
        let mut device = self.lock_device(op)?;
        let ctx = device
            .current
            .as_mut()
            .ok_or(BackendError::unsupported(op))?;
        let (elem_size, bytes) = {
            let reg = self.lock_registry(op)?;
            let record = reg.validate(op, source)?;
            (record.elem_size, record.bytes)
        };
        if ctx.poisoned {
            return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
        }
        let memory = *ctx
            .memory
            .get(&source.allocation())
            .ok_or(err(op, ErrorKind::Internal, FailureEffect::NotStarted))?;
        let chunk_elems = self.config.staging_slot_bytes / elem_size;
        let chunk_bytes = chunk_elems * elem_size;
        let total = bytes as usize;

        // Private host result; never visible to the caller unless complete.
        let mut out = vec![0u8; total];
        let stream = ctx.copy_stream;
        let nslots = ctx.slots.len();

        let harvest = |ctx: &mut Context, s: usize, out: &mut [u8]| {
            if let Some((offset, len)) = ctx.slots[s].pending.take() {
                let slot = &ctx.slots[s];
                driver.before_host_access(&slot.buffer);
                // SAFETY: the slot's completion event has been synchronized, so
                // its DMA has retired; `len` bytes were written into it.
                let src = unsafe { std::slice::from_raw_parts(slot.buffer.ptr, len) };
                out[offset..offset + len].copy_from_slice(src);
            }
        };

        let mut offset = 0usize;
        let mut index = 0usize;
        let mut enqueued = false;
        let r = driver.record_event(ctx.timing_start, stream);
        let mut timing = self.optional(ctx, op, r, None).map_err(|a| a.error)?.is_some();
        while offset < total {
            if cancel.is_cancelled() {
                let kind = ErrorKind::Cancelled;
                if !enqueued {
                    return Err(err(op, kind, FailureEffect::NotStarted));
                }
                return Err(self
                    .fail_after_enqueue(driver, ctx, op, kind, None, stream, None)
                    .error);
            }
            let s = index % nslots;
            if ctx.slots[s].pending.is_some() {
                let event = ctx.slots[s].event;
                self.wait_event(driver, ctx, op, event, None)
                    .map_err(|a| a.error)?;
                harvest(ctx, s, &mut out);
            }
            let len = chunk_bytes.min(total - offset);
            enqueued = true;
            let pinned = ctx.slots[s].buffer.pinned();
            if let Err(e) = driver.copy_d2h(&ctx.slots[s].buffer, 0, memory, offset as u64, len, stream) {
                return Err(self
                    .fail_after_enqueue(driver, ctx, op, Self::kind_for(&e), Some(e), stream, None)
                    .error);
            }
            if let Err(e) = driver.record_event(ctx.slots[s].event, stream) {
                return Err(self
                    .fail_after_enqueue(driver, ctx, op, Self::kind_for(&e), Some(e), stream, None)
                    .error);
            }
            ctx.slots[s].pending = Some((offset, len));
            self.with_telemetry(|t| {
                if pinned {
                    t.d2h_pinned_bytes += len as u64;
                } else {
                    t.d2h_pageable_bytes += len as u64;
                }
            });
            offset += len;
            index += 1;
        }
        let r = driver.record_event(ctx.timing_end, stream);
        timing &= self.optional(ctx, op, r, None).map_err(|a| a.error)?.is_some();
        let done = ctx.copy_done;
        if let Err(e) = driver.record_event(done, stream) {
            return Err(self
                .fail_after_enqueue(driver, ctx, op, Self::kind_for(&e), Some(e), stream, None)
                .error);
        }
        self.wait_event(driver, ctx, op, done, None)
            .map_err(|a| a.error)?;
        for s in 0..nslots {
            harvest(ctx, s, &mut out);
        }
        if timing {
            let r = driver.event_elapsed_ms(ctx.timing_start, ctx.timing_end);
            if let Some(ms) = self.optional(ctx, op, r, None).map_err(|a| a.error)? {
                self.with_telemetry(|t| t.gpu_event_ms += f64::from(ms));
            }
        }
        drop(device);

        // Decode and validate every element before publishing the vector.
        let decoded = decode::<F>(&out, elem_size)
            .ok_or(err(op, ErrorKind::Internal, FailureEffect::InputsPreserved))?;
        self.with_telemetry(|t| t.transfers += 1);
        Ok(decoded)
    }

    // -----------------------------------------------------------------------
    // Launch (runtime self-test kernel)
    // -----------------------------------------------------------------------

    /// Runtime self-test launch: copy `source` into a fresh allocation on a
    /// compute stream and publish the new handle only after the completion
    /// event retires. This exercises the reserve → allocate → launch → sync →
    /// publish path that arithmetic kernels will use; it is not arithmetic.
    pub fn duplicate_scalars<F: PrimeField>(
        &self,
        source: &ResidentBuffer<F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        let op = Operation::ResidentTransform;
        let driver = self.driver.as_deref().ok_or(BackendError::unsupported(op))?;
        let id = AllocationId(self.fresh_id(op)?);
        let mut device = self.lock_device(op)?;
        let ctx = device
            .current
            .as_mut()
            .ok_or(BackendError::unsupported(op))?;
        let (generation, elem_size, bytes) = {
            let mut reg = self.lock_registry(op)?;
            let record = reg.validate(op, source)?;
            let (generation, elem_size, bytes) = (record.generation, record.elem_size, record.bytes);
            if ctx.poisoned {
                return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
            }
            if bytes > reg.available() {
                return Err(err(op, ErrorKind::Capacity, FailureEffect::NotStarted));
            }
            reg.reserved += bytes;
            (generation, elem_size, bytes)
        };
        let src = *ctx
            .memory
            .get(&source.allocation())
            .ok_or(err(op, ErrorKind::Internal, FailureEffect::NotStarted))?;
        let dst = match driver.device_alloc(bytes) {
            Ok(m) => m,
            Err(e) => {
                self.release_internal_reservation(bytes, 0);
                if Self::is_uncertain(&e) {
                    self.poison(ctx, None);
                    return Err(err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned));
                }
                return Err(err(op, Self::kind_for(&e), FailureEffect::NotStarted));
            }
        };
        let stream = ctx.compute_streams[0];
        let step = |ctx: &mut Context, r: Result<(), DriverError>| -> Result<(), Abort> {
            r.map_err(|e| {
                self.fail_after_enqueue(
                    driver, ctx, op, Self::kind_for(&e), Some(e), stream, Some((dst, bytes)),
                )
            })
        };
        // Order the compute stream after any earlier copy-stream work on the
        // source (v1 uploads already retired; this keeps the dependency
        // explicit for future asynchronous producers).
        let (copy_done, copy_stream, done) = (ctx.copy_done, ctx.copy_stream, ctx.compute_done);
        let (timing_start, timing_end) = (ctx.timing_start, ctx.timing_end);
        let mut outcome = || -> Result<(), Abort> {
            let r = driver.record_event(copy_done, copy_stream);
            step(ctx, r)?;
            let r = driver.stream_wait_event(stream, copy_done);
            step(ctx, r)?;
            let mem = Some((dst, bytes));
            let r = driver.record_event(timing_start, stream);
            let timing = self.optional(ctx, op, r, mem)?.is_some();
            let r = driver.launch_copy(dst, src, bytes, stream);
            step(ctx, r)?;
            let r = driver.record_event(timing_end, stream);
            let timing = self.optional(ctx, op, r, mem)?.is_some() && timing;
            let r = driver.record_event(done, stream);
            step(ctx, r)?;
            self.wait_event(driver, ctx, op, done, mem)?;
            if timing {
                let r = driver.event_elapsed_ms(timing_start, timing_end);
                if let Some(ms) = self.optional(ctx, op, r, mem)? {
                    self.with_telemetry(|t| t.gpu_event_ms += f64::from(ms));
                }
            }
            Ok(())
        };
        let outcome = outcome();
        if let Err(abort) = outcome {
            self.release_internal_reservation(bytes, 0);
            return Err(abort.error);
        }

        ctx.memory.insert(id, dst);
        let live = {
            let mut reg = self.lock_registry(op)?;
            reg.reserved = reg.reserved.saturating_sub(bytes);
            reg.live += bytes;
            reg.allocations.insert(
                id,
                AllocRecord {
                    generation,
                    element: TypeId::of::<F>(),
                    elem_size,
                    form: Form::ScalarCanonicalRepr,
                    len: source.len(),
                    bytes,
                    quarantined: false,
                    checkpoint: None,
                },
            );
            reg.live
        };
        self.with_telemetry(|t| {
            t.launches += 1;
            t.live_high_water = t.live_high_water.max(live);
        });
        Ok(ResidentBuffer::from_untrusted_metadata(
            generation,
            id,
            source.len(),
            bytes,
        ))
    }

    // -----------------------------------------------------------------------
    // Transform launch (P4)
    // -----------------------------------------------------------------------

    /// Whether the driver implements P4 transforms (no device access).
    pub(crate) fn driver_supports_transform(&self) -> bool {
        self.driver.as_ref().is_some_and(|d| d.supports_transform())
    }

    /// CODE (P4): run one transform launch from `source` into a fresh output
    /// allocation of `launch.output_len` elements and publish it only after
    /// the completion event retires.
    ///
    /// Output and scratch are reserved together for their full size before
    /// either is allocated; scratch is freed after completion and never
    /// published. `source` is only read. Error effects follow
    /// [`duplicate_scalars`](Self::duplicate_scalars): `NotStarted` before an
    /// enqueue, `InputsPreserved` after a successful drain, `SessionPoisoned`
    /// when completion is uncertain (output and scratch quarantined).
    pub(crate) fn launch_transform_fresh<F: PrimeField>(
        &self,
        source: &ResidentBuffer<F>,
        launch: &TransformLaunch,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        let op = Operation::ResidentTransform;
        let driver = self.driver.as_deref().ok_or(BackendError::unsupported(op))?;
        let not_started = |kind| err(op, kind, FailureEffect::NotStarted);
        if !driver.supports_transform() {
            return Err(BackendError::unsupported(op));
        }
        let elem = TRANSFORM_ELEM_BYTES as u64;
        let out_len = usize::try_from(launch.output_len)
            .map_err(|_| not_started(ErrorKind::InvalidInput))?;
        let out_bytes = launch
            .output_len
            .checked_mul(elem)
            .ok_or(not_started(ErrorKind::Capacity))?;
        let scratch_bytes = launch
            .scratch_bytes()
            .ok_or(not_started(ErrorKind::Capacity))?;
        let total = out_bytes
            .checked_add(scratch_bytes)
            .ok_or(not_started(ErrorKind::Capacity))?;
        if out_len == 0 {
            return Err(not_started(ErrorKind::InvalidInput));
        }
        // Identity is allocated before any device work so exhaustion can
        // never strand a completed launch.
        let id = AllocationId(self.fresh_id(op)?);

        let mut device = self.lock_device(op)?;
        let ctx = device
            .current
            .as_mut()
            .ok_or(BackendError::unsupported(op))?;
        let generation = {
            let mut reg = self.lock_registry(op)?;
            let record = reg.validate(op, source)?;
            if record.elem_size != TRANSFORM_ELEM_BYTES
                || record.len as u64 != launch.source_len
            {
                return Err(not_started(ErrorKind::InvalidInput));
            }
            let generation = record.generation;
            if ctx.poisoned {
                return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
            }
            if total > reg.available() {
                return Err(not_started(ErrorKind::Capacity));
            }
            reg.reserved += total;
            generation
        };
        let src = *ctx
            .memory
            .get(&source.allocation())
            .ok_or(not_started(ErrorKind::Internal))?;

        // Allocate output, then scratch. A failed allocation frees what was
        // already allocated (nothing has been enqueued) and releases the
        // whole reservation.
        let alloc_fail = |this: &Self, ctx: &mut Context, e: DriverError, held: Option<(DeviceMemory, u64)>| {
            this.release_internal_reservation(total, 0);
            if Self::is_uncertain(&e) {
                this.poison(ctx, held);
                return err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned);
            }
            if let Some((m, b)) = held {
                // The cleanup failure, not the allocation failure, decides the
                // reported kind once the context is untrusted.
                if let Err(fe) = driver.device_free(m) {
                    this.poison(ctx, Some((m, b)));
                    return err(op, Self::kind_for(&fe), FailureEffect::SessionPoisoned);
                }
            }
            err(op, Self::kind_for(&e), FailureEffect::NotStarted)
        };
        let dst = match driver.device_alloc(out_bytes) {
            Ok(m) => m,
            Err(e) => return Err(alloc_fail(self, ctx, e, None)),
        };
        let scratch = if scratch_bytes > 0 {
            match driver.device_alloc(scratch_bytes) {
                Ok(m) => Some(m),
                Err(e) => return Err(alloc_fail(self, ctx, e, Some((dst, out_bytes)))),
            }
        } else {
            None
        };

        let stream = ctx.compute_streams[0];
        let (copy_done, copy_stream, done) = (ctx.copy_done, ctx.copy_stream, ctx.compute_done);
        let (timing_start, timing_end) = (ctx.timing_start, ctx.timing_end);
        let started = Instant::now();
        let outcome = (|| -> Result<(), Abort> {
            let step = |this: &Self, ctx: &mut Context, r: Result<(), DriverError>| {
                r.map_err(|e| {
                    this.fail_after_enqueue(
                        driver, ctx, op, Self::kind_for(&e), Some(e), stream, Some((dst, out_bytes)),
                    )
                })
            };
            // Order the compute stream after earlier copy-stream work.
            let r = driver.record_event(copy_done, copy_stream);
            step(self, ctx, r)?;
            let r = driver.stream_wait_event(stream, copy_done);
            step(self, ctx, r)?;
            // Timing calls are optional, but a sticky failure poisons (the
            // scratch retirement below then quarantines scratch as well).
            let mem = Some((dst, out_bytes));
            let r = driver.record_event(timing_start, stream);
            let timing = self.optional(ctx, op, r, mem)?.is_some();
            let r = driver.launch_transform(dst, src, scratch, launch, stream);
            step(self, ctx, r)?;
            let r = driver.record_event(timing_end, stream);
            let timing = self.optional(ctx, op, r, mem)?.is_some() && timing;
            let r = driver.record_event(done, stream);
            step(self, ctx, r)?;
            self.wait_event(driver, ctx, op, done, mem)?;
            if timing {
                let r = driver.event_elapsed_ms(timing_start, timing_end);
                if let Some(ms) = self.optional(ctx, op, r, mem)? {
                    self.with_telemetry(|t| {
                        t.gpu_event_ms += f64::from(ms);
                        t.transform_kernel_ms += f64::from(ms);
                    });
                }
            }
            Ok(())
        })();
        let wall = started.elapsed().as_nanos() as u64;

        // Retire scratch. On success or after a successful drain it is
        // unreferenced and freed; after an uncertain failure it stays
        // quarantined. A failed free makes the context untrusted: whatever
        // the launch outcome, the call reports `SessionPoisoned` and
        // publishes nothing (a completed `dst` joins the quarantine).
        if let Some(m) = scratch {
            if ctx.poisoned {
                self.poison(ctx, Some((m, scratch_bytes)));
            } else if let Err(e) = driver.device_free(m) {
                self.poison(ctx, Some((m, scratch_bytes)));
                if outcome.is_ok() {
                    self.poison(ctx, Some((dst, out_bytes)));
                }
                self.release_internal_reservation(total, 0);
                return Err(err(op, Self::kind_for(&e), FailureEffect::SessionPoisoned));
            }
        }
        if let Err(abort) = outcome {
            self.release_internal_reservation(total, 0);
            return Err(abort.error);
        }

        ctx.memory.insert(id, dst);
        let live = {
            let mut reg = self.lock_registry(op)?;
            reg.reserved = reg.reserved.saturating_sub(total);
            reg.live += out_bytes;
            reg.allocations.insert(
                id,
                AllocRecord {
                    generation,
                    element: TypeId::of::<F>(),
                    elem_size: TRANSFORM_ELEM_BYTES,
                    form: Form::ScalarCanonicalRepr,
                    len: out_len,
                    bytes: out_bytes,
                    quarantined: false,
                    checkpoint: None,
                },
            );
            reg.live
        };
        self.with_telemetry(|t| {
            t.launches += 1;
            t.transform_launches += 1;
            t.transform_wall_ns += wall;
            t.live_high_water = t.live_high_water.max(live);
            t.scratch_high_water = t.scratch_high_water.max(scratch_bytes);
        });
        Ok(ResidentBuffer::from_untrusted_metadata(
            generation,
            id,
            out_len,
            out_bytes,
        ))
    }

    // -----------------------------------------------------------------------
    // Release and recovery
    // -----------------------------------------------------------------------

    /// Release a resident handle without naming a group type (P4 chains).
    pub(crate) fn release_handle<T: 'static>(&self, handle: ResidentBuffer<T>) -> Result<(), BackendError> {
        self.release_impl(handle)
    }

    fn release_impl<T: 'static>(&self, handle: ResidentBuffer<T>) -> Result<(), BackendError> {
        let op = Operation::Release;
        let driver = self.driver.as_deref().ok_or(BackendError::unsupported(op))?;
        let mut device = self.lock_device(op)?;
        let ctx = device
            .current
            .as_mut()
            .ok_or(BackendError::unsupported(op))?;
        {
            let mut reg = self.lock_registry(op)?;
            reg.validate(op, &handle)?;
            let record = reg
                .allocations
                .remove(&handle.allocation())
                .expect("validated record exists");
            reg.live = reg.live.saturating_sub(record.bytes);
            if let Some(cp) = &record.checkpoint {
                reg.checkpoint_bytes = reg.checkpoint_bytes.saturating_sub(cp.len() as u64);
            }
        }
        // v1 work retires before each call returns, so no queued work can
        // reference this allocation; destroy it outside the registry lock.
        let memory = ctx
            .memory
            .remove(&handle.allocation())
            .ok_or(err(op, ErrorKind::Internal, FailureEffect::SessionPoisoned))?;
        if driver.device_free(memory).is_err() {
            self.poison(ctx, Some((memory, handle.bytes())));
            return Err(err(op, ErrorKind::Internal, FailureEffect::SessionPoisoned));
        }
        Ok(())
    }

    /// Consume a lost handle and return its backend-retained host checkpoint.
    /// Fails with `ContextLost` when no checkpoint exists (resident-only
    /// data), and with `InvalidInput` for live handles or forged metadata.
    /// The returned values are host data captured at upload, not a device
    /// result; device memory of the lost generation stays quarantined.
    pub fn recover_checkpoint<F: PrimeField>(
        &self,
        handle: ResidentBuffer<F>,
    ) -> Result<Vec<F>, BackendError> {
        let op = Operation::Download;
        match self.loss_class(&handle) {
            LossClass::Unknown | LossClass::Live => {
                return Err(err(op, ErrorKind::InvalidInput, FailureEffect::NotStarted))
            }
            LossClass::Unrecoverable => {
                return Err(err(op, ErrorKind::ContextLost, FailureEffect::NotStarted))
            }
            LossClass::RecoverableFromHostCheckpoint => {}
        }
        let mut reg = self.lock_registry(op)?;
        let record = reg
            .allocations
            .get_mut(&handle.allocation())
            .ok_or(err(op, ErrorKind::InvalidInput, FailureEffect::NotStarted))?;
        let elem_size = record.elem_size;
        let cp = record
            .checkpoint
            .take()
            .ok_or(err(op, ErrorKind::ContextLost, FailureEffect::NotStarted))?;
        reg.checkpoint_bytes = reg.checkpoint_bytes.saturating_sub(cp.len() as u64);
        drop(reg);
        decode::<F>(&cp, elem_size).ok_or(err(op, ErrorKind::Internal, FailureEffect::NotStarted))
    }

    // -----------------------------------------------------------------------
    // Reservations
    // -----------------------------------------------------------------------

    fn try_reserve_impl(&self, bytes: u64) -> Result<ReservationResult, BackendError> {
        let op = Operation::Reserve;
        if self.driver.is_none() {
            return Err(BackendError::unsupported(op));
        }
        let mut reg = self.lock_registry(op)?;
        let generation = reg.require_healthy(op)?;
        let available = reg.available();
        if bytes > available {
            return Ok(ReservationResult::Rejected {
                requested_bytes: bytes,
                available_bytes: available,
            });
        }
        let id = ReservationId(self.fresh_id(op)?);
        reg.reserved += bytes;
        reg.reservations
            .insert(id, ReservationRecord { generation, bytes });
        Ok(ReservationResult::Reserved(
            ReservationToken::from_untrusted_metadata(id, generation, bytes),
        ))
    }

    fn release_reservation_impl(&self, token: ReservationToken) -> Result<(), BackendError> {
        let op = Operation::Release;
        if self.driver.is_none() {
            return Err(BackendError::unsupported(op));
        }
        let mut reg = self.lock_registry(op)?;
        let invalid = err(op, ErrorKind::InvalidInput, FailureEffect::NotStarted);
        let current = reg.generation.ok_or(BackendError::unsupported(op))?;
        let record = reg.reservations.get(&token.id()).ok_or(invalid)?;
        if record.generation != token.generation() || record.bytes != token.bytes() {
            return Err(invalid);
        }
        if record.generation != current {
            return Err(err(op, ErrorKind::ContextLost, FailureEffect::NotStarted));
        }
        if !reg.healthy {
            return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
        }
        let record = reg.reservations.remove(&token.id()).expect("checked");
        reg.reserved = reg.reserved.saturating_sub(record.bytes);
        Ok(())
    }

    fn api_snapshot(&self) -> Result<ApiRuntimeSnapshot, BackendError> {
        let op = Operation::Runtime;
        let reg = self.lock_registry(op)?;
        let generation = reg.generation.ok_or(BackendError::unsupported(op))?;
        Ok(ApiRuntimeSnapshot {
            generation,
            context_healthy: reg.healthy,
            budget_bytes: reg.budget,
            live_bytes: reg.live,
            reserved_bytes: reg.reserved,
        })
    }

    fn generation_impl(&self) -> Result<DeviceGeneration, BackendError> {
        let op = Operation::Runtime;
        let reg = self.lock_registry(op)?;
        reg.generation.ok_or(BackendError::unsupported(op))
    }
}

fn decode<F: PrimeField>(bytes: &[u8], elem_size: usize) -> Option<Vec<F>> {
    if elem_size == 0 || bytes.len() % elem_size != 0 {
        return None;
    }
    bytes
        .chunks_exact(elem_size)
        .map(|chunk| {
            let mut repr = F::Repr::default();
            if repr.as_ref().len() != elem_size {
                return None;
            }
            repr.as_mut().copy_from_slice(chunk);
            Option::from(F::from_repr(repr))
        })
        .collect()
}

impl Drop for GpuRuntime {
    fn drop(&mut self) {
        let owns_native = self.owns_native;
        let Some(driver) = self.driver.clone() else {
            return;
        };
        let state = self.device.get_mut();
        // HostBuffer/DeviceMemory have no Drop: discarding the records of a
        // retired (reset) context frees nothing through the driver.
        state.retired.clear();
        let Some(ctx) = state.current.take() else {
            if state.setup_failed.is_some() {
                // A failed setup left the process context uncertain: keep the
                // native claim, exactly as for a poisoned context.
                return;
            }
            // Never initialized (or nothing live): nothing native to protect.
            #[cfg(feature = "cuda")]
            if owns_native {
                NATIVE_CLAIMED.store(false, Ordering::SeqCst);
            }
            let _ = owns_native;
            return;
        };
        if ctx.poisoned {
            // Never destruct through a poisoned context; leak until process
            // teardown so no queued work can reach freed memory. The native
            // claim is kept: the process context is not known to be clean.
            drop(ctx);
            return;
        }
        // Healthy teardown. Every result is checked: the first failure means
        // completion or context health is no longer certain, so destruction
        // stops immediately and everything not yet destroyed is leaked.
        if Self::teardown(driver.as_ref(), ctx) {
            #[cfg(feature = "cuda")]
            if owns_native {
                NATIVE_CLAIMED.store(false, Ordering::SeqCst);
            }
        }
    }
}

impl GpuRuntime {
    /// Destroy a healthy context; returns `false` (and leaks the remainder)
    /// at the first failed synchronization or destroy.
    fn teardown(driver: &dyn DeviceDriver, ctx: Context) -> bool {
        if driver.stream_synchronize(ctx.copy_stream).is_err() {
            return false;
        }
        for s in ctx.compute_streams {
            if driver.stream_synchronize(s).is_err() {
                return false;
            }
        }
        for (_, memory) in ctx.memory {
            if driver.device_free(memory).is_err() {
                return false;
            }
        }
        for slot in ctx.slots {
            if driver.destroy_event(slot.event).is_err() || driver.host_free(slot.buffer).is_err() {
                return false;
            }
        }
        for event in [ctx.copy_done, ctx.compute_done, ctx.timing_start, ctx.timing_end] {
            if driver.destroy_event(event).is_err() {
                return false;
            }
        }
        if driver.destroy_stream(ctx.copy_stream).is_err() {
            return false;
        }
        for s in ctx.compute_streams {
            if driver.destroy_stream(s).is_err() {
                return false;
            }
        }
        true
    }
}

// ===========================================================================
// API trait implementations
// ===========================================================================

impl<F, G> ResidentBackend<F, G> for GpuRuntime
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn generation(&self) -> Result<DeviceGeneration, BackendError> {
        self.generation_impl()
    }

    fn runtime_snapshot(&self) -> Result<ApiRuntimeSnapshot, BackendError> {
        self.api_snapshot()
    }

    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError> {
        self.try_reserve_impl(bytes)
    }

    fn release_reservation(&self, reservation: ReservationToken) -> Result<(), BackendError> {
        self.release_reservation_impl(reservation)
    }

    fn upload_scalars(&self, values: &[F]) -> Result<ResidentBuffer<F>, BackendError> {
        self.upload_impl(values, &CancelToken::new(), false)
    }

    /// Generic `Curve` types carry no wire encoding, so points cannot be
    /// staged without a layout cast. SRS points arrive through the P3
    /// registry's concrete adapter instead.
    fn upload_points(&self, _values: &[G]) -> Result<ResidentBuffer<G>, BackendError> {
        Err(BackendError::unsupported(Operation::Upload))
    }

    fn download_scalars(&self, source: &ResidentBuffer<F>) -> Result<Vec<F>, BackendError> {
        self.download_scalars_cancellable(source, &CancelToken::new())
    }

    fn transform_resident(
        &self,
        source: &ResidentBuffer<F>,
        spec: TransformSpec<F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        crate::ntt::transform_resident(self, source, spec)
    }

    fn release_scalars(&self, buffer: ResidentBuffer<F>) -> Result<(), BackendError> {
        self.release_impl(buffer)
    }

    fn release_points(&self, buffer: ResidentBuffer<G>) -> Result<(), BackendError> {
        self.release_impl(buffer)
    }
}

impl LokiGpu {
    /// Initialize the process runtime (native CUDA driver with the `cuda`
    /// feature; `Unsupported` otherwise). Capability queries never initialize
    /// the device implicitly.
    pub fn initialize(&self) -> Result<DeviceGeneration, BackendError> {
        GpuRuntime::process().initialize()
    }
}

impl<F, G> ResidentBackend<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn generation(&self) -> Result<DeviceGeneration, BackendError> {
        ResidentBackend::<F, G>::generation(GpuRuntime::process())
    }

    fn runtime_snapshot(&self) -> Result<ApiRuntimeSnapshot, BackendError> {
        ResidentBackend::<F, G>::runtime_snapshot(GpuRuntime::process())
    }

    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError> {
        ResidentBackend::<F, G>::try_reserve(GpuRuntime::process(), bytes)
    }

    fn release_reservation(&self, reservation: ReservationToken) -> Result<(), BackendError> {
        ResidentBackend::<F, G>::release_reservation(GpuRuntime::process(), reservation)
    }

    fn upload_scalars(&self, values: &[F]) -> Result<ResidentBuffer<F>, BackendError> {
        ResidentBackend::<F, G>::upload_scalars(GpuRuntime::process(), values)
    }

    fn upload_points(&self, values: &[G]) -> Result<ResidentBuffer<G>, BackendError> {
        ResidentBackend::<F, G>::upload_points(GpuRuntime::process(), values)
    }

    fn download_scalars(&self, source: &ResidentBuffer<F>) -> Result<Vec<F>, BackendError> {
        ResidentBackend::<F, G>::download_scalars(GpuRuntime::process(), source)
    }

    fn transform_resident(
        &self,
        source: &ResidentBuffer<F>,
        spec: TransformSpec<F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        crate::ntt::transform_resident(GpuRuntime::process(), source, spec)
    }

    fn release_scalars(&self, buffer: ResidentBuffer<F>) -> Result<(), BackendError> {
        ResidentBackend::<F, G>::release_scalars(GpuRuntime::process(), buffer)
    }

    fn release_points(&self, buffer: ResidentBuffer<G>) -> Result<(), BackendError> {
        ResidentBackend::<F, G>::release_points(GpuRuntime::process(), buffer)
    }
}

// ===========================================================================
// Fault-injecting fake driver
// ===========================================================================

pub mod fake {
    //! In-memory fake device with deferred execution and fault injection.
    //!
    //! Enqueued copies and launches run only when an event or stream is
    //! synchronized, and they read/write host staging buffers at that time.
    //! The fake records a [`Violation`] instead of performing undefined
    //! behaviour when the runtime frees, rewrites or reuses a resource that
    //! queued work still references, destructs through a poisoned context,
    //! or uses a handle from an older context epoch.

    use super::*;
    pub use super::DriverCall;
    use std::collections::VecDeque;
    use std::sync::mpsc::{Receiver, Sender};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct FakeConfig {
        pub compute_capability: (u32, u32),
        pub total_bytes: u64,
        pub free_bytes: u64,
        /// Simulated process-wide pinned-host limit (WSL2 caps near 500 MB).
        pub pinned_limit_bytes: Option<u64>,
    }

    impl Default for FakeConfig {
        fn default() -> Self {
            Self {
                compute_capability: (12, 0),
                total_bytes: 32 << 30,
                free_bytes: 30 << 30,
                pinned_limit_bytes: None,
            }
        }
    }

    /// Injected behaviour for the n-th call (counted from injection).
    pub enum Fault {
        /// Return an error from the call itself. Poisoned/ContextLost become
        /// sticky: every later call fails until `reset`.
        Fail(DriverErrorClass),
        /// The call succeeds, but the enqueued work fails when synchronized.
        FailAtSync(DriverErrorClass),
        /// Cancel the token, then proceed normally.
        Cancel(CancelToken),
        /// Signal `reached`, then block until `resume` receives.
        Pause {
            reached: Sender<()>,
            resume: Arc<Mutex<Receiver<()>>>,
        },
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum Violation {
        StagingAccessWhilePending { buffer: u64 },
        DeviceFreeWhilePending { memory: u64 },
        HostFreeWhilePending { buffer: u64 },
        DoubleFree { handle: u64 },
        UseAfterFree { handle: u64 },
        StaleEpochUse { handle: u64, epoch: u64 },
        DestroyThroughPoisonedContext { call: DriverCall },
        OutOfBounds { handle: u64 },
        UnknownHandle { handle: u64 },
    }

    #[derive(Debug)]
    enum Op {
        H2D { dst: u64, dst_off: u64, src: u64, src_off: usize, len: usize },
        D2H { dst: u64, dst_off: usize, src: u64, src_off: u64, len: usize },
        Copy { dst: u64, src: u64, len: u64 },
        /// P4 transform stand-in: deterministic, non-arithmetic. `dst`
        /// receives `src` bytes (zero-padded or truncated); scratch is
        /// overwritten. Used only to test transactional behaviour.
        Transform { dst: u64, src: u64, scratch: Option<u64> },
        Record(u64),
        Wait(u64),
        AsyncFail(DriverErrorClass),
    }

    impl Op {
        fn references(&self, handle: u64) -> bool {
            match *self {
                Op::H2D { dst, src, .. } | Op::D2H { dst, src, .. } | Op::Copy { dst, src, .. } => {
                    dst == handle || src == handle
                }
                Op::Transform { dst, src, scratch } => {
                    dst == handle || src == handle || scratch == Some(handle)
                }
                _ => false,
            }
        }
    }

    struct Alloc {
        data: Vec<u8>,
        epoch: u64,
        freed: bool,
    }
    struct Host {
        ptr: *mut u8,
        len: usize,
        pinned: bool,
        epoch: u64,
        freed: bool,
    }
    struct Stream {
        epoch: u64,
        destroyed: bool,
        queue: VecDeque<Op>,
    }
    struct Event {
        epoch: u64,
        destroyed: bool,
        /// Stream on which the latest record is still queued.
        pending_on: Option<u64>,
    }

    struct State {
        epoch: u64,
        sticky: Option<DriverErrorClass>,
        counts: HashMap<DriverCall, usize>,
        faults: Vec<(DriverCall, usize, Option<Fault>)>,
        next_handle: u64,
        allocs: HashMap<u64, Alloc>,
        hosts: HashMap<u64, Host>,
        streams: HashMap<u64, Stream>,
        events: HashMap<u64, Event>,
        pinned_in_use: u64,
        violations: Vec<Violation>,
        double_frees: usize,
        all_non_blocking: bool,
        corrupt: Option<u8>,
        transform_enabled: bool,
        transform_launches: usize,
    }

    // SAFETY: host pointers are owned by the fake (from Box::into_raw) and
    // only dereferenced while the state mutex is held.
    unsafe impl Send for State {}

    pub struct FakeDriver {
        config: FakeConfig,
        state: Mutex<State>,
    }

    enum Action {
        Proceed,
        AsyncFail(DriverErrorClass),
        Pause(Sender<()>, Arc<Mutex<Receiver<()>>>),
    }

    fn derr(call: DriverCall, class: DriverErrorClass) -> DriverError {
        DriverError {
            call,
            class,
            code: 0,
        }
    }

    impl FakeDriver {
        pub fn new(config: FakeConfig) -> Self {
            Self {
                config,
                state: Mutex::new(State {
                    epoch: 1,
                    sticky: None,
                    counts: HashMap::new(),
                    faults: Vec::new(),
                    next_handle: 0x1000,
                    allocs: HashMap::new(),
                    hosts: HashMap::new(),
                    streams: HashMap::new(),
                    events: HashMap::new(),
                    pinned_in_use: 0,
                    violations: Vec::new(),
                    double_frees: 0,
                    all_non_blocking: true,
                    corrupt: None,
                    transform_enabled: false,
                    transform_launches: 0,
                }),
            }
        }

        fn st(&self) -> MutexGuard<'_, State> {
            self.state.lock().unwrap_or_else(|p| p.into_inner())
        }

        /// Inject `fault` at the `nth` (1-based) future call of `call`.
        pub fn inject(&self, call: DriverCall, nth: usize, fault: Fault) {
            let mut st = self.st();
            let base = st.counts.get(&call).copied().unwrap_or(0);
            st.faults.push((call, base + nth, Some(fault)));
        }

        pub fn calls(&self, call: DriverCall) -> usize {
            self.st().counts.get(&call).copied().unwrap_or(0)
        }

        pub fn violations(&self) -> Vec<Violation> {
            self.st().violations.clone()
        }

        pub fn double_frees(&self) -> usize {
            self.st().double_frees
        }

        pub fn live_device_allocations(&self) -> usize {
            let st = self.st();
            st.allocs
                .values()
                .filter(|a| !a.freed && a.epoch == st.epoch)
                .count()
        }

        pub fn live_host_buffers(&self) -> usize {
            let st = self.st();
            st.hosts
                .values()
                .filter(|h| !h.freed && h.epoch == st.epoch)
                .count()
        }

        pub fn live_streams(&self) -> usize {
            let st = self.st();
            st.streams
                .values()
                .filter(|s| !s.destroyed && s.epoch == st.epoch)
                .count()
        }

        pub fn live_events(&self) -> usize {
            let st = self.st();
            st.events
                .values()
                .filter(|e| !e.destroyed && e.epoch == st.epoch)
                .count()
        }

        pub fn streams_created_non_blocking(&self) -> bool {
            self.st().all_non_blocking
        }

        /// Overwrite every live device allocation with `byte`.
        /// Make `supports_transform` true (P4 transactional tests). The fake
        /// transform is not arithmetic; see `Op::Transform`.
        pub fn enable_transform(&self) {
            self.st().transform_enabled = true;
        }

        /// Fake transforms executed (retired), not merely enqueued.
        pub fn transform_launches(&self) -> usize {
            self.st().transform_launches
        }

        pub fn corrupt_device_memory(&self, byte: u8) {
            let mut st = self.st();
            st.corrupt = Some(byte);
            let epoch = st.epoch;
            for a in st.allocs.values_mut() {
                if !a.freed && a.epoch == epoch {
                    a.data.iter_mut().for_each(|b| *b = byte);
                }
            }
        }

        /// Count the call, apply sticky state and injected faults.
        fn enter(&self, st: &mut State, call: DriverCall) -> Result<Action, DriverError> {
            let n = {
                let c = st.counts.entry(call).or_insert(0);
                *c += 1;
                *c
            };
            let destroy = matches!(
                call,
                DriverCall::DeviceFree
                    | DriverCall::HostFree
                    | DriverCall::StreamDestroy
                    | DriverCall::EventDestroy
            );
            if let Some(class) = st.sticky {
                if call != DriverCall::Reset {
                    if destroy {
                        st.violations
                            .push(Violation::DestroyThroughPoisonedContext { call });
                    }
                    return Err(derr(call, class));
                }
            }
            let fault = st
                .faults
                .iter_mut()
                .find(|(c, at, f)| *c == call && *at == n && f.is_some())
                .and_then(|(_, _, f)| f.take());
            let mut action = Action::Proceed;
            match fault {
                Some(Fault::Fail(class)) => {
                    if matches!(class, DriverErrorClass::Poisoned | DriverErrorClass::ContextLost) {
                        st.sticky = Some(class);
                    }
                    return Err(derr(call, class));
                }
                Some(Fault::FailAtSync(class)) => action = Action::AsyncFail(class),
                Some(Fault::Cancel(token)) => token.cancel(),
                Some(Fault::Pause { reached, resume }) => action = Action::Pause(reached, resume),
                None => {}
            }
            Ok(action)
        }

        fn pause(action: &Action) {
            if let Action::Pause(reached, resume) = action {
                let _ = reached.send(());
                if let Ok(rx) = resume.lock() {
                    let _ = rx.recv();
                }
            }
        }

        fn fresh(st: &mut State) -> u64 {
            st.next_handle += 0x100;
            st.next_handle
        }

        fn check_epoch(st: &mut State, h: &DriverHandle) -> bool {
            if h.epoch != st.epoch {
                st.violations.push(Violation::StaleEpochUse {
                    handle: h.raw,
                    epoch: h.epoch,
                });
                false
            } else {
                true
            }
        }

        fn pending_refs(st: &State, handle: u64) -> bool {
            st.streams
                .values()
                .any(|s| s.queue.iter().any(|op| op.references(handle)))
        }

        fn enqueue(st: &mut State, stream: &StreamId, op: Op, call: DriverCall) -> Result<(), DriverError> {
            if !Self::check_epoch(st, &stream.0) {
                return Err(derr(call, DriverErrorClass::Transient));
            }
            match st.streams.get_mut(&stream.0.raw) {
                Some(s) if !s.destroyed => {
                    s.queue.push_back(op);
                    Ok(())
                }
                _ => {
                    st.violations.push(Violation::UnknownHandle {
                        handle: stream.0.raw,
                    });
                    Err(derr(call, DriverErrorClass::Transient))
                }
            }
        }

        /// Run `stream` until `until` (an event record) is popped, or fully.
        fn run(st: &mut State, stream: u64, until: Option<u64>, call: DriverCall) -> Result<(), DriverError> {
            loop {
                let op = match st.streams.get_mut(&stream) {
                    Some(s) => s.queue.pop_front(),
                    None => return Ok(()),
                };
                let Some(op) = op else { return Ok(()) };
                match op {
                    Op::Record(e) => {
                        if let Some(ev) = st.events.get_mut(&e) {
                            if ev.pending_on == Some(stream) {
                                ev.pending_on = None;
                            }
                        }
                        if until == Some(e) {
                            return Ok(());
                        }
                    }
                    Op::Wait(e) => {
                        let other = st.events.get(&e).and_then(|ev| ev.pending_on);
                        if let Some(o) = other {
                            if o != stream {
                                Self::run(st, o, Some(e), call)?;
                            }
                        }
                    }
                    Op::AsyncFail(class) => {
                        if matches!(class, DriverErrorClass::Poisoned | DriverErrorClass::ContextLost) {
                            st.sticky = Some(class);
                        }
                        return Err(derr(call, class));
                    }
                    Op::H2D { dst, dst_off, src, src_off, len } => {
                        let bytes = match st.hosts.get(&src) {
                            Some(h) if !h.freed && src_off + len <= h.len => {
                                // SAFETY: fake-owned live allocation of h.len bytes.
                                unsafe { std::slice::from_raw_parts(h.ptr.add(src_off), len) }.to_vec()
                            }
                            _ => {
                                st.violations.push(Violation::UseAfterFree { handle: src });
                                continue;
                            }
                        };
                        match st.allocs.get_mut(&dst) {
                            Some(a) if !a.freed && dst_off as usize + len <= a.data.len() => {
                                a.data[dst_off as usize..dst_off as usize + len].copy_from_slice(&bytes);
                            }
                            _ => st.violations.push(Violation::UseAfterFree { handle: dst }),
                        }
                    }
                    Op::D2H { dst, dst_off, src, src_off, len } => {
                        let bytes = match st.allocs.get(&src) {
                            Some(a) if !a.freed && src_off as usize + len <= a.data.len() => {
                                a.data[src_off as usize..src_off as usize + len].to_vec()
                            }
                            _ => {
                                st.violations.push(Violation::UseAfterFree { handle: src });
                                continue;
                            }
                        };
                        match st.hosts.get(&dst) {
                            Some(h) if !h.freed && dst_off + len <= h.len => {
                                // SAFETY: fake-owned live allocation of h.len bytes.
                                unsafe {
                                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), h.ptr.add(dst_off), len)
                                };
                            }
                            _ => st.violations.push(Violation::UseAfterFree { handle: dst }),
                        }
                    }
                    Op::Transform { dst, src, scratch } => {
                        let bytes = match st.allocs.get(&src) {
                            Some(a) if !a.freed => a.data.clone(),
                            _ => {
                                st.violations.push(Violation::UseAfterFree { handle: src });
                                continue;
                            }
                        };
                        if let Some(sc) = scratch {
                            match st.allocs.get_mut(&sc) {
                                Some(a) if !a.freed => a.data.iter_mut().for_each(|b| *b = 0xa5),
                                _ => st.violations.push(Violation::UseAfterFree { handle: sc }),
                            }
                        }
                        match st.allocs.get_mut(&dst) {
                            Some(a) if !a.freed => {
                                let n = a.data.len().min(bytes.len());
                                a.data[..n].copy_from_slice(&bytes[..n]);
                                a.data[n..].iter_mut().for_each(|b| *b = 0);
                            }
                            _ => st.violations.push(Violation::UseAfterFree { handle: dst }),
                        }
                        st.transform_launches += 1;
                    }
                    Op::Copy { dst, src, len } => {
                        let bytes = match st.allocs.get(&src) {
                            Some(a) if !a.freed && len as usize <= a.data.len() => a.data[..len as usize].to_vec(),
                            _ => {
                                st.violations.push(Violation::UseAfterFree { handle: src });
                                continue;
                            }
                        };
                        match st.allocs.get_mut(&dst) {
                            Some(a) if !a.freed && len as usize <= a.data.len() => {
                                a.data[..len as usize].copy_from_slice(&bytes)
                            }
                            _ => st.violations.push(Violation::UseAfterFree { handle: dst }),
                        }
                    }
                }
            }
        }

        fn handle(st: &mut State, kind: HandleKind, bytes: u64) -> DriverHandle {
            DriverHandle {
                kind,
                epoch: st.epoch,
                raw: Self::fresh(st),
                bytes,
            }
        }
    }

    /// Safe control surface for a fake driver owned by exactly one runtime:
    /// fault injection and inspection only. It cannot call any
    /// `DeviceDriver` method, so it cannot reset, free or launch behind the
    /// runtime's back.
    #[derive(Clone)]
    pub struct FakeControl(Arc<FakeDriver>);

    impl FakeControl {
        pub fn inject(&self, call: DriverCall, nth: usize, fault: Fault) {
            self.0.inject(call, nth, fault)
        }
        pub fn calls(&self, call: DriverCall) -> usize {
            self.0.calls(call)
        }
        pub fn violations(&self) -> Vec<Violation> {
            self.0.violations()
        }
        pub fn double_frees(&self) -> usize {
            self.0.double_frees()
        }
        pub fn live_device_allocations(&self) -> usize {
            self.0.live_device_allocations()
        }
        pub fn live_host_buffers(&self) -> usize {
            self.0.live_host_buffers()
        }
        pub fn live_streams(&self) -> usize {
            self.0.live_streams()
        }
        pub fn live_events(&self) -> usize {
            self.0.live_events()
        }
        pub fn streams_created_non_blocking(&self) -> bool {
            self.0.streams_created_non_blocking()
        }
        pub fn enable_transform(&self) {
            self.0.enable_transform()
        }
        pub fn transform_launches(&self) -> usize {
            self.0.transform_launches()
        }
        pub fn corrupt_device_memory(&self, byte: u8) {
            self.0.corrupt_device_memory(byte)
        }
    }

    impl GpuRuntime {
        /// A runtime over a fresh fake driver that it exclusively owns, plus
        /// the restricted control handle for that driver.
        pub fn with_fake(fake: FakeConfig, config: RuntimeConfig) -> (FakeControl, GpuRuntime) {
            let driver = Arc::new(FakeDriver::new(fake));
            // SAFETY: the driver is new; the only other reference is the
            // `FakeControl`, which exposes no `DeviceDriver` method.
            let rt = unsafe { GpuRuntime::with_driver(driver.clone(), config) };
            (FakeControl(driver), rt)
        }
    }

    impl Drop for FakeDriver {
        fn drop(&mut self) {
            let st = self.state.get_mut().unwrap_or_else(|p| p.into_inner());
            for h in st.hosts.values() {
                // SAFETY: allocated by Box::into_raw with this length; freed
                // exactly once here (host_free only marks it freed).
                unsafe {
                    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(h.ptr, h.len)));
                }
            }
        }
    }

    // SAFETY: host buffers are boxed slices that stay valid until the fake is
    // dropped; copies are bounds-checked; synchronization executes every
    // queued operation up to the governing event before returning Ok.
    unsafe impl DeviceDriver for FakeDriver {
        fn probe(&self, ordinal: u32) -> Result<DeviceInfo, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::Probe)?;
            Ok(DeviceInfo {
                ordinal,
                compute_capability: self.config.compute_capability,
                total_bytes: self.config.total_bytes,
                free_bytes: self.config.free_bytes,
                context_epoch: st.epoch,
                name: "loki-gpu fake device".into(),
            })
        }

        fn reset(&self, ordinal: u32) -> Result<DeviceInfo, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::Reset)?;
            st.epoch += 1;
            st.sticky = None;
            st.pinned_in_use = 0;
            for s in st.streams.values_mut() {
                s.queue.clear();
            }
            Ok(DeviceInfo {
                ordinal,
                compute_capability: self.config.compute_capability,
                total_bytes: self.config.total_bytes,
                free_bytes: self.config.free_bytes,
                context_epoch: st.epoch,
                name: "loki-gpu fake device".into(),
            })
        }

        fn create_stream(&self, non_blocking: bool) -> Result<StreamId, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::StreamCreate)?;
            st.all_non_blocking &= non_blocking;
            let h = Self::handle(&mut st, HandleKind::Stream, 0);
            let epoch = st.epoch;
            st.streams.insert(
                h.raw,
                Stream {
                    epoch,
                    destroyed: false,
                    queue: VecDeque::new(),
                },
            );
            Ok(StreamId(h))
        }

        fn destroy_stream(&self, stream: StreamId) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::StreamDestroy)?;
            Self::check_epoch(&mut st, &stream.0);
            match st.streams.get_mut(&stream.0.raw) {
                Some(s) if !s.destroyed => {
                    s.destroyed = true;
                    Ok(())
                }
                _ => {
                    st.double_frees += 1;
                    st.violations.push(Violation::DoubleFree { handle: stream.0.raw });
                    Err(derr(DriverCall::StreamDestroy, DriverErrorClass::Transient))
                }
            }
        }

        fn create_event(&self, _timing: bool) -> Result<EventId, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::EventCreate)?;
            let h = Self::handle(&mut st, HandleKind::Event, 0);
            let epoch = st.epoch;
            st.events.insert(
                h.raw,
                Event {
                    epoch,
                    destroyed: false,
                    pending_on: None,
                },
            );
            Ok(EventId(h))
        }

        fn destroy_event(&self, event: EventId) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::EventDestroy)?;
            Self::check_epoch(&mut st, &event.0);
            match st.events.get_mut(&event.0.raw) {
                Some(e) if !e.destroyed => {
                    e.destroyed = true;
                    Ok(())
                }
                _ => {
                    st.double_frees += 1;
                    st.violations.push(Violation::DoubleFree { handle: event.0.raw });
                    Err(derr(DriverCall::EventDestroy, DriverErrorClass::Transient))
                }
            }
        }

        fn device_alloc(&self, bytes: u64) -> Result<DeviceMemory, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::DeviceAlloc)?;
            let h = Self::handle(&mut st, HandleKind::DeviceMemory, bytes);
            let epoch = st.epoch;
            st.allocs.insert(
                h.raw,
                Alloc {
                    data: vec![0xcd; bytes as usize],
                    epoch,
                    freed: false,
                },
            );
            Ok(DeviceMemory(h))
        }

        fn device_free(&self, memory: DeviceMemory) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::DeviceFree)?;
            Self::check_epoch(&mut st, &memory.0);
            let raw = memory.0.raw;
            if Self::pending_refs(&st, raw) {
                st.violations.push(Violation::DeviceFreeWhilePending { memory: raw });
                return Ok(()); // keep the memory so queued work stays defined
            }
            match st.allocs.get_mut(&raw) {
                Some(a) if !a.freed => {
                    a.freed = true;
                    a.data = Vec::new();
                    Ok(())
                }
                Some(_) => {
                    st.double_frees += 1;
                    st.violations.push(Violation::DoubleFree { handle: raw });
                    Err(derr(DriverCall::DeviceFree, DriverErrorClass::Transient))
                }
                None => {
                    st.violations.push(Violation::UnknownHandle { handle: raw });
                    Err(derr(DriverCall::DeviceFree, DriverErrorClass::Transient))
                }
            }
        }

        fn host_alloc(&self, bytes: usize, pinned: bool) -> Result<HostBuffer, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::HostAlloc)?;
            if pinned {
                if let Some(limit) = self.config.pinned_limit_bytes {
                    if st.pinned_in_use + bytes as u64 > limit {
                        return Err(derr(DriverCall::HostAlloc, DriverErrorClass::Transient));
                    }
                }
                st.pinned_in_use += bytes as u64;
            }
            let kind = if pinned {
                HandleKind::HostPinned
            } else {
                HandleKind::HostPageable
            };
            let h = Self::handle(&mut st, kind, bytes as u64);
            let ptr = Box::into_raw(vec![0xabu8; bytes].into_boxed_slice()) as *mut u8;
            let epoch = st.epoch;
            st.hosts.insert(
                h.raw,
                Host {
                    ptr,
                    len: bytes,
                    pinned,
                    epoch,
                    freed: false,
                },
            );
            Ok(HostBuffer {
                handle: h,
                ptr,
                len: bytes,
            })
        }

        fn host_free(&self, buffer: HostBuffer) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::HostFree)?;
            Self::check_epoch(&mut st, &buffer.handle);
            let raw = buffer.handle.raw;
            if Self::pending_refs(&st, raw) {
                st.violations.push(Violation::HostFreeWhilePending { buffer: raw });
                return Ok(());
            }
            let mut released = 0;
            let result = match st.hosts.get_mut(&raw) {
                Some(h) if !h.freed => {
                    h.freed = true;
                    if h.pinned {
                        released = h.len as u64;
                    }
                    Ok(())
                }
                _ => {
                    st.double_frees += 1;
                    st.violations.push(Violation::DoubleFree { handle: raw });
                    Err(derr(DriverCall::HostFree, DriverErrorClass::Transient))
                }
            };
            st.pinned_in_use = st.pinned_in_use.saturating_sub(released);
            result
        }

        fn copy_h2d(
            &self,
            dst: DeviceMemory,
            dst_offset: u64,
            src: &HostBuffer,
            src_offset: usize,
            len: usize,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            let call = DriverCall::CopyH2D;
            let action = {
                let mut st = self.st();
                let action = self.enter(&mut st, call)?;
                Self::check_epoch(&mut st, &dst.0);
                Self::check_epoch(&mut st, &src.handle);
                if dst_offset.checked_add(len as u64).map_or(true, |e| e > dst.0.bytes)
                    || src_offset.checked_add(len).map_or(true, |e| e > src.len)
                {
                    st.violations.push(Violation::OutOfBounds { handle: dst.0.raw });
                    return Err(derr(call, DriverErrorClass::Transient));
                }
                Self::enqueue(
                    &mut st,
                    &stream,
                    Op::H2D {
                        dst: dst.0.raw,
                        dst_off: dst_offset,
                        src: src.handle.raw,
                        src_off: src_offset,
                        len,
                    },
                    call,
                )?;
                if let Action::AsyncFail(class) = action {
                    Self::enqueue(&mut st, &stream, Op::AsyncFail(class), call)?;
                }
                action
            };
            Self::pause(&action);
            Ok(())
        }

        fn copy_d2h(
            &self,
            dst: &HostBuffer,
            dst_offset: usize,
            src: DeviceMemory,
            src_offset: u64,
            len: usize,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            let call = DriverCall::CopyD2H;
            let action = {
                let mut st = self.st();
                let action = self.enter(&mut st, call)?;
                Self::check_epoch(&mut st, &dst.handle);
                Self::check_epoch(&mut st, &src.0);
                if src_offset.checked_add(len as u64).map_or(true, |e| e > src.0.bytes)
                    || dst_offset.checked_add(len).map_or(true, |e| e > dst.len)
                {
                    st.violations.push(Violation::OutOfBounds { handle: src.0.raw });
                    return Err(derr(call, DriverErrorClass::Transient));
                }
                Self::enqueue(
                    &mut st,
                    &stream,
                    Op::D2H {
                        dst: dst.handle.raw,
                        dst_off: dst_offset,
                        src: src.0.raw,
                        src_off: src_offset,
                        len,
                    },
                    call,
                )?;
                if let Action::AsyncFail(class) = action {
                    Self::enqueue(&mut st, &stream, Op::AsyncFail(class), call)?;
                }
                action
            };
            Self::pause(&action);
            Ok(())
        }

        fn launch_copy(
            &self,
            dst: DeviceMemory,
            src: DeviceMemory,
            bytes: u64,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            let call = DriverCall::Launch;
            let action = {
                let mut st = self.st();
                let action = self.enter(&mut st, call)?;
                Self::check_epoch(&mut st, &dst.0);
                Self::check_epoch(&mut st, &src.0);
                if bytes > dst.0.bytes || bytes > src.0.bytes {
                    st.violations.push(Violation::OutOfBounds { handle: dst.0.raw });
                    return Err(derr(call, DriverErrorClass::Transient));
                }
                Self::enqueue(
                    &mut st,
                    &stream,
                    Op::Copy {
                        dst: dst.0.raw,
                        src: src.0.raw,
                        len: bytes,
                    },
                    call,
                )?;
                if let Action::AsyncFail(class) = action {
                    Self::enqueue(&mut st, &stream, Op::AsyncFail(class), call)?;
                }
                action
            };
            Self::pause(&action);
            Ok(())
        }

        fn supports_transform(&self) -> bool {
            self.st().transform_enabled
        }

        fn launch_transform(
            &self,
            dst: DeviceMemory,
            src: DeviceMemory,
            scratch: Option<DeviceMemory>,
            launch: &TransformLaunch,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            let call = DriverCall::Launch;
            let action = {
                let mut st = self.st();
                if !st.transform_enabled {
                    return Err(derr(call, DriverErrorClass::Unsupported));
                }
                let action = self.enter(&mut st, call)?;
                Self::check_epoch(&mut st, &dst.0);
                Self::check_epoch(&mut st, &src.0);
                let elem = TRANSFORM_ELEM_BYTES as u64;
                let scratch_ok = match (scratch, launch.scratch_bytes()) {
                    (Some(m), Some(b)) => {
                        Self::check_epoch(&mut st, &m.0);
                        m.0.bytes >= b && b > 0
                    }
                    (None, Some(0)) => true,
                    _ => false,
                };
                if launch.output_len.checked_mul(elem) != Some(dst.0.bytes)
                    || launch.source_len.checked_mul(elem) != Some(src.0.bytes)
                    || !scratch_ok
                {
                    st.violations.push(Violation::OutOfBounds { handle: dst.0.raw });
                    return Err(derr(call, DriverErrorClass::Transient));
                }
                Self::enqueue(
                    &mut st,
                    &stream,
                    Op::Transform {
                        dst: dst.0.raw,
                        src: src.0.raw,
                        scratch: scratch.map(|m| m.0.raw),
                    },
                    call,
                )?;
                if let Action::AsyncFail(class) = action {
                    Self::enqueue(&mut st, &stream, Op::AsyncFail(class), call)?;
                }
                action
            };
            Self::pause(&action);
            Ok(())
        }

        fn record_event(&self, event: EventId, stream: StreamId) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::EventRecord)?;
            Self::check_epoch(&mut st, &event.0);
            Self::enqueue(&mut st, &stream, Op::Record(event.0.raw), DriverCall::EventRecord)?;
            if let Some(e) = st.events.get_mut(&event.0.raw) {
                e.pending_on = Some(stream.0.raw);
            }
            Ok(())
        }

        fn stream_wait_event(&self, stream: StreamId, event: EventId) -> Result<(), DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::StreamWait)?;
            Self::check_epoch(&mut st, &event.0);
            Self::enqueue(&mut st, &stream, Op::Wait(event.0.raw), DriverCall::StreamWait)
        }

        fn event_synchronize(&self, event: EventId) -> Result<(), DriverError> {
            let call = DriverCall::EventSync;
            let mut st = self.st();
            self.enter(&mut st, call)?;
            Self::check_epoch(&mut st, &event.0);
            let pending = st.events.get(&event.0.raw).and_then(|e| e.pending_on);
            match pending {
                Some(stream) => Self::run(&mut st, stream, Some(event.0.raw), call),
                None => Ok(()),
            }
        }

        fn event_query(&self, event: EventId) -> Result<bool, DriverError> {
            let call = DriverCall::EventQuery;
            let mut st = self.st();
            self.enter(&mut st, call)?;
            let pending = st.events.get(&event.0.raw).and_then(|e| e.pending_on);
            if let Some(stream) = pending {
                Self::run(&mut st, stream, Some(event.0.raw), call)?;
            }
            Ok(true)
        }

        fn event_elapsed_ms(&self, _start: EventId, _end: EventId) -> Result<f32, DriverError> {
            let mut st = self.st();
            self.enter(&mut st, DriverCall::EventElapsed)?;
            Ok(0.001)
        }

        fn stream_synchronize(&self, stream: StreamId) -> Result<(), DriverError> {
            let call = DriverCall::StreamSync;
            let mut st = self.st();
            self.enter(&mut st, call)?;
            Self::check_epoch(&mut st, &stream.0);
            Self::run(&mut st, stream.0.raw, None, call)
        }

        fn before_host_access(&self, buffer: &HostBuffer) {
            let mut st = self.st();
            if Self::pending_refs(&st, buffer.handle.raw) {
                st.violations.push(Violation::StagingAccessWhilePending {
                    buffer: buffer.handle.raw,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_order_is_enforced() {
        let rt = GpuRuntime::unavailable();
        // Correct order: device -> registry -> telemetry.
        {
            let _d = rt.device.lock().unwrap();
            let _r = rt.registry.lock().unwrap();
            let _t = rt.telemetry.lock().unwrap();
        }
        // Inverted order panics instead of risking deadlock.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _r = rt.registry.lock().unwrap();
            let _d = rt.device.lock().unwrap();
        }));
        assert!(result.is_err());
        // Levels were released by unwinding; normal use still works.
        let _d = rt.device.lock().unwrap();
    }

    #[test]
    fn reentrant_lock_panics() {
        let rt = GpuRuntime::unavailable();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _a = rt.registry.lock().unwrap();
            let _b = rt.registry.lock().unwrap();
        }));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod identity_tests {
    use super::checked_next;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn identity_counter_fails_instead_of_wrapping() {
        let c = AtomicU64::new(u64::MAX - 1);
        assert_eq!(checked_next(&c), Some(u64::MAX - 1));
        assert_eq!(checked_next(&c), None);
        assert_eq!(checked_next(&c), None);
    }
}
