//! Snapshot the W13 cache source from an immutable commit into OUT_DIR.
//! Never edits the source repository. With `gpu`, also compiles the pinned
//! native adapter exactly as the root build.rs does (flags copied from it).
use std::{env, fs, path::PathBuf, process::Command};

const REV: &str = "a256a9a15ff18ae12a8131b86cff576787fa5e3c";

fn show(repo: &PathBuf, rev: &str, path: &str) -> String {
    let out = Command::new("git")
        .args(["show", &format!("{rev}:{path}")])
        .current_dir(repo)
        .output()
        .expect("git show");
    assert!(out.status.success(), "missing pinned source {rev}:{path}");
    String::from_utf8(out.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/driver.rs");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let repo = root.join("../../..");
    let rev = env::var("Q4_SRS_REV").unwrap_or_else(|_| REV.into());
    println!("cargo:rerun-if-env-changed=Q4_SRS_REV");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let dir = out.join("w13");
    fs::create_dir_all(&dir).unwrap();
    for file in ["convert.rs", "backend.rs"] {
        fs::write(dir.join(file), show(&repo, &rev, &format!("src/poly/kzg/msm/cuda/{file}"))).unwrap();
    }
    // The implementation is unchanged; the replay driver is appended as a child
    // module so it can observe private cache state.
    let mut backend = fs::read_to_string(dir.join("backend.rs")).unwrap();
    backend.push_str(&format!(
        "\n#[cfg(test)]\nmod q4_mbt {{ use super::*; include!({:?}); }}\n",
        root.join("src/driver.rs")
    ));
    fs::write(dir.join("backend.rs"), backend).unwrap();
    fs::write(dir.join("mod.rs"), show(&repo, &rev, "src/poly/kzg/msm/cuda.rs")).unwrap();
    fs::write(dir.join("tests.rs"), "// Hardware tests run in the Loki package.\n").unwrap();
    fs::write(out.join("w13.rs"), format!("#[path = {:?}] pub(crate) mod cuda;\n", dir.join("mod.rs"))).unwrap();
    #[cfg(feature = "gpu")]
    native(&repo, &rev, &out);
}

#[cfg(feature = "gpu")]
fn native(repo: &PathBuf, rev: &str, out: &PathBuf) {
    // Copied from root build.rs at REV (cuda() body), with sources read from REV.
    let arch = env::var("LOKI_CUDA_ARCH").unwrap_or_else(|_| "120".into());
    let toolkit = PathBuf::from(env::var_os("CUDA_HOME").expect("set CUDA_HOME"));
    println!("cargo:rustc-link-search=native={}", toolkit.join("lib64").display());
    let sppark = PathBuf::from(env::var_os("DEP_SPPARK_ROOT").expect("sppark headers"));
    let blst = PathBuf::from(env::var_os("DEP_BLST_C_SRC").expect("BLST headers"));
    let header = fs::read_to_string(sppark.join("msm/pippenger.cuh")).unwrap();
    assert_eq!(header.matches("~msm_t()").count(), 1, "sppark destructor changed");
    let generated = out.join("native");
    fs::create_dir_all(generated.join("msm")).unwrap();
    fs::write(generated.join("msm/pippenger.cuh"), header.replace("~msm_t()", "~msm_t() noexcept(false)")).unwrap();
    // Rename the three C entry points to *_native so src/driver.rs can wrap
    // them with counting shims; the C++ bodies are unchanged.
    let cu = show(repo, rev, "cuda/msm.cu");
    let mut renamed = cu.clone();
    for name in ["loki_gpu_prepare", "loki_gpu_msm", "loki_gpu_release"] {
        assert_eq!(renamed.matches(&format!("{name}(")).count(), 1, "{name} definition");
        renamed = renamed.replace(&format!("{name}("), &format!("{name}_native("));
    }
    fs::write(generated.join("msm.cu"), renamed).unwrap();
    let mut nvcc = cc::Build::new();
    if std::is_x86_feature_detected!("adx") {
        nvcc.define("__ADX__", None);
    }
    nvcc.cuda(true)
        .cudart("static")
        .opt_level(3)
        .flag("-std=c++17")
        .flag(format!("-gencode=arch=compute_{arch},code=[sm_{arch},compute_{arch}]"))
        .define("FEATURE_BLS12_381", None)
        .define("TAKE_RESPONSIBILITY_FOR_ERROR_MESSAGE", None)
        .include(&generated)
        .include(sppark.join("msm"))
        .include(sppark)
        .include(blst)
        .file(generated.join("msm.cu"))
        .compile("loki_gpu_msm");
}
