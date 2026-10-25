//! Optional GPU backend implementation for the `loki-backend-api` contracts.
//!
//! This crate is not wired into Loki's default dependency graph. The initial
//! implementation advertises no supported operations; stubs return
//! `Unsupported` until correctness and stock-verifier gates pass.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod msm;
pub mod ntt;
pub mod quotient;
pub mod runtime;
pub mod srs;

pub(crate) mod ffi;

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
        // The implementation skeleton has no kernels enabled. Never advertise
        // support based only on whether a CUDA toolkit was available at build.
        BackendCapabilities::default()
    }
}
