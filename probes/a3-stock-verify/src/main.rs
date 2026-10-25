use ff::Field;
use group::Group;
use midnight_circuits::hash::poseidon::PoseidonState;
use midnight_curves::{Bls12, Fq, G1Projective};
use midnight_proofs::{
    plonk::prepare,
    poly::{
        commitment::{Guard, Params},
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
    },
    transcript::{CircuitTranscript, Transcript},
    utils::SerdeFormat,
};
use midnight_zk_stdlib::{MidnightVK, ZkStdLibArch};
use sha2::{Digest, Sha256};
use std::{
    env, error::Error, fs,
    io::{Cursor, Read},
    path::PathBuf,
    process::Command,
};

const EXPECTED_LANES: usize = 28;
const EXPECTED_K: u32 = 20;
const EXPECTED_TOTAL_VK_INSTANCE_COLUMNS: usize = 2;
const SRS_BYTES: usize = 201_326_980;
const SRS_SHA256: &str = "1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1";
const W10_MANIFEST_SHA256: &str =
    "f4e2c035e37f996ee0b13563cc2c8dc7e16e607b044f7f95228b12cb6e4531bb";
const W10_VK_SHA256: &str =
    "801467c825c79e063c0603b94c49eef63fd527ff8b3a6d42aa24022b108849a8";
const W10_VK_PATH: &str = "/home/charles/repos/loki-wt/w10-gpu-quotient/target/a3-profile/gpu-after-1792902759671818294-2247472/artifacts/parent/w.vk";
const W10_LOKI_HEAD: &str = "053577f6083b329f2bbef1f3711f92cadc92a24f";
const W10_MORIARTY_HEAD: &str = "6ee918696ab907244c703bbd94bfea9acde7ddd1";
const W10_PROVER_FEATURES: &str = "loki-gpu,midnight-proofs/loki-profile,midnight-proofs/cuda-argument-commit,midnight-proofs/loki-cpu-budget";
const LAB_HOST: &str = "WSL2; Core Ultra 9 285K; RTX 5090 sm_120; CUDA 13.4.3";
const DOCTORWATSON_MODULES: &str =
    "/home/charles/repos/Moriarty-factory/models/doctorwatson";
const X2_REPO: &str = "/home/charles/repos/loki-wt/x2-lab";
const HEAVY_ID: &str = "LAB-A3-STOCK-VERIFY";

type Scheme = KZGCommitmentScheme<Bls12>;

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("a3-stock-verify: {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let artifacts = PathBuf::from(args.next().ok_or(
        "usage: a3-stock-verify <parent-artifact-dir> <pinned-k20-srs> <w10-public-manifest>",
    )?);
    let srs_path = PathBuf::from(args.next().ok_or("missing pinned k20 SRS path")?);
    let manifest_path = PathBuf::from(args.next().ok_or("missing W10 public manifest path")?);
    let inside_heavy = match args.next() {
        None => false,
        Some(arg) if arg == "--inside-heavy" => true,
        Some(_) => return Err("unexpected extra arguments".into()),
    };
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }

    if inside_heavy {
        verify_artifacts(&artifacts, &srs_path, &manifest_path)?;
        Ok(0)
    } else {
        run_under_doctorwatson(&artifacts, &srs_path, &manifest_path)
    }
}

fn verify_artifacts(
    artifacts: &std::path::Path,
    srs_path: &std::path::Path,
    manifest_path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    let rayon_threads = rayon::current_num_threads();
    if rayon_threads != 4 {
        return Err(format!("expected a 4-thread Rayon pool, found {rayon_threads}").into());
    }
    let artifacts = artifacts.canonicalize()?;
    let srs_path = srs_path.canonicalize()?;
    let manifest_path = manifest_path.canonicalize()?;
    let vk_path = artifacts.join("w.vk");
    let proof_path = artifacts.join("w.proof");
    let instance_path = artifacts.join("w.instance.json");
    let vk_bytes = fs::read(&vk_path)?;
    let proof = fs::read(&proof_path)?;
    let instance_json = fs::read(&instance_path)?;
    let instance = a3_stock_verify::parse_instance_json(&instance_json, EXPECTED_LANES)?;

    let mut vk_cursor = Cursor::new(vk_bytes.as_slice());
    let mut shape_cursor = Cursor::new(vk_bytes.as_slice());
    let _architecture = ZkStdLibArch::read(&mut shape_cursor)?;
    let mut shape_k = [0u8; 1];
    shape_cursor.read_exact(&mut shape_k)?;
    let mut public_count_bytes = [0u8; 4];
    shape_cursor.read_exact(&mut public_count_bytes)?;
    let public_input_count = u32::from_le_bytes(public_count_bytes);
    if public_input_count as usize != EXPECTED_LANES {
        return Err(format!(
            "VK declares {public_input_count} public inputs; expected {EXPECTED_LANES}"
        )
        .into());
    }
    let vk = MidnightVK::read(&mut vk_cursor, SerdeFormat::Processed)?;
    if vk_cursor.position() != vk_bytes.len() as u64 {
        return Err("trailing bytes after processed MidnightVK".into());
    }
    if u32::from(vk.k()) != EXPECTED_K {
        return Err(format!("VK k={} differs from expected k20", vk.k()).into());
    }
    if shape_k[0] != vk.k()
        || vk.vk().cs().num_instance_columns() != EXPECTED_TOTAL_VK_INSTANCE_COLUMNS
    {
        return Err(
            "A3 VK must have one committed identity column and one public instance column".into(),
        );
    }

    let srs_bytes = fs::read(&srs_path)?;
    let srs_sha256 = sha256_hex(&srs_bytes);
    if srs_bytes.len() != SRS_BYTES || srs_sha256 != SRS_SHA256 {
        return Err(format!(
            "SRS does not match pinned k20 ceremony: bytes={} sha256={srs_sha256}",
            srs_bytes.len()
        )
        .into());
    }
    let mut srs_cursor = Cursor::new(srs_bytes.as_slice());
    let params = ParamsKZG::<Bls12>::read_custom(
        &mut srs_cursor,
        SerdeFormat::RawBytesUnchecked,
    )?;
    if srs_cursor.position() != srs_bytes.len() as u64 || params.max_k() != EXPECTED_K {
        return Err("pinned SRS has wrong degree or trailing bytes".into());
    }
    let manifest_sha256 = verify_w10_manifest(
        &manifest_path,
        &artifacts,
        &srs_path,
        &vk_bytes,
        &proof,
        &instance_json,
        &srs_bytes,
    )?;

    verify_once(vk.vk(), &params, &proof, &instance)
        .map_err(|error| format!("stock A3 proof rejected: {error}"))?;

    for lane in 0..instance.len() {
        let mut changed = instance.clone();
        changed[lane] += Fq::ONE;
        require_rejected(vk.vk(), &params, &proof, &changed, &format!("lane {lane}"))?;
    }
    if proof.is_empty() {
        return Err("A3 proof is empty".into());
    }
    let mut corrupt = proof.clone();
    let corrupt_at = corrupt.len() / 2;
    corrupt[corrupt_at] ^= 1;
    require_rejected(vk.vk(), &params, &corrupt, &instance, "corrupted proof byte")?;
    require_rejected(
        vk.vk(),
        &params,
        &proof[..proof.len() - 1],
        &instance,
        "truncated proof",
    )?;
    let mut appended = proof.clone();
    appended.push(0);
    require_rejected(vk.vk(), &params, &appended, &instance, "appended proof byte")?;

    println!(
        "stock_a3_verify=accepted circuit=mori-recursion::SherlockLinkRelation verifier=midnight-proofs-0.8.3 source=registry verifier_head={} verifier_source_sha256={} cargo_lock_sha256={} verifier_features=committed-instances,blst-no-threads k={} vk_total_instance_columns=2 committed_identity_columns=1 public_instance_columns=1 lanes={} proof_bytes={} transcript_fully_consumed=true negative_controls=28_lane_mutations+proof_byte+truncate+append all_rejected=true w10_loki_head={} w10_moriarty_head={} w10_mode=gpu-after prover_features={} prover_rayon=4 prover_blst=1 verifier_rayon={} verifier_blst=no-threads host=\"{}\" argv={:?} w10_manifest_sha256={} srs_sha256={} vk_sha256={} proof_sha256={} instance_sha256={}",
        git_head(),
        package_source_sha256()?,
        file_sha256(&PathBuf::from(X2_REPO).join("probes/a3-stock-verify/Cargo.lock"))?,
        vk.k(),
        instance.len(),
        proof.len(),
        W10_LOKI_HEAD,
        W10_MORIARTY_HEAD,
        W10_PROVER_FEATURES,
        rayon_threads,
        LAB_HOST,
        env::args_os().collect::<Vec<_>>(),
        manifest_sha256,
        srs_sha256,
        sha256_hex(&vk_bytes),
        sha256_hex(&proof),
        sha256_hex(&instance_json),
    );
    Ok(())
}

fn git_head() -> String {
    Command::new("git")
        .args(["-C", X2_REPO, "rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|head| !head.is_empty())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn package_source_sha256() -> Result<String, Box<dyn Error>> {
    let package = PathBuf::from(X2_REPO).join("probes/a3-stock-verify");
    let mut hasher = Sha256::new();
    for relative in ["Cargo.toml", "src/lib.rs", "src/main.rs"] {
        let bytes = fs::read(package.join(relative))?;
        hasher.update(relative.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn file_sha256(path: &std::path::Path) -> Result<String, Box<dyn Error>> {
    Ok(sha256_hex(&fs::read(path)?))
}

fn run_under_doctorwatson(
    artifacts: &std::path::Path,
    srs_path: &std::path::Path,
    manifest_path: &std::path::Path,
) -> Result<i32, Box<dyn Error>> {
    let executable = env::current_exe()?;
    let hypothesis = "Run 32 stock midnight-proofs 0.8.3 verifier attempts on saved public Sherlock A3 k20 material: accept the exact proof and reject 28 in-memory lane mutations plus corrupt, truncated, and appended proof encodings. Load the hash-pinned public SRS and VK once. No cost_model, keygen, proving, or GPU.";
    let status = Command::new("python3")
        .env("PYTHONPATH", DOCTORWATSON_MODULES)
        .env("DW_REPO", X2_REPO)
        .env("RAYON_NUM_THREADS", "4")
        .args([
            "-m",
            "factory",
            "heavy",
            "--id",
            HEAVY_ID,
            "--hypothesis",
            hypothesis,
            "--",
        ])
        .arg(executable)
        .arg(artifacts)
        .arg(srs_path)
        .arg(manifest_path)
        .arg("--inside-heavy")
        .status()?;
    Ok(status.code().unwrap_or(1).clamp(0, 255))
}

fn verify_once(
    vk: &midnight_proofs::plonk::VerifyingKey<Fq, Scheme>,
    params: &ParamsKZG<Bls12>,
    proof: &[u8],
    instance: &[Fq],
) -> Result<(), String> {
    let mut transcript = CircuitTranscript::<PoseidonState<Fq>>::init_from_bytes(proof);
    let committed_identity = [a3_committed_identity()];
    let committed_instances = [&committed_identity[..]];
    let guard = prepare::<Fq, Scheme, _>(vk, &committed_instances, &[&[instance]], &mut transcript)
        .map_err(|error| format!("prepare failed: {error:?}"))?;
    transcript
        .assert_empty()
        .map_err(|error| format!("proof transcript has unconsumed bytes: {error:?}"))?;
    guard
        .verify(&params.verifier_params())
        .map_err(|error| format!("pairing verification failed: {error:?}"))
}

fn a3_committed_identity() -> G1Projective {
    G1Projective::identity()
}

fn require_rejected(
    vk: &midnight_proofs::plonk::VerifyingKey<Fq, Scheme>,
    params: &ParamsKZG<Bls12>,
    proof: &[u8],
    instance: &[Fq],
    label: &str,
) -> Result<(), Box<dyn Error>> {
    if verify_once(vk, params, proof, instance).is_ok() {
        return Err(format!("stock verifier unexpectedly accepted {label}").into());
    }
    Ok(())
}

fn verify_w10_manifest(
    manifest_path: &std::path::Path,
    artifacts: &std::path::Path,
    srs_path: &std::path::Path,
    vk: &[u8],
    proof: &[u8],
    instance: &[u8],
    srs: &[u8],
) -> Result<String, Box<dyn Error>> {
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest_sha256 = sha256_hex(&manifest_bytes);
    if manifest_sha256 != W10_MANIFEST_SHA256 {
        return Err(format!("W10 A3 manifest digest mismatch: {manifest_sha256}").into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    validate_w10_manifest_identity(&manifest).map_err(std::io::Error::other)?;
    let entries = manifest
        .get("artifacts")
        .and_then(serde_json::Value::as_array)
        .ok_or("W10 A3 manifest has no artifact list")?;
    let expected_artifacts = [
        (artifacts.join("w.vk"), vk),
        (artifacts.join("w.proof"), proof),
        (artifacts.join("w.instance.json"), instance),
        (srs_path.to_path_buf(), srs),
    ];
    for (path, bytes) in expected_artifacts {
        let path_text = path.to_string_lossy();
        let record = entries
            .iter()
            .find(|entry| {
                entry.get("path").and_then(serde_json::Value::as_str) == Some(path_text.as_ref())
            })
            .ok_or_else(|| format!("W10 manifest does not pin {}", path.display()))?;
        let actual_hash = sha256_hex(bytes);
        if record
            .get("size_bytes")
            .and_then(serde_json::Value::as_u64)
            != Some(bytes.len() as u64)
            || record.get("sha256").and_then(serde_json::Value::as_str)
                != Some(actual_hash.as_str())
        {
            return Err(format!("artifact differs from W10 manifest: {}", path.display()).into());
        }
    }
    Ok(manifest_sha256)
}

fn validate_w10_manifest_identity(manifest: &serde_json::Value) -> Result<(), String> {
    for (field, expected) in [
        ("mode", "gpu-after"),
        ("loki_head", W10_LOKI_HEAD),
        ("moriarty_head", W10_MORIARTY_HEAD),
    ] {
        if manifest.get(field).and_then(serde_json::Value::as_str) != Some(expected) {
            return Err(format!("W10 A3 manifest {field} differs from the accepted run"));
        }
    }
    if manifest
        .get("instance_lanes")
        .and_then(serde_json::Value::as_u64)
        != Some(EXPECTED_LANES as u64)
        || manifest
            .get("prove_verify_seconds")
            .and_then(serde_json::Value::as_f64)
            != Some(237.808)
    {
        return Err("W10 A3 manifest shape or timing differs from the accepted run".into());
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod manifest_tests {
    use serde_json::{Value, json};

    use super::{W10_LOKI_HEAD, W10_MORIARTY_HEAD, validate_w10_manifest_identity};

    fn valid_manifest() -> Value {
        json!({
            "mode": "gpu-after",
            "loki_head": W10_LOKI_HEAD,
            "moriarty_head": W10_MORIARTY_HEAD,
            "instance_lanes": 28,
            "prove_verify_seconds": 237.808
        })
    }

    #[test]
    fn accepts_the_pinned_w10_run_identity() {
        assert!(validate_w10_manifest_identity(&valid_manifest()).is_ok());
    }

    #[test]
    fn rejects_a_different_run_or_public_shape() {
        for (field, value) in [
            ("mode", json!("gpu-before")),
            ("loki_head", json!("other")),
            ("moriarty_head", json!("other")),
            ("instance_lanes", json!(27)),
            ("prove_verify_seconds", json!(1.0)),
        ] {
            let mut manifest = valid_manifest();
            manifest[field] = value;
            assert!(validate_w10_manifest_identity(&manifest).is_err(), "{field}");
        }
    }
}

#[cfg(test)]
mod instance_shape_tests {
    use std::{fs, io::Cursor};

    use group::Group;
    use super::{
        EXPECTED_K, EXPECTED_TOTAL_VK_INSTANCE_COLUMNS, G1Projective, SerdeFormat, MidnightVK,
        W10_VK_PATH, W10_VK_SHA256, a3_committed_identity, sha256_hex,
    };

    #[test]
    fn a3_vk_includes_the_committed_identity_and_public_instance_columns() {
        assert_eq!(EXPECTED_TOTAL_VK_INSTANCE_COLUMNS, 2);
        assert_eq!(a3_committed_identity(), G1Projective::identity());
    }

    #[test]
    fn pinned_public_a3_vk_has_two_total_instance_columns() {
        let bytes = fs::read(W10_VK_PATH).expect("read pinned public W10 A3 VK");
        assert_eq!(sha256_hex(&bytes), W10_VK_SHA256);
        let mut cursor = Cursor::new(bytes.as_slice());
        let vk = MidnightVK::read(&mut cursor, SerdeFormat::Processed)
            .expect("parse processed W10 A3 VK");
        assert_eq!(cursor.position(), bytes.len() as u64);
        assert_eq!(u32::from(vk.k()), EXPECTED_K);
        assert_eq!(
            vk.vk().cs().num_instance_columns(),
            EXPECTED_TOTAL_VK_INSTANCE_COLUMNS
        );
    }
}
