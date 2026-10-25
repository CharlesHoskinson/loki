//! The seven-stage suffix of verify-w9.sh, under one host GPU reservation.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let stages: &[(&str, &[&str])] = &[
        (
            "gpu msm boundaries",
            &[
                "test",
                "--release",
                "--offline",
                "--features",
                "cuda,cuda-ntt",
                "--lib",
                "gpu_large_boundaries",
                "--",
                "--ignored",
                "--test-threads=1",
            ],
        ),
        (
            "gpu proof",
            &[
                "test",
                "--release",
                "--offline",
                "--features",
                "cuda,cuda-ntt",
                "--lib",
                "gpu_proof",
                "--",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
        ),
        (
            "gpu proof stock-verified",
            &[
                "run",
                "--release",
                "--offline",
                "--manifest-path",
                "tests/gpu-compat/Cargo.toml",
                "--",
                "target/gpu-msm-proof",
            ],
        ),
        (
            "ntt k17 stock-verified",
            &[
                "run",
                "--release",
                "--offline",
                "--manifest-path",
                "tests/ntt-stock/Cargo.toml",
                "--features",
                "gpu",
                "--",
                "17",
                "3",
            ],
        ),
        (
            "gpu concurrent proofs",
            &[
                "test",
                "--release",
                "--offline",
                "--features",
                "cuda,cuda-ntt,loki-harden",
                "--lib",
                "gpu_concurrent_proofs",
                "--",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
        ),
        (
            "gpu budget harden lib",
            &[
                "test",
                "--release",
                "--offline",
                "--features",
                "cuda,cuda-ntt,loki-harden,loki-cpu-budget",
                "--lib",
                "--",
                "--test-threads=1",
            ],
        ),
        (
            "gpu budget concurrent proofs",
            &[
                "test",
                "--release",
                "--offline",
                "--features",
                "cuda,cuda-ntt,loki-harden,loki-cpu-budget",
                "--lib",
                "gpu_concurrent_proofs",
                "--",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
        ),
    ];
    let logs = root.join("target/w9-validation/gate");
    fs::create_dir_all(&logs)?;
    let mut results = String::new();
    let mut failed = false;
    for (label, args) in stages {
        let log_path = logs.join(format!("{}.log", label.replace(' ', "-")));
        let log = fs::File::create(&log_path)?;
        let status = Command::new("cargo")
            .args(*args)
            .current_dir(&root)
            .env("CARGO_BUILD_JOBS", "4")
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .status()?;
        let rc = status.code().unwrap_or(128);
        failed |= !status.success();
        results.push_str(&format!("{label}\t{rc}\t{}\n", log_path.display()));
        fs::write(logs.join("batch-results.tsv"), &results)?;
        println!("W9_STAGE {label}: rc={rc} log={}", log_path.display());
    }
    std::process::exit(i32::from(failed));
}
