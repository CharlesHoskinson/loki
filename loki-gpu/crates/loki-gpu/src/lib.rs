//! Optional GPU backend implementation for the `loki-backend-api` contracts.
//!
//! This crate is not wired into Loki's default dependency graph. Arithmetic
//! operations (MSM, NTT, quotient, SRS registry) still return `Unsupported`
//! until their packages pass correctness and stock-verifier gates. The P2
//! transactional device runtime ([`runtime::GpuRuntime`]) provides resident
//! scalar buffers once explicitly initialized on an sm_120 device.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod msm;
pub mod ntt;
pub mod quotient;
pub mod runtime;
pub mod srs;

/// Versioned C ABI mirrors and (with `cuda`) the native runtime driver.
pub mod ffi;

pub use loki_backend_api as api;

use api::{Backend, BackendCapabilities};
use ff::PrimeField;
use group::Curve;

/// Stateless shell for the optional GPU backend.
///
/// Runtime state and allocations are owned by the private runtime/SRS modules;
/// the public value carries no raw pointer or CUDA handle.
#[derive(Clone, Copy, Debug, Default)]
pub struct LokiGpu;

impl<F, G> Backend<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn capabilities(&self) -> BackendCapabilities {
        // Never advertise support based only on whether a CUDA toolkit was
        // available at build time, and never initialize the device here:
        // only an explicitly initialized, healthy runtime reports resident
        // buffers. No arithmetic operation is advertised yet.
        runtime::GpuRuntime::process().capabilities()
    }
}
