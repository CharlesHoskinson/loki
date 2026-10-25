# W5: GPU NTT for the quotient

## Design and implementation plan

The `cuda-ntt` feature is independent of W4's `cuda`. It selects sppark's
BLS12-381 Fr NTT through a small optional path dependency. The unsafe FFI lives
in that dependency; the proofs crate retains its unsafe-code prohibition.
Dispatch uses a safe `Any` downcast of `Vec<F>` to `Vec<Fq>`, so other fields
retain the stock CPU implementation. Transforms below 2^10 use the CPU.

The adapter uses natural input/output ordering and standard (not sppark coset)
NTTs. `Fq` and sppark both use reduced Montgomery residues with R=2^256.
Conversion uses the public `Fq`/`blst_fr` conversions, not a Rust layout cast.
The root table is checked against the domain root. sppark inverse transforms
already normalize by 1/n; the domain must not apply that factor twice.
The existing order-three `ZETA` distribution, zero-padding, truncation and
rotation/extended-domain stride remain in `domain.rs`. CUDA errors fail loudly;
there is no silent CPU fallback for eligible transforms. Calls are serialized
for bounded GPU memory use, and buffers return to the host after each call.

Alternatives considered: a custom radix-two kernel adds arithmetic/reduction
risk; a full resident quotient backend exceeds this first-step package.
Device residency is deferred until the equality and compatibility gates pass.

Execution checklist:

- [x] Add strict CPU/GPU differential tests first; observe missing dispatch fail.
- [x] Add optional sppark backend, checked roots, serialized FFI and feature dispatch.
- [x] Compare all five domain conversion entry points on random, zero, one,
  maximum scalar, impulse and mixed edge inputs, transform logs 10 through 23,
  with extension factors 1, 2 and 4. Check real GPU call counts.
- [x] Run a k=17 proof through an unmodified crates.io 0.8.3 CPU verifier using
  serialized public SRS/VK and fresh proof randomness. Benchmark quotient spans
  and end-to-end proving against an explicitly selected CPU backend in the
  same executable, using identical public keys and circuit.
- [x] Record timings, commands, logs, limitations and commits; review and commit.

## Result

All 1260 entry-point comparisons passed for transform logs 10 through 23, random
and edge inputs, and extension factors 1, 2 and 4. The largest cases ran as 18
separate jobs with exit 0. Three GPU k17 proofs were accepted by the unmodified stock CPU
verifier; the feature-disabled regression also passed. Warm quotient/proving
medians improved by 1.41x/1.11x on the synthetic fixture, with the limitations
below. Device residency remains a later step.

## Initial evidence

`cargo test --lib poly::domain -- --test-threads=1`: 2 passed, 0 failed.
Host runner log: `target/gpu-runs/20261024T173827-w5-baseline.log`.
The same tests passed locally with `CARGO_HOME=/tmp/loki-w5-cargo` and
`CARGO_BUILD_JOBS=4` (the shared registry cache is read-only in this sandbox).

Upstream reference: https://github.com/supranational/sppark (Apache-2.0).

## Implementation and use

Enable with `--features cuda-ntt`. No CUDA dependency is built without that
feature. A working CUDA toolkit and GPU are required for eligible transforms;
GPU errors panic with the backend error rather than silently executing CPU.
The proofs crate still denies unsafe code; only `cuda-ntt/src/lib.rs` contains
the documented FFI boundary. The dependency is pinned to sppark 0.1.16.

The threshold is 1024 elements. Other field types and lengths outside
2^10..2^28 retain `best_fft`. Presence of `LOKI_NTT_CPU` explicitly selects the
stock CPU arithmetic (used by the controlled benchmark). Toolkit discovery
uses `CUDA_HOME`, or the resolved `NVCC`/`nvcc` on PATH. The adapter adds the
non-system toolkit library directory to the link search path. Its compiler
uses four workers and emits compute_80 PTX plus native sm_120 when supported;
`LOKI_NTT_CUDA_ARCH` selects a single explicit target.

All host values use public `Fq <-> blst_fr` conversions. The adapter stages a
second host vector, transfers Montgomery residues, and copies back only after
successful synchronization. Roots are compared with sppark's table before
launching a transform. Inverse normalization happens on the device exactly
once. Coset powers, zero-padding, truncation and quotient rotations remain the
stock host code. No transcript, VK, SRS or proof-format code changes.

## Reproduce

Use `CARGO_BUILD_JOBS=4`. The commands below run in this worktree; use the host
GPU runner for commands requiring a device. The independent fixture manifest
uses two distinct Cargo sources: Loki by path and stock 0.8.3 from crates.io.
There is no registry patch in that fixture.

```sh
cargo check --offline --no-default-features
cargo test --offline --lib -- --test-threads=1
cargo check --offline --features cuda-ntt
cargo test --release --features cuda-ntt --lib gpu_ntt_ -- --test-threads=1
cargo test --release --features cuda-ntt --lib gpu_ntt_equality -- --ignored --skip gpu_ntt_equality_22 --skip gpu_ntt_equality_23 --test-threads=1 --nocapture
cargo test --release --features cuda-ntt --lib gpu_ntt_equality_22 -- --ignored --test-threads=1 --nocapture
for factor in 1 2 4; do
  for pattern in random zero one max impulse mixed; do
    cargo test --release --features cuda-ntt --lib "gpu_ntt_equality_23_f${factor}_${pattern}" -- --ignored --test-threads=1 --nocapture
  done
done
cargo test --release --features cuda-ntt --lib gpu_ntt_benchmark -- --ignored --test-threads=1 --nocapture
cargo run --release --manifest-path tests/ntt-stock/Cargo.toml --features gpu -- 17 3
cargo run --release --manifest-path tests/ntt-stock/Cargo.toml -- 17 1
```

The ignored matrix and benchmark must run serially. The benchmark changes the
process backend environment variable; the matrix checks exact successful GPU
call deltas, including both transforms in `extended_to_lagrange`. Sizes below
the threshold inside extended-domain cases are explicitly checked as CPU calls.

## Proof compatibility and quotient timing

On the supplied RTX 5090/CUDA 13.4 host, the synthetic k=17 (extended k=18)
circuit fills all
but 32 rows, has three advice columns, a public instance, multiplication,
a next-row rotation and copy/permutation constraints. Setup is outside timing.
All witnesses are synthetic public constants. No private witness or setup
secret is serialized. The serialized public SRS and VK are read by the
**unmodified crates.io midnight-proofs 0.8.3** verifier with the same Blake2b
transcript. Proof randomness is fresh `OsRng` on every invocation.

Three CPU-mode proofs and three GPU-mode proofs were accepted by stock. Every
wrong-public-input mutation was rejected. Each GPU proof completed 17 NTTs;
CPU-mode proofs completed zero. Each proof was 1344 bytes. This is verifier
compatibility, not a claim of byte identity under fixed randomness.

The comparison below uses the **same feature-enabled executable**, same SRS/VK
and circuit, with `LOKI_NTT_CPU` present/absent. Samples are three proofs per
backend, CPU first, so these numbers remain subject to shared-host and ordering
noise. They are not production recursion measurements.

| Phase (wall time) | CPU median ms | GPU median ms | Ratio |
| --- | ---: | ---: | ---: |
| Numerator, including column extension | 349.819 | 176.309 | 1.98x |
| Vanishing division and inverse coset recovery | 28.121 | 10.606 | 2.65x |
| Quotient, including its CPU MSM commitments | 723.207 | 512.459 | 1.41x |
| Complete `create_proof` | 2172.838 | 1950.617 | 1.11x |

The quotient span covers `compute_nu_poly` through `vanishing.construct`, so
it includes quotient commitments. Numerator/recovery spans are nested; do not
add them to the quotient total. Tracing spans exist only with `cuda-ntt` and
have no subscriber/output requirement for ordinary proving.

Evidence: `target/gpu-runs/20261024T174424-w5-stock-proof.log`, runner exit 0,
32.781 s including fixture compilation/setup/verification.

## Per-call timing

[LOKI-GPU-NTT-BENCH.csv](LOKI-GPU-NTT-BENCH.csv) records all 140 combinations:
transform logs 10..23, extension factors 1/4, five entry points. Values are
median wall milliseconds from three samples after one warmup pair. CPU/GPU
execution order alternates. Both paths use the same public domain methods and
include cloning, allocation, coset handling and host/device transfers.
CSV sizes label the extended domain: base-only transforms use size 2^log/factor.
At logs10/11 with factor4, those base-only operations remain below the threshold
and the GPU-mode column therefore measures the documented CPU fallback.
The independent CPU equality oracle's timings are **not** these benchmarks.

Representative factor-one results (base and extended length both 2^k):

| Operation | k17 CPU/GPU ms | k20 CPU/GPU ms | k23 CPU/GPU ms |
| --- | ---: | ---: | ---: |
| `coeff_to_lagrange` | 8.280 / 2.184 | 122.470 / 46.629 | 1323.385 / 350.314 |
| `lagrange_to_coeff` | 7.739 / 2.189 | 121.632 / 46.360 | 1181.203 / 385.162 |
| `coeff_to_extended` | 7.294 / 2.614 | 130.324 / 49.466 | 1652.929 / 403.127 |
| `extended_to_coeff` | 8.210 / 2.682 | 141.668 / 50.752 | 1665.055 / 447.421 |
| `extended_to_lagrange` | 15.235 / 4.562 | 245.846 / 82.421 | 2017.758 / 669.064 |

Evidence: `target/gpu-runs/20261024T174457-w5-call-bench.log`, runner exit 0,
147.154 s including compilation. Context/table initialization is warmed and
has not been independently benchmarked.

## Validation record and limits

- Before implementation, the strict dispatch test failed at runtime with
  `eligible FFT did not execute on the GPU`; CPU numerical output alone passed.
- Default unit suite: 27 passed, 0 failed (`target/w5-default-lib.log`).
- Feature and feature-disabled checks passed; the feature-enabled stock fixture
  also passed `cargo check` (`target/w5-final-fixture-check.log`).
- Wrong-root and invalid-length errors preserve input exactly; local targeted
  test passed (`target/w5-final-error-test.log`).
- GPU smoke passed after adding the non-system CUDA runtime library path:
  `target/gpu-runs/20261024T174151-w5-dispatch-link.log` (1 passed).
- Initial equality run passed complete tests for logs 10..21, covering six
  patterns (seeded random, zero, one, -one, last-position impulse and mixed
  reduced edge values) at extension factors 1/2/4. Its log stopped during log22
  without a runner result; that run alone is not evidence for logs22/23.
  Evidence: `target/gpu-runs/20261024T174247-w5-equality.log`.
- Isolated log22 rerun passed all 90 combinations with the strengthened exact
  call-count checks: 1 passed, 0 failed, 77.49 s test time (90.673 s runner wall).
  Evidence: `target/gpu-runs/20261024T174902-w5-equality-22.log`.
- Refreshed logs10..21 matrix passed 12/12 with exact call-count assertions:
  `target/gpu-runs/20261024T175302-w5-equality-low-final.log`, runner exit 0,
  86.796 s wall (86.32 s tests).
- Final GPU smoke/error selection passed 2/2 (large matrix ignored):
  `target/gpu-runs/20261024T175207-w5-smoke-final.log`, exit 0, 1.368 s wall.
- Feature-disabled k17 regression passed stock verification and public-input
  mutation rejection: `target/gpu-runs/20261024T175151-w5-default-stock.log`,
  exit 0, 15.503 s wall. It printed extended k=18, 131040 occupied rows and
  20 Rayon threads; `create_proof` took 2313.322 ms (one regression sample,
  separate setup/SRS from the controlled paired benchmark).
- The original monolithic log23 job also stopped without an exit result during
  its factor-one impulse case. The runner subsequently completed other jobs.
  The largest matrix is split into 18 independently runnable cases (six
  patterns times three extension factors); each still checks all five entry
  points and exact successful GPU-call deltas. The change affects test
  scheduling; arithmetic and backend code are unchanged. Interrupted logs do
  not count as passes.
- All 18 independent log23 cases subsequently passed with explicit runner exit
  0 and passing test footers, completing the requested 1260 comparisons across
  all sizes. [LOKI-GPU-NTT-EQUALITY.csv](LOKI-GPU-NTT-EQUALITY.csv) records all 20
  successful validation jobs, exit statuses, wall times and exact log paths.
  Its 18 largest cases cover each factor/pattern pair and all five conversions.
  The cause of the two interrupted combined jobs remains undiagnosed; no failure
  or mismatch output was returned for them. Long combined-job reliability
  remains unverified.
- No critical or important defect remained. The exact call-count assertions and
  architecture configuration were corrected before a fresh feature build.

Device-resident columns and `evaluate_numerator` fusion are deferred. Coset
multiplications, numerator evaluation, vanishing division and all MSMs remain
on CPU. No claim is made for a full GPU quotient or production recursion.
Concurrent W4 MSM/W5 NTT scheduling requires one shared lock/context policy:
sppark shares GPU streams, while these packages currently have separate locks.
Default sequential proof phases do not overlap the two backends.

Two implementation commits precede this report:

- `8e1a33c` Add optional CUDA NTT domain backend
- `ee46f60` Verify CUDA NTT proofs with the stock CPU verifier

Only the supplied Linux/RTX 5090 host has run this backend. Other architectures,
CUDA failures during device teardown, and lengths above 2^23 have not been
exercised. The 2^10 threshold is a prototype dispatch policy, not a portable
autotuned crossover. No constant-time GPU execution claim is made.
