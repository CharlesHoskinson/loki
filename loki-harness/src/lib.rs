//! Differential compatibility gate against unmodified midnight-proofs 0.8.3.

mod backend;

use serde::Serialize;
use std::{error::Error, io, sync::Mutex};

// Cargo's default test runner can invoke several size cases at once.
static PROOF_LOCK: Mutex<()> = Mutex::new(());

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    None,
    Gate,
    Permutation,
    Lookup,
    Phase,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub k: u32,
    pub keygen_s: f64,
    pub prove_s: f64,
    pub verify_s: f64,
    pub proof_bytes: usize,
}

struct Sample {
    vk: Vec<u8>,
    verifier_params: Vec<u8>,
    proof: Vec<u8>,
    metrics: Metrics,
}

#[derive(Debug, Serialize)]
pub struct Difference {
    pub equal: bool,
    pub loki_bytes: usize,
    pub stock_bytes: usize,
    pub first_difference: Option<usize>,
    pub differing_bytes: usize,
    pub loki_byte: Option<u8>,
    pub stock_byte: Option<u8>,
}

impl Difference {
    pub fn between(loki: &[u8], stock: &[u8]) -> Self {
        let first_difference = loki
            .iter()
            .zip(stock)
            .position(|(a, b)| a != b)
            .or_else(|| (loki.len() != stock.len()).then_some(loki.len().min(stock.len())));
        Self {
            equal: loki == stock,
            loki_bytes: loki.len(),
            stock_bytes: stock.len(),
            differing_bytes: loki.iter().zip(stock).filter(|(a, b)| a != b).count()
                + loki.len().abs_diff(stock.len()),
            loki_byte: first_difference.and_then(|i| loki.get(i).copied()),
            stock_byte: first_difference.and_then(|i| stock.get(i).copied()),
            first_difference,
        }
    }
}

/// Whether this build enables any Loki accelerator feature.
const ACCELERATED: bool = cfg!(any(
    feature = "cuda",
    feature = "cuda-ntt",
    feature = "loki-sppark",
    feature = "loki-gpu-backend"
));

/// Selected Loki backend and its dispatch counters over one Loki proof.
#[derive(Debug, Serialize)]
pub struct BackendReport {
    /// `stock-cpu` for a build without accelerator features, otherwise
    /// Loki's compile-time backend identity.
    pub selected: String,
    /// MSM attempted/succeeded/fallback/error counters (accelerated builds).
    pub msm: Option<OpCounters>,
    /// NTT attempted/succeeded/fallback/error counters (accelerated builds).
    pub ntt: Option<OpCounters>,
}

/// Mirror of Loki's per-operation telemetry counters.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct OpCounters {
    pub calls: u64,
    pub cpu: u64,
    pub attempted: u64,
    pub succeeded: u64,
    pub error: u64,
    pub fallback: u64,
}

#[cfg(any(
    feature = "cuda",
    feature = "cuda-ntt",
    feature = "loki-sppark",
    feature = "loki-gpu-backend"
))]
mod telemetry {
    use super::{BackendReport, OpCounters};
    pub use loki::telemetry::Snapshot;

    fn mirror(c: loki::telemetry::OpCounters) -> OpCounters {
        OpCounters {
            calls: c.calls,
            cpu: c.cpu,
            attempted: c.attempted,
            succeeded: c.succeeded,
            error: c.error,
            fallback: c.fallback,
        }
    }

    pub fn snapshot() -> Snapshot {
        loki::telemetry::snapshot()
    }

    pub fn report(before: &Snapshot) -> BackendReport {
        let d = loki::telemetry::snapshot().since(before);
        BackendReport {
            selected: d.backend.clone(),
            msm: Some(mirror(d.msm)),
            ntt: Some(mirror(d.ntt)),
        }
    }
}

#[cfg(not(any(
    feature = "cuda",
    feature = "cuda-ntt",
    feature = "loki-sppark",
    feature = "loki-gpu-backend"
)))]
mod telemetry {
    use super::BackendReport;
    pub struct Snapshot;
    pub fn snapshot() -> Snapshot {
        Snapshot
    }
    pub fn report(_: &Snapshot) -> BackendReport {
        BackendReport {
            selected: "stock-cpu".into(),
            msm: None,
            ntt: None,
        }
    }
}

/// GPU gates: an accelerated build must publish accelerator results and must
/// not fall back. MSM eligibility starts at `LOKI_GPU_MSM_MIN` (default 2^14),
/// so MSM success is only required from k14.
fn require_dispatch(k: u32, report: &BackendReport) -> Result<()> {
    if !ACCELERATED {
        return Ok(());
    }
    let (msm, ntt) = (
        report.msm.unwrap_or_default(),
        report.ntt.unwrap_or_default(),
    );
    ensure(
        msm.fallback + ntt.fallback + msm.error + ntt.error == 0,
        &format!("accelerator fallback/error in a GPU-required gate: {report:?}"),
    )?;
    let wants_msm = cfg!(any(
        feature = "cuda",
        feature = "loki-sppark",
        feature = "loki-gpu-backend"
    ));
    let wants_ntt = cfg!(any(
        feature = "cuda-ntt",
        feature = "loki-sppark",
        feature = "loki-gpu-backend"
    ));
    if wants_msm && k >= 14 {
        ensure(
            msm.succeeded > 0,
            &format!("no accelerated MSM completed: {report:?}"),
        )?;
    }
    if wants_ntt {
        ensure(
            ntt.succeeded > 0,
            &format!("no accelerated NTT completed: {report:?}"),
        )?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct Comparison {
    pub k: u32,
    pub lookups: bool,
    pub cross_verified: bool,
    pub vk_equal: bool,
    /// Backend identity and dispatch counters over the Loki keygen+proof only.
    pub loki_backend: BackendReport,
    pub loki: Metrics,
    pub stock: Metrics,
    pub determinism: Difference,
}

fn ensure(condition: bool, message: &str) -> Result<()> {
    if !condition {
        return Err(io::Error::other(message).into());
    }
    Ok(())
}

fn compare<const LOOKUPS: bool>(k: u32) -> Result<Comparison> {
    let _proof_lock = PROOF_LOCK
        .lock()
        .map_err(|_| io::Error::other("proof worker panicked"))?;
    ensure([10, 14, 17, 20].contains(&k), "supported k: 10, 14, 17, 20")?;
    // Exercise both writers across the ordinary size matrix.
    let params = if k == 14 || k == 20 {
        backend::reference::setup(k)?
    } else {
        backend::fork::setup(k)?
    };
    let stock_params = backend::reference::roundtrip_params(&params)?;
    ensure(stock_params == params, "stock SRS bytes differ")?;
    ensure(
        backend::fork::roundtrip_params(&stock_params)? == params,
        "Loki SRS bytes differ",
    )?;
    let before = telemetry::snapshot();
    let loki = backend::fork::prove::<LOOKUPS>(k, &stock_params)?;
    let loki_backend = telemetry::report(&before);
    require_dispatch(k, &loki_backend)?;
    let stock = backend::reference::prove::<LOOKUPS>(k, &params)?;
    ensure(
        loki.vk == stock.vk,
        "independently generated VK bytes differ",
    )?;
    ensure(
        loki.verifier_params == stock.verifier_params,
        "verifier params differ",
    )?;
    backend::reference::verify::<LOOKUPS>(&loki.vk, &loki.verifier_params, &loki.proof, 7)?;
    backend::fork::verify::<LOOKUPS>(&stock.vk, &stock.verifier_params, &stock.proof, 7)?;
    // These controls catch accidental unconditional acceptance or loss of public input binding.
    ensure(
        backend::reference::verify::<LOOKUPS>(&loki.vk, &loki.verifier_params, &loki.proof, 8)
            .is_err(),
        "stock accepted wrong public input",
    )?;
    ensure(
        backend::fork::verify::<LOOKUPS>(&stock.vk, &stock.verifier_params, &stock.proof, 8)
            .is_err(),
        "Loki accepted wrong public input",
    )?;
    ensure(
        backend::reference::verify::<LOOKUPS>(&loki.vk, &loki.verifier_params, &[], 7).is_err(),
        "stock accepted empty proof",
    )?;
    ensure(
        backend::fork::verify::<LOOKUPS>(&stock.vk, &stock.verifier_params, &[], 7).is_err(),
        "Loki accepted empty proof",
    )?;
    let mut extended = loki.proof.clone();
    extended.push(0);
    ensure(
        backend::reference::verify::<LOOKUPS>(&loki.vk, &loki.verifier_params, &extended, 7)
            .is_err(),
        "stock harness accepted trailing proof bytes",
    )?;
    let mut extended = stock.proof.clone();
    extended.push(0);
    ensure(
        backend::fork::verify::<LOOKUPS>(&stock.vk, &stock.verifier_params, &extended, 7).is_err(),
        "Loki harness accepted trailing proof bytes",
    )?;
    let difference = Difference::between(&loki.proof, &stock.proof);
    if !LOOKUPS {
        ensure(
            difference.equal,
            &format!("no-lookup proof bytes differ: {difference:?}"),
        )?;
    }
    Ok(Comparison {
        k,
        lookups: LOOKUPS,
        cross_verified: true,
        vk_equal: true,
        loki_backend,
        loki: loki.metrics,
        stock: stock.metrics,
        determinism: difference,
    })
}

/// Both cross directions under a byte-identical VK and serialized shared SRS.
pub fn cross_verify(k: u32) -> Result<Comparison> {
    compare::<true>(k)
}

/// Report lookup byte differences; require a byte-identical no-lookup control.
pub fn determinism(k: u32) -> Result<Comparison> {
    ensure(
        std::env::var("RAYON_NUM_THREADS").as_deref() == Ok("1"),
        "determinism requires RAYON_NUM_THREADS=1 before process startup",
    )?;
    ensure(
        rayon::current_num_threads() == 1,
        "determinism requires one Rayon worker",
    )?;
    let control = compare::<false>(10)?;
    eprintln!(
        "no_lookup_control {}",
        serde_json::to_string(&control.determinism)?
    );
    let result = compare::<true>(k)?;
    eprintln!(
        "lookup_determinism k={k} {}",
        serde_json::to_string(&result.determinism)?
    );
    Ok(result)
}

pub fn negative_controls() -> Result<()> {
    backend::fork::negative_controls()?;
    backend::reference::negative_controls()
}
