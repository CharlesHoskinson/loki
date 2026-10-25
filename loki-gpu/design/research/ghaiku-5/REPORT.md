# ghaiku-5 REPORT: Filecoin ec-gpu / ec-gpu-gen / bellperson (public docs and source)

Assignment from astra-gpu (RESEARCH ASSIGNMENT). Access date: 2026-10-25 (UTC).
Source boundary: public docs and crate metadata of ec-gpu, ec-gpu-gen and bellperson only.
Not opened: open-icicle source, lab-haiku-1/H21.md, sim scripts. No code copied; the
observations below are paraphrased. Labels: SOURCE (read in a fetched document),
CODE (read in fetched source file), MEASURED (none here), ESTIMATE (our inference),
OPEN QUESTION.

## Sources

- S1 ec-gpu README: https://github.com/filecoin-project/ec-gpu
- S2 ec-gpu-gen directory listing: https://github.com/filecoin-project/ec-gpu/tree/master/ec-gpu-gen
- S3 ec-gpu-gen Cargo.toml: https://raw.githubusercontent.com/filecoin-project/ec-gpu/master/ec-gpu-gen/Cargo.toml
- S4 ec-gpu-gen build.rs: https://raw.githubusercontent.com/filecoin-project/ec-gpu/master/ec-gpu-gen/build.rs
- S5 ec-gpu-gen src/lib.rs: https://raw.githubusercontent.com/filecoin-project/ec-gpu/master/ec-gpu-gen/src/lib.rs
- S6 ec-gpu Cargo.toml: https://raw.githubusercontent.com/filecoin-project/ec-gpu/master/ec-gpu/Cargo.toml
- S7 docs.rs ec-gpu 0.2.0: https://docs.rs/ec-gpu/latest/ec_gpu/
- S8 bellperson README: https://github.com/filecoin-project/bellperson
- S9 bellperson GPU lock source (locks.rs): https://raw.githubusercontent.com/filecoin-project/bellperson/master/src/gpu/locks.rs (was read via a summarising fetch; the key lines are reproduced as paraphrase)
- S10 bellperson multiexp source (multiexp.rs, fetched via summary): https://raw.githubusercontent.com/filecoin-project/bellperson/master/src/gpu/multiexp.rs
- S11 bellperson search hit (docs.rs README 0.26.0 for BELLPERSON_GPUS_PER_LOCK): https://docs.rs/crate/bellperson/latest/source/README.md
- S12 bellperson issue #192 (FFT/multiexp benchmark): https://github.com/filecoin-project/bellperson/issues/192
- S13 filecoin-ffi issue #218 title only ("GPU FFT failed! Falling back to CPU..."): https://github.com/filecoin-project/filecoin-ffi/issues/218 (body NOT read)
- Not available: bellperson src/gpu/fft.rs returned 404 at the path tried; ec-gpu-gen/src/source.rs and program.rs not fetched; the ec-gpu repo tests directory not read.

## Findings

### 1. Licence and provenance (SOURCE)
- ec-gpu (S1, S6): dual "MIT/Apache-2.0" (README: "Licensed under either of" Apache-2.0 or MIT, at your option). Version 0.2.0 (S6, S7).
- ec-gpu-gen (S3): version 0.7.1, license "MIT/Apache-2.0", description "Code generator for field and elliptic curve operations on the GPUs". The repository field still points to `https://github.com/filecoin-project/ff-cl-gen` (a stale URL, observed in S3). Directory listing (S2) has LICENSE-APACHE and LICENSE-MIT.
- bellperson (S8): dual Apache-2.0 / MIT.
- ESTIMATE: All three are compatible with loki-gpu's Apache-2.0. If any code is copied, the attribution goes into loki-gpu NOTICE and design/PROVENANCE.md, and the MIT notice must be kept. Recommend copying nothing; only the design lessons below are used.

### 2. Backend abstraction and fallback (SOURCE)
- ec-gpu exposes two feature flags, `cuda` and `opencl` (S1, S5). If both are compiled in, the runtime env var `EC_GPU_FRAMEWORK` selects one; it defaults to `cuda`, and an unknown value also falls back to cuda (S1).
- The README describes no CPU fallback inside ec-gpu. The CPU path lives in the caller. In ec-gpu-gen (S5) the modules `fft_cpu` and `multiexp_cpu` sit beside `fft` and `multiexp`, which are gated on `cuda` or `opencl`.
- bellperson multiexp (S10): GPU and CPU run concurrently. `BELLMAN_CPU_UTILIZATION` (0..1, default 0, clamped, invalid value logs and uses 0) sets `cpu_n = floor(len * util)` leading elements to the CPU (`multiexp_cpu`, FullDensity); the rest go to the GPU kernels. Each GPU partial result is added to an accumulator; the CPU result is waited on and added last.
- bellperson GPU error handling (S10): GPU errors are stored in `Arc<RwLock<EcResult<()>>>`; after the scope, the error is propagated with `?`, and the CPU result is discarded. The same code uses `expect`/`unwrap`, so a poisoned lock or unexpected reference panics instead of returning an error.
- bellperson README (S8, S11): env var `BELLMAN_NO_GPU` forces CPU. `BELLMAN_GPU_FRAMEWORK` picks cuda or opencl. No automatic CPU fallback on device failure is documented beyond these settings.
- Search snippet only (not verified in body): filecoin-ffi #218 is titled "GPU FFT failed! Falling back to CPU...", which implies a fallback exists at a higher layer.

### 3. Multi-GPU, locking and concurrency (SOURCE + CODE)
- `BELLPERSON_GPUS_PER_LOCK` (S8, S9, S11):
  - unset: one blocking exclusive lock file `bellman.gpu.lock` (no device suffix); each calculation uses all devices;
  - `0`: no lock; README warns "can break things easily";
  - `N > 0`: one advisory lock file per device in the temp directory (name `bellman.gpu.lock` + device unique ID). It uses non-blocking try-lock; busy devices are skipped; the calculation gets up to N free devices and does not wait.
- The locks are advisory OS file locks via the `fs2` crate, not in-process mutexes (S9). `Drop` unlocks.
- The bellperson README says kernels expect exclusive GPU use; overlapping GPU work can crash or give wrong results (S11, search snippet).
- ESTIMATE: Filecoin's multi-proof scheduling is file-lock partitioning of whole devices, not a stream-level scheduler. This is weaker than the scheduler the owner asked for and should not be copied as the design.
- `EC_GPU_NUM_THREADS` (default logical cores) limits host threads (S1). `RAYON_NUM_THREADS` limits library rayon threads (S8).

### 4. Kernel generation and build (SOURCE)
- ec-gpu-gen `generate()` is called from `build.rs` with a `SourceBuilder` (S5). For CUDA it produces a fatbin at compile time and exposes its path in `_EC_GPU_CUDA_KERNEL_FATBIN`; for OpenCL it produces source exposed in `_EC_GPU_OPENCL_KERNEL_SOURCE`, compiled at run time (S1, S5).
- `EC_GPU_CUDA_NVCC_ARGS` overrides the nvcc architecture flags; the default targets several architectures and can make the build slow (S1). bellperson has the equivalent `BELLMAN_CUDA_NVCC_ARGS` (S8).
- The `build.rs` of ec-gpu-gen (S4) is an EMPTY program whose comment says it only makes `OUT_DIR` set for a test. So the generation logic is NOT in ec-gpu-gen's build.rs; it must be in the `generate` function in another file (`source.rs`, not read) or in the caller. OPEN QUESTION: where the nvcc call and its default `-arch`/`-gencode` flags live. Read `ec-gpu-gen/src/source.rs` before relying on it.
- Limits (SOURCE, S1): limbs may be 32 or 64 bit, but CUDA supports 32-bit limbs only. The code assumes the prime's most significant bit is unset, which allows cheaper reduction.
- ESTIMATE: BLS12-381 Fr (255 bits) and Fq (381 bits) fit in 256- and 384-bit containers with the top bit clear, so the MSB assumption holds for both. Recheck against our exact limb layout.

### 5. Byte equivalence with the stock path (SOURCE + OPEN QUESTION)
- bellperson has consistency tests that compare GPU and CPU output: `multiexp::gpu_multiexp_consistency` and `domain::tests::gpu_fft_consistency` (from search snippets; bodies not read, S11 and the search index). These are the closest public evidence. They compare values, not serialized proof bytes.
- No public text found that states GPU and CPU Groth16 proofs are byte-identical.
- ESTIMATE (actionable): the CPU/GPU split adds partial results in a different order, and Jacobian/projective representations can differ while still being the same group element. Byte equivalence therefore needs normalisation to affine (canonical form) before serialisation, which is what our Lean `loki_proof_eq_stock` contract should require. bellperson's accumulator uses `AddAssign` in an order set by the split, so its raw projective bytes are not a stable contract.
- ESTIMATE: `cpu_n = floor(len * util)` makes the split depend on a runtime env var. Our stock-byte contract must not depend on such a split; the split must be a scheduling decision only, with the result normalised.

## Actionable implications for loki-gpu
1. Keep the backend trait with an explicit `Unsupported` error and no panics: bellperson's `expect`/`unwrap` on the GPU error channel is a pattern not to copy. Our "error leaves state" rule is stricter than theirs.
2. Do not use advisory file locks for scheduling several proofs on one GPU. Use an in-process scheduler with explicit stream and buffer ownership; treat the `BELLPERSON_GPUS_PER_LOCK` model as the baseline we improve on.
3. Define the CPU/GPU split as a scheduling decision that does not change output; normalise outputs to affine before comparing to stock bytes. Add a differential test that compares serialised bytes, not only group equality (ties to TEST-PLAN).
4. Build config: expose a per-arch override (like `EC_GPU_CUDA_NVCC_ARGS`) but pin the default to sm_120 for our RTX 5090 target. OPEN QUESTION: whether ec-gpu's nvcc path supports sm_120 and CUDA 13.4 (not verified; we would test it).
5. Cite-only use: ec-gpu/bellperson may be cited as prior art for the env-var and fallback design, with licence attribution in PROVENANCE.md if any text is reused.

## Limits
- Several fetched summaries came through a summarising model, not raw text; quoted env var names and lock behaviour are as reported in S9/S10 and should be re-checked against the raw source before any design decision cites them.
- The bellperson fft.rs path returned 404; the FFT fallback path is not verified in source.
- The ec-gpu-gen `source.rs` (nvcc invocation) and the ec-gpu tests directory were not read.
- No measurements were made. No GPU was used. No network time-sensitive claim is made beyond the access date.

## Addendum (curve applicability, after astra-gpu CURVE CORRECTION)
- No BN254 assumption is made in this report. bellperson uses the `blstrs` BLS12-381 backend (S8), so its FFT/multiexp findings are on BLS12-381, which matches Loki's deployed KZG curve. ec-gpu itself is curve-generic (`GpuField`, `GpuName`; S7); its concrete curve is whatever the caller instantiates.
- Loki's actual curve types and scalar ABI were NOT inspected here (out of this scout's source boundary). OPEN QUESTION for the owners: confirm in the stock/Loki source that the scalar and base fields are BLS12-381 Fr/Fq with 32-bit limb layout before reusing the MSB assumption in section 4.
