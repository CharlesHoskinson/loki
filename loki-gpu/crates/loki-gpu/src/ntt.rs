//! Transform operations for the optional GPU backend.
//!
//! The v1 implementation is intentionally unsupported. It does not mutate or
//! consume host inputs, publish resident output, or report a successful GPU
//! dispatch. Keep this module as the single NTT dispatch seam for future kernels.

use ff::PrimeField;
use loki_backend_api::{
    BackendError, Operation, ResidentBuffer, TransformBackend, TransformSpec,
};

use crate::LokiGpu;

impl<F: PrimeField> TransformBackend<F> for LokiGpu {
    fn transform(
        &self,
        _values: &[F],
        _spec: TransformSpec<F>,
    ) -> Result<Vec<F>, BackendError> {
        // The CUDA byte-wire function is also an Unsupported stub. Returning
        // here avoids serializing field representations only to learn that no
        // kernel exists, and guarantees no tentative output is exposed.
        Err(BackendError::unsupported(Operation::Transform))
    }
}

/// Resident counterpart used by the `ResidentBackend` implementation in the
/// runtime module. A future kernel must validate the source generation and
/// return a fresh handle only after its result is complete. Until then, this
/// function preserves the input handle and reports `Unsupported`.
pub(crate) fn transform_resident<F: PrimeField>(
    _source: &ResidentBuffer<F>,
    _spec: TransformSpec<F>,
) -> Result<ResidentBuffer<F>, BackendError> {
    Err(BackendError::unsupported(Operation::ResidentTransform))
}
