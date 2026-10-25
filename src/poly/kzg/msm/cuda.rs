//! Optional BLS12-381 GPU adapter.
mod convert;

#[cfg(feature = "cuda")]
mod backend;
#[cfg(all(test, feature = "cuda"))]
mod tests;

#[cfg(feature = "cuda")]
use midnight_curves::{CurveAffine, Fq, G1Affine, G1Projective};
#[cfg(feature = "cuda")]
use std::{
    any::TypeId,
    sync::{Once, OnceLock},
};

#[cfg(feature = "cuda")]
fn parse_min(value: Option<&str>) -> usize {
    value.and_then(|v| v.parse().ok()).unwrap_or(1 << 14)
}

/// Minimum effective pair count (`LOKI_GPU_MSM_MIN`, default 2^14) below
/// which the legacy path uses the CPU.
#[cfg(feature = "cuda")]
pub(crate) fn min_len() -> usize {
    static MIN: OnceLock<usize> = OnceLock::new();
    *MIN.get_or_init(|| parse_min(std::env::var("LOKI_GPU_MSM_MIN").ok().as_deref()))
}

#[cfg(feature = "cuda")]
#[cfg_attr(feature = "loki-sppark", allow(dead_code))]
pub(super) fn try_msm<C: CurveAffine>(
    coeffs: &[C::Scalar],
    bases: &[C::Curve],
) -> Option<C::Curve> {
    dispatch::<C>(coeffs, bases, min_len(), backend::msm)
}

/// The legacy sppark MSM (serialized device guard, resident SRS cache, zip
/// truncation) over the concrete BLS12-381 types. P3's `loki-sppark` typed
/// adapter (`crate::backend::sppark`) calls exactly this, so routing through
/// the dispatcher cannot change the arithmetic.
#[cfg(feature = "loki-sppark")]
pub(crate) use backend::{Failure, FailureClass};

#[cfg(feature = "loki-sppark")]
pub(crate) fn sppark_msm(coeffs: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, Failure> {
    backend::classified_msm(coeffs, bases)
}

#[cfg(feature = "cuda")]
#[allow(unsafe_code)]
#[cfg_attr(feature = "loki-sppark", allow(dead_code))]
fn dispatch<C: CurveAffine>(
    coeffs: &[C::Scalar],
    bases: &[C::Curve],
    min: usize,
    gpu: impl FnOnce(&[Fq], &[G1Projective]) -> Result<G1Projective, String>,
) -> Option<C::Curve> {
    if coeffs.len().min(bases.len()) < min || TypeId::of::<C>() != TypeId::of::<G1Affine>() {
        return None;
    }
    // SAFETY: C is exactly midnight_curves::G1Affine, so its associated types
    // are Fq and G1Projective. Preserve slice metadata and borrow lifetimes.
    let coeffs = unsafe { &*(coeffs as *const [C::Scalar] as *const [Fq]) };
    let bases = unsafe { &*(bases as *const [C::Curve] as *const [G1Projective]) };
    match gpu(coeffs, bases) {
        Ok(point) => {
            // SAFETY: same concrete associated type checked above.
            Some(unsafe { std::mem::transmute_copy(&point) })
        }
        Err(error) => {
            static LOGGED: Once = Once::new();
            LOGGED.call_once(|| tracing::warn!(%error, "GPU MSM failed; using CPU MSM"));
            None
        }
    }
}

#[cfg(all(feature = "cuda", not(target_endian = "little")))]
compile_error!("the sppark CUDA adapter requires a little-endian host");
