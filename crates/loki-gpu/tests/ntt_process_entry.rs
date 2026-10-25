//! The `TransformBackend` trait entry point on `LokiGpu`, which runs on the
//! process-wide runtime. It is a separate binary because that runtime is a
//! never-dropped static that owns the native driver claim (and its pinned
//! staging slots) for the rest of the process.
#![cfg(feature = "cuda")]

mod ntt_support;

use loki_backend_api::{Backend, TransformBackend};
use loki_gpu::ntt::{self, MAX_TRANSFORM_LOG};
use loki_gpu::runtime::GpuRuntime;
use loki_gpu::LokiGpu;
use ntt_support::*;

#[test]
fn trait_entry_point_uses_the_process_runtime() {
    let gpu = LokiGpu;
    if let Err(e) = gpu.initialize() {
        if std::env::var("LOKI_GPU_ALLOW_NO_DEVICE").as_deref() != Ok("1") {
            panic!("device required (set LOKI_GPU_ALLOW_NO_DEVICE=1 to skip): {e:?}");
        }
        return;
    }
    let s = Stock::new(3, 8);
    let a = input(Pattern::Random, s.n(), 3);
    let before = GpuRuntime::process().telemetry().transform_launches;
    let got = TransformBackend::<Fr>::transform(&gpu, &a, s.spec_coeff_to_extended()).unwrap();
    assert_vec_eq("trait", &got, &s.coeff_to_extended(&a));
    assert_eq!(GpuRuntime::process().telemetry().transform_launches, before + 1);
    let caps = Backend::<Fr, midnight_curves::G1Projective>::capabilities(&gpu);
    assert!(caps.host_transform && caps.resident_transform);
    assert_eq!(caps.max_transform_log, Some(MAX_TRANSFORM_LOG));
    assert!(caps.supports_arithmetic(ntt::TRANSFORM_ARITHMETIC));
}
