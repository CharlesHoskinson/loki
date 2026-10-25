//! P2 native ABI gate (`--features cuda`).
//!
//! Part 1 needs no device: the Rust `#[repr(C)]` mirrors are compared with the
//! layout reported by the linked CUDA translation unit, status/handle
//! constants are compared with the shared header text, and every linked
//! runtime symbol is called with malformed arguments to check that it
//! rejects them before touching memory.
//!
//! Part 2 opens the GPU (run under the shared GPU lock). Device tests are
//! serialized in-process because the native context epoch is process-global.
//! A missing device fails the gate; set `LOKI_GPU_ALLOW_NO_DEVICE=1` to skip
//! the device tests on a toolkit-only host.
//! These tests are the native lifetime tests run under compute-sanitizer.

mod support;

use std::ptr;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use loki_backend_api::{ErrorKind, FailureEffect, ResidentBackend};
use loki_gpu::ffi::{
    self, native_abi_layout, native_driver, AbiLayoutV1, DeviceInfoV1, HandleV1, StatusV1,
};
use loki_gpu::runtime::{GpuRuntime, RuntimeConfig, REQUIRED_COMPUTE_CAPABILITY};
use support::{scalars, Toy, ToyPoint};

// Independent declarations of the linked symbols (not the crate's own), so a
// renamed or re-typed export fails to link here.
extern "C" {
    fn loki_gpu_msm_v1(s: *const u8, sb: usize, b: *const u8, bb: usize, o: *mut u8, ob: usize) -> i32;
    fn loki_gpu_ntt_v1(i: *const u8, ib: usize, o: *mut u8, ob: usize, p: *const u8, pb: usize) -> i32;
    fn loki_gpu_quotient_v1(i: *const u8, ib: usize, o: *mut u8, ob: usize, p: *const u8, pb: usize) -> i32;
    fn loki_gpu_rt_abi_version() -> u32;
    fn loki_gpu_rt_probe_v1(ordinal: i32, out: *mut DeviceInfoV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_reset_v1(ordinal: i32, out: *mut DeviceInfoV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_stream_create_v1(out: *mut HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_stream_destroy_v1(h: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_event_create_v1(timing: u32, out: *mut HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_event_destroy_v1(h: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_device_alloc_v1(bytes: u64, out: *mut HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_device_free_v1(h: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_host_alloc_v1(bytes: u64, pinned: u32, out: *mut HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_host_free_v1(h: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_memcpy_h2d_v1(
        d: *const HandleV1, doff: u64, s: *const HandleV1, soff: u64, len: u64, q: *const HandleV1,
        st: *mut StatusV1,
    ) -> i32;
    fn loki_gpu_rt_memcpy_d2h_v1(
        d: *const HandleV1, doff: u64, s: *const HandleV1, soff: u64, len: u64, q: *const HandleV1,
        st: *mut StatusV1,
    ) -> i32;
    fn loki_gpu_rt_launch_copy_v1(
        d: *const HandleV1, s: *const HandleV1, bytes: u64, q: *const HandleV1, st: *mut StatusV1,
    ) -> i32;
    fn loki_gpu_rt_event_record_v1(e: *const HandleV1, q: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_stream_wait_event_v1(q: *const HandleV1, e: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_event_synchronize_v1(e: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_event_query_v1(e: *const HandleV1, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_event_elapsed_ms_v1(a: *const HandleV1, b: *const HandleV1, ms: *mut f32, st: *mut StatusV1) -> i32;
    fn loki_gpu_rt_stream_synchronize_v1(q: *const HandleV1, st: *mut StatusV1) -> i32;
}

const HEADER: &str = include_str!("../cuda/include/loki_gpu_ffi.h");

fn header_define(name: &str) -> i64 {
    let line = HEADER
        .lines()
        .find(|l| l.split_whitespace().nth(1) == Some(name) && l.trim_start().starts_with("#define"))
        .unwrap_or_else(|| panic!("{name} missing from header"));
    let open = line.find(")(").or_else(|| line.find(')')).unwrap();
    let digits: String = line[open + 1..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().unwrap()
}

fn offset<T, F>(base: &T, field: &F) -> u32 {
    (field as *const F as usize - base as *const T as usize) as u32
}

// --------------------------------------------------------------------------
// Part 1: no device
// --------------------------------------------------------------------------

#[test]
fn status_and_kind_constants_match_header() {
    let pairs: [(&str, i64); 17] = [
        ("LOKI_GPU_OK", ffi::OK.into()),
        ("LOKI_GPU_UNSUPPORTED", ffi::UNSUPPORTED.into()),
        ("LOKI_GPU_INVALID_ARGUMENT", ffi::INVALID_ARGUMENT.into()),
        ("LOKI_GPU_RUNTIME_ERROR", ffi::RUNTIME_ERROR.into()),
        ("LOKI_GPU_OUT_OF_MEMORY", ffi::OUT_OF_MEMORY.into()),
        ("LOKI_GPU_NOT_READY", ffi::NOT_READY.into()),
        ("LOKI_GPU_STALE_HANDLE", ffi::STALE_HANDLE.into()),
        ("LOKI_GPU_CONTEXT_LOST", ffi::CONTEXT_LOST.into()),
        ("LOKI_GPU_ABI_MISMATCH", ffi::ABI_MISMATCH.into()),
        ("LOKI_GPU_NO_DEVICE", ffi::NO_DEVICE.into()),
        ("LOKI_GPU_RT_ABI_VERSION", ffi::RT_ABI_VERSION.into()),
        ("LOKI_GPU_HANDLE_DEVICE_MEMORY", ffi::HANDLE_DEVICE_MEMORY.into()),
        ("LOKI_GPU_HANDLE_HOST_PINNED", ffi::HANDLE_HOST_PINNED.into()),
        ("LOKI_GPU_HANDLE_HOST_PAGEABLE", ffi::HANDLE_HOST_PAGEABLE.into()),
        ("LOKI_GPU_HANDLE_STREAM", ffi::HANDLE_STREAM.into()),
        ("LOKI_GPU_HANDLE_EVENT", ffi::HANDLE_EVENT.into()),
        ("LOKI_GPU_RT_ABI_VERSION", 1),
    ];
    for (name, value) in pairs {
        assert_eq!(header_define(name), value, "{name}");
    }
    // Status codes are distinct.
    let mut codes: Vec<i64> = pairs[..10].iter().map(|p| p.1).collect();
    codes.sort();
    codes.dedup();
    assert_eq!(codes.len(), 10);
}

#[test]
fn rust_mirrors_match_native_layout() {
    let (version, status, native) = native_abi_layout();
    assert_eq!(version, ffi::RT_ABI_VERSION);
    // SAFETY: no arguments; returns a constant.
    assert_eq!(unsafe { loki_gpu_rt_abi_version() }, version);
    assert_eq!(status, ffi::OK);

    let h = HandleV1::default();
    let s = StatusV1::default();
    let d = DeviceInfoV1::default();
    let expected = AbiLayoutV1 {
        abi_version: ffi::RT_ABI_VERSION,
        handle_size: std::mem::size_of::<HandleV1>() as u32,
        handle_align: std::mem::align_of::<HandleV1>() as u32,
        handle_offset_context_epoch: offset(&h, &h.context_epoch),
        handle_offset_raw: offset(&h, &h.raw),
        handle_offset_bytes: offset(&h, &h.bytes),
        status_size: std::mem::size_of::<StatusV1>() as u32,
        status_offset_sticky: offset(&s, &s.sticky),
        device_info_size: std::mem::size_of::<DeviceInfoV1>() as u32,
        device_info_offset_total_bytes: offset(&d, &d.total_bytes),
        device_info_offset_context_epoch: offset(&d, &d.context_epoch),
        device_info_offset_name: offset(&d, &d.name),
        layout_size: std::mem::size_of::<AbiLayoutV1>() as u32,
        reserved: 0,
    };
    assert_eq!(native, expected);
    assert_eq!((native.handle_size, native.status_size), (32, 16));
    assert_eq!((native.device_info_size, native.layout_size), (120, 56));
}

#[test]
fn v1_operation_stubs_are_unsupported_and_leave_output_untouched() {
    let mut out = [0x5a_u8; 48];
    let before = out;
    // SAFETY: the v1 stub contract returns UNSUPPORTED before reading any
    // input or writing output; null inputs check that contract.
    unsafe {
        assert_eq!(loki_gpu_msm_v1(ptr::null(), 32, ptr::null(), 96, out.as_mut_ptr(), 48), ffi::UNSUPPORTED);
        assert_eq!(loki_gpu_ntt_v1(ptr::null(), 64, out.as_mut_ptr(), 48, ptr::null(), 16), ffi::UNSUPPORTED);
        assert_eq!(loki_gpu_quotient_v1(ptr::null(), 64, out.as_mut_ptr(), 48, ptr::null(), 16), ffi::UNSUPPORTED);
    }
    assert_eq!(out, before);
}

fn bogus(kind: u32) -> HandleV1 {
    HandleV1 {
        abi_version: ffi::RT_ABI_VERSION,
        kind,
        context_epoch: u64::MAX, // never a live epoch
        raw: 0xdead_0000,
        bytes: 4096,
    }
}

#[test]
fn malformed_handles_and_null_pointers_are_rejected_before_use() {
    let mut st = StatusV1 { code: 77, cuda_error: 77, sticky: 77, reserved: 77 };
    let stale_mem = bogus(ffi::HANDLE_DEVICE_MEMORY);
    let stale_host = bogus(ffi::HANDLE_HOST_PINNED);
    let stale_stream = bogus(ffi::HANDLE_STREAM);
    let stale_event = bogus(ffi::HANDLE_EVENT);
    let wrong_version = HandleV1 { abi_version: 99, ..stale_mem };
    let zero_raw = HandleV1 { raw: 0, ..stale_mem };
    let mut ms = -1.0f32;
    // SAFETY: every call validates its handles/pointers before dereferencing;
    // the bogus raw bits are never dereferenced because validation fails.
    unsafe {
        assert_eq!(loki_gpu_rt_probe_v1(0, ptr::null_mut(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(st.code, ffi::INVALID_ARGUMENT);
        assert_eq!((st.sticky, st.reserved), (0, 0));
        assert_eq!(loki_gpu_rt_probe_v1(-1, &mut DeviceInfoV1::default(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_reset_v1(0, ptr::null_mut(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_stream_create_v1(ptr::null_mut(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_event_create_v1(1, ptr::null_mut(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_device_alloc_v1(64, ptr::null_mut(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_device_alloc_v1(0, &mut HandleV1::default(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_host_alloc_v1(0, 1, &mut HandleV1::default(), &mut st), ffi::INVALID_ARGUMENT);

        assert_eq!(loki_gpu_rt_device_free_v1(ptr::null(), &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_device_free_v1(&wrong_version, &mut st), ffi::ABI_MISMATCH);
        assert_eq!(loki_gpu_rt_device_free_v1(&zero_raw, &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_device_free_v1(&stale_stream, &mut st), ffi::INVALID_ARGUMENT, "kind");
        assert_eq!(loki_gpu_rt_device_free_v1(&stale_mem, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_host_free_v1(&stale_host, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_host_free_v1(&stale_mem, &mut st), ffi::INVALID_ARGUMENT, "kind");
        assert_eq!(loki_gpu_rt_stream_destroy_v1(&stale_stream, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_destroy_v1(&stale_event, &mut st), ffi::STALE_HANDLE);
        assert_eq!(
            loki_gpu_rt_memcpy_h2d_v1(&stale_mem, 0, &stale_host, 0, 8, &stale_stream, &mut st),
            ffi::STALE_HANDLE
        );
        assert_eq!(
            loki_gpu_rt_memcpy_d2h_v1(&stale_host, 0, &stale_mem, 0, 8, &stale_stream, &mut st),
            ffi::STALE_HANDLE
        );
        assert_eq!(
            loki_gpu_rt_memcpy_h2d_v1(&stale_mem, 0, ptr::null(), 0, 8, &stale_stream, &mut st),
            ffi::STALE_HANDLE, // dst is validated first
        );
        assert_eq!(loki_gpu_rt_launch_copy_v1(&stale_mem, &stale_mem, 8, &stale_stream, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_record_v1(&stale_event, &stale_stream, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_stream_wait_event_v1(&stale_stream, &stale_event, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_synchronize_v1(&stale_event, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_query_v1(&stale_event, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_elapsed_ms_v1(&stale_event, &stale_event, &mut ms, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_stream_synchronize_v1(&stale_stream, &mut st), ffi::STALE_HANDLE);
        // A null status pointer is allowed.
        assert_eq!(loki_gpu_rt_stream_synchronize_v1(&stale_stream, ptr::null_mut()), ffi::STALE_HANDLE);
    }
    assert_eq!(ms, -1.0, "elapsed output untouched on rejection");
}

// --------------------------------------------------------------------------
// Part 2: device (serialized)
// --------------------------------------------------------------------------

static DEVICE: Mutex<()> = Mutex::new(());

fn device_lock() -> MutexGuard<'static, ()> {
    DEVICE.lock().unwrap_or_else(|p| p.into_inner())
}

/// Probe the device; `None` means skip (no device and skipping allowed).
fn probe() -> Option<DeviceInfoV1> {
    let mut info = DeviceInfoV1::default();
    let mut st = StatusV1::default();
    // SAFETY: valid out-pointers.
    let code = unsafe { loki_gpu_rt_probe_v1(0, &mut info, &mut st) };
    if code == ffi::OK {
        return Some(info);
    }
    if std::env::var("LOKI_GPU_ALLOW_NO_DEVICE").as_deref() != Ok("1") {
        panic!(
            "device required (set LOKI_GPU_ALLOW_NO_DEVICE=1 to skip) but probe returned {code} \
             (cuda error {})",
            st.cuda_error
        );
    }
    eprintln!("skipping device test: probe returned {code}");
    None
}

fn native_runtime(cfg: RuntimeConfig) -> GpuRuntime {
    let rt = GpuRuntime::with_driver(native_driver(), cfg);
    rt.initialize().expect("native runtime initializes on sm_120");
    rt
}

fn small_config() -> RuntimeConfig {
    RuntimeConfig {
        device_budget_cap: Some(256 << 20),
        device_headroom_bytes: 0,
        staging_slots: 2,
        staging_slot_bytes: 4096,
        ..RuntimeConfig::default()
    }
}

type Rb = dyn ResidentBackend<Toy, ToyPoint>;
fn rb(rt: &GpuRuntime) -> &Rb {
    rt
}

#[test]
fn device_probe_reports_sm120() {
    let _g = device_lock();
    let Some(info) = probe() else { return };
    assert_eq!(info.abi_version, ffi::RT_ABI_VERSION);
    assert_eq!(
        (info.cc_major as u32, info.cc_minor as u32),
        REQUIRED_COMPUTE_CAPABILITY
    );
    assert!(info.total_bytes > 0 && info.free_bytes <= info.total_bytes);
    assert!(info.context_epoch >= 1);
    let name = String::from_utf8_lossy(&info.name);
    eprintln!(
        "device: {} cc {}.{} total {} MiB free {} MiB runtime {} driver {} SMs {} copy engines {}",
        name.trim_end_matches('\0'),
        info.cc_major,
        info.cc_minor,
        info.total_bytes >> 20,
        info.free_bytes >> 20,
        info.runtime_version,
        info.driver_version,
        info.multiprocessors,
        info.async_engines
    );
}

#[test]
fn native_runtime_lifetime_round_trip() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    let rt = native_runtime(small_config());
    let cap = rt.staging_capability().unwrap();
    eprintln!("staging: {cap:?}");
    let values = scalars(10_007, 1); // 80 056 bytes = 20 chunks through 2 slots
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    let d = rt.duplicate_scalars(&h).unwrap();
    assert_eq!(rb(&rt).download_scalars(&d).unwrap(), values);
    let snap = rb(&rt).runtime_snapshot().unwrap();
    assert_eq!(snap.live_bytes, 2 * 80_056);
    assert!(snap.context_healthy);
    rb(&rt).release_scalars(d).unwrap();
    rb(&rt).release_scalars(h).unwrap();
    assert_eq!(rb(&rt).runtime_snapshot().unwrap().live_bytes, 0);
    let t = rt.telemetry();
    assert_eq!(t.poison_events, 0);
    assert_eq!(t.h2d_pinned_bytes + t.h2d_pageable_bytes, 80_056);
    eprintln!("telemetry: {t:?}");
}

#[test]
fn native_pageable_fallback_round_trip() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    let rt = native_runtime(RuntimeConfig {
        pinned_staging: false,
        ..small_config()
    });
    let cap = rt.staging_capability().unwrap();
    assert_eq!((cap.pinned_slots, cap.pageable_slots), (0, 2));
    assert!(!cap.async_overlap_available);
    let values = scalars(3_001, 2);
    let h = rb(&rt).upload_scalars(&values).unwrap();
    assert_eq!(rb(&rt).download_scalars(&h).unwrap(), values);
    rb(&rt).release_scalars(h).unwrap();
    let t = rt.telemetry();
    assert_eq!(t.h2d_pinned_bytes, 0);
    assert_eq!(t.h2d_pageable_bytes, 3_001 * 8);
}

#[test]
fn native_rejections_and_metadata_checks() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    let rt = native_runtime(RuntimeConfig {
        device_budget_cap: Some(1 << 16),
        ..small_config()
    });
    // Budget enforced before the device allocates.
    let too_big = scalars((1 << 13) + 1, 3);
    match rb(&rt).upload_scalars(&too_big) {
        Err(e) => assert_eq!((e.kind, e.effect), (ErrorKind::Capacity, FailureEffect::NotStarted)),
        Ok(_) => panic!("over budget"),
    }
    let h = rb(&rt).upload_scalars(&scalars(16, 4)).unwrap();
    let forged = loki_backend_api::ResidentBuffer::<Toy>::from_untrusted_metadata(
        h.generation(),
        h.allocation(),
        h.len() + 1,
        h.bytes(),
    );
    assert_eq!(
        rb(&rt).download_scalars(&forged).unwrap_err().kind,
        ErrorKind::InvalidInput
    );
    // A healthy generation cannot be reinitialized.
    assert_eq!(rt.reinitialize().unwrap_err().kind, ErrorKind::InvalidInput);
    rb(&rt).release_scalars(h).unwrap();
}

#[test]
fn native_bounds_and_stale_epoch_after_reset() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    let mut st = StatusV1::default();
    let (mut mem, mut host, mut stream, mut ev) =
        (HandleV1::default(), HandleV1::default(), HandleV1::default(), HandleV1::default());
    // SAFETY: valid out-pointers; handles are used only while live.
    unsafe {
        assert_eq!(loki_gpu_rt_device_alloc_v1(256, &mut mem, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_host_alloc_v1(256, 1, &mut host, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_stream_create_v1(&mut stream, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_event_create_v1(0, &mut ev, &mut st), ffi::OK);
        assert_eq!(mem.bytes, 256);
        // Out-of-bounds and overflowing ranges are rejected before enqueue.
        assert_eq!(loki_gpu_rt_memcpy_h2d_v1(&mem, 200, &host, 0, 64, &stream, &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_memcpy_h2d_v1(&mem, 0, &host, u64::MAX, 2, &stream, &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_memcpy_d2h_v1(&host, 0, &mem, 0, 0, &stream, &mut st), ffi::INVALID_ARGUMENT);
        assert_eq!(loki_gpu_rt_launch_copy_v1(&mem, &mem, 257, &stream, &mut st), ffi::INVALID_ARGUMENT);
        // Valid round trip through the raw ABI.
        let hp = host.raw as usize as *mut u8;
        for i in 0..256 {
            *hp.add(i) = i as u8;
        }
        assert_eq!(loki_gpu_rt_memcpy_h2d_v1(&mem, 0, &host, 0, 256, &stream, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_event_record_v1(&ev, &stream, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_event_synchronize_v1(&ev, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_event_query_v1(&ev, &mut st), ffi::OK);
        for i in 0..256 {
            *hp.add(i) = 0;
        }
        assert_eq!(loki_gpu_rt_memcpy_d2h_v1(&host, 0, &mem, 0, 256, &stream, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_stream_synchronize_v1(&stream, &mut st), ffi::OK);
        assert!((0..256).all(|i| *hp.add(i) == i as u8));

        // Release everything, then reset: every handle minted before the
        // reset is stale and is rejected before its bits are interpreted.
        assert_eq!(loki_gpu_rt_event_destroy_v1(&ev, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_stream_destroy_v1(&stream, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_host_free_v1(&host, &mut st), ffi::OK);
        assert_eq!(loki_gpu_rt_device_free_v1(&mem, &mut st), ffi::OK);
        let mut info = DeviceInfoV1::default();
        let before = mem.context_epoch;
        assert_eq!(loki_gpu_rt_reset_v1(0, &mut info, &mut st), ffi::OK);
        assert!(info.context_epoch > before);
        assert_eq!(loki_gpu_rt_device_free_v1(&mem, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_host_free_v1(&host, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_stream_destroy_v1(&stream, &mut st), ffi::STALE_HANDLE);
        assert_eq!(loki_gpu_rt_event_destroy_v1(&ev, &mut st), ffi::STALE_HANDLE);
    }
}

#[test]
fn native_full_slot_transfer_timing() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    // Default two 32 MiB pinned slots; 2^22 elements = 32 MiB per direction.
    let rt = native_runtime(RuntimeConfig {
        device_budget_cap: Some(512 << 20),
        device_headroom_bytes: 0,
        ..RuntimeConfig::default()
    });
    let values = scalars(1 << 22, 5);
    let t0 = Instant::now();
    let h = rb(&rt).upload_scalars(&values).unwrap();
    let up = t0.elapsed();
    let t1 = Instant::now();
    let back = rb(&rt).download_scalars(&h).unwrap();
    let down = t1.elapsed();
    assert_eq!(back, values);
    rb(&rt).release_scalars(h).unwrap();
    let t = rt.telemetry();
    eprintln!(
        "32 MiB upload wall {:?}, download wall {:?}; gpu event total {:.3} ms; event wait {:.3} ms; init {:.3} ms; staging {:?}",
        up,
        down,
        t.gpu_event_ms,
        t.event_wait_ns as f64 / 1e6,
        t.init_ns as f64 / 1e6,
        rt.staging_capability()
    );
}

/// Measurement probe, not a gate: allocate 32 MiB pinned chunks until failure
/// or `LOKI_GPU_PINNED_PROBE_MAX_MIB` (default 1024), then free them all.
#[test]
#[ignore]
fn measure_pinned_budget() {
    let _g = device_lock();
    if probe().is_none() {
        return;
    }
    let max_mib: u64 = std::env::var("LOKI_GPU_PINNED_PROBE_MAX_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);
    let chunk: u64 = 32 << 20;
    let mut held = Vec::new();
    let mut st = StatusV1::default();
    let t0 = Instant::now();
    let mut failure = None;
    while (held.len() as u64 + 1) * (chunk >> 20) <= max_mib {
        let mut h = HandleV1::default();
        // SAFETY: valid out-pointers.
        let code = unsafe { loki_gpu_rt_host_alloc_v1(chunk, 1, &mut h, &mut st) };
        if code != ffi::OK {
            failure = Some((code, st.cuda_error));
            break;
        }
        held.push(h);
    }
    let pinned = held.len() as u64 * chunk;
    let elapsed = t0.elapsed();
    for h in &held {
        // SAFETY: live handles from host_alloc above.
        assert_eq!(unsafe { loki_gpu_rt_host_free_v1(h, &mut st) }, ffi::OK);
    }
    eprintln!(
        "PINNED-PROBE: {} MiB pinned in {} x 32 MiB chunks (cap {} MiB) in {:?}; first failure {:?}",
        pinned >> 20,
        held.len(),
        max_mib,
        elapsed,
        failure
    );
}
