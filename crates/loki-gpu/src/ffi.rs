//! Private C ABI declarations shared by the operation modules.
//!
//! The v1 symbols are synchronous Unsupported stubs. Public transfer methods
//! block until completion. Any future internal asynchronous ABI must carry
//! context, stream, and owned staging/completion state explicitly.

#![allow(dead_code)] // The v1 symbols are exercised only by the CUDA smoke check.

pub(crate) const OK: i32 = 0;
pub(crate) const UNSUPPORTED: i32 = 1;
pub(crate) const INVALID_ARGUMENT: i32 = 2;
pub(crate) const RUNTIME_ERROR: i32 = 3;

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
