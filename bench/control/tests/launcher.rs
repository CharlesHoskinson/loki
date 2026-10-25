//! Launcher contract tests with a fake arm and a private HOME, so the real
//! host locks under ~/.cache/doctorwatson are never touched.

use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Env {
    root: PathBuf,
}

fn sha(p: &Path) -> String {
    let o = Command::new("sha256sum").arg(p).output().unwrap();
    String::from_utf8(o.stdout).unwrap().split_whitespace().next().unwrap().to_string()
}

impl Env {
    fn new(name: &str, mode: &str, extra: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("launcher-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in ["home/.cache/doctorwatson", "arm", "params", "out"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        let arm = root.join("arm/fake-arm.sh");
        fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake-arm.sh"), &arm).unwrap();
        fs::write(root.join("arm/mode"), mode).unwrap();
        fs::write(root.join("srs"), b"srs").unwrap();
        fs::write(root.join("verifier"), b"verifier").unwrap();
        let r = root.display();
        let manifest = format!(
            "schema = loki-bench-launch/1\ncohort = test\ncollection = false\nout_root = {r}/out\n\
             srs = {r}/srs\nsrs_sha256 = {}\nparams_dir = {r}/params\nstock_verifier = {r}/verifier\n\
             stock_verifier_sha256 = {}\ntimeout_s = 30\nidle_gate = off\nidle_window_s = 1\n\
             idle_max_wait_s = 1\nvk_policy = require-equal\nheavy = own\ngpu_lock = block\nnvml = off\npass_env = PATH\n\
             source.loki_rev = deadbeef\nbudget.N1.mask = 0\nbudget.N1.workers = 1\n\
             arm.S0.binary = {r}/arm/fake-arm.sh\narm.S0.sha256 = {}\narm.S0.gpu = false\n\
             arm.S0.features = circuits\narm.S0.workspace = stock\n\
             arm.S0.env.LOKI_GPU_MSM_MIN = 16384\nblock.B000 = N1: S0\n{extra}",
            sha(&root.join("srs")),
            sha(&root.join("verifier")),
            sha(&arm),
        );
        fs::write(root.join("manifest.conf"), manifest).unwrap();
        Self { root }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_bench-launch"));
        c.args(["--manifest"])
            .arg(self.root.join("manifest.conf"))
            .args(["--block", "B000"])
            .env("HOME", self.root.join("home"))
            .env("LOKI_NTT_CPU", "1")
            .env("LOKI_NTT_FORCE_ERROR", "1")
            .env("SECRET_TOKEN", "do-not-relay");
        c
    }

    fn lock(&self, name: &str) -> PathBuf {
        self.root.join(format!("home/.cache/doctorwatson/{name}.lock"))
    }

    fn block_json(&self) -> String {
        let dir = self.root.join("out/test");
        let run = fs::read_dir(&dir).unwrap().next().unwrap().unwrap().path();
        fs::read_to_string(run.join("block.json")).unwrap()
    }

    fn arm_file(&self, prefix: &str) -> String {
        for e in fs::read_dir(self.root.join("arm")).unwrap() {
            let p = e.unwrap().path();
            if p.file_name().unwrap().to_str().unwrap().starts_with(prefix) {
                return fs::read_to_string(p).unwrap();
            }
        }
        panic!("no {prefix} file")
    }
}

fn free(path: &Path) -> bool {
    let f = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path).unwrap();
    let ok = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    ok
}

fn field_ms(json: &str, lock: &str, field: &str) -> i64 {
    let at = json.find(&format!("\"lock\":\"{lock}\"")).expect(lock);
    let rest = &json[at..];
    let i = rest.find(&format!("\"{field}\":")).unwrap() + field.len() + 3;
    rest[i..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap()
}

#[test]
fn accepted_block_takes_heavy_then_gpu_and_relays_only_the_allowlist() {
    let e = Env::new("ok", "ok", "");
    let out = e.cmd().output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let b = e.block_json();
    assert!(b.contains("\"status\":\"ACCEPTED\""), "{b}");
    assert!(b.contains("\"status\":\"COMPLETE\""), "{b}");
    assert!(b.contains("SMOKE"), "{b}");
    assert!(b.contains("\"loki_rev\":\"deadbeef\""), "{b}");
    // heavy acquired before gpu; gpu released before heavy.
    assert!(field_ms(&b, "heavy", "acquired_unix_ms") <= field_ms(&b, "gpu", "acquired_unix_ms"));
    assert!(field_ms(&b, "gpu", "released_unix_ms") <= field_ms(&b, "heavy", "released_unix_ms"));
    assert!(free(&e.lock("heavy")) && free(&e.lock("gpu")));
    // Environment: cleared, allowlist only, RAYON from the budget.
    let env = e.arm_file("env.");
    assert!(env.contains("RAYON_NUM_THREADS=1\n"), "{env}");
    assert!(env.contains("LOKI_GPU_MSM_MIN=16384\n"), "{env}");
    for leak in ["LOKI_NTT_CPU", "LOKI_NTT_FORCE_ERROR", "SECRET_TOKEN", "HOME="] {
        assert!(!env.contains(leak), "{leak} leaked: {env}");
    }
    assert!(e.arm_file("affinity.").contains("\t0\n") || e.arm_file("affinity.").trim().ends_with(":\t0"));
    assert!(b.contains("\"all_equal\":true"), "{b}");
    assert!(b.contains("\"qualification\":\"PASS\""), "{b}");
}

/// Two arms of the same fake binary; the second writes a different VK.
fn two_arm_env(name: &str, policy: &str) -> Env {
    let e = Env::new(name, "ok", "");
    let arm2 = e.root.join("arm2");
    fs::create_dir_all(&arm2).unwrap();
    fs::copy(e.root.join("arm/fake-arm.sh"), arm2.join("fake-arm.sh")).unwrap();
    fs::write(arm2.join("mode"), "ok").unwrap();
    fs::write(arm2.join("vk"), "other-vk").unwrap();
    let m = e.root.join("manifest.conf");
    let r = e.root.display();
    let text = fs::read_to_string(&m)
        .unwrap()
        .replace("vk_policy = require-equal", &format!("vk_policy = {policy}"))
        .replace("block.B000 = N1: S0", "block.B000 = N1: S0 L0")
        + &format!(
            "arm.L0.binary = {r}/arm2/fake-arm.sh\narm.L0.sha256 = {}\narm.L0.gpu = false\n\
             arm.L0.features = circuits\narm.L0.workspace = loki\n",
            sha(&arm2.join("fake-arm.sh"))
        );
    fs::write(&m, text).unwrap();
    e
}

#[test]
fn required_vk_mismatch_fails_qualification() {
    let e = two_arm_env("vk-req", "require-equal");
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let b = e.block_json();
    assert!(b.contains("\"L0_vs_S0\":false"), "{b}");
    assert!(b.contains("\"qualification\":\"FAIL\""), "{b}");
}

#[test]
fn recorded_vk_mismatch_is_reported_but_does_not_fail() {
    let e = two_arm_env("vk-rec", "record");
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let b = e.block_json();
    assert!(b.contains("\"L0_vs_S0\":false"), "{b}");
    assert!(b.contains("\"qualification\":\"PASS\""), "{b}");
}

#[test]
fn a_descendant_that_escapes_the_group_fails_the_arm_and_is_killed() {
    let e = Env::new("escape", "escape", "");
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let b = e.block_json();
    assert!(b.contains("\"status\":\"FAILED\""), "{b}");
    assert!(b.contains("orphan"), "{b}");
    let child: i32 = e.arm_file("child").trim().parse().unwrap();
    assert!(!Path::new(&format!("/proc/{child}")).exists(), "escaped descendant {child} survived");
}

#[test]
fn queued_for_a_busy_gpu_lock_records_queue_time() {
    let e = Env::new("queue", "ok", "");
    let holder = Command::new("flock")
        .arg(e.lock("gpu"))
        .args(["sleep", "1.5"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let out = e.cmd().output().unwrap();
    assert!(out.status.success());
    let b = e.block_json();
    let gpu_at = b.find("\"lock\":\"gpu\"").unwrap();
    let q = &b[gpu_at..];
    let i = q.find("\"queue_ms\":").unwrap() + 11;
    let queue: f64 = q[i..].split([',', '}']).next().unwrap().parse().unwrap();
    assert!(queue > 500.0, "queue_ms {queue}");
    drop(holder);
}

#[test]
fn failing_arm_is_receipted_failed_and_locks_are_released() {
    let e = Env::new("fail", "fail", "");
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(1), "an unusable arm fails qualification");
    let b = e.block_json();
    assert!(b.contains("\"status\":\"FAILED\""), "{b}");
    assert!(b.contains("\"status\":\"COMPLETE\""), "collection still completes: {b}");
    assert!(b.contains("\"qualification\":\"FAIL\""), "{b}");
    assert!(b.contains("injected"), "{b}");
    assert!(free(&e.lock("heavy")) && free(&e.lock("gpu")));
}

#[test]
fn timeout_kills_the_whole_process_group() {
    let e = Env::new("timeout", "hang", "");
    let m = e.root.join("manifest.conf");
    fs::write(&m, fs::read_to_string(&m).unwrap().replace("timeout_s = 30", "timeout_s = 1")).unwrap();
    let t = Instant::now();
    let out = e.cmd().output().unwrap();
    assert!(t.elapsed() < Duration::from_secs(10));
    assert_eq!(out.status.code(), Some(1));
    let b = e.block_json();
    assert!(b.contains("\"status\":\"TIMEOUT\""), "{b}");
    assert!(b.contains("\"qualification\":\"FAIL\""), "{b}");
    assert!(b.contains("\"orphans_after_kill\":[]"), "{b}");
    let child: i32 = e.arm_file("child").trim().parse().unwrap();
    assert!(!Path::new(&format!("/proc/{child}")).exists(), "background child {child} survived");
}

#[test]
fn sigterm_cancels_the_block_kills_the_arm_and_releases_locks() {
    let e = Env::new("cancel", "hang", "");
    let mut launcher = e.cmd().stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let pidfile = e.root.join("arm/child");
    let t = Instant::now();
    while !pidfile.exists() {
        assert!(t.elapsed() < Duration::from_secs(10), "arm never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(!free(&e.lock("heavy")), "heavy must be held while the arm runs");
    unsafe { libc::kill(launcher.id() as i32, libc::SIGTERM) };
    let st = launcher.wait().unwrap();
    assert_eq!(st.code(), Some(130));
    let b = e.block_json();
    assert!(b.contains("\"status\":\"CANCELLED\""), "{b}");
    assert!(b.contains("\"orphans_after_kill\":[]"), "{b}");
    let child: i32 = fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    assert!(!Path::new(&format!("/proc/{child}")).exists());
    assert!(free(&e.lock("heavy")) && free(&e.lock("gpu")));
}

/// Scope of the SIGKILL guarantee: the direct arm process (PDEATHSIG armed
/// by the launcher) and, in the real driver, its verifier worker (PDEATHSIG
/// armed by the driver). PDEATHSIG is not inherited, so an arbitrary
/// grandchild such as the fixture's `sleep` is NOT covered when the launcher
/// itself is SIGKILLed; ordinary cancellation and timeout kill the group.
#[test]
fn sigkill_of_the_launcher_takes_the_arm_with_it() {
    let e = Env::new("kill9", "hang", "");
    let mut launcher = e.cmd().stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let pidfile = e.root.join("arm/pid");
    let t = Instant::now();
    while !pidfile.exists() {
        assert!(t.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(200));
    let arm: i32 = fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
    launcher.kill().unwrap();
    launcher.wait().unwrap();
    let t = Instant::now();
    while Path::new(&format!("/proc/{arm}/stat")).exists()
        && !fs::read_to_string(format!("/proc/{arm}/stat")).unwrap_or_default().contains(") Z")
    {
        assert!(t.elapsed() < Duration::from_secs(5), "arm {arm} outlived a SIGKILLed launcher");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Outside the guarantee (see above): clean up the fixture grandchild.
    if let Ok(c) = fs::read_to_string(e.root.join("arm/child")) {
        if let Ok(c) = c.trim().parse::<i32>() {
            unsafe { libc::kill(c, libc::SIGKILL) };
        }
    }
    assert!(free(&e.lock("heavy")) && free(&e.lock("gpu")));
}

#[test]
fn dw_mode_refuses_without_an_ancestor_holding_heavy() {
    let e = Env::new("dw-refuse", "ok", "");
    let m = e.root.join("manifest.conf");
    fs::write(&m, fs::read_to_string(&m).unwrap().replace("heavy = own", "heavy = dw")).unwrap();
    // Another, unrelated process holds heavy: that is not our reservation.
    let mut other = Command::new("flock").arg(e.lock("heavy")).args(["sleep", "3"]).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not held by an ancestor"));
    other.kill().unwrap();
    other.wait().unwrap();
}

#[test]
fn dw_mode_runs_under_an_ancestor_holding_heavy() {
    let e = Env::new("dw-ok", "ok", "");
    let m = e.root.join("manifest.conf");
    fs::write(&m, fs::read_to_string(&m).unwrap().replace("heavy = own", "heavy = dw")).unwrap();
    let inner = e.cmd();
    let mut outer = Command::new("flock");
    outer.arg(e.lock("heavy")).arg(inner.get_program()).args(inner.get_args());
    for (k, v) in inner.get_envs() {
        if let Some(v) = v {
            outer.env(k, v);
        }
    }
    let out = outer.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let b = e.block_json();
    assert!(b.contains("\"mode\":\"held-by-ancestor\""), "{b}");
    assert!(b.contains("\"status\":\"ACCEPTED\""), "{b}");
}

#[test]
fn tampered_binary_is_rejected_before_any_lock() {
    let e = Env::new("tamper", "ok", "");
    fs::write(e.root.join("arm/fake-arm.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    let mut holder = Command::new("flock").arg(e.lock("heavy")).args(["sleep", "5"]).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let t = Instant::now();
    let out = e.cmd().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(t.elapsed() < Duration::from_secs(3), "must fail before queueing on heavy");
    assert!(String::from_utf8_lossy(&out.stderr).contains("sha256"));
    holder.kill().unwrap();
    holder.wait().unwrap();
}
