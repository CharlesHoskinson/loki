# W13 SRS registry

Worktree: `/home/charles/repos/loki-wt/w13-srs-registry`, branch
`loki/w13-srs-registry`, starting revision `877114d`.

## Design and implementation plan

Source: WP3, baseline §1 and risks §d of
`/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`
(2026-10-24). Implementation is based on the memo and Loki / Apache-2.0 sppark 0.1.16.
The implementation uses Loki's existing midnight-curves / BLST normalization
calls. No files under `~/research/open-icicle-src` were consulted.

- [x] Instrument the existing two-slot cache with per-phase backend counters;
  measure three k17 fixture proofs and three synthetic cold/full/prefix/warm
  sequences at capacity 2^20, n=2^20−1 before changing cache behavior.
- [x] Add the opt-in `cuda-srs-registry` feature (implies `cuda`) and forward
  harness features. Keep the legacy path behind its existing feature.
- [x] Add differential tests that assert prefix reuse, same-address mutation
  detection, corrupt/torsion rejection, LRU eviction, and four interleaved
  coefficient/Lagrange arrays at k19/k20 against stock CPU MSM.
- [x] Replace two slots, only under the feature, with address/capacity entries,
  exact requested-prefix comparisons, and LRU eviction under
  `LOKI_GPU_SRS_BUDGET_BYTES` (default 4294967296). Account for sppark's
  persistent points, buckets and histogram, not just the 96-byte base payload.
- [x] Validate upload candidates on up to four native scoped workers; never use
  Rayon cooperative waits while holding the device/cache locks.
- [x] Repeat measurements; run W2 with the feature, stock verification,
  and every copied full-gate step. GPU commands go through gpu-runner with
  slices at most 600 seconds. Preserve commands, runner logs and results here.
- [x] Document the exact-hit argument, notify `lean-speedups`, coordinate shared-file
  additions with W10, commit tested changes, and publish the handoff with remaining risks.

The registry stores a validated projective coordinate snapshot. This preserves
the exact-coordinate hit predicate without normalizing or trusting a mutable
host pointer. The snapshot uses 144 bytes per host point; the affine device
wire uses 96 bytes.
A match checks all x/y/z limbs of the requested prefix; unused suffixes need
not match. No digest, pinned host identity or generation assumption is used.

The memory budget covers persistent sppark handle allocations, with the
rounded point count and window/bucket count used by its constructor. The SM
count is the maximum over visible CUDA devices: this conservatively bounds
sppark's first eligible device, which need not be CUDA ordinal zero. It is
exact on the supplied single-GPU host. Invocation scratch and other CUDA backends remain outside this
registry budget. An entry larger than the budget returns an error before
upload; existing sticky backend disable and CPU fallback semantics apply.

## Evidence

Runner file names use host local time (UTC−6); bus messages use UTC. CUDA
toolkit is the supplied 13.4.3; GPU jobs are submitted to `gpu-runner`, never
executed directly in this sandbox. These are host-runner measurements, not
lab DoctorWatson trace evidence. Admission, replies and queue durations are
in the durable Loki AgentBus log. The baseline synthetic ran at
05:04 to 05:07 UTC after 902.494 seconds queued; W13 did not request an A3 job.

Completed CPU checks on the instrumented legacy cache:

| Command | Result | Log |
|---|---|---|
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --lib -- --test-threads=4 --skip dev::cost_model::tests::cost_model` | 33 passed, 2 ignored, 1 filtered | `target/w13/baseline-cpu.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --features loki-profile --lib profile::tests -- --test-threads=1` | 1 passed; preceding red run failed for missing counter API | `target/w13/counters-{red,green}.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline -- --test-threads=4 --skip dev::cost_model::tests::cost_model` | 33 unit, 2 integration, 9 doctests passed | `target/w13/full-cpu.log` |
| `./loki-harness/run.sh` | k10/k14/k17 cross-verification and equal VK passed; lookup proof bytes differ as on baseline | `target/w13/w2-cpu.log` |
| `CARGO_BUILD_JOBS=8 RAYON_NUM_THREADS=4 cargo run --release --offline --example cpu-bench -- 16 3 target/cpu-fixture` | Three proofs verified, identical bytes | `target/w13/cpu-fixture.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --manifest-path tests-stock/Cargo.toml` | 1 passed after fixture generation | `target/w13/tests-stock-after-fixture.log` |

The first standalone tests-stock invocation failed because `target/cpu-fixture`
was absent (`target/w13/tests-stock.log`). Generating it in the supplied gate
script's order fixed that invocation; no verifier code was changed.

Baseline source snapshots: `target/w13/baseline-source/{backend,profile,tests}.rs`;
patch: `target/w13/baseline-instrumentation.patch`. SHA256:

- backend.rs: `66c849e35c2a4a44c1c70db0824d8cd46fb715f9bb4ce6bc6fcef4e7900de2c1`
- profile.rs: `55a8082e56e03ee70fbdc3347c8de43aa24db4fee81b24c16b7c1f85b3212a3b`
- tests.rs: `13ff50468dc1ac1276a29cbb2cc61b656feb8e7f9736e730046ff9a2d31d0b12`

The exact full-gate copy with only `cd` changed is
`target/w13/verify-main.sh` (SHA256
`f689b4736e64500e4cb7d1f92f6b976f318039ab1712111be62327ad447665e0`).
`target/w13/verify-main-via-runner.sh` substitutes `runner-cargo` for `flock`
and adds the registry feature to GPU tests, preserving each gate command.
The proxy submits one Cargo command and waits for its runner result; all GPU
slices must stay at most 600 seconds.


## Baseline GPU measurements

Baseline commit: `c40bbf3` (Instrument GPU SRS preparation and prefix regression),
from `877114d`. All baseline GPU commands enabled
`cuda,loki-profile,loki-cpu-budget`; the registry feature did not yet exist.
The latter feature caps BLST at one thread. The synthetic explicitly sets
`RAYON_NUM_THREADS=4`. The original k17 runs did not record startup thread
settings; they are retained as historical evidence, excluded from controlled
speedup claims. The host confirmed the runner inherits environment and does
not force Rayon. Later controlled k17 repeats explicitly force Rayon 4 with
Cargo config. No native validation workers on baseline.

The synthetic uses ChaCha20 seed `[113;32]`, 1024 random subgroup points
repeated to capacity 2^20, and random scalar coefficients. Each of three
samples clears residency, then runs full, n−1 prefix, and repeated prefix.
Every returned group point is checked against independent stock `msm_cpu`,
including its zero-filter path; CPU references are computed outside timing.
This synthetic is not an actual parameter file or chain circuit.

| Baseline quantity | Samples (ms) | Median (ms) |
|---|---|---:|
| Full cold MSM | 29916.847, 29898.936, 29883.397 | 29898.936 |
| n−1 prefix after full | 29879.869, 29887.181, 29853.333 | 29879.869 |
| Repeated n−1 prefix | 62.595, 70.036, 66.941 | 66.941 |
| Full validation | see raw records | 29678.434 |
| n−1 validation | see raw records | 29675.715 |
| Full upload | 235.240, 15.238, 17.478 | 17.478 |
| n−1 upload | see raw records | 17.489 |
| Warm exact compare / prepare | see raw records | 23.308 |
| k17 fixture create_proof | 8758.680, 8847.638, 8894.557 | 8847.638 |
| k17 prepare | see raw records | 7492.751 |
| k17 validation | see raw records | 7419.843 |

Per synthetic sample: two misses/uploads, 201326496 uploaded bytes, followed
by one hit. Per k17 proof: 14 prepares, 12 hits, 2 misses/uploads, 25165728
uploaded bytes. The full-upload first sample includes a cold-context outlier;
report it separately rather than describing all uploads as 235 ms.

Commands and logs:

- `cargo test --release --offline --features cuda,loki-profile,loki-cpu-budget --lib gpu_srs_prepare_measurement -- --ignored --nocapture --test-threads=1`
  Result: exit 0, 203.255 s runner wall including compilation;
  `target/gpu-runs/20261024T224908-w13-before-synthetic.log`.
- Three invocations of `cargo run --release --offline --features cuda,loki-profile,loki-cpu-budget --example loki-profile -- 17`
  Result: exit 0 each, proof verified;
  `target/gpu-runs/20261024T224921-w13-before-k17-{0,1,2}.log`.
- `cargo test --release --offline --features cuda,loki-profile,loki-cpu-budget --lib gpu_srs_prefix_reuses_upload -- --ignored --nocapture --test-threads=1`
  at `c40bbf3` returned expected exit 101: five misses versus one. The regression
  detects the old behavior. In the final source that assertion is gated by
  `cuda-srs-registry`, so legacy ignored-test runs stay compatible.
  `target/gpu-runs/20261024T225022-w13-prefix-red.log`.

`target/w13/summarize.py` derives aggregates directly from JSON records and
writes `target/w13/measurements.json`. Lab lead independently recomputed the
baseline medians from all nine raw records and source `c40bbf3` (bus review
2026-10-25 05:10 UTC). No ungated lab measurements are included.

## Preparation counter schema

Counters live in each exclusive phase's `backends` map. `calls`, `elements`
and `sizes` retain the histogram invariant; additive `wall_ns` defaults to zero
when reading older records. JSONL and serde serialization were checked equal.

| Counter | Histogram unit | Timing scope |
|---|---|---|
| `msm.prepare` | requested n | lookup through retained entry, excluding device/cache lock wait, scalar conversion and MSM invoke |
| `msm.prepare.hit`, `.miss` | requested n | count only |
| `msm.prepare.validation` | supplied capacity | on-curve check, normalization, subgroup check, all worker joins |
| `msm.prepare.upload` | uploaded capacity | synchronous native prepare/allocation/upload call |
| `msm.prepare.upload_bytes` | bytes (96 × capacity) | successful upload payload count |
| `msm.prepare.evict` | evicted capacity | successfully released entries, count only |

Prepare timing includes its validation/upload subtimings; these backend
measurements are not additional exclusive proof phases and must not be added
to the phase total. Only the calling proof thread records them. Key generation
and verification remain outside the proof report; synthetic tests explicitly
create a report guard around each timed MSM for observability.

## Registry soundness and failure behavior

Each resident handle corresponds to an immutable validated host snapshot S.
On a hit, the request has n=min(scalar length, supplied base length), shares
the start address, n is at most the stored capacity, and all x/y/z limbs in
S[0..n] equal the live input prefix exactly. Thus every consumed point is the
same validated group element used for upload. A changed unused suffix cannot
affect that MSM; a later request consuming it must compare it and miss.
The address is a search filter, never a soundness assumption. There is no
digest, generation check, host pinning or immutable-address assumption.

Pinned sppark 0.1.16 `pippenger.cuh:448 to 565` accepts n≤resident capacity and
uses runtime n for digits, batches and point accesses. Combined with exact
prefix equality this yields the stock zip-truncated sum. Empty input returns
the identity. This is the prefix refinement of `stockHit_sound`,
`cachedMsmSeq_eq` and `msmSpecific_take_min`; validity of the stored prefix
follows from whole-upload validation as required by W9 `runV_sound` /
`dispatchSeqW9_eq`. `srs-identity-immutable` remains open and unused.

Lean status (loki-lean `SrsRegistry.lean`, lean-w1, 3ab6f4f): registry soundness
is proved (`msmReg_sound`, `registryCache_eq`, mutation-miss and budget theorems),
but only under a new kernel hypothesis `KernelPrefixOk`: n scalars on a handle
prepared from validated S with n ≤ |S| compute the MSM over S[0..n]. W9's
`KernelOkOnValid` covers equal lengths only and does not imply it
(`w9Kernel_not_enough`), so prefix soundness does not follow from W9 alone. The
prefix behavior of sppark is an assumption backed by the source reading above and
by the prefix regression tests, not by a proof.

Misses validate the entire supplied capacity before upload: on-curve check,
batch normalization, then subgroup check, with at most four native scoped
workers for n≥4096 and serial execution below that. Exact comparison uses the
same partitioning. These workers are additional to Rayon 4 / BLST 1, not a
four-thread process cap. No Rayon install/scope is called under DEVICE/CACHE;
all native workers join before return, including after worker errors/panics.
Normalization uses subbatches of 512, below pinned BLST's 768-point pool
threshold, so it remains serial within each native worker even without
`loki-cpu-budget`. BLST may still initialize its pool before checking that threshold; other BLST
operations may use it. This is a bound on normalization worker dispatch, not
on process thread creation.

A malformed unused suffix on a cold miss rejects that upload. The public API
still computes the valid consumed prefix through CPU fallback. Budget errors,
validation errors and CUDA errors retain the existing sticky disable policy;
there is no automatic recovery in that process. Failed release quarantines
its entry, marks the cache failed, and never retries the possibly deallocated
handle. Host inputs and caller-visible output are unchanged on errors.

The default budget is 4294967296 bytes. On a 170-SM device, persistent charges
are 55409664 bytes for k19 and 109755392 for k20; two coefficient/Lagrange
arrays at each size total 330330112 bytes. The budget excludes per-invocation
scratch, CUDA driver overhead, and other backends. An oversized resident is
rejected before upload. The projective snapshot costs 144 bytes per point;
the device wire is 96 bytes. This trades host RAM for the existing exact
coordinate test without per-hit normalization.

## Verification progress

The prefix regression passed with the feature enabled:
`target/gpu-runs/20261024T230947-w13-prefix-green.log` (exit 0).
The initial feature-enabled library run passed 40 tests:
`target/gpu-runs/20261024T231133-w13-registry-unit.log` (exit 0).

All six isolated registry gates returned exit 0, using
`cargo test --release --offline --features cuda-srs-registry,loki-profile,loki-cpu-budget --lib NAME -- --ignored --nocapture --test-threads=1`:

| NAME | Runner wall (s) | Log under `target/gpu-runs/` |
|---|---:|---|
| `gpu_srs_mutation` | 10.701 (includes compilation) | `20261024T231746-w13-gpu_srs_mutation.log` |
| `gpu_srs_eviction` | 2.070 | `20261024T231746-w13-gpu_srs_eviction.log` |
| `gpu_srs_interleaved` | 31.649 | `20261024T231746-w13-gpu_srs_interleaved.log` |
| `gpu_srs_rayon_contention` | 4.025 | `20261024T231747-w13-gpu_srs_rayon_contention.log` |
| `gpu_srs_budget_rejects_oversize` | 1.719 | `20261024T231747-w13-gpu_srs_budget_rejects_oversize.log` |
| `gpu_srs_invalid_unused_suffix_fallback` | 1.668 | `20261024T231747-w13-gpu_srs_invalid_unused_suffix_fallback.log` |

The LRU case uses 8 MiB and A,B,A,C,A,B, so two residents fit but three do not;
this distinguishes LRU from FIFO. Interleaving uploads four k19/k20 arrays,
then checks n−1, 12345 and full-length requests in mixed order: four uploads,
twelve hits, no eviction. Mutation covers changed used/unused suffixes at the
same address and rejects a consumed off-curve point before upload. Existing
upload tests also reject an on-curve order-three point outside prime-order G1.

W2 feature gate:
`cargo run --release --locked --config target/w13/rayon-one.toml --manifest-path loki-harness/Cargo.toml --features cuda,cuda-srs-registry --bin gate -- 10 14 17`
returned 0 in 175.106 s. All three sizes cross-verified with equal VKs; known
lookup-proof byte differences remain. Log:
`target/gpu-runs/20261024T232012-w13-w2-registry-rayon1.log`.
The first attempt lacked the explicit environment and exited 1 at the startup
Rayon guard before proving (`20261024T231748-w13-w2-registry.log`). The Cargo
config forces `RAYON_NUM_THREADS=1` before child startup; no harness check was
weakened. The harness lock adds the newly activated CUDA dependency closure,
with `home` pinned to the root lockfile's existing 0.5.5.

The first full-gate proxy wrongly treated AgentBus wait exit 75 as a terminal
job result, so that wrapper reported failures and ran stock verification
before its proof existed (`target/w13/full-gate-first.log`). The GPU jobs
continued under the runner. The corrected proxy checks durable result messages
and retries waits; the complete copied gate was rerun successfully. This is a transport
wrapper failure, not a reported CUDA test failure.

Checks covered exact prefix identity, whole-capacity validation, LRU,
conservative SM accounting and failure behavior. The changes drain every worker
join, quarantine failed releases, gate registry assertions on the registry
feature, and keep BLST normalization below its worker-dispatch threshold. The
final source check found no remaining critical, important or minor code issues.
Runtime gates and controlled measurements later completed successfully.

W10 coordination: it adds the phase `backends` map independently. Both sides
agreed that the required `msm.prepare*` auxiliary metrics must be excluded from
W10's later example assertion that sums terminal `msm.*` outcomes. The
coordinator authorized that minimal example filter change after W10 merges;
the current 877114d-based example does not have the conflicting assertion.
This integration prerequisite is not silently renamed or declared resolved.

## Controlled before/after measurements

Final implementation commit: `b6af6f9` (Add opt-in exact-prefix GPU SRS registry).
Measurements used the same file bytes before that host commit, verified by
SHA256 below. The controlled before runs select the unchanged legacy path
by omitting `cuda-srs-registry`; after runs select the registry. All use the
same 4-thread Rayon startup config and `loki-cpu-budget` (BLST 1). After
validation/comparison additionally uses up to four native workers.

All commands went through the host runner in sequential GPU slices. No local
GPU access or A3 job was attempted. Hardware/toolkit supplied by the task:
RTX 5090, 32 GB, sm_120, CUDA 13.4.3; host 20 cores / 90 GB RAM. The build uses
Loki's existing sm_120 native CUDA adapter and pinned sppark 0.1.16; no kernel
or C++ source was changed. `Cargo.lock` pins midnight-curves 0.3.1 and BLST
0.3.17. These fixture measurements are not estimates of chain/A3 speedup.

| Metric (median of 3) | Legacy | Registry |
|---|---:|---:|
| k17 create_proof (ms) | 8968.678 | 2479.872 |
| 2^20 cold full MSM (ms) | 29898.936 | 7689.551 |
| 2^20−1 after full (ms) | 29879.869 | 53.333 |
| Repeated 2^20−1 (ms) | 66.941 | 52.579 |
| k17 prepare (ms) | 7460.926 | 968.290 |
| k17 validation (ms) | 7387.520 | 926.077 |
| k17 upload time (ms) | 4.434 | 1.981 |
| k17 misses | 2 | 1 |
| k17 uploads | 2 | 1 |
| k17 uploaded bytes | 25165728 | 12582912 |

Controlled k17 command, three independent processes per mode:

```
cargo run --release --offline --config target/w13/rayon-four.toml --features cuda,loki-profile,loki-cpu-budget --example loki-profile -- 17
cargo run --release --offline --config target/w13/rayon-four.toml --features cuda-srs-registry,loki-profile,loki-cpu-budget --example loki-profile -- 17
```

The config is `[env] RAYON_NUM_THREADS={value="4",force=true}`. Logs:
`target/gpu-runs/20261024T232601-w13-before-k17-controlled-{0,1}.log`,
`20261024T232602-w13-before-k17-controlled-2.log`, and
`20261024T232602-w13-after-k17-controlled-{0,1,2}.log`.

| k17 samples | Legacy | Registry |
|---|---|---|
| create_proof (ms) | 8968.678, 8962.079, 8976.873 | 2450.309, 2479.872, 2524.992 |
| Runner wall (s) | 33.114, 18.625, 18.572 | 14.610, 9.343, 9.394 |

The complete driver includes SRS construction, key generation, proof creation,
verification, and Cargo/runner overhead. Its median runner wall is
from 18.625 to 9.394 seconds. The first process in each mode compiled: Cargo's
`Finished` durations were 15.95 / 6.65 seconds; subsequent Cargo phases were
1.39 / 1.39 and 1.44 / 1.45 seconds respectively. Thus the driver wall and
`create_proof` timings have different scopes; neither is a chain proof timing.
Each k17 proof was accepted by the example's verifier.

After synthetic command:
`cargo test --release --offline --features cuda-srs-registry,loki-profile,loki-cpu-budget --lib gpu_srs_prepare_measurement -- --ignored --nocapture --test-threads=1`
returned 0 in 36.478 seconds including compilation; log
`target/gpu-runs/20261024T232603-w13-after-synthetic.log`.

The synthetic definitions/seeds/oracles are identical to the initial baseline.
Cold full samples were 7958.077, 7689.551, 7673.446 ms; prefix samples 52.848,
54.443, 53.333 ms; repeated prefix samples 52.579, 53.873, 50.594 ms.
Each after triplet has one miss/upload, two hits, and 100663296 uploaded bytes,
versus two misses/uploads, one hit, and 201326496 bytes before. Cold validation
median is 7473.835 ms. Warm exact-compare/prepare median is 11.641 ms, versus
23.308 ms before. These hit comparisons remain exact and linear in n.

The first cold context cost can move between the registry geometry query and
native prepare; total prepare/MSM measurements include both, while the upload
counter measures the native prepare call. No upload-bandwidth speedup is
claimed from comparing cold-context samples.

Final implementation provenance (same bytes as measured/reviewed):

- `src/poly/kzg/msm/cuda/backend.rs`: `d84fbb23b5ff80513de8b4c0d0f5629d2372e4126418950479b4130fa7ca6d8a`
- `src/poly/kzg/msm/cuda/tests.rs`: `d21738f487a6cc3faa464a9d2103e257eb9b80850170b319caf37b5be492d3c8`
- `src/profile.rs`: `55a8082e56e03ee70fbdc3347c8de43aa24db4fee81b24c16b7c1f85b3212a3b`
- `Cargo.lock`: `38e93c45c658371933cd8f5147b1d00312fa404f79422339ad7d7780ead5b41b`

`target/w13/runner-results.json` preserves exact submitted Cargo argument
arrays, submission/result UTC timestamps, exit codes, runner wall and queue
durations, and absolute log paths. It is collected from durable AgentBus
messages by `target/w13/collect-results.py`. The raw logs and result records
are authoritative over rounded values in this document.

## Final focused gates at b6af6f9

All seven isolated cases were repeated after the final normalization fix at
commit `b6af6f9`; every runner result was exit 0. The command form is the
same as the earlier isolated gates (feature set includes registry, profile,
and CPU budget). No implementation file changed afterward.

| Test | Wall (s) | Log under `target/gpu-runs/` |
|---|---:|---|
| Prefix reuse | 1.972 | `20261024T233520-w13-final-gpu_srs_prefix_reuses_upload.log` |
| Mutation / corrupt consumed suffix | 2.171 | `20261024T233520-w13-final-gpu_srs_mutation.log` |
| LRU eviction | 2.171 | `20261024T233521-w13-final-gpu_srs_eviction.log` |
| Interleaved k19/k20 | 34.195 | `20261024T233521-w13-final-gpu_srs_interleaved.log` |
| Rayon contention | 2.376 | `20261024T233521-w13-final-gpu_srs_rayon_contention.log` |
| Oversized budget rejection | 1.922 | `20261024T233522-w13-final-gpu_srs_budget_rejects_oversize.log` |
| Invalid unused suffix CPU fallback | 1.971 | `20261024T233522-w13-final-gpu_srs_invalid_unused_suffix_fallback.log` |

The final full-gate library stage, without `loki-cpu-budget`, passed 43 tests
(48 ignored). Its long differential stage also passed (180.769 s including
Cargo overhead, 179.19 s test time), covering 10,000 randomized small cases,
size boundaries, same-address mutation, mismatched lengths, identities,
cancellation and concurrent callers. This is in addition to the focused
registry tests above. The remaining full-gate results are recorded below.

## Final full gate

`bash target/w13/verify-main-via-runner.sh` returned 0. Its log
`target/w13/full-gate.log` has exactly twelve stage headers and twelve
`rc=0` results; this was checked explicitly because the supplied script does
not fail fast on individual stages. GPU commands were submitted through the
runner proxy, not executed in the sandbox. The final library/differential/
boundary/proof checks and focused repeats used the final implementation bytes.

| Stage / command | Result |
|---|---|
| `cargo test --release --offline -- --test-threads=4 --skip dev::cost_model::tests::cost_model` | 33 unit, 2 integration, 9 doctests passed; 2 ignored, 1 intentionally filtered |
| `./loki-harness/run.sh` | k10/k14/k17 cross-verification passed |
| `cargo test --release --offline --lib --features loki-multiopen poly::kzg::multiopen -- --test-threads=1` | 6 passed |
| `RAYON_NUM_THREADS=1 cargo run --release --locked --manifest-path loki-harness/Cargo.toml --features loki-multiopen --bin gate -- 10 14 17` | all sizes cross-verified |
| `cargo run --release --offline --example cpu-bench -- 16 3 target/cpu-fixture` | three verified proofs |
| `cargo test --release --offline --manifest-path tests-stock/Cargo.toml` | 1 passed |
| `cargo test --release --offline --features cuda,cuda-srs-registry,cuda-ntt,loki-profile --lib -- --test-threads=1` | 43 passed, 48 ignored; 11.902 s runner wall |
| `cargo test --release --offline --features cuda,cuda-srs-registry,cuda-ntt --lib gpu_differential -- --ignored --test-threads=1` | passed; 180.769 s runner wall |
| `cargo test --release --offline --features cuda,cuda-srs-registry,cuda-ntt --lib gpu_large_boundaries -- --ignored --test-threads=1` | passed, k17 through k22 ±1 boundaries; 142.687 s runner wall |
| `cargo test --release --offline --features cuda,cuda-srs-registry,cuda-ntt --lib gpu_proof -- --ignored --nocapture --test-threads=1` | passed; GPU calls asserted, sticky failure false; 1.920 s runner wall |
| `cargo run --release --offline --manifest-path tests/gpu-compat/Cargo.toml -- target/gpu-msm-proof` | external stock verifier returned 0 |
| `cargo run --release --offline --manifest-path tests/ntt-stock/Cargo.toml --features gpu -- 17 3` | three CPU and three GPU proofs stock-verified; mutations rejected; each GPU proof recorded 17 NTT calls; 19.345 s runner wall |

Final runner logs, under `target/gpu-runs/`:

- `20261024T232831-w13-full-1792906111721064626.log`: library
- `20261024T233230-w13-full-1792906350752885782.log`: differential
- `20261024T233532-w13-full-1792906532159321348.log`: large boundaries
- `20261024T233842-w13-full-1792906722409893356.log`: GPU proof
- `20261024T233846-w13-full-1792906726665506700.log`: NTT stock checks

Final stock-verified GPU proof artifact SHA256:

- `target/gpu-msm-proof/params.bin`: `5950e5128541b5a7e1783fb76d733b2c23770e4052ac2cb77785a501644b93d4`
- `target/gpu-msm-proof/proof.bin`: `a32a7109dbdd4a82d1c2b0349ff5e809c9df0325a376ce783463dc67bc09d280`
- `target/gpu-msm-proof/vk.bin`: `d9e8bbe3b4480224e1ad26696a81673611aff57cb56db889c9bed80c03da1384`

Every completed W13 GPU job ran below 600 seconds; the largest actual runner
wall was 203.255 seconds (initial synthetic including compilation). Queue
waiting is reported separately and is not part of a GPU execution slice.
The host runner itself permits longer jobs; the matrices submitted here stayed
within the task's ten-minute slice bound. `git diff --check` passed, and only
this evidence document remained dirty after the code commit.

## Handoff and remaining limits

- Feature remains opt-in. No real chain/A3 benchmark or real chain stock-verifier
  check was run for this feature; the stock proof fixture is k14. No claim is
  made about chain speedup or production default enablement.
- The budget covers persistent registry allocations, not invocation scratch,
  driver memory/pools or other CUDA backends. Mixed-GPU hosts are charged
  conservatively by maximum visible SM count and were not hardware-tested.
- Exact hits retain O(n) comparison and 144 host bytes per point. Native workers
  add CPU concurrency; they are not a four-thread cap for the entire prover.
- Validation/budget errors intentionally disable GPU for the rest of the process
  and use the existing public CPU fallback. Unknown handles after CUDA release
  errors are quarantined and never retried.
- Existing Lean contracts motivate the invariant; the new prefix-model lemma
  has been requested from `lean-speedups` but was not proved or checked in this
  package. Digest identity remains unused; `srs-identity-immutable` stays open.
- Main was still `877114d` at handoff. W10 shared-file changes were coordinated,
  but no rebase onto its pending merge was performed. The coordinator must
  preserve both profile additions and apply the authorized `msm.prepare*`
  exclusion in W10's later terminal-dispatch assertion before merging these
  branches together. That combined revision needs its integration gate.

Commits before this evidence-only handoff:

- `c40bbf34384bf46a5ad74b0234ad2da3f52bc762`: Instrument GPU SRS preparation and prefix regression
- `b6af6f9108741c0fd1c4f02f6d3f90bedb357bf2`: Add opt-in exact-prefix GPU SRS registry

The assigned branch/worktree is preserved. Git metadata is read-only in the
sandbox; the coordinator/authorized host runner created the commits.
