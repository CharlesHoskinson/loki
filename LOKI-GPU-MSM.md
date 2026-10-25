# W4 GPU MSM

## Result

The optional `cuda` feature dispatches BLS12-381 G1 MSMs to sppark and preserves
the original CPU implementation otherwise. A CUDA-produced k8 proof is accepted
by an unmodified crates.io `midnight-proofs` 0.8.3 CPU verifier with the same
serialized SRS and VK. Changing its public input is rejected.

sppark **0.1.16** builds and runs on the supplied RTX 5090 (sm_120), using CUDA
13.4 at `/home/charles/.local/opt/cuda-13.4.3`. Local `nvcc --version` reports
13.4.92; `cuobjdump -lelf` confirms `msm.sm_120.cubin`. The adapter also embeds
compute_120 PTX. Rust used for validation: 1.99.1.

ICICLE was not attempted: sppark is usable on this GPU. Both the crate and its
MSM templates are Apache-2.0.

## Dispatch and residency

- `msm_specific` attempts CUDA before zero filtering, preserving stable SRS
  identity. Only `midnight_curves::G1Affine` dispatches to CUDA.
- `LOKI_GPU_MSM_MIN` is a decimal length, read once per process; the default is
  16384. Invalid/unset values use the default; zero is allowed. The effective
  length is `min(coeffs.len(), bases.len())`, preserving the CPU zip semantics.
- GPU errors return to the unchanged CPU path and emit one `tracing::warn!` per
  process. The backend stays disabled after its first error. Below-threshold
  and unsupported-curve calls use the CPU directly.
- A mutex serializes all MSM calls to sppark's shared streams. At most two SRS
  arrays and their bucket workspaces remain resident, with LRU eviction.
- Cache keys are source pointer plus effective length. Each hit compares retained
  projective coordinates exactly, so pointer reuse and in-place mutation cannot
  reuse stale bases. This incurs an O(n) host memory scan on warm calls.
- CUDA's active device is set on every FFI entry because it is thread-local.
  Initial uploads synchronize before their host buffers are released.

The default build has no CUDA dependency or runtime work. Its CPU implementation
is factored into `msm_cpu` without changing its arithmetic or zero filtering.

## Conversion and native error boundary

FFI structs contain arrays of least-significant-first `u64` limbs. Coordinates
are reduced Montgomery residues with R=2^384; scalars use R=2^256. They are
constructed using midnight-curves' public coordinate getters and public BLST
conversion traits. The adapter never casts midnight-curves' struct layout.
A little-endian host is required. C++ static assertions check wire sizes and
alignments against the instantiated sppark types.

Infinity is affine (0,0). sppark's Jacobian result is converted to affine on the
native side, then checked for reduced coordinates, curve membership and subgroup
membership on the Rust side. This avoids depending on midnight-curves'
projective coordinate convention.

Conversion tests were written first and failed with missing conversion types and
functions. The implementation passes 1024 independent big-integer comparisons
and point round trips, including zero, one, maximum scalar and identity. Invalid
coordinate outputs are rejected.

Two build integration issues were found and fixed:

1. The CUDA toolkit's `lib64` must be added explicitly to the linker search path.
   The adapter links the static CUDA runtime, matching sppark.
2. sppark's host field arithmetic uses BLST internal assembly symbols. The
   build mirrors BLST's default ADX selection to select the matching symbols.

Review also found that sppark's implicit `noexcept` MSM destructor calls throwing
CUDA helpers. `build.rs` generates a shadow of the pinned `pippenger.cuh`, changing
only `~msm_t()` to `~msm_t() noexcept(false)`, with a unique-anchor assertion.
The extern C boundaries catch CUDA, standard and unknown exceptions. After an
error, device handles may be retained until process exit rather than destructed
on a poisoned context. This is bounded by disabling subsequent GPU work.

## Verification

All commands use `CARGO_BUILD_JOBS=4`. GPU commands below were submitted through
`gpu-runner`, one at a time. GPU tests are ignored by ordinary local test runs;
the explicit `--ignored` commands are required to exercise the device.

Local checks:

```text
cargo test --offline --lib                         # 30 passed
cargo check --offline --all-targets                # passed
cargo check --offline --features cuda --all-targets # passed
cargo test --offline --features cuda --lib         # 31 passed, 5 ignored
cargo test --offline --features cuda --lib cuda_   # 4 passed, GPU-less test ignored
cargo test --offline --features cuda --lib cuda_no_device_fallback -- --ignored --nocapture --test-threads=1
                                                   # 1 passed in GPU-less sandbox
```

The no-device test exercises the public dispatcher, confirms GPU failure and zero
successful GPU calls, and checks current and subsequent CPU fallback results.
Dispatch tests also check threshold boundaries, invalid environment values,
unsupported G2 dispatch and injected errors.

Initial GPU differential:

```text
cargo test --release --offline --features cuda --lib gpu_differential -- --ignored --nocapture --test-threads=1
# 1123 cases passed, 6.92 seconds of test execution
```

An expanded 10,000-case loop adds power-of-two boundaries through 2^16±1, zero
scalars, identity bases, maximum scalars, cancellation, mutation at a stable
address, mismatched lengths and eight concurrent host callers. Each comparison
calls the GPU backend directly; errors cannot be hidden by CPU fallback.
The expanded run passed **10,108 cases** in **117.09 seconds** of test execution
(120.465 seconds runner wall time including Cargo).
Log: `target/gpu-runs/20261024T175459-w4-msm-10000.log`.

A separate large-size run exercises partial final warps and batches at
2^17±1 through 2^22±1, with a maximum scalar at the last base:

```text
cargo test --release --offline --features cuda --lib gpu_large_boundaries -- --ignored --nocapture --test-threads=1
# all 12 sizes passed in 31.21 seconds (43.652 seconds runner wall time)
```

Log: `target/gpu-runs/20261024T180147-w4-msm-large-boundaries.log`.
Together, the expanded matrix and this run cover 10,120 comparisons.

Full proof and stock verifier:

```text
cargo test --release --offline --features cuda --lib gpu_proof -- --ignored --nocapture --test-threads=1
# passed in 0.66 seconds, 416-byte proof; GPU calls required and any fallback fails
cargo run --offline --manifest-path tests/gpu-compat/Cargo.toml -- target/gpu-msm-proof
# stock 0.8.3 accepted; changed public input rejected
```

The k8 fixture forces `LOKI_GPU_MSM_MIN=1` to exercise CUDA on a small complete
proof. It serializes SRS/VK/proof using existing processed encodings. The verifier
package depends on registry `midnight-proofs = "=0.8.3"`, with no path dependency
or patch. The initial fixture used inferred k4 key generation with a k8 SRS and
failed with `SrsError(8,4)`; it now explicitly requests k8.

Logs:

- `target/gpu-runs/20261024T175209-w4-msm-differential-2.log`
- `target/gpu-runs/20261024T175445-w4-msm-proof-2.log`
- `target/w4-validation/loki-w4-stock-verifier.log`
- Other local checks and the expected initial failures: `target/w4-validation/`.

An initial isolated Cargo home was used to diagnose shared cache-lock contention;
subsequent checks use the shared Cargo home per coordinator instruction.

## Benchmark

```text
cargo test --release --offline --features cuda --lib gpu_benchmark -- --ignored --nocapture --test-threads=1
# passed; every timed result equals the original CPU MSM
```

Milliseconds, one CPU call, one cold call, mean of three warm calls:

| log2(n) | n | CPU | GPU cold | GPU warm |
|---:|---:|---:|---:|---:|
| 16 | 65,536 | 72.692 | 307.278 | 6.927 |
| 17 | 131,072 | 108.222 | 32.333 | 14.468 |
| 18 | 262,144 | 221.874 | 55.652 | 24.440 |
| 19 | 524,288 | 430.804 | 132.956 | 31.312 |
| 20 | 1,048,576 | 759.250 | 247.003 | 57.657 |
| 21 | 2,097,152 | 1,435.199 | 486.217 | 100.870 |
| 22 | 4,194,304 | 3,005.208 | 1,153.978 | 181.221 |

Warm speedups at 2^20 and 2^22 are 13.17x and 16.58x respectively for this MSM
benchmark. These are shared-host samples, not whole-prover speedups.

Bases are distinct subgroup points in an arithmetic progression from random
starting/step points; scalars are random. Generation is outside the timed region.
Cold means an empty SRS cache; the first row also includes initial CUDA context
startup, while later rows retain the context and allocator state. Warm timings
include source-coordinate validation, scalar conversion/upload, kernels, host
reduction, output validation and synchronization, with resident SRS bases.
They are not bare-kernel timings. The first cold call is slower than CPU.

Log: `target/gpu-runs/20261024T175244-w4-msm-benchmark.log` (14.56 seconds of test
execution; 16.982 seconds runner wall time including Cargo).

## Limits and integration notes

- No production k20 recursion proof or end-to-end proving speedup was measured.
  The correctness fixture is k8 with an explicitly lowered threshold.
- No deliberate device-reset or GPU-OOM fault injection was run. No-device and
  injected adapter-error fallback are tested; the native exception boundary and
  destructor change were reviewed.
- Residency includes bases and buckets, with per-call temporary digit/scalar
  buffers. Fixed-base precomputation and commitment batching are future work.
- Only the supplied sm_120/CUDA 13.4 host was tested. `LOKI_CUDA_ARCH` selects
  another numeric compute capability at build time; other targets are untested.
- The separately developed CUDA NTT adapter must share the MSM synchronization
  mechanism before concurrent cross-backend calls: sppark shares GPU streams.
- Git metadata is read-only in the sandbox. Commits use the coordinator's host
  `gpu-runner COMMIT` protocol and remain on `loki/w4-gpu-msm`; nothing is pushed.

## Commits and review

- `fdef4c4 Add resident sppark CUDA MSM with CPU fallback`
- `21fecba Cover partial final batches in large CUDA MSMs`
- Separate code review found the destructor issue above; the final committed
  fix was reviewed again with no remaining important findings.
- The results document is committed separately after final verification.

## Sources

- `~/repos/Moriarty-factory/wiki-llm/loki/LOKI-DESIGN.md`, L3 and correctness gates;
  `10x/LOKI-10X.md`, residency and verifier compatibility.
- [sppark](https://github.com/supranational/sppark), including distributed
  `msm/pippenger.cuh`, `ff/bls12-381.hpp` and the MSM proof-of-concept build.
- midnight-curves 0.3.1 public BLST conversion implementations in `fp.rs`/`fq.rs`
  and coordinate getters in `g1.rs`; BLST 0.3.17 host assembly selection.
