//! Benchmark driver for the serial Sherlock recursion chain (SUITE.md CHAIN):
//! A102 + A80 leaves (A2) -> A3 parent (k20) -> DEC terminal decider.
//!
//! The same source builds three arms in independent workspaces: `bench/stock`
//! (registry midnight-proofs 0.8.3), `bench/loki` (staged Loki snapshot, CPU)
//! and `bench/loki --features g0` (legacy sppark MSM+NTT). Each stage calls the
//! pinned Moriarty APIs exactly as their production tests do, including every
//! recurring key reconstruction, SRS load, model guard, PK construction and
//! production verification. Every stage artifact is then checked by a separate
//! unmodified-stock verifier process (`stock-verifier serve`), whose
//! request/receipt round trip is inside the chain timer.
//!
//! Usage:
//!   bench-driver identity
//!   bench-driver chain --out DIR --srs K20_SRS --params-dir DIR \
//!       --stock-verifier BIN [--samples cold,warm] [--block ID] [--gate stdin]
//!
//! `--gate stdin`: before every sample after the first, print a
//! `{"event":"gate"}` line and wait for `go` on stdin (the launcher runs its
//! idle gate there while this process keeps its allocations).
//!   bench-driver stage-parent --children DIR --out DIR --srs K20_SRS --params-dir DIR
//!
//! Only public synthetic artifacts are written (keys, proofs, instances and
//! timing receipts). The decider's communication randomness exists only in
//! memory and is never written.

#[path = "common.rs"]
#[allow(dead_code)]
mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use common::{error, Result};
use serde_json::{json, Value};

const ARM: &str = if cfg!(feature = "g0") {
    "G0"
} else if cfg!(feature = "loki") {
    "L0"
} else {
    "S0"
};

fn identity() -> Value {
    json!({
        "schema": "loki-bench-driver-identity/1",
        "arm": ARM,
        "prover": if cfg!(feature = "loki") { "loki staged snapshot (see deps/STAGED)" } else { "midnight-proofs 0.8.3 registry stock" },
        "circuits": cfg!(feature = "circuits"),
        "features": {
            "loki": cfg!(feature = "loki"),
            "g0": cfg!(feature = "g0"),
            "profile": cfg!(feature = "profile"),
        },
        "telemetry": telemetry_available(),
        "rayon_env": std::env::var("RAYON_NUM_THREADS").ok(),
        "cpus_allowed_list": std::fs::read_to_string("/proc/self/status").ok().and_then(|t| {
            t.lines().find_map(|l| l.strip_prefix("Cpus_allowed_list:").map(|v| v.trim().to_string()))
        }),
        "pid": std::process::id(),
        "rng_policy": "production entropy (OsRng/from_entropy inside the pinned APIs); not seeded",
    })
}

const fn telemetry_available() -> bool {
    cfg!(feature = "g0")
}

#[cfg(feature = "g0")]
fn telemetry_snapshot() -> Option<midnight_proofs::telemetry::Snapshot> {
    Some(midnight_proofs::telemetry::snapshot())
}

#[cfg(feature = "g0")]
fn telemetry_since(before: &Option<midnight_proofs::telemetry::Snapshot>) -> Value {
    match before {
        Some(b) => serde_json::to_value(midnight_proofs::telemetry::snapshot().since(b))
            .unwrap_or(Value::Null),
        None => Value::Null,
    }
}

#[cfg(not(feature = "g0"))]
fn telemetry_snapshot() -> Option<()> {
    None
}

#[cfg(not(feature = "g0"))]
fn telemetry_since(_: &Option<()>) -> Value {
    Value::Null
}

#[cfg(feature = "profile")]
fn take_profile() -> Value {
    serde_json::to_value(midnight_proofs::profile::take_report()).unwrap_or(Value::Null)
}

#[cfg(not(feature = "profile"))]
fn take_profile() -> Value {
    Value::Null
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// G0 acceptance (SUITE: production dispatch counters qualified): every
/// stage has zero accelerator errors and zero CPU fallbacks; every stage
/// published at least one accelerated operation; and across the chain both
/// MSM and NTT were accelerated. A sample that fails this is not a GPU
/// performance observation.
#[cfg_attr(not(any(test, feature = "g0")), allow(dead_code))]
fn g0_dispatch_qualification(stages: &[Value]) -> std::result::Result<Value, String> {
    let get = |t: &Value, op: &str, k: &str| t[op][k].as_u64();
    let (mut msm_ok, mut ntt_ok) = (0u64, 0u64);
    let mut per_stage = serde_json::Map::new();
    for s in stages {
        let name = s["stage"].as_str().unwrap_or("?").to_string();
        let t = &s["telemetry"];
        let mut fields = Vec::new();
        for op in ["msm", "ntt"] {
            for k in ["attempted", "succeeded", "error", "fallback"] {
                fields.push(get(t, op, k).ok_or_else(|| format!("stage {name}: telemetry {op}.{k} missing"))?);
            }
        }
        let [_ma, ms, me, mf, _na, ns, ne, nf] = fields[..] else { unreachable!() };
        if me + mf + ne + nf > 0 {
            return Err(format!(
                "stage {name}: accelerator error/fallback (msm error {me} fallback {mf}, ntt error {ne} fallback {nf})"
            ));
        }
        if ms + ns == 0 {
            return Err(format!("stage {name}: no accelerated MSM or NTT operation"));
        }
        msm_ok += ms;
        ntt_ok += ns;
        per_stage.insert(name, json!({"msm_succeeded": ms, "ntt_succeeded": ns}));
    }
    if msm_ok == 0 {
        return Err("chain: no accelerated msm operation".into());
    }
    if ntt_ok == 0 {
        return Err("chain: no accelerated ntt operation".into());
    }
    Ok(json!({"passed": true, "per_stage": per_stage, "msm_succeeded": msm_ok, "ntt_succeeded": ntt_ok}))
}

struct Args {
    out: PathBuf,
    srs: PathBuf,
    params_dir: PathBuf,
    stock_verifier: Option<PathBuf>,
    children: Option<PathBuf>,
    samples: Vec<String>,
    block: String,
    gate_stdin: bool,
}

fn parse(args: &[String]) -> Result<Args> {
    let mut a = Args {
        out: PathBuf::new(),
        srs: PathBuf::new(),
        params_dir: PathBuf::new(),
        stock_verifier: None,
        children: None,
        samples: vec!["cold".into(), "warm".into()],
        block: "adhoc".into(),
        gate_stdin: false,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| error(format!("{flag} needs a value")))
        };
        match flag.as_str() {
            "--out" => a.out = value()?.into(),
            "--srs" => a.srs = value()?.into(),
            "--params-dir" => a.params_dir = value()?.into(),
            "--stock-verifier" => a.stock_verifier = Some(value()?.into()),
            "--children" => a.children = Some(value()?.into()),
            "--samples" => a.samples = value()?.split(',').map(str::to_string).collect(),
            "--block" => a.block = value()?,
            "--gate" => match value()?.as_str() {
                "stdin" => a.gate_stdin = true,
                "none" => a.gate_stdin = false,
                g => return Err(error(format!("unknown gate {g}"))),
            },
            other => return Err(error(format!("unknown flag {other}"))),
        }
    }
    if a.out.as_os_str().is_empty() || a.srs.as_os_str().is_empty() || a.params_dir.as_os_str().is_empty() {
        return Err(error("--out, --srs and --params-dir are required"));
    }
    Ok(a)
}

/// Persistent unmodified-stock verifier worker.
struct Verifier {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    init: Value,
}

impl Verifier {
    fn spawn(bin: &Path, srs: &Path) -> Result<Self> {
        let started = Instant::now();
        let mut cmd = Command::new(bin);
        // The worker dies with this process (launcher cancellation kills the
        // process group; this also covers an abnormal driver exit).
        // SAFETY: prctl is async-signal-safe and touches no Rust state.
        let parent = std::process::id() as libc::pid_t;
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // If we died before prctl, no signal would ever arrive.
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("driver exited before PDEATHSIG was armed"));
                }
                Ok(())
            });
        }
        let mut child = cmd
            .arg("serve")
            .arg("--srs")
            .arg(srs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| error("verifier stdin"))?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or_else(|| error("verifier stdout"))?);
        let mut line = String::new();
        stdout.read_line(&mut line)?;
        let mut init: Value = serde_json::from_str(&line)
            .map_err(|e| error(format!("verifier did not initialize: {e}: {line:?}")))?;
        if init["ready"] != true {
            return Err(error(format!("verifier not ready: {init}")));
        }
        init["spawn_to_ready_ms"] = json!(started.elapsed().as_secs_f64() * 1e3);
        init["pid"] = json!(child.id());
        Ok(Self {
            child,
            stdin,
            stdout,
            init,
        })
    }

    fn request(&mut self, req: Value) -> Result<Value> {
        writeln!(self.stdin, "{req}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            return Err(error("stock verifier exited"));
        }
        let receipt: Value = serde_json::from_str(&line)?;
        if receipt["accepted"] != true {
            return Err(error(format!("stock verifier rejected: {receipt}")));
        }
        Ok(receipt)
    }
}

impl Verifier {
    /// Ask the worker to exit and reap it with `wait4`, returning its own
    /// lifetime resource usage (kept separate from the prover's).
    fn finish(mut self) -> Value {
        let _ = writeln!(self.stdin, "{}", json!({"stage": "exit"}));
        let _ = self.stdin.flush();
        let pid = self.child.id() as libc::pid_t;
        // SAFETY: wait4 on our own unreaped child with a zeroed rusage.
        let (rc, status, ru) = unsafe {
            let mut status = 0;
            let mut ru: libc::rusage = std::mem::zeroed();
            let rc = libc::wait4(pid, &mut status, 0, &mut ru);
            (rc, status, ru)
        };
        std::mem::forget(self);
        if rc != pid {
            return json!({"reaped": false});
        }
        json!({
            "reaped": true,
            "exit_code": if libc::WIFEXITED(status) { json!(libc::WEXITSTATUS(status)) } else { Value::Null },
            "lifetime_maxrss_bytes": ru.ru_maxrss as u64 * 1024,
            "cpu_user_s": ru.ru_utime.tv_sec as f64 + ru.ru_utime.tv_usec as f64 / 1e6,
            "cpu_system_s": ru.ru_stime.tv_sec as f64 + ru.ru_stime.tv_usec as f64 / 1e6,
        })
    }
}

impl Drop for Verifier {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "{}", json!({"stage": "exit"}));
        let _ = self.child.wait();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("identity") => {
            println!("{}", identity());
            Ok(())
        }
        Some("chain") => parse(&args[1..]).and_then(|a| run_chain(&a)),
        Some("stage-parent") => parse(&args[1..]).and_then(|a| run_parent_only(&a)),
        _ => Err(error(
            "usage: bench-driver identity | chain --out DIR --srs SRS --params-dir DIR --stock-verifier BIN [--samples cold,warm] [--block ID] | stage-parent --children DIR --out DIR --srs SRS --params-dir DIR",
        )),
    };
    if let Err(e) = result {
        eprintln!("bench-driver: error: {e}");
        println!("{}", json!({"schema": "loki-bench-error/1", "arm": ARM, "error": e.to_string()}));
        std::process::exit(1);
    }
}

#[cfg(not(feature = "circuits"))]
fn run_chain(_: &Args) -> Result<()> {
    Err(error("built without the `circuits` feature"))
}

#[cfg(not(feature = "circuits"))]
fn run_parent_only(_: &Args) -> Result<()> {
    Err(error("built without the `circuits` feature"))
}

#[cfg(feature = "circuits")]
fn run_parent_only(a: &Args) -> Result<()> {
    let children = a.children.as_ref().ok_or_else(|| error("--children is required"))?;
    std::fs::create_dir_all(&a.out)?;
    let stage = sherlock::parent(children, &a.out.join("parent"), &a.srs, &a.params_dir)?;
    println!("{}", json!({"schema": "loki-bench-stage/1", "arm": ARM, "stage": stage}));
    Ok(())
}

#[cfg(feature = "circuits")]
fn run_chain(a: &Args) -> Result<()> {
    let verifier_bin = a
        .stock_verifier
        .as_ref()
        .ok_or_else(|| error("--stock-verifier is required"))?;
    std::fs::create_dir_all(&a.out)?;
    let process_start = Instant::now();
    // Worker initialization is outside every chain interval and timed separately.
    let mut verifier = Verifier::spawn(verifier_bin, &a.srs)?;
    let receipts_path = a.out.join("samples.jsonl");
    for (index, cache) in a.samples.iter().enumerate() {
        if a.gate_stdin && index > 0 {
            println!("{}", json!({"event": "gate", "before": cache, "status": common::status_bytes()}));
            std::io::stdout().flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            if line.trim() != "go" {
                return Err(error(format!("gate refused before {cache}: {:?}", line.trim())));
            }
        }
        let dir = a.out.join(format!("sample-{index}-{cache}"));
        std::fs::create_dir(&dir)?;
        let started_unix_ms = now_unix_ms();
        let status_start = common::status_bytes();
        let cpu_start = common::cpu_seconds();
        let chain = Instant::now();
        let mut stages = Vec::new();
        let mut create_proof_ms = 0.0;

        let tel = telemetry_snapshot();
        let mut s = sherlock::children(&dir.join("children"), &a.params_dir)?;
        s["telemetry"] = telemetry_since(&tel);
        s["profile"] = take_profile();
        let t = Instant::now();
        s["stock_receipt"] = verifier.request(json!({"stage": "children", "dir": dir.join("children")}))?;
        s["stock_receipt_ms"] = json!(t.elapsed().as_secs_f64() * 1e3);
        stages.push(s);

        let tel = telemetry_snapshot();
        let mut s = sherlock::parent(&dir.join("children"), &dir.join("parent"), &a.srs, &a.params_dir)?;
        s["telemetry"] = telemetry_since(&tel);
        s["profile"] = take_profile();
        let t = Instant::now();
        s["stock_receipt"] = verifier.request(json!({"stage": "parent", "dir": dir.join("parent"), "children": dir.join("children")}))?;
        s["stock_receipt_ms"] = json!(t.elapsed().as_secs_f64() * 1e3);
        stages.push(s);

        let tel = telemetry_snapshot();
        let mut s = sherlock::decider(&dir.join("parent"), &dir.join("terminal"), &a.srs)?;
        s["telemetry"] = telemetry_since(&tel);
        s["profile"] = take_profile();
        let t = Instant::now();
        s["stock_receipt"] = verifier.request(json!({"stage": "decider", "dir": dir.join("terminal"), "parent": dir.join("parent")}))?;
        s["stock_receipt_ms"] = json!(t.elapsed().as_secs_f64() * 1e3);
        stages.push(s);

        let chain_ms = chain.elapsed().as_secs_f64() * 1e3;
        for s in &stages {
            create_proof_ms += s["create_proof_ms"].as_f64().unwrap_or(0.0);
        }
        let production_chain_ms: f64 = stages.iter().map(|s| s["operational_ms"].as_f64().unwrap_or(0.0)).sum();
        let stock_receipt_ms_sum: f64 = stages.iter().map(|s| s["stock_receipt_ms"].as_f64().unwrap_or(0.0)).sum();
        let dispatch = if cfg!(feature = "g0") {
            Some(g0_dispatch_qualification(&stages))
        } else {
            None
        };
        let cpu_end = common::cpu_seconds();
        let receipt = json!({
            "schema": "loki-bench-chain-sample/1",
            "arm": ARM,
            "block": a.block,
            "sample_index": index,
            "cache": cache,
            "started_unix_ms": started_unix_ms,
            "chain_ms": chain_ms,
            "chain_ms_scope": "qualification-inclusive: SUITE CHAIN endpoint, wall from the first leaf to the last independent stock receipt (includes production stage checks and the extra stock-verifier round trips)",
            "qualification_inclusive_chain_ms": chain_ms,
            "production_chain_ms": production_chain_ms,
            "production_chain_ms_scope": "sum of the three stage operational windows (production APIs and their own checks), excluding the extra stock-verifier requests",
            "stock_receipt_ms_sum": stock_receipt_ms_sum,
            "g0_dispatch": match &dispatch { None => Value::Null, Some(Ok(v)) => v.clone(), Some(Err(e)) => json!({"passed": false, "error": e}) },
            "create_proof_sum_ms": create_proof_ms,
            "create_proof_scope": "sum of the externally timed proof calls of the four stage proofs (A102+A80 as one nested call)",
            "stages": stages,
            "status_start": status_start,
            "status_end": common::status_bytes(),
            "cpu_user_s": cpu_end.0 - cpu_start.0,
            "cpu_system_s": cpu_end.1 - cpu_start.1,
            "lifetime_maxrss_bytes": common::lifetime_maxrss_bytes(),
            "verifier_init": verifier.init,
            "identity": identity(),
        });
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&receipts_path)?;
        writeln!(f, "{receipt}")?;
        if let Some(Err(e)) = dispatch {
            return Err(error(format!("G0 dispatch qualification failed ({cache}): {e}")));
        }
        println!("{}", json!({"event": "sample", "arm": ARM, "cache": cache, "chain_ms": chain_ms, "production_chain_ms": production_chain_ms, "create_proof_sum_ms": create_proof_ms}));
        std::io::stdout().flush()?;
    }
    let verifier_rusage = verifier.finish();
    // Live observation immediately before exit (never read after exit).
    let exit = json!({
        "schema": "loki-bench-driver-exit/1",
        "arm": ARM,
        "block": a.block,
        "status_before_exit": common::status_bytes(),
        "prover_lifetime_maxrss_bytes": common::lifetime_maxrss_bytes(),
        "verifier_rusage": verifier_rusage,
        "process_wall_s": process_start.elapsed().as_secs_f64(),
    });
    std::fs::write(a.out.join("driver-exit.json"), format!("{exit}\n"))?;
    println!("{}", json!({"event": "exit", "record": exit}));
    eprintln!(
        "bench-driver: {} samples in {:.3}s; receipts {}",
        a.samples.len(),
        process_start.elapsed().as_secs_f64(),
        receipts_path.display()
    );
    Ok(())
}

#[cfg(feature = "circuits")]
mod sherlock {
    //! Stage bodies, transcribed from the pinned production tests:
    //! `mori-backend/tests/sherlock_linked_children.rs` (A2),
    //! `mori-recursion/tests/sherlock_parent.rs` (A3) and
    //! `mori-recursion/tests/sherlock_terminal.rs::terminal_proof(false)` (DEC).
    //! Negative controls in those tests are qualification work and run in the
    //! stock verifier, outside these timers.

    use super::common::{self, error, publish_dir, Result, Windows};
    use ff::Field;
    use group::Curve;
    use midnight_curves::Bls12;
    use midnight_proofs::{poly::kzg::params::ParamsKZG, utils::SerdeFormat};
    use midnight_transient_crypto::{
        curve::Fr,
        proofs::{Proof, VerifierKey, PARAMS_VERIFIER},
    };
    use midnight_zk_stdlib::{prove, setup_pk, verify, Relation};
    use mori_backend::proving::{ProofOptions, MAX_K};
    use mori_backend::sherlock_child::ChildKey;
    use mori_backend::sherlock_link::{
        produce_linked_requests, prove_linked_children, verify_linked_children, write_link_dir,
        LinkShape, LinkedRequests, ReturnChildKey,
    };
    use mori_backend::sherlock_machine::literal_bridge::LiteralSourceIdentity;
    use mori_backend::sherlock_machine::literal_host::LiteralCapacities;
    use mori_backend::sherlock_machine::literal_witness::core_program_capacities;
    use mori_core::sherlock::layout_a::prepare_a;
    use mori_core::sherlock::{Expr, Program, Value as MValue, K_SOURCE_SHA256};
    use mori_recursion::{
        child::SherlockChild,
        decider::{DeciderRelation, DeciderWitness},
        inner::{verify_offcircuit, vk_name},
        parent::{
            keygen_sherlock_decider, prove_link_parent, sherlock_decider_relation, write_w_dir,
            LinkPins, ParentKeygenPlan, SherlockLinkRelation, SherlockLinkWitness,
        },
        template::Template,
        util::{f_from_hex, f_to_hex, sha256_hex, vk_bytes, vk_from_bytes, F},
    };
    use serde_json::{json, Value};
    use std::{fs, io::Cursor, path::Path, sync::Arc, time::Instant};

    const REFERENCE_RS: &[u8] =
        include_bytes!("../deps/moriarty/crates/mori-core/src/sherlock/reference.rs");
    const ACCEPT_TEMPLATE: &[u8] =
        include_bytes!("../deps/moriarty/crates/mori-recursion/tests/fixtures/accept-template.json");
    pub const SRS_LEN: u64 = 201_326_980;
    pub const SRS_SHA256: &str = "1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1";
    const INSTANCE_ID: &str = "489080d3e57cacab615cd0cde4c87914b47c4ff0ab5857d865ee2474ecd45c00";

    /// The current A3 fixture (SUITE.md "Workloads"): literal UInt(7).
    fn requests() -> Result<(LinkedRequests, LinkShape)> {
        let capacities = LiteralCapacities {
            program: [2, 8, 2, 2, 2, 2, 1, 1, 1, 1, 4, 4, 5, 2, 2],
            state: [1; 6],
            stack_limit: 1,
        };
        let program = Arc::new(Program {
            functions: Vec::new(),
            entry: Arc::new(Expr::Literal(MValue::UInt(7))),
        });
        let prepared = prepare_a(
            program.clone(),
            core_program_capacities(&capacities).map_err(|e| error(format!("{e:?}")))?,
        )
        .map_err(|e| error(format!("{e:?}")))?;
        let identity = LiteralSourceIdentity {
            k_source_sha256: K_SOURCE_SHA256.into(),
            reference_source_sha256: sha256_hex(REFERENCE_RS),
            program_source_sha256: sha256_hex(
                &prepared.program_bytes().map_err(|e| error(format!("{e:?}")))?,
            ),
        };
        produce_linked_requests(program, &capacities, 2, 1, &identity)
            .map_err(|e| error(format!("{e:?}")))
    }

    fn options(params_dir: &Path) -> ProofOptions {
        ProofOptions {
            params_dir: params_dir.to_path_buf(),
            max_k: MAX_K,
        }
    }

    /// Authenticated k20 ceremony SRS (recurring load: inside the timers).
    fn read_srs(path: &Path) -> Result<ParamsKZG<Bls12>> {
        if fs::metadata(path)?.len() != SRS_LEN {
            return Err(error("SRS length differs from the pinned k20 ceremony"));
        }
        let bytes = fs::read(path)?;
        if common::sha256_hex(&bytes) != SRS_SHA256 {
            return Err(error("SRS digest differs from the pinned k20 ceremony"));
        }
        let mut cursor = Cursor::new(bytes);
        let params = ParamsKZG::<Bls12>::read_custom(&mut cursor, SerdeFormat::RawBytesUnchecked)?;
        if cursor.position() != cursor.get_ref().len() as u64 {
            return Err(error("trailing SRS bytes"));
        }
        Ok(params)
    }

    fn artifact_ids(dir: &Path, names: &[&str]) -> Result<Value> {
        let mut m = serde_json::Map::new();
        for name in names {
            let mut e = serde_json::Map::new();
            for suffix in ["vk", "proof", "instance.json"] {
                let p = dir.join(format!("{name}.{suffix}"));
                if p.exists() {
                    e.insert(format!("{suffix}_sha256"), json!(common::file_sha256(&p)?));
                    e.insert(format!("{suffix}_bytes"), json!(fs::metadata(&p)?.len()));
                }
            }
            m.insert(name.to_string(), Value::Object(e));
        }
        Ok(Value::Object(m))
    }

    fn stage_json(name: &str, w: &Windows, create_proof_ms: f64, extra: Value) -> Value {
        let mut v = json!({
            "stage": name,
            "operational_ms": w.total_ms(),
            "windows_ms": w.to_json(),
            "create_proof_ms": create_proof_ms,
            "status_end": common::status_bytes(),
        });
        if let (Some(o), Value::Object(e)) = (v.as_object_mut(), extra) {
            o.extend(e);
        }
        v
    }

    /// A2: both linked leaves (A102 then A80), production verification and
    /// atomic publication of the public link directory.
    pub fn children(out: &Path, params_dir: &Path) -> Result<Value> {
        let opts = options(params_dir);
        let mut w = Windows::start();
        let (req, shape) = requests()?;
        w.lap("requests");
        // prove_linked_children reconstructs both keys (SRS reload + setup_vk)
        // and proves A102 then A80; the nested split is not exposed by the API.
        let t = Instant::now();
        let children = prove_linked_children(&req, &opts).map_err(|e| error(format!("A2 prove: {e:?}")))?;
        let prove_ms = t.elapsed().as_secs_f64() * 1e3;
        w.lap("key_reconstruct_and_prove");
        let verified = verify_linked_children(&req, &children, &opts)
            .map_err(|e| error(format!("A2 production verification: {e:?}")))?;
        if verified.shape() != shape {
            return Err(error("A2 verified shape differs"));
        }
        w.lap("production_verify");
        publish_dir(out, |tmp| {
            write_link_dir(tmp, &req, &children, &verified)
                .map(|_| ())
                .map_err(|e| error(format!("A2 publish: {e:?}")))
        })?;
        w.lap("publish");
        let ks = [verified.a102().circuit_k, verified.a80().circuit_k];
        Ok(stage_json(
            "A2:A102+A80",
            &w,
            prove_ms,
            json!({
                "k": {"a102": ks[0], "a80": ks[1]},
                "create_proof_scope": "prove_linked_children: both key reconstructions plus both proofs (nested; not separable through the pinned API)",
                "artifacts": artifact_ids(out, &["a102", "a80"])?,
            }),
        ))
    }

    /// A3: SherlockLinkRelation at k20 from the published children.
    pub fn parent(children: &Path, out: &Path, srs: &Path, params_dir: &Path) -> Result<Value> {
        let opts = options(params_dir);
        let mut w = Windows::start();
        for name in ["a102", "a80"] {
            for suffix in ["vk", "instance.json", "proof"] {
                let p = children.join(format!("{name}.{suffix}"));
                if !p.is_file() {
                    return Err(error(format!("missing dependency artifact {}", p.display())));
                }
            }
        }
        let (requests, shape) = requests()?;
        w.lap("requests");
        let a102_key = ChildKey::reconstruct(&requests.a102, &opts).map_err(|e| error(format!("{e:?}")))?;
        let a80_key = ReturnChildKey::reconstruct(&requests.a80, &opts).map_err(|e| error(format!("{e:?}")))?;
        w.lap("child_key_reconstruct");
        let pins = LinkPins {
            a102_vk_sha256: a102_key.vk_sha256(),
            a80_vk_sha256: a80_key.vk_sha256(),
            a102_context_hash: a102_key.claim().context_hash.clone(),
            a80_context_hash: a80_key.claim().context_hash.clone(),
            program_lanes: shape.program_lanes.try_into()?,
            state_lanes: shape.state_lanes.try_into()?,
        };
        let read_child = |name: &str, pin: &str, expected: Vec<F>| -> Result<SherlockChild> {
            let raw = fs::read(children.join(format!("{name}.vk")))?;
            if common::sha256_hex(&raw) != pin {
                return Err(error(format!(
                    "wrong child VK for {name}: published {} but the parent was built for {pin}",
                    common::sha256_hex(&raw)
                )));
            }
            let lanes: Vec<String> =
                serde_json::from_slice(&fs::read(children.join(format!("{name}.instance.json")))?)?;
            let instance = lanes.iter().map(|s| f_from_hex(s)).collect::<std::io::Result<Vec<_>>>()?;
            if instance != expected {
                return Err(error(format!("{name} instance differs from independent reconstruction")));
            }
            SherlockChild::from_bytes(
                &raw,
                pin,
                shape.instance_lanes,
                instance,
                fs::read(children.join(format!("{name}.proof")))?,
            )
            .map_err(|e| error(format!("{name}: {e}")))
        };
        let a102 = read_child("a102", &pins.a102_vk_sha256, a102_key.instance().iter().map(|x| x.0).collect())?;
        let a80 = read_child("a80", &pins.a80_vk_sha256, a80_key.instance().iter().map(|x| x.0).collect())?;
        w.lap("child_read_check");
        let relation = SherlockLinkRelation::from_pins(
            vk_from_bytes(a102_key.vk_bytes())?,
            vk_from_bytes(a80_key.vk_bytes())?,
            &pins,
        )?;
        let witness = SherlockLinkWitness::from_children(&a102, &a80, 0);
        w.lap("relation_witness");
        let plan = ParentKeygenPlan::new(&relation)?;
        let k = plan.model().k;
        let model = serde_json::to_value(plan.model()).unwrap_or(Value::Null);
        let model_ms = plan.model_elapsed().as_secs_f64() * 1e3;
        if k != 20 {
            return Err(error(format!("A3 requires a production k20 parent, model k={k}")));
        }
        w.lap("cost_model");
        let params = read_srs(srs)?;
        let s_g2 = params.s_g2().to_affine();
        w.lap("srs_load");
        let tmp_key = out.with_extension("key.partial");
        if tmp_key.exists() {
            fs::remove_dir_all(&tmp_key)?;
        }
        fs::create_dir_all(&tmp_key)?;
        let vk = plan.keygen(&params, &s_g2, &tmp_key.join("key"))?;
        w.lap("keygen_vk_pk");
        let t = Instant::now();
        let parent = prove_link_parent(&params, &s_g2, &relation, &vk, &witness).map_err(error)?;
        let prove_ms = t.elapsed().as_secs_f64() * 1e3;
        if parent.instance.len() != 28 || parent.proof.is_empty() {
            return Err(error("A3 produced an unexpected instance/proof shape"));
        }
        w.lap("prove_and_production_verify");
        publish_dir(out, |tmp| {
            write_w_dir(tmp, &parent, "Moriarty A3: fixed A102 to A80 at height zero")?;
            Ok(())
        })?;
        fs::remove_dir_all(&tmp_key).ok();
        w.lap("publish");
        Ok(stage_json(
            "A3",
            &w,
            prove_ms,
            json!({
                "k": k,
                "create_proof_scope": "prove_link_parent: off-circuit child checks, proof and its production verifications",
                "model": model,
                "model_internal_ms": model_ms,
                "artifacts": artifact_ids(out, &["w"])?,
            }),
        ))
    }

    /// DEC: terminal decider over the published parent (offline preparation).
    pub fn decider(parent_dir: &Path, out: &Path, srs: &Path) -> Result<Value> {
        let mut w = Windows::start();
        for suffix in ["vk", "proof", "instance.json"] {
            let p = parent_dir.join(format!("w.{suffix}"));
            if !p.is_file() {
                return Err(error(format!("missing dependency artifact {}", p.display())));
            }
        }
        let parent_raw = fs::read(parent_dir.join("w.vk"))?;
        let params = read_srs(srs)?;
        let vp = params.verifier_params();
        w.lap("srs_load");
        let parent_vk = vk_from_bytes(&parent_raw)?;
        let parent_proof = fs::read(parent_dir.join("w.proof"))?;
        let lanes: Vec<String> = serde_json::from_slice(&fs::read(parent_dir.join("w.instance.json"))?)?;
        let parent_instance = lanes.iter().map(|s| f_from_hex(s)).collect::<std::io::Result<Vec<_>>>()?;
        if parent_instance.len() != 28 {
            return Err(error("parent instance is not 28 lanes"));
        }
        verify::<SherlockLinkRelation, midnight_circuits::hash::poseidon::PoseidonState<F>>(
            &vp,
            &parent_vk,
            &parent_instance,
            None,
            &parent_proof,
        )
        .map_err(|e| error(format!("DEC parent verify: {e:?}")))?;
        verify_offcircuit(&vp, &parent_vk, &vk_name("w", &parent_vk), &parent_instance, &parent_proof, true)
            .map_err(error)?;
        w.lap("parent_production_verify");
        let template = Template::from_any_json(ACCEPT_TEMPLATE)?;
        let relation = sherlock_decider_relation(parent_vk, params.s_g2().to_affine(), template)?;
        let mut relation_bytes = Vec::new();
        relation.write_relation(&mut relation_bytes)?;
        w.lap("relation");
        let tmp = out.with_extension("partial");
        if tmp.exists() {
            fs::remove_dir_all(&tmp)?;
        }
        fs::create_dir_all(&tmp)?;
        let vk = keygen_sherlock_decider(&params, &relation, &tmp.join("key"))?;
        let candidate = vk_bytes(&vk);
        let k = vk.k();
        w.lap("keygen_vk");
        // The communication opening exists only in memory, never persisted.
        let mut rng = rand::rngs::OsRng;
        let instance_id = f_from_hex(INSTANCE_ID)?;
        let witness = DeciderWitness {
            w_instance: parent_instance,
            w_proof: parent_proof,
            instance_id,
            batch_no: F::ZERO,
            comm_rand: F::random(&mut rng),
        };
        let stmt = relation.stmt(&witness.w_instance, witness.instance_id, witness.batch_no);
        let public = relation
            .template
            .public_inputs(F::ONE, relation.comm_commitment_for(stmt, witness.comm_rand), stmt);
        w.lap("witness");
        let pk = setup_pk(&relation, &vk);
        w.lap("setup_pk");
        let t = Instant::now();
        let proof = prove::<DeciderRelation, blake2b_simd::State>(&params, &pk, &relation, &public, witness, &mut rng)
            .map_err(|e| error(format!("DEC prove: {e:?}")))?;
        let prove_ms = t.elapsed().as_secs_f64() * 1e3;
        w.lap("create_proof");
        drop(pk);
        verify::<DeciderRelation, blake2b_simd::State>(&vp, &vk, &public, None, &proof)
            .map_err(|e| error(format!("DEC verify: {e:?}")))?;
        let ledger_key = VerifierKey::from(vk.clone());
        ledger_key
            .verify(&PARAMS_VERIFIER, &Proof(proof.clone()), public.iter().copied().map(Fr))
            .map_err(|e| error(format!("DEC ledger verify: {e:?}")))?;
        let mut tagged = Vec::new();
        midnight_serialize::tagged_serialize(&ledger_key, &mut tagged)?;
        let decoded: VerifierKey = midnight_serialize::tagged_deserialize(&tagged[..])?;
        if decoded.original_bytes() != candidate {
            return Err(error("ledger key round trip changed the VK"));
        }
        decoded
            .verify(&PARAMS_VERIFIER, &Proof(proof.clone()), public.iter().copied().map(Fr))
            .map_err(|e| error(format!("DEC ledger readback verify: {e:?}")))?;
        w.lap("production_verify_ledger");
        fs::write(tmp.join("key/acceptValidity.verifier"), tagged)?;
        fs::write(tmp.join("terminal.proof"), &proof)?;
        fs::write(
            tmp.join("terminal.public.json"),
            serde_json::to_vec_pretty(&public.iter().map(f_to_hex).collect::<Vec<_>>())?,
        )?;
        fs::write(tmp.join("terminal.relation"), &relation_bytes)?;
        fs::rename(&tmp, out)?;
        w.lap("publish");
        Ok(stage_json(
            "DEC",
            &w,
            prove_ms,
            json!({
                "k": k,
                "create_proof_scope": "midnight_zk_stdlib::prove of the decider",
                "relation_sha256": sha256_hex(&relation_bytes),
                "vk_sha256": sha256_hex(&candidate),
                "proof_sha256": sha256_hex(&proof),
                "public_lanes": public.len(),
            }),
        ))
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn op(attempted: u64, succeeded: u64, error: u64, fallback: u64) -> Value {
        json!({"calls": attempted + 3, "cpu": 3, "attempted": attempted, "succeeded": succeeded, "error": error, "fallback": fallback})
    }
    fn stage(name: &str, msm: Value, ntt: Value) -> Value {
        json!({"stage": name, "telemetry": {"backend": "legacy-sppark:msm+srs-registry,ntt", "msm": msm, "ntt": ntt}})
    }

    #[test]
    fn accelerated_chain_with_zero_errors_qualifies() {
        let s = vec![
            stage("children", op(0, 0, 0, 0), op(4, 4, 0, 0)),
            stage("parent", op(9, 9, 0, 0), op(7, 7, 0, 0)),
            stage("decider", op(5, 5, 0, 0), op(3, 3, 0, 0)),
        ];
        assert!(g0_dispatch_qualification(&s).is_ok());
    }

    #[test]
    fn cpu_only_chain_is_rejected() {
        let s = vec![
            stage("children", op(0, 0, 0, 0), op(0, 0, 0, 0)),
            stage("parent", op(0, 0, 0, 0), op(0, 0, 0, 0)),
            stage("decider", op(0, 0, 0, 0), op(0, 0, 0, 0)),
        ];
        let e = g0_dispatch_qualification(&s).unwrap_err();
        assert!(e.contains("no accelerated"), "{e}");
    }

    #[test]
    fn any_fallback_or_error_is_rejected() {
        let s = vec![
            stage("children", op(0, 0, 0, 0), op(4, 4, 0, 0)),
            stage("parent", op(9, 8, 1, 1), op(7, 7, 0, 0)),
            stage("decider", op(5, 5, 0, 0), op(3, 3, 0, 0)),
        ];
        let e = g0_dispatch_qualification(&s).unwrap_err();
        assert!(e.contains("parent") && e.contains("fallback"), "{e}");
    }

    #[test]
    fn missing_msm_acceleration_across_the_chain_is_rejected() {
        let s = vec![
            stage("children", op(0, 0, 0, 0), op(4, 4, 0, 0)),
            stage("parent", op(0, 0, 0, 0), op(7, 7, 0, 0)),
            stage("decider", op(0, 0, 0, 0), op(3, 3, 0, 0)),
        ];
        let e = g0_dispatch_qualification(&s).unwrap_err();
        assert!(e.contains("msm"), "{e}");
    }

    #[test]
    fn missing_telemetry_is_rejected() {
        let s = vec![json!({"stage": "children", "telemetry": null})];
        assert!(g0_dispatch_qualification(&s).is_err());
    }
}
