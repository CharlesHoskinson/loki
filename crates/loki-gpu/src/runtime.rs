//! Device-runtime ownership and memory-accounting surface.
//!
//! CUDA support is intentionally not implemented in this skeleton. These
//! types make the generation, health, and memory information explicit without
//! exposing a raw CUDA pointer or claiming that a device is available.

use std::sync::{Mutex, OnceLock};

use loki_backend_api::{
    BackendCapabilities, BackendError, Curve, DeviceGeneration, ErrorKind, FailureEffect,
    Operation, PrimeField, ReservationResult, ReservationToken, ResidentBackend, ResidentBuffer,
    RuntimeSnapshot as ApiRuntimeSnapshot, TransformSpec,
};

use crate::LokiGpu;

/// WSL2-friendly pinned transfer slot size. A failed pinned allocation must
/// use pageable staging or return an error; it must never grow this ring
/// without updating the separate bounded host-memory budget.
pub const PINNED_SLOT_BYTES: usize = 32 * 1024 * 1024;

/// Start with two slots; increase only after measuring overlap and reviewing
/// the host-memory budget.
pub const INITIAL_PINNED_SLOTS: usize = 2;

/// Keep the pinned ring small on WSL2/WDDM. `cudaHostAlloc` is checked for
/// every slot; pageable copies remain a supported fallback.
pub const MAX_PINNED_SLOTS: usize = 4;

/// Maximum host memory reserved by the pinned transfer ring. This is separate
/// from the CUDA device-allocation budget.
pub const PINNED_HOST_BUDGET_BYTES: usize = MAX_PINNED_SLOTS * PINNED_SLOT_BYTES;

/// Conservative starting limit for persistent SRS residency. Runtime scratch
/// is accounted separately against the shared device budget.
pub const DEFAULT_SRS_BUDGET_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Read-only status used by the scheduler and diagnostics. Unknown memory
/// values remain `None`; a CUDA-free build must not invent a free-memory
/// estimate.
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

/// Shared process runtime state. All GPU modules must use this coordinator so
/// that MSM, NTT, quotient work, allocations, and destruction share one device
/// generation and lock order. The initial implementation always reports the
/// backend as unsupported.
#[derive(Debug)]
pub struct GpuRuntime {
    snapshot: Mutex<RuntimeSnapshot>,
}

impl Default for GpuRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuRuntime {
    pub const fn new() -> Self {
        Self {
            snapshot: Mutex::new(RuntimeSnapshot::unavailable()),
        }
    }

    /// Return the process-wide runtime coordinator shared by every operation
    /// module. This does not initialize CUDA or allocate device memory.
    pub fn process() -> &'static Self {
        static RUNTIME: OnceLock<GpuRuntime> = OnceLock::new();
        RUNTIME.get_or_init(Self::new)
    }

    /// Report the current runtime state. A poisoned metadata lock is reported
    /// as unhealthy rather than making capabilities look better than they are.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| *snapshot)
            .unwrap_or_else(|_| RuntimeSnapshot::unavailable())
    }

    /// Advertise only capabilities that this crate can currently guarantee.
    pub fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::default()
    }

    /// Device discovery is a stub until the CUDA runtime is implemented.
    pub fn initialize(&self) -> Result<DeviceGeneration, BackendError> {
        Err(BackendError::unsupported(Operation::Runtime))
    }

    /// Construct the uniform error used by currently unsupported operations.
    pub(crate) const fn unsupported(operation: Operation) -> BackendError {
        BackendError {
            operation,
            kind: ErrorKind::Unsupported,
            effect: FailureEffect::NotStarted,
        }
    }
}

impl<F, G> ResidentBackend<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn generation(&self) -> Result<DeviceGeneration, BackendError> {
        // The unit shell has no live context; do not fabricate a generation.
        Err(GpuRuntime::unsupported(Operation::Runtime))
    }

    fn runtime_snapshot(&self) -> Result<ApiRuntimeSnapshot, BackendError> {
        Err(GpuRuntime::unsupported(Operation::Runtime))
    }

    fn try_reserve(&self, _bytes: u64) -> Result<ReservationResult, BackendError> {
        Err(GpuRuntime::unsupported(Operation::Reserve))
    }

    fn release_reservation(&self, _reservation: ReservationToken) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::Release))
    }

    fn upload_scalars(&self, _values: &[F]) -> Result<ResidentBuffer<F>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::Upload))
    }

    fn upload_points(&self, _values: &[G]) -> Result<ResidentBuffer<G>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::Upload))
    }

    fn download_scalars(&self, _source: &ResidentBuffer<F>) -> Result<Vec<F>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::Download))
    }

    fn transform_resident(
        &self,
        source: &ResidentBuffer<F>,
        spec: TransformSpec<F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        crate::ntt::transform_resident(source, spec)
    }

    fn release_scalars(&self, _buffer: ResidentBuffer<F>) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::Release))
    }

    fn release_points(&self, _buffer: ResidentBuffer<G>) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::Release))
    }
}
