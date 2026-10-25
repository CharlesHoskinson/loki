//! SRS residency registry surface.
//!
//! The v1 crate has no CUDA implementation, so every operation returns
//! `Unsupported` before inspecting or mutating caller data. The cache design is
//! exact-content and lease-aware; a digest or host pointer alone is never a hit.

use loki_backend_api::{
    BackendError, Curve, ErrorKind, FailureEffect, Operation, PrimeField, SrsLease, SrsRegistry,
    SrsRequest,
};

use crate::runtime::{GpuRuntime, RuntimeSnapshot, DEFAULT_SRS_BUDGET_BYTES};
use crate::LokiGpu;

/// Environment key for the persistent SRS allocation budget.
pub const SRS_BUDGET_ENV: &str = "LOKI_GPU_SRS_BUDGET_BYTES";

/// SRS registry handle. The future CUDA implementation stores immutable
/// validated host snapshots and generation-tagged device allocations here.
/// Cache entries with active leases are not eligible for eviction.
#[derive(Debug)]
pub struct GpuSrsRegistry {
    runtime: &'static GpuRuntime,
    budget_bytes: u64,
}

impl GpuSrsRegistry {
    /// Construct a registry using the default 4 GiB persistent-SRS budget.
    pub fn new() -> Self {
        Self {
            runtime: GpuRuntime::process(),
            budget_bytes: DEFAULT_SRS_BUDGET_BYTES,
        }
    }

    /// Construct a registry using the optional byte budget override. Invalid
    /// values are rejected instead of silently disabling or enlarging the
    /// resident cache.
    pub fn from_environment() -> Result<Self, BackendError> {
        let budget_bytes = match std::env::var(SRS_BUDGET_ENV) {
            Ok(raw) => raw.parse::<u64>().map_err(|_| BackendError {
                operation: Operation::SrsAcquire,
                kind: ErrorKind::InvalidInput,
                effect: FailureEffect::NotStarted,
            })?,
            Err(std::env::VarError::NotPresent) => DEFAULT_SRS_BUDGET_BYTES,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(BackendError {
                    operation: Operation::SrsAcquire,
                    kind: ErrorKind::InvalidInput,
                    effect: FailureEffect::NotStarted,
                });
            }
        };
        Ok(Self {
            runtime: GpuRuntime::process(),
            budget_bytes,
        })
    }

    /// Construct a registry with an explicit persistent SRS budget.
    pub fn with_budget(budget_bytes: u64) -> Self {
        Self {
            runtime: GpuRuntime::process(),
            budget_bytes,
        }
    }

    /// Return the configured persistent SRS budget. This excludes per-call
    /// MSM scratch, which is charged to the shared device runtime budget.
    pub const fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Snapshot the shared process runtime for scheduler admission and
    /// diagnostics. Unknown values remain unknown on a CUDA-free build.
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }

    /// Validate request metadata before a future implementation starts an
    /// upload. This does not establish a cache hit or validate curve points.
    pub fn validate_metadata<G>(&self, request: &SrsRequest<'_, G>) -> Result<(), BackendError> {
        if request.key.point_count != request.points.len() {
            return Err(BackendError {
                operation: Operation::SrsAcquire,
                kind: ErrorKind::InvalidInput,
                effect: FailureEffect::NotStarted,
            });
        }
        if self.budget_bytes == 0 {
            return Err(BackendError {
                operation: Operation::SrsAcquire,
                kind: ErrorKind::Capacity,
                effect: FailureEffect::NotStarted,
            });
        }
        Ok(())
    }
}

impl Default for GpuSrsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl<F, G> SrsRegistry<F, G> for GpuSrsRegistry
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, _request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsAcquire))
    }

    fn srs_msm(
        &self,
        _lease: &SrsLease<G>,
        _scalars: &[F],
        _length_policy: loki_backend_api::MsmLengthPolicy,
    ) -> Result<G, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsMsm))
    }

    fn release_srs(&self, _lease: SrsLease<G>) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsRelease))
    }
}

/// The public backend shell directly implements every API supertrait. Keep the
/// registry shell separate for future lifecycle/accounting methods, while
/// reporting unsupported operations consistently through `LokiGpu`.
impl<F, G> SrsRegistry<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, _request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsAcquire))
    }

    fn srs_msm(
        &self,
        _lease: &SrsLease<G>,
        _scalars: &[F],
        _length_policy: loki_backend_api::MsmLengthPolicy,
    ) -> Result<G, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsMsm))
    }

    fn release_srs(&self, _lease: SrsLease<G>) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsRelease))
    }
}
