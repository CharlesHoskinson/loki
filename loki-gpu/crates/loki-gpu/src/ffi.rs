//! C ABI shared with `cuda/` (header `cuda/include/loki_gpu_ffi.h`).
//!
//! * The v1 operation symbols (`loki_gpu_{msm,ntt,quotient}_v1`) remain
//!   synchronous Unsupported stubs until their packages land.
//! * The P2 runtime symbols (`loki_gpu_rt_*`) implement the device driver used
//!   by [`crate::runtime::GpuRuntime`]. Their handles are versioned
//!   (`abi_version`, `kind`, `context_epoch`) and every call returns an
//!   explicit status. Raw handle bits never leave this crate.
//!
//! The `#[repr(C)]` mirrors below are checked against the native layout by
//! `tests/ffi_abi.rs` and by the compile-time assertions in this module.
//!
//! The native driver is crate-private: the only way to reach the device is
//! `GpuRuntime::native` (single owner per process) or `GpuRuntime::process`.
//! None of the following compile outside the crate:
//!
//! ```compile_fail
//! let _ = loki_gpu::ffi::native_driver();
//! ```
//!
//! ```compile_fail
//! let _ = loki_gpu::ffi::NativeDriver::default();
//! ```
//!
//! Driver handles cannot be fabricated from raw bits either:
//!
//! ```compile_fail
//! fn forge(h: loki_gpu::runtime::DriverHandle) -> loki_gpu::runtime::DeviceMemory {
//!     loki_gpu::runtime::DeviceMemory(h)
//! }
//! ```
//!
//! ```compile_fail
//! fn raw(h: &loki_gpu::runtime::DriverHandle) -> u64 {
//!     h.raw
//! }
//! ```

#![allow(dead_code)] // Stub symbols are exercised only by the CUDA tests.

pub const OK: i32 = 0;
pub const UNSUPPORTED: i32 = 1;
pub const INVALID_ARGUMENT: i32 = 2;
pub const RUNTIME_ERROR: i32 = 3;
pub const OUT_OF_MEMORY: i32 = 4;
pub const NOT_READY: i32 = 5;
pub const STALE_HANDLE: i32 = 6;
pub const CONTEXT_LOST: i32 = 7;
pub const ABI_MISMATCH: i32 = 8;
pub const NO_DEVICE: i32 = 9;

pub const RT_ABI_VERSION: u32 = 1;

pub const HANDLE_DEVICE_MEMORY: u32 = 1;
pub const HANDLE_HOST_PINNED: u32 = 2;
pub const HANDLE_HOST_PAGEABLE: u32 = 3;
pub const HANDLE_STREAM: u32 = 4;
pub const HANDLE_EVENT: u32 = 5;

/// Mirror of `loki_gpu_handle_v1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HandleV1 {
    pub abi_version: u32,
    pub kind: u32,
    pub context_epoch: u64,
    pub raw: u64,
    pub bytes: u64,
}

/// Mirror of `loki_gpu_status_v1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StatusV1 {
    pub code: i32,
    pub cuda_error: i32,
    pub sticky: u32,
    pub reserved: u32,
}

/// Mirror of `loki_gpu_device_info_v1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceInfoV1 {
    pub abi_version: u32,
    pub ordinal: i32,
    pub cc_major: i32,
    pub cc_minor: i32,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub context_epoch: u64,
    pub runtime_version: i32,
    pub driver_version: i32,
    pub multiprocessors: i32,
    pub async_engines: i32,
    pub name: [u8; 64],
}

impl Default for DeviceInfoV1 {
    fn default() -> Self {
        Self {
            abi_version: 0,
            ordinal: 0,
            cc_major: 0,
            cc_minor: 0,
            total_bytes: 0,
            free_bytes: 0,
            context_epoch: 0,
            runtime_version: 0,
            driver_version: 0,
            multiprocessors: 0,
            async_engines: 0,
            name: [0; 64],
        }
    }
}

/// Mirror of `loki_gpu_abi_layout_v1`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AbiLayoutV1 {
    pub abi_version: u32,
    pub handle_size: u32,
    pub handle_align: u32,
    pub handle_offset_context_epoch: u32,
    pub handle_offset_raw: u32,
    pub handle_offset_bytes: u32,
    pub status_size: u32,
    pub status_offset_sticky: u32,
    pub device_info_size: u32,
    pub device_info_offset_total_bytes: u32,
    pub device_info_offset_context_epoch: u32,
    pub device_info_offset_name: u32,
    pub layout_size: u32,
    pub reserved: u32,
}

// Compile-time size/alignment assertions, matching the C++ static_asserts in
// the header. Field offsets are compared with the native layout at test time
// (offset_of! needs Rust 1.77; the workspace MSRV is 1.76).
const _: () = {
    assert!(core::mem::size_of::<HandleV1>() == 32);
    assert!(core::mem::align_of::<HandleV1>() == 8);
    assert!(core::mem::size_of::<StatusV1>() == 16);
    assert!(core::mem::size_of::<DeviceInfoV1>() == 120);
    assert!(core::mem::size_of::<AbiLayoutV1>() == 56);
};

#[cfg(feature = "cuda")]
#[allow(unsafe_code)]
extern "C" {
    pub(crate) fn loki_gpu_msm_v1(
        scalars: *const u8,
        scalar_bytes: usize,
        bases: *const u8,
        base_bytes: usize,
        out: *mut u8,
        out_bytes: usize,
    ) -> i32;

    pub(crate) fn loki_gpu_ntt_v1(
        input: *const u8,
        input_bytes: usize,
        out: *mut u8,
        out_bytes: usize,
        params: *const u8,
        params_bytes: usize,
    ) -> i32;

    pub(crate) fn loki_gpu_quotient_v1(
        input: *const u8,
        input_bytes: usize,
        out: *mut u8,
        out_bytes: usize,
        program: *const u8,
        program_bytes: usize,
    ) -> i32;
}

#[cfg(feature = "cuda")]
#[allow(unsafe_code)]
extern "C" {
    pub(crate) fn loki_gpu_rt_abi_version() -> u32;
    pub(crate) fn loki_gpu_rt_abi_layout_v1(out: *mut AbiLayoutV1) -> i32;
    pub(crate) fn loki_gpu_rt_probe_v1(ordinal: i32, out: *mut DeviceInfoV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_reset_v1(ordinal: i32, out: *mut DeviceInfoV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_stream_create_v1(out: *mut HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_stream_destroy_v1(stream: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_event_create_v1(timing: u32, out: *mut HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_event_destroy_v1(event: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_device_alloc_v1(bytes: u64, out: *mut HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_device_free_v1(memory: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_host_alloc_v1(bytes: u64, pinned: u32, out: *mut HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_host_free_v1(buffer: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_memcpy_h2d_v1(
        dst: *const HandleV1,
        dst_offset: u64,
        src_host: *const HandleV1,
        src_offset: u64,
        len: u64,
        stream: *const HandleV1,
        status: *mut StatusV1,
    ) -> i32;
    pub(crate) fn loki_gpu_rt_memcpy_d2h_v1(
        dst_host: *const HandleV1,
        dst_offset: u64,
        src: *const HandleV1,
        src_offset: u64,
        len: u64,
        stream: *const HandleV1,
        status: *mut StatusV1,
    ) -> i32;
    pub(crate) fn loki_gpu_rt_launch_copy_v1(
        dst: *const HandleV1,
        src: *const HandleV1,
        bytes: u64,
        stream: *const HandleV1,
        status: *mut StatusV1,
    ) -> i32;
    pub(crate) fn loki_gpu_rt_event_record_v1(event: *const HandleV1, stream: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_stream_wait_event_v1(stream: *const HandleV1, event: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_event_synchronize_v1(event: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_event_query_v1(event: *const HandleV1, status: *mut StatusV1) -> i32;
    pub(crate) fn loki_gpu_rt_event_elapsed_ms_v1(
        start: *const HandleV1,
        end: *const HandleV1,
        ms: *mut f32,
        status: *mut StatusV1,
    ) -> i32;
    pub(crate) fn loki_gpu_rt_stream_synchronize_v1(stream: *const HandleV1, status: *mut StatusV1) -> i32;
}

/// Native layout as reported by the linked CUDA translation unit.
#[cfg(feature = "cuda")]
pub fn native_abi_layout() -> (u32, i32, AbiLayoutV1) {
    let mut layout = AbiLayoutV1::default();
    // SAFETY: both functions only read/write the provided out-pointer, which
    // points to a live, correctly laid out AbiLayoutV1.
    let (version, status) = unsafe { (loki_gpu_rt_abi_version(), loki_gpu_rt_abi_layout_v1(&mut layout)) };
    (version, status, layout)
}

#[cfg(feature = "cuda")]
pub(crate) use native::native_driver;

#[cfg(feature = "cuda")]
mod native {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::runtime::{
        DeviceDriver, DeviceInfo, DeviceMemory, DriverCall, DriverError, DriverErrorClass,
        DriverHandle, EventId, HandleKind, HostBuffer, StreamId,
    };

    /// CUDA runtime driver over the `loki_gpu_rt_*` ABI. Crate-private: it is
    /// only reachable through `GpuRuntime::native`, which enforces a single
    /// owner per process. The driver additionally records every handle it
    /// minted and refuses (as `Poisoned`) any handle it did not mint or has
    /// already destroyed, so even an internal bug cannot pass a foreign or
    /// freed raw pointer to CUDA.
    #[derive(Debug, Default)]
    pub(crate) struct NativeDriver {
        minted: Mutex<HashSet<(u32, u64)>>,
    }

    /// Fresh native driver instance (crate-private; see `GpuRuntime::native`).
    pub(crate) fn native_driver() -> Arc<dyn DeviceDriver> {
        Arc::new(NativeDriver::default())
    }

    fn key(h: &DriverHandle) -> (u32, u64) {
        (kind_code(h.kind), h.raw)
    }

    fn unknown(call: DriverCall) -> DriverError {
        DriverError {
            call,
            class: DriverErrorClass::Poisoned,
            code: STALE_HANDLE,
        }
    }

    impl NativeDriver {
        fn set(&self) -> std::sync::MutexGuard<'_, HashSet<(u32, u64)>> {
            // A poisoned registry mutex still holds a consistent set: every
            // mutation is a single insert/remove/clear.
            self.minted.lock().unwrap_or_else(|p| p.into_inner())
        }

        fn mint(&self, h: DriverHandle) -> DriverHandle {
            self.set().insert(key(&h));
            h
        }

        fn known(&self, call: DriverCall, handles: &[&DriverHandle]) -> Result<(), DriverError> {
            let set = self.set();
            if handles.iter().all(|h| set.contains(&key(h))) {
                Ok(())
            } else {
                Err(unknown(call))
            }
        }

        /// Destroy a minted handle: forgotten only once native destruction
        /// succeeded (a failed destroy leaves it quarantined, still known).
        fn retire(
            &self,
            call: DriverCall,
            h: &DriverHandle,
            destroy: impl FnOnce() -> Result<(), DriverError>,
        ) -> Result<(), DriverError> {
            self.known(call, &[h])?;
            destroy()?;
            self.set().remove(&key(h));
            Ok(())
        }
    }

    fn kind_code(kind: HandleKind) -> u32 {
        kind as u32
    }

    fn kind_from(code: u32) -> Option<HandleKind> {
        Some(match code {
            HANDLE_DEVICE_MEMORY => HandleKind::DeviceMemory,
            HANDLE_HOST_PINNED => HandleKind::HostPinned,
            HANDLE_HOST_PAGEABLE => HandleKind::HostPageable,
            HANDLE_STREAM => HandleKind::Stream,
            HANDLE_EVENT => HandleKind::Event,
            _ => return None,
        })
    }

    fn to_c(h: &DriverHandle) -> HandleV1 {
        HandleV1 {
            abi_version: RT_ABI_VERSION,
            kind: kind_code(h.kind),
            context_epoch: h.epoch,
            raw: h.raw,
            bytes: h.bytes,
        }
    }

    fn from_c(call: DriverCall, h: &HandleV1, expect: HandleKind) -> Result<DriverHandle, DriverError> {
        match kind_from(h.kind) {
            Some(k) if k == expect && h.abi_version == RT_ABI_VERSION && h.raw != 0 => Ok(DriverHandle {
                kind: k,
                epoch: h.context_epoch,
                raw: h.raw,
                bytes: h.bytes,
            }),
            _ => Err(DriverError {
                call,
                class: DriverErrorClass::Poisoned,
                code: ABI_MISMATCH,
            }),
        }
    }

    /// Map a native status to the runtime's error classes.
    pub(crate) fn classify(call: DriverCall, code: i32, status: &StatusV1) -> Result<(), DriverError> {
        let class = match code {
            OK => return Ok(()),
            OUT_OF_MEMORY | RUNTIME_ERROR | INVALID_ARGUMENT | NOT_READY => DriverErrorClass::Transient,
            UNSUPPORTED | NO_DEVICE => DriverErrorClass::Unsupported,
            CONTEXT_LOST => DriverErrorClass::ContextLost,
            // A stale or malformed handle reaching native code breaks a
            // runtime invariant: treat the context as untrusted.
            STALE_HANDLE | ABI_MISMATCH => DriverErrorClass::Poisoned,
            _ => DriverErrorClass::Poisoned,
        };
        let class = if status.sticky != 0 && class == DriverErrorClass::Transient {
            DriverErrorClass::Poisoned
        } else {
            class
        };
        Err(DriverError {
            call,
            class,
            code: if status.cuda_error != 0 { status.cuda_error } else { code },
        })
    }

    fn info_from(v: &DeviceInfoV1) -> DeviceInfo {
        let end = v.name.iter().position(|&b| b == 0).unwrap_or(v.name.len());
        DeviceInfo {
            ordinal: v.ordinal.max(0) as u32,
            compute_capability: (v.cc_major.max(0) as u32, v.cc_minor.max(0) as u32),
            total_bytes: v.total_bytes,
            free_bytes: v.free_bytes,
            context_epoch: v.context_epoch,
            name: String::from_utf8_lossy(&v.name[..end]).into_owned(),
        }
    }

    macro_rules! ffi_call {
        ($call:expr, |$st:ident| $body:expr) => {{
            let mut $st = StatusV1::default();
            // SAFETY: every pointer argument references a live local or a
            // handle mirror owned by this call; the native side validates
            // handles (version, kind, epoch, bounds) before dereferencing.
            let code = unsafe { $body };
            classify($call, code, &$st)
        }};
    }

    fn ordinal(call: DriverCall, ordinal: u32) -> Result<i32, DriverError> {
        i32::try_from(ordinal).map_err(|_| DriverError {
            call,
            class: DriverErrorClass::Unsupported,
            code: INVALID_ARGUMENT,
        })
    }

    // SAFETY: host buffers come from cudaHostAlloc/malloc of exactly `len`
    // bytes and stay valid until host_free (or cudaDeviceReset for pinned
    // memory, after which the runtime never touches them). Copies are
    // bounds-checked natively. Event/stream synchronization is CUDA's.
    unsafe impl DeviceDriver for NativeDriver {
        fn probe(&self, ord: u32) -> Result<DeviceInfo, DriverError> {
            let call = DriverCall::Probe;
            let o = ordinal(call, ord)?;
            let mut info = DeviceInfoV1::default();
            ffi_call!(call, |st| loki_gpu_rt_probe_v1(o, &mut info, &mut st))?;
            Ok(info_from(&info))
        }

        fn reset(&self, ord: u32) -> Result<DeviceInfo, DriverError> {
            let call = DriverCall::Reset;
            let o = ordinal(call, ord)?;
            let mut info = DeviceInfoV1::default();
            // Reset invalidates every handle, whatever its outcome.
            self.set().clear();
            ffi_call!(call, |st| loki_gpu_rt_reset_v1(o, &mut info, &mut st))?;
            Ok(info_from(&info))
        }

        fn create_stream(&self, non_blocking: bool) -> Result<StreamId, DriverError> {
            let call = DriverCall::StreamCreate;
            // The native ABI only creates non-blocking streams.
            debug_assert!(non_blocking);
            let mut h = HandleV1::default();
            ffi_call!(call, |st| loki_gpu_rt_stream_create_v1(&mut h, &mut st))?;
            Ok(StreamId(self.mint(from_c(call, &h, HandleKind::Stream)?)))
        }

        fn destroy_stream(&self, stream: StreamId) -> Result<(), DriverError> {
            let call = DriverCall::StreamDestroy;
            let h = to_c(&stream.0);
            self.retire(call, &stream.0, || ffi_call!(call, |st| loki_gpu_rt_stream_destroy_v1(&h, &mut st)))
        }

        fn create_event(&self, timing: bool) -> Result<EventId, DriverError> {
            let call = DriverCall::EventCreate;
            let mut h = HandleV1::default();
            ffi_call!(call, |st| loki_gpu_rt_event_create_v1(u32::from(timing), &mut h, &mut st))?;
            Ok(EventId(self.mint(from_c(call, &h, HandleKind::Event)?)))
        }

        fn destroy_event(&self, event: EventId) -> Result<(), DriverError> {
            let call = DriverCall::EventDestroy;
            let h = to_c(&event.0);
            self.retire(call, &event.0, || ffi_call!(call, |st| loki_gpu_rt_event_destroy_v1(&h, &mut st)))
        }

        fn device_alloc(&self, bytes: u64) -> Result<DeviceMemory, DriverError> {
            let call = DriverCall::DeviceAlloc;
            let mut h = HandleV1::default();
            ffi_call!(call, |st| loki_gpu_rt_device_alloc_v1(bytes, &mut h, &mut st))?;
            Ok(DeviceMemory(self.mint(from_c(call, &h, HandleKind::DeviceMemory)?)))
        }

        fn device_free(&self, memory: DeviceMemory) -> Result<(), DriverError> {
            let call = DriverCall::DeviceFree;
            let h = to_c(&memory.0);
            self.retire(call, &memory.0, || ffi_call!(call, |st| loki_gpu_rt_device_free_v1(&h, &mut st)))
        }

        fn host_alloc(&self, bytes: usize, pinned: bool) -> Result<HostBuffer, DriverError> {
            let call = DriverCall::HostAlloc;
            let mut h = HandleV1::default();
            ffi_call!(call, |st| loki_gpu_rt_host_alloc_v1(bytes as u64, u32::from(pinned), &mut h, &mut st))?;
            let kind = if pinned { HandleKind::HostPinned } else { HandleKind::HostPageable };
            let handle = self.mint(from_c(call, &h, kind)?);
            Ok(HostBuffer {
                handle,
                ptr: handle.raw as usize as *mut u8,
                len: bytes,
            })
        }

        fn host_free(&self, buffer: HostBuffer) -> Result<(), DriverError> {
            let call = DriverCall::HostFree;
            let h = to_c(&buffer.handle);
            self.retire(call, &buffer.handle, || ffi_call!(call, |st| loki_gpu_rt_host_free_v1(&h, &mut st)))
        }

        fn copy_h2d(
            &self,
            dst: DeviceMemory,
            dst_offset: u64,
            src: &HostBuffer,
            src_offset: usize,
            len: usize,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            self.known(DriverCall::CopyH2D, &[&dst.0, &src.handle, &stream.0])?;
            let (d, s, q) = (to_c(&dst.0), to_c(&src.handle), to_c(&stream.0));
            ffi_call!(DriverCall::CopyH2D, |st| loki_gpu_rt_memcpy_h2d_v1(
                &d, dst_offset, &s, src_offset as u64, len as u64, &q, &mut st
            ))
        }

        fn copy_d2h(
            &self,
            dst: &HostBuffer,
            dst_offset: usize,
            src: DeviceMemory,
            src_offset: u64,
            len: usize,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            self.known(DriverCall::CopyD2H, &[&dst.handle, &src.0, &stream.0])?;
            let (d, s, q) = (to_c(&dst.handle), to_c(&src.0), to_c(&stream.0));
            ffi_call!(DriverCall::CopyD2H, |st| loki_gpu_rt_memcpy_d2h_v1(
                &d, dst_offset as u64, &s, src_offset, len as u64, &q, &mut st
            ))
        }

        fn launch_copy(
            &self,
            dst: DeviceMemory,
            src: DeviceMemory,
            bytes: u64,
            stream: StreamId,
        ) -> Result<(), DriverError> {
            self.known(DriverCall::Launch, &[&dst.0, &src.0, &stream.0])?;
            let (d, s, q) = (to_c(&dst.0), to_c(&src.0), to_c(&stream.0));
            ffi_call!(DriverCall::Launch, |st| loki_gpu_rt_launch_copy_v1(&d, &s, bytes, &q, &mut st))
        }

        fn record_event(&self, event: EventId, stream: StreamId) -> Result<(), DriverError> {
            self.known(DriverCall::EventRecord, &[&event.0, &stream.0])?;
            let (e, q) = (to_c(&event.0), to_c(&stream.0));
            ffi_call!(DriverCall::EventRecord, |st| loki_gpu_rt_event_record_v1(&e, &q, &mut st))
        }

        fn stream_wait_event(&self, stream: StreamId, event: EventId) -> Result<(), DriverError> {
            self.known(DriverCall::StreamWait, &[&stream.0, &event.0])?;
            let (q, e) = (to_c(&stream.0), to_c(&event.0));
            ffi_call!(DriverCall::StreamWait, |st| loki_gpu_rt_stream_wait_event_v1(&q, &e, &mut st))
        }

        fn event_synchronize(&self, event: EventId) -> Result<(), DriverError> {
            self.known(DriverCall::EventSync, &[&event.0])?;
            let e = to_c(&event.0);
            ffi_call!(DriverCall::EventSync, |st| loki_gpu_rt_event_synchronize_v1(&e, &mut st))
        }

        fn event_query(&self, event: EventId) -> Result<bool, DriverError> {
            self.known(DriverCall::EventQuery, &[&event.0])?;
            let e = to_c(&event.0);
            let mut st = StatusV1::default();
            // SAFETY: as in ffi_call!.
            let code = unsafe { loki_gpu_rt_event_query_v1(&e, &mut st) };
            if code == NOT_READY {
                return Ok(false);
            }
            classify(DriverCall::EventQuery, code, &st).map(|()| true)
        }

        fn event_elapsed_ms(&self, start: EventId, end: EventId) -> Result<f32, DriverError> {
            self.known(DriverCall::EventElapsed, &[&start.0, &end.0])?;
            let (a, b) = (to_c(&start.0), to_c(&end.0));
            let mut ms = 0f32;
            ffi_call!(DriverCall::EventElapsed, |st| loki_gpu_rt_event_elapsed_ms_v1(&a, &b, &mut ms, &mut st))?;
            Ok(ms)
        }

        fn stream_synchronize(&self, stream: StreamId) -> Result<(), DriverError> {
            self.known(DriverCall::StreamSync, &[&stream.0])?;
            let q = to_c(&stream.0);
            ffi_call!(DriverCall::StreamSync, |st| loki_gpu_rt_stream_synchronize_v1(&q, &mut st))
        }
    }
}

#[cfg(all(test, feature = "cuda"))]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn all_unsupported_stubs_leave_output_untouched() {
        let mut msm_out = [0xa5_u8; 16];
        let msm_before = msm_out;
        // SAFETY: the CUDA v1 ABI contract requires Unsupported before any
        // input read or output write. Null/nonempty inputs intentionally check
        // that contract; the stub must not dereference them.
        let status = unsafe {
            loki_gpu_msm_v1(
                ptr::null(),
                32,
                ptr::null(),
                96,
                msm_out.as_mut_ptr(),
                msm_out.len(),
            )
        };
        assert_eq!(status, UNSUPPORTED);
        assert_eq!(msm_out, msm_before);

        let mut ntt_out = [0x5a_u8; 24];
        let ntt_before = ntt_out;
        // SAFETY: same v1 Unsupported/no-touch contract as above.
        let status = unsafe {
            loki_gpu_ntt_v1(
                ptr::null(),
                64,
                ntt_out.as_mut_ptr(),
                ntt_out.len(),
                ptr::null(),
                16,
            )
        };
        assert_eq!(status, UNSUPPORTED);
        assert_eq!(ntt_out, ntt_before);

        let mut quotient_out = [0x3c_u8; 32];
        let quotient_before = quotient_out;
        // SAFETY: same v1 Unsupported/no-touch contract as above.
        let status = unsafe {
            loki_gpu_quotient_v1(
                ptr::null(),
                128,
                quotient_out.as_mut_ptr(),
                quotient_out.len(),
                ptr::null(),
                48,
            )
        };
        assert_eq!(status, UNSUPPORTED);
        assert_eq!(quotient_out, quotient_before);
    }
}
