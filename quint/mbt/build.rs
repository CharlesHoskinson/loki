use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn snapshot(repo: &Path, rev: &str, source: &str, target: &Path) {
    let output = Command::new("git")
        .args(["show", &format!("{rev}:{source}")])
        .current_dir(repo)
        .output()
        .expect("read pinned source commit");
    assert!(
        output.status.success(),
        "missing pinned source {rev}:{source}"
    );
    fs::write(target, output.stdout).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/w4_driver.rs");
    println!("cargo:rerun-if-changed=src/pk_driver.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    if env::var_os("CARGO_FEATURE_W4_SOURCE").is_some() {
        let dir = out.join("w4");
        fs::create_dir_all(&dir).unwrap();
        let repo = root.join("../..");
        let rev = "fdef4c4d70b6a5ea040a8bc94638a57ecc1eb3b2";
        for file in ["convert.rs", "backend.rs"] {
            snapshot(
                &repo,
                rev,
                &format!("src/poly/kzg/msm/cuda/{file}"),
                &dir.join(file),
            );
        }
        // The implementation is unchanged. This test-only hook resets its process
        // lifetime circuit breaker between independent model traces.
        let backend = dir.join("backend.rs");
        let mut code = fs::read_to_string(&backend).unwrap();
        code.push_str("\n#[cfg(test)] pub(super) fn mbt_reset() { let mut c = CACHE.lock().unwrap(); c.failed = false; for e in c.entries.drain(..) { status(unsafe { loki_gpu_release(e.resident) }).unwrap(); } }\n");
        fs::write(backend, code).unwrap();
        snapshot(&repo, rev, "src/poly/kzg/msm/cuda.rs", &dir.join("mod.rs"));
        fs::write(
            dir.join("tests.rs"),
            "// Hardware tests run in the W4 package.\n",
        )
        .unwrap();
        let mut code = fs::read_to_string(dir.join("mod.rs")).unwrap();
        code.push_str(&format!(
            "\nmod mbt {{ use super::*; include!({:?}); }}\n",
            root.join("src/w4_driver.rs")
        ));
        fs::write(dir.join("mod.rs"), code).unwrap();
        fs::write(
            out.join("w4.rs"),
            format!("#[path = {:?}] mod host;\n", dir.join("mod.rs")),
        )
        .unwrap();
    }
    if env::var_os("CARGO_FEATURE_W6_SOURCE").is_some() {
        let repo = env::var_os("LOKI_RECL9_REPO")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/charles/repos/fenrir-loki"));
        println!("cargo:rerun-if-env-changed=LOKI_RECL9_REPO");
        let rev = "abe97019b099026698d10c97f57d490a9554018f";
        let dir = out.join("w6");
        fs::create_dir_all(&dir).unwrap();
        let prefix = "recursion/stagenet-strawman/recl9/src";
        for file in ["key_cache.rs", "key_cache_tests.rs"] {
            snapshot(&repo, rev, &format!("{prefix}/{file}"), &dir.join(file));
        }
        let path = dir.join("key_cache.rs");
        let mut code = fs::read_to_string(&path).unwrap();
        code.push_str(&format!(
            "\ninclude!({:?});\n",
            root.join("src/pk_driver.rs")
        ));
        fs::write(&path, code).unwrap();
        fs::write(
            out.join("w6.rs"),
            format!("#[path = {:?}] mod key_cache;\n", path),
        )
        .unwrap();
        fs::write(
            out.join("circuit-sources.bin"),
            b"W8 pinned public cache test circuits v1",
        )
        .unwrap();
    }
}
