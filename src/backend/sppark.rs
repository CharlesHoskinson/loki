//! P3 legacy sppark adapter, selected by `loki-sppark`.
//!
//! [`SpparkBackend`] presents the existing W13/legacy sppark MSM
//! (`poly::kzg::msm::cuda::sppark_msm`: the `loki-cuda-runtime` device guard,
//! the resident SRS cache and its validation, the 2^14 `LOKI_GPU_MSM_MIN`
//! threshold) as a typed `MsmBackend`, so `msm_specific` reaches it through
//! the Loki dispatcher instead of the hard-wired legacy arm. The arithmetic,
//! conversions and type check are unchanged, so outputs are unchanged:
//!
//! - admission reproduces the legacy arm exactly: an effective pair count
//!   below the threshold, or a type other than BLS12-381 G1, is an ordinary
//!   CPU call (`capabilities_for`), never an accelerator failure;
//! - an admitted call runs the same `backend::msm` with the same untruncated
//!   base slice (the registry may upload the whole supplied capacity);
//! - any legacy error is reported as `SessionPoisoned`: the legacy cache
//!   already disables itself for the process after an error, and the
//!   dispatcher then stops offering work and recomputes on the CPU.
//!
//! Never-race argument: `loki-sppark` implies the legacy `cuda`/`cuda-ntt`
//! features, and `src/lib.rs` rejects any build that also enables
//! `loki-gpu-backend` (hence `loki-gpu-srs-registry`). The legacy sppark
//! guard and the new runtime's coordinator therefore never coexist in one
//! Loki build, so the new runtime's `cudaDeviceReset` on reinitialization
//! (P2 review risk #7) cannot destroy live sppark state.

use std::any::TypeId;
use std::sync::Once;

use ff::PrimeField;
use group::Curve;
use midnight_curves::{Fq, G1Projective};

use super::api::{
    BackendCapabilities, BackendError, ErrorKind, FailureEffect, MsmBackend, MsmRequest,
    Operation,
};
use super::arithmetic_abi;
use crate::poly::kzg::msm::cuda::{min_len, sppark_msm};

/// Typed adapter over the legacy sppark MSM.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpparkBackend;

impl SpparkBackend {
    /// Capabilities for one call with `pairs` effective pairs. The legacy
    /// threshold is part of admission, so below-threshold calls are counted
    /// as ordinary CPU calls exactly as the legacy arm ran them.
    pub fn capabilities_for(pairs: usize) -> BackendCapabilities {
        let abi = arithmetic_abi::<midnight_curves::G1Affine>().expect("BLS12-381 G1 ABI");
        BackendCapabilities {
            host_msm: pairs >= min_len(),
            supported_arithmetic: vec![abi],
            ..BackendCapabilities::default()
        }
    }
}

const fn unsupported() -> BackendError {
    BackendError::unsupported(Operation::Msm)
}

impl<F, G> MsmBackend<F, G> for SpparkBackend
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn msm(&self, request: MsmRequest<'_, F, G>) -> Result<G, BackendError> {
        msm_with(request, sppark_msm)
    }
}

const fn poisoned(kind: ErrorKind) -> BackendError {
    BackendError {
        operation: Operation::Msm,
        kind,
        effect: FailureEffect::SessionPoisoned,
    }
}

/// W9 output membership: an accelerator point is published only if it is on
/// the curve and in the prime-order G1 subgroup. The legacy decode already
/// refuses non-canonical and off-curve device coordinates; this re-checks the
/// typed result at the adapter boundary (the dispatcher publishes a
/// successful accelerator point unchecked), including the subgroup.
fn admit_result(point: &G1Projective) -> Result<(), BackendError> {
    let affine = point.to_affine();
    if bool::from(point.is_on_curve()) && bool::from(affine.is_torsion_free()) {
        Ok(())
    } else {
        Err(poisoned(ErrorKind::Kernel))
    }
}

/// Adapter body over an injectable device call (the real one is
/// `sppark_msm`; tests inject a fake returning invalid points).
#[allow(unsafe_code)]
fn msm_with<F, G>(
    request: MsmRequest<'_, F, G>,
    gpu: impl FnOnce(&[Fq], &[G1Projective]) -> Result<G1Projective, String>,
) -> Result<G, BackendError>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    let pairs = request.pair_count()?;
    if TypeId::of::<F>() != TypeId::of::<Fq>()
        || TypeId::of::<G>() != TypeId::of::<G1Projective>()
        || pairs == 0
        || pairs < min_len()
    {
        return Err(unsupported());
    }
    // SAFETY: F and G are exactly Fq and G1Projective (checked above);
    // slice metadata and borrow lifetimes are preserved.
    let scalars = unsafe { &*(request.scalars as *const [F] as *const [Fq]) };
    let bases = unsafe { &*(request.bases as *const [G] as *const [G1Projective]) };
    // RequireEqual has already rejected unequal lengths; ZipTruncate keeps
    // the legacy slices (the legacy entry applies `min` itself).
    match gpu(scalars, bases) {
        Ok(point) => {
            if let Err(rejected) = admit_result(&point) {
                static LOGGED: Once = Once::new();
                LOGGED.call_once(|| {
                    tracing::warn!("GPU MSM result outside G1; disabling GPU MSM, using CPU")
                });
                return Err(rejected);
            }
            // SAFETY: G is exactly G1Projective (checked above).
            Ok(unsafe { std::mem::transmute_copy::<G1Projective, G>(&point) })
        }
        Err(error) => {
            static LOGGED: Once = Once::new();
            LOGGED.call_once(|| tracing::warn!(%error, "GPU MSM failed; using CPU MSM"));
            Err(poisoned(ErrorKind::Kernel))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{api::MsmLengthPolicy, dispatch::Dispatcher};
    use crate::poly::kzg::msm::msm_cpu;
    use ff::Field;
    use group::Group;
    use midnight_curves::{G1Affine, G2Projective};
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    #[test]
    fn sppark_adapter_admission_matches_legacy_threshold() {
        let min = min_len();
        assert!(!SpparkBackend::capabilities_for(min - 1).host_msm);
        assert!(SpparkBackend::capabilities_for(min).host_msm);
        let abi = arithmetic_abi::<G1Affine>().unwrap();
        assert!(SpparkBackend::capabilities_for(min).supports_arithmetic(abi));
        // Below the threshold the dispatcher computes on CPU without calling
        // the adapter (no GPU needed here) and counts no fallback.
        let mut rng = ChaCha20Rng::from_seed([3; 32]);
        let s: Vec<Fq> = (0..64).map(|_| Fq::random(&mut rng)).collect();
        let b: Vec<G1Projective> = (0..64).map(|_| G1Projective::random(&mut rng)).collect();
        let d = Dispatcher::new();
        let caps = SpparkBackend::capabilities_for(64);
        let got = d.msm::<G1Affine, _>(Some((&SpparkBackend, &caps)), &s, &b);
        assert_eq!(got, msm_cpu::<G1Affine>(&s, &b));
        let c = d.counters();
        assert_eq!((c.msm_cpu, c.msm_accelerated, c.msm_fallback), (1, 0, 0));
    }

    #[test]
    fn sppark_adapter_rejects_other_types_and_lengths_before_device() {
        let s = vec![Fq::ONE; 1 << 15];
        let g2 = vec![G2Projective::generator(); 1 << 15];
        let r = SpparkBackend.msm(MsmRequest::new(&s, &g2, MsmLengthPolicy::ZipTruncate));
        assert_eq!(r.unwrap_err(), unsupported());
        let g1 = vec![G1Projective::generator(); 1 << 15];
        let r = SpparkBackend.msm(MsmRequest::new(&s[..10], &g1, MsmLengthPolicy::RequireEqual));
        assert_eq!(r.unwrap_err().kind, ErrorKind::InvalidInput);
        let r = SpparkBackend.msm(MsmRequest::new(&s[..10], &g1, MsmLengthPolicy::ZipTruncate));
        assert_eq!(r.unwrap_err(), unsupported(), "below threshold");
    }

    /// Fake device returning an off-curve or non-subgroup point: the adapter
    /// refuses it, the dispatcher poisons the accelerator and publishes the
    /// CPU result; a valid but wrong-looking point is never needed here.
    #[test]
    fn sppark_adapter_rejects_invalid_device_points() {
        #![allow(unsafe_code)]
        use midnight_curves::{CurveExt, Fp};
        struct Fake(fn() -> G1Projective);
        impl MsmBackend<Fq, G1Projective> for Fake {
            fn msm(&self, r: MsmRequest<'_, Fq, G1Projective>) -> Result<G1Projective, BackendError> {
                let point = (self.0)();
                msm_with(r, |_, _| Ok(point))
            }
        }
        fn off_curve() -> G1Projective {
            let raw = blst::blst_p1 { x: Fp::ONE.into(), y: Fp::ONE.into(), z: Fp::ONE.into() };
            // SAFETY: G1Projective is repr(transparent) over blst_p1; reduced limbs.
            unsafe { std::mem::transmute(raw) }
        }
        fn torsion() -> G1Projective {
            // (0, 2) is on y^2 = x^3 + 4 but has order 3.
            G1Projective::new_jacobian(Fp::ZERO, Fp::from(2), Fp::ONE).unwrap()
        }
        assert!(bool::from(torsion().is_on_curve()));
        let n = min_len();
        let mut rng = ChaCha20Rng::from_seed([4; 32]);
        let s: Vec<Fq> = (0..n).map(|_| Fq::random(&mut rng)).collect();
        let b: Vec<G1Projective> = (0..n).map(|_| G1Projective::random(&mut rng)).collect();
        let cpu = msm_cpu::<G1Affine>(&s, &b);
        let caps = SpparkBackend::capabilities_for(n);
        for bad in [off_curve as fn() -> G1Projective, torsion] {
            let direct = Fake(bad).msm(MsmRequest::new(&s, &b, MsmLengthPolicy::ZipTruncate));
            assert_eq!(direct.unwrap_err(), poisoned(ErrorKind::Kernel));
            let d = Dispatcher::new();
            assert_eq!(d.msm::<G1Affine, _>(Some((&Fake(bad), &caps)), &s, &b), cpu);
            let c = d.counters();
            assert_eq!((c.msm_accelerated, c.msm_fallback, c.accelerator_poisoned), (0, 1, 1), "{c:?}");
            // Poisoned: later calls are not offered to the accelerator.
            assert_eq!(d.msm::<G1Affine, _>(Some((&Fake(bad), &caps)), &s, &b), cpu);
            let c = d.counters();
            assert_eq!((c.msm_accelerated, c.msm_fallback, c.msm_cpu), (0, 1, 2), "{c:?}");
        }
        // A valid point (identity and the CPU answer) is admitted unchanged.
        let ok = Fake(G1Projective::identity);
        assert_eq!(ok.msm(MsmRequest::new(&s, &b, MsmLengthPolicy::ZipTruncate)), Ok(G1Projective::identity()));
    }

    /// Adapter output equals the legacy direct call and the stock CPU MSM,
    /// including prefixes, same-address mutation and zip truncation.
    #[test]
    #[ignore = "requires a GPU; run under the GPU lock with --ignored"]
    fn gpu_sppark_adapter_matches_legacy_and_cpu() {
        let mut rng = ChaCha20Rng::from_seed([5; 32]);
        let n = (1 << 15) + 3;
        let mut s: Vec<Fq> = (0..n).map(|_| Fq::random(&mut rng)).collect();
        s[7] = Fq::ZERO;
        s[8] = -Fq::ONE;
        let base: Vec<G1Projective> = (0..256).map(|_| G1Projective::random(&mut rng)).collect();
        let mut b: Vec<G1Projective> = (0..n).map(|i| base[i % 256]).collect();
        let d = Dispatcher::new();
        let mut cases = 0;
        for (sl, bl) in [(n, n), (n - 1, n), (n, n - 1), (1 << 14, n), (n, 1 << 14), ((1 << 14) - 1, n)] {
            let cpu = msm_cpu::<G1Affine>(&s[..sl], &b[..bl]);
            let legacy = sppark_msm(&s[..sl], &b[..bl]).unwrap();
            let caps = SpparkBackend::capabilities_for(sl.min(bl));
            let adapted = d.msm::<G1Affine, _>(Some((&SpparkBackend, &caps)), &s[..sl], &b[..bl]);
            assert_eq!(legacy, cpu, "legacy {sl}/{bl}");
            assert_eq!(adapted, cpu, "adapter {sl}/{bl}");
            cases += 1;
        }
        b[1000] = G1Projective::random(&mut rng); // same-address mutation
        let caps = SpparkBackend::capabilities_for(n);
        assert_eq!(
            d.msm::<G1Affine, _>(Some((&SpparkBackend, &caps)), &s, &b),
            msm_cpu::<G1Affine>(&s, &b)
        );
        let c = d.counters();
        assert_eq!(c.msm_accelerated, cases - 1 + 1, "{c:?}");
        assert_eq!((c.msm_cpu, c.msm_fallback, c.accelerator_poisoned), (1, 0, 0));
    }
}
