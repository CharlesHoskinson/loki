//! Narrow, serialized sppark NTT adapter for Midnight's BLS12-381 scalar field.

use std::sync::atomic::{AtomicU64, Ordering};

use blst::blst_fr;
use midnight_curves::Fq;

static COMPLETED: AtomicU64 = AtomicU64::new(0);

extern "C" {
    fn loki_ntt(
        values: *mut blst_fr,
        log_n: u32,
        inverse: bool,
        root: *const blst_fr,
    ) -> sppark::Error;
}

/// Number of successfully completed GPU transforms in this process.
pub fn completed_calls() -> u64 {
    COMPLETED.load(Ordering::Relaxed)
}

/// Transform reduced Montgomery field elements in natural order.
///
/// Inverse transforms include normalization by `1 / values.len()`.
/// `root` must equal the corresponding Midnight/sppark domain root.
/// Errors leave the caller's slice unchanged.
pub fn fft(values: &mut [Fq], root: Fq, inverse: bool) -> Result<(), String> {
    if !values.len().is_power_of_two() || values.len().trailing_zeros() > 28 {
        return Err("cuda-ntt requires a power-of-two length no larger than 2^28".into());
    }
    if std::env::var_os("LOKI_NTT_FORCE_ERROR").is_some() {
        return Err("injected CUDA NTT error".into());
    }
    let _device = loki_cuda_runtime::lock()?;
    // Public conversions preserve Montgomery limbs without relying on Fq layout.
    let mut limbs: Vec<blst_fr> = values.iter().copied().map(Into::into).collect();
    let root: blst_fr = root.into();
    // SAFETY: the FFI accepts exactly `2^log_n` writable blst_fr (4 x u64)
    // values and a live root. sppark uses the same modulus/R and completes all
    // GPU work before returning. The mutex bounds concurrent device allocations.
    let error = unsafe {
        loki_ntt(
            limbs.as_mut_ptr(),
            values.len().trailing_zeros(),
            inverse,
            &root,
        )
    };
    if error.code != 0 {
        return Err(error.to_string());
    }
    for (value, limb) in values.iter_mut().zip(limbs) {
        *value = limb.into();
    }
    COMPLETED.fetch_add(1, Ordering::Relaxed);
    Ok(())
}
