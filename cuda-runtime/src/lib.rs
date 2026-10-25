//! Process-wide serialization for the MSM and NTT sppark adapters.
use std::sync::{Mutex, MutexGuard};

static DEVICE: Mutex<()> = Mutex::new(());

/// Hold across every sppark call, including resident allocation/destruction.
/// Acquire this guard before any adapter-local cache lock.
///
/// Serialize the FFI operations abstracted by quint/gpu_dispatch.qnt. Keeping
/// the mutex in a common dependency also covers direct loki-cuda-ntt callers.
pub fn lock() -> Result<MutexGuard<'static, ()>, String> {
    DEVICE.lock().map_err(|_| "CUDA device lock poisoned".to_owned())
}
