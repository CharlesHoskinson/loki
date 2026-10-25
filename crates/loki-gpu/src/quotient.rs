//! Resident quotient numerator evaluation seam.
//!
//! The v1 bytecode interpreter is intentionally unsupported. It neither
//! reads resident columns nor publishes an output buffer; a future kernel
//! must implement the checked, ordered six-op contract in the backend API.

use ff::PrimeField;
use loki_backend_api::{BackendError, Operation, QuotientBackend, QuotientProgram, ResidentBuffer};

use crate::LokiGpu;

impl<F: PrimeField> QuotientBackend<F> for LokiGpu {
    fn evaluate_quotient(
        &self,
        _program: QuotientProgram<'_, F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        // Do not inspect handles or return a partially initialized result.
        Err(BackendError::unsupported(Operation::Quotient))
    }
}
