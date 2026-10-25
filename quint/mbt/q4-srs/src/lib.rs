//! quint-q4: model-based replay of Loki's W13 GPU SRS cache.
//!
//! The pinned `src/poly/kzg/msm/cuda/{cuda.rs,backend.rs,convert.rs}` source is
//! compiled unchanged. Without `gpu`, the native sppark adapter and the two
//! CUDA runtime queries are CPU surrogates (src/driver.rs) with fault
//! injection; nothing here runs a GPU kernel. With `gpu`, the pinned
//! `cuda/msm.cu` is linked and the replay runs on the device.
#![allow(dead_code)]

#[cfg(test)]
mod w13 {
    include!(concat!(env!("OUT_DIR"), "/w13.rs"));
}

// CPU-surrogate status type standing in for `sppark::Error`.
#[cfg(all(test, not(feature = "gpu")))]
extern crate self as sppark;
#[cfg(not(feature = "gpu"))]
#[repr(C)]
pub struct Error {
    pub code: i32,
    pub(crate) message: *mut std::ffi::c_char,
}
#[cfg(not(feature = "gpu"))]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "injected backend error {}", self.code)
    }
}
