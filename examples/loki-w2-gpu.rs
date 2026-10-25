//! Run the independent W2 compatibility gate, then verify GPU dispatch profiles.

use std::{collections::BTreeSet, env, fs, path::Path, process::Command, time::SystemTime};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const FEATURES: &str = "cuda,cuda-ntt,cuda-argument-commit,loki-profile,loki-cpu-budget";

fn validate_profiles(jsonl: &str, requested: u32) -> Result<usize> {
    let mut seen = BTreeSet::new();
    let mut records = 0;
    for line in jsonl.lines() {
        let report: serde_json::Value = serde_json::from_str(line)?;
        let k = report["k"].as_u64().ok_or("missing profile k")?;
        if ![10, u64::from(requested)].contains(&k)
            || report["success"] != true
            || report["cuda_msm"] != true
            || report["cuda_ntt"] != true
            || report["rayon_threads"] != 1
            || report["blst_threads"] != 1
            || !report["jsonl_error"].is_null()
        {
            return Err(
                "W2 profile has invalid size, status, CUDA features, or worker settings".into(),
            );
        }
        let mut gpu_msm = 0;
        let mut gpu_ntt = 0;
        let mut coefficients = 0;
        for (phase, timing) in report["phases"].as_object().ok_or("missing phases")? {
            for (backend, counts) in timing["backends"]
                .as_object()
                .ok_or("missing backend counters")?
            {
                let calls = counts["calls"].as_u64().ok_or("missing call count")?;
                let sizes = counts["sizes"]
                    .as_object()
                    .ok_or("missing size histogram")?;
                let mut histogram_calls = 0;
                let mut elements = 0;
                for (size, count) in sizes {
                    let size: u64 = size.parse()?;
                    let count = count.as_u64().ok_or("invalid histogram count")?;
                    histogram_calls += count;
                    elements += size * count;
                }
                if histogram_calls != calls || counts["elements"].as_u64() != Some(elements) {
                    return Err("W2 inconsistent backend counts/size histogram".into());
                }
                match backend.as_str() {
                    "msm.gpu" => gpu_msm += calls,
                    "ntt.gpu" => gpu_ntt += calls,
                    "msm.cpu_fallback"
                    | "ntt.gpu_error"
                    | "ntt.cpu_forced"
                    | "argument_commit.lagrange"
                        if calls != 0 =>
                    {
                        return Err(
                            "W2 error, fallback, forced CPU NTT, or Lagrange argument strategy"
                                .into(),
                        );
                    }
                    "argument_commit.coeff" => {
                        if !["lookup_prepare", "permutation_products"].contains(&phase.as_str())
                            || sizes.len() != 1
                            || sizes
                                .get(&(1u64 << k).to_string())
                                .and_then(|count| count.as_u64())
                                != Some(calls)
                        {
                            return Err("W2 coefficient strategy phase/domain mismatch".into());
                        }
                        coefficients += calls;
                    }
                    _ => {}
                }
                println!(
                    "W2 k={k} phase={phase} backend={backend} calls={calls} sizes={}",
                    counts["sizes"]
                );
            }
        }
        if gpu_msm == 0 || gpu_ntt == 0 || coefficients == 0 {
            return Err("W2 requires actual GPU MSM, GPU NTT, and coefficient argument commits in every proof".into());
        }
        println!("W2 k={k} gpu_msm={gpu_msm} gpu_ntt={gpu_ntt} argument_coeff={coefficients} status=passed");
        seen.insert(k);
        records += 1;
    }
    if !seen.contains(&u64::from(requested)) {
        return Err("W2 requested size missing from profiles".into());
    }
    Ok(records)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    let sizes: Vec<u32> = if args.is_empty() {
        vec![10, 14, 17]
    } else {
        args.iter()
            .map(|arg| arg.parse())
            .collect::<std::result::Result<_, _>>()?
    };
    if sizes.iter().any(|k| ![10, 14, 17].contains(k))
        || sizes.iter().copied().collect::<BTreeSet<_>>().len() != sizes.len()
    {
        return Err("usage: loki-w2-gpu [10] [14] [17] (unique subset)".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let harness = root.join("loki-harness");
    let sequence = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let output = root.join(format!("target/w2-gpu/{sequence}-{}", std::process::id()));
    fs::create_dir_all(&output)?;
    // The outer GPU runner owns the lock. Inherit its environment/markers and
    // keep Cargo's child build separate from the outer package's build lock.
    for k in sizes {
        let profile = output.join(format!("k{k}.jsonl"));
        println!("W2 k={k} status=running profile={}", profile.display());
        let child = Command::new("cargo")
            .current_dir(&harness)
            .args([
                "run",
                "--release",
                "--locked",
                "--features",
                FEATURES,
                "--bin",
                "gate",
                "--",
            ])
            .arg(k.to_string())
            .env("CARGO_TARGET_DIR", harness.join("target"))
            .env("CARGO_BUILD_JOBS", "4")
            .env("RAYON_NUM_THREADS", "1")
            .env("LOKI_GPU_MSM_MIN", "1")
            .env_remove("LOKI_NTT_CPU")
            .env("LOKI_PROFILE", &profile)
            // The gate's raw determinism diagnostics include byte differences.
            // Capture them; this launcher emits only public counters/statuses.
            .output()?;
        if !child.status.success() {
            return Err(format!("W2 gate k={k} failed: {}", child.status).into());
        }
        let records = validate_profiles(&fs::read_to_string(&profile)?, k)?;
        println!("W2 k={k} records={records} status=passed");
    }
    println!("W2 status=passed output={}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn record(k: u32) -> Value {
        json!({"k": k, "circuits": 1, "rayon_threads": 1, "blst_threads": 1,
        "cuda_msm": true, "cuda_ntt": true, "success": true,
        "jsonl_error": null, "total_wall_ns": 1,
        "msm": {"calls": 1, "elements": 1024, "sizes": {"1024": 1}},
        "fft": {"calls": 1, "elements": 1024, "sizes": {"1024": 1}},
        "phases": {"permutation_products": {"calls": 1, "wall_ns": 1, "backends": {
            "msm.gpu": {"calls": 1, "elements": 1024, "sizes": {"1024": 1}},
            "ntt.gpu": {"calls": 1, "elements": 1024, "sizes": {"1024": 1}},
            "argument_commit.coeff": {"calls": 1, "elements": 1024, "sizes": {"1024": 1}}
        }}}})
    }

    #[test]
    fn profiles_require_success_dispatch_strategy_and_requested_size() {
        let complete = record(10);
        assert_eq!(validate_profiles(&complete.to_string(), 10).unwrap(), 1);
        let mut k14 = complete.clone();
        k14["k"] = json!(14);
        k14["phases"]["permutation_products"]["backends"]["argument_commit.coeff"] =
            json!({"calls": 1, "elements": 16384, "sizes": {"16384": 1}});
        assert_eq!(
            validate_profiles(&format!("{complete}\n{k14}"), 14).unwrap(),
            2
        );
        assert!(validate_profiles("", 10).is_err());
        assert!(validate_profiles(&complete.to_string(), 14).is_err());
        for field in ["success", "cuda_msm", "cuda_ntt"] {
            let mut invalid = complete.clone();
            invalid[field] = json!(false);
            assert!(
                validate_profiles(&invalid.to_string(), 10).is_err(),
                "{field}"
            );
        }
        let mut parallel = complete.clone();
        parallel["rayon_threads"] = json!(4);
        assert!(validate_profiles(&parallel.to_string(), 10).is_err());
        for backend in ["msm.gpu", "ntt.gpu", "argument_commit.coeff"] {
            let mut missing = complete.clone();
            missing["phases"]["permutation_products"]["backends"]
                .as_object_mut()
                .unwrap()
                .remove(backend);
            assert!(
                validate_profiles(&missing.to_string(), 10).is_err(),
                "{backend}"
            );
        }
        for backend in [
            "msm.cpu_fallback",
            "ntt.gpu_error",
            "ntt.cpu_forced",
            "argument_commit.lagrange",
        ] {
            let mut fallback = complete.clone();
            fallback["phases"]["permutation_products"]["backends"][backend] =
                json!({"calls": 1, "elements": 1024, "sizes": {"1024": 1}});
            assert!(
                validate_profiles(&fallback.to_string(), 10).is_err(),
                "{backend}"
            );
        }
        let mut failed = complete.clone();
        failed["success"] = json!(false);
        assert!(validate_profiles(&format!("{complete}\n{failed}"), 10).is_err());
        let mut wrong_size = complete;
        wrong_size["phases"]["permutation_products"]["backends"]["argument_commit.coeff"]
            ["sizes"] = json!({"512": 1});
        wrong_size["phases"]["permutation_products"]["backends"]["argument_commit.coeff"]
            ["elements"] = json!(512);
        assert!(validate_profiles(&wrong_size.to_string(), 10).is_err());
    }
}
