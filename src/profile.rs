//! Optional wall-clock profiling for `plonk::create_proof`.
//!
//! Enable `loki-profile`, then call [`take_report`] to drain completed proofs.
//! Setting `LOKI_PROFILE=/path/to/profile.jsonl` also appends one JSON object per
//! proof attempt. Output failures are recorded in [`ProofReport::jsonl_error`]
//! and never change the proof result. No witnesses or transcript data are logged.
//!
//! Phases contain **exclusive** wall time, including time waiting for parallel
//! arithmetic. `other` covers work outside named phases. Counts measure arithmetic
//! entry calls and input lengths (MSM lengths are measured before zero filtering).
//! Key generation and verification are excluded. Concurrent calling threads have
//! independent active records; completed records and JSONL writes are synchronized
//! within this process. Arithmetic entry calls currently run on the proof's calling
//! thread, before dispatching worker tasks; worker CPU time is not summed.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::OpenOptions,
    io::{self, Write},
    marker::PhantomData,
    rc::Rc,
    sync::Mutex,
    time::Instant,
};

use serde::{Deserialize, Serialize};

/// Completed proof attempts, in completion order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    /// Completed attempts since the previous [`take_report`].
    pub proofs: Vec<ProofReport>,
}

/// Exclusive wall time for one phase, accumulated across circuit instances.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhaseTiming {
    /// Number of entries into this phase.
    pub calls: u64,
    /// Exclusive elapsed wall time in nanoseconds.
    pub wall_ns: u64,
    /// Backend dispatch outcomes (for example `msm.gpu` or `ntt.cpu_forced`)
    /// and preparation counters for this phase.
    #[serde(default)]
    pub backends: BTreeMap<String, OperationCounts>,
}

/// Arithmetic call counts and input-size histogram.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OperationCounts {
    /// Number of arithmetic entry calls, including empty/zero MSMs.
    pub calls: u64,
    /// Sum of input lengths over all calls.
    pub elements: u64,
    /// Input length to call count.
    pub sizes: BTreeMap<usize, u64>,
    /// Accumulated backend wall time in nanoseconds (zero for untimed counts).
    #[serde(default)]
    pub wall_ns: u64,
}

impl OperationCounts {
    fn record(&mut self, size: usize) {
        self.calls += 1;
        self.elements += size as u64;
        *self.sizes.entry(size).or_default() += 1;
    }
}

/// One `create_proof` attempt. Timings exclude report publication and file I/O.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProofReport {
    /// Base-two logarithm of the proof domain size.
    pub k: u32,
    /// Number of circuits batched in this proof.
    pub circuits: usize,
    /// Effective number of workers in the active Rayon pool.
    #[serde(default)]
    pub rayon_threads: usize,
    /// Known BLST worker count when `loki-cpu-budget` enforces `blst/no-threads`.
    /// Otherwise unknown; no inference is made from the active Rayon pool.
    #[serde(default)]
    pub blst_threads: Option<usize>,
    /// Whether the MSM CUDA adapter was compiled in.
    #[serde(default)]
    pub cuda_msm: bool,
    /// Whether the NTT CUDA adapter was compiled in.
    #[serde(default)]
    pub cuda_ntt: bool,
    /// Whether `create_proof` returned `Ok` (false also covers unwinding).
    pub success: bool,
    /// Total wall time in nanoseconds; equals the sum of exclusive phases.
    pub total_wall_ns: u64,
    /// Phase names to exclusive wall timings.
    pub phases: BTreeMap<String, PhaseTiming>,
    /// MSM entry calls and input lengths before zero filtering.
    pub msm: OperationCounts,
    /// FFT entry calls and transform lengths (forward and inverse combined).
    pub fft: OperationCounts,
    /// JSONL output failure, if any. The completed report remains available.
    pub jsonl_error: Option<String>,
}

static COMPLETED: Mutex<Report> = Mutex::new(Report { proofs: Vec::new() });

// Unit tests that drain the process-wide report must not consume each other's
// completed records. This lock does not serialize proofs or production callers.
#[cfg(test)]
pub(crate) static TEST_REPORTS: Mutex<()> = Mutex::new(());

/// Atomically drain completed proof attempts from the process-wide report.
///
/// Active proofs are unaffected and will appear in a later call. The caller
/// owns the returned serializable data. Repeated calls without completed proofs
/// return an empty report. Records accumulate until drained.
pub fn take_report() -> Report {
    std::mem::take(&mut *COMPLETED.lock().unwrap_or_else(|e| e.into_inner()))
}

struct ActiveProof {
    report: ProofReport,
    started: Instant,
    checkpoint: Instant,
    phase: String,
}

impl ActiveProof {
    fn charge(&mut self, now: Instant) {
        self.report.phases.get_mut(&self.phase).unwrap().wall_ns +=
            now.duration_since(self.checkpoint).as_nanos() as u64;
        self.checkpoint = now;
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<ActiveProof>> = const { RefCell::new(None) };
}

// Guards must stay on their originating thread. Restoring the previous record
// also makes recursive create_proof calls safe without merging their counters.
pub(crate) struct ProofGuard {
    previous: Option<ActiveProof>,
    pub(crate) success: bool,
    _thread: PhantomData<Rc<()>>,
}

impl ProofGuard {
    pub(crate) fn start(k: u32, circuits: usize) -> Self {
        let now = Instant::now();
        let mut report = ProofReport {
            k,
            circuits,
            rayon_threads: rayon::current_num_threads(),
            blst_threads: cfg!(feature = "loki-cpu-budget").then_some(1),
            cuda_msm: cfg!(feature = "cuda"),
            cuda_ntt: cfg!(feature = "cuda-ntt"),
            ..ProofReport::default()
        };
        eprintln!(
            "LOKI_PROFILE k={k} rayon_threads={} blst_threads={} loki_cpu_budget={} cuda_msm={} cuda_ntt={}",
            report.rayon_threads,
            report.blst_threads.map_or("unknown", |_| "1"),
            cfg!(feature = "loki-cpu-budget"),
            report.cuda_msm, report.cuda_ntt
        );
        report.phases.insert(
            "other".into(),
            PhaseTiming {
                calls: 1,
                wall_ns: 0,
                backends: BTreeMap::new(),
            },
        );
        let active = ActiveProof {
            report,
            started: now,
            checkpoint: now,
            phase: "other".into(),
        };
        Self {
            previous: ACTIVE.with(|slot| slot.replace(Some(active))),
            success: false,
            _thread: PhantomData,
        }
    }
}

impl Drop for ProofGuard {
    fn drop(&mut self) {
        let mut active = ACTIVE
            .with(|slot| slot.replace(self.previous.take()))
            .unwrap();
        let now = Instant::now();
        active.charge(now);
        active.report.total_wall_ns = now.duration_since(active.started).as_nanos() as u64;
        active.report.success = self.success;
        let mut completed = COMPLETED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(path) = std::env::var_os("LOKI_PROFILE") {
            if let Err(error) = append_jsonl(path, &active.report) {
                active.report.jsonl_error = Some(error.to_string());
            }
        }
        completed.proofs.push(active.report);
    }
}

pub(crate) struct PhaseGuard {
    previous: Option<String>,
    _thread: PhantomData<Rc<()>>,
}

impl PhaseGuard {
    pub(crate) fn start(name: impl Into<String>) -> Self {
        let previous = ACTIVE.with(|slot| {
            let mut slot = slot.borrow_mut();
            let active = slot.as_mut()?;
            active.charge(Instant::now());
            let name = name.into();
            active.report.phases.entry(name.clone()).or_default().calls += 1;
            Some(std::mem::replace(&mut active.phase, name))
        });
        Self {
            previous,
            _thread: PhantomData,
        }
    }
}

impl Drop for PhaseGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            ACTIVE.with(|slot| {
                if let Some(active) = slot.borrow_mut().as_mut() {
                    active.charge(Instant::now());
                    active.phase = previous;
                }
            });
        }
    }
}

pub(crate) fn record_msm(size: usize) {
    ACTIVE.with(|slot| {
        if let Some(active) = slot.borrow_mut().as_mut() {
            active.report.msm.record(size);
        }
    });
}

pub(crate) fn record_fft(size: usize) {
    ACTIVE.with(|slot| {
        if let Some(active) = slot.borrow_mut().as_mut() {
            active.report.fft.record(size);
        }
    });
}

/// Record the selected backend at the arithmetic dispatch boundary. Calls are
/// synchronous on the proof thread, so the active exclusive phase is unambiguous.
pub(crate) fn record_backend(outcome: &'static str, size: usize) {
    ACTIVE.with(|slot| {
        if let Some(active) = slot.borrow_mut().as_mut() {
            active
                .report
                .phases
                .get_mut(&active.phase)
                .unwrap()
                .backends
                .entry(outcome.into())
                .or_default()
                .record(size);
        }
    });
}

/// Record preparation counts and elapsed time on the calling proof thread.
pub(crate) fn record_backend_timing(outcome: &'static str, size: usize, wall_ns: u64) {
    ACTIVE.with(|slot| {
        if let Some(active) = slot.borrow_mut().as_mut() {
            let counts = active
                .report
                .phases
                .get_mut(&active.phase)
                .unwrap()
                .backends
                .entry(outcome.into())
                .or_default();
            counts.record(size);
            counts.wall_ns += wall_ns;
        }
    });
}

fn write_counts(mut out: impl Write, counts: &OperationCounts) -> io::Result<()> {
    write!(
        out,
        "{{\"calls\":{},\"elements\":{},\"wall_ns\":{},\"sizes\":{{",
        counts.calls, counts.elements, counts.wall_ns
    )?;
    for (i, (size, calls)) in counts.sizes.iter().enumerate() {
        if i != 0 {
            write!(out, ",")?;
        }
        write!(out, "\"{}\":{}", size, calls)?;
    }
    write!(out, "}}}}")
}

// The schema has only fixed internal phase names and numeric/boolean values,
// so emitting JSON here needs no runtime serializer dependency. Public report
// types implement serde for consumers that need other encodings.
fn append_jsonl(path: std::ffi::OsString, report: &ProofReport) -> io::Result<()> {
    let mut line = Vec::new();
    write!(
        line,
        "{{\"k\":{},\"circuits\":{},\"success\":{},\"total_wall_ns\":{},\"phases\":{{",
        report.k, report.circuits, report.success, report.total_wall_ns
    )?;
    for (i, (name, phase)) in report.phases.iter().enumerate() {
        if i != 0 {
            write!(line, ",")?;
        }
        write!(
            line,
            "\"{}\":{{\"calls\":{},\"wall_ns\":{},\"backends\":{{",
            name, phase.calls, phase.wall_ns
        )?;
        for (i, (outcome, counts)) in phase.backends.iter().enumerate() {
            if i != 0 {
                write!(line, ",")?;
            }
            write!(line, "\"{}\":", outcome)?;
            write_counts(&mut line, counts)?;
        }
        write!(line, "}}}}")?;
    }
    write!(line, "}}")?;
    for (name, counts) in [("msm", &report.msm), ("fft", &report.fft)] {
        write!(line, ",\"{}\":", name)?;
        write_counts(&mut line, counts)?;
    }
    write!(
        line,
        ",\"rayon_threads\":{},\"blst_threads\":{},\"cuda_msm\":{},\"cuda_ntt\":{}",
        report.rayon_threads,
        report.blst_threads.map_or("null", |_| "1"),
        report.cuda_msm,
        report.cuda_ntt
    )?;
    writeln!(line, ",\"jsonl_error\":null}}")?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    // Other lib tests can finish real proofs concurrently. Reserve impossible
    // domain sizes for these synthetic records so assertions ignore that traffic.
    fn take_test_report() -> Report {
        let mut report = take_report();
        report.proofs.retain(|proof| proof.k >= 1000);
        report
    }

    #[test]
    fn blst_thread_metadata_matches_budget_policy() {
        let _serial = TEST_REPORTS.lock().unwrap();
        let _ = take_test_report();
        drop(ProofGuard::start(1004, 1));
        let proof = take_test_report().proofs.pop().unwrap();
        let value = serde_json::to_value(&proof).unwrap();
        #[cfg(feature = "loki-cpu-budget")]
        assert_eq!(value["blst_threads"], serde_json::json!(1));
        #[cfg(not(feature = "loki-cpu-budget"))]
        assert_eq!(value["blst_threads"], serde_json::Value::Null);
        let mut legacy = value;
        legacy.as_object_mut().unwrap().remove("blst_threads");
        let restored: ProofReport = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            serde_json::to_value(restored).unwrap()["blst_threads"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn isolate_threads_drain_completed_and_serialize() {
        let _serial = TEST_REPORTS.lock().unwrap();
        assert!(take_test_report().proofs.is_empty());
        let barrier = Arc::new(Barrier::new(3));
        std::thread::scope(|scope| {
            for k in [1000, 1001] {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    let mut proof = ProofGuard::start(k, 1);
                    let outer = PhaseGuard::start("compute_nu_poly");
                    record_msm(16);
                    record_backend("msm.cpu_fallback", 16);
                    record_backend_timing("msm.prepare", 16, 123);
                    record_backend_timing("msm.prepare", 8, 77);
                    {
                        let _inner = PhaseGuard::start("quotient_evaluation");
                        record_fft(32);
                        record_backend("ntt.gpu", 32);
                        record_backend("ntt.gpu_error", 64);
                        record_backend("ntt.cpu_forced", 8);
                    }
                    drop(outer);
                    barrier.wait();
                    barrier.wait();
                    proof.success = true;
                });
            }
            barrier.wait();
            let active_reports = take_test_report();
            barrier.wait();
            assert!(
                active_reports.proofs.is_empty(),
                "active records must stay private"
            );
        });
        let reports = take_test_report();
        assert_eq!(reports.proofs.len(), 2);
        for proof in &reports.proofs {
            assert!(proof.success);
            let prepare = &proof.phases["compute_nu_poly"].backends["msm.prepare"];
            assert_eq!(prepare.calls, 2);
            assert_eq!(prepare.elements, 24);
            assert_eq!(prepare.wall_ns, 200);
            assert_eq!(proof.msm.calls, 1);
            assert_eq!(proof.msm.sizes[&16], 1);
            assert_eq!(proof.fft.calls, 1);
            assert_eq!(proof.fft.sizes[&32], 1);
            assert!(proof.rayon_threads > 0);
            assert_eq!(
                proof.blst_threads,
                cfg!(feature = "loki-cpu-budget").then_some(1)
            );
            let outer = &proof.phases["compute_nu_poly"].backends;
            assert_eq!(outer["msm.cpu_fallback"].sizes[&16], 1);
            // The dispatch outcome and the timed `msm.prepare` counter
            // share the phase.
            assert_eq!(outer.len(), 2);
            let inner = &proof.phases["quotient_evaluation"].backends;
            assert_eq!(inner["ntt.gpu"].sizes[&32], 1);
            assert_eq!(inner["ntt.gpu_error"].sizes[&64], 1);
            assert_eq!(inner["ntt.cpu_forced"].sizes[&8], 1);
            assert_eq!(inner.len(), 3);
            assert_eq!(
                proof.total_wall_ns,
                proof.phases.values().map(|p| p.wall_ns).sum::<u64>()
            );
        }
        assert!(take_test_report().proofs.is_empty());

        let path =
            std::env::temp_dir().join(format!("loki-profile-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        for proof in &reports.proofs {
            append_jsonl(path.clone().into_os_string(), proof).unwrap();
        }
        let lines = std::fs::read_to_string(&path).unwrap();
        assert_eq!(lines.lines().count(), 2);
        for (line, proof) in lines.lines().zip(&reports.proofs) {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value, serde_json::to_value(proof).unwrap());
            let roundtrip: ProofReport = serde_json::from_value(value).unwrap();
            assert_eq!(roundtrip.total_wall_ns, proof.total_wall_ns);
        }
        std::fs::remove_file(path).unwrap();
        assert!(append_jsonl(std::env::temp_dir().into_os_string(), &reports.proofs[0]).is_err());

        // An early return and an unwind both publish failed attempts, and restore
        // the thread-local slot so that subsequent attempts work normally.
        {
            let _failed = ProofGuard::start(1002, 1);
        }
        let unwind = std::panic::catch_unwind(|| {
            let _failed = ProofGuard::start(1003, 1);
            let _phase = PhaseGuard::start("evaluations");
            panic!("synthetic profile unwind");
        });
        assert!(unwind.is_err());
        let failed = take_test_report();
        assert_eq!(failed.proofs.len(), 2);
        assert!(failed.proofs.iter().all(|proof| !proof.success));
        record_msm(100);
        record_fft(100);
        assert!(take_test_report().proofs.is_empty());

        {
            let mut outer = ProofGuard::start(1004, 1);
            let _phase = PhaseGuard::start("evaluations");
            record_msm(16);
            {
                let mut inner = ProofGuard::start(1005, 1);
                record_msm(32);
                inner.success = true;
            }
            record_msm(64);
            outer.success = true;
        }
        let nested = take_test_report();
        assert_eq!(nested.proofs.len(), 2);
        assert_eq!(nested.proofs[0].k, 1005);
        assert_eq!(nested.proofs[0].msm.sizes, BTreeMap::from([(32, 1)]));
        assert_eq!(nested.proofs[1].k, 1004);
        assert_eq!(
            nested.proofs[1].msm.sizes,
            BTreeMap::from([(16, 1), (64, 1)])
        );
        for proof in nested.proofs {
            assert!(proof.success);
            assert_eq!(
                proof.total_wall_ns,
                proof.phases.values().map(|p| p.wall_ns).sum::<u64>()
            );
        }
    }
}
