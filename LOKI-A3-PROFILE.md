# Sherlock A3 phase profile

W10 measures the real k20 Sherlock parent and optimizes its largest remaining
GPU phase: **lookup preparation**, not quotient evaluation. The opt-in
`cuda-argument-commit` feature commits lookup and permutation polynomials in
the coefficient basis already required for openings. No `cuda-quotient`
feature or GPU numerator kernel is implemented. This follows the brief's
explicit instruction to select another dominant phase when measurements
justify it.


Final integrated result (main `877114d`, measured W10 `053577f`):
**294.285 s baseline GPU → 237.808 s optimized GPU**, 56.477 s / **19.19% lower**
(1.2375x). `create_proof` falls 206.230720970 → 143.825054089 s (30.26%).
CPU was 264.036 s. Both GPU proofs completed 45 MSMs and 79 NTTs with no
fallback. A separate baseline that overlapped an external CPU diagnostic is
excluded; the clean repeat supplies the final comparison below.

## Measurement environment and reproduction

Worktree: `/home/charles/repos/loki-wt/w10-gpu-quotient`, branch
`loki/w10-gpu-quotient`, initially `8eb0cee`. GPU: RTX 5090, CUDA 13.4.
Build jobs: 4; A3 Rayon workers: 4. Historical runs used unconditional
`blst/no-threads` (one thread). After merging W9, every measured child mode
explicitly enables `midnight-proofs/loki-cpu-budget` to preserve BLST=1.

The launcher executes the supplied ignored test in Moriarty-loki:

```sh
cargo test --release -p mori-recursion --features loki-gpu --test sherlock_parent -- --include-ignored --nocapture --test-threads=1
```

It adds `midnight-proofs/loki-profile`, patches midnight-proofs to this W10
worktree through an output-local Cargo config, and supplies the existing
A2/SRS environment from `a3-gpu.sh`. CPU mode selects `sherlock-recursion`
instead of `loki-gpu`; optimized GPU mode also selects
`midnight-proofs/cuda-argument-commit`. No Moriarty source/config edit was
made by W10. Generated Cargo artifacts are ordinary test outputs.

Run through the host GPU runner, using an outer CUDA feature to serialize
all modes, including the CPU proof:

```sh
cargo run --offline --release --features cuda --example loki-a3-profile -- cpu-before
cargo run --offline --release --features cuda --example loki-a3-profile -- gpu-before
cargo run --offline --release --features cuda --example loki-a3-profile -- gpu-after
```

The integrated launcher additionally supports `matched-profile`, which runs
`gpu-after`, `gpu-before`, then `cpu-before` sequentially under the inherited
runner lock, and prints exact W10 HEAD and nested features. It rejects a HEAD
change between runs. It acquires no nested lock.

Moriarty is pinned to `6ee918696ab907244c703bbd94bfea9acde7ddd1`.
The launcher rejects tracked Rust/Cargo configuration changes, allowing only
the supplied exact two-line `loki-gpu` feature addition. Baseline pins were
checked after execution; optimized execution checked and printed the pin.
This excludes W12's separate cost-model/PK-reuse changes.

All three historical attributed runs used A2 children from
`/home/charles/repos/Moriarty-loki/target/sherlock-a2/run-Afq6Ibrp/children`,
whose files predate the runs (2026-10-25 00:49:57 UTC). All parent instances
have identical SHA256
`dd9078f802d41c7d3ea8e4501e5fee44952533e0232259bc05e913c25839c6c2`.
Only public counters, timings and synthetic test values are reported.

## H0: actual backend attribution

Every exclusive phase contains a `backends` map with `calls`, `elements`, and
a length-to-count `sizes` histogram. Outcomes distinguish successful
`msm.gpu` / `ntt.gpu`, deliberately selected CPU work (`cpu_no_feature`,
`cpu_threshold`, `cpu_type`, `cpu_size`, `cpu_empty`, `cpu_forced` as
applicable), and failed device attempts (`msm.cpu_fallback`, `ntt.gpu_error`).
One dispatch contributes exactly one terminal outcome. Before integration an
NTT error panicked; integrated W9 preserves input and completes the CPU FFT,
including exactly one inverse normalization, after `ntt.gpu_error`.

Profiles print/store Rayon workers, BLST workers, and enabled GPU adapters.
BLST is `Some(1)` / JSON 1 under `loki-cpu-budget`, otherwise `None` / JSON
null: the private BLST pool is not observable through Rayon. Historical
builds unconditionally selected `blst/no-threads` and reported 1.
MSM errors print on stderr even without a tracing subscriber.

A3 requires successful GPU MSM and NTT calls and rejects any fallback/error;
CPU mode rejects GPU calls. W2 additionally checks histogram totals,
thread/feature metadata and argument-commitment strategy. Metadata and all
exclusive phase nanoseconds are validated from fresh JSONL artifacts.

The original overlapping, pre-H0 runs (CPU 292.712 s, GPU-feature 263.119 s)
are **not** speedup evidence: GPU execution was unattributed and the proofs
overlapped. Logs remain under `target/gpu-runs/20261024T193913-w10-a3-cpu-profile.log`
and `20261024T193914-w10-a3-gpu-profile.log`.

## Pre-integration attributed A3 table

These are serialized historical measurements **before W9/W11 main
integration**. Each test accepted a 6,432-byte parent proof with 28 instance
lanes, with Rayon=4 and BLST=1. A3 acceptance here is Moriarty's verifier
linked to Loki, not an isolated registry stock A3 verifier.

| Exclusive phase | CPU (s) | GPU before (s) | GPU argument feature (s) |
|---|---:|---:|---:|
| `lookup_prepare` | 21.550 | 51.431 | 12.467 |
| `quotient_evaluation` | 59.749 | 46.779 | 47.171 |
| `permutation_products` | 10.573 | 23.825 | 1.644 |
| `witness_synthesis.phase_0` | 6.152 | 6.309 | 6.628 |
| `kzg_multiopen` | 8.243 | 5.581 | 5.928 |
| `compute_nu_poly` | 13.343 | 3.513 | 3.940 |
| `advice_commitments` | 2.581 | 3.282 | 3.369 |
| `trash` | 2.328 | 1.611 | 1.561 |
| `other` | 3.681 | 1.399 | 0.997 |
| `evaluations` | 1.039 | 1.069 | 1.051 |
| `lookup_products` | 8.165 | 0.736 | 0.672 |
| `quotient_commitment` | 5.718 | 0.270 | 0.265 |
| `quotient_construct` | 0.831 | 0.178 | 0.217 |
| `multiopen` | 0.000 | 0.000 | 0.000 |
| **`create_proof` total** | **143.954** | **145.983** | **85.910** |
| **Test prove-plus-verify** | **253.101** | **233.284** | **175.256** |

The GPU before/after prove-plus-verify difference is 58.028 s: **24.87% lower
(1.331x)**. `create_proof` is 41.15% lower. Lookup preparation and permutation
products explain the targeted change; quotient evaluation stays similar.
These are single shared-host samples, not a confidence interval. The supplied
stock 220.2 s and Loki CPU 252.6 s figures are reference values, not fresh
stock measurements by W10.

The profiler's exclusive phase interval excludes keygen, verification, test
setup and JSONL publication. `compute_nu_poly` excludes the nested quotient
timer; quotient evaluation includes extension of argument columns; quotient
construction excludes nested quotient commitments. The residual outside
`create_proof` includes cost modeling, PK setup, child checks and parent
verification. It is not attributed to numerator evaluation.

GPU baseline and optimized runs both completed **45 MSMs and 79 NTTs on the
GPU, zero fallback**. CPU counts match those histograms under `cpu_no_feature`.
All MSMs have length 1,048,576 except one length 1,048,575 in `kzg_multiopen`.
NTTs: 39 length 1,048,576 and 40 length 4,194,304. The latter are 17 in
`compute_nu_poly`, 22 in `quotient_evaluation`, and one in `quotient_construct`.
Optimized argument commits are exactly 10 lookup + 6 permutation, each length
1,048,576, with no extra transform. Exclusive phase sums exactly equal totals:
143,953,700,813 / 145,982,583,427 / 85,910,208,915 ns.

| Mode | Raw runner log under `target/gpu-runs/` | JSONL under `target/a3-profile/` | Runner wall / queued (s) |
|---|---|---|---:|
| CPU | `20261024T195041-w10-a3-h0-cpu.log` | `cpu-before-1792894111/phases.jsonl` | 454.587 / 1055.066 |
| GPU baseline | `20261024T200153-w10-a3-h0-gpu.log` | `gpu-before-1792895178/phases.jsonl` | 402.781 / 1463.621 |
| GPU optimized | `20261024T211142-w10-a3-argument-after.log` | `gpu-after-1792898627/phases.jsonl` | 373.915 / 710.787 |

All runner exits are 0. Baseline W10 source starts were `906c980` / `1ccb1af`,
inferred from commit chronology, not launch-time HEAD prints. Their production
profiler code is identical; the intervening changes are tests/docs. Optimized
core is `4267d75` and checked launcher `c716a9e`.

## Why this optimization

The attributed GPU baseline's largest phase was lookup preparation at
51.431 s, ahead of quotient evaluation at 46.779 s. It regressed relative to
CPU 21.550 s; permutation products also regressed, 10.573 to 23.825 s. This
justifies the alternate target. The provisional numerator plan is deferred.

A direct-backend synthetic warm-cache benchmark demonstrated strong scalar
pattern sensitivity on exactly the same immutable base pointer and length.
One random call warmed each size; three timed repetitions rotated pattern
order. All calls matched independent CPU MSM oracles, without fallback.

| k | Synthetic scalar pattern | CPU sample (ms) | Warm GPU adapter median (ms) |
|---|---|---:|---:|
| 16 | random | 145.961 | 8.147 |
| 16 | repeating 0..15 | 19.783 | 44.161 |
| 16 | repeated full-width | 124.170 | 835.003 |
| 16 | all one | 13.493 | 6.027 |
| 16 | alternating zero/one | 8.512 | 4.972 |
| 18 | random | 445.016 | 27.228 |
| 18 | repeating 0..15 | 64.144 | 170.901 |
| 18 | repeated full-width | 422.948 | 2371.463 |
| 18 | all one | 71.500 | 15.141 |
| 18 | alternating zero/one | 43.781 | 16.381 |

GPU adapter timings include host work, not just kernels. CPU is one oracle
sample per pattern. The k18 full-width-repeat median is 87.1 times random.
sppark's serial oversized-bucket loop supports a bucket-imbalance explanation;
precise kernel attribution remains an inference without a device trace.
The separate digit-one path makes all-one a necessary control. Real argument
scalar values or distributions were never logged.

```sh
cargo test --offline --release --features cuda,loki-profile --lib gpu_structured_scalars -- --ignored --nocapture --test-threads=1
```

Runner exit 0, 30.355 s wall / 1863.903 s queued, test 12.49 s. Log:
`target/gpu-runs/20261024T203817-w10-structured-msm-before.log`.
Benchmark source is identical at `fae1c64` and execution HEAD `4267d75`
(HEAD inferred from chronology); it calls the unchanged backend directly.

## Implementation and mathematical scope

`cuda-argument-commit` implies `cuda` and leaves the feature-off path intact.
For exact-size parameters (`params.max_k() == domain.k()`), lookup and
permutation arguments use their already-required coefficient polynomial for
commitment. The owned polynomial is consumed by the existing inverse
transform. No extra FFT, allocation, proof RNG draw or transcript event is
introduced. Oversized parameters retain the original prefix-Lagrange path.

The accepted `LokiLean/Msm.lean` theorem
`Contract.commitLagrange_eq_commit` (line 810) supplies the basis identity.
The prefix-domain caveat at line 783 motivates the exact-domain guard.
This assumes consistent SRS bases: equal dimensions do not validate arbitrary
`from_parts` or deserialized inputs.

The unchanged numerator contract is `LokiLean/Plonk.lean`:
`numeratorAt`, `EvaluatesNumerator`, `quotient_contract`, and fixed term order
`hornerY_swap`. Rotations use Euclidean remainder with step `2^(K-k)` as in
`LokiLean/Ntt.lean:rotate_extended`; zero-skipping batch inversion is
unchanged. The per-coset gate below exercises the existing evaluator with
independent CPU transforms versus actual GPU transforms, not a separate GPU
numerator kernel.

## Correctness gates before integration

All commands below actually ran and exited 0. GPU commands ran through the
host runner. Local builds use `CARGO_BUILD_JOBS=4`; CUDA builds also set the
supplied toolkit PATH/CUDA_HOME.

```sh
cargo test --offline --release --features cuda-argument-commit,cuda-ntt --lib argument_commit_gpu -- --ignored --nocapture --test-threads=1
cargo test --offline --release --features cuda-argument-commit,cuda-ntt --lib gpu_argument_basis -- --ignored --nocapture --test-threads=1
cargo run --offline --release --features cuda-argument-commit --example loki-w2-gpu
cargo run --offline --release --features cuda-argument-commit,cuda-ntt,loki-profile --example loki-profile -- 14 target/w10-argument-k14.proof
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo run --offline --release --manifest-path tests/stock-profile-verifier/Cargo.toml -- 14 /home/charles/repos/loki-wt/w10-gpu-quotient/target/w10-argument-k14.proof
```

- Per-coset: 1/1 passed, 15 schedules and 56,320 checked entries, random
  gates, multiwrap positive/negative rotations, permutations, lookups,
  trash, multiple instances and ordered Horner challenges. Actual GPU FFT
  completion counts are asserted. Runner wall 26.373 s, test 1.64 s; log
  `target/gpu-runs/20261024T213017-w10-per-coset-argument-gpu.log`.
- Basis: 1/1 passed, 112 direct cold/warm GPU MSMs over synthetic vectors
  checked against independent CPU group sums and CPU interpolation.
  Runner wall 3.477 s, test 1.80 s; log
  `target/gpu-runs/20261024T213017-w10-argument-basis-gpu.log`.
- W2: full k10/k14/k17 differential gate passed, including independent stock
  cross-verification, deterministic controls and the harness's documented
  lookup-byte nondeterminism policy. Real GPU telemetry/strategy checked in
  every proof; Rayon=1, BLST=1. Runner wall 198.251 s; log
  `target/gpu-runs/20261024T212122-w10-w2-argument-gpu.log`.
- Stock: new-feature k14 GPU proof independently accepted by unmodified
  registry `midnight-proofs 0.8.3`, all 1,280 bytes consumed. Generation wall
  16.931 s; log
  `target/gpu-runs/20261024T212122-w10-argument-k14-proof.log`.
  `cargo tree --offline --manifest-path tests/stock-profile-verifier/Cargo.toml -p midnight-proofs --depth 0`
  confirmed the registry package, without a Loki path override.
- Real pinned A3: accepted, counters and timing as above. This is separate
  from the stock k14 fixture. Independent registry stock verification of the
  final integrated A3 artifact is recorded below.

CPU helper tests (3/3) cover edge/repeated/random vectors, oversized prefix
behavior and regenerated downsized bases. Real profiling fixture tests (2/2)
passed with GPU threshold forced off. Test-only per-coset support tests (2/2),
default/no-default Cargo checks, launcher/parser checks, and harness locked
feature checks passed. The strategy test failed before implementation, then
passed. Task reviews passed spec compliance and code quality.

H0 also exercised the actual GPU-less MSM fallback in the sandbox: one
`cudaErrorNoDevice` followed by a disabled-backend call, both matching CPU,
exactly two `msm.cpu_fallback` entries of size 17 and visible stderr output.
Profile draining initially reproduced a parallel-test race; a test-only mutex
fixed it, followed by 100 direct parallel runs (300 tests, all passed).
Production publication never acquires that test mutex.

## Main integration and final measurements

Coordinator requires main `877114d` (W9/W11) before readiness. Its shared
CUDA runtime lock, safe NTT fallback and optional BLST budget change the
measured source. Main is merged in `000b3ed`, with W9 fallback/runtime locking and W11 retained.
BLST metadata is feature-aware; merged CPU fallback/profile checks passed.
The final review found no production correctness defect and requested fixed
A2 inputs for the matched triple. Launcher-only `053577f` snapshots the six
consumed public files once, records their paths/sizes/SHA256 hashes and checks
them before/after every mode. Production source is identical to `000b3ed`.
The historical 24.87% gain must not be relabeled as merged-main performance.
The reverse-order matched triple passed all proof/counter/input gates, but
its baseline timing is excluded for external CPU contention (details below).
The clean baseline repeat passed and replaces the overlapped timing in the
final comparison below.

## Limits

Single shared-host measurements do not quantify variance. Stock 220.2 s is
supplied reference data; stock GPU-proof compatibility is demonstrated by
W2 and the independent k14 fixture, and the final integrated A3 artifact (details below). SRS
coherence is assumed by the basis identity. Quotient evaluation remains on
CPU after GPU column transforms; its device-resident numerator/vanishing
pipeline is deferred. No masked-MSM or cache optimization is claimed.


### Integrated correctness evidence

On the merged production source `000b3ed`:

```sh
CARGO_BUILD_JOBS=4 cargo check --offline --release --no-default-features
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --features cuda-argument-commit,cuda-ntt,loki-profile,loki-cpu-budget --lib profile -- --test-threads=1
cargo test --offline --release --features cuda-argument-commit,cuda-ntt,loki-profile,loki-cpu-budget --lib argument -- --ignored --nocapture --test-threads=1
```

All exited 0. Local release profile-filter tests passed 4/4; the host GPU
pointwise/basis gate passed 2/2 (3.14 s test, 22.340 s runner wall,
261.793 s queued). Raw GPU log:
`target/gpu-runs/20261024T221202-w10-integrated-argument-gates.log`.
The independent stock verifier's historical new-feature acceptance output
was separately captured at `target/w10-argument-k14-stock-verifier.log`, with
registry identity at `target/w10-stock-verifier-dependency.txt` (both exit 0).
Integrated W2 and independent stock k14 verification also passed:

```sh
cargo run --offline --release --features cuda-argument-commit --example loki-w2-gpu
cargo run --offline --release --features cuda-argument-commit,cuda-ntt,loki-profile,loki-cpu-budget --example loki-profile -- 14 target/w10-integrated-k14.proof
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo run --offline --release --manifest-path tests/stock-profile-verifier/Cargo.toml -- 14 /home/charles/repos/loki-wt/w10-gpu-quotient/target/w10-integrated-k14.proof
```

W2 exited 0 at all k10/k14/k17 (222.764 s runner wall / 283.977 s queued),
with actual GPU calls, coefficient strategy, Rayon=1, BLST=1, zero fallback.
Log: `target/gpu-runs/20261024T221202-w10-integrated-w2.log`; fresh records:
`target/w2-gpu/1792901819355917056-2146233/`.
The k14 generator exited 0 (8.489 s wall / 506.569 s queued): 13 GPU MSMs,
one deliberate threshold CPU MSM, 17 GPU NTTs, zero fallback; Rayon=20,
BLST=1. This functional fixture is not an A3 timing sample. Log:
`target/gpu-runs/20261024T221203-w10-integrated-k14.log`.
The separate registry stock verifier exited 0 and accepted all 1,280 bytes;
raw output `target/w10-integrated-k14-stock-verifier.log`.
Default and no-default release Cargo checks passed. The final matched A3 triple passed; its baseline timing overlaps an external
CPU diagnostic and is excluded from the controlled speedup claim. The clean baseline repeat passed. The lab's actual-A3 stock verification also passed, as recorded below.


## W10 commits

- `b38b48e` Add a reproducible Sherlock A3 profiling launcher
- `0486f79` Attribute GPU and CPU dispatches in proof profiles
- `46e17e8` Record exploratory A3 timings and diagnose runner queues
- `906c980` Verify profile attribution when no GPU is accessible
- `6b9ae6e` Record successful GPU dispatch and stock verification evidence
- `1ccb1af` Record the attributed A3 CPU baseline and isolate report tests
- `fae1c64` Select lookup preparation from the attributed A3 GPU profile
- `4267d75` Commit lookup and permutation arguments from existing coefficients
- `c716a9e` Run the W2 gates with real GPU features and checked telemetry
- `0bfc15a` Add randomized per-coset and GPU commitment-basis gates
- `b72d0d3` Record the optimized A3 and stock verification results
- `000b3ed` Merge main and preserve GPU profiling contracts
- `053577f` Pin public A2 inputs across matched A3 profiles


### Matched A3 input identity

Final triple source: `053577f6083b329f2bbef1f3711f92cadc92a24f`.
Input manifest: `target/a3-profile/inputs-1792902759586627637-2246382/input-manifest.json`.
The launcher selected this snapshot once and passes the same expected
manifest to every mode; it checks hashes before and after each proof.

| Public artifact | Bytes | SHA256 |
|---|---:|---|
| `a102.vk` | 1229 | `a6e6ed282daba635711e5d15863441a4888eb28407ec3899ee8722b5e1b02ab9` |
| `a102.instance.json` | 28074 | `9c4aa1a654e6ff6c5ef999581a4667f05b51526b93a9a86d916b3e019b968a64` |
| `a102.proof` | 3440 | `e92150aaa542c1b5f4d3819030f05077635648fd027daf42cff6c5d503856391` |
| `a80.vk` | 1229 | `65414136eabb682558c39e8a76d8d5f436641fe2d76a87bbe88a5b26b093c104` |
| `a80.instance.json` | 28074 | `ed3d7c411c115206b5e9fe6fa8bf40fafe104aac26402b5e7fa69cfb909f0f1e` |
| `a80.proof` | 3440 | `b840bb9276087f837d97181ef498e0ab2999a009dedcf3dbf330e621a46c4eb9` |


### Excluded integrated timing comparison

The first integrated GPU baseline accepted its proof at 292.347 s and
`create_proof` 207.495156365 s, while optimized accepted at 237.808 s and
143.825054089 s. Both dispatched 45 GPU MSMs / 79 GPU NTTs with no fallback.
However, lab reported an ungated CPU diagnostic during the baseline: durable
runner receipt submission 04:45:44.577041 UTC, result 04:46:28.574808 UTC,
wall 43.845 s, queue 0. Its later 04:49:57 communication was not runtime end.

The baseline profile publication mtime is 04:48:34.021235 UTC; subtracting
its phase total gives approximate creation interval 04:45:06.526079 to
04:48:34.021235 UTC. This overlaps the diagnostic. The observed 18.6556%
prove-plus-verify difference is therefore excluded from the controlled
speedup claim, and replaced by the clean baseline repeat below. Proof validity and counters
are unaffected. Optimized creation was approximately 04:37:42.484376 to
04:40:06.309430 UTC and did not overlap; the CPU proof starts after the
reported diagnostic ended. Interval starts inferred from publication mtime
are approximate, not independently instrumented start timestamps.

The confounded baseline remains auditable at
`target/a3-profile/gpu-before-1792903207157075105-2294451/phases.jsonl`;
raw combined log `target/gpu-runs/20261024T222012-w10-integrated-a3-matched.log`.


### Completed integrated CPU and optimized measurements

The matched runner exited 0: 1412.631 s execution, 731.718 s queued. Its
three accepted tests used identical input manifests, with all six hashes
independently rechecked after execution and the same parent instance SHA256.
CPU proof creation occurred after the reported external diagnostic ended.

| Result | CPU | Optimized GPU |
|---|---:|---:|
| Test prove-plus-verify (s) | 264.036 | 237.808 |
| `create_proof` (s) | 147.424826269 | 143.825054089 |
| MSM calls | 45 CPU, no feature | 45 GPU |
| NTT calls | 79 CPU, no feature | 79 GPU |
| Fallback/errors | 0 | 0 |
| Coefficient argument commits | 0 | 16 |

Both report Rayon=4, BLST=1 and accepted a 6,432-byte proof with 28 instance
lanes. Optimized prove-plus-verify is 9.9335% lower than this CPU sample, while
its `create_proof` is only 2.4418% lower. The residual outside the profiler is
not attributed to quotient or argument evaluation. W9's added SRS validation
is preserved; the much larger integrated GPU lookup/multiopen phase times
versus historical measurements are consistent with added host validation and
upload work, but no isolated validation timer was recorded. Phase differences
alone do not establish that causal attribution.

CPU JSONL:
`target/a3-profile/cpu-before-1792903714990966642-2354080/phases.jsonl`.
Optimized JSONL:
`target/a3-profile/gpu-after-1792902759671818294-2247472/phases.jsonl`.
Post-run audit: `target/w10-matched-profile-audit.json`.
Optimized public artifact identity (VK, proof, instance, relation, SRS, inputs,
profile): `target/w10-integrated-a3-public-manifest.json`.


### Final integrated phase table

All three runs use W10 `053577f`, Moriarty `6ee9186`, Rayon=4 and BLST=1.
All six consumed public artifact hashes match, despite the clean repeat
using a separately acquired snapshot; parent instance hashes also match.
The clean repeat was admitted separately after the reported CPU diagnostic
had ended. GPU optimized and CPU samples were unaffected by that diagnostic.

| Exclusive phase | CPU (s) | Clean GPU baseline (s) | Optimized GPU (s) |
|---|---:|---:|---:|
| `lookup_prepare` | 22.088 | 52.586 | 41.864 |
| `quotient_evaluation` | 59.895 | 46.958 | 46.450 |
| `kzg_multiopen` | 8.761 | 34.663 | 36.749 |
| `other` | 4.366 | 30.660 | 0.971 |
| `permutation_products` | 10.333 | 24.626 | 1.364 |
| `witness_synthesis.phase_0` | 6.953 | 6.775 | 6.512 |
| `advice_commitments` | 2.560 | 3.253 | 3.126 |
| `compute_nu_poly` | 12.969 | 3.055 | 3.113 |
| `trash` | 2.486 | 1.394 | 1.523 |
| `evaluations` | 1.067 | 1.082 | 1.015 |
| `lookup_products` | 8.770 | 0.667 | 0.716 |
| `quotient_commitment` | 6.401 | 0.285 | 0.234 |
| `quotient_construct` | 0.775 | 0.227 | 0.189 |
| `multiopen` | 0.000 | 0.000 | 0.000 |
| **`create_proof`** | **147.425** | **206.231** | **143.825** |
| **Test prove-plus-verify** | **264.036** | **294.285** | **237.808** |

The final before/after prove-plus-verify difference is **56.477 s (19.1913%
lower, 1.2375x)**; proof creation is **30.2601% lower**. This is one clean
baseline and one optimized shared-host sample, not a confidence interval.
The earlier 24.87% measurement is historical pre-main-integration evidence.

Lookup, permutation and `other` account for the main changes. Quotient
49.590 s in the excluded run is not the clean baseline: the clean quotient
phase is 46.958 → 46.450 s. Do not attribute that small difference to a GPU
numerator kernel. The first coefficient-basis validated upload can move
between phases under the strategy change; validation was not separately
timed, so the table records observed wall time without claiming an isolated
validation cost. W9 safety checks were retained in both modes.

The clean baseline accepted a 6,432-byte proof with 28 instance lanes,
45 successful GPU MSMs, 79 successful GPU NTTs and zero fallback; optimized
has the same dispatch counts plus 16 coefficient argument commitments.
The CPU sample records exactly 45/79 `cpu_no_feature`, not GPU fallback.
All exclusive nanosecond sums and terminal-outcome histograms were checked
against arithmetic entry totals. Final audit:
`target/w10-final-profile-audit.json`.

Clean-repeat command (host runner, reserved quiet heavy-work window):

```sh
cargo run --offline --release --features cuda --example loki-a3-profile -- gpu-before
```

Raw log `target/gpu-runs/20261024T225842-w10-integrated-a3-before-clean.log`;
JSONL `target/a3-profile/gpu-before-1792904933037812447-2505523/phases.jsonl`.
Runner exit 0, 427.621 s execution / 609.170 s queued; test 425.11 s.
The combined triple's log and CPU/optimized paths are recorded above.
The runner's GPU lock does not serialize CPU-only diagnostics; reproducible
measurements also require the coordinator's heavy-work admission.


### Actual A3 accepted by isolated registry stock 0.8.3

Luna2 ran an independent, verify-only tool from the lab worktree; no new
keygen or proof was performed and no W10 source was changed. The root agent
reviewed its stock dependency/source and verification path and inspected the
completed receipt. The corrected invocation exited 0 and accepted the exact
optimized public artifact pinned by W10's manifest. DoctorWatson measured
1.075 s / 508,866,560 bytes peak RSS for the verification.

Command run by the lab (under its admitted DoctorWatson wrapper):

```sh
cargo run --offline --locked --release --manifest-path probes/a3-stock-verify/Cargo.toml -- /home/charles/repos/loki-wt/w10-gpu-quotient/target/a3-profile/gpu-after-1792902759671818294-2247472/artifacts/parent /home/charles/repos/Moriarty-loki/target/sherlock-srs/bls_midnight_2p20 /home/charles/repos/loki-wt/w10-gpu-quotient/target/w10-integrated-a3-public-manifest.json
```

The tool reads processed `MidnightVK`, authenticates the official k20 SRS,
checks the accepted artifact manifest, runs stock PLONK `prepare`, asserts an
empty proof transcript, and verifies the returned pairing guard. The VK has
one committed identity column and one ordinary 28-lane public column (two
columns in total). All **28 lane mutations**, a corrupted proof byte,
truncation and an appended byte were rejected. All 6,432 accepted proof bytes
were consumed. Seven parser/shape regression tests passed before this run.

The first harness attempt stopped before SRS/pairing at an incorrect
one-total-column assumption; this was fixed to match stock's committed
identity input. It was a harness failure, not a rejected A3 proof. Only the
corrected completed run is counted as the acceptance gate.

Registry `midnight-proofs 0.8.3` checksum:
`19b2e4b1acb581ad741b0cb7c0e9dbdf51142f4e9518886f25fcff40a6702dcc`.
Compiled dep-info names the registry source; fingerprint has no Loki
features. The lab tool source revision is
`4fbb5e930f0c217dcfe5d33486614474ee0386a3`; its receipt pins source and lock hashes.

Accepted W10 public-manifest SHA256:
`f4e2c035e37f996ee0b13563cc2c8dc7e16e607b044f7f95228b12cb6e4531bb`.
Accepted proof SHA256:
`c1dcbec168ee33fdc457c8fe3ca0a7e55fac84042bd25598c556d5107462d39d`.
Accepted processed VK SHA256:
`801467c825c79e063c0603b94c49eef63fd527ff8b3a6d42aa24022b108849a8`.

Original receipt:
`/home/charles/repos/loki-wt/x2-lab/target/gpu-runs/20261024T232041-lab-a3-stock-verify-corrected.log`.
Byte-for-byte local archive: `target/w10-integrated-a3-stock-verifier.log`.
DoctorWatson trace in the lab:
`qa/sherlock/traces/20261025T052309Z-LAB-A3-STOCK-VERIFY.log`.

All gates for this opt-in argument-commit package passed: actual GPU
pointwise/basis checks, full W2, independent stock k14 and actual A3
acceptance, default/feature-off checks, error fallback checks, and the clean
measured comparison. The GPU numerator and resident NTT/vanishing pipeline
are the coordinator's subsequent WP1/WP2 scope, not implemented here.
