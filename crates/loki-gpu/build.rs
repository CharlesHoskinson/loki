#[cfg(feature = "cuda")]
fn main() {
    use std::{env, path::PathBuf};

    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    println!("cargo:rerun-if-env-changed=NVCC");

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets this"));
    let cuda_dir = manifest_dir.join("cuda");
    let sources = [
        cuda_dir.join("msm.cu"),
        cuda_dir.join("ntt.cu"),
        cuda_dir.join("quotient.cu"),
    ];
    let header = cuda_dir.join("include/loki_gpu_ffi.h");
    for path in sources.iter().chain(std::iter::once(&header)) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    for source in &sources {
        if !source.is_file() {
            panic!(
                "CUDA feature enabled, but required source is missing: {}",
                source.display()
            );
        }
    }
    if !header.is_file() {
        panic!(
            "CUDA feature enabled, but missing shared ABI header: {}",
            header.display()
        );
    }

    let cuda_home = env::var_os("CUDA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!("CUDA_HOME must name the toolkit root when the cuda feature is enabled")
        });
    let nvcc = env::var_os("NVCC")
        .map(PathBuf::from)
        .unwrap_or_else(|| cuda_home.join("bin/nvcc"));
    let cuda_home = cuda_home
        .canonicalize()
        .unwrap_or_else(|error| panic!("invalid CUDA_HOME {}: {error}", cuda_home.display()));
    let nvcc = nvcc
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot resolve NVCC {}: {error}", nvcc.display()));
    let nvcc_root = nvcc
        .parent()
        .and_then(|bin| bin.parent())
        .unwrap_or_else(|| panic!("NVCC path must be under a toolkit bin directory"));
    if nvcc_root != cuda_home.as_path() {
        panic!(
            "NVCC resolves under {}, which differs from CUDA_HOME {}",
            nvcc_root.display(),
            cuda_home.display()
        );
    }

    // cc-rs selects its CUDA compiler from its inherited environment snapshot;
    // Build::env only configures the child process after that selection. Set
    // the validated executable before constructing Build so NVCC is the exact
    // path checked above, including when supplied through the NVCC override.
    env::set_var("NVCC", &nvcc);
    let mut build = cc::Build::new();
    build
        .cuda(true)
        .cudart("shared")
        .include(cuda_dir.join("include"))
        .flag("-std=c++17")
        .flag("-gencode=arch=compute_120,code=sm_120")
        .flag("-gencode=arch=compute_80,code=compute_80");
    for source in &sources {
        build.file(source);
    }

    // Target-prefixed NVCC variables take precedence over the generic NVCC
    // variable in cc-rs. Verify cc's actual selection rather than assuming the
    // environment binding above won, so cudart headers/libraries cannot come
    // from a different toolkit.
    let selected_nvcc = build
        .get_compiler()
        .path()
        .canonicalize()
        .unwrap_or_else(|error| {
            panic!(
                "cannot resolve cc-rs selected CUDA compiler {}: {error}",
                build.get_compiler().path().display()
            )
        });
    if selected_nvcc != nvcc {
        panic!(
            "cc-rs selected NVCC {}, but validated toolkit compiler is {}",
            selected_nvcc.display(),
            nvcc.display()
        );
    }
    println!(
        "cargo:warning=CUDA compiler selected: {}",
        selected_nvcc.display()
    );
    build.compile("loki_gpu_cuda");

    println!(
        "cargo:rustc-link-search=native={}",
        cuda_home.join("lib64").display()
    );
}

#[cfg(not(feature = "cuda"))]
fn main() {
    // The default build deliberately does not probe for CUDA or compile C++.
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_CUDA");
}
