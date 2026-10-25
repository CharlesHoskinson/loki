// quint-q4 replay driver, included unchanged-source-side as `backend::q4_mbt`.
//
// Native boundary. The pinned backend.rs calls `loki_gpu_{prepare,msm,release}`.
// This file defines those symbols as thin counting shims. Without `gpu` the
// shim body is a CPU surrogate with fault injection and a fake CUDA device
// table (cudaGetDeviceCount / cudaDeviceGetAttribute). With `gpu` the shim
// forwards to the real adapter, whose C symbols build.rs renames to
// `*_native`; the C++ code and the Rust under test are otherwise unchanged.
use ff::Field;
use group::Group;
use midnight_curves::Fp;
use quint_connect::{Driver, Result, State, Step};
use serde::Deserialize;
use std::collections::BTreeMap;
#[cfg(not(feature = "gpu"))]
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{Mutex as Lock, MutexGuard};

const BLOCK: usize = 1024;
const CAP_BLOCKS: usize = 5;
static TEST_LOCK: Lock<()> = Lock::new(());

// ---------------------------------------------------------------- model types
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash)]
struct MPoint {
    e: i64,
    r: i64,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum Basis {
    Coef,
    Lag,
    Other,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum Fault {
    NoFault,
    WorkerErr,
    WorkerPanic,
    SpawnErr,
    PrepareCtorErr,
    PrepareSyncErr,
    ReleaseErr,
    InvokeErr,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum WOut {
    WOk,
    WErr,
    WPanic,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum Outcome {
    Idle,
    Empty,
    Disabled,
    GpuOk,
    GpuErr,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
struct Device {
    sm: i64,
    eligible: bool,
    #[serde(rename = "queryOk")]
    query_ok: bool,
}

fn coef(len: usize) -> Vec<MPoint> {
    (0..len).map(|k| MPoint { e: k as i64 + 1, r: 0 }).collect()
}
fn lag(len: usize) -> Vec<MPoint> {
    (0..len).map(|k| MPoint { e: ((k + len) % 6) as i64 + 1, r: 0 }).collect()
}
fn valid(p: MPoint) -> bool {
    p.e != 0 && p.e != 7
}

// Independent transcription of pinned sppark 0.1.16 msm_t's persistent
// allocation (pippenger.cuh:351-375), in the C++ evaluation order.
fn sppark_bytes(np: usize, sm: usize) -> usize {
    fn lg2(mut n: usize) -> usize {
        let mut r = 0;
        loop {
            n >>= 1;
            if n == 0 {
                break r;
            }
            r += 1;
        }
    }
    let npoints = (np + 31) & !31usize;
    let mut wbits = 17;
    if npoints > 192 {
        wbits = (lg2(npoints + npoints / 2) - 8).min(18);
        if wbits < 10 {
            wbits = 10;
        }
    } else if npoints > 0 {
        wbits = 10;
    }
    let nwins = (255 - 1) / wbits + 1;
    let row_sz = 1usize << (wbits - 1);
    let d_buckets_sz = nwins * row_sz + sm * 256 / 32;
    d_buckets_sz * 192 + nwins * row_sz * 4 + npoints * 96
}

// ------------------------------------------------------------- real points
fn raw(x: Fp, y: Fp, z: Fp) -> G1Projective {
    let p = blst::blst_p1 { x: x.into(), y: y.into(), z: z.into() };
    // SAFETY: G1Projective is repr(transparent) over blst_p1 (as in upload_tests).
    unsafe { std::mem::transmute(p) }
}
// Jacobian rescaling (X, Y, Z) -> (l^2 X, l^3 Y, l Z): same group element,
// different limbs.
fn rescale(p: G1Projective, l: u64) -> G1Projective {
    if l == 1 {
        return p;
    }
    let l = Fp::from(l);
    raw(p.x() * l.square(), p.y() * l.square() * l, p.z() * l)
}
fn real_point(p: MPoint) -> G1Projective {
    let l = 1 + p.r as u64;
    match p.e {
        0 => raw(Fp::from(l), Fp::ONE, Fp::ONE),
        7 => rescale(raw(Fp::ZERO, Fp::from(2), Fp::ONE), l),
        e => rescale(G1Projective::generator() * Fq::from(e as u64), l),
    }
}

// --------------------------------------------------------- native tracking
#[derive(Clone, Debug)]
struct Handle {
    ptr: usize,
    live: bool,
    len: usize,
    bytes: usize,
    release_failed: bool,
    #[cfg(not(feature = "gpu"))]
    points: Vec<convert::Affine>,
}
#[derive(Default)]
struct Track {
    handles: Vec<Handle>, // allocation order == model handle id
    prepares: usize,
    msms: usize,
    releases: usize,
    native_calls: usize,
    violations: Vec<String>,
    // Surrogate-only controls.
    devices: Vec<Device>,
    ctor_err: bool,
    sync_err: bool,
    release_err: bool,
    invoke_err: bool,
    query_calls: usize,
}
static TRACK: Lock<Option<Track>> = Lock::new(None);
fn track<R>(f: impl FnOnce(&mut Track) -> R) -> R {
    f(TRACK.lock().unwrap().get_or_insert_with(Track::default))
}
fn live_index(t: &Track, ptr: usize) -> Option<usize> {
    t.handles.iter().rposition(|h| h.ptr == ptr && h.live)
}

#[cfg(not(feature = "gpu"))]
fn code(code: i32) -> sppark::Error {
    sppark::Error { code, message: std::ptr::null_mut() }
}
#[cfg(not(feature = "gpu"))]
const OK: i32 = 0;
// Surrogate error codes (decoded by `category`).
const E_PREPARE_CTOR: i32 = 2;
const E_PREPARE_SYNC: i32 = 3;
const E_RELEASE: i32 = 4;
const E_INVOKE: i32 = 5;
const E_NO_DEVICE: i32 = 100;
const E_VIOLATION: i32 = 99;

#[cfg(feature = "gpu")]
extern "C" {
    fn loki_gpu_prepare_native(out: &mut *mut c_void, bases: *const convert::Affine, n: usize) -> sppark::Error;
    fn loki_gpu_msm_native(
        resident: *mut c_void,
        out: &mut convert::Affine,
        scalars: *const convert::Scalar,
        n: usize,
    ) -> sppark::Error;
    fn loki_gpu_release_native(resident: *mut c_void) -> sppark::Error;
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_prepare(out: &mut *mut c_void, bases: *const convert::Affine, n: usize) -> sppark::Error {
    track(|t| {
        t.native_calls += 1;
        t.prepares += 1;
    });
    #[cfg(feature = "gpu")]
    {
        let status = unsafe { loki_gpu_prepare_native(out, bases, n) };
        let ptr = *out as usize;
        if ptr != 0 {
            track(|t| {
                t.handles.push(Handle { ptr, live: true, len: n, bytes: sppark_bytes(n, 170), release_failed: false })
            });
        }
        status
    }
    #[cfg(not(feature = "gpu"))]
    track(|t| {
        *out = std::ptr::null_mut();
        // sppark select_gpu(0): first device with major >= 7 and cooperative launch.
        let Some(sm) = t.devices.iter().find(|d| d.eligible).map(|d| d.sm as usize) else {
            return code(E_NO_DEVICE);
        };
        if std::mem::take(&mut t.ctor_err) {
            return code(E_PREPARE_CTOR);
        }
        let points = unsafe { std::slice::from_raw_parts(bases, n) }.to_vec();
        let ptr = Box::into_raw(Box::new(0u64)) as usize; // unique, never reused
        t.handles.push(Handle { ptr, live: true, len: n, bytes: sppark_bytes(n, sm), release_failed: false, points });
        *out = ptr as *mut c_void;
        if std::mem::take(&mut t.sync_err) {
            return code(E_PREPARE_SYNC); // msm.cu:31-35: allocated, then sync failed
        }
        code(OK)
    })
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_msm(
    resident: *mut c_void,
    out: &mut convert::Affine,
    scalars: *const convert::Scalar,
    n: usize,
) -> sppark::Error {
    let ok = track(|t| {
        t.native_calls += 1;
        t.msms += 1;
        match live_index(t, resident as usize) {
            None => {
                t.violations.push(format!("msm on unknown or released handle {resident:p}"));
                false
            }
            Some(i) if t.handles[i].release_failed => {
                t.violations.push("msm on a quarantined handle".into());
                false
            }
            Some(i) if n > t.handles[i].len => {
                t.violations.push(format!("msm reads {n} > resident capacity {}", t.handles[i].len));
                false
            }
            Some(_) => true,
        }
    });
    #[cfg(feature = "gpu")]
    {
        if !ok {
            panic!("native contract violation: {:?}", track(|t| t.violations.clone()));
        }
        unsafe { loki_gpu_msm_native(resident, out, scalars, n) }
    }
    #[cfg(not(feature = "gpu"))]
    track(|t| {
        if !ok {
            return code(E_VIOLATION);
        }
        if std::mem::take(&mut t.invoke_err) {
            return code(E_INVOKE);
        }
        let h = &t.handles[live_index(t, resident as usize).unwrap()];
        let scalars = unsafe { std::slice::from_raw_parts(scalars, n) };
        // Group equal (scalar, point) pairs so the surrogate stays cheap; the
        // arithmetic is still sum_i s_i P_i over the resident wire points.
        let mut groups: HashMap<([u64; 4], [u64; 6], [u64; 6]), u64> = HashMap::new();
        for (p, s) in h.points.iter().zip(scalars) {
            *groups.entry((s.0, p.x, p.y)).or_default() += 1;
        }
        let mut sum = G1Projective::identity();
        for ((s, x, y), count) in groups {
            let point = convert::decode(&convert::Affine { x, y }).expect("resident wire point decodes");
            sum += point * (Fq::from(blst::blst_fr { l: s }) * Fq::from(count));
        }
        *out = convert::affine(&sum.to_affine());
        code(OK)
    })
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_release(resident: *mut c_void) -> sppark::Error {
    let index = track(|t| {
        t.native_calls += 1;
        match live_index(t, resident as usize) {
            Some(i) if t.handles[i].release_failed => {
                t.violations.push("release retried on a quarantined handle".into());
                None
            }
            Some(i) => Some(i),
            None => {
                t.violations.push(format!("double release or unknown handle {resident:p}"));
                None
            }
        }
    });
    #[cfg(feature = "gpu")]
    {
        let Some(i) = index else {
            panic!("native contract violation: {:?}", track(|t| t.violations.clone()));
        };
        let status = unsafe { loki_gpu_release_native(resident) };
        track(|t| {
            if status.code == 0 {
                t.handles[i].live = false;
                t.releases += 1;
            } else {
                t.handles[i].release_failed = true;
            }
        });
        status
    }
    #[cfg(not(feature = "gpu"))]
    track(|t| {
        let Some(i) = index else { return code(E_VIOLATION) };
        if std::mem::take(&mut t.release_err) {
            t.handles[i].release_failed = true;
            return code(E_RELEASE);
        }
        t.handles[i].live = false;
        t.releases += 1;
        code(OK)
    })
}

// Fake CUDA runtime queries (surrogate only); attribute 16 is
// cudaDevAttrMultiProcessorCount in CUDA 13.4.3 driver_types.h:2079.
#[cfg(not(feature = "gpu"))]
#[no_mangle]
extern "C" fn cudaGetDeviceCount(count: &mut i32) -> i32 {
    track(|t| {
        t.query_calls += 1;
        *count = t.devices.len() as i32;
        0
    })
}
#[cfg(not(feature = "gpu"))]
#[no_mangle]
extern "C" fn cudaDeviceGetAttribute(value: &mut i32, attribute: i32, device: i32) -> i32 {
    track(|t| {
        t.query_calls += 1;
        if attribute != 16 {
            t.violations.push(format!("unexpected CUDA attribute {attribute}"));
            return 1;
        }
        let d = t.devices[device as usize];
        if !d.query_ok {
            return 1;
        }
        *value = d.sm as i32;
        0
    })
}

// ------------------------------------------------------------- state compare
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct SEntry {
    addr: i64,
    len: i64,
    handle: i64,
    bytes: i64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct SNative {
    live: bool,
    bytes: i64,
    len: i64,
    #[serde(rename = "releaseFailed")]
    release_failed: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct SOp {
    outcome: Outcome,
    n: i64,
    hit: bool,
    evictions: i64,
    error: String,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct SPar {
    spawned: i64,
    joined: i64,
    err: i64,
    unwound: bool,
}
const NO_PAR: SPar = SPar { spawned: 0, joined: 0, err: -1, unwound: false };
const IDLE: SOp = SOp { outcome: Outcome::Idle, n: 0, hit: false, evictions: 0, error: String::new() };

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Snap {
    entries: Vec<SEntry>,
    failed: bool,
    #[serde(rename = "lastOp")]
    last_op: SOp,
    #[serde(rename = "lastPar")]
    last_par: SPar,
    // Native ground truth is only observable through the CPU surrogate.
    #[cfg(not(feature = "gpu"))]
    native: BTreeMap<i64, SNative>,
    #[cfg(not(feature = "gpu"))]
    #[serde(rename = "nativeAfterFail")]
    native_after_fail: i64,
}

struct W13 {
    slots: Vec<Vec<G1Projective>>,
    mem: Vec<Vec<MPoint>>,
    last_op: SOp,
    last_par: SPar,
    native_after_fail: i64,
    steps: usize,
    hits: usize,
    misses: usize,
    events: BTreeMap<String, usize>,
    _guard: MutexGuard<'static, ()>,
}

impl State<W13> for Snap {
    // Instance modules qualify state names (`w13_registry::srs_w13::entries`).
    fn from_spec(value: itf::Value) -> Result<Self> {
        let itf::Value::Record(record) = value else { anyhow::bail!("spec state is not a record") };
        let mut plain = itf::value::Record::new();
        for (key, v) in record.iter() {
            plain.insert(key.rsplit("::").next().unwrap().to_owned(), v.clone());
        }
        Ok(Self::deserialize(itf::Value::Record(plain))?)
    }
    fn from_driver(d: &W13) -> Result<Self> {
        let cache = CACHE.lock().unwrap();
        let t = TRACK.lock().unwrap();
        let t = t.as_ref().unwrap();
        let entries = cache
            .entries
            .iter()
            .map(|e| SEntry {
                addr: d.slots.iter().position(|s| s.as_ptr() as usize == e.address).expect("known address") as i64,
                len: e.source.len() as i64,
                handle: live_index(t, e.resident as usize)
                    .or_else(|| t.handles.iter().rposition(|h| h.ptr == e.resident as usize))
                    .map_or(-1, |i| i as i64),
                #[cfg(feature = "cuda-srs-registry")]
                bytes: e.device_bytes as i64,
                #[cfg(not(feature = "cuda-srs-registry"))]
                bytes: 0,
            })
            .collect();
        Ok(Snap {
            entries,
            failed: cache.failed,
            last_op: d.last_op.clone(),
            last_par: d.last_par.clone(),
            #[cfg(not(feature = "gpu"))]
            native: t
                .handles
                .iter()
                .enumerate()
                .map(|(i, h)| {
                    (i as i64, SNative { live: h.live, bytes: h.bytes as i64, len: h.len as i64, release_failed: h.release_failed })
                })
                .collect(),
            #[cfg(not(feature = "gpu"))]
            native_after_fail: d.native_after_fail,
        })
    }
}

fn category(error: &str) -> &'static str {
    if error.contains("invalid SRS base") {
        "invalid"
    } else if error.contains("query failed") {
        "query"
    } else if error.contains("budget is") {
        "budget"
    } else if error.contains("SRS worker") {
        "worker"
    } else if error.ends_with(&format!(" {E_RELEASE}")) {
        "release"
    } else if [E_PREPARE_CTOR, E_PREPARE_SYNC, E_NO_DEVICE].iter().any(|c| error.ends_with(&format!(" {c}")))
        || error.contains("null handle")
    {
        "prepare"
    } else if error.ends_with(&format!(" {E_INVOKE}")) {
        "invoke"
    } else {
        "other"
    }
}

impl W13 {
    fn new() -> Self {
        let guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        Self {
            slots: vec![Vec::with_capacity(CAP_BLOCKS * BLOCK), Vec::with_capacity(CAP_BLOCKS * BLOCK)],
            mem: vec![vec![], vec![]],
            last_op: IDLE,
            last_par: NO_PAR,
            native_after_fail: 0,
            steps: 0,
            hits: 0,
            misses: 0,
            events: BTreeMap::new(),
            _guard: guard,
        }
    }
    fn reset(&mut self, devices: Vec<Device>, budget: i64) {
        {
            // Test-only: forget resident handles without native calls; the
            // tracker is reset with them. Production code is untouched.
            let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
            cache.entries.clear();
            cache.failed = false;
        }
        *TRACK.lock().unwrap() = Some(Track { devices, ..Track::default() });
        std::env::set_var("LOKI_GPU_SRS_BUDGET_BYTES", budget.to_string());
        self.mem = vec![coef(4), lag(4)];
        for a in 0..2 {
            self.write_slot(a);
        }
        self.last_op = IDLE;
        self.last_par = NO_PAR;
        self.native_after_fail = 0;
    }
    fn write_slot(&mut self, a: usize) {
        let before = self.slots[a].as_ptr();
        let blocks: Vec<G1Projective> = self.mem[a].iter().map(|p| real_point(*p)).collect();
        let slot = &mut self.slots[a];
        slot.clear();
        for p in blocks {
            slot.extend(std::iter::repeat(p).take(BLOCK));
        }
        assert_eq!(before, slot.as_ptr(), "slot address must stay stable");
    }
    fn expected(&self, a: usize, n: usize) -> Option<G1Projective> {
        let mut total = 0u64;
        for i in 0..n {
            let p = self.mem[a][i / BLOCK];
            if !valid(p) {
                return None;
            }
            total += (1 + (i % 3) as u64) * p.e as u64;
        }
        Some(G1Projective::generator() * Fq::from(total))
    }
    fn call(&mut self, a: usize, nb: i64, trim: i64, f: Fault) {
        track(|t| {
            t.ctor_err = f == Fault::PrepareCtorErr;
            t.sync_err = f == Fault::PrepareSyncErr;
            t.release_err = f == Fault::ReleaseErr;
            t.invoke_err = f == Fault::InvokeErr;
        });
        let len = self.mem[a].len() * BLOCK;
        let ns = (nb.max(0) as usize * BLOCK).saturating_sub(trim as usize);
        let scalars: Vec<Fq> = (0..ns).map(|i| Fq::from(1 + (i % 3) as u64)).collect();
        let was_failed = CACHE.lock().unwrap().failed;
        let addr = self.slots[a].as_ptr() as usize;
        let n_req = ns.min(len);
        let candidate = CACHE.lock().unwrap().entries.iter().any(|e| {
            e.address == addr
                && if cfg!(feature = "cuda-srs-registry") { n_req <= e.source.len() } else { n_req == e.source.len() }
        });
        let before = track(|t| (t.prepares, t.msms, t.releases, t.native_calls));
        let result = msm(&scalars, &self.slots[a][..len]);
        let after = track(|t| {
            t.ctor_err = false;
            t.sync_err = false;
            t.release_err = false;
            t.invoke_err = false;
            (t.prepares, t.msms, t.releases, t.native_calls)
        });
        let violations = track(|t| std::mem::take(&mut t.violations));
        assert!(violations.is_empty(), "native contract violations: {violations:?}");
        let n = ns.min(len);
        let outcome = match &result {
            Ok(_) if n == 0 => Outcome::Empty,
            Ok(_) => Outcome::GpuOk,
            Err(e) if e.contains("disabled after an earlier error") => Outcome::Disabled,
            Err(_) => Outcome::GpuErr,
        };
        if let Ok(point) = &result {
            if n == 0 {
                assert!(bool::from(point.is_identity()));
            } else {
                let want = self.expected(a, n).expect("GPU result computed over invalid bases");
                assert!(*point == want, "GPU path result differs from the requested bases (addr {a}, n {n})");
            }
        }
        if was_failed && after.3 != before.3 {
            self.native_after_fail += 1;
        }
        let hit = after.1 > before.1 && after.0 == before.0;
        if outcome == Outcome::GpuOk {
            if hit {
                self.hits += 1
            } else {
                self.misses += 1
            }
        }
        let mut note = |k: String| *self.events.entry(k).or_default() += 1;
        note(format!("{outcome:?}"));
        if let Err(e) = &result {
            note(format!("err:{}", category(e)));
        }
        if after.2 > before.2 {
            note("evict".into());
        }
        if outcome == Outcome::GpuOk && !hit && candidate {
            note("miss:same-address-stale".into());
        }
        if outcome == Outcome::GpuOk && hit && (n as i64) % (BLOCK as i64) == BLOCK as i64 - 1 {
            note("hit:n-1".into());
        }
        if outcome == Outcome::GpuOk && hit && CACHE.lock().unwrap().entries.back().is_some_and(|e| n < e.source.len()) {
            note("hit:prefix".into());
        }
        self.last_op = SOp {
            outcome,
            n: n as i64,
            hit,
            evictions: (after.2 - before.2) as i64,
            error: match (&result, outcome) {
                (Err(e), Outcome::GpuErr) => category(e).into(),
                _ => String::new(),
            },
        };
    }
    #[cfg(feature = "cuda-srs-registry")]
    fn par_probe(&mut self, plen: usize, outs: Vec<WOut>) {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let bases = vec![G1Projective::default(); plen];
        let chunk = plen.div_ceil(4);
        // Model: workerCount(plen) outcomes; the serial path (< 4096) has none.
        let workers = if plen < 4096 { 0 } else { plen.div_ceil(chunk) };
        let outs: Vec<WOut> = outs.into_iter().take(workers).collect();
        let spawned = AtomicUsize::new(0);
        let finished = AtomicUsize::new(0);
        struct Done<'a>(&'a AtomicUsize);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, SeqCst);
            }
        }
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parallel_chunks(&bases, |start, _chunk| -> std::result::Result<(), String> {
                let on_worker = std::thread::current().name() == Some("loki-srs");
                let _done = on_worker.then(|| Done(&finished));
                if on_worker {
                    spawned.fetch_add(1, SeqCst);
                }
                match outs.get(start / chunk).copied().unwrap_or(WOut::WOk) {
                    WOut::WOk => Ok(()),
                    WOut::WErr => Err(format!("chunk {}", start / chunk)),
                    WOut::WPanic => panic!("synthetic worker panic"),
                }
            })
        }));
        std::panic::set_hook(hook);
        let err = match &result {
            Ok(Ok(_)) | Err(_) => -1,
            Ok(Err(m)) if m.starts_with("chunk ") => m[6..].parse().unwrap(),
            Ok(Err(m)) if m == "SRS worker panicked" => outs.iter().position(|o| *o == WOut::WPanic).unwrap() as i64,
            Ok(Err(_)) => -2,
        };
        *self.events.entry(format!("par:spawned={} err={} unwound={}", spawned.load(SeqCst), err, result.is_err())).or_default() += 1;
        self.last_par = SPar {
            spawned: spawned.load(SeqCst) as i64,
            joined: finished.load(SeqCst) as i64,
            err,
            unwound: result.is_err(),
        };
    }
}

impl Driver for W13 {
    type State = Snap;
    fn step(&mut self, step: &Step) -> Result {
        self.steps += 1;
        quint_connect::switch!(step {
            init(devs: Vec<Device>, bud: i64) => { self.reset(devs, bud); },
            initGpu(bud: i64) => {
                self.reset(vec![Device { sm: 170, eligible: true, query_ok: true }], bud);
            },
            writeArray(addr: usize, basis: Basis, len: usize) => {
                self.mem[addr] = if basis == Basis::Coef { coef(len) } else { lag(len) };
                self.write_slot(addr);
                self.last_op = IDLE;
            },
            mutate(addr: usize, i: usize, p: MPoint) => {
                self.mem[addr][i] = p;
                let rp = real_point(p);
                self.slots[addr][i * BLOCK..(i + 1) * BLOCK].fill(rp);
                self.last_op = IDLE;
            },
            msmCall(addr: usize, nb: i64, trim: i64) => { self.call(addr, nb, trim, Fault::NoFault); },
            msmFault(addr: usize, nb: i64, trim: i64, f: Fault) => { self.call(addr, nb, trim, f); },
            parProbe(plen: usize, o0: WOut, o1: WOut, o2: WOut, o3: WOut, o4: WOut) => {
                #[cfg(feature = "cuda-srs-registry")]
                self.par_probe(plen, vec![o0, o1, o2, o3, o4]);
                #[cfg(not(feature = "cuda-srs-registry"))]
                { let _ = (plen, o0, o1, o2, o3, o4); unreachable!("legacy traces exclude parProbe"); }
                self.last_op = IDLE;
            }
        })
    }
}

impl Drop for W13 {
    fn drop(&mut self) {
        println!("q4 replay: {} steps, {} GPU-path hits, {} GPU-path misses", self.steps, self.hits, self.misses);
        if self.steps > 0 {
            println!("q4 replay events: {:?}", self.events);
        }
    }
}

// ------------------------------------------------------------- MBT entries
#[cfg(all(not(feature = "gpu"), feature = "cuda-srs-registry"))]
#[quint_connect::quint_run(
    spec = "../../q4/srs_w13.qnt",
    main = "w13_registry",
    step = "stepMbt",
    max_samples = 200,
    max_steps = 24,
    seed = "20261025"
)]
fn replay_w13_registry_cpu_surrogate() -> W13 {
    W13::new()
}

#[cfg(all(not(feature = "gpu"), not(feature = "cuda-srs-registry")))]
#[quint_connect::quint_run(
    spec = "../../q4/srs_w13.qnt",
    main = "w13_legacy",
    step = "stepMbtLegacy",
    max_samples = 200,
    max_steps = 24,
    seed = "20261025"
)]
fn replay_w13_legacy_cpu_surrogate() -> W13 {
    W13::new()
}

#[cfg(all(feature = "gpu", feature = "cuda-srs-registry"))]
#[quint_connect::quint_run(
    spec = "../../q4/srs_w13.qnt",
    main = "w13_registry",
    init = "initGpu",
    step = "stepGpu",
    max_samples = 12,
    max_steps = 20,
    seed = "20261025"
)]
fn replay_w13_registry_gpu() -> W13 {
    W13::new()
}

// ------------------------------------------------------------- directed checks
#[test]
fn q4_point_encoding_matches_model() {
    for e in 0..8 {
        for r in 0..2 {
            let p = MPoint { e, r };
            let q = real_point(p);
            assert_eq!(bool::from(q.is_on_curve()), e != 0, "{p:?}");
            if e == 7 {
                assert!(!bool::from(q.to_affine().is_torsion_free()));
            }
            if valid(p) {
                assert!(q == real_point(MPoint { e, r: 0 }), "rescaling keeps the group element");
                assert!(q == G1Projective::generator() * Fq::from(e as u64));
            }
            if r == 1 {
                assert!(!same_coordinates(&q, &real_point(MPoint { e, r: 0 })), "rescaling changes the limbs");
            }
        }
    }
}

#[cfg(feature = "cuda-srs-registry")]
#[test]
fn q4_sppark_formula_matches_backend() {
    for n in [1, 31, 32, 33, 192, 193, 1024, 4095, 4096, 5120, 1 << 17, (1 << 19) - 1, 1 << 19, 1 << 20, 1 << 22] {
        for sm in [1, 28, 84, 170, 200] {
            assert_eq!(resident_bytes(n, sm).unwrap(), sppark_bytes(n, sm), "n={n} sm={sm}");
        }
    }
}

// F2 (legacy): a failed eviction release discards the entry (backend.rs:351-353),
// so the possibly-live handle is no longer reachable/quarantined, unlike the
// registry's release_entry (backend.rs:247-263). Run with --ignored to see it fail.
#[cfg(all(not(feature = "gpu"), not(feature = "cuda-srs-registry")))]
#[test]
#[ignore = "repro for REPORT.md F2; expected to fail"]
fn q4_legacy_release_failure_quarantines_handle() {
    let mut d = W13::new();
    d.reset(vec![Device { sm: 170, eligible: true, query_ok: true }], 4 << 30);
    d.call(0, 1, 0, Fault::NoFault);
    d.call(1, 1, 0, Fault::NoFault);
    d.call(0, 2, 0, Fault::ReleaseErr);
    let t = TRACK.lock().unwrap();
    let t = t.as_ref().unwrap();
    let cache = CACHE.lock().unwrap();
    for h in t.handles.iter().filter(|h| h.release_failed) {
        assert!(
            cache.entries.iter().any(|e| e.resident as usize == h.ptr),
            "handle whose release failed was dropped from the cache (not quarantined)"
        );
    }
}

// F3 (registry, mixed hosts): the charge uses the largest SM count over all
// CUDA ordinals, sppark allocates on the first eligible device. Conservative
// but not exact; and a failing query on a device sppark ignores disables the
// registry although sppark itself would succeed.
#[cfg(all(not(feature = "gpu"), feature = "cuda-srs-registry"))]
#[test]
fn q4_mixed_host_accounting() {
    let mut d = W13::new();
    d.reset(
        vec![Device { sm: 200, eligible: false, query_ok: true }, Device { sm: 84, eligible: true, query_ok: true }],
        4 << 30,
    );
    d.call(0, 4, 0, Fault::NoFault);
    assert_eq!(d.last_op.outcome, Outcome::GpuOk);
    let charged = CACHE.lock().unwrap().entries[0].device_bytes;
    let actual = track(|t| t.handles[0].bytes);
    assert_eq!(charged, sppark_bytes(4 * BLOCK, 200));
    assert_eq!(actual, sppark_bytes(4 * BLOCK, 84));
    assert!(charged > actual, "overcharge {charged} vs {actual}");
    println!("mixed host: charged {charged} bytes, sppark allocates {actual} (+{})", charged - actual);

    d.reset(
        vec![Device { sm: 170, eligible: true, query_ok: true }, Device { sm: 1, eligible: false, query_ok: false }],
        4 << 30,
    );
    d.call(0, 4, 0, Fault::NoFault);
    assert_eq!(d.last_op.error, "query");
    assert!(CACHE.lock().unwrap().failed, "registry disabled by an ineligible device's query");
}

// F4: msm.cu:31-35 can return an error after allocating (sync failure). The
// handle is dropped by backend.rs:359 `?` without release: a live allocation
// that no cache entry or budget accounts for. The backend is disabled after.
#[cfg(not(feature = "gpu"))]
#[test]
fn q4_prepare_sync_error_leaks_untracked_allocation() {
    let mut d = W13::new();
    d.reset(vec![Device { sm: 170, eligible: true, query_ok: true }], 4 << 30);
    d.call(0, 4, 0, Fault::PrepareSyncErr);
    assert_eq!(d.last_op.error, "prepare");
    let cache = CACHE.lock().unwrap();
    assert!(cache.failed && cache.entries.is_empty());
    assert!(track(|t| t.handles.len() == 1 && t.handles[0].live), "allocated handle is live but untracked");
}

// Rewarm, then mutate in place at the same address: the next consuming call
// must miss and re-upload; a representation-only change (same group element,
// new limbs) misses conservatively; a change confined to the unused suffix
// still hits. Runs on the CPU surrogate and, with `gpu`, on the device.
#[cfg(feature = "cuda-srs-registry")]
#[test]
fn q4_rewarm_then_mutate_same_address_misses() {
    let mut d = W13::new();
    d.reset(vec![Device { sm: 170, eligible: true, query_ok: true }], 4 << 30);
    let mut set = |d: &mut W13, i: usize, p: MPoint| {
        d.mem[0][i] = p;
        let rp = real_point(p);
        d.slots[0][i * BLOCK..(i + 1) * BLOCK].fill(rp);
    };
    d.call(0, 4, 0, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && !d.last_op.hit);
    d.call(0, 4, 1, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && d.last_op.hit, "n-1 prefix rewarm hits");
    set(&mut d, 3, MPoint { e: 6, r: 0 });
    d.call(0, 4, 1, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && !d.last_op.hit, "same-address mutation must miss");
    set(&mut d, 0, MPoint { e: 1, r: 1 });
    d.call(0, 4, 0, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && !d.last_op.hit, "new limbs, same element: conservative miss");
    set(&mut d, 3, MPoint { e: 5, r: 0 });
    d.call(0, 3, 0, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && d.last_op.hit, "unused-suffix change keeps the prefix hit");
    d.call(0, 4, 0, Fault::NoFault);
    assert!(d.last_op.outcome == Outcome::GpuOk && !d.last_op.hit, "consuming the changed suffix misses");
    assert_eq!(CACHE.lock().unwrap().entries.len(), 1, "stale capacity at the address was replaced");
}
