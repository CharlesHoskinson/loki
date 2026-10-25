# KZG multiopen polynomial parallelism

`loki-multiopen` opts into clone-free, ordered polynomial work inside
`KZGCommitmentScheme::multi_open`. The default feature set is unchanged.

The feature borrows query polynomials while grouping them, then moves the
combined polynomials and point sets through the existing `(cardinality, index)`
ordering. Independent sets use indexed Rayon iteration once there are at least
4,096 input coefficients. Each set's root divisions still call the unchanged
serial `kate_division` kernel, in the original root order. The x1, x2 and x4
linear combinations allocate one output and process disjoint coefficient chunks,
with at least 1,024 coefficients per chunk and polynomial terms in input order.
The output length remains the first polynomial's length, including when `f` is
padded beyond the q polynomials. f_i still pads to `2^params.max_k()`.

Powers, truncated powers, x3 truncation, evaluations, transcript writes,
commitments and the final pi division retain the original algorithm. The verifier,
MSM implementation, evaluation engine and domain implementation are unchanged.
The prover's unused query evaluations in `construct_intermediate_sets` remain;
removing them is outside this package.

The design follows rank 1 of the supplied `other-chains-plonk-kzg.md` research
(axiom-crypto/halo2 PR #37 precedent) and the clone/parallelism findings in
`kzg-multiopen.md`. It does not replace the division kernel with a scan or
`divide_by_roots`.

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
synthetic division remains serial. Unused prover query evaluations are unchanged:
queries are visited sequentially, using the existing parallel evaluation kernel. Lean
proof completion and Rust-to-Lean extraction remain separate obligations.

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
