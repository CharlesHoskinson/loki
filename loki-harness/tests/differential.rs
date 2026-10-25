use loki_harness::{cross_verify, determinism, negative_controls};

macro_rules! cases {
    ($module:ident, $k:literal) => {
        mod $module {
            #[test]
            fn cross_verify() {
                super::cross_verify($k).unwrap();
            }

            #[test]
            fn determinism() {
                super::determinism($k).unwrap();
            }
        }
    };
}

cases!(k10, 10);
cases!(k14, 14);
cases!(k17, 17);

#[test]
#[ignore = "large CPU/GPU fixture; run explicitly with --ignored --test-threads=1"]
fn k20_cross_verify_and_determinism() {
    determinism(20).unwrap();
}

#[test]
fn reject_broken_constraints() {
    negative_controls().unwrap();
}

#[test]
fn comparison_reports_selected_backend_and_dispatch_counters() {
    let comparison = cross_verify(14).unwrap();
    let backend = &comparison.loki_backend;
    let accelerated = cfg!(any(
        feature = "cuda",
        feature = "cuda-ntt",
        feature = "loki-sppark",
        feature = "loki-gpu-backend"
    ));
    if accelerated {
        assert_ne!(backend.selected, "stock-cpu");
        let msm = backend.msm.expect("accelerated build reports MSM counters");
        let ntt = backend.ntt.expect("accelerated build reports NTT counters");
        assert!(
            msm.calls + ntt.calls > 0,
            "the Loki proof made no counted dispatch"
        );
        assert_eq!(
            msm.fallback + ntt.fallback,
            0,
            "unexpected fallback: {backend:?}"
        );
        assert_eq!(
            msm.error + ntt.error,
            0,
            "unexpected backend error: {backend:?}"
        );
    } else {
        assert_eq!(backend.selected, "stock-cpu");
        assert!(backend.msm.is_none() && backend.ntt.is_none());
    }
}
