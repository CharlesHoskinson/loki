//! Frozen run manifest: `key = value` lines, `#` comments. Every key is
//! validated; unknown keys, duplicate keys and environment outside the
//! allowlist are rejected before any lock is taken.
//!
//! ```text
//! schema = loki-bench-launch/1
//! cohort = p7-smoke-20261025
//! collection = false            # true only for a SUITE collection cohort
//! out_root = /abs/campaign/runs
//! srs = /abs/bls_midnight_2p20
//! srs_sha256 = <hex>
//! params_dir = /abs/zk-params
//! stock_verifier = /abs/stock-verifier
//! stock_verifier_sha256 = <hex>
//! timeout_s = 5400              # per arm process
//! idle_gate = record            # enforce | record | off
//! idle_window_s = 60
//! idle_max_wait_s = 900
//! heavy = dw                    # dw: assert an ancestor holds heavy.lock; own: take it
//! gpu_lock = block              # block: hold across the block; gpu-arms: only around GPU arms
//! nvml = on
//! pass_env = PATH,LD_LIBRARY_PATH
//! staged = /abs/deps/STAGED
//! source.<name> = <value>       # provenance copied into every receipt
//! budget.N4.mask = 0-3
//! budget.N4.workers = 4
//! arm.S0.binary = /abs/bench-driver
//! arm.S0.sha256 = <hex>
//! arm.S0.gpu = false
//! arm.S0.features = circuits
//! arm.S0.workspace = stock
//! arm.G0.env.LOKI_GPU_MSM_MIN = 16384
//! block.B000 = N4: S0 L0 G0
//! samples = cold,warm
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Variables an arm may set (besides RAYON_NUM_THREADS, which comes from the budget).
pub const SETTABLE_ENV: &[&str] = &["LOKI_GPU_MSM_MIN", "LOKI_GPU_SRS_BUDGET_BYTES", "LOKI_PROFILE"];
/// Variables that may be relayed from the launcher's own environment.
pub const PASSABLE_ENV: &[&str] = &["PATH", "LD_LIBRARY_PATH"];
/// Variables that must never reach an arm.
pub const FORBIDDEN_ENV: &[&str] = &["LOKI_NTT_CPU", "LOKI_NTT_FORCE_ERROR"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleMode {
    Enforce,
    Record,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeavyMode {
    /// DoctorWatson `heavy` is our ancestor and holds heavy.lock.
    Dw,
    /// Take heavy.lock ourselves (blocking, queue time recorded).
    Own,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuLockMode {
    Block,
    GpuArms,
}

#[derive(Clone, Debug)]
pub struct Budget {
    pub mask: String,
    pub cpus: Vec<u32>,
    pub workers: u32,
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub binary: PathBuf,
    pub sha256: String,
    pub gpu: bool,
    pub features: String,
    pub workspace: String,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub budget: String,
    pub arms: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub raw: String,
    pub cohort: String,
    pub collection: bool,
    pub out_root: PathBuf,
    pub srs: PathBuf,
    pub srs_sha256: String,
    pub params_dir: PathBuf,
    pub stock_verifier: PathBuf,
    pub stock_verifier_sha256: String,
    pub timeout_s: u64,
    pub idle: IdleMode,
    pub idle_window_s: u64,
    pub idle_max_wait_s: u64,
    pub heavy: HeavyMode,
    pub gpu_lock: GpuLockMode,
    pub nvml: bool,
    pub pass_env: Vec<String>,
    pub staged: Option<PathBuf>,
    pub samples: String,
    pub source: Vec<(String, String)>,
    pub budgets: BTreeMap<String, Budget>,
    pub arms: BTreeMap<String, Arm>,
    pub blocks: BTreeMap<String, Block>,
}

pub fn parse_mask(mask: &str) -> Result<Vec<u32>, String> {
    let mut cpus = Vec::new();
    for part in mask.split(',') {
        let part = part.trim();
        let (lo, hi) = match part.split_once('-') {
            Some((a, b)) => (a, b),
            None => (part, part),
        };
        let lo: u32 = lo.parse().map_err(|_| format!("bad CPU mask {mask:?}"))?;
        let hi: u32 = hi.parse().map_err(|_| format!("bad CPU mask {mask:?}"))?;
        if lo > hi {
            return Err(format!("bad CPU range {part:?}"));
        }
        for c in lo..=hi {
            if cpus.contains(&c) {
                return Err(format!("CPU {c} repeated in mask {mask:?}"));
            }
            cpus.push(c);
        }
    }
    cpus.sort_unstable();
    Ok(cpus)
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn abs_path(key: &str, v: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(v);
    if !p.is_absolute() {
        return Err(format!("{key} must be an absolute path"));
    }
    Ok(p)
}

fn boolean(key: &str, v: &str) -> Result<bool, String> {
    match v {
        "true" | "on" => Ok(true),
        "false" | "off" => Ok(false),
        _ => Err(format!("{key} must be true/false")),
    }
}

fn ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Manifest {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut kv: BTreeMap<String, String> = BTreeMap::new();
        for (n, line) in raw.lines().enumerate() {
            let line = match line.find('#') {
                Some(i) => &line[..i],
                None => line,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected key = value", n + 1))?;
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            if kv.insert(k.clone(), v).is_some() {
                return Err(format!("line {}: duplicate key {k}", n + 1));
            }
        }
        let mut take = |k: &str| kv.remove(k).ok_or_else(|| format!("missing key {k}"));
        if take("schema")? != "loki-bench-launch/1" {
            return Err("schema must be loki-bench-launch/1".into());
        }
        let cohort = take("cohort")?;
        if !ident(&cohort) {
            return Err("cohort must be [A-Za-z0-9_-]+".into());
        }
        let collection = boolean("collection", &take("collection")?)?;
        let out_root = abs_path("out_root", &take("out_root")?)?;
        let srs = abs_path("srs", &take("srs")?)?;
        let srs_sha256 = take("srs_sha256")?;
        let params_dir = abs_path("params_dir", &take("params_dir")?)?;
        let stock_verifier = abs_path("stock_verifier", &take("stock_verifier")?)?;
        let stock_verifier_sha256 = take("stock_verifier_sha256")?;
        for (k, v) in [("srs_sha256", &srs_sha256), ("stock_verifier_sha256", &stock_verifier_sha256)] {
            if !is_hex64(v) {
                return Err(format!("{k} must be 64 lowercase hex digits"));
            }
        }
        let num = |k: &str, v: String| v.parse::<u64>().map_err(|_| format!("{k} must be an integer"));
        let timeout_s = num("timeout_s", take("timeout_s")?)?;
        let idle = match take("idle_gate")?.as_str() {
            "enforce" => IdleMode::Enforce,
            "record" => IdleMode::Record,
            "off" => IdleMode::Off,
            _ => return Err("idle_gate must be enforce|record|off".into()),
        };
        if collection && idle != IdleMode::Enforce {
            return Err("a collection cohort requires idle_gate = enforce".into());
        }
        let idle_window_s = num("idle_window_s", take("idle_window_s")?)?;
        let idle_max_wait_s = num("idle_max_wait_s", take("idle_max_wait_s")?)?;
        let heavy = match take("heavy")?.as_str() {
            "dw" => HeavyMode::Dw,
            "own" => HeavyMode::Own,
            _ => return Err("heavy must be dw|own".into()),
        };
        let gpu_lock = match take("gpu_lock")?.as_str() {
            "block" => GpuLockMode::Block,
            "gpu-arms" => GpuLockMode::GpuArms,
            _ => return Err("gpu_lock must be block|gpu-arms".into()),
        };
        if collection && gpu_lock != GpuLockMode::Block {
            return Err("a collection cohort requires gpu_lock = block (SUITE single owner)".into());
        }
        let nvml = boolean("nvml", &take("nvml")?)?;
        let pass_env: Vec<String> = take("pass_env")?
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        for k in &pass_env {
            if !PASSABLE_ENV.contains(&k.as_str()) {
                return Err(format!("pass_env {k} is not in the relay allowlist {PASSABLE_ENV:?}"));
            }
        }
        let staged = match kv.remove("staged") {
            Some(v) => Some(abs_path("staged", &v)?),
            None => None,
        };
        let samples = kv.remove("samples").unwrap_or_else(|| "cold,warm".into());
        if samples.split(',').any(|s| !matches!(s, "cold" | "warm")) {
            return Err("samples must be a list of cold/warm".into());
        }

        let mut source = Vec::new();
        let mut budgets: BTreeMap<String, Budget> = BTreeMap::new();
        let mut arm_kv: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut blocks = BTreeMap::new();
        for (k, v) in kv {
            let parts: Vec<&str> = k.splitn(3, '.').collect();
            match parts.as_slice() {
                ["source", name] if ident(name) => source.push((name.to_string(), v)),
                ["budget", name, field] if ident(name) => {
                    let b = budgets.entry(name.to_string()).or_insert(Budget {
                        mask: String::new(),
                        cpus: Vec::new(),
                        workers: 0,
                    });
                    match *field {
                        "mask" => {
                            b.cpus = parse_mask(&v)?;
                            b.mask = v;
                        }
                        "workers" => b.workers = v.parse().map_err(|_| format!("{k} must be an integer"))?,
                        _ => return Err(format!("unknown key {k}")),
                    }
                }
                ["arm", name, field] if ident(name) => {
                    arm_kv.entry(name.to_string()).or_default().insert(field.to_string(), v);
                }
                ["block", name] if ident(name) => {
                    let (budget, arms) = v
                        .split_once(':')
                        .ok_or_else(|| format!("{k} must be `BUDGET: ARM ARM ...`"))?;
                    let arms: Vec<String> = arms.split_whitespace().map(str::to_string).collect();
                    if arms.is_empty() {
                        return Err(format!("{k} has no arms"));
                    }
                    blocks.insert(
                        name.to_string(),
                        Block {
                            budget: budget.trim().to_string(),
                            arms,
                        },
                    );
                }
                _ => return Err(format!("unknown key {k}")),
            }
        }
        for (name, b) in &budgets {
            if b.cpus.is_empty() || b.workers == 0 {
                return Err(format!("budget {name} needs mask and workers"));
            }
            if b.workers as usize != b.cpus.len() {
                return Err(format!(
                    "budget {name}: workers {} must equal the mask CPU count {}",
                    b.workers,
                    b.cpus.len()
                ));
            }
        }
        let mut arms = BTreeMap::new();
        for (name, mut f) in arm_kv {
            let mut req = |k: &str| f.remove(k).ok_or_else(|| format!("arm {name}: missing {k}"));
            let binary = abs_path("binary", &req("binary")?)?;
            let sha256 = req("sha256")?;
            if !is_hex64(&sha256) {
                return Err(format!("arm {name}: sha256 must be 64 lowercase hex digits"));
            }
            let gpu = boolean("gpu", &req("gpu")?)?;
            let features = req("features")?;
            let workspace = req("workspace")?;
            let mut env = Vec::new();
            for (k, v) in f {
                let var = k
                    .strip_prefix("env.")
                    .ok_or_else(|| format!("arm {name}: unknown key {k}"))?;
                if FORBIDDEN_ENV.contains(&var) {
                    return Err(format!("arm {name}: {var} is forbidden"));
                }
                if !SETTABLE_ENV.contains(&var) {
                    return Err(format!("arm {name}: {var} is not in the allowlist {SETTABLE_ENV:?}"));
                }
                env.push((var.to_string(), v));
            }
            arms.insert(
                name,
                Arm {
                    binary,
                    sha256,
                    gpu,
                    features,
                    workspace,
                    env,
                },
            );
        }
        for (name, b) in &blocks {
            if !budgets.contains_key(&b.budget) {
                return Err(format!("block {name}: unknown budget {}", b.budget));
            }
            for a in &b.arms {
                if !arms.contains_key(a) {
                    return Err(format!("block {name}: unknown arm {a}"));
                }
            }
        }
        Ok(Self {
            raw: raw.to_string(),
            cohort,
            collection,
            out_root,
            srs,
            srs_sha256,
            params_dir,
            stock_verifier,
            stock_verifier_sha256,
            timeout_s,
            idle,
            idle_window_s,
            idle_max_wait_s,
            heavy,
            gpu_lock,
            nvml,
            pass_env,
            staged,
            samples,
            source,
            budgets,
            arms,
            blocks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1";

    fn base() -> String {
        format!(
            "schema = loki-bench-launch/1\ncohort = t\ncollection = false\nout_root = /o\n\
             srs = /s\nsrs_sha256 = {H}\nparams_dir = /p\nstock_verifier = /v\n\
             stock_verifier_sha256 = {H}\ntimeout_s = 10\nidle_gate = record\n\
             idle_window_s = 1\nidle_max_wait_s = 1\nheavy = own\ngpu_lock = block\n\
             nvml = off\npass_env = PATH\nbudget.N4.mask = 0-3\nbudget.N4.workers = 4\n\
             arm.S0.binary = /b\narm.S0.sha256 = {H}\narm.S0.gpu = false\n\
             arm.S0.features = circuits\narm.S0.workspace = stock\nblock.B000 = N4: S0\n"
        )
    }

    #[test]
    fn accepts_a_complete_manifest() {
        let m = Manifest::parse(&base()).unwrap();
        assert_eq!(m.budgets["N4"].cpus, vec![0, 1, 2, 3]);
        assert_eq!(m.blocks["B000"].arms, vec!["S0".to_string()]);
        assert_eq!(m.samples, "cold,warm");
    }

    #[test]
    fn rejects_forbidden_and_unlisted_environment() {
        let e = Manifest::parse(&(base() + "arm.S0.env.LOKI_NTT_CPU = 1\n")).unwrap_err();
        assert!(e.contains("forbidden"), "{e}");
        let e = Manifest::parse(&(base() + "arm.S0.env.HOME = /x\n")).unwrap_err();
        assert!(e.contains("allowlist"), "{e}");
        let e = Manifest::parse(&(base() + "arm.S0.env.RAYON_NUM_THREADS = 8\n")).unwrap_err();
        assert!(e.contains("allowlist"), "{e}");
        let e = Manifest::parse(&base().replace("pass_env = PATH", "pass_env = PATH,HOME")).unwrap_err();
        assert!(e.contains("relay allowlist"), "{e}");
    }

    #[test]
    fn rejects_worker_count_that_differs_from_mask() {
        let e = Manifest::parse(&base().replace("workers = 4", "workers = 20")).unwrap_err();
        assert!(e.contains("must equal"), "{e}");
    }

    #[test]
    fn rejects_unknown_keys_duplicates_and_unknown_arms() {
        assert!(Manifest::parse(&(base() + "colour = blue\n")).unwrap_err().contains("unknown key"));
        assert!(Manifest::parse(&(base() + "cohort = again\n")).unwrap_err().contains("duplicate"));
        let e = Manifest::parse(&base().replace("N4: S0", "N4: S0 X9")).unwrap_err();
        assert!(e.contains("unknown arm"), "{e}");
    }

    #[test]
    fn collection_requires_enforced_idle_gate_and_block_gpu_lock() {
        let m = base().replace("collection = false", "collection = true");
        assert!(Manifest::parse(&m).unwrap_err().contains("idle_gate"));
        let m = m.replace("idle_gate = record", "idle_gate = enforce").replace("gpu_lock = block", "gpu_lock = gpu-arms");
        assert!(Manifest::parse(&m).unwrap_err().contains("gpu_lock"));
    }

    #[test]
    fn masks_parse_ranges_and_reject_repeats() {
        assert_eq!(parse_mask("0-1,4").unwrap(), vec![0, 1, 4]);
        assert!(parse_mask("0-3,2").is_err());
        assert!(parse_mask("3-1").is_err());
    }
}
