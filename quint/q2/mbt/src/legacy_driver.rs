// quint-connect driver: quint/q2/legacy.qnt (`mbtStep`) against the pinned
// a256a9a legacy sppark host code (cuda.rs dispatch, cuda/backend.rs Cache
// with its sticky `failed`, cuda-ntt fft, domain.rs gpu_fft glue) followed by
// the real P1 dispatcher continuation in midnight-proofs (feature
// loki-backend-api, as in `loki-sppark`). Included inside the snapshotted
// `cuda.rs` module so it can reach private `dispatch`, `backend`, `convert`.
//
// FFI symbols are CPU surrogates: loki_gpu_msm computes a naive MSM over the
// prepared bases, loki_ntt runs stock best_fft. No CUDA is linked.
use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

use ff::Field;
use group::Group;
use quint_connect::{Driver, Result, State, Step};
use serde::Deserialize;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static MSM_DEVICE_ERR: AtomicBool = AtomicBool::new(false);
static MSM_DECODE_ERR: AtomicBool = AtomicBool::new(false);
static NTT_DEVICE_ERR: AtomicBool = AtomicBool::new(false);
static MSM_FFI: AtomicU64 = AtomicU64::new(0);
static NTT_FFI: AtomicU64 = AtomicU64::new(0);

fn ok_status() -> sppark::Error {
    unsafe { std::mem::zeroed() }
}
fn err_status() -> sppark::Error {
    let mut e = ok_status();
    e.code = 1;
    e
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_prepare(out: &mut *mut c_void, bases: *const convert::Affine, n: usize) -> sppark::Error {
    let points: Vec<G1Projective> = unsafe { std::slice::from_raw_parts(bases, n) }
        .iter()
        .map(|p| convert::decode(p).unwrap())
        .collect();
    *out = Box::into_raw(Box::new(points)).cast();
    ok_status()
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_msm(
    handle: *mut c_void,
    out: &mut convert::Affine,
    scalars: *const convert::Scalar,
    n: usize,
) -> sppark::Error {
    MSM_FFI.fetch_add(1, Ordering::SeqCst);
    if MSM_DEVICE_ERR.swap(false, Ordering::SeqCst) {
        return err_status();
    }
    let points = unsafe { &*handle.cast::<Vec<G1Projective>>() };
    let scalars = unsafe { std::slice::from_raw_parts(scalars, n) };
    let sum = points.iter().zip(scalars).fold(G1Projective::identity(), |acc, (p, s)| {
        acc + *p * Fq::from(blst::blst_fr { l: s.0 })
    });
    *out = if MSM_DECODE_ERR.swap(false, Ordering::SeqCst) {
        // Non-canonical coordinates: convert::decode must reject them.
        convert::Affine { x: [u64::MAX; 6], y: [u64::MAX; 6] }
    } else {
        convert::affine(&group::Curve::to_affine(&sum))
    };
    ok_status()
}

#[no_mangle]
unsafe extern "C" fn loki_gpu_release(handle: *mut c_void) -> sppark::Error {
    unsafe { drop(Box::from_raw(handle.cast::<Vec<G1Projective>>())) };
    ok_status()
}

#[no_mangle]
unsafe extern "C" fn cudaGetDeviceCount(count: &mut i32) -> i32 {
    *count = 1;
    0
}

#[no_mangle]
unsafe extern "C" fn cudaDeviceGetAttribute(value: &mut i32, _attribute: i32, _device: i32) -> i32 {
    *value = 4;
    0
}

#[no_mangle]
unsafe extern "C" fn loki_ntt(values: *mut blst::blst_fr, log_n: u32, inverse: bool, root: *const blst::blst_fr) -> sppark::Error {
    NTT_FFI.fetch_add(1, Ordering::SeqCst);
    if NTT_DEVICE_ERR.swap(false, Ordering::SeqCst) {
        return err_status();
    }
    let limbs = unsafe { std::slice::from_raw_parts_mut(values, 1usize << log_n) };
    let mut v: Vec<Fq> = limbs.iter().copied().map(Into::into).collect();
    let root: Fq = unsafe { *root }.into();
    midnight_curves::fft::best_fft(&mut v, root, log_n);
    if inverse {
        // sppark inverse NTTs include 1/n.
        let inv = Fq::from(1u64 << log_n).invert().unwrap();
        v.iter_mut().for_each(|x| *x *= inv);
    }
    for (l, x) in limbs.iter_mut().zip(v) {
        *l = x.into();
    }
    ok_status()
}

// ------------------------------------------------------------------ model types

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum MsmErr {
    LNone,
    LDevice,
    LDecode,
    LInvalidBase,
    LBudget,
    LSpawn,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag")]
enum NttErr {
    NNone,
    NDevice,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct P1Ctr {
    msm_cpu: i64,
    msm_fb: i64,
    tr_cpu: i64,
    tr_fb: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    msm_failed: bool,
    gpu_msm_calls: i64,
    gpu_ntt_calls: i64,
    p1: P1Ctr,
    last_result: i64,
    last_oracle: i64,
}

struct LegacyDriver {
    base: midnight_proofs::backend::dispatch::DispatchCounters,
    last_result: i64,
    last_oracle: i64,
    salt: u64,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Default for LegacyDriver {
    fn default() -> Self {
        Self {
            base: Default::default(),
            last_result: 0,
            last_oracle: 0,
            salt: 0,
            _guard: TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner()),
        }
    }
}

fn p1_global() -> midnight_proofs::backend::dispatch::DispatchCounters {
    midnight_proofs::backend::dispatch::global().counters()
}

impl State<LegacyDriver> for Snapshot {
    fn from_driver(d: &LegacyDriver) -> Result<Self> {
        let c = p1_global();
        Ok(Snapshot {
            msm_failed: backend::failed(),
            gpu_msm_calls: MSM_FFI.load(Ordering::SeqCst) as i64,
            gpu_ntt_calls: NTT_FFI.load(Ordering::SeqCst) as i64,
            p1: P1Ctr {
                msm_cpu: (c.msm_cpu - d.base.msm_cpu) as i64,
                msm_fb: (c.msm_fallback - d.base.msm_fallback) as i64,
                tr_cpu: (c.transform_cpu - d.base.transform_cpu) as i64,
                tr_fb: (c.transform_fallback - d.base.transform_fallback) as i64,
            },
            last_result: d.last_result,
            last_oracle: d.last_oracle,
        })
    }
}

fn off_curve() -> G1Projective {
    let raw = blst::blst_p1 {
        x: midnight_curves::Fp::ONE.into(),
        y: midnight_curves::Fp::ONE.into(),
        z: midnight_curves::Fp::ONE.into(),
    };
    // SAFETY: G1Projective is repr(transparent) over blst_p1 (as in backend.rs's own test).
    unsafe { std::mem::transmute(raw) }
}

impl LegacyDriver {
    fn msm_call(&mut self, len: usize, err: MsmErr, x: i64) {
        assert_ne!(err, MsmErr::LSpawn, "not injectable");
        self.salt += 1;
        // Fresh base values per call: a cache hit would skip validation/budget.
        let fresh = matches!(err, MsmErr::LInvalidBase | MsmErr::LBudget);
        let offset = if fresh { 1000 * self.salt } else { 0 };
        let scalars: Vec<Fq> = (0..len).map(|k| Fq::from(x as u64 + k as u64)).collect();
        let mut bases: Vec<G1Projective> =
            (0..len).map(|k| G1Projective::generator() * Fq::from(offset + k as u64 + 1)).collect();
        if err == MsmErr::LInvalidBase && len > 0 {
            bases[0] = off_curve();
        }
        MSM_DEVICE_ERR.store(err == MsmErr::LDevice, Ordering::SeqCst);
        MSM_DECODE_ERR.store(err == MsmErr::LDecode, Ordering::SeqCst);
        if err == MsmErr::LBudget {
            std::env::set_var("LOKI_GPU_SRS_BUDGET_BYTES", "1");
        }
        // msm_specific (src/poly/kzg/msm.rs:169-176): legacy arm, then P1.
        let got = dispatch::<G1Affine>(&scalars, &bases, 4, backend::msm).unwrap_or_else(|| {
            midnight_proofs::poly::kzg::msm::msm_specific::<G1Affine>(&scalars, &bases)
        });
        std::env::remove_var("LOKI_GPU_SRS_BUDGET_BYTES");
        MSM_DEVICE_ERR.store(false, Ordering::SeqCst);
        MSM_DECODE_ERR.store(false, Ordering::SeqCst);
        // Stock CPU oracle: msm_cpu for G1 = blst multi_exp (all scalars nonzero).
        let oracle = if len == 0 { G1Projective::identity() } else { G1Projective::multi_exp(&bases, &scalars) };
        self.last_oracle = 10 * x + 1;
        // Off-curve bases have no CPU oracle: blst multi_exp is order-dependent
        // (non-associative addition off the curve), so two identical stock CPU
        // calls disagree. Only the sticky/counter behaviour is checked then.
        let comparable = err != MsmErr::LInvalidBase || len == 0;
        self.last_result = if !comparable || got == oracle { self.last_oracle } else { -99 };
    }

    fn ntt_call(&mut self, logn: u32, inverse: bool, err: NttErr, x: i64) {
        let domain = midnight_proofs::poly::EvaluationDomain::<Fq>::new(2, logn);
        let values: Vec<Fq> = (0..1u64 << logn).map(|k| Fq::from(x as u64 * 7 + k * k)).collect();
        let mut expect = values.clone();
        let root = if inverse { domain.get_omega_inv() } else { domain.get_omega() };
        midnight_curves::fft::best_fft(&mut expect, root, logn);
        if inverse {
            let inv = Fq::from(1u64 << logn).invert().unwrap();
            expect.iter_mut().for_each(|v| *v *= inv);
        }
        NTT_DEVICE_ERR.store(err == NttErr::NDevice, Ordering::SeqCst);
        // EvaluationDomain::fft / ifft (domain.rs:412-422, 455-465): glue, then P1.
        let mut a = values.clone();
        let got: Vec<Fq> = if crate::legacy::glue::Glue::<Fq>::gpu_fft(&mut a, root, inverse) {
            a
        } else if inverse {
            domain.lagrange_to_coeff(domain.lagrange_from_vec(values)).to_vec()
        } else {
            domain.coeff_to_lagrange(domain.coeff_from_vec(values)).to_vec()
        };
        NTT_DEVICE_ERR.store(false, Ordering::SeqCst);
        self.last_oracle = 10 * x + if inverse { 3 } else { 2 };
        self.last_result = if got == expect { self.last_oracle } else { -99 };
    }
}

fn set_env(name: &str, on: bool) {
    if on {
        std::env::set_var(name, "1");
    } else {
        std::env::remove_var(name);
    }
}

impl Driver for LegacyDriver {
    type State = Snapshot;
    fn step(&mut self, step: &Step) -> Result {
        quint_connect::switch!(step {
            initSppark => { self.reset(); },
            initSpparkRegistry => { self.reset(); },
            msmCall(len: i64, err: MsmErr, x: i64) => { self.msm_call(len as usize, err, x); },
            nttCall(logn: i64, inverse: bool, nerr: NttErr, x: i64) => {
                self.ntt_call(logn as u32, inverse, nerr, x);
            },
            setEnv(force: bool, cpu: bool) => {
                set_env("LOKI_NTT_FORCE_ERROR", force);
                set_env("LOKI_NTT_CPU", cpu);
            }
        })
    }
}

impl LegacyDriver {
    fn reset(&mut self) {
        backend::mbt_reset();
        set_env("LOKI_NTT_FORCE_ERROR", false);
        set_env("LOKI_NTT_CPU", false);
        MSM_FFI.store(0, Ordering::SeqCst);
        NTT_FFI.store(0, Ordering::SeqCst);
        self.base = p1_global();
        self.last_result = 0;
        self.last_oracle = 0;
    }
}

#[cfg(not(feature = "cuda-srs-registry"))]
#[quint_connect::quint_run(
    spec = "../legacy.qnt",
    main = "q2_legacy",
    init = "initSppark",
    step = "mbtStep",
    max_samples = 2000,
    max_steps = 30,
    seed = "20261025"
)]
fn replay_legacy_sppark() -> LegacyDriver {
    LegacyDriver::default()
}

#[cfg(feature = "cuda-srs-registry")]
#[quint_connect::quint_run(
    spec = "../legacy.qnt",
    main = "q2_legacy",
    init = "initSpparkRegistry",
    step = "mbtStep",
    max_samples = 2000,
    max_steps = 30,
    seed = "20261025"
)]
fn replay_legacy_sppark_registry() -> LegacyDriver {
    LegacyDriver::default()
}
