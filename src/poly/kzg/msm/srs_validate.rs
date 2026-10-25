//! Shared SRS upload validation and exact-coordinate comparison.
//!
//! Moved without behavioural change from W13's legacy registry
//! (`src/poly/kzg/msm/cuda/backend.rs` at `b6af6f9`, SHA-256
//! `d84fbb23b5ff80513de8b4c0d0f5629d2372e4126418950479b4130fa7ca6d8a`), so the
//! legacy `cuda-srs-registry` cache and the new-backend codec
//! (`loki-gpu-srs-registry`) share one implementation (P3).
//!
//! Never enter Rayon while holding a device/cache lock: even `install` on a
//! private pool can execute jobs from a calling Rayon worker's pool while it
//! waits. Scoped native workers only read CPU data and join without scheduler
//! re-entry.
#![allow(unsafe_code)]

use group::Curve;
use midnight_curves::{G1Affine, G1Projective};

/// Why an upload candidate was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Rejection {
    /// The point does not satisfy the curve equation.
    OffCurve { index: usize },
    /// The point is on the curve but outside the prime-order G1 subgroup.
    NotInSubgroup { index: usize },
    /// A native worker could not be created or panicked.
    Worker,
}

impl Rejection {
    /// The legacy error strings (kept verbatim for the W13 tests and logs).
    #[cfg_attr(not(feature = "cuda-srs-registry"), allow(dead_code))]
    pub(crate) fn message(self) -> String {
        match self {
            Self::OffCurve { .. } => "invalid SRS base: off curve".into(),
            Self::NotInSubgroup { .. } => "invalid SRS base: outside G1 subgroup".into(),
            Self::Worker => "SRS worker failed".into(),
        }
    }
}

/// Exact coordinate equality (all x/y/z limbs). Cheap and sufficient; it
/// implies group-element equality without per-point field multiplications.
pub(crate) fn same_coordinates(a: &G1Projective, b: &G1Projective) -> bool {
    blst::blst_fp::from(a.x()).l == blst::blst_fp::from(b.x()).l
        && blst::blst_fp::from(a.y()).l == blst::blst_fp::from(b.y()).l
        && blst::blst_fp::from(a.z()).l == blst::blst_fp::from(b.z()).l
}

/// Run `f` over at most four contiguous chunks on native scoped workers
/// (serially below 4096 points). Every worker is joined, even after an error
/// or panic; letting `Scope` implicitly join a second panicking worker would
/// unwind the caller.
pub(crate) fn parallel_chunks<R: Send, E: Send>(
    bases: &[G1Projective],
    worker_error: E,
    f: impl Fn(usize, &[G1Projective]) -> Result<R, E> + Sync,
) -> Result<Vec<R>, E> {
    if bases.len() < 4096 {
        return Ok(vec![f(0, bases)?]);
    }
    let chunk_len = bases.len().div_ceil(4);
    std::thread::scope(|scope| {
        let f = &f;
        let mut jobs = Vec::with_capacity(4);
        let mut error = None;
        for (i, chunk) in bases.chunks(chunk_len).enumerate() {
            match std::thread::Builder::new()
                .name("loki-srs".into())
                .spawn_scoped(scope, move || f(i * chunk_len, chunk))
            {
                Ok(job) => jobs.push(job),
                Err(_) => {
                    error = Some(None);
                    break;
                }
            }
        }
        let mut results = Vec::with_capacity(jobs.len());
        for job in jobs {
            match job.join() {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(cause)) => {
                    error.get_or_insert(Some(cause));
                }
                Err(_) => {
                    error.get_or_insert(None);
                }
            }
        }
        match error {
            None => Ok(results),
            Some(Some(cause)) => Err(cause),
            Some(None) => Err(worker_error),
        }
    })
}

/// Whether `bases` equals the prefix of `source` coordinate-for-coordinate.
#[cfg_attr(not(feature = "cuda-srs-registry"), allow(dead_code))]
pub(crate) fn same_prefix(source: &[G1Projective], bases: &[G1Projective]) -> Result<bool, Rejection> {
    if bases.len() > source.len() {
        return Ok(false);
    }
    let matches = parallel_chunks(bases, Rejection::Worker, |start, chunk| {
        Ok(source[start..start + chunk.len()]
            .iter()
            .zip(chunk)
            .all(|(a, b)| same_coordinates(a, b)))
    })?;
    Ok(matches.into_iter().all(|equal| equal))
}

/// Validate every point (curve equation, then subgroup) and return the
/// normalized affine points. Normalization uses sub-batches of 512, below the
/// pinned BLST 768-point pool threshold, so it stays serial inside each native
/// worker even without `loki-cpu-budget`.
pub(crate) fn validate(bases: &[G1Projective]) -> Result<Vec<G1Affine>, Rejection> {
    let chunks = parallel_chunks(bases, Rejection::Worker, |start, chunk| {
        if let Some(i) = chunk.iter().position(|base| !bool::from(base.is_on_curve())) {
            return Err(Rejection::OffCurve { index: start + i });
        }
        let mut affine = vec![G1Affine::default(); chunk.len()];
        for (points, output) in chunk.chunks(512).zip(affine.chunks_mut(512)) {
            G1Projective::batch_normalize(points, output);
        }
        if let Some(i) = affine
            .iter()
            .position(|base| !bool::from(base.is_torsion_free()))
        {
            return Err(Rejection::NotInSubgroup { index: start + i });
        }
        Ok(affine)
    })?;
    Ok(chunks.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff::Field;
    use group::Group;
    use midnight_curves::{CurveExt, Fp};
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    fn corrupt() -> G1Projective {
        let raw = blst::blst_p1 {
            x: Fp::ONE.into(),
            y: Fp::ONE.into(),
            z: Fp::ONE.into(),
        };
        // SAFETY: G1Projective is repr(transparent) over blst_p1; reduced limbs.
        unsafe { std::mem::transmute(raw) }
    }

    #[test]
    fn srs_validate_rejects_with_index_and_joins_workers() {
        let mut rng = ChaCha20Rng::from_seed([7; 32]);
        let g = G1Projective::random(&mut rng);
        let mut bases = vec![g; 5000];
        assert_eq!(validate(&bases).unwrap().len(), 5000);
        bases[4321] = corrupt();
        assert_eq!(validate(&bases), Err(Rejection::OffCurve { index: 4321 }));
        // (0, 2) is on y^2 = x^3 + 4 but has order 3.
        let torsion = G1Projective::new_jacobian(Fp::ZERO, Fp::from(2), Fp::ONE).unwrap();
        bases[4321] = g;
        bases[17] = torsion;
        assert_eq!(validate(&bases), Err(Rejection::NotInSubgroup { index: 17 }));
        assert_eq!(validate(&bases).unwrap_err().message(), "invalid SRS base: outside G1 subgroup");
        let outcome = parallel_chunks(&bases, "worker", |start, _| -> Result<(), &str> {
            if start == 0 {
                return Err("first chunk rejected");
            }
            panic!("synthetic worker failure");
        });
        assert_eq!(outcome.unwrap_err(), "first chunk rejected");
    }

    #[test]
    fn srs_same_prefix_is_exact() {
        let mut rng = ChaCha20Rng::from_seed([8; 32]);
        let source: Vec<_> = (0..4100).map(|_| G1Projective::random(&mut rng)).collect();
        assert!(same_prefix(&source, &source[..4099]).unwrap());
        let mut other = source[..4099].to_vec();
        other[4000] = source[0];
        assert!(!same_prefix(&source, &other).unwrap());
        // Same group element, different projective representation: a miss.
        let doubled_z = {
            let p = source[5];
            let two = Fp::from(2);
            let raw = blst::blst_p1 {
                x: (p.x() * two.square()).into(),
                y: (p.y() * two.square() * two).into(),
                z: (p.z() * two).into(),
            };
            // SAFETY: as above; reduced limbs of an equivalent Jacobian point.
            let q: G1Projective = unsafe { std::mem::transmute(raw) };
            assert_eq!(q, p);
            q
        };
        let mut rep = source[..10].to_vec();
        rep[5] = doubled_z;
        assert!(!same_prefix(&source, &rep).unwrap());
        assert!(!same_prefix(&source[..3], &source[..4]).unwrap());
    }
}
