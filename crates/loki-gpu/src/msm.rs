//! Skew-robust BLS12-381 G1 MSM backend seam.
//!
//! V1 is deliberately unsupported. The future implementation will convert
//! field and curve values through checked wire conversions and resolve
//! validated resident SRS handles before invoking a private native path; this
//! module never casts Rust field/group layouts. The synchronous host-byte ABI
//! in `cuda/include/loki_gpu_ffi.h` is a stub only. Its documented wire layout
//! uses four little-endian `u64` scalar limbs and affine G1 `x[6] || y[6]`
//! limbs, both in the documented Montgomery representation.

use ff::PrimeField;
use group::Curve;
use loki_backend_api::{BackendError, MsmBackend, MsmRequest, Operation};

use crate::LokiGpu;

impl<F, G> MsmBackend<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn msm(&self, request: MsmRequest<'_, F, G>) -> Result<G, BackendError> {
        // Preserve the selected stock call-site policy even while unsupported:
        // RequireEqual rejects before dispatch; ZipTruncate uses the stock
        // `min(scalars, bases)` effective length. Inputs are never inspected.
        let _pair_count = request.pair_count()?;

        Err(BackendError::unsupported(Operation::Msm))
    }
}
