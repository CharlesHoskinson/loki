//! Test-only connection between Loki and executable Quint models.
#[cfg(test)]
mod circuit;
#[cfg(test)]
mod logging;
#[cfg(test)]
mod proof;
#[cfg(test)]
mod transcript_driver;

#[cfg(test)]
mod tests {
    #[test]
    fn real_transcript_families() {
        for profile in 0..4 {
            let (shape, ops, equal) = crate::proof::prove(profile, 1, false, 81 + profile as u64);
            let mut segments = vec![0];
            for op in &ops {
                if op.kind == "squeeze" {
                    segments.push(0);
                } else {
                    *segments.last_mut().unwrap() += 1;
                }
            }
            println!(
                "profile {profile}: {shape:?}; absorb counts {segments:?}; same bytes={equal}"
            );
        }
    }
}

#[cfg(all(test, feature = "w4-source"))]
mod w4 {
    include!(concat!(env!("OUT_DIR"), "/w4.rs"));
}

// CPU-only FFI surrogate's status type; no CUDA runtime is linked by this crate.
#[cfg(all(test, feature = "w4-source"))]
extern crate self as sppark;
#[cfg(all(test, feature = "w4-source"))]
#[repr(C)]
pub struct Error {
    pub code: i32,
    message: *mut std::ffi::c_char,
}
#[cfg(all(test, feature = "w4-source"))]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "injected backend error {}", self.code)
    }
}

#[cfg(all(test, feature = "w6-source"))]
mod util;
#[cfg(all(test, feature = "w6-source"))]
include!(concat!(env!("OUT_DIR"), "/w6.rs"));
