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

#[cfg(feature = "cuda")]
pub(super) fn try_msm<C: CurveAffine>(
    coeffs: &[C::Scalar],
    bases: &[C::Curve],
) -> Option<C::Curve> {
    static MIN: OnceLock<usize> = OnceLock::new();
    let min = *MIN.get_or_init(|| parse_min(std::env::var("LOKI_GPU_MSM_MIN").ok().as_deref()));
    dispatch::<C>(coeffs, bases, min, backend::msm)
}

#[cfg(feature = "cuda")]
#[allow(unsafe_code)]
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
