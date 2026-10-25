<p align="center">
  <img src="docs/brand/loki-mark.png" alt="Loki" width="120" height="120" />
</p>

<h1 align="center">Loki</h1>

<p align="center"><b>A prover for midnight-proofs. Tested Loki proofs pass the stock 0.8.3 verifier.</b></p>

<p align="center"><i>Same proof. Any shape.</i></p>

<p align="center"><img src="docs/brand/loki-hero.png" alt="Loki, the shapeshifter, in green and gold flame beneath a full moon, his shape shifting into a wolf and a serpent" width="100%"></p>

<p align="center">
  <a href="docs/ARCHITECTURE.md">Architecture</a> · <a href="docs/GPU.md">GPU</a> · <a href="docs/modules/README.md">Modules</a> · <a href="docs/PLATFORMS.md">Platforms</a>
</p>

## 1. What Loki is

Loki is a fork of [`midnight-proofs`](https://crates.io/crates/midnight-proofs) 0.8.3, the PLONK prover with KZG commitments on BLS12-381 (a fork of halo2). It replaces prover hot paths while keeping the same proof system. The package is still named `midnight-proofs`, so a downstream workspace can use a `[patch.crates-io]` entry without changing its source imports.

In Norse myth Loki is the shapeshifter and the father of Fenrir. Project Fenrir uses Sherlock recursion, and Loki makes its proofs. Loki changes where computation runs, on CPU threads or a CUDA GPU. Other accelerators remain planned.

Here, "same proof" means verifier compatibility for the tested, supported cases. The unmodified stock 0.8.3 verifier accepts the recorded Loki proofs. This does not promise byte-for-byte equality with stock proofs. The default build follows stock's randomized `HashMap` order for leftover lookup rows, so proof bytes can vary across runs. The opt-in `loki-deterministic-lookup-order` feature sorts those rows and can change lookup proof bytes ([`LOKI-CPU.md`](LOKI-CPU.md), [`LOKI-P1-SEAM.md`](LOKI-P1-SEAM.md)). A known inherited limitation also remains: with multiple proofs and mixed committed/plain instance columns, stock's prover and verifier absorb instances in different orders, and stock rejects its own proof. See [`LOKI-QUINT.md`](LOKI-QUINT.md). The compatibility results below cover the recorded fixtures, not every API shape.

Loki changes:

- CPU work: stock lookup-row order by default, with optional deterministic sorting; parallel batch inversions; and an optional CPU budget shared by BLST and Rayon ([`LOKI-CPU.md`](LOKI-CPU.md), [`LOKI-P1-SEAM.md`](LOKI-P1-SEAM.md)).
- GPU MSM and NTT: sppark-based CUDA kernels for BLS12-381 G1 multi-scalar multiplication and scalar-field NTT, behind opt-in features, with CPU fallback ([`LOKI-GPU-MSM.md`](LOKI-GPU-MSM.md), [`LOKI-GPU-NTT.md`](LOKI-GPU-NTT.md)).
- GPU SRS registry: exact-prefix caching avoids preparing the same SRS for every commitment ([`LOKI-SRS-REGISTRY.md`](LOKI-SRS-REGISTRY.md)).
- KZG multiopen: parallel polynomial work without cloning and root-product division ([`LOKI-MULTIOPEN.md`](LOKI-MULTIOPEN.md)).
- Domain construction and rotation arithmetic checks ([`LOKI-HARDEN.md`](LOKI-HARDEN.md)).
- Per-phase prover profiling ([`LOKI-PROFILE.md`](LOKI-PROFILE.md)).

Loki changes the prover. It keeps the stock circuits, keys, transcript order and verifier.

## 2. Status and what comes next

### What runs today

The W2 harness ([`loki-harness/`](loki-harness/)) compares this crate with unmodified crates.io `midnight-proofs =0.8.3`. On the recorded k10, k14 and k17 fixtures, each verifier accepts the other's proofs and the verifying-key bytes match. G2 in [`LOKI-DONE.md`](LOKI-DONE.md) lists k20 and stock 0.8.1 as goals, but neither case has a recorded run; see [`TODO.md`](TODO.md).

The `cuda` and `cuda-ntt` features run sppark 0.1.16 on an RTX 5090 (sm_120) with CUDA 13.4.3. The recorded k8 CUDA proof passed the independent stock 0.8.3 CPU verifier ([`LOKI-GPU-MSM.md`](LOKI-GPU-MSM.md)). GPU errors fall back to the unchanged CPU path (W9, [`LOKI-HARDEN.md`](LOKI-HARDEN.md)).

Section 4 describes the Lean spec, Quint models and differential fuzzing.

### Measured results

All numbers below come from one host (WSL2, Core Ultra 9 285K, RTX 5090). Source links document the measurement scope and available samples. These records describe the revisions named in their source notes. P1 merged in Loki at `a256a9a`; the W10 result remains branch-only.

| Package | Result | Source |
|---|---|---|
| W13 exact-prefix SRS registry | k17 SRS preparation 7.46 s to 0.97 s (7460.926 ms to 968.290 ms) | [`LOKI-SRS-REGISTRY.md`](LOKI-SRS-REGISTRY.md) |
| W11 root-product multiopen | k17 synthetic fixture, medians of three samples: multiopen 12.83% / 19.00% lower than serial at 4 / 20 threads; `create_proof` 2.74% lower / 0.11% higher. End-to-end improvement is not established. | [`LOKI-MULTIOPEN.md`](LOKI-MULTIOPEN.md) |
| W10 GPU argument commitments | A3 Sherlock parent prove-plus-verify: 294.285 s to 237.808 s (-19.2%), one matched sample per side, branch-only. See the checked-in [GPU result summary](docs/GPU.md#the-real-workload-the-k20-sherlock-parent-a3). | [`docs/GPU.md`](docs/GPU.md) |

W10 lives on the branch `loki/w10-gpu-quotient` and is not part of Loki `main` at `a256a9a`. The W11 measurements compare Run 2 with the serial control in the same k17 synthetic fixture. The phase medians are lower, but `create_proof` is 2.74% lower at four threads and 0.11% higher at 20 threads. These samples do not establish an end-to-end improvement.

### Limits

- The GPU MSM slows down on repeated scalars: 2.4 s against 25 ms at k18.
- Kernel speedups alone barely moved the end-to-end time. The time sits in CPU phases such as lookup preparation and multiopen.
- Measure end-to-end latency for the recursion chain; kernel speed alone does not show its performance.
- GPU support is tested on one card and one CUDA version.

### What comes next

- P1's backend API seam and CPU adapter are merged in Loki at `a256a9a`. The separate Apache-2.0 `loki-gpu` backend has its P2 runtime merged at `d71b7ca`; P4 is in progress. The P1 `loki-gpu-backend` feature builds a skeleton that advertises no accelerated operations. See [`LOKI-P1-SEAM.md`](LOKI-P1-SEAM.md) and [`docs/GPU.md`](docs/GPU.md).
- Intel, AMD, mobile, WASM and Apple support remain TODO. See [`docs/PLATFORMS.md`](docs/PLATFORMS.md).
- The resident GPU quotient and tree scheduler are planned in [`LOKI-DONE.md`](LOKI-DONE.md).

## 3. Quick start

The Loki crate and the `loki-gpu/` workspace inside this repository declare Rust 1.76.0. Validation used Rust 1.99.1. Cargo resolves Loki's optional `loki-gpu/crates/*` path crates even for a CPU-only build; they are part of this checkout, so no second repository is needed:

```text
workspace/
  loki/          # this repository, including loki-gpu/
  consumer/
```

Add the patch to the manifest of the downstream workspace that depends on `midnight-proofs`:

```toml
[patch.crates-io]
midnight-proofs = { path = "../loki" }
# or from git:
# midnight-proofs = { git = "https://github.com/CharlesHoskinson/loki", rev = "<commit>" }
```

The git form has not yet been validated by a standalone consumer (TODO T02); the local path patch is the tested form.

Resolve `path` from the downstream workspace root to the directory containing Loki's `Cargo.toml`. In the layout above, `../loki` points to it. Keep the dependency requirement at `midnight-proofs = "0.8"`. The patch selects the Loki source under the existing package name. A validated standalone consumer example remains open under TODO T02; see [`TODO.md`](TODO.md).

### Feature flags

These are the features in [`Cargo.toml`](Cargo.toml).

| Feature | Effect |
|---|---|
| `committed-instances` | Committed instances. Enabled by default (upstream). |
| `truncated-challenges` | Fiat-Shamir challenges truncated to 128 bits, for recursion (upstream). |
| `circuit-params`, `derive_serde`, `dev-curves`, `bench-internal` | Upstream features, unchanged. |
| `tabbycat` | Optional upstream graph-export dependency. Its source in `src/dev/graph.rs` is uncompiled in this checkout. |
| `cuda` | sppark CUDA MSM for BLS12-381 G1. CPU fallback on error. |
| `cuda-ntt` | sppark CUDA NTT for the BLS12-381 scalar field. Independent of `cuda`. |
| `cuda-srs-registry` | Exact-prefix GPU SRS registry. Implies `cuda`. |
| `loki-cpu-budget` | Serial BLST with Rayon MSM partitioning, one CPU budget. |
| `loki-multiopen` | Parallel clone-free KZG multiopen. |
| `loki-harden` | Checked domain construction and widened rotation arithmetic. |
| `loki-profile` | Per-phase prover profiling (`midnight_proofs::profile`). |
| `loki-backend-api` | Opt-in typed backend dispatcher with a stock CPU adapter. |
| `loki-sppark` | Enables the backend API and existing sppark CUDA MSM and NTT adapters. |
| `loki-gpu-backend` | Enables the separate `loki-gpu` CUDA backend. The current skeleton advertises no accelerated operations. |
| `loki-deterministic-lookup-order` | Sorts leftover lookup rows for deterministic placement; can change lookup proof bytes. |

The default feature set remains `committed-instances`. `loki-gpu-backend` is
mutually exclusive with `cuda`, `cuda-ntt` and `loki-sppark`.

`truncated-challenges` is spelled this way in `Cargo.toml`. An older upstream README spelled it `truncate-challenges`.

Select them as usual:

```toml
midnight-proofs = { version = "0.8", features = ["loki-sppark", "loki-multiopen"] }
```

The prover and the verifier must agree on `truncated-challenges`. A mismatch makes verification fail.

### Build without CUDA

Run these commands from the Loki repository root. No CUDA toolkit or `nvcc` is needed for the default build or the `loki-cpu-budget`, `loki-multiopen`, `loki-harden`, `loki-profile` and `loki-backend-api` features.

```sh
cargo build --release
cargo test --release --features loki-multiopen
```

### Build with CUDA

Run these commands from the Loki repository root. The legacy `cuda`, `cuda-ntt`, `cuda-srs-registry` and `loki-sppark` features need the CUDA toolkit. The `loki-gpu-backend` feature also needs it to compile the backend skeleton.

```sh
export CUDA_HOME=/path/to/cuda        # required by the new backend; also works for legacy CUDA
export LOKI_CUDA_ARCH=120             # legacy MSM only; default 120 (Blackwell), digits only
# Optional legacy NTT target: use nvcc spelling, for example sm_120
export LOKI_NTT_CUDA_ARCH=sm_120
"$CUDA_HOME/bin/nvcc" --version
cargo build --release --features loki-sppark
# The new backend requires CUDA_HOME; NVCC defaults to $CUDA_HOME/bin/nvcc
cargo check --release --features loki-gpu-backend
```

The legacy MSM build accepts `CUDA_HOME`, `CUDA_PATH`, `CUDA_TOOLKIT_PATH`, `NVCC` or `nvcc` on `PATH`. The legacy NTT build accepts `CUDA_HOME`, `NVCC` or `nvcc` on `PATH`. The new `loki-gpu-backend` build requires `CUDA_HOME`; if `NVCC` is set, it must resolve under that toolkit root. The command above checks the compiler under `CUDA_HOME`. The validated setup is CUDA 13.4.3 with an RTX 5090. Other architectures and toolkits are untested.

`LOKI_CUDA_ARCH` and `LOKI_NTT_CUDA_ARCH` configure the legacy MSM and NTT builds. The `loki-gpu-backend` skeleton currently advertises no accelerated operations, so its successful build does not demonstrate GPU dispatch.

Runtime variables:

| Variable | Meaning | Default |
|---|---|---|
| `LOKI_GPU_MSM_MIN` | smallest MSM length sent to the GPU | 16384 |
| `LOKI_GPU_SRS_BUDGET_BYTES` | GPU memory budget of the SRS registry | 4294967296 |
| `LOKI_PROFILE` | path of a JSON Lines file for phase profiles (`loki-profile`) | unset |
| `RAYON_NUM_THREADS` | Rayon worker count | all cores |

The first two are documented in [`LOKI-GPU-MSM.md`](LOKI-GPU-MSM.md) and [`LOKI-SRS-REGISTRY.md`](LOKI-SRS-REGISTRY.md).

### Run the cross-verification gate

Run from the Loki repository root. Use this independent harness as the first
end-to-end check. It produces proofs with Loki and unmodified stock 0.8.3, then
cross-verifies them in both directions.

```sh
./loki-harness/run.sh
```

The gate covers k10, k14 and k17 by default. The k20 and stock 0.8.1 cases listed by G2 have no recorded run yet; see [`TODO.md`](TODO.md). See [`loki-harness/README.md`](loki-harness/README.md) for per-case commands.

## 4. How correctness is checked

Loki's compatibility claim is specific: the unmodified stock verifier accepts Loki proofs for the recorded fixtures and configurations. The claim does not imply universal proof-byte equality or acceptance for every API shape. Four kinds of evidence cover different parts of it.

1. W2 stock cross-verification: [`loki-harness/`](loki-harness/) keeps Loki beside unmodified crates.io `midnight-proofs =0.8.3` through package renaming. It never patches the stock crate. Proofs and keys cross between the two as bytes.
2. Lean spec: the `loki-lean` repository (`SPEC.md` and `LokiLean/`, kept next to this one) specifies the NTT, MSM, transcript and prover pipeline. The conditional abstract theorem `Refinement.loki_proof_eq_stock` assumes `Refines`, `SameOrder`, equal inputs, equal randomness and thread count, and the same fixed lookup map order. It proves equality of transcript events for the shared abstract `prove` function; `loki_proof_bytes_eq_stock` derives proof-byte equality using the same codec. It does not establish that deployed Rust or CUDA code satisfies those assumptions. Runtime checks in [`LOKI-HARDEN.md`](LOKI-HARDEN.md) provide empirical component evidence.
3. Quint models: [`quint/`](quint/) holds four models for transcript order, GPU dispatch and fallback, the proving-key cache, and a scheduler proposal. Each passed a 10,000-sample safety campaign. Fifteen Rust tests exercise related implementation behavior. These checks sample safety; they do not prove GPU arithmetic or scheduler liveness. Details: [`LOKI-QUINT.md`](LOKI-QUINT.md).
4. Differential fuzzing (W7): [`fuzz/`](fuzz/) compares Loki with stock 0.8.3 and a naive definition on MSM, NTT, proof round trips, negative cases and VK parsing. For mutated witnesses, Loki and stock `MockProver` must agree: both verifiers reject a mutant that fails constraints, and both accept one that satisfies them. Both reject corrupted proofs, wrong instances and swapped verifying keys. The CPU proof fuzz targets stayed at 4 to 6 input bytes, so those runs were close to seeded random case generation rather than structured-byte mutation. Details: [`LOKI-FUZZ.md`](LOKI-FUZZ.md).

For lookup-row ordering and proof-byte reproducibility, see [`LOKI-CPU.md`](LOKI-CPU.md) and [`LOKI-P1-SEAM.md`](LOKI-P1-SEAM.md).

Each gate has a command or an artifact in [`LOKI-DONE.md`](LOKI-DONE.md). Items without completed evidence are listed in [`TODO.md`](TODO.md).

## 5. Performance

The source links provide methods, hosts and samples where available:

- [`LOKI-PROFILE.md`](LOKI-PROFILE.md): where prover time goes, by phase.
- [`LOKI-CPU.md`](LOKI-CPU.md): CPU work and the CPU-budget option. At four Rayon workers, `loki-cpu-budget` had a k16 median of 1706.948 ms against 1062.044 ms for the default build. At 20 workers, the results were close. The feature is opt-in.
- [`LOKI-GPU-MSM.md`](LOKI-GPU-MSM.md), [`LOKI-GPU-NTT.md`](LOKI-GPU-NTT.md): kernel results and equality tests.
- [`LOKI-SRS-REGISTRY.md`](LOKI-SRS-REGISTRY.md): k17 SRS preparation 7.46 s to 0.97 s.
- [`LOKI-MULTIOPEN.md`](LOKI-MULTIOPEN.md): k17 synthetic fixture measurements. Run 2 multiopen medians were 12.83% and 19.00% lower than serial at 4 and 20 threads. `create_proof` was 2.74% lower and 0.11% higher, respectively, so end-to-end improvement is not established.

The W10 result in section 2 is a single matched sample for an A3 Sherlock parent at k20. It is not a median, and it is not on `main`.

Kernel speedup does not guarantee a faster proof. Most time sits in CPU phases such as lookup preparation, multiopen and witness work. The profile points to end-to-end latency of the Sherlock recursion chain as the target.

To measure on your own host, enable `loki-profile`, call `midnight_proofs::profile::take_report()` after `create_proof`, or set `LOKI_PROFILE=/absolute/path.jsonl`. Benchmarks live in [`benches/`](benches/) and [`examples/`](examples/).

## 6. Documentation map

Written for this release:

| Document | Contents |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | How Loki sits inside the prover, with diagrams. |
| [`docs/GPU.md`](docs/GPU.md) | How GPU acceleration works, and the loki-gpu backend. |
| [`docs/modules/README.md`](docs/modules/README.md) | Every module explained. |
| [`docs/PLATFORMS.md`](docs/PLATFORMS.md) | Intel, AMD, mobile, WASM and Apple: status and TODOs. |
| [`docs/brand/IDENTITY.md`](docs/brand/IDENTITY.md) | Name, lineage, palette and image style. |
| [`docs/NOVELTY.md`](docs/NOVELTY.md) | Evidence-backed comparison with public prior art. |

Engineering notes, one per work package:

| Note | Package |
|---|---|
| [`LOKI-PROFILE.md`](LOKI-PROFILE.md) | W1 phase profiling |
| [`LOKI-CPU.md`](LOKI-CPU.md) | W3 CPU work |
| [`LOKI-GPU-MSM.md`](LOKI-GPU-MSM.md) | W4 GPU MSM |
| [`LOKI-GPU-NTT.md`](LOKI-GPU-NTT.md) | W5 GPU NTT, with [`LOKI-GPU-NTT-BENCH.csv`](LOKI-GPU-NTT-BENCH.csv) and [`LOKI-GPU-NTT-EQUALITY.csv`](LOKI-GPU-NTT-EQUALITY.csv) |
| [`LOKI-FUZZ.md`](LOKI-FUZZ.md) | W7 differential fuzzing |
| [`LOKI-QUINT.md`](LOKI-QUINT.md) | W8 Quint models |
| [`LOKI-HARDEN.md`](LOKI-HARDEN.md) | W9 correctness and robustness |
| [`LOKI-MULTIOPEN.md`](LOKI-MULTIOPEN.md) | W11 KZG multiopen |
| [`LOKI-SRS-REGISTRY.md`](LOKI-SRS-REGISTRY.md) | W13 GPU SRS registry |
| [`LOKI-P1-SEAM.md`](LOKI-P1-SEAM.md) | P1 stock CPU adapter seam and feature status |
| [`LOKI-DONE.md`](LOKI-DONE.md) | Definition of done, gates G1 to G9 |

Other files: [`CHANGELOG.md`](CHANGELOG.md), [`COPYING.md`](COPYING.md), [`TODO.md`](TODO.md), [`loki-harness/README.md`](loki-harness/README.md).

## 7. Licence

Dual licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option. See [`COPYING.md`](COPYING.md). The license files are those of the original Zcash Sapling proving system (halo2), which this code descends from.

The loki-gpu backend module is Apache-2.0 and is a separate, clean-room implementation.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## 8. Acknowledgements

[midnight-proofs](https://crates.io/crates/midnight-proofs), from the Midnight team ([midnight-zk](https://github.com/midnightntwrk/midnight-zk)), is the proof system Loki forks. It started as a fork of [PSE halo2](https://github.com/privacy-scaling-explorations/halo2) v0.3.0 by the Privacy Scaling Explorations team, itself derived from the [Zcash Sapling proving system](https://github.com/zcash/halo2) by the Electric Coin Company. The Midnight team's upstream changes include the generic `PolynomialCommitmentScheme` interface, the simplified transcript, additive selectors, committed instances and truncated challenges.
- [sppark](https://github.com/supranational/sppark) (Supranational, Apache-2.0), version 0.1.16, provides the CUDA MSM and NTT kernels behind `cuda` and `cuda-ntt`.
- [blst](https://github.com/supranational/blst) provides CPU curve arithmetic.
- [Rayon](https://github.com/rayon-rs/rayon) provides CPU parallelism. Set `RAYON_NUM_THREADS` to control it. For WASM, Rayon falls back as described in [Rayon: Usage with WebAssembly](https://github.com/rayon-rs/rayon#usage-with-webassembly).
- Project Fenrir and the Sherlock recursion chain gave Loki its purpose.
