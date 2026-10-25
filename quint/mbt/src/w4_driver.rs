// Runs pinned W4 host cache code. FFI is a CPU arithmetic surrogate, never a GPU claim.
use ff::Field;
use group::{Curve, Group};
use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use quint_connect::{Driver, Result, State, Step};
use serde::Deserialize;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static ERROR: AtomicBool = AtomicBool::new(false);
static PREPARE_ERROR: AtomicBool = AtomicBool::new(false);
static PREPARES: AtomicUsize = AtomicUsize::new(0);

#[no_mangle]
unsafe extern "C" fn loki_gpu_prepare(
    out: &mut *mut c_void,
    bases: *const convert::Affine,
    n: usize,
) -> sppark::Error {
    if PREPARE_ERROR.swap(false, Ordering::SeqCst) {
        let mut error: sppark::Error = unsafe { std::mem::zeroed() };
        error.code = 1;
        return error;
    }
    let points: Vec<_> = unsafe { std::slice::from_raw_parts(bases, n) }
        .iter()
        .map(|p| convert::decode(p).unwrap())
        .collect();
    *out = Box::into_raw(Box::new(points)).cast();
    PREPARES.fetch_add(1, Ordering::SeqCst);
    unsafe { std::mem::zeroed() }
}
#[no_mangle]
unsafe extern "C" fn loki_gpu_msm(
    handle: *mut c_void,
    out: &mut convert::Affine,
    scalars: *const convert::Scalar,
    n: usize,
) -> sppark::Error {
    let mut error: sppark::Error = unsafe { std::mem::zeroed() };
    if ERROR.swap(false, Ordering::SeqCst) {
        error.code = 1;
        return error;
    }
    let points = unsafe { &*handle.cast::<Vec<G1Projective>>() };
    let scalars = unsafe { std::slice::from_raw_parts(scalars, n) };
    let result = points
        .iter()
        .zip(scalars)
        .fold(G1Projective::identity(), |sum, (p, s)| {
            sum + *p * Fq::from(blst::blst_fr { l: s.0 })
        });
    *out = convert::affine(&result.to_affine());
    error
}
#[no_mangle]
unsafe extern "C" fn loki_gpu_release(handle: *mut c_void) -> sppark::Error {
    unsafe { drop(Box::from_raw(handle.cast::<Vec<G1Projective>>())) };
    unsafe { std::mem::zeroed() }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum Phase {
    Idle,
    Queued,
    Gpu,
    Fallback,
    Done,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum Op {
    Msm,
    Ntt,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Request {
    phase: Phase,
    op: Op,
    ptr: usize,
    len: usize,
    content: i64,
    resident: i64,
    resident_ptr: i64,
    resident_len: usize,
    result: i64,
    failed: bool,
    hit: bool,
    rejected_stale: bool,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            op: Op::Msm,
            ptr: 0,
            len: 0,
            content: 0,
            resident: -1,
            resident_ptr: -1,
            resident_len: 0,
            result: -1,
            failed: false,
            hit: false,
            rejected_stale: false,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct Cache {
    valid: bool,
    ptr: usize,
    len: usize,
    content: i64,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct Snapshot {
    requests: BTreeMap<usize, Request>,
    cache: Cache,
    memory: BTreeMap<usize, i64>,
}
struct GpuDriver {
    s: Snapshot,
    slots: Vec<Vec<G1Projective>>,
    completed: BTreeMap<usize, G1Projective>,
    _guard: std::sync::MutexGuard<'static, ()>,
}
impl Default for GpuDriver {
    fn default() -> Self {
        Self {
            s: Snapshot::default(),
            slots: vec![],
            completed: BTreeMap::new(),
            _guard: TEST_LOCK.lock().unwrap(),
        }
    }
}
impl State<GpuDriver> for Snapshot {
    fn from_driver(d: &GpuDriver) -> Result<Self> {
        Ok(d.s.clone())
    }
}
impl GpuDriver {
    fn calculate(&mut self, p: usize, force_error: bool) -> G1Projective {
        let r = &self.s.requests[&p];
        assert_eq!(r.op, Op::Msm, "this adapter covers W4 MSM, not W5 NTT");
        for point in &mut self.slots[r.ptr] {
            *point = G1Projective::generator() * Fq::from(r.content as u64);
        }
        let scalars = vec![Fq::ONE; r.len];
        let bases = &self.slots[r.ptr][..r.len];
        let cpu = bases.iter().fold(G1Projective::identity(), |a, p| a + *p);
        let before = PREPARES.load(Ordering::SeqCst);
        ERROR.store(force_error, Ordering::SeqCst);
        let got = dispatch::<G1Affine>(&scalars, bases, 4, backend::msm);
        if force_error {
            assert!(got.is_none());
            assert!(backend::failed());
        } else if let Some(point) = got {
            assert!(point == cpu, "resident cache changed the MSM result");
            if r.hit {
                assert_eq!(
                    before,
                    PREPARES.load(Ordering::SeqCst),
                    "reported hit uploaded again"
                );
            }
        } else {
            // W4's permanent circuit breaker refines the model's allowed fallback.
            assert!(backend::failed());
        }
        let point = got.unwrap_or_else(|| {
            midnight_proofs::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, bases)
        });
        assert!(
            point == cpu,
            "native CPU fallback disagrees with independent group sum"
        );
        point
    }
    fn publish(&mut self, p: usize, point: G1Projective) {
        let r = self.s.requests.get_mut(&p).unwrap();
        let value = r.len as i64 * r.content;
        assert!(point == G1Projective::generator() * Fq::from(value as u64));
        r.result = value;
        r.phase = Phase::Done;
    }
    fn finish_cpu(&mut self, p: usize) {
        let r = &self.s.requests[&p];
        let bases = vec![G1Projective::generator() * Fq::from(r.content as u64); r.len];
        let scalars = vec![Fq::ONE; r.len];
        let point = midnight_proofs::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &bases);
        self.publish(p, point);
    }
}
impl Driver for GpuDriver {
    type State = Snapshot;
    fn step(&mut self, step: &Step) -> Result {
        quint_connect::switch!(step {
            init => {
                backend::mbt_reset(); backend::clear_cache().unwrap(); let _ = backend::gpu_calls();
                assert_eq!(parse_min(Some("4")), 4);
                assert!(try_msm::<G1Affine>(&[], &[]).is_none()); ERROR.store(false, Ordering::SeqCst);
                self.slots = vec![vec![G1Projective::identity(); 5]; 2]; self.completed.clear();
                self.s = Snapshot { requests: (0..2).map(|p| (p, Request::default())).collect(),
                    cache: Cache::default(), memory: [(0, 1), (1, 1)].into() };
            },
            submit(p: usize, op: Op, ptr: usize, len: usize) => {
                self.s.requests.insert(p, Request { phase: Phase::Queued, op, ptr, len,
                    content: self.s.memory[&ptr], ..Request::default() });
            },
            dispatch(p: usize) => {
                let c = self.s.cache.clone(); let r = self.s.requests.get_mut(&p).unwrap();
                let hit = c.valid && c.ptr == r.ptr && c.len == r.len && c.content == r.content;
                r.phase = if r.len < 4 { Phase::Fallback } else { Phase::Gpu };
                r.resident = if r.len < 4 { -1 } else if hit { c.content } else { r.content };
                r.resident_ptr = if hit { c.ptr as i64 } else { r.ptr as i64 };
                r.resident_len = if hit { c.len } else { r.len };
                r.hit = r.len >= 4 && hit;
                r.rejected_stale = r.len >= 4 && c.valid && c.ptr == r.ptr && c.len == r.len && c.content != r.content;
                if r.len >= 4 { self.s.cache = Cache { valid: true, ptr: r.ptr, len: r.len, content: r.content };
                    let point = self.calculate(p, false); self.completed.insert(p, point); }
            },
            gpuSuccess(p: usize) => { let point = self.completed.remove(&p).unwrap(); self.publish(p, point); },
            gpuError(p: usize) => {
                // W4 is synchronous; this probes an invocation failure before publishing.
                self.completed.remove(&p); self.calculate(p, true); let r = self.s.requests.get_mut(&p).unwrap();
                r.result = -1; r.phase = Phase::Fallback; r.failed = true;
            },
            uploadError(p: usize) => {
                if !backend::failed() {
                    backend::clear_cache().unwrap(); PREPARE_ERROR.store(true, Ordering::SeqCst);
                    self.calculate(p, false);
                }
                let r = self.s.requests.get_mut(&p).unwrap(); r.result = -1; r.phase = Phase::Fallback; r.failed = true;
            },
            cpuFinish(p: usize) => {
                self.finish_cpu(p);
            },
            recycle(p: usize) => { self.completed.remove(&p); self.s.requests.insert(p, Request::default()); },
            reusePointer(ptr: usize, content: i64) => { self.s.memory.insert(ptr, content); }
        })
    }
}

#[quint_connect::quint_run(
    spec = "../gpu_dispatch.qnt",
    step = "msmStep",
    max_samples = 24,
    max_steps = 120,
    seed = "20261024"
)]
fn replay_w4_host_cache() -> GpuDriver {
    GpuDriver::default()
}

#[test]
fn concurrent_host_calls_match_cpu() {
    let _guard = TEST_LOCK.lock().unwrap();
    backend::mbt_reset();
    let bases = vec![G1Projective::generator(); 5];
    let scalars = vec![Fq::from(7); 5];
    let cpu = G1Projective::generator() * Fq::from(35);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let bases = &bases;
            let scalars = &scalars;
            scope.spawn(move || {
                let result = dispatch::<G1Affine>(scalars, bases, 4, backend::msm).unwrap();
                assert!(result == cpu);
            });
        }
    });
    backend::clear_cache().unwrap();
}
