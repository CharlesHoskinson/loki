//! Serialized access to sppark streams and a bounded cache of resident SRS bases.
#![allow(unsafe_code)]

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use std::{collections::VecDeque, ffi::c_void, sync::Mutex};

use group::Curve;
use midnight_curves::{Fq, G1Affine, G1Projective};

use super::convert::{self, Affine, Scalar};

extern "C" {
    fn loki_gpu_prepare(out: &mut *mut c_void, bases: *const Affine, n: usize) -> sppark::Error;
    fn loki_gpu_msm(
        resident: *mut c_void,
        out: &mut Affine,
        scalars: *const Scalar,
        n: usize,
    ) -> sppark::Error;
    fn loki_gpu_release(resident: *mut c_void) -> sppark::Error;
}

struct Entry {
    address: usize,
    source: Vec<G1Projective>,
    resident: *mut c_void,
}

// All access, including destruction, holds the process device guard, then CACHE.
// Each FFI entry sets the active device and completes work before returning.
unsafe impl Send for Entry {}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    failed: bool,
}

#[cfg(test)]
static GPU_CALLS: AtomicUsize = AtomicUsize::new(0);

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    entries: VecDeque::new(),
    failed: false,
});

fn status(error: sppark::Error) -> Result<(), String> {
    if error.code == 0 {
        Ok(())
    } else {
        Err(error.to_string())
    }
}

fn same_coordinates(a: &G1Projective, b: &G1Projective) -> bool {
    // Exact coordinate equality is cheap and sufficient here. Mathematical
    // equality would perform field multiplications for every cached point.
    blst::blst_fp::from(a.x()).l == blst::blst_fp::from(b.x()).l
        && blst::blst_fp::from(a.y()).l == blst::blst_fp::from(b.y()).l
        && blst::blst_fp::from(a.z()).l == blst::blst_fp::from(b.z()).l
}

impl Cache {
    fn run(&mut self, scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, String> {
        if self.failed {
            return Err("GPU backend disabled after an earlier error".into());
        }
        let address = bases.as_ptr() as usize;
        let hit = self.entries.iter().position(|entry| {
            entry.address == address
                && entry.source.len() == bases.len()
                && entry
                    .source
                    .iter()
                    .zip(bases)
                    .all(|(a, b)| same_coordinates(a, b))
        });
        let entry = if let Some(index) = hit {
            self.entries.remove(index).unwrap()
        } else {
            // LokiLean/Msm.lean: MeetsMsmSpecific assumes prime-order G1
            // inputs. Validate once per upload, before preparing device bases;
            // a cache hit compares every coordinate against this validated copy.
            // Keep validation serial while holding the device/cache locks:
            // Rayon workers may themselves be waiting to enter this backend.
            if bases.iter().any(|base| !bool::from(base.is_on_curve())) {
                return Err("invalid SRS base: off curve".into());
            }
            let mut affine = vec![G1Affine::default(); bases.len()];
            G1Projective::batch_normalize(bases, &mut affine);
            if affine.iter().any(|base| !bool::from(base.is_torsion_free())) {
                return Err("invalid SRS base: outside G1 subgroup".into());
            }
            // Bound residency to two SRS arrays (coefficient and Lagrange).
            if self.entries.len() == 2 {
                let old = self.entries.pop_front().unwrap();
                // SAFETY: this handle was created by prepare and is not in use.
                status(unsafe { loki_gpu_release(old.resident) })?;
            }
            let wire: Vec<_> = affine.iter().map(convert::affine).collect();
            let mut resident = std::ptr::null_mut();
            // SAFETY: live, correctly sized/aligned arrays; C++ copies them.
            status(unsafe { loki_gpu_prepare(&mut resident, wire.as_ptr(), wire.len()) })?;
            if resident.is_null() {
                return Err("GPU preparation returned a null handle".into());
            }
            Entry {
                address,
                source: bases.to_vec(),
                resident,
            }
        };
        // Keep the entry reachable even when invoking/decoding fails. After a
        // GPU fault we retain handles until process exit rather than invoke
        // sppark destructors on a potentially poisoned context.
        let resident = entry.resident;
        self.entries.push_back(entry);
        let scalars: Vec<_> = scalars.iter().map(convert::scalar).collect();
        let mut output = Affine::default();
        // SAFETY: resident owns at least scalars.len() bases. Wire structs have
        // repr(C), and C++ asserts their sizes/alignments against kernel types.
        status(unsafe { loki_gpu_msm(resident, &mut output, scalars.as_ptr(), scalars.len()) })?;
        convert::decode(&output)
    }
}

pub(super) fn msm(scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, String> {
    // Preserve msm_specific's zip semantics.
    let n = scalars.len().min(bases.len());
    if n == 0 {
        return Ok(G1Projective::default());
    }
    let _device = loki_cuda_runtime::lock()?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| "GPU cache lock poisoned".to_owned())?;
    let result = cache.run(&scalars[..n], &bases[..n]);
    if result.is_err() {
        cache.failed = true;
    }
    #[cfg(test)]
    if result.is_ok() {
        GPU_CALLS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

#[cfg(test)]
pub(super) fn clear_cache() -> Result<(), String> {
    let _device = loki_cuda_runtime::lock()?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| "GPU cache lock poisoned".to_owned())?;
    if cache.failed {
        return Err("GPU backend disabled".into());
    }
    while let Some(entry) = cache.entries.pop_front() {
        status(unsafe { loki_gpu_release(entry.resident) })?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn gpu_calls() -> usize {
    GPU_CALLS.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn failed() -> bool {
    CACHE.lock().unwrap().failed
}

#[cfg(test)]
mod upload_tests {
    use super::*;
    use ff::Field;

    #[test]
    fn gpu_rejects_corrupted_resident_base() {
        use midnight_curves::Fp;
        // Raw unchecked APIs can construct points outside the group. Never cache one.
        let raw = blst::blst_p1 {
            x: Fp::ONE.into(),
            y: Fp::ONE.into(),
            z: Fp::ONE.into(),
        };
        // SAFETY: G1Projective is repr(transparent) over blst_p1; reduced limbs.
        let corrupt: G1Projective = unsafe { std::mem::transmute(raw) };
        assert!(!bool::from(corrupt.is_on_curve()));
        let mut cache = Cache::default();
        let error = cache.run(&[Fq::ONE], &[corrupt]).unwrap_err();
        assert!(error.contains("invalid SRS base"), "{error}");
        assert_eq!(cache.entries.len(), 0);
        // (0, 2) lies on y^2 = x^3 + 4 but has order 3, outside prime-order G1.
        use midnight_curves::CurveExt;
        let torsion = G1Projective::new_jacobian(Fp::ZERO, Fp::from(2), Fp::ONE).unwrap();
        assert!(bool::from(torsion.is_on_curve()));
        let error = cache.run(&[Fq::ONE], &[torsion]).unwrap_err();
        assert!(error.contains("outside G1 subgroup"), "{error}");
        assert_eq!(cache.entries.len(), 0);
    }
}
