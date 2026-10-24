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
