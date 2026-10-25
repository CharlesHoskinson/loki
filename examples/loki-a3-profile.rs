//! Run the real Sherlock A3 test without changing the Moriarty worktree.

use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

use sha2::{Digest, Sha256};

const MORIARTY_COMMIT: &str = "6ee918696ab907244c703bbd94bfea9acde7ddd1";

const PUBLIC_INPUTS: [&str; 6] = [
    "a102.vk",
    "a102.instance.json",
    "a102.proof",
    "a80.vk",
    "a80.instance.json",
    "a80.proof",
];
const INPUT_SNAPSHOT_ENV: &str = "LOKI_A3_INPUT_SNAPSHOT";
const INPUT_MANIFEST_ENV: &str = "LOKI_A3_INPUT_MANIFEST";
type ProfileResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct InputArtifact {
    original_path: PathBuf,
    snapshot_path: PathBuf,
    size_bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct InputManifest {
    original_children: PathBuf,
    snapshot_children: PathBuf,
    artifacts: Vec<InputArtifact>,
}

fn fingerprint(path: &Path) -> ProfileResult<(u64, String)> {
    let mut file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(format!("A3 public input is not a file: {}", path.display()).into());
    }
    let mut hasher = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn unique_directory(parent: &Path, prefix: &str) -> ProfileResult<PathBuf> {
    fs::create_dir_all(parent)?;
    let sequence = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let path = parent.join(format!("{prefix}-{sequence}-{}", std::process::id()));
    // create_dir fails on a collision, so existing runs can never be overwritten.
    fs::create_dir(&path)?;
    Ok(path)
}

fn latest_children(a2: &Path) -> ProfileResult<PathBuf> {
    let mut children = fs::read_dir(a2)?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("children"))
        .filter(|path| path.is_dir())
        .map(|path| Ok((fs::metadata(&path)?.modified()?, path)))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    children.sort_by_key(|(modified, _)| *modified);
    Ok(children.pop().ok_or("no A2 children directory")?.1)
}

fn snapshot_latest_inputs(a2: &Path, output: &Path) -> ProfileResult<InputManifest> {
    let original_children = fs::canonicalize(latest_children(a2)?)?;
    let snapshot_children = fs::canonicalize(unique_directory(output, "inputs")?)?;
    let artifacts = PUBLIC_INPUTS
        .iter()
        .map(|name| {
            let original_path = original_children.join(name);
            let (size_bytes, sha256) = fingerprint(&original_path)?;
            Ok(InputArtifact {
                snapshot_path: snapshot_children.join(name),
                original_path,
                size_bytes,
                sha256,
            })
        })
        .collect::<ProfileResult<Vec<_>>>()?;
    let inputs = InputManifest {
        original_children,
        snapshot_children,
        artifacts,
    };
    for artifact in &inputs.artifacts {
        fs::copy(&artifact.original_path, &artifact.snapshot_path)?;
    }
    inputs.verify()?;
    // Reject a source change during acquisition. Once acquired, source/newest
    // changes are irrelevant: all modes consume only this verified snapshot.
    for artifact in &inputs.artifacts {
        if fingerprint(&artifact.original_path)? != (artifact.size_bytes, artifact.sha256.clone()) {
            return Err(format!(
                "A3 source changed while snapshotting: {}",
                artifact.original_path.display()
            )
            .into());
        }
    }
    fs::write(
        inputs.snapshot_children.join("input-manifest.json"),
        serde_json::to_string_pretty(&inputs)? + "\n",
    )?;
    Ok(inputs)
}

impl InputManifest {
    fn verify(&self) -> ProfileResult<()> {
        if self.artifacts.len() != PUBLIC_INPUTS.len() {
            return Err("A3 input manifest must contain exactly six public artifacts".into());
        }
        for (artifact, name) in self.artifacts.iter().zip(PUBLIC_INPUTS) {
            if artifact.original_path != self.original_children.join(name)
                || artifact.snapshot_path != self.snapshot_children.join(name)
            {
                return Err("A3 input manifest paths differ from the six public artifacts".into());
            }
            let actual = fingerprint(&artifact.snapshot_path)?;
            if actual != (artifact.size_bytes, artifact.sha256.clone()) {
                return Err(
                    format!("A3 input changed: {}", artifact.snapshot_path.display()).into(),
                );
            }
        }
        Ok(())
    }

    fn record(&self, output: &Path, mode: &str) -> ProfileResult<()> {
        self.verify()?;
        fs::write(
            output.join("input-manifest.json"),
            serde_json::to_string_pretty(self)? + "\n",
        )?;
        println!(
            "A3 mode={mode} input_snapshot={} original_children={}",
            self.snapshot_children.display(),
            self.original_children.display()
        );
        for artifact in &self.artifacts {
            println!(
                "A3 input original={} snapshot={} size_bytes={} sha256={}",
                artifact.original_path.display(),
                artifact.snapshot_path.display(),
                artifact.size_bytes,
                artifact.sha256
            );
        }
        Ok(())
    }
}

fn pinned_inputs(snapshot: &Path, expected_manifest: &str) -> ProfileResult<InputManifest> {
    let inputs: InputManifest = serde_json::from_str(expected_manifest)?;
    if inputs.snapshot_children != snapshot {
        return Err("A3 snapshot path differs from expected manifest".into());
    }
    inputs.verify()?;
    Ok(inputs)
}

fn mode_inputs(a2: &Path, output: &Path) -> ProfileResult<InputManifest> {
    match (
        env::var_os(INPUT_SNAPSHOT_ENV),
        env::var_os(INPUT_MANIFEST_ENV),
    ) {
        (Some(snapshot), Some(manifest)) => pinned_inputs(
            Path::new(&snapshot),
            &manifest
                .into_string()
                .map_err(|_| "A3 expected manifest is not UTF-8")?,
        ),
        (None, None) => snapshot_latest_inputs(a2, output),
        _ => Err("A3 requires both input snapshot and expected manifest".into()),
    }
}

fn check_backend_attribution(
    a3: &serde_json::Value,
    mode: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let gpu = mode.starts_with("gpu-");
    if a3["rayon_threads"] != 4
        || a3["blst_threads"] != 1
        || a3["cuda_msm"] != gpu
        || a3["cuda_ntt"] != gpu
    {
        return Err("A3 profile has invalid worker settings or CUDA features".into());
    }
    let mut gpu_msm = 0;
    let mut gpu_ntt = 0;
    let mut fallback = 0;
    for (phase, timing) in a3["phases"].as_object().ok_or("missing phases")? {
        for (backend, counts) in timing["backends"]
            .as_object()
            .ok_or("missing backend counters")?
        {
            let calls = counts["calls"].as_u64().ok_or("missing call count")?;
            match backend.as_str() {
                "msm.gpu" => gpu_msm += calls,
                "ntt.gpu" => gpu_ntt += calls,
                "msm.cpu_fallback" | "ntt.gpu_error" => fallback += calls,
                _ => {}
            }
            println!("A3 backend phase={phase} outcome={backend} counts={counts}");
        }
    }
    println!("A3 gpu_msm={gpu_msm} gpu_ntt={gpu_ntt} fallback={fallback}");
    if fallback != 0 || (mode.starts_with("gpu-") && (gpu_msm == 0 || gpu_ntt == 0)) {
        return Err("A3 GPU attribution failed; inspect phase counters and stderr".into());
    }
    if mode.starts_with("cpu-") && (gpu_msm != 0 || gpu_ntt != 0) {
        return Err("CPU baseline unexpectedly used a GPU".into());
    }
    Ok(())
}

fn loki_head(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err("cannot read Loki source HEAD".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn check_argument_coefficients(a3: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let phases = a3["phases"].as_object().ok_or("missing phases")?;
    let mut total = 0;
    for (phase, timing) in phases {
        let backends = timing["backends"]
            .as_object()
            .ok_or("missing backend counters")?;
        if let Some(counts) = backends.get("argument_commit.lagrange") {
            if counts["calls"].as_u64() != Some(0) {
                return Err("A3 retained a Lagrange argument commitment".into());
            }
        }
        if let Some(counts) = backends.get("argument_commit.coeff") {
            let expected = match phase.as_str() {
                "lookup_prepare" => 10,
                "permutation_products" => 6,
                _ => return Err(format!("unexpected coefficient commitment phase {phase}").into()),
            };
            if counts["calls"].as_u64() != Some(expected)
                || counts["elements"].as_u64() != Some(expected * 1048576)
                || counts["sizes"] != serde_json::json!({"1048576": expected})
            {
                return Err(
                    format!("A3 coefficient commitment counts/size mismatch in {phase}").into(),
                );
            }
            total += expected;
        }
    }
    if total != 16 {
        return Err(
            "A3 requires all 16 argument coefficient commitments (10 lookup, 6 permutation)".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            Self(unique_directory(&env::temp_dir(), "loki-a3-input-test").unwrap())
        }

        fn children(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.0.join("a2").join(name).join("children");
            fs::create_dir_all(&path).unwrap();
            for name in PUBLIC_INPUTS {
                fs::write(path.join(name), contents).unwrap();
            }
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn matched_inputs_stay_fixed_after_source_mutation_and_newest_change() {
        let fixture = Fixture::new();
        let original = fixture.children("original", b"abc");
        // Only the six consumed public files may enter the snapshot.
        fs::create_dir(original.join("witnesses")).unwrap();
        fs::write(original.join("witnesses/private"), b"do not copy").unwrap();
        let inputs =
            snapshot_latest_inputs(&fixture.0.join("a2"), &fixture.0.join("snapshots")).unwrap();
        let expected = serde_json::to_string(&inputs).unwrap();
        let newer = fixture.children("newer", b"replacement");
        // Set mtimes explicitly: no sleeps or filesystem timestamp assumptions.
        fs::File::open(&original)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH)
            .unwrap();
        assert_eq!(latest_children(&fixture.0.join("a2")).unwrap(), newer);
        for name in PUBLIC_INPUTS {
            fs::write(original.join(name), b"changed original").unwrap();
        }
        for mode in ["gpu-after", "gpu-before", "cpu-before"] {
            let child = pinned_inputs(&inputs.snapshot_children, &expected).unwrap();
            child.verify().unwrap();
            for artifact in &child.artifacts {
                assert_eq!(fs::read(&artifact.snapshot_path).unwrap(), b"abc", "{mode}");
                assert_eq!(artifact.size_bytes, 3);
                assert_eq!(
                    artifact.sha256,
                    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                );
                assert_eq!(artifact.original_path.parent(), Some(original.as_path()));
            }
            let output = fixture.0.join(mode);
            fs::create_dir(&output).unwrap();
            child.record(&output, mode).unwrap();
            assert_eq!(
                fs::read_to_string(output.join("input-manifest.json"))
                    .unwrap()
                    .trim(),
                serde_json::to_string_pretty(&inputs).unwrap()
            );
        }
        assert_eq!(inputs.artifacts.len(), 6);
        assert!(!inputs.snapshot_children.join("witnesses").exists());
        assert_ne!(inputs.snapshot_children, original);
    }

    #[test]
    fn pinned_inputs_reject_mutated_and_missing_snapshot_files() {
        let fixture = Fixture::new();
        fixture.children("original", b"abc");
        let inputs =
            snapshot_latest_inputs(&fixture.0.join("a2"), &fixture.0.join("snapshots")).unwrap();
        let expected = serde_json::to_string(&inputs).unwrap();
        for artifact in &inputs.artifacts {
            // Same size: checking length alone cannot pass this regression.
            fs::write(&artifact.snapshot_path, b"abd").unwrap();
            assert!(inputs.verify().is_err());
            assert!(pinned_inputs(&inputs.snapshot_children, &expected).is_err());
            fs::write(&artifact.snapshot_path, b"abc").unwrap();
            inputs.verify().unwrap();
            fs::remove_file(&artifact.snapshot_path).unwrap();
            assert!(inputs.verify().is_err());
            assert!(pinned_inputs(&inputs.snapshot_children, &expected).is_err());
            fs::write(&artifact.snapshot_path, b"abc").unwrap();
        }
        let mut incomplete = inputs.clone();
        incomplete.artifacts.pop();
        assert!(incomplete.verify().is_err());
        assert!(pinned_inputs(&fixture.0, &expected).is_err());
        let independent =
            snapshot_latest_inputs(&fixture.0.join("a2"), &fixture.0.join("snapshots")).unwrap();
        assert_ne!(inputs.snapshot_children, independent.snapshot_children);
    }

    #[test]
    fn attribution_rejects_gpu_error_and_fallback_in_every_mode() {
        let complete = json!({"rayon_threads": 4, "blst_threads": 1, "cuda_msm": true, "cuda_ntt": true, "phases": {"lookup_prepare": {"backends": {
            "msm.gpu": {"calls": 1}, "ntt.gpu": {"calls": 1}
        }}}});
        assert!(check_backend_attribution(&complete, "gpu-before").is_ok());
        let cpu = json!({"rayon_threads": 4, "blst_threads": 1, "cuda_msm": false, "cuda_ntt": false, "phases": {"lookup_prepare": {"backends": {}}}});
        assert!(check_backend_attribution(&cpu, "cpu-before").is_ok());
        for (mode, valid) in [
            ("cpu-before", &cpu),
            ("gpu-before", &complete),
            ("gpu-after", &complete),
        ] {
            for field in ["rayon_threads", "blst_threads", "cuda_msm", "cuda_ntt"] {
                for invalid in [serde_json::Value::Null, json!(99)] {
                    let mut bad = valid.clone();
                    bad[field] = invalid;
                    assert!(
                        check_backend_attribution(&bad, mode).is_err(),
                        "{mode} {field}"
                    );
                }
                let mut missing = valid.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(check_backend_attribution(&missing, mode).is_err());
            }
            let mut wrong_cuda = valid.clone();
            wrong_cuda["cuda_ntt"] = json!(mode == "cpu-before");
            assert!(check_backend_attribution(&wrong_cuda, mode).is_err());
        }
        for mode in ["cpu-before", "gpu-before", "gpu-after"] {
            for backend in ["msm.cpu_fallback", "ntt.gpu_error"] {
                let mut bad = if mode == "cpu-before" {
                    cpu.clone()
                } else {
                    complete.clone()
                };
                bad["phases"]["lookup_prepare"]["backends"][backend] = json!({"calls": 1});
                assert!(check_backend_attribution(&bad, mode).is_err());
            }
        }
    }

    #[test]
    fn after_gate_requires_all_argument_commits_at_the_a3_domain() {
        let complete = json!({"phases": {
            "lookup_prepare": {"backends": {"argument_commit.coeff": {
                "calls": 10, "elements": 10485760, "sizes": {"1048576": 10}}}},
            "permutation_products": {"backends": {"argument_commit.coeff": {
                "calls": 6, "elements": 6291456, "sizes": {"1048576": 6}}}}
        }});
        assert!(check_argument_coefficients(&complete).is_ok());
        let mut missing = complete.clone();
        missing["phases"]
            .as_object_mut()
            .unwrap()
            .remove("permutation_products");
        assert!(check_argument_coefficients(&missing).is_err());
        let mut wrong_size = complete.clone();
        wrong_size["phases"]["lookup_prepare"]["backends"]["argument_commit.coeff"]["sizes"] =
            json!({"524288": 10});
        assert!(check_argument_coefficients(&wrong_size).is_err());
        let mut retained = complete.clone();
        retained["phases"]["lookup_prepare"]["backends"]["argument_commit.lagrange"] =
            json!({"calls": 1, "elements": 1048576, "sizes": {"1048576": 1}});
        assert!(check_argument_coefficients(&retained).is_err());
        let mut extra = complete;
        extra["phases"]["lookup_products"] = json!({"backends": {
            "argument_commit.coeff": {"calls": 5, "elements": 5242880, "sizes": {"1048576": 5}}
        }});
        assert!(check_argument_coefficients(&extra).is_err());
    }
}

fn check_moriarty_pin(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let git = |args: &[&str]| Command::new("git").current_dir(root).args(args).output();
    let head = git(&["rev-parse", "HEAD"])?;
    if !head.status.success() || String::from_utf8(head.stdout)?.trim() != MORIARTY_COMMIT {
        return Err(format!("A3 requires Moriarty {MORIARTY_COMMIT}").into());
    }
    let unchanged = git(&[
        "diff",
        "--quiet",
        MORIARTY_COMMIT,
        "--",
        "Cargo.toml",
        ".cargo",
        ":(glob)crates/**/*.rs",
        ":(glob)crates/**/Cargo.toml",
        ":(exclude)crates/mori-recursion/Cargo.toml",
    ])?;
    if !unchanged.status.success() {
        return Err("pinned Moriarty Rust sources or Cargo configuration changed".into());
    }
    let original = git(&[
        "show",
        &format!("{MORIARTY_COMMIT}:crates/mori-recursion/Cargo.toml"),
    ])?;
    if !original.status.success() {
        return Err("cannot read pinned Moriarty feature manifest".into());
    }
    // The supplied worktree adds this feature to the pinned commit. Permit
    // exactly that addition; Cargo.lock and QA result logs are generated data.
    let actual = fs::read_to_string(root.join("crates/mori-recursion/Cargo.toml"))?;
    let normalized = actual.lines().filter(|line| {
        *line != "# GPU MSM and NTT from the Loki prover (requires the loki-proofs patch and CUDA)."
            && *line != "loki-gpu = [\"sherlock-recursion\", \"midnight-proofs/cuda\", \"midnight-proofs/cuda-ntt\"]"
    }).collect::<Vec<_>>().join("\n");
    if normalized.trim_end() != String::from_utf8(original.stdout)?.trim_end() {
        return Err("Moriarty feature manifest differs from the allowed loki-gpu addition".into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = env::args().nth(1).unwrap_or_default();
    if mode == "status" {
        let output = Command::new("ps")
            .args(["-eo", "pid,ppid,etimes,pcpu,rss,comm"])
            .output()?;
        if !output.status.success() {
            return Err("cannot read host proof process status".into());
        }
        // Command names and resource counters only; never print environments
        // or arguments from unrelated agents on this shared host.
        for line in String::from_utf8(output.stdout)?.lines() {
            let name = line.split_whitespace().last().unwrap_or_default();
            if ["COMMAND", "cargo", "rustc", "loki-a3-profile", "flock"].contains(&name)
                || name.starts_with("sherlock_parent")
                || name.starts_with("midnight_proofs")
                || name.starts_with("proof_roundtrip")
                || name.starts_with("cases-")
                || ["cargo-fuzz", "ntt_diff", "msm_diff"].contains(&name)
            {
                println!("{line}");
                if name.starts_with("proof_roundtrip") || ["ntt_diff", "msm_diff"].contains(&name) {
                    if let Some(pid) = line.split_whitespace().next() {
                        if let Ok(args) = fs::read(format!("/proc/{pid}/cmdline")) {
                            for arg in args.split(|byte| *byte == 0) {
                                if let Ok(arg) = std::str::from_utf8(arg) {
                                    if let Some((flag, value)) = arg.split_once('=') {
                                        if ["-max_total_time", "-runs", "-timeout"].contains(&flag)
                                            && value.bytes().all(|byte| byte.is_ascii_digit())
                                        {
                                            println!("Fuzz budget pid={pid} {flag}={value}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let locks = Command::new("lslocks")
            .args(["-o", "PID,COMMAND,MODE,PATH"])
            .output()?;
        if !locks.status.success() {
            return Err("cannot read host GPU lock status".into());
        }
        for line in String::from_utf8(locks.stdout)?.lines() {
            if line.contains("doctorwatson/gpu.lock") {
                println!("GPU lock: {line}");
                if line.split_whitespace().nth(2) == Some("WRITE") {
                    if let Some(pid) = line.split_whitespace().next() {
                        let tree = Command::new("pstree").args(["-p", pid]).output()?;
                        print!("{}", String::from_utf8(tree.stdout)?);
                    }
                }
            }
        }
        return Ok(());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let head = loki_head(&root)?;
    if mode == "matched-profile" {
        let inputs = snapshot_latest_inputs(
            Path::new("/home/charles/repos/Moriarty-loki/target/sherlock-a2"),
            &root.join("target/a3-profile"),
        )?;
        let expected_manifest = serde_json::to_string(&inputs)?;
        for child_mode in ["gpu-after", "gpu-before", "cpu-before"] {
            if loki_head(&root)? != head {
                return Err("Loki HEAD changed during matched profile".into());
            }
            inputs.verify()?;
            println!("A3 matched boundary=start mode={child_mode} loki_head={head}");
            let result = Command::new(env::current_exe()?)
                .arg(child_mode)
                .env(INPUT_SNAPSHOT_ENV, &inputs.snapshot_children)
                .env(INPUT_MANIFEST_ENV, &expected_manifest)
                .status();
            inputs.verify()?;
            let status = result?;
            if !status.success() {
                return Err(format!("A3 matched mode {child_mode} failed: {status}").into());
            }
            if loki_head(&root)? != head {
                return Err("Loki HEAD changed during matched profile".into());
            }
            println!("A3 matched boundary=complete mode={child_mode} loki_head={head}");
        }
        return Ok(());
    }
    let features = match mode.as_str() {
        "check-pin" => {
            check_moriarty_pin(Path::new("/home/charles/repos/Moriarty-loki"))?;
            println!("A3 pinned Moriarty {MORIARTY_COMMIT}");
            return Ok(());
        }
        "cpu-before" => "sherlock-recursion,midnight-proofs/loki-profile,midnight-proofs/loki-cpu-budget",
        "gpu-before" => "loki-gpu,midnight-proofs/loki-profile,midnight-proofs/loki-cpu-budget",
        "gpu-after" => "loki-gpu,midnight-proofs/loki-profile,midnight-proofs/cuda-argument-commit,midnight-proofs/loki-cpu-budget",
        _ => {
            return Err(
                "usage: loki-a3-profile cpu-before|gpu-before|gpu-after|matched-profile|status|check-pin".into(),
            )
        }
    };
    println!("A3 mode={mode} loki_head={head} features={features}");
    let moriarty = PathBuf::from("/home/charles/repos/Moriarty-loki");
    check_moriarty_pin(&moriarty)?;
    println!("A3 moriarty_commit={MORIARTY_COMMIT}");
    let output = unique_directory(&root.join("target/a3-profile"), &mode)?;
    let inputs = mode_inputs(
        &moriarty.join("target/sherlock-a2"),
        &root.join("target/a3-profile"),
    )?;
    inputs.record(&output, &mode)?;
    fs::write(
        output.join("moriarty-commit.txt"),
        format!("{MORIARTY_COMMIT}\n"),
    )?;
    let config = output.join("patch.toml");
    fs::write(
        &config,
        format!(
            "[patch.crates-io]\nmidnight-proofs = {{ path = {:?} }}\n",
            root
        ),
    )?;
    let profile = output.join("phases.jsonl");
    println!(
        "A3 mode={mode} loki={} output={}",
        root.display(),
        output.display()
    );
    inputs.verify()?;
    let result = Command::new("cargo")
        .current_dir(&moriarty)
        .args(["--config"])
        .arg(&config)
        .args([
            "test",
            "--release",
            "-p",
            "mori-recursion",
            "--features",
            features,
            "--test",
            "sherlock_parent",
            "--",
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CARGO_BUILD_JOBS", "4")
        .env("RAYON_NUM_THREADS", "4")
        .env(
            "MORIARTY_TEST_PARAMS_DIR",
            "/home/charles/.cache/midnight/zk-params",
        )
        .env(
            "MORIARTY_PARENT_SRS",
            moriarty.join("target/sherlock-srs/bls_midnight_2p20"),
        )
        .env("MORIARTY_A2_LINK_DIR", &inputs.snapshot_children)
        .env("MORIARTY_PARENT_OUTPUT_DIR", output.join("artifacts"))
        .env("LOKI_PROFILE", &profile)
        .status();
    inputs.verify()?;
    let status = result?;
    if !status.success() {
        return Err(format!("A3 failed: {status}; output={}", output.display()).into());
    }
    let reports: Vec<serde_json::Value> = fs::read_to_string(&profile)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let a3 = reports
        .iter()
        .find(|r| r["k"] == 20 && r["success"] == true)
        .ok_or("no successful k20 A3 profile")?;
    check_backend_attribution(a3, &mode)?;
    if mode == "gpu-after" {
        check_argument_coefficients(a3)?;
        println!("A3 argument_commit.coeff lookup=10 permutation=6 size=1048576");
    }
    println!("A3 phase report={}", profile.display());
    Ok(())
}
