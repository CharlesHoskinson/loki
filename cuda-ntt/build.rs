fn main() {
    println!("cargo:rerun-if-changed=cuda/ntt.cu");
    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    println!("cargo:rerun-if-env-changed=NVCC");
    // cc/sppark find nvcc on PATH but do not add the toolkit's runtime library
    // directory when CUDA is installed outside the system linker search path.
    let toolkit = std::env::var_os("CUDA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let nvcc = std::env::var_os("NVCC").unwrap_or_else(|| "nvcc".into());
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|dir| dir.join(&nvcc))
                .find(|path| path.is_file())
                .and_then(|path| path.canonicalize().ok())
                .and_then(|path| path.parent()?.parent().map(std::path::Path::to_path_buf))
        })
        .expect("set CUDA_HOME or put nvcc on PATH");
    for lib in ["lib64", "lib/x64", "targets/x86_64-linux/lib"] {
        let path = toolkit.join(lib);
        if path.is_dir() {
            println!("cargo:rustc-link-search=native={}", path.display());
        }
    }
    println!("cargo:rerun-if-env-changed=LOKI_NTT_CUDA_ARCH");
    let mut build = cc::Build::new();
    build.cuda(true);
    build.include(std::env::var_os("DEP_SPPARK_ROOT").expect("sppark headers"));
    build.include(std::env::var_os("DEP_BLST_C_SRC").expect("blst headers"));
    build.define("TAKE_RESPONSIBILITY_FOR_ERROR_MESSAGE", None);
    // Include portable PTX by default. Build native Blackwell code when the
    // toolkit supports it; an explicit architecture selects a single target.
    if let Ok(arch) = std::env::var("LOKI_NTT_CUDA_ARCH") {
        build.flag(format!("-arch={arch}"));
    } else {
        build.flag("-gencode=arch=compute_80,code=compute_80");
        let supported = build
            .get_compiler()
            .to_command()
            .arg("--list-gpu-code")
            .output()
            .expect("nvcc GPU target list");
        if String::from_utf8_lossy(&supported.stdout)
            .lines()
            .any(|line| line.trim() == "sm_120")
        {
            build.flag("-gencode=arch=compute_120,code=sm_120");
        }
    }
    build.flag("-t4");
    build.flag("-Xcompiler=-Wno-unused-function");
    build.define("FEATURE_BLS12_381", None);
    build.file("cuda/ntt.cu").compile("loki_ntt");
}
