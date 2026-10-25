# KZG multiopen polynomial parallelism

`loki-multiopen` opts into clone-free, ordered polynomial work inside
`KZGCommitmentScheme::multi_open`. The default feature set is unchanged.

The feature borrows query polynomials while grouping them, then moves the
combined polynomials and point sets through the existing `(cardinality, index)`
ordering. Independent sets use indexed Rayon iteration once there are at least
4,096 input coefficients. In run 1 each set's root divisions called the unchanged
serial `kate_division` kernel, in the original root order. Run 2 replaces that
feature-only fold with monic root-product division (details below). The x1, x2 and x4
linear combinations allocate one output and process disjoint coefficient chunks,
with at least 1,024 coefficients per chunk and polynomial terms in input order.
The output length remains the first polynomial's length, including when `f` is
padded beyond the q polynomials. f_i still pads to `2^params.max_k()`.

Powers, truncated powers, x3 truncation, transcript evaluations and writes,
commitments and the final pi division retain the original algorithm. The verifier,
MSM implementation, evaluation engine and domain implementation are unchanged.
Run 2 skips unused evaluations stored only in the prover's intermediate grouping.

The design follows rank 1 of the supplied `other-chains-plonk-kzg.md` research
(axiom-crypto/halo2 PR #37 precedent) and the clone/parallelism findings in
`kzg-multiopen.md`. The original run-1 measurements below precede product division
and the unused-evaluation shortcut; the run-2 section measures their addition.

The separate formal contract is being developed in
`/home/charles/repos/loki-lean/LokiLean/Multiopen.lean` by `lean-speedups`.
The Rust helper cites it. W11 confirmed the polynomial/power pairing,
first-polynomial output length, padding, truncated powers, unchanged evaluations,
and root order over AgentBus. Its presence is not a claim that the separate Lean
file has been compiled or its intended theorems completed by this package.

## Verification

All commands use `CARGO_BUILD_JOBS=4`, CPU only. Logs and synthetic proof artifacts
are under `target/w11/` (ignored by Git). Source review independently checked
ordered scalar streams, stable set sorting, root order, padding, first-polynomial
length and the unchanged default path. The length check was corrected and covered
before final validation.

Commands run successfully:

```sh
cargo check --offline --locked
cargo check --offline --locked --features loki-multiopen
cargo test --offline --locked --release --lib --features loki-multiopen \
  poly::kzg -- --test-threads=1
cargo test --offline --locked --release --lib \
  --features loki-multiopen,truncated-challenges poly::kzg -- --test-threads=1
cargo test --offline --locked --release --lib --features loki-multiopen \
  poly::kzg::multiopen -- --test-threads=1
cargo test --offline --locked --release --lib \
  --features loki-multiopen,truncated-challenges \
  poly::kzg::multiopen -- --test-threads=1
git diff --check
```

The final four differential tests pass with and without truncated challenges.
They compare against an independent copy of the original serial polynomial and
transcript algorithm. Coverage includes seeded random polynomials, single and
multiple point sets, equal-cardinality ties, repeated points across polynomials,
rejected duplicate queries, zero polynomials, 1/4/20-worker pools, coefficient
chunk boundaries, padded right operands, and short polynomials with a larger SRS.
The test-first compile failed on missing feature helpers before implementation.

## Measurement protocol

The circuit is `examples/loki-profile/circuit.rs::ProfileCircuit`: dense squares,
lookup, permutation and two witness phases, with `2^k - 16` rows. It is a synthetic
fixture, not a Sherlock recursive parent. Host: `homeoffice`, Intel Core Ultra 9
285K, 20 logical CPUs, Linux 6.18.33.1-microsoft-standard-WSL2 x86_64; rustc 1.99.1. No CPU affinity is pinned. Seeds and thread
counts are identical within each comparison. Build baseline is Loki `8eb0cee`;
serial features are defaults plus `loki-profile`; optimized features additionally
include `loki-multiopen`. Builds use release optimization and the locked dependency
set, without CUDA.

Build and save distinct executables before running either:

```sh
cargo build --offline --locked --release --features loki-profile --example loki-profile
cp target/release/examples/loki-profile target/w11/profile-serial
cargo build --offline --locked --release --features loki-profile,loki-multiopen \
  --example loki-profile
cp target/release/examples/loki-profile target/w11/profile-feature
cargo build --offline --locked --release --manifest-path loki-harness/Cargo.toml \
  --features loki-multiopen --bin gate
```

Under the shared DoctorWatson heavy lock, run each binary sequentially for each
`k` in `16 17` and `threads` in `4 20`:

```sh
RAYON_NUM_THREADS="$threads" target/w11/profile-serial "$k" \
  "target/w11/before-k${k}-t${threads}.proof" > "target/w11/before-k${k}-t${threads}.json"
RAYON_NUM_THREADS="$threads" target/w11/profile-feature "$k" \
  "target/w11/after-k${k}-t${threads}.proof" > "target/w11/after-k${k}-t${threads}.json"
cmp "target/w11/before-k${k}-t${threads}.proof" \
    "target/w11/after-k${k}-t${threads}.proof"
RAYON_NUM_THREADS=1 loki-harness/target/release/gate 10 14 17
```

`total_wall_ns` is end-to-end `create_proof`, excluding SRS generation, keygen and
verification. `phases.kzg_multiopen.wall_ns` is the exclusive multiopen phase;
report its time and share alongside the end-to-end time. Every profile invocation
also verifies its proof locally. The profiler asserts its phase sum agrees with
an external timer around `create_proof` to within 10%.

Loki already sorts lookup leftovers at this baseline, so serial/feature lookup
proofs can be compared directly at a pinned thread count. Unmodified registry
stock retains randomized HashMap iteration: W2 lookup equality is diagnostic,
while cross-verification and VK/SRS identity are mandatory. W2's no-lookup control
requires exact bytes. Pinning threads matters because proof RNG use depends on
the active pool size.

## Measurements (2026-10-25 UTC)

Optimized implementation commit: `9ce5d58` (`Parallelize KZG multiopen behind an
opt-in feature`); baseline commit: `8eb0cee`. Each invocation is a fresh process.
k16 uses one sample; k17 uses the median of three samples for phase and
end-to-end times. Phase share is the ratio of those median times.
After the initial pair, k17 sample 2 ran serial then feature, and sample 3 ran
feature then serial. All heavy work was serialized under DoctorWatson's global
lock. Times below are milliseconds; end-to-end always means `create_proof`.

| k | Rayon threads | Path | Samples | KZG multiopen ms | End-to-end ms | Phase share |
|---|---:|---|---:|---:|---:|---:|
| 16 | 4 | Serial | 1 | 291.518 | 1342.234 | 21.72% |
| 16 | 4 | Feature | 1 | 259.243 | 1298.463 | 19.97% |
| 17 | 4 | Serial | 3 | 542.586 | 2612.462 | 20.77% |
| 17 | 4 | Feature | 3 | 488.263 | 2593.579 | 18.83% |
| 16 | 20 | Serial | 1 | 178.479 | 850.694 | 20.98% |
| 16 | 20 | Feature | 1 | 140.596 | 782.283 | 17.97% |
| 17 | 20 | Serial | 3 | 301.311 | 1614.697 | 18.66% |
| 17 | 20 | Feature | 3 | 242.512 | 1551.694 | 15.63% |

The k17 median multiopen reduction is **10.0% at 4 threads** and **19.5% at
20 threads**; corresponding end-to-end median reductions are **0.7% and 3.9%**.
These are shared-host observations from three samples, not a confidence bound.
End-to-end k17 ranges overlap: serial/feature 2608.726–2618.369 /
2581.290–2628.570 ms at 4 threads, and 1551.782–1672.712 /
1496.783–1570.242 ms at 20 threads. The small 4-thread end-to-end difference
should not be treated as an established improvement.

All **eight** before/after whole-proof pairs are byte-identical (1,280 bytes
per proof). Repeated samples also have the same bytes within each thread count.
SHA-256 of the shared serial/feature proof bytes:

| k | Threads | SHA-256 |
|---|---:|---|
| 16 | 4 | `11497864597e0ba1f4d8c411277951028ccb08e2e661a17d0b5ba0765dfb4490` |
| 16 | 20 | `d27893df71c2793b879fd6d6856c6e94cce60f02ad0bcf7768eeb86dd4ec754e` |
| 17 | 4 | `1f708ba944b3f9d57b5b7e561504b0ba6ea3acd2401fbdde31cdf998390e107f` |
| 17 | 20 | `d34d3f07c5c437c445b7758597f593ea55eb554fe9fa51e144a1741190133714` |

Raw phase reports are `target/w11/{before,after}-k{k}-t{threads}[-r{2,3}].json`.
Saved executable SHA-256 values:

- Serial: `485596a3fe1793954d0846af9e9433fedcbbf4a84894e4bf11fdd256a7f649d7`
- Feature: `1c95d07343ac13bbc1ffe5f80df96c6bb419a0d411f5b1bf8a75593cb1daa31e`

Baseline wrapper: `W11-baseline`, exit 0, 16.530 seconds wall, 437,600,256 bytes
peak process-group RSS. The wrapper includes SRS setup, keygen and verification;
those are excluded from the table. Trace:
`target/w11/dw/qa/sherlock/traces/20261025T022844Z-W11-baseline.log`.
An initial preflight attempt found a missing local record directory before any
proof ran; the directory was created before the successful run. Heavy-slot waits
and that preflight failure are excluded from measurements.

## Limits

No GPU, k20 proof, real Sherlock recursive-parent circuit or cross-thread byte
identity is claimed. Parallel scheduling may lose on other shapes/hosts; the
feature remains opt-in and uses a fixed small-input threshold. The final pi
synthetic division remains serial. Lean proof completion and Rust-to-Lean
extraction remain separate obligations.

## W2 gate result

The feature-forwarding harness build and the exact sequential gate command above
passed for k10, k14 and k17. Every case passed both cross-verification directions,
independently generated VK equality, full SRS roundtrips, wrong-public-input and
empty-proof rejection, and appended-byte rejection. The gate also ran its four
broken-constraint MockProver controls. All three no-lookup controls (k10, repeated
by the gate) matched exactly at 1,136 bytes. Lookup proofs were 1,280 bytes and
first differed from registry stock at byte 192, as expected from stock's random
lookup-table map ordering. This diagnostic does not weaken the separate strict
Loki-serial/feature whole-proof equality checks.

`W11-validation` completed with exit 0 in 141.507 seconds wall and 502,743,040
bytes peak process-group RSS, including the feature/repeated profiles, setup,
verification and full W2 gate. Trace:
`target/w11/dw/qa/sherlock/traces/20261025T022901Z-W11-validation.log`.
W2 JSON records and diagnostics: `target/w11/w2-gate.jsonl` and
`target/w11/w2-gate.stderr`. A final readback checked all 16 profile records for
success and exact phase-sum equality, all eight proof pairs for byte equality,
and all three W2 records for cross-verification/VK equality.

The core commit was made through the host runner because this sandbox cannot
write the linked worktree's Git metadata. No push was requested or performed.

## Run 2: root-product division and unused grouping evaluations

The coordinator-approved extension ports the KZG division from
[midnight-zk #565](https://github.com/midnightntwrk/midnight-zk/pull/565) and the
unused prover evaluation change from
[#554's evaluation commit](https://github.com/midnightntwrk/midnight-zk/commit/6eb5d8727d3b05805f678aa9479faca3d85e816d).
Both are behind `loki-multiopen`.

`multiopen::divide_by_roots` builds the ascending coefficients of the monic
`Z(X) = product (X - point)`. One descending pass writes the quotient coefficients
and subtracts only nonzero lower terms of Z from the working remainder. The
remainder is discarded, even when nonzero. The output retains exactly
`poly.len().saturating_sub(points.len())` entries, including trailing zeros; empty
points return the input vector. An empty quotient returns early. For every input
where the stock fold is defined, it equals the successive Kate quotient vector.
This includes repeated roots. The serial Kate kernel and final pi division are
unchanged. The product is especially sparse for roots of a common power; no
sparsity or speedup is assumed for arbitrary point sets.

`utils::construct_prover_sets` calls the existing grouping logic with a zero
evaluation callback. Only this feature prover entry point uses that callback.
`construct_intermediate_sets` retains `Query::get_eval` for the default prover,
verifier, and independent test oracle. Commitment/point indices, labels, duplicate
query rejection and sorted point sets follow the same code. The feature still
allocates the small grouping evaluation vectors, filled with zeros. It skips no
query and removes no transcript field. In particular, the q evaluations written
after x3 and the evaluations in PLONK's transcript remain intact.

The two new differential tests cover random coefficients, arbitrary/repeated/zero
and fourth roots, zero and trailing-zero polynomials, empty points, short vectors,
and a concrete nonzero remainder. They also compare grouping metadata and points
against the default entry point and verify its real evaluations are nonzero while
the feature placeholders are zero. Existing independent coefficient/transcript
comparisons still run at 1/4/20 workers and with truncated challenges. The tests
first failed on the missing helpers; all targeted suites then passed. An
independent source review found no correctness or scope issues.

Run-2 logs and synthetic artifacts are in `target/w11-run2/`. Commands completed:

```sh
CARGO_BUILD_JOBS=4 cargo check --offline --locked
CARGO_BUILD_JOBS=4 cargo check --offline --locked --features loki-multiopen
CARGO_BUILD_JOBS=4 cargo test --offline --locked --release --lib \
  poly::kzg -- --test-threads=1
CARGO_BUILD_JOBS=4 cargo test --offline --locked --release --lib \
  --features loki-multiopen poly::kzg -- --test-threads=1
CARGO_BUILD_JOBS=4 cargo test --offline --locked --release --lib \
  --features loki-multiopen,truncated-challenges poly::kzg -- --test-threads=1
git diff --check
```

Results: default **8 passed**, feature **14 passed**, feature with truncated
challenges **14 passed**, no failures. CUDA conversion tests in these suites are
CPU tests; no GPU was used.

### Run-2 measurement protocol

Circuit and host are the same `ProfileCircuit` and `homeoffice` configuration
described above: Intel Core Ultra 9 285K, 20 logical CPUs, WSL2 Linux
6.18.33.1, rustc 1.99.1, CPU only, no affinity pinning. Measurements are fresh
processes on 2026-10-25 UTC. The comparison has three paths:

- Serial: `61bc758`, with default features and `loki-profile`; it retains the
  serial algorithm from `06b9b38` and its grouping callback calls `get_eval`.
- Run 1: clean merged `06b9b38`, default features plus
  `loki-profile,loki-multiopen`, saved before run-2 implementation.
- Run 2: implementation **`61bc758`**, default features plus
  `loki-profile,loki-multiopen`.

No source changed after the core commit and before these gates. The profile
metadata records the parent HEAD at launch (the implementation was uncommitted
when the job was queued), the final core commit, matching committed source blob
hashes, and all three executable SHA-256 values.

All builds use `CARGO_BUILD_JOBS=4`. The run-1 binary was built and copied first;
serial and run-2 use the same commands with their respective feature sets:

```sh
cargo build --offline --locked --release --features loki-profile,loki-multiopen \
  --example loki-profile
cp target/release/examples/loki-profile target/w11-run2/profile-run1
# Save serial and run-2 executables separately after building each.
cargo build --offline --locked --release --features loki-profile --example loki-profile
cp target/release/examples/loki-profile target/w11-run2/profile-serial
cargo build --offline --locked --release --features loki-profile,loki-multiopen \
  --example loki-profile
cp target/release/examples/loki-profile target/w11-run2/profile-run2
cargo build --offline --locked --release --manifest-path loki-harness/Cargo.toml \
  --features loki-multiopen --bin gate
python3 target/w11-run2/profile.py > target/w11-run2/profile.log 2>&1
flock /home/charles/.cache/doctorwatson/heavy.lock target/w11-run2/w2.sh
```

The saved profile driver runs k17 at 4 and 20 Rayon threads, with three samples
per binary. It takes the global `heavy.lock` for one trio at a time, releasing it
between trios. Mode order rotates: serial/run1/run2, run1/run2/serial,
run2/serial/run1. Each invocation is equivalent to:

```sh
RAYON_NUM_THREADS="$threads" target/w11-run2/profile-"$mode" 17 \
  "target/w11-run2/${mode}-k17-t${threads}-r${sample}.proof" \
  > "target/w11-run2/${mode}-k17-t${threads}-r${sample}.json"
```

Each profile verifies its own proof and the driver compares whole proof bytes
within each trio. `w2.sh` runs the feature-enabled
`RAYON_NUM_THREADS=1 loki-harness/target/release/gate 10 14 17`, saving JSON and
diagnostics to `target/w11-run2/w2-gate.{jsonl,stderr}`. There is no overlap between
these heavy jobs. As above, end-to-end is `create_proof` only; setup, keygen and
verification are excluded.

### Run-2 W2 gate

The rebuilt feature-enabled gate exited **0** for k10, k14 and k17. All three
JSON records report `cross_verified: true` and `vk_equal: true`. The gate also
passed full SRS roundtrips, wrong-public-input/empty/appended-proof rejection and
the broken-constraint MockProver controls. All three no-lookup controls were
byte-identical at 1,136 bytes. Registry-stock lookup proofs differed from Loki
at byte 192, as expected from stock's randomized lookup map ordering; both
cross-verification directions passed. Strict Loki-serial/feature proof comparison
is recorded separately below.

### Run-2 k17 measurements

All numbers are milliseconds, medians of three samples per path. Phase share
is the ratio of the phase and end-to-end medians. Run 1 is the immediate feature
baseline; serial is the feature-off control built from the final source.

| k | Rayon threads | Path | Samples | KZG multiopen ms | End-to-end ms | Phase share |
|---|---:|---|---:|---:|---:|---:|
| 17 | 4 | Serial | 3 | 695.021 | 3398.600 | 20.45% |
| 17 | 4 | Run 1 | 3 | 632.745 | 3385.013 | 18.69% |
| 17 | 4 | Run 2 | 3 | 605.850 | 3305.562 | 18.33% |
| 17 | 20 | Serial | 3 | 451.020 | 2305.820 | 19.56% |
| 17 | 20 | Run 1 | 3 | 382.232 | 2251.260 | 16.98% |
| 17 | 20 | Run 2 | 3 | 365.314 | 2308.253 | 15.83% |

Relative to run 1, run 2 reduces the median multiopen phase by **4.25% at four
threads** and **4.43% at twenty threads**. End-to-end changes are **2.35% lower**
at four threads and **2.53% higher** at twenty threads. Against the serial control,
run-2 multiopen is 12.83%/19.00% lower, while end-to-end is 2.74% lower/0.11% higher.
This measures #565 and #554 together; it does not attribute their individual gains.

The shared host was substantially more variable than in run 1. Neither the small
phase difference nor an end-to-end improvement is established by these samples.
In particular, the twenty-thread end-to-end median regressed relative to run 1.
Do not combine timings from the earlier measurement window with this table to
estimate the run-2 increment. Observed minimum–maximum ranges:

| Threads | Path | Multiopen ms range | End-to-end ms range |
|---:|---|---:|---:|
| 4 | Serial | 682.961–762.944 | 3366.937–3894.662 |
| 4 | Run 1 | 595.520–676.951 | 3184.614–3890.547 |
| 4 | Run 2 | 540.291–762.821 | 3014.942–4218.968 |
| 20 | Serial | 448.242–481.246 | 2255.236–2310.784 |
| 20 | Run 1 | 344.958–393.228 | 2118.268–2450.620 |
| 20 | Run 2 | 311.843–408.555 | 2098.284–2342.930 |

All **18** profile reports passed local proof verification and exact phase-sum
checks. All six serial/run1/run2 proof trios are byte-identical at 1,280 bytes;
the repeated samples are identical too. SHA-256 values at four and twenty threads
remain respectively `1f708ba944b3f9d57b5b7e561504b0ba6ea3acd2401fbdde31cdf998390e107f`
and `d34d3f07c5c437c445b7758597f593ea55eb554fe9fa51e144a1741190133714`.
These also match run 1's k17 proof hashes above.

Saved executable SHA-256 values:

- Serial: `409f906e6abb57a596af7c39a3551c1ae31cb0c88b5bdce97c48a46878123530`
- Run 1: `fda44223936e3e45498473b50359d49581d850d7d3b05055b95f17c837692d09`
- Run 2: `4d169363dec56e8c3962ecb8824a6784abda078bc4e036be5add74cce0687f9a`

Raw profiles: `target/w11-run2/{serial,run1,run2}-k17-t{4,20}-r{1,2,3}.json`.
`profile.log` records execution order and whole-proof comparisons;
`profile-metadata.json` records binary/source identities. Final readback command
`python3 target/w11-run2/summarize.py` validated all 18 reports, repeated proof
equality and all three W2 cross/VK records; results are in `summary.{json,log}`.

### Run-2 formal contract and limits

The root-product algorithm, coefficient length, zero-term skipping, repeated-root
behavior, out-of-domain empty quotient and prover evaluation callback were sent to
`lean-speedups` over AgentBus. The Rust helper cites `LokiLean/Multiopen.lean`.
Its run-2 sections O11.1 and O11.2 now contain `divideByRoots_contract`,
`length_divideByRoots`, `toPoly_divideByRoots`, `divideByRoots_eq_kateFold` and
`prover_evals_unused`. The product-division loop is modeled separately from the
uniqueness contract. This lane read the source and answered the implementation
questions; it did not run Lean or certify a Rust-to-Lean extraction.

The feature stays opt-in. Dense root products still require O(n times number of
roots) arithmetic and use a working remainder buffer; sparse-product gains do not
apply to every circuit. No k20, real recursive-parent, GPU or cross-thread byte
identity result was run for this extension. End-to-end performance remains an
open measurement limitation on this shared host.

## Run 3: preserve the stock verifier source

The verifier source is restored exactly to `8eb0cee`, including whitespace.
The affected verifier functions are `KZGCommitmentScheme::multi_prepare` and
`utils::construct_intermediate_sets`. The latter now contains its original
implementation, with real `Query::get_eval` calls. The separate
`construct_intermediate_sets_with_evals` copy and its `construct_prover_sets`
entry point are both behind `loki-multiopen`; only the feature prover calls them.
The copy differs from the original grouping body only in its evaluation callback.
This duplication keeps the stock verifier source intact and preserves an
independent grouping oracle for the differential tests.

`get_verifier_params` is also unchanged. The complete `params.rs` and `msm.rs`
files are byte-identical to `8eb0cee`, covering verifier parameter access and
serialization, `DualMSM::check`, `SplitDualMSM::verify` and all MSM operations.
No verifier function changes appear in
`git diff 8eb0cee -- src/poly/kzg/`: the only modified existing function is the
prover's `multi_open`; all other additions are feature prover helpers and tests.
No changes were made to `evaluation.rs`, `domain.rs` or any `msm*` file.

The source regression check in `target/w11-run3/check_stock_verifier.py` failed
on both affected functions before restoration, then passed afterward. It compares
the entire `multi_prepare` implementation, the entire original `utils.rs` prefix
(including grouping and shared types), and the full parameter/MSM files as bytes
against `git show 8eb0cee:<path>`. An independent read-only review confirmed the
source equality, callback-only difference and unchanged duplicate-query rejection.
`target/w11-run3/kzg-vs-stock.diff` saves the complete KZG diff for inspection.
Lean coordination was notified that division/reordering semantics are unchanged
and the verifier no longer shares the prover's callback helper. This run adds no
Lean or Rust-to-Lean extraction claim.

Run-3 validation uses `CARGO_BUILD_JOBS=4` throughout. Commands completed for the
restored source (logs are in `target/w11-run3/`):

```sh
RAYON_NUM_THREADS=4 cargo test --offline --locked --release --lib \
  poly::kzg -- --test-threads=1
RAYON_NUM_THREADS=4 cargo test --offline --locked --release --lib \
  --features loki-multiopen poly::kzg -- --test-threads=1
RAYON_NUM_THREADS=4 cargo test --offline --locked --release --lib \
  --features loki-multiopen,truncated-challenges poly::kzg -- --test-threads=1
cargo build --offline --locked --release --features loki-profile --example loki-profile
cp target/release/examples/loki-profile target/w11-run3/profile-serial
cargo build --offline --locked --release --features loki-profile,loki-multiopen \
  --example loki-profile
cp target/release/examples/loki-profile target/w11-run3/profile-run3
cargo build --offline --locked --release --manifest-path loki-harness/Cargo.toml \
  --features loki-multiopen --bin gate
python3 target/w11-run3/check_stock_verifier.py
git diff --check
```

The KZG suites passed **8/8 default**, **14/14 feature**, and **14/14 feature
with truncated challenges**, with no failures. Source byte comparisons also
passed. The profile executables and W2 gate were rebuilt from this restored
source; the Run-2 feature control is the saved `61bc758` executable whose hash
is recorded above.

The Lean owner subsequently reported that the Run-2 composition builds with zero
`sorry` and auditor acceptance. The source contains `lokiOps_meets`,
`loki_multiOpen_eq_stock`, `loki_multiOpen_zero_evals` and
`divideByRoots_eq_kateFold` in `LokiLean/Multiopen.lean`. This lane checked those
names in the source and notified the owner of the restored verifier/prover
separation; it did not run the Lean build itself.

Restoration implementation commit: **`8711224`** (`Restore stock KZG verifier
source`), committed through the host runner after the targeted suites and source
review passed. `target/w11-run3/metadata.json` records this commit, matching
committed source blobs and all three profile executable hashes. Its saved Run-2
control hash matches the recorded Run-2 hash exactly.

### Run-3 k17 trio

A fresh k17 trio passed local verification, exact phase-sum checks and strict
whole-proof byte equality at **four Rayon threads**. The paths are rebuilt
feature-off serial, saved Run-2 feature (`61bc758`), and restored Run-3 feature
(`8711224`). All three proofs are 1,280 bytes with SHA-256
`1f708ba944b3f9d57b5b7e561504b0ba6ea3acd2401fbdde31cdf998390e107f`, also matching
the previous Run-2 serial proof at this thread count.

The circuit remains `examples/loki-profile/circuit.rs::ProfileCircuit`, on
`homeoffice`, Intel Core Ultra 9 285K, 20 logical CPUs, WSL2 Linux 6.18.33.1,
rustc 1.99.1. These fresh-process CPU measurements were taken on 2026-10-25 UTC,
with release builds, default features plus `loki-profile`, and additionally
`loki-multiopen` for Run 2/3. No CPU affinity was pinned. One sample per path:

| k | Rayon threads | Path | KZG multiopen ms | End-to-end ms | Phase share |
|---|---:|---|---:|---:|---:|
| 17 | 4 | Serial | 585.996 | 2862.069 | 20.47% |
| 17 | 4 | Run 2 | 484.178 | 2598.031 | 18.64% |
| 17 | 4 | Run 3 | 480.620 | 2715.963 | 17.70% |

End-to-end is `create_proof` only, excluding setup, keygen and verification.
The shared heavy lock was held across the entire trio. This is a compatibility
rerun with one sample, not evidence of a performance change from restoring the
verifier source. No fresh twenty-thread measurement was requested or run in
Run 3; the Run-2 measurements above remain the repeated-sample record.

`python3 target/w11-run3/validate.py > target/w11-run3/validation.log 2>&1` runs
the trio and then the W2 gate, acquiring the shared DoctorWatson `heavy.lock`
for each job. Each profile invocation is equivalent to:

```sh
RAYON_NUM_THREADS=4 target/w11-run3/profile-serial 17 target/w11-run3/serial-k17-t4.proof
RAYON_NUM_THREADS=4 target/w11-run2/profile-run2 17 target/w11-run3/run2-k17-t4.proof
RAYON_NUM_THREADS=4 target/w11-run3/profile-run3 17 target/w11-run3/run3-k17-t4.proof
cmp target/w11-run3/serial-k17-t4.proof target/w11-run3/run2-k17-t4.proof
cmp target/w11-run3/serial-k17-t4.proof target/w11-run3/run3-k17-t4.proof
```

Raw reports and synthetic proof artifacts are
`target/w11-run3/{serial,run2,run3}-k17-t4.{json,proof}`; the driver saves stderr
separately. `metadata.json` records the executable hashes and source identities.

### Run-3 W2 gate result

The rebuilt feature gate completed with **exit 0**:

```sh
RAYON_NUM_THREADS=1 loki-harness/target/release/gate 10 14 17
```

All three records report `cross_verified: true` and `vk_equal: true`. The full
SRS roundtrips, broken-constraint controls, wrong-input/empty/appended-proof
rejections and three byte-identical 1,136-byte no-lookup controls passed. Registry
stock lookup proofs first differ at byte 192, consistent with its randomized
HashMap ordering; both cross-verification directions passed. The strict
Loki-serial/Run-2/Run-3 byte check above passed independently.

Raw gate output is `target/w11-run3/w2-gate.{jsonl,stderr}`. Final readback checked
all KZG suite summaries, the three profile reports, source and executable hashes,
proof equality and all W2 records. `git diff --check` passed. Only the report
changed after implementation commit `8711224`. No push, GPU or k20 run occurred.
The deliberate prover grouping copy can drift in future edits; its differential
test remains the guard. Shared-host timings are single samples in this rerun,
and Rust-to-Lean extraction is still outside this lane's verification.
