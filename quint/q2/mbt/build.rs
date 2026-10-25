//! Snapshot the legacy sppark host adapters at the pinned Loki commit into
//! OUT_DIR, unchanged except for (a) a test-only reset hook and (b) the
//! `gpu_fft` glue cut out of `EvaluationDomain` into a generic wrapper. The
//! native FFI symbols are provided by CPU surrogates in legacy_ffi.rs; no CUDA
//! is linked and no GPU arithmetic is claimed.
use std::{env, fs, path::PathBuf, process::Command};

const REV: &str = "a256a9a";

fn show(repo: &PathBuf, path: &str) -> String {
    let out = Command::new("git")
        .args(["show", &format!("{REV}:{path}")])
        .current_dir(repo)
        .output()
        .expect("git show");
    assert!(out.status.success(), "missing pinned source {REV}:{path}");
    String::from_utf8(out.stdout).unwrap()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/legacy_driver.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let repo = root.join("../../..");
    let dir = out.join("legacy");
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("convert.rs"), show(&repo, "src/poly/kzg/msm/cuda/convert.rs")).unwrap();
    let mut backend = show(&repo, "src/poly/kzg/msm/cuda/backend.rs");
    backend.push_str(
        "\n#[cfg(test)] pub(super) fn mbt_reset() { let _d = loki_cuda_runtime::lock().unwrap(); \
         let mut c = CACHE.lock().unwrap(); c.failed = false; \
         for e in c.entries.drain(..) { let _ = status(unsafe { loki_gpu_release(e.resident) }); } }\n",
    );
    fs::write(dir.join("backend.rs"), backend).unwrap();
    fs::write(dir.join("tests.rs"), "// Hardware tests run in the main package.\n").unwrap();
    let mut module = show(&repo, "src/poly/kzg/msm/cuda.rs");
    module.push_str(&format!(
        "\n#[cfg(test)] mod mbt {{ use super::*; include!({:?}); }}\n",
        root.join("src/legacy_driver.rs")
    ));
    fs::write(dir.join("mod.rs"), module).unwrap();

    // cuda-ntt adapter, unchanged.
    fs::write(out.join("ntt.rs"), show(&repo, "cuda-ntt/src/lib.rs")).unwrap();

    // EvaluationDomain::gpu_fft (domain.rs), verbatim body in a generic impl.
    let domain = show(&repo, "src/poly/domain.rs");
    let start = domain
        .find("    #[cfg(feature = \"cuda-ntt\")]\n    fn gpu_fft")
        .expect("gpu_fft start");
    let end = start + domain[start..].find("\n    fn ifft").expect("gpu_fft end");
    let body = domain[start..end].replace("#[cfg(feature = \"cuda-ntt\")]", "");
    fs::write(
        out.join("glue.rs"),
        format!(
            "/// Verbatim from {REV}:src/poly/domain.rs.\n\
             pub struct Glue<F>(core::marker::PhantomData<F>);\n\
             impl<F: ff::Field + 'static> Glue<F> {{\n    pub {}\n}}\n",
            body.trim_start()
        ),
    )
    .unwrap();

    fs::write(
        out.join("legacy.rs"),
        format!(
            "#[path = {:?}] pub mod host;\n#[path = {:?}] pub mod loki_cuda_ntt;\n\
             pub mod glue {{ use super::loki_cuda_ntt; include!({:?}); }}\n",
            dir.join("mod.rs"),
            out.join("ntt.rs"),
            out.join("glue.rs")
        ),
    )
    .unwrap();
}
