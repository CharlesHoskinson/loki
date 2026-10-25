//! Shared helpers for the bench driver and the stock verifier: digests,
//! process memory observations and monotonic timers. No prover code.

use std::fs;
use std::path::Path;
use std::time::Instant;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub type BoxError = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, BoxError>;

pub fn error(msg: impl std::fmt::Display) -> BoxError {
    std::io::Error::other(msg.to_string()).into()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn file_sha256(path: &Path) -> Result<String> {
    Ok(sha256_hex(&fs::read(path)?))
}

/// Values of selected `/proc/self/status` fields, in bytes. Read while the
/// process is alive only (never after exit).
pub fn status_bytes() -> Value {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let mut out = serde_json::Map::new();
    for line in text.lines() {
        for key in ["VmHWM", "VmRSS", "VmSwap", "Threads"] {
            if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
                let mut it = rest.split_whitespace();
                let n: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let v = if it.next() == Some("kB") { n * 1024 } else { n };
                out.insert(key.to_string(), json!(v));
            }
        }
    }
    Value::Object(out)
}

/// Process-lifetime peak RSS (getrusage ru_maxrss), bytes.
pub fn lifetime_maxrss_bytes() -> u64 {
    // SAFETY: getrusage writes into a zeroed, correctly sized struct.
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            usage.ru_maxrss as u64 * 1024
        } else {
            0
        }
    }
}

/// User and system CPU seconds of this process so far.
pub fn cpu_seconds() -> (f64, f64) {
    // SAFETY: as above.
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return (0.0, 0.0);
        }
        let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 / 1e6;
        (t(u.ru_utime), t(u.ru_stime))
    }
}

/// Non-overlapping named windows inside one stage. Each `lap` closes the
/// current window; windows never nest, so their sum equals the stage wall.
pub struct Windows {
    start: Instant,
    last: Instant,
    entries: Vec<(String, f64)>,
}

impl Windows {
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last: now,
            entries: Vec::new(),
        }
    }

    pub fn lap(&mut self, name: &str) {
        let now = Instant::now();
        self.entries
            .push((name.to_string(), (now - self.last).as_secs_f64() * 1e3));
        self.last = now;
    }

    pub fn total_ms(&self) -> f64 {
        (self.last - self.start).as_secs_f64() * 1e3
    }

    pub fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.entries {
            let prior = m.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            m.insert(k.clone(), json!(prior + v));
        }
        Value::Object(m)
    }
}

/// Atomic publication: write into `<dir>.partial`, then rename to `dir`.
pub fn publish_dir(dir: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    if dir.exists() {
        return Err(error(format!("refusing to overwrite {}", dir.display())));
    }
    let partial = dir.with_extension("partial");
    if partial.exists() {
        fs::remove_dir_all(&partial)?;
    }
    write(&partial)?;
    fs::rename(&partial, dir)?;
    Ok(())
}
