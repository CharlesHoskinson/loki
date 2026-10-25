//! Serialized access to sppark streams and bounded resident SRS storage.
#![allow(unsafe_code)]

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use std::{collections::VecDeque, ffi::c_void, sync::Mutex};

#[cfg(not(feature = "cuda-srs-registry"))]
use group::Curve;

struct PrepareTimer {
    name: &'static str,
    size: usize,
    started: std::time::Instant,
}

impl PrepareTimer {
    fn start(name: &'static str, size: usize) -> Self {
        Self {
            name,
            size,
            started: std::time::Instant::now(),
        }
    }
}

impl Drop for PrepareTimer {
    fn drop(&mut self) {
        record(
            self.name,
            self.size,
            self.started.elapsed().as_nanos() as u64,
        );
    }
}

fn record(name: &'static str, size: usize, wall_ns: u64) {
    #[cfg(feature = "loki-profile")]
    crate::profile::record_backend_timing(name, size, wall_ns);
    #[cfg(not(feature = "loki-profile"))]
    let _ = (name, size, wall_ns);
}
#[cfg(not(feature = "cuda-srs-registry"))]
use midnight_curves::G1Affine;
use midnight_curves::{Fq, G1Projective};

use super::convert::{self, Affine, Scalar};
#[cfg(feature = "cuda-srs-registry")]
use crate::poly::kzg::msm::srs_validate::{same_prefix, validate};
#[cfg(not(feature = "cuda-srs-registry"))]
use crate::poly::kzg::msm::srs_validate::same_coordinates;

extern "C" {
    fn loki_gpu_prepare(out: &mut *mut c_void, bases: *const Affine, n: usize) -> sppark::Error;
    fn loki_gpu_msm(
        resident: *mut c_void,
        out: &mut Affine,
        scalars: *const Scalar,
        n: usize,
    ) -> sppark::Error;
    fn loki_gpu_release(resident: *mut c_void) -> sppark::Error;
    #[cfg(feature = "cuda-srs-registry")]
    fn cudaDeviceGetAttribute(value: &mut i32, attribute: i32, device: i32) -> i32;
    #[cfg(feature = "cuda-srs-registry")]
    fn cudaGetDeviceCount(count: &mut i32) -> i32;
}

struct Entry {
    address: usize,
    source: Vec<G1Projective>,
    resident: *mut c_void,
    #[cfg(feature = "cuda-srs-registry")]
    device_bytes: usize,
}

// All access, including destruction, holds the process device guard, then CACHE.
// Each FFI entry sets the active device and completes work before returning.
unsafe impl Send for Entry {}

/// Why a legacy MSM request failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(feature = "cuda-srs-registry"), allow(dead_code))]
pub(crate) enum FailureClass {
    /// The device, driver or kernel failed, or produced an invalid result.
    Device,
    /// The request's SRS bases were refused before upload.
    InvalidInput,
    /// The request does not fit the persistent SRS budget.
    Capacity,
    /// A host-side worker could not run (no device work started).
    Host,
}

/// A legacy MSM failure. Only device faults are sticky (quint-q2 L2): a
/// request refused on the host (invalid or oversized SRS, host worker
/// failure) leaves the backend usable, and the caller recomputes that one
/// request on the CPU. Every `String` error converts as a device fault, so an
/// unclassified failure stays conservative.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Failure {
    pub(crate) message: String,
    pub(crate) class: FailureClass,
}

impl Failure {
    fn new(class: FailureClass, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            class,
        }
    }

    /// Whether this failure disables the backend for the process.
    pub(crate) fn is_device(&self) -> bool {
        self.class == FailureClass::Device
    }

    #[cfg(feature = "cuda-srs-registry")]
    fn rejection(rejection: crate::poly::kzg::msm::srs_validate::Rejection) -> Self {
        use crate::poly::kzg::msm::srs_validate::Rejection;
        let class = match rejection {
            Rejection::OffCurve { .. } | Rejection::NotInSubgroup { .. } => FailureClass::InvalidInput,
            Rejection::Worker => FailureClass::Host,
        };
        Self::new(class, rejection.message())
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::new(FailureClass::Device, message)
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    failed: bool,
    /// Live handles returned with a preparation error (quint-q4 F4: the
    /// constructor allocated, then the completing sync failed). They are
    /// never used or destroyed on the faulted context, only kept reachable.
    retained: Vec<Retained>,
}

struct Retained(#[allow(dead_code)] *mut c_void);

// Held only for reachability under CACHE; never dereferenced.
unsafe impl Send for Retained {}

#[cfg(test)]
static GPU_CALLS: AtomicUsize = AtomicUsize::new(0);

static CACHE: Mutex<Cache> = Mutex::new(Cache {
    entries: VecDeque::new(),
    failed: false,
    retained: Vec::new(),
});

fn status(error: sppark::Error) -> Result<(), String> {
    if error.code == 0 {
        Ok(())
    } else {
        Err(error.to_string())
    }
}

#[cfg(feature = "cuda-srs-registry")]
fn parse_budget(value: Option<&str>) -> usize {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(4 << 30)
}

// Exact persistent allocation requested by pinned sppark 0.1.16's msm_t
// constructor (pippenger.cuh:351-375). BLS12-381 affine_h=96, bucket_h=192,
// scalar bit length=255, WARP_SZ=32, BATCH_ADD_BLOCK_SIZE=256. Invocation
// scratch, driver overhead and other backends are outside this budget.
#[cfg(feature = "cuda-srs-registry")]
fn resident_bytes(capacity: usize, sm_count: usize) -> Result<usize, String> {
    let overflow = || "SRS resident size overflow".to_owned();
    let rounded = capacity.checked_add(31).ok_or_else(overflow)? & !31;
    let wbits = if rounded > 192 {
        let scaled = rounded.checked_add(rounded / 2).ok_or_else(overflow)?;
        ((scaled.ilog2() as usize).saturating_sub(8)).clamp(10, 18)
    } else {
        10
    };
    let rows = (254 / wbits + 1) * (1 << (wbits - 1));
    let buckets = sm_count
        .checked_mul(8)
        .and_then(|extra| rows.checked_add(extra))
        .and_then(|count| count.checked_mul(192))
        .ok_or_else(overflow)?;
    rounded
        .checked_mul(96)
        .and_then(|points| points.checked_add(buckets))
        .and_then(|bytes| bytes.checked_add(rows * 4))
        .ok_or_else(overflow)
}

#[cfg(feature = "cuda-srs-registry")]
fn device_sm_count() -> Result<usize, String> {
    // sppark index 0 is the first *eligible* CUDA ordinal, which need not be
    // runtime ordinal 0. Bound its per-SM allocation using the largest visible
    // SM count. This is exact on one GPU, conservative on mixed-GPU hosts,
    // and does not require changing the native adapter or its device selection.
    let mut devices = 0;
    let code = unsafe { cudaGetDeviceCount(&mut devices) };
    if code != 0 || devices <= 0 {
        return Err(format!(
            "CUDA device count query failed: {code}, count={devices}"
        ));
    }
    let mut largest = 0;
    for device in 0..devices {
        // CUDA Runtime cudaDevAttrMultiProcessorCount = 16 (driver_types.h).
        let mut count = 0;
        let code = unsafe { cudaDeviceGetAttribute(&mut count, 16, device) };
        if code != 0 || count <= 0 {
            return Err(format!("CUDA SM count query failed: {code}, count={count}"));
        }
        largest = largest.max(count as usize);
    }
    Ok(largest)
}

impl Cache {
    /// Release one entry, removing it only after the release succeeded
    /// (quint-q4 F2 for the legacy two-entry path as well as the registry).
    fn release_entry(
        &mut self,
        index: usize,
        release: impl FnOnce(*mut c_void) -> Result<(), String>,
    ) -> Result<(), String> {
        // Keep the handle and validated snapshot reachable on release failure.
        // Never try this possibly damaged handle again after setting failed.
        if self.failed {
            return Err("GPU backend disabled after an earlier error".into());
        }
        if let Err(error) = release(self.entries[index].resident) {
            self.failed = true;
            return Err(error);
        }
        self.entries.remove(index);
        Ok(())
    }

    /// Keep a handle returned together with a preparation error reachable
    /// (quint-q4 F4). The error is a device fault, so the cache is disabled
    /// by the caller and the handle is never released through that context.
    fn track_prepare(&mut self, prepared: Result<(), String>, resident: *mut c_void) -> Result<(), String> {
        if prepared.is_err() && !resident.is_null() {
            self.retained.push(Retained(resident));
        }
        prepared
    }

    fn run(&mut self, scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, Failure> {
        if self.failed {
            return Err(String::from("GPU backend disabled after an earlier error").into());
        }
        let _prepare = PrepareTimer::start("msm.prepare", scalars.len());
        let address = bases.as_ptr() as usize;
        #[cfg(feature = "cuda-srs-registry")]
        let hit = {
            let mut hit = None;
            for (i, entry) in self.entries.iter().enumerate() {
                if entry.address == address
                    && scalars.len() <= entry.source.len()
                    && same_prefix(&entry.source, &bases[..scalars.len()])
                        .map_err(Failure::rejection)?
                {
                    hit = Some(i);
                    break;
                }
            }
            hit
        };
        #[cfg(not(feature = "cuda-srs-registry"))]
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
            record("msm.prepare.hit", scalars.len(), 0);
            self.entries.remove(index).unwrap()
        } else {
            record("msm.prepare.miss", scalars.len(), 0);
            let validation = PrepareTimer::start("msm.prepare.validation", bases.len());
            #[cfg(feature = "cuda-srs-registry")]
            let affine = validate(bases).map_err(Failure::rejection)?;
            #[cfg(not(feature = "cuda-srs-registry"))]
            let affine = {
                // Legacy validation stays serial: Rayon workers can be waiting
                // for this backend while DEVICE/CACHE are held.
                if bases.iter().any(|base| !bool::from(base.is_on_curve())) {
                    return Err(Failure::new(FailureClass::InvalidInput, "invalid SRS base: off curve"));
                }
                let mut affine = vec![G1Affine::default(); bases.len()];
                G1Projective::batch_normalize(bases, &mut affine);
                if affine
                    .iter()
                    .any(|base| !bool::from(base.is_torsion_free()))
                {
                    return Err(Failure::new(
                        FailureClass::InvalidInput,
                        "invalid SRS base: outside G1 subgroup",
                    ));
                }
                affine
            };
            drop(validation);
            #[cfg(feature = "cuda-srs-registry")]
            let device_bytes = {
                let sm_count = device_sm_count()?;
                let bytes = resident_bytes(bases.len(), sm_count)
                    .map_err(|e| Failure::new(FailureClass::Capacity, e))?;
                let budget =
                    parse_budget(std::env::var("LOKI_GPU_SRS_BUDGET_BYTES").ok().as_deref());
                if bytes > budget {
                    return Err(Failure::new(FailureClass::Capacity, format!(
                        "SRS resident needs {bytes} bytes, budget is {budget}"
                    )));
                }
                // Replace stale capacities at this address, then evict LRU
                // entries until the new persistent allocation fits the budget.
                while let Some(index) = self
                    .entries
                    .iter()
                    .position(|entry| entry.address == address)
                    .or_else(|| {
                        let used: usize = self.entries.iter().map(|entry| entry.device_bytes).sum();
                        (used > budget - bytes).then_some(0)
                    })
                {
                    let capacity = self.entries[index].source.len();
                    self.release_entry(index, |resident| {
                        status(unsafe { loki_gpu_release(resident) })
                    })?;
                    record("msm.prepare.evict", capacity, 0);
                }
                bytes
            };
            #[cfg(not(feature = "cuda-srs-registry"))]
            if self.entries.len() == 2 {
                self.release_entry(0, |resident| status(unsafe { loki_gpu_release(resident) }))?;
            }
            let wire: Vec<_> = affine.iter().map(convert::affine).collect();
            let mut resident = std::ptr::null_mut();
            // SAFETY: live, correctly sized/aligned arrays; C++ copies them.
            let upload = PrepareTimer::start("msm.prepare.upload", wire.len());
            let prepared = status(unsafe { loki_gpu_prepare(&mut resident, wire.as_ptr(), wire.len()) });
            self.track_prepare(prepared, resident)?;
            drop(upload);
            record(
                "msm.prepare.upload_bytes",
                wire.len() * std::mem::size_of::<Affine>(),
                0,
            );
            if resident.is_null() {
                return Err(String::from("GPU preparation returned a null handle").into());
            }
            Entry {
                address,
                source: bases.to_vec(),
                resident,
                #[cfg(feature = "cuda-srs-registry")]
                device_bytes,
            }
        };
        // Keep the entry reachable even when invoking/decoding fails. After a
        // GPU fault we retain handles until process exit rather than invoke
        // sppark destructors on a potentially poisoned context.
        let resident = entry.resident;
        self.entries.push_back(entry);
        drop(_prepare);
        let scalars: Vec<_> = scalars.iter().map(convert::scalar).collect();
        let mut output = Affine::default();
        // SAFETY: resident owns at least scalars.len() bases. Wire structs have
        // repr(C), and C++ asserts their sizes/alignments against kernel types.
        status(unsafe { loki_gpu_msm(resident, &mut output, scalars.as_ptr(), scalars.len()) })?;
        Ok(convert::decode(&output)?)
    }
}

pub(super) fn msm(scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, String> {
    classified_msm(scalars, bases).map_err(|failure| failure.message)
}

/// `msm` with the failure class kept: only device faults disable the cache.
pub(super) fn classified_msm(scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, Failure> {
    // Preserve msm_specific's zip semantics.
    let n = scalars.len().min(bases.len());
    if n == 0 {
        return Ok(G1Projective::default());
    }
    let _device = loki_cuda_runtime::lock()?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| "GPU cache lock poisoned".to_owned())?;
    #[cfg(feature = "cuda-srs-registry")]
    let result = cache.run(&scalars[..n], bases);
    #[cfg(not(feature = "cuda-srs-registry"))]
    let result = cache.run(&scalars[..n], &bases[..n]);
    if result.as_ref().is_err_and(Failure::is_device) {
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
    while !cache.entries.is_empty() {
        cache.release_entry(0, |resident| status(unsafe { loki_gpu_release(resident) }))?;
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
    fn registry_release_failure_retains_entry_and_disables_backend() {
        let mut cache = Cache::default();
        cache.entries.push_back(Entry {
            address: 123,
            source: vec![G1Projective::default()],
            resident: std::ptr::null_mut(),
            #[cfg(feature = "cuda-srs-registry")]
            device_bytes: 1,
        });
        let error = cache
            .release_entry(0, |_| Err("injected release failure".into()))
            .unwrap_err();
        assert_eq!(error, "injected release failure");
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.entries[0].address, 123);
        assert_eq!(cache.entries[0].source.len(), 1);
        assert!(cache.failed);
        assert!(cache
            .release_entry(0, |_| panic!("must never retry a quarantined handle"))
            .unwrap_err()
            .contains("disabled"));
        let disabled = cache.run(&[Fq::ONE], &[G1Projective::default()]).unwrap_err();
        assert!(disabled.message.contains("disabled") && disabled.is_device());
    }

    /// quint-q4 F4: a handle returned with a preparation error stays
    /// reachable; a clean preparation or a null handle records nothing.
    #[test]
    fn prepare_error_with_live_handle_is_retained() {
        let mut cache = Cache::default();
        let live = std::ptr::NonNull::<u8>::dangling().as_ptr().cast::<c_void>();
        assert_eq!(cache.track_prepare(Err("sync failed".into()), live).unwrap_err(), "sync failed");
        assert_eq!(cache.retained.len(), 1);
        assert!(cache.track_prepare(Err("alloc failed".into()), std::ptr::null_mut()).is_err());
        assert!(cache.track_prepare(Ok(()), live).is_ok());
        assert_eq!(cache.retained.len(), 1);
        // The error is a device fault: classified_msm disables the cache.
        assert!(Failure::from(String::from("sync failed")).is_device());
    }

    #[cfg(feature = "cuda-srs-registry")]
    #[test]
    fn registry_workers_join_after_errors_and_panics() {
        use crate::poly::kzg::msm::srs_validate::parallel_chunks;
        let bases = vec![G1Projective::default(); 4096];
        let worker_error = "worker".to_owned();
        let outcome = parallel_chunks(&bases, worker_error, |start, _| -> Result<(), String> {
            if start == 0 {
                return Err("first chunk rejected".into());
            }
            panic!("synthetic worker failure");
        });
        assert_eq!(outcome.unwrap_err(), "first chunk rejected");
    }

    #[cfg(feature = "cuda-srs-registry")]
    #[test]
    fn registry_persistent_byte_accounting() {
        assert_eq!(parse_budget(None), 4_294_967_296);
        assert_eq!(parse_budget(Some("bad")), 4_294_967_296);
        assert_eq!(parse_budget(Some("0")), 0);
        assert_eq!(parse_budget(Some("8388608")), 8_388_608);
        // Literal totals from the pinned constructor, with 170 SMs.
        assert_eq!(resident_bytes(1025, 170).unwrap(), 2_971_648);
        assert_eq!(resident_bytes(1 << 19, 170).unwrap(), 55_409_664);
        assert_eq!(resident_bytes(1 << 20, 170).unwrap(), 109_755_392);
        assert!(resident_bytes(usize::MAX, 170).is_err());
    }

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
        assert!(error.message.contains("invalid SRS base"), "{error}");
        assert_eq!(error.class, FailureClass::InvalidInput, "quint-q2 L2");
        assert_eq!(cache.entries.len(), 0);
        // (0, 2) lies on y^2 = x^3 + 4 but has order 3, outside prime-order G1.
        use midnight_curves::CurveExt;
        let torsion = G1Projective::new_jacobian(Fp::ZERO, Fp::from(2), Fp::ONE).unwrap();
        assert!(bool::from(torsion.is_on_curve()));
        let error = cache.run(&[Fq::ONE], &[torsion]).unwrap_err();
        assert!(error.message.contains("outside G1 subgroup"), "{error}");
        assert_eq!(error.class, FailureClass::InvalidInput);
        assert_eq!(cache.entries.len(), 0);
        assert!(!cache.failed, "run never sets the sticky flag for a request error");
        // Unclassified failures convert conservatively as device faults.
        assert!(Failure::from(String::from("cuda error")).is_device());
    }
}
