//! Host observations and exclusion: flock host locks, /proc readers, the
//! idle gate, cancellation signals and process-group inspection.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::json::J;

pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- signals

static CANCEL: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    CANCEL.store(sig, Ordering::SeqCst);
}

/// Install SIGTERM/SIGINT/SIGHUP handlers WITHOUT SA_RESTART, so a blocking
/// flock or wait returns EINTR and the cancellation flag is observed.
pub fn install_cancel_handlers() {
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        // SAFETY: plain sigaction with an async-signal-safe handler.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = 0;
            libc::sigaction(sig, &sa, std::ptr::null_mut());
        }
    }
}

pub fn cancelled() -> Option<i32> {
    match CANCEL.load(Ordering::SeqCst) {
        0 => None,
        s => Some(s),
    }
}

/// Sleep in small steps, returning early on cancellation.
pub fn nap(d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end && cancelled().is_none() {
        std::thread::sleep(Duration::from_millis(20).min(end - Instant::now()));
    }
}

// ---------------------------------------------------------------- locks

pub fn lock_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
    home.join(".cache/doctorwatson")
}

pub fn inode(path: &Path) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::metadata(path)?.ino())
}

/// PIDs holding a FLOCK on the inode (from /proc/locks; blocked waiters,
/// shown with `->`, are excluded).
pub fn flock_holders(ino: u64) -> Vec<i32> {
    let text = fs::read_to_string("/proc/locks").unwrap_or_default();
    let needle = format!(":{ino}");
    text.lines()
        .filter(|l| !l.contains("->"))
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // "N: FLOCK ADVISORY WRITE PID MAJ:MIN:INO START END"
            if f.len() >= 6 && f[1] == "FLOCK" && f[5].ends_with(&needle) {
                f[4].parse().ok()
            } else {
                None
            }
        })
        .collect()
}

pub fn ppid_of(pid: i32) -> Option<i32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

pub fn ancestors() -> Vec<i32> {
    let mut out = Vec::new();
    let mut p = std::process::id() as i32;
    while let Some(pp) = ppid_of(p) {
        if pp <= 1 {
            break;
        }
        out.push(pp);
        p = pp;
    }
    out
}

pub fn cmdline(pid: i32) -> String {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|b| {
            b.split(|c| *c == 0)
                .filter(|s| !s.is_empty())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// A held exclusive flock; released (and receipted) on drop or `release`.
pub struct HostLock {
    #[allow(dead_code)]
    pub name: &'static str,
    file: Option<File>,
    pub receipt: J,
}

impl HostLock {
    /// Blocking exclusive flock. Returns Ok(None) if cancelled while queued.
    pub fn acquire(name: &'static str, path: &Path) -> io::Result<Option<Self>> {
        fs::create_dir_all(path.parent().unwrap_or(Path::new("/")))?;
        let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
        let requested = unix_ms();
        let t = Instant::now();
        let busy_at_request = flock_holders(inode(path)?);
        loop {
            // SAFETY: flock on an owned, open descriptor.
            let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if rc == 0 {
                break;
            }
            let err = io::Error::last_os_error();
            if err.kind() != io::ErrorKind::Interrupted {
                return Err(err);
            }
            if cancelled().is_some() {
                return Ok(None);
            }
        }
        let receipt = J::obj()
            .with("lock", name)
            .with("path", path.display().to_string())
            .with("mode", "owned")
            .with("requested_unix_ms", requested)
            .with("acquired_unix_ms", unix_ms())
            .with("queue_ms", t.elapsed().as_secs_f64() * 1e3)
            .with(
                "holders_at_request",
                J::Arr(busy_at_request.into_iter().map(J::from).collect()),
            )
            .with("holder_pid", std::process::id());
        Ok(Some(Self {
            name,
            file: Some(file),
            receipt,
        }))
    }

    /// Assert that heavy.lock is held by one of our ancestors (DoctorWatson
    /// `heavy` holds it while it runs us). Returns the receipt.
    pub fn assert_held_by_ancestor(name: &'static str, path: &Path) -> Result<Self, String> {
        let ino = inode(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let holders = flock_holders(ino);
        let anc = ancestors();
        let holder = holders
            .iter()
            .copied()
            .find(|h| anc.contains(h))
            .ok_or_else(|| format!("{name}.lock is not held by an ancestor (holders {holders:?}, ancestors {anc:?})"))?;
        let receipt = J::obj()
            .with("lock", name)
            .with("path", path.display().to_string())
            .with("mode", "held-by-ancestor")
            .with("verified_unix_ms", unix_ms())
            .with("holder_pid", holder)
            .with("holder_cmdline", cmdline(holder));
        Ok(Self {
            name,
            file: None,
            receipt,
        })
    }

    pub fn release(mut self) -> J {
        self.unlock();
        self.receipt.clone()
    }

    fn unlock(&mut self) {
        if let Some(f) = self.file.take() {
            // SAFETY: unlocking our own descriptor; close follows on drop.
            unsafe {
                libc::flock(f.as_raw_fd(), libc::LOCK_UN);
            }
            self.receipt.set("released_unix_ms", unix_ms());
        }
    }
}

impl Drop for HostLock {
    fn drop(&mut self) {
        self.unlock();
    }
}

// ---------------------------------------------------------------- /proc

pub fn meminfo() -> J {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut j = J::obj();
    for line in text.lines() {
        for key in ["MemTotal", "MemAvailable", "SwapTotal", "SwapFree"] {
            if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
                let kb: u64 = rest.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0);
                j.set(&format!("{key}_bytes"), kb * 1024);
            }
        }
    }
    j
}

/// (pswpin, pswpout) page counters from /proc/vmstat.
pub fn swap_counters() -> (u64, u64) {
    let text = fs::read_to_string("/proc/vmstat").unwrap_or_default();
    let get = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(k).and_then(|r| r.trim().parse().ok()))
            .unwrap_or(0)
    };
    (get("pswpin "), get("pswpout "))
}

pub fn psi_cpu() -> String {
    fs::read_to_string("/proc/pressure/cpu").unwrap_or_default().trim().to_string()
}

/// Selected /proc/PID/status fields (bytes for memory), or None if gone.
pub fn proc_status(pid: i32) -> Option<(u64, u64, u64, u64)> {
    let text = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let kb = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|r| r.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or(0)
    };
    Some((kb("VmRSS:") * 1024, kb("VmHWM:") * 1024, kb("VmSwap:") * 1024, kb("Threads:")))
}

/// PIDs whose process group is `pgid`.
pub fn group_members(pgid: i32) -> Vec<i32> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(i) = stat.rfind(')') {
                    let f: Vec<&str> = stat[i + 2..].split_whitespace().collect();
                    if f.get(2).and_then(|g| g.parse::<i32>().ok()) == Some(pgid) {
                        out.push(pid);
                    }
                }
            }
        }
    }
    out
}

pub fn allowed_cpus() -> Vec<u32> {
    let text = fs::read_to_string("/proc/self/status").unwrap_or_default();
    text.lines()
        .find_map(|l| l.strip_prefix("Cpus_allowed_list:"))
        .and_then(|v| crate::manifest::parse_mask(v.trim()).ok())
        .unwrap_or_default()
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| format!("sha256sum: {e}"))?;
    if !out.status.success() {
        return Err(format!("sha256sum {} failed", path.display()));
    }
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| "sha256sum: no output".into())
}

pub fn command_text(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|e| format!("unavailable: {e}"))
}

// ---------------------------------------------------------------- idle gate

/// Per-CPU (busy, total) jiffies from /proc/stat; index 0 is the aggregate.
fn cpu_jiffies() -> Vec<(Option<u32>, u64, u64)> {
    let text = fs::read_to_string("/proc/stat").unwrap_or_default();
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let name = f.next()?;
            let id = name.strip_prefix("cpu").and_then(|n| n.parse::<u32>().ok());
            let v: Vec<u64> = f.filter_map(|x| x.parse().ok()).collect();
            if v.len() < 8 {
                return None;
            }
            // user nice system idle iowait irq softirq steal
            let idle = v[3] + v[4];
            let total: u64 = v[..8].iter().sum();
            Some((id, total - idle, total))
        })
        .collect()
}

/// utime+stime ticks of every live process.
fn proc_ticks() -> Vec<(i32, u64)> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else {
                continue;
            };
            if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(i) = stat.rfind(')') {
                    let f: Vec<&str> = stat[i + 2..].split_whitespace().collect();
                    let t = |n: usize| f.get(n).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                    out.push((pid, t(11) + t(12)));
                }
            }
        }
    }
    out
}

pub struct GateParams<'a> {
    pub window: Duration,
    pub selected_cpus: &'a [u32],
    /// PIDs excluded from external-interference accounting (controller,
    /// ancestors, sampler, the paused arm process group).
    pub exempt: Vec<i32>,
    pub nvml_csv: Option<&'a Path>,
}

/// One idle-gate observation window. Returns (passed, trace).
pub fn idle_window(p: &GateParams) -> (bool, J) {
    let started = unix_ms();
    let jiffies0 = cpu_jiffies();
    let ticks0 = proc_ticks();
    let swap0 = swap_counters();
    let nvml_off = p.nvml_csv.and_then(|f| fs::metadata(f).ok()).map(|m| m.len()).unwrap_or(0);
    let psi0 = psi_cpu();
    let t = Instant::now();
    nap(p.window);
    let secs = t.elapsed().as_secs_f64();
    let jiffies1 = cpu_jiffies();
    let ticks1 = proc_ticks();
    let swap1 = swap_counters();
    let mut reasons = Vec::new();

    let pct = |id: Option<u32>| -> f64 {
        let a = jiffies0.iter().find(|x| x.0 == id);
        let b = jiffies1.iter().find(|x| x.0 == id);
        match (a, b) {
            (Some(a), Some(b)) if b.2 > a.2 => 100.0 * (b.1 - a.1) as f64 / (b.2 - a.2) as f64,
            _ => 0.0,
        }
    };
    let host_busy = pct(None);
    if host_busy >= 5.0 {
        reasons.push(format!("host busy {host_busy:.2}% >= 5%"));
    }
    let mut per_cpu = J::obj();
    for &c in p.selected_cpus {
        let b = pct(Some(c));
        per_cpu.set(&c.to_string(), b);
        if b >= 2.0 {
            reasons.push(format!("cpu{c} busy {b:.2}% >= 2%"));
        }
    }
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
    let mut offenders = J::Arr(Vec::new());
    for (pid, t1) in &ticks1 {
        if p.exempt.contains(pid) {
            continue;
        }
        let t0 = ticks0.iter().find(|x| x.0 == *pid).map(|x| x.1).unwrap_or(0);
        let share = 100.0 * t1.saturating_sub(t0) as f64 / hz / secs;
        if share > 1.0 {
            let name = cmdline(*pid);
            offenders.push(
                J::obj()
                    .with("pid", *pid)
                    .with("cpu_pct_of_one", share)
                    .with("cmd", name.chars().take(160).collect::<String>()),
            );
            reasons.push(format!("pid {pid} at {share:.1}% of one CPU"));
        }
    }
    let swapped = (swap1.0 - swap0.0, swap1.1 - swap0.1);
    if swapped != (0, 0) {
        reasons.push(format!("swap I/O during gate: in {} out {}", swapped.0, swapped.1));
    }
    let mut gpu = J::Null;
    if let Some(csv) = p.nvml_csv {
        let text = fs::read(csv).unwrap_or_default();
        let tail = String::from_utf8_lossy(&text[(nvml_off as usize).min(text.len())..]).into_owned();
        let (mut util_max, mut mem_min, mut mem_max, mut n) = (0.0f64, f64::MAX, 0.0f64, 0);
        for line in tail.lines() {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            // timestamp, memory.used MiB, power W, utilization %
            if f.len() >= 4 {
                if let (Ok(m), Ok(u)) = (f[1].parse::<f64>(), f[3].parse::<f64>()) {
                    util_max = util_max.max(u);
                    mem_min = mem_min.min(m);
                    mem_max = mem_max.max(m);
                    n += 1;
                }
            }
        }
        if n == 0 {
            reasons.push("no NVML samples during gate".into());
        } else {
            if util_max >= 2.0 {
                reasons.push(format!("GPU utilization {util_max}% >= 2%"));
            }
            if mem_max - mem_min > 64.0 {
                reasons.push(format!("GPU memory moved {:.0} MiB > 64 MiB", mem_max - mem_min));
            }
        }
        gpu = J::obj()
            .with("samples", n as u64)
            .with("util_max_pct", util_max)
            .with("mem_used_min_mib", if n > 0 { J::Num(mem_min) } else { J::Null })
            .with("mem_used_max_mib", mem_max);
    }
    let passed = reasons.is_empty();
    let trace = J::obj()
        .with("started_unix_ms", started)
        .with("window_s", secs)
        .with("passed", passed)
        .with("host_busy_pct", host_busy)
        .with("selected_cpu_busy_pct", per_cpu)
        .with("external_offenders", offenders)
        .with("swap_in_pages", swapped.0)
        .with("swap_out_pages", swapped.1)
        .with("gpu", gpu)
        .with("psi_cpu_before", psi0)
        .with("psi_cpu_after", psi_cpu())
        .with("loadavg", fs::read_to_string("/proc/loadavg").unwrap_or_default().trim().to_string())
        .with("reasons", J::Arr(reasons.into_iter().map(J::from).collect()));
    (passed, trace)
}
