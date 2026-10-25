fn main() {
    #[cfg(feature = "cuda")]
    cuda();
}

#[cfg(feature = "cuda")]
fn cuda() {
    use std::{env, fs, path::PathBuf};
    println!("cargo:rerun-if-changed=cuda/msm.cu");
    println!("cargo:rerun-if-env-changed=LOKI_CUDA_ARCH");
    // Explicit SASS + PTX for Blackwell by default. Set LOKI_CUDA_ARCH to
    // another compute capability (e.g. 80) when building for another GPU.
    let arch = env::var("LOKI_CUDA_ARCH").unwrap_or_else(|_| "120".into());
    assert!(arch.bytes().all(|c| c.is_ascii_digit()) && !arch.is_empty());
    for key in [
        "CUDA_HOME",
        "CUDA_PATH",
        "CUDA_TOOLKIT_PATH",
        "NVCC",
        "PATH",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let toolkit = ["CUDA_HOME", "CUDA_PATH", "CUDA_TOOLKIT_PATH"]
        .iter()
        .find_map(env::var_os)
        .map(PathBuf::from)
        .or_else(|| {
            let nvcc = env::var_os("NVCC").unwrap_or_else(|| "nvcc".into());
            let compiler = PathBuf::from(&nvcc);
            let compiler = if compiler.is_absolute() {
                Some(compiler)
            } else {
                env::var_os("PATH").and_then(|path| {
                    env::split_paths(&path)
                        .map(|dir| dir.join(&nvcc))
                        .find(|path| path.is_file())
                })
            }?;
            compiler
                .canonicalize()
                .ok()?
                .parent()?
                .parent()
                .map(PathBuf::from)
        })
        .expect("set CUDA_HOME to the CUDA toolkit directory");
    println!(
        "cargo:rustc-link-search=native={}",
        toolkit.join("lib64").display()
    );
    let sppark = PathBuf::from(env::var_os("DEP_SPPARK_ROOT").expect("sppark headers"));
    let blst = PathBuf::from(env::var_os("DEP_BLST_C_SRC").expect("BLST headers"));
    // Upstream's implicitly noexcept destructor calls throwing CUDA helpers.
    // Permit release-time errors to reach our catch at the C ABI boundary.
    // Keep this narrowly scoped patch checked against the pinned source.
    let header = fs::read_to_string(sppark.join("msm/pippenger.cuh")).unwrap();
    assert_eq!(
        header.matches("~msm_t()").count(),
        1,
        "sppark destructor changed"
    );
    let patched = header.replace("~msm_t()", "~msm_t() noexcept(false)");
    let generated = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::create_dir_all(generated.join("msm")).unwrap();
    fs::write(generated.join("msm/pippenger.cuh"), patched).unwrap();
    // Match blst's default ADX selection. Its internal assembly symbols have
    // distinct names (mulx_mont_384, etc.) when ADX is selected.
    let mut nvcc = cc::Build::new();
    #[cfg(target_arch = "x86_64")]
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64") {
        let flags = env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
        let adx = if flags.contains("target-cpu=") {
            env::var("CARGO_CFG_TARGET_FEATURE")
                .unwrap_or_default()
                .split(',')
                .any(|f| f == "adx")
        } else {
            std::is_x86_feature_detected!("adx")
        };
        if adx {
            nvcc.define("__ADX__", None);
        }
    }
    nvcc.cuda(true)
        .cudart("static")
        .opt_level(3)
        .flag("-std=c++17")
        .flag(format!(
            "-gencode=arch=compute_{arch},code=[sm_{arch},compute_{arch}]"
        ))
        .define("FEATURE_BLS12_381", None)
        .define("TAKE_RESPONSIBILITY_FOR_ERROR_MESSAGE", None)
        .include(&generated)
        .include(sppark.join("msm"))
        .include(sppark)
        .include(blst)
        .file("cuda/msm.cu")
        .compile("loki_gpu_msm");
}
