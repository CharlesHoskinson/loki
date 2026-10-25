//! quint-q2 model-based tests (test-only; production code is unchanged).
#[cfg(test)]
mod dispatch_driver;
#[cfg(test)]
mod bug_repros;

// Legacy sppark host adapters at a256a9a, compiled with CPU FFI surrogates.
#[cfg(all(test, feature = "cuda"))]
#[allow(dead_code, unused_imports, unexpected_cfgs, clippy::all)]
mod legacy {
    include!(concat!(env!("OUT_DIR"), "/legacy.rs"));
}

// Status type of the CPU-only FFI surrogates; no CUDA runtime is linked.
#[cfg(all(test, feature = "cuda"))]
extern crate self as sppark;
#[cfg(all(test, feature = "cuda"))]
#[repr(C)]
pub struct Error {
    pub code: i32,
    message: *mut std::ffi::c_char,
}
#[cfg(all(test, feature = "cuda"))]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "injected backend error {}", self.code)
    }
}
