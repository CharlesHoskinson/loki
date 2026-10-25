//! bench-launch: the SUITE.md block controller.
//!
//!   bench-launch --manifest FILE --block ID [--hypothesis TEXT]
//!   bench-launch validate --manifest FILE
//!
//! Order of operations for one block (nothing is built here):
//!  1. validate the manifest; hash every arm binary, the stock verifier and
//!     the SRS against their pinned digests; run `<arm> identity`; check the
//!     budget mask is allowed on this host;
//!  2. take heavy.lock (own) or prove an ancestor holds it (dw), THEN take
//!     gpu.lock (whole block, or only around GPU arms);
//!  3. start one NVML sampler for the whole block;
//!  4. per arm, serially: idle gate, swap counters, spawn
//!     `taskset -c MASK <arm> chain ... --gate stdin` with a cleared
//!     environment plus the allowlist, its own process group and
//!     PDEATHSIG; sample VmRSS/VmSwap/Threads at 200 ms; run the idle gate
//!     again between cold and warm while the arm keeps its allocations; reap
//!     with wait4; mark INVALID on any swap I/O; kill the process group and
//!     check for orphans;
//!  5. release gpu.lock then heavy.lock and write the block receipt.
//! SIGTERM/SIGINT/SIGHUP cancel: the arm group is killed, locks released,
//! and the block is receipted CANCELLED.

mod host;
mod json;
mod manifest;

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use host::HostLock;
use json::J;
use manifest::{Arm, Budget, GpuLockMode, HeavyMode, IdleMode, Manifest, VkPolicy};

const TASKSET: &str = "/usr/bin/taskset";

/// Absolute path of nvidia-smi (PATH, then the WSL driver directory).
fn nvidia_smi() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    dirs.push("/usr/lib/wsl/lib".into());
    dirs.into_iter().map(|d| d.join("nvidia-smi")).find(|p| p.is_file())
}

fn main() {
    host::install_cancel_handlers();
    // Orphaned descendants of an arm are re-parented to us, so they can be
    // detected, killed and reaped instead of outliving the reservation.
    // SAFETY: prctl with integer arguments.
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bench-launch: error: {e}");
            2
        }
    };
    std::process::exit(code);
}

struct Cli {
    validate_only: bool,
    manifest: PathBuf,
    block: Option<String>,
    hypothesis: Option<String>,
}

fn parse_cli(args: &[String]) -> Result<Cli, String> {
    let mut cli = Cli {
        validate_only: false,
        manifest: PathBuf::new(),
        block: None,
        hypothesis: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "validate" => cli.validate_only = true,
            "--manifest" => cli.manifest = val()?.into(),
            "--block" => cli.block = Some(val()?),
            "--hypothesis" => cli.hypothesis = Some(val()?),
            _ => return Err(format!("unknown argument {a}")),
        }
    }
    if cli.manifest.as_os_str().is_empty() {
        return Err("usage: bench-launch [validate] --manifest FILE [--block ID] [--hypothesis TEXT]".into());
    }
    if !cli.validate_only && cli.block.is_none() {
        return Err("--block is required".into());
    }
    Ok(cli)
}

/// Everything checked before any lock is requested.
struct Prepared {
    m: Manifest,
    manifest_sha256: String,
    identities: Vec<(String, String)>,
    staged: Option<String>,
    nvidia_smi: Option<PathBuf>,
}

fn prepare(cli: &Cli) -> Result<Prepared, String> {
    let raw = fs::read_to_string(&cli.manifest).map_err(|e| format!("{}: {e}", cli.manifest.display()))?;
    let m = Manifest::parse(&raw)?;
    let manifest_sha256 = host::sha256_file(&cli.manifest)?;
    let check = |what: &str, p: &Path, want: &str| -> Result<(), String> {
        let got = host::sha256_file(p)?;
        if got != want {
            return Err(format!("{what} {} sha256 {got} != pinned {want}", p.display()));
        }
        Ok(())
    };
    check("srs", &m.srs, &m.srs_sha256)?;
    check("stock_verifier", &m.stock_verifier, &m.stock_verifier_sha256)?;
    if !m.params_dir.is_dir() {
        return Err(format!("params_dir {} is not a directory", m.params_dir.display()));
    }
    let allowed = host::allowed_cpus();
    for (name, b) in &m.budgets {
        if let Some(c) = b.cpus.iter().find(|c| !allowed.contains(c)) {
            return Err(format!("budget {name}: CPU {c} is not allowed here (allowed {allowed:?})"));
        }
    }
    let arm_names: Vec<String> = match &cli.block {
        Some(b) => m.blocks.get(b).ok_or_else(|| format!("unknown block {b}"))?.arms.clone(),
        None => m.arms.keys().cloned().collect(),
    };
    let mut identities = Vec::new();
    for name in &arm_names {
        let arm = &m.arms[name];
        check(&format!("arm {name}"), &arm.binary, &arm.sha256)?;
        let out = arm_command(&m, arm, None)
            .arg(&arm.binary)
            .arg("identity")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|e| format!("arm {name} identity: {e}"))?;
        if !out.status.success() {
            return Err(format!("arm {name} identity failed"));
        }
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let expect_g0 = arm.gpu;
        if id.contains("\"g0\":true") != expect_g0 {
            return Err(format!("arm {name}: gpu={expect_g0} disagrees with binary identity {id}"));
        }
        identities.push((name.clone(), id));
    }
    let staged = match &m.staged {
        Some(p) => Some(fs::read_to_string(p).map_err(|e| format!("staged {}: {e}", p.display()))?),
        None => None,
    };
    let nvidia_smi = nvidia_smi();
    if m.nvml && nvidia_smi.is_none() {
        return Err("nvml = on but nvidia-smi was not found".into());
    }
    Ok(Prepared {
        m,
        manifest_sha256,
        identities,
        staged,
        nvidia_smi,
    })
}

/// `taskset` with a cleared environment holding only the allowlist.
/// `budget` None = no affinity restriction (identity probe only).
fn arm_command(m: &Manifest, arm: &Arm, budget: Option<&Budget>) -> Command {
    let mut cmd = Command::new(TASKSET);
    cmd.env_clear();
    for k in &m.pass_env {
        if let Some(v) = std::env::var_os(k) {
            cmd.env(k, v);
        }
    }
    for (k, v) in &arm.env {
        cmd.env(k, v);
    }
    match budget {
        Some(b) => {
            cmd.env("RAYON_NUM_THREADS", b.workers.to_string());
            cmd.arg("-c").arg(&b.mask);
        }
        None => {
            cmd.arg("-c").arg(mask_of(&host::allowed_cpus()));
        }
    }
    cmd
}

fn mask_of(cpus: &[u32]) -> String {
    cpus.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
}

fn effective_env(m: &Manifest, arm: &Arm, b: &Budget) -> J {
    let mut j = J::obj();
    for k in &m.pass_env {
        j.set(k, std::env::var(k).ok());
    }
    for (k, v) in &arm.env {
        j.set(k, v.as_str());
    }
    j.set("RAYON_NUM_THREADS", b.workers.to_string());
    j
}

fn host_record() -> J {
    J::obj()
        .with("uname", host::command_text("uname", &["-a"]))
        .with("nproc", host::command_text("nproc", &[]))
        .with("allowed_cpus", mask_of(&host::allowed_cpus()))
        .with("meminfo", host::meminfo())
        .with(
            "gpu",
            host::command_text(
                &nvidia_smi().map(|p| p.display().to_string()).unwrap_or_else(|| "nvidia-smi".into()),
                &[
                    "--query-gpu=uuid,name,driver_version,memory.total,memory.used,clocks.sm,clocks.mem,power.limit,temperature.gpu,clocks_throttle_reasons.active",
                    "--format=csv,noheader",
                ],
            ),
        )
        .with("taskset", host::command_text(TASKSET, &["--version"]))
        .with("cpu_governor", fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").map(|s| s.trim().to_string()).unwrap_or_else(|_| "unavailable (WSL2)".into()))
        .with("loadavg", fs::read_to_string("/proc/loadavg").unwrap_or_default().trim().to_string())
}

/// Append-only event log for recovery; also mirrored to stderr.
struct Events(File);

impl Events {
    fn log(&mut self, kind: &str, body: J) {
        let line = J::obj().with("t_unix_ms", host::unix_ms()).with("event", kind).with("body", body).render();
        let _ = writeln!(self.0, "{line}");
        let _ = self.0.flush();
        eprintln!("bench-launch: {kind}");
    }
}

fn run(args: &[String]) -> Result<i32, String> {
    let cli = parse_cli(args)?;
    let p = prepare(&cli)?;
    if cli.validate_only {
        println!(
            "{}",
            J::obj()
                .with("valid", true)
                .with("manifest_sha256", p.manifest_sha256.as_str())
                .with("arms", J::Arr(p.identities.iter().map(|(_, i)| J::Raw(i.clone())).collect()))
                .render()
        );
        return Ok(0);
    }
    let m = &p.m;
    let block_id = cli.block.clone().unwrap_or_default();
    let block = &m.blocks[&block_id];
    let budget = &m.budgets[&block.budget];

    if m.heavy == HeavyMode::Dw {
        // The outer runner request must stay non-GPU-classified (SUITE).
        if let Some(a) = std::env::args().find(|a| a.contains("cuda") || a.contains("gpu")) {
            return Err(format!("argument {a:?} contains cuda/gpu; the outer request must not be GPU-classified"));
        }
    }

    let stamp = host::command_text("date", &["-u", "+%Y%m%dT%H%M%SZ"]);
    let run_dir = m.out_root.join(&m.cohort).join(format!("{block_id}-{stamp}"));
    fs::create_dir_all(&run_dir).map_err(|e| format!("{}: {e}", run_dir.display()))?;
    fs::write(run_dir.join("manifest.conf"), &m.raw).map_err(|e| e.to_string())?;
    let mut ev = Events(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(run_dir.join("events.jsonl"))
            .map_err(|e| e.to_string())?,
    );
    let mut receipt = J::obj()
        .with("schema", "loki-bench-block/1")
        .with("cohort", m.cohort.as_str())
        .with("collection", m.collection)
        .with(
            "label",
            if m.collection { "SUITE collection" } else { "SMOKE: not a SUITE collection (no idle qualification, no randomized block schedule)" },
        )
        .with("block", block_id.as_str())
        .with("budget", block.budget.as_str())
        .with("mask", budget.mask.as_str())
        .with("workers", budget.workers)
        .with("arm_order", J::Arr(block.arms.iter().map(J::from).collect()))
        .with("hypothesis", cli.hypothesis.clone())
        .with("manifest_sha256", p.manifest_sha256.as_str())
        .with("source", J::Obj(m.source.iter().map(|(k, v)| (k.clone(), J::from(v))).collect()))
        .with("staged", p.staged.clone())
        .with("controller_pid", std::process::id())
        .with("controller_binary_sha256", std::env::current_exe().ok().and_then(|e| host::sha256_file(&e).ok()))
        .with("host_before", host_record())
        .with("swap_before", swap_json(host::swap_counters()))
        .with("identities", J::Obj(p.identities.iter().map(|(k, v)| (k.clone(), J::Raw(v.clone()))).collect()));
    ev.log("prepared", J::obj().with("run_dir", run_dir.display().to_string()));

    // ---- locks: heavy THEN gpu
    let lock_dir = host::lock_dir();
    let heavy = match m.heavy {
        HeavyMode::Own => match HostLock::acquire("heavy", &lock_dir.join("heavy.lock")).map_err(|e| e.to_string())? {
            Some(l) => l,
            None => return finish_cancelled(&run_dir, receipt, &mut ev, "cancelled while queued for heavy.lock"),
        },
        HeavyMode::Dw => HostLock::assert_held_by_ancestor("heavy", &lock_dir.join("heavy.lock"))?,
    };
    ev.log("heavy", heavy.receipt.clone());
    let any_gpu = block.arms.iter().any(|a| m.arms[a].gpu);
    let mut gpu_lock: Option<HostLock> = None;
    let mut gpu_receipts = J::Arr(Vec::new());
    if m.gpu_lock == GpuLockMode::Block {
        match HostLock::acquire("gpu", &lock_dir.join("gpu.lock")).map_err(|e| e.to_string())? {
            Some(l) => {
                ev.log("gpu", l.receipt.clone());
                gpu_lock = Some(l);
            }
            None => {
                receipt.set("heavy_lock", heavy.release());
                return finish_cancelled(&run_dir, receipt, &mut ev, "cancelled while queued for gpu.lock");
            }
        }
    }

    // ---- block-wide NVML sampler (identical for every arm)
    let nvml_path = run_dir.join("nvml.csv");
    let mut nvml = if m.nvml {
        let f = File::create(&nvml_path).map_err(|e| e.to_string())?;
        let mut c = Command::new(p.nvidia_smi.as_ref().expect("resolved in prepare"));
        c.args([
            "--query-gpu=timestamp,memory.used,power.draw,utilization.gpu",
            "--format=csv,noheader,nounits",
            "-lms",
            "200",
        ])
        .stdout(f)
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .process_group(0);
        set_pdeathsig(&mut c);
        Some(c.spawn().map_err(|e| format!("nvml sampler: {e}"))?)
    } else {
        None
    };
    let nvml_pid = nvml.as_ref().map(|c| c.id() as i32);
    if nvml.is_some() {
        host::nap(Duration::from_secs(2));
    }

    let mut arms_out = J::obj();
    let mut block_status = "COMPLETE";
    for name in &block.arms {
        let arm = &m.arms[name];
        if host::cancelled().is_some() {
            block_status = "CANCELLED";
            break;
        }
        if m.gpu_lock == GpuLockMode::GpuArms && arm.gpu && gpu_lock.is_none() {
            match HostLock::acquire("gpu", &lock_dir.join("gpu.lock")).map_err(|e| e.to_string())? {
                Some(l) => {
                    ev.log("gpu", l.receipt.clone());
                    gpu_lock = Some(l);
                }
                None => {
                    block_status = "CANCELLED";
                    break;
                }
            }
        }
        let ctx = ArmCtx {
            m,
            name,
            arm,
            budget,
            block: &block_id,
            run_dir: &run_dir,
            nvml_csv: m.nvml.then_some(nvml_path.as_path()),
            nvml_pid,
        };
        let r = run_arm(&ctx, &mut ev);
        let status = match &r {
            J::Obj(f) => f.iter().find(|(k, _)| k == "status").map(|(_, v)| v.render()).unwrap_or_default(),
            _ => String::new(),
        };
        fs::write(run_dir.join(format!("{name}.receipt.json")), r.render() + "\n").map_err(|e| e.to_string())?;
        ev.log("arm", J::obj().with("arm", name.as_str()).with("status", J::Raw(status.clone())));
        arms_out.set(name, r);
        if m.gpu_lock == GpuLockMode::GpuArms && arm.gpu {
            if let Some(l) = gpu_lock.take() {
                gpu_receipts.push(l.release());
            }
        }
        if status.contains("CANCELLED") {
            block_status = "CANCELLED";
            break;
        }
        if status.contains("DEFERRED") {
            block_status = "DEFERRED";
            break;
        }
    }
    if let Some(mut c) = nvml.take() {
        // SAFETY: signalling our own sampler's process group.
        unsafe {
            libc::killpg(c.id() as i32, libc::SIGTERM);
        }
        let _ = c.wait();
    }
    let vk = vk_comparison(&run_dir, &block.arms);
    let vk_equal = vk.render().contains("\"all_equal\":true");
    let arm_statuses: Vec<(String, String)> = match &arms_out {
        J::Obj(f) => f
            .iter()
            .map(|(k, v)| {
                let st = match v {
                    J::Obj(g) => g.iter().find(|(k, _)| k == "status").map(|(_, v)| v.render()).unwrap_or_default(),
                    _ => String::new(),
                };
                (k.clone(), st.trim_matches('"').to_string())
            })
            .collect(),
        _ => Vec::new(),
    };
    let (qualification, q_reasons) = qualify(block_status, block.arms.len(), &arm_statuses, m.vk_policy, vk_equal);
    receipt.set("vk_policy", if m.vk_policy == VkPolicy::RequireEqual { "require-equal" } else { "record" });
    receipt.set("vk_comparison", vk);
    receipt.set("arms", arms_out);
    receipt.set("swap_after", swap_json(host::swap_counters()));
    receipt.set("host_after", host_record());
    // ---- release gpu THEN heavy
    if let Some(l) = gpu_lock.take() {
        gpu_receipts.push(l.release());
    }
    receipt.set("gpu_lock", gpu_receipts);
    let _ = any_gpu;
    receipt.set("heavy_lock", heavy.release());
    receipt.set("status", block_status);
    receipt.set("status_scope", "collection: whether every scheduled arm process ran; see qualification for usability");
    receipt.set("qualification", qualification);
    receipt.set("qualification_reasons", J::Arr(q_reasons.into_iter().map(J::from).collect()));
    receipt.set("finished_unix_ms", host::unix_ms());
    fs::write(run_dir.join("block.json"), receipt.render() + "\n").map_err(|e| e.to_string())?;
    ev.log("block", J::obj().with("status", block_status).with("receipt", run_dir.join("block.json").display().to_string()));
    println!("{}", run_dir.join("block.json").display());
    Ok(match (block_status, qualification) {
        ("CANCELLED", _) => 130,
        ("COMPLETE", "PASS") => 0,
        _ => 1,
    })
}

/// Block qualification: PASS only if collection completed, every scheduled
/// arm is ACCEPTED, and the declared VK policy holds.
fn qualify(
    collection: &str,
    scheduled: usize,
    arms: &[(String, String)],
    policy: VkPolicy,
    vk_equal: bool,
) -> (&'static str, Vec<String>) {
    let mut reasons = Vec::new();
    if collection != "COMPLETE" {
        reasons.push(format!("collection {collection}"));
    }
    if arms.len() != scheduled {
        reasons.push(format!("{} of {scheduled} arms ran", arms.len()));
    }
    for (a, st) in arms {
        if st != "ACCEPTED" {
            reasons.push(format!("arm {a} {st}"));
        }
    }
    if policy == VkPolicy::RequireEqual && !vk_equal {
        reasons.push("vk_policy require-equal: verifying keys differ or are missing".into());
    }
    (if reasons.is_empty() { "PASS" } else { "FAIL" }, reasons)
}

/// Arm cell status from the observed process facts (pure; unit-tested).
fn classify(
    outcome: Option<&'static str>,
    exit_code: Option<i32>,
    samples_seen: u64,
    expected: u64,
    exit_seen: bool,
    swapped: (u64, u64),
    orphans: usize,
) -> &'static str {
    if let Some(o) = outcome {
        return o;
    }
    if exit_code != Some(0) || samples_seen != expected || !exit_seen || orphans > 0 {
        return "FAILED";
    }
    if swapped != (0, 0) {
        return "INVALID";
    }
    "ACCEPTED"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_marks_swap_invalid_and_orphans_failed() {
        assert_eq!(classify(None, Some(0), 2, 2, true, (0, 0), 0), "ACCEPTED");
        assert_eq!(classify(None, Some(0), 2, 2, true, (1, 0), 0), "INVALID");
        assert_eq!(classify(None, Some(0), 2, 2, true, (0, 3), 0), "INVALID");
        assert_eq!(classify(None, Some(0), 2, 2, true, (0, 0), 1), "FAILED");
        assert_eq!(classify(None, Some(0), 1, 2, true, (0, 0), 0), "FAILED");
        assert_eq!(classify(None, Some(1), 2, 2, true, (0, 0), 0), "FAILED");
        assert_eq!(classify(Some("TIMEOUT"), None, 0, 2, false, (0, 0), 0), "TIMEOUT");
    }

    #[test]
    fn qualification_requires_every_arm_and_the_vk_policy() {
        let ok = vec![("S0".to_string(), "ACCEPTED".to_string())];
        assert_eq!(qualify("COMPLETE", 1, &ok, VkPolicy::RequireEqual, true).0, "PASS");
        assert_eq!(qualify("COMPLETE", 1, &ok, VkPolicy::RequireEqual, false).0, "FAIL");
        assert_eq!(qualify("COMPLETE", 1, &ok, VkPolicy::Record, false).0, "PASS");
        assert_eq!(qualify("DEFERRED", 1, &ok, VkPolicy::Record, true).0, "FAIL");
        assert_eq!(qualify("COMPLETE", 2, &ok, VkPolicy::Record, true).0, "FAIL");
        let bad = vec![("S0".to_string(), "INVALID".to_string())];
        assert_eq!(qualify("COMPLETE", 1, &bad, VkPolicy::Record, true).0, "FAIL");
    }
}

fn finish_cancelled(run_dir: &Path, mut receipt: J, ev: &mut Events, why: &str) -> Result<i32, String> {
    receipt.set("status", "CANCELLED");
    receipt.set("reason", why);
    fs::write(run_dir.join("block.json"), receipt.render() + "\n").map_err(|e| e.to_string())?;
    ev.log("block", J::obj().with("status", "CANCELLED").with("reason", why));
    Ok(130)
}

fn swap_json(c: (u64, u64)) -> J {
    J::obj().with("pswpin", c.0).with("pswpout", c.1).with("meminfo", host::meminfo())
}

/// Arm PDEATHSIG=SIGKILL in the child, then confirm the expected parent is
/// still alive (if it died before prctl, no signal would ever be sent).
/// The guarantee covers this direct child only; PDEATHSIG is not inherited.
fn set_pdeathsig(c: &mut Command) {
    let parent = std::process::id() as libc::pid_t;
    // SAFETY: prctl and getppid are async-signal-safe.
    unsafe {
        c.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                return Err(std::io::Error::other("parent exited before PDEATHSIG was armed"));
            }
            Ok(())
        });
    }
}

/// Kill and reap every child of ours except `keep` (descendants of an arm
/// that escaped its process group are re-parented here by the subreaper).
fn sweep_children(keep: &[i32]) -> Vec<J> {
    let me = std::process::id() as i32;
    let mut swept = Vec::new();
    for _ in 0..5 {
        let kids: Vec<i32> = host::children_of(me).into_iter().filter(|p| !keep.contains(p)).collect();
        if kids.is_empty() {
            break;
        }
        for k in kids {
            swept.push(J::obj().with("pid", k).with("cmd", host::cmdline(k)));
            // SAFETY: signalling and reaping our own child.
            unsafe {
                libc::kill(k, libc::SIGKILL);
                let mut st = 0;
                libc::waitpid(k, &mut st, 0);
            }
        }
    }
    swept
}

struct ArmCtx<'a> {
    m: &'a Manifest,
    name: &'a str,
    arm: &'a Arm,
    budget: &'a Budget,
    block: &'a str,
    run_dir: &'a Path,
    nvml_csv: Option<&'a Path>,
    nvml_pid: Option<i32>,
}

/// Idle gate according to the manifest mode. Returns (may_proceed, trace).
fn gate(ctx: &ArmCtx, label: &str, exempt: Vec<i32>) -> (bool, J) {
    let m = ctx.m;
    if m.idle == IdleMode::Off {
        return (true, J::obj().with("mode", "off").with("label", label));
    }
    let mut exempt = exempt;
    exempt.push(std::process::id() as i32);
    exempt.extend(host::ancestors());
    if let Some(p) = ctx.nvml_pid {
        exempt.push(p);
    }
    let params = host::GateParams {
        window: Duration::from_secs(m.idle_window_s),
        selected_cpus: &ctx.budget.cpus,
        exempt,
        nvml_csv: ctx.nvml_csv,
    };
    let started = Instant::now();
    let mut windows = Vec::new();
    loop {
        let (passed, trace) = host::idle_window(&params);
        windows.push(trace);
        let mode = if m.idle == IdleMode::Record { "record" } else { "enforce" };
        let done = passed || m.idle == IdleMode::Record || host::cancelled().is_some();
        let expired = started.elapsed().as_secs() >= m.idle_max_wait_s;
        if done || expired {
            let ok = passed || m.idle == IdleMode::Record;
            return (
                ok && host::cancelled().is_none(),
                J::obj()
                    .with("mode", mode)
                    .with("label", label)
                    .with("passed", passed)
                    .with("wait_s", started.elapsed().as_secs_f64())
                    .with("windows", J::Arr(windows)),
            );
        }
    }
}

#[derive(Default, Clone, Copy)]
struct Peaks {
    rss: u64,
    hwm: u64,
    swap: u64,
    threads: u64,
    samples: u64,
}

fn run_arm(ctx: &ArmCtx, ev: &mut Events) -> J {
    let m = ctx.m;
    let name = ctx.name;
    let out_dir = ctx.run_dir.join(name);
    let mut r = J::obj()
        .with("schema", "loki-bench-arm-process/1")
        .with("arm", name)
        .with("block", ctx.block)
        .with("binary", ctx.arm.binary.display().to_string())
        .with("binary_sha256", ctx.arm.sha256.as_str())
        .with("workspace", ctx.arm.workspace.as_str())
        .with("features", ctx.arm.features.as_str())
        .with("gpu", ctx.arm.gpu)
        .with("mask", ctx.budget.mask.as_str())
        .with("workers", ctx.budget.workers)
        .with("env", effective_env(m, ctx.arm, ctx.budget))
        .with("forbidden_env_absent", true);

    let (ok, g) = gate(ctx, "before-cold", Vec::new());
    r.set("gate_before_cold", g);
    if !ok {
        r.set("status", if host::cancelled().is_some() { "CANCELLED" } else { "DEFERRED" });
        r.set("proof_attempts", 0u64);
        return r;
    }

    let stdout_path = ctx.run_dir.join(format!("{name}.stdout.jsonl"));
    let stderr_path = ctx.run_dir.join(format!("{name}.stderr.log"));
    let rss_path = ctx.run_dir.join(format!("{name}.rss.tsv"));
    let stderr = match File::create(&stderr_path) {
        Ok(f) => f,
        Err(e) => return r.with("status", "FAILED").with("error", e.to_string()),
    };
    let mut cmd = arm_command(m, ctx.arm, Some(ctx.budget));
    cmd.arg(&ctx.arm.binary)
        .arg("chain")
        .arg("--out")
        .arg(&out_dir)
        .arg("--srs")
        .arg(&m.srs)
        .arg("--params-dir")
        .arg(&m.params_dir)
        .arg("--stock-verifier")
        .arg(&m.stock_verifier)
        .arg("--samples")
        .arg(&m.samples)
        .arg("--block")
        .arg(ctx.block)
        .arg("--gate")
        .arg("stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .process_group(0);
    set_pdeathsig(&mut cmd);
    let argv: Vec<J> = std::iter::once(TASKSET.to_string())
        .chain(cmd.get_args().map(|a| a.to_string_lossy().into_owned()))
        .map(J::from)
        .collect();
    r.set("argv", J::Arr(argv));

    let swap0 = host::swap_counters();
    let mem0 = host::meminfo();
    let nvml_from = ctx.nvml_csv.and_then(|p| fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
    let spawned_ms = host::unix_ms();
    let t0 = Instant::now();
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return r.with("status", "FAILED").with("error", format!("spawn: {e}")),
    };
    let pid = child.id() as i32;
    ev.log("spawn", J::obj().with("arm", name).with("pid", pid));
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take();

    // stdout reader: tee to file and forward lines.
    let (tx, rx) = mpsc::channel::<String>();
    let tee_path = stdout_path.clone();
    let reader = std::thread::spawn(move || {
        let mut tee = File::create(tee_path).ok();
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(f) = tee.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
                let _ = tx.send(line);
            }
        }
    });

    // 200 ms process sampler (live /proc only; never after exit).
    let stop = Arc::new(AtomicBool::new(false));
    let peaks = Arc::new(Mutex::new(Peaks::default()));
    let sampler = {
        let stop = stop.clone();
        let peaks = peaks.clone();
        std::thread::spawn(move || {
            let mut f = File::create(rss_path).ok();
            if let Some(f) = f.as_mut() {
                let _ = writeln!(f, "unix_ms\tvm_rss\tvm_hwm\tvm_swap\tthreads\tgroup_procs");
            }
            while !stop.load(Ordering::SeqCst) {
                if let Some((rss, hwm, swap, threads)) = host::proc_status(pid) {
                    let procs = host::group_members(pid).len();
                    if let Some(f) = f.as_mut() {
                        let _ = writeln!(f, "{}\t{rss}\t{hwm}\t{swap}\t{threads}\t{procs}", host::unix_ms());
                    }
                    let mut p = peaks.lock().unwrap();
                    p.rss = p.rss.max(rss);
                    p.hwm = p.hwm.max(hwm);
                    p.swap = p.swap.max(swap);
                    p.threads = p.threads.max(threads);
                    p.samples += 1;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        })
    };

    let mut deadline = t0 + Duration::from_secs(m.timeout_s);
    let mut outcome: Option<&str> = None;
    let mut events = J::Arr(Vec::new());
    let mut gates = J::Arr(Vec::new());
    let mut samples_seen = 0u64;
    let mut exit_seen = false;
    let mut cancel_signal = None;
    let (status, ru) = loop {
        let mut st = 0;
        // SAFETY: wait4 on our own child with a zeroed rusage.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::wait4(pid, &mut st, libc::WNOHANG, &mut ru) };
        if rc == pid {
            break (st, ru);
        }
        while let Ok(line) = rx.try_recv() {
            if line.contains("\"event\":\"gate\"") {
                let g0 = Instant::now();
                let (ok, g) = gate(ctx, "between-samples", host::group_members(pid));
                gates.push(g);
                deadline += g0.elapsed();
                if ok {
                    if let Some(s) = stdin.as_mut() {
                        let _ = writeln!(s, "go");
                        let _ = s.flush();
                    }
                } else {
                    outcome = Some(if host::cancelled().is_some() { "CANCELLED" } else { "DEFERRED" });
                    kill_group(pid);
                }
            } else if line.contains("\"event\":\"sample\"") {
                samples_seen += 1;
                events.push(J::Raw(line));
            } else if line.contains("\"event\":\"exit\"") {
                exit_seen = true;
                events.push(J::Raw(line));
            } else if line.contains("loki-bench-error") {
                events.push(J::Raw(line));
            }
        }
        if outcome.is_none() {
            if let Some(sig) = host::cancelled() {
                cancel_signal = Some(sig);
                outcome = Some("CANCELLED");
                kill_group(pid);
            } else if Instant::now() > deadline {
                outcome = Some("TIMEOUT");
                kill_group(pid);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let wall_s = t0.elapsed().as_secs_f64();
    drop(stdin);
    // Stragglers (e.g. a verifier worker) die with the group; anything that
    // left the group was re-parented to us and is swept (and fails the arm).
    let group_left = host::group_members(pid);
    kill_group(pid);
    std::thread::sleep(Duration::from_millis(100));
    let escaped = sweep_children(&ctx.nvml_pid.into_iter().collect::<Vec<_>>());
    stop.store(true, Ordering::SeqCst);
    let _ = sampler.join();
    let _ = reader.join();
    while let Ok(line) = rx.try_recv() {
        if line.contains("\"event\":\"sample\"") {
            samples_seen += 1;
            events.push(J::Raw(line));
        } else if line.contains("\"event\":\"exit\"") {
            exit_seen = true;
            events.push(J::Raw(line));
        } else if line.contains("loki-bench-error") {
            events.push(J::Raw(line));
        }
    }
    std::thread::sleep(Duration::from_millis(200));
    let still_in_group = host::group_members(pid);
    let orphan_count = group_left.len() + escaped.len() + still_in_group.len();
    let swap1 = host::swap_counters();
    let swapped = (swap1.0.saturating_sub(swap0.0), swap1.1.saturating_sub(swap0.1));
    let peaks = *peaks.lock().unwrap();
    let expected = m.samples.split(',').count() as u64;
    let exit_code = if libc::WIFEXITED(status) { Some(libc::WEXITSTATUS(status)) } else { None };
    let signal = if libc::WIFSIGNALED(status) { Some(libc::WTERMSIG(status)) } else { None };
    let mut reasons = Vec::new();
    let status_str = classify(outcome, exit_code, samples_seen, expected, exit_seen, swapped, orphan_count);
    if exit_code != Some(0) || samples_seen != expected || !exit_seen {
        reasons.push(format!("exit {exit_code:?} signal {signal:?}, samples {samples_seen}/{expected}, exit record {exit_seen}"));
    }
    if swapped != (0, 0) {
        reasons.push(format!("host swap I/O during the arm process: pswpin +{} pswpout +{}", swapped.0, swapped.1));
    }
    if orphan_count > 0 {
        reasons.push(format!(
            "orphan processes after arm exit: group {group_left:?}, escaped {}, unkillable {still_in_group:?}",
            escaped.len()
        ));
    }
    let nvml = ctx.nvml_csv.map(|p| nvml_summary(p, nvml_from)).unwrap_or(J::Null);
    r.with("status", status_str)
        .with("reasons", J::Arr(reasons.into_iter().map(J::from).collect()))
        .with("cancel_signal", cancel_signal)
        .with("pid", pid)
        .with("spawned_unix_ms", spawned_ms)
        .with("process_wall_s", wall_s)
        .with("exit_code", exit_code)
        .with("term_signal", signal)
        .with(
            "wait4",
            J::obj()
                .with("maxrss_bytes_prover_plus_reaped_children", ru.ru_maxrss as u64 * 1024)
                .with("cpu_user_s", ru.ru_utime.tv_sec as f64 + ru.ru_utime.tv_usec as f64 / 1e6)
                .with("cpu_system_s", ru.ru_stime.tv_sec as f64 + ru.ru_stime.tv_usec as f64 / 1e6)
                .with("note", "wait4 maxrss is max(driver, its reaped verifier worker); the driver's own RUSAGE_SELF peak and the verifier's own peak are in driver-exit.json"),
        )
        .with(
            "sampled_200ms",
            J::obj()
                .with("samples", peaks.samples)
                .with("vm_rss_max_bytes", peaks.rss)
                .with("vm_hwm_last_live_bytes", peaks.hwm)
                .with("vm_swap_max_bytes", peaks.swap)
                .with("threads_max", peaks.threads)
                .with("label", "sampled lower bound; lifetime VmHWM is per process, not per phase"),
        )
        .with(
            "swap",
            J::obj()
                .with("pswpin_delta", swapped.0)
                .with("pswpout_delta", swapped.1)
                .with("meminfo_before", mem0)
                .with("meminfo_after", host::meminfo()),
        )
        .with("gates_between_samples", gates)
        .with("driver_events", events)
        .with("group_members_after_exit", J::Arr(group_left.into_iter().map(J::from).collect()))
        .with("escaped_descendants_swept", J::Arr(escaped))
        .with("orphans_after_kill", J::Arr(still_in_group.into_iter().map(J::from).collect()))
        .with("nvml", nvml)
        .with("stdout", stdout_path.display().to_string())
        .with("stderr", stderr_path.display().to_string())
        .with("out_dir", out_dir.display().to_string())
}

fn kill_group(pgid: i32) {
    // SAFETY: signalling the arm's own process group.
    unsafe {
        libc::killpg(pgid, libc::SIGKILL);
    }
}

fn nvml_summary(path: &Path, from: u64) -> J {
    let text = fs::read(path).unwrap_or_default();
    let tail = String::from_utf8_lossy(&text[(from as usize).min(text.len())..]).into_owned();
    let (mut n, mut base, mut mem_max, mut pow_max, mut util_max) = (0u64, None, 0.0f64, 0.0f64, 0.0f64);
    for line in tail.lines() {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < 4 {
            continue;
        }
        let (Ok(mem), Ok(util)) = (f[1].parse::<f64>(), f[3].parse::<f64>()) else {
            continue;
        };
        let pow = f[2].parse::<f64>().unwrap_or(0.0);
        base.get_or_insert(mem);
        mem_max = mem_max.max(mem);
        pow_max = pow_max.max(pow);
        util_max = util_max.max(util);
        n += 1;
    }
    J::obj()
        .with("samples", n)
        .with("mem_used_first_mib", base)
        .with("mem_used_max_mib", mem_max)
        .with("mem_peak_minus_first_mib", base.map(|b| mem_max - b))
        .with("power_max_w", pow_max)
        .with("util_max_pct", util_max)
        .with("label", "200 ms NVML samples; sampled lower bound")
}

/// Hash every verifying-key file each arm wrote and compare across arms.
fn vk_comparison(run_dir: &Path, arms: &[String]) -> J {
    let mut per_arm: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for a in arms {
        let root = run_dir.join(a);
        let mut files = Vec::new();
        collect_vks(&root, &root, &mut files);
        files.sort();
        per_arm.push((a.clone(), files));
    }
    let mut j = J::obj();
    let mut all_equal = true;
    if let Some((ref_arm, ref_files)) = per_arm.first() {
        for (a, files) in &per_arm[1..] {
            let eq = !files.is_empty() && files == ref_files;
            all_equal &= eq;
            j.set(&format!("{a}_vs_{ref_arm}"), eq);
        }
        j.set(
            &format!("{ref_arm}_files"),
            J::Obj(ref_files.iter().map(|(k, v)| (k.clone(), J::from(v))).collect()),
        );
        all_equal &= !ref_files.is_empty();
    }
    j.with("all_equal", all_equal)
}

fn collect_vks(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_vks(root, &p, out);
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".vk") || n == "vk") {
            if let (Ok(rel), Ok(h)) = (p.strip_prefix(root), host::sha256_file(&p)) {
                out.push((rel.display().to_string(), h));
            }
        }
    }
}
