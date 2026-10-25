# Build and feature contract

Status: P2 runtime merged at `d71b7ca`; arithmetic remains an Unsupported
skeleton. Independent runtime review has open blockers in [REVIEW.md](REVIEW.md). **SOURCE** means a design or interface fact;
**CODE** means present in this repository; **MEASURED** means demonstrated by
the named command on the named host; **ESTIMATE** and **OPEN QUESTION** are not
performance claims. Normative prose below is **CODE (proposed contract)** unless
it explicitly identifies an implemented file or a measurement. Build commands
are requirements, not evidence that they have already run.

## Default build

- **SOURCE:** `loki-gpu` is a separate Apache-2.0 repository. Loki must depend on
  the backend API only behind an opt-in feature. The stock `midnight-proofs`
  0.8.3 dependency graph and the no-feature Loki build remain unchanged.
- **CODE:** this workspace contains `loki-backend-api` and `loki-gpu`. The GPU
  crate has `default = []`; its `cuda` feature is explicit. The GPU crate's
  `build.rs` returns before looking for CUDA when that feature is absent. No
  CUDA headers, toolkit, CUDA libraries, or C++ build crate are needed by the
  default build.
- **CODE:** MSM, NTT, quotient and SRS arithmetic shims still report
  `Unsupported`. P2 implements initialization and resident scalar transfers;
  only an initialized healthy runtime advertises `resident_buffers`. No
  arithmetic capability is advertised. Runtime correctness review remains
  open; successful compilation is not production approval. Unsupported is not a successful GPU
  dispatch and must never be counted as acceleration.

From the repository root, the intended CPU-only compile check is:

```sh
CARGO_BUILD_JOBS=3 cargo check --workspace
```

This checks the API and GPU adapter crates as a standalone repository; it does
not change Loki's dependency graph. The Loki integration work must separately
prove that its no-feature build has the stock crate graph, transcript order,
proof bytes, and verifier behavior.

## CUDA feature build

The opt-in build requires CUDA 13.4 or a compatible toolkit whose `nvcc`
recognizes `sm_120`. Set `CUDA_HOME` to the toolkit root; optionally set `NVCC`
to override its `bin/nvcc` with a compiler resolving under the same toolkit root.
The build script invokes `nvcc` only when `cuda` is enabled and
builds native `sm_120` code plus a `compute_80` PTX fallback. A missing compiler,
header, or operation source is a hard build error under this feature; it never
silently changes to the CPU implementation.

```sh
CUDA_HOME="$HOME/.local/opt/cuda-13.4.3" \
  CARGO_BUILD_JOBS=3 cargo check -p loki-gpu --features cuda --locked
```

The linked ABI smoke check uses null/nonempty inputs and verifies that each
stub returns `Unsupported` without changing sentinel output bytes. It does not
initialize or execute a GPU:

```sh
CUDA_HOME="$HOME/.local/opt/cuda-13.4.3" \
  CARGO_BUILD_JOBS=3 cargo test -p loki-gpu --features cuda --locked --lib \
  ffi::tests::all_unsupported_stubs_leave_output_untouched
```

**CODE:** the feature build consumes one shared C ABI header and four CUDA
translation units: `msm.cu`, `ntt.cu`, `quotient.cu`, and P2 `runtime.cu`. The
three arithmetic entry points remain synchronous byte-wire placeholders.
The separate `loki_gpu_rt_*_v1` ABI implements transfers, allocation, events,
streams and a byte-copy self-test kernel. Its 32-byte handle mirror and
16-byte status mirror have checked layouts. Ownership/publication policy
belongs to the Rust coordinator; these raw entry points do not enforce it.
Public v1 resident API calls block until transfer retirement, use backend-owned upload staging, and
return completed download vectors. Device streams/events must remain behind
the coordinator; the current public driver exposure is review finding D4. Any future asynchronous public API must use owned completion state
whose safety does not rely on `Drop` running; direct DMA from or to borrowed
caller slices is forbidden. It must not reinterpret this placeholder ABI as a
production transfer path.

The linker needs CUDA's `cudart` library for a normal linked CUDA build. The
runtime probe must reject devices other than compute capability 12.0 for the
sm_120 implementation. PTX fallback may JIT on another supported device, but
that is not an enabled production target until capability and correctness gates
are documented.

## Interface and failure behavior

- The GPU crate depends on `loki-backend-api`; it owns CUDA compilation and
  implementation details, while the API owns host-side operation contracts.
- No CUDA pointer or runtime type crosses the backend-neutral API. The GPU
  crate's public raw-driver exposure is a review blocker (D4), not an approved
  application interface.
- The C ABI uses fixed-width integer status codes. Every arithmetic stub returns
  `UNSUPPORTED` before reading input or modifying output.
- API errors distinguish `NotStarted`, `InputsPreserved`, and
  `SessionPoisoned`. Every returned error preserves caller-visible inputs and
  publishes no output; a poisoned generation cannot accept more device work.
  The GPU crate never decides that Loki may retry on CPU. The Loki dispatch
  layer can retry only from retained host inputs and before transcript state
  has advanced.
- Device failures poison the affected backend generation. New sessions may be
  created only after runtime state is re-established; a poisoned generation is
  not reused for another proof.

## Measurements and provenance

**MEASURED:** host-runner `cargo check --workspace --locked -j3`
exited 0 (log: `target/gpu-runs/20261024T232057-gluna8-default-check.log`,
GPU: false). The earlier API-only check also exited 0
(`target/gpu-runs/20261024T231232-gluna8-api-check.log`, GPU: false).
After the cc-rs compiler-selection guard was added, the same locked workspace
check exited 0 again (`target/gpu-runs/20261024T233221-gluna8-final-default-check.log`,
GPU: false).

**MEASURED runner setup failures:** two CUDA-feature requests exited 101 before
compiler selection because the runner did not provide `CUDA_HOME`
(`target/gpu-runs/20261024T232104-gluna8-cuda-check.log` and
`target/gpu-runs/20261024T232321-gluna8-cuda-check-configured.log`). The runner
ignored the JSON `env` object. Three attempts to pass `--config env.*` were
rejected as `command not allowed` before Cargo started. The final supported invocation used a temporary workspace `.cargo/config.toml`
containing only `CUDA_HOME` and `NVCC`. It was removed after verification and is
not part of the committed build configuration.

**MEASURED:** the frozen skeleton passed host-runner
`cargo check -p loki-gpu --features cuda --locked -j3` (exit 0;
[log](verification/cuda-check.txt)), followed by
`cargo test -p loki-gpu --features cuda --locked -j3 --lib` (exit 0;
[log](verification/cuda-stub-test.txt)). The one linked test called all three
native stubs and verified Unsupported plus untouched output. It did not open a
GPU or execute arithmetic. The selected compiler was
`/home/charles/.local/opt/cuda-13.4.3/bin/nvcc`, reporting CUDA release 13.4,
V13.4.92; Rust was `rustc 1.99.1 (58cf8f9c3 2026-10-13)`. The directory name
13.4.3 is an installation label, not the compiler's reported version.
The receipt applies to the original skeleton commit `df9584f`;
[default-check log](verification/rust-default-check.txt) records the CPU-only
workspace check. GPU kernel timing and proof speedup require separate
measurements; these checks establish neither.

The architecture and clean-room constraints come from
`/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`
and `OWNER-DIRECTION-recursion.md` (2026-10-24). **Do not read or derive code
from open-icicle or the prohibited lab transcription.** For the exact
cross-crate traits and safety contracts, see the assigned API and runtime
designs in this repository.

Build-tool provenance: Cargo's optional-dependency/feature behavior follows
the [Cargo features guide](https://doc.rust-lang.org/cargo/reference/features.html)
and [build-script reference](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
The locked `cc` build dependency uses its documented
[`Build::cuda` interface](https://docs.rs/cc/latest/cc/struct.Build.html) to
select NVCC while leaving host compiler selection to cc-rs. Because cc-rs
selects NVCC from its inherited environment snapshot, the build script
canonicalizes and validates the toolkit paths, then binds `NVCC` in its own
isolated process environment before constructing `cc::Build`. `Build::env`
sets child-process variables and does not select the compiler. No cc-rs code is
copied into this repository.



## P2 gates and their limits

**MEASURED (implementer evidence):** [P2 review](p2/REVIEW.md) records default
compilation, 39 runtime-contract tests, 10 native ABI tests (one measurement
ignored), three CUDA unit tests and sanitizer runs over seven native cases.
The native ABI gate now requires an actual device by default;
`LOKI_GPU_ALLOW_NO_DEVICE=1` explicitly permits skipping and must never be
used for a GPU-required qualification. Run native tests only through the
host runner with device admission. Existing commands are:

```sh
cargo check --workspace --locked -j3
cargo test -p loki-gpu --locked -j3 --test runtime_contract
CUDA_HOME="$HOME/.local/opt/cuda-13.4.3" \
  cargo test -p loki-gpu --locked --features cuda --test ffi_abi -- --test-threads=1
```

**MEASURED (independent review):** four additional no-GPU regressions fail
against `d71b7ca`: cross-runtime handle identity, setup teardown after a
sticky error, normal teardown after a sticky error, and success publication
after a sticky timing error. [Raw log](verification/p2-review-regressions.txt)
and [reproducer](verification/p2-review-regressions.rs) are retained.
These failures qualify the existing passes; the runtime is not approved for
resident arithmetic promotion until the corrective gates in REVIEW.md pass.

**CODE:** `ErrorKind::Cancelled` is an API category, separate from the native
status ABI. Cancellation before work is `NotStarted`; after successful
retirement it is `InputsPreserved`. A failed drain/free retains its actual
failure category and `SessionPoisoned`. P2 follow-up owns the runtime mappings
and tests. Cancellation never authorizes automatic CPU retry.


**MEASURED (lead rerun after the additive API change):** locked workspace
check exits 0 ([log](verification/p2-api-check.txt)); existing runtime contract
suite passes 39/39 ([log](verification/p2-existing-contracts.txt)). Both ran
through the host runner with no CUDA feature or device access. These are
compatibility checks, not closure of the four independently reproduced defects.
