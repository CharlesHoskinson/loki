//! Serialized access to sppark streams and bounded resident SRS storage.
#![allow(unsafe_code)]

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use std::{collections::VecDeque, ffi::c_void, sync::Mutex};

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

// Never enter Rayon while holding DEVICE/CACHE: even install on a private
// pool can execute jobs from a calling Rayon worker's pool while it waits.
// Scoped native workers only read CPU data, and join without scheduler reentry.
#[cfg(feature = "cuda-srs-registry")]
fn parallel_chunks<R: Send>(
    bases: &[G1Projective],
    f: impl Fn(usize, &[G1Projective]) -> Result<R, String> + Sync,
) -> Result<Vec<R>, String> {
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
                Err(cause) => {
                    error = Some(format!("SRS worker creation failed: {cause}"));
                    break;
                }
            }
        }
        let mut results = Vec::with_capacity(jobs.len());
        // Join every worker, even after an error or panic. Letting Scope
        // implicitly join a second panicking worker would unwind the caller.
        for job in jobs {
            match job.join() {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(cause)) => {
                    error.get_or_insert(cause);
                }
                Err(_) => {
                    error.get_or_insert_with(|| "SRS worker panicked".into());
                }
            }
        }
        error.map_or(Ok(results), Err)
    })
}

#[cfg(feature = "cuda-srs-registry")]
fn same_prefix(source: &[G1Projective], bases: &[G1Projective]) -> Result<bool, String> {
    let matches = parallel_chunks(bases, |start, chunk| {
        Ok(source[start..start + chunk.len()]
            .iter()
            .zip(chunk)
            .all(|(a, b)| same_coordinates(a, b)))
    })?;
    Ok(matches.into_iter().all(|equal| equal))
}

#[cfg(feature = "cuda-srs-registry")]
fn validate(bases: &[G1Projective]) -> Result<Vec<G1Affine>, String> {
    let chunks = parallel_chunks(bases, |_, chunk| {
        if chunk.iter().any(|base| !bool::from(base.is_on_curve())) {
            return Err("invalid SRS base: off curve".into());
        }
        let mut affine = vec![G1Affine::default(); chunk.len()];
        // Pinned BLST dispatches batches >=768 to its own global pool unless
        // no-threads is enabled. Keep normalization serial inside each native
        // worker even when loki-cpu-budget is off (without changing its feature).
        for (points, output) in chunk.chunks(512).zip(affine.chunks_mut(512)) {
            G1Projective::batch_normalize(points, output);
        }
        if affine
            .iter()
            .any(|base| !bool::from(base.is_torsion_free()))
        {
            return Err("invalid SRS base: outside G1 subgroup".into());
        }
        Ok(affine)
    })?;
    Ok(chunks.into_iter().flatten().collect())
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
    #[cfg(feature = "cuda-srs-registry")]
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

    fn run(&mut self, scalars: &[Fq], bases: &[G1Projective]) -> Result<G1Projective, String> {
        if self.failed {
            return Err("GPU backend disabled after an earlier error".into());
        }
        let _prepare = PrepareTimer::start("msm.prepare", scalars.len());
        let address = bases.as_ptr() as usize;
        #[cfg(feature = "cuda-srs-registry")]
        let hit = {
            let mut hit = None;
            for (i, entry) in self.entries.iter().enumerate() {
                if entry.address == address
                    && scalars.len() <= entry.source.len()
                    && same_prefix(&entry.source, &bases[..scalars.len()])?
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
            let affine = validate(bases)?;
            #[cfg(not(feature = "cuda-srs-registry"))]
            let affine = {
                // Legacy validation stays serial: Rayon workers can be waiting
                // for this backend while DEVICE/CACHE are held.
                if bases.iter().any(|base| !bool::from(base.is_on_curve())) {
                    return Err("invalid SRS base: off curve".into());
                }
                let mut affine = vec![G1Affine::default(); bases.len()];
                G1Projective::batch_normalize(bases, &mut affine);
                if affine
                    .iter()
                    .any(|base| !bool::from(base.is_torsion_free()))
                {
                    return Err("invalid SRS base: outside G1 subgroup".into());
                }
                affine
            };
            drop(validation);
            #[cfg(feature = "cuda-srs-registry")]
            let device_bytes = {
                let bytes = resident_bytes(bases.len(), device_sm_count()?)?;
                let budget =
                    parse_budget(std::env::var("LOKI_GPU_SRS_BUDGET_BYTES").ok().as_deref());
                if bytes > budget {
                    return Err(format!(
                        "SRS resident needs {bytes} bytes, budget is {budget}"
                    ));
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
                let old = self.entries.pop_front().unwrap();
                status(unsafe { loki_gpu_release(old.resident) })?;
            }
            let wire: Vec<_> = affine.iter().map(convert::affine).collect();
            let mut resident = std::ptr::null_mut();
            // SAFETY: live, correctly sized/aligned arrays; C++ copies them.
            let upload = PrepareTimer::start("msm.prepare.upload", wire.len());
            status(unsafe { loki_gpu_prepare(&mut resident, wire.as_ptr(), wire.len()) })?;
            drop(upload);
            record(
                "msm.prepare.upload_bytes",
                wire.len() * std::mem::size_of::<Affine>(),
                0,
            );
            if resident.is_null() {
                return Err("GPU preparation returned a null handle".into());
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
    #[cfg(feature = "cuda-srs-registry")]
    let result = cache.run(&scalars[..n], bases);
    #[cfg(not(feature = "cuda-srs-registry"))]
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
    #[cfg(feature = "cuda-srs-registry")]
    while !cache.entries.is_empty() {
        cache.release_entry(0, |resident| status(unsafe { loki_gpu_release(resident) }))?;
    }
    #[cfg(not(feature = "cuda-srs-registry"))]
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

    #[cfg(feature = "cuda-srs-registry")]
    #[test]
    fn registry_release_failure_retains_entry_and_disables_backend() {
        let mut cache = Cache::default();
        cache.entries.push_back(Entry {
            address: 123,
            source: vec![G1Projective::default()],
            resident: std::ptr::null_mut(),
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
        assert!(cache
            .run(&[Fq::ONE], &[G1Projective::default()])
            .unwrap_err()
            .contains("disabled"));
    }

    #[cfg(feature = "cuda-srs-registry")]
    #[test]
    fn registry_workers_join_after_errors_and_panics() {
        let bases = vec![G1Projective::default(); 4096];
        let outcome = parallel_chunks(&bases, |start, _| -> Result<(), String> {
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
