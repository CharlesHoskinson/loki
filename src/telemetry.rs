//! Production accelerator dispatch counters (P7 benchmark telemetry).
//!
//! Compiled only with an accelerator feature (`cuda`, `cuda-ntt` or
//! `loki-backend-api`); the default build has no such module and no counters.
//! Counters are process-wide relaxed atomics updated at the dispatch decision
//! points. They never touch the transcript, RNG, proof bytes or any operand.
//!
//! Per operation:
//! * `calls` — every entry into the dispatch decision;
//! * `cpu` — routed straight to the CPU (ineligible size/type, disabled);
//! * `attempted` — offered to the accelerator;
//! * `succeeded` — accelerator result published;
//! * `error` — accelerator returned an error;
//! * `fallback` — CPU recomputed after an accelerator error.
//!
//! `calls == cpu + attempted` and `attempted == succeeded + error` hold for the
//! legacy routes. With `loki-backend-api` the dispatcher's own counters are
//! merged in (see [`snapshot`]).

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// One operation's counters. See the module documentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpCounters {
    /// Entries into the dispatch decision.
    pub calls: u64,
    /// Routed to the CPU without an accelerator attempt.
    pub cpu: u64,
    /// Offered to the accelerator.
    pub attempted: u64,
    /// Accelerator result published.
    pub succeeded: u64,
    /// Accelerator errors.
    pub error: u64,
    /// CPU recomputations after an accelerator error.
    pub fallback: u64,
}

impl OpCounters {
    /// Counter-wise difference `self - earlier` (saturating).
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            calls: self.calls.saturating_sub(earlier.calls),
            cpu: self.cpu.saturating_sub(earlier.cpu),
            attempted: self.attempted.saturating_sub(earlier.attempted),
            succeeded: self.succeeded.saturating_sub(earlier.succeeded),
            error: self.error.saturating_sub(earlier.error),
            fallback: self.fallback.saturating_sub(earlier.fallback),
        }
    }
}

/// Process-wide telemetry snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Compile-time selected accelerator backend (see [`backend_identity`]).
    pub backend: String,
    /// MSM dispatch counters.
    pub msm: OpCounters,
    /// Host NTT/FFT dispatch counters.
    pub ntt: OpCounters,
}

impl Snapshot {
    /// Counter-wise difference over an interval.
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            backend: self.backend.clone(),
            msm: self.msm.since(&earlier.msm),
            ntt: self.ntt.since(&earlier.ntt),
        }
    }
}

/// Identity of the accelerator selected at compile time.
pub fn backend_identity() -> &'static str {
    if cfg!(feature = "loki-gpu-backend") {
        "loki-gpu-backend"
    } else if cfg!(all(feature = "cuda-srs-registry", feature = "cuda-ntt")) {
        "legacy-sppark:msm+srs-registry,ntt"
    } else if cfg!(all(feature = "cuda", feature = "cuda-ntt")) {
        "legacy-sppark:msm,ntt"
    } else if cfg!(feature = "cuda-srs-registry") {
        "legacy-sppark:msm+srs-registry"
    } else if cfg!(feature = "cuda") {
        "legacy-sppark:msm"
    } else if cfg!(feature = "cuda-ntt") {
        "legacy-sppark:ntt"
    } else {
        "loki-backend-api:cpu"
    }
}

struct Counters {
    calls: AtomicU64,
    cpu: AtomicU64,
    attempted: AtomicU64,
    succeeded: AtomicU64,
    error: AtomicU64,
    fallback: AtomicU64,
}

impl Counters {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            cpu: AtomicU64::new(0),
            attempted: AtomicU64::new(0),
            succeeded: AtomicU64::new(0),
            error: AtomicU64::new(0),
            fallback: AtomicU64::new(0),
        }
    }

    fn load(&self) -> OpCounters {
        let g = |c: &AtomicU64| c.load(Ordering::Relaxed);
        OpCounters {
            calls: g(&self.calls),
            cpu: g(&self.cpu),
            attempted: g(&self.attempted),
            succeeded: g(&self.succeeded),
            error: g(&self.error),
            fallback: g(&self.fallback),
        }
    }
}

fn bump(c: &AtomicU64) {
    c.fetch_add(1, Ordering::Relaxed);
}

static MSM: Counters = Counters::new();
static NTT: Counters = Counters::new();

/// Operation selector for the internal recording hooks.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum Op {
    Msm,
    Ntt,
}

impl Op {
    fn counters(self) -> &'static Counters {
        match self {
            Op::Msm => &MSM,
            Op::Ntt => &NTT,
        }
    }
}

/// Outcome of one legacy dispatch decision.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) enum Route {
    /// Not offered to the accelerator.
    Cpu,
    /// Offered and published.
    Accelerated,
    /// Offered, failed, recomputed on CPU.
    Fallback,
}

/// Record one legacy dispatch decision.
#[allow(dead_code)]
pub(crate) fn record(op: Op, route: Route) {
    let c = op.counters();
    bump(&c.calls);
    match route {
        Route::Cpu => bump(&c.cpu),
        Route::Accelerated => {
            bump(&c.attempted);
            bump(&c.succeeded);
        }
        Route::Fallback => {
            bump(&c.attempted);
            bump(&c.error);
            bump(&c.fallback);
        }
    }
}

/// Current process-wide counters, including the P1 dispatcher's counters when
/// `loki-backend-api` is enabled.
pub fn snapshot() -> Snapshot {
    #[allow(unused_mut)]
    let mut s = Snapshot {
        backend: backend_identity().into(),
        msm: MSM.load(),
        ntt: NTT.load(),
    };
    #[cfg(feature = "loki-backend-api")]
    {
        let d = crate::backend::dispatch::global().counters();
        // The dispatcher counts every CPU computation, including CPU recomputation
        // after an accelerator failure, in `*_cpu`.
        let merge = |o: &mut OpCounters, cpu: u64, accelerated: u64, fallback: u64| {
            o.calls += cpu.saturating_sub(fallback) + accelerated + fallback;
            o.cpu += cpu.saturating_sub(fallback);
            o.attempted += accelerated + fallback;
            o.succeeded += accelerated;
            o.error += fallback;
            o.fallback += fallback;
        };
        merge(&mut s.msm, d.msm_cpu, d.msm_accelerated, d.msm_fallback);
        merge(
            &mut s.ntt,
            d.transform_cpu,
            d.transform_accelerated,
            d.transform_fallback,
        );
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // Counters are process-wide; tests assert on deltas only.

    #[test]
    fn routes_update_the_documented_counters() {
        let before = snapshot();
        record(Op::Msm, Route::Cpu);
        record(Op::Msm, Route::Accelerated);
        record(Op::Msm, Route::Accelerated);
        record(Op::Msm, Route::Fallback);
        record(Op::Ntt, Route::Fallback);
        let d = snapshot().since(&before);
        assert!(d.msm.calls >= 4);
        assert!(d.msm.cpu >= 1);
        assert!(d.msm.succeeded >= 2);
        assert!(d.msm.error >= 1 && d.msm.fallback >= 1);
        assert!(d.msm.attempted >= 3);
        assert!(d.ntt.error >= 1 && d.ntt.fallback >= 1);
    }

    #[test]
    fn interval_difference_is_counterwise() {
        let a = OpCounters {
            calls: 10,
            cpu: 4,
            attempted: 6,
            succeeded: 5,
            error: 1,
            fallback: 1,
        };
        let b = OpCounters {
            calls: 3,
            cpu: 1,
            attempted: 2,
            succeeded: 2,
            error: 0,
            fallback: 0,
        };
        assert_eq!(
            a.since(&b),
            OpCounters {
                calls: 7,
                cpu: 3,
                attempted: 4,
                succeeded: 3,
                error: 1,
                fallback: 1,
            }
        );
    }

    #[cfg(feature = "cuda")]
    #[test]
    fn legacy_msm_dispatch_records_cpu_success_and_fallback() {
        use crate::poly::kzg::msm::cuda::dispatch_for_test as dispatch;
        use ff::Field;
        use group::Group;
        use midnight_curves::{Fq, G1Affine, G1Projective};

        let scalars = vec![Fq::ONE; 8];
        let bases = vec![G1Projective::generator(); 8];
        let before = snapshot().msm;
        // Below the size threshold: CPU route, accelerator never called.
        assert!(dispatch::<G1Affine>(&scalars, &bases, 16, |_, _| panic!("not offered")).is_none());
        let d = snapshot().msm.since(&before);
        assert!(d.cpu >= 1 && d.calls >= 1);

        let before = snapshot().msm;
        assert!(dispatch::<G1Affine>(&scalars, &bases, 4, |_, _| Ok(G1Projective::identity()))
            .is_some());
        let d = snapshot().msm.since(&before);
        assert!(d.attempted >= 1 && d.succeeded >= 1);

        let before = snapshot().msm;
        assert!(
            dispatch::<G1Affine>(&scalars, &bases, 4, |_, _| Err("injected".into())).is_none()
        );
        let d = snapshot().msm.since(&before);
        assert!(d.attempted >= 1 && d.error >= 1 && d.fallback >= 1);
    }

    #[cfg(feature = "cuda-ntt")]
    #[test]
    fn legacy_ntt_forced_error_is_counted_as_fallback() {
        use crate::poly::EvaluationDomain;
        use midnight_curves::Fq;

        // SAFETY of the environment write: this test binary's other tests do not
        // read LOKI_NTT_FORCE_ERROR; cuda-ntt reads it per call.
        std::env::set_var("LOKI_NTT_FORCE_ERROR", "1");
        let domain = EvaluationDomain::<Fq>::new(1, 11);
        let mut poly = domain.empty_lagrange();
        for (i, v) in poly.iter_mut().enumerate() {
            *v = Fq::from(i as u64);
        }
        let before = snapshot().ntt;
        let coeffs = domain.lagrange_to_coeff(poly.clone());
        std::env::remove_var("LOKI_NTT_FORCE_ERROR");
        let d = snapshot().ntt.since(&before);
        assert!(d.attempted >= 1 && d.error >= 1 && d.fallback >= 1, "{d:?}");
        assert_eq!(coeffs.len(), 1 << 11);
    }
}
