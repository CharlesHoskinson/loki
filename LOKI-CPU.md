# CPU work (W3)

## Design and implementation plan

Preserve the 0.8.3 circuit, keys, transcript ordering, RNG consumption and proof
encoding. Deterministic table placement selects one valid lookup permutation;
stock previously selected a different valid permutation on each process run.

1. Add a public, synthetic lookup/copy circuit and a k=16 timing example. Export
   VK, verifier parameters and proof for a separate registry-only verifier.
   Establish the baseline and demonstrate the fixed-RNG reproducibility failure.
2. Make lookup iteration deterministic. Measure ordered maps versus sorting
   distinct hash-map entries; test repeated proofs at fixed Rayon pool sizes.
3. Parallelize batch inversions and independent preparation work, reuse product
   buffers where profitable, and compare proof bytes against the deterministic
   reference. Test zero denominators and product scan boundaries.
4. Give BLST and Rayon one CPU budget. Prefer BLST's serial backend with MSM
   chunks scheduled on the active Rayon pool, subject to measured cost and MSM
   differentials. Do not change global Rayon pool settings in library code.
5. Audit region replay. Keep changes that alter closure invocation semantics out
   of the default build; only implement an optional path if correctness and a
   measured benefit justify the added code.
6. Run release tests serially, verify exported proofs using registry 0.8.3, and
   record timings, limits and exact commands below. Commit verified milestones.

All fixture witnesses and setup seeds are public synthetic test data. This
benchmark uses an unsafe deterministic test SRS and is not a production setup.

## Measurements

Release build, `CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4`, k=16 (65,520 assigned
rows), three equality-enabled advice columns, one lookup with 256 distinct table
values and 128 distinct input values, rational advice assignments. Setup is
excluded. Every timed proof is verified after stopping the timer. This is a
shared host; small differences are not statistically established speedups.

| Stage | Proof times (ms) | Median (ms) | Fixed-RNG repeated bytes |
| --- | --- | --- | --- |
| Original helper | 1638.224, 1126.831, 1131.766 | 1131.766 | Different |
| BTreeMap reference (subsequently replaced) | 1048.809, 1054.197, 1056.362 | 1054.197 | Identical |
| Parallel inversions and product-buffer reuse | 1181.276, 1009.816, 1033.446 | 1033.446 | Identical to ordered reference |
| Compact rational denominators | 1191.529, 1545.809, 1522.530 | 1522.530 | Identical to ordered reference |

The original implementation fails `fixed_rng_lookup_proofs_are_reproducible`;
the ordered-map implementation passes at each of 1, 2 and 4 Rayon workers.
Reproducibility means a fixed RNG **and fixed worker count**: stock's vanishing
polynomial generation consumes a seed per worker. That behavior is preserved.

Command: `cargo run --offline --release --example cpu-bench -- 16 3 target/cpu-reference`
(with the environment above). The reference proof is 1,376 bytes; its BLAKE2b-512
digest is
`3f7c38e0fa0e9a56dcb4c98016240f570c3d2540e549cc1644cb27c4dc6e8713f265e2839e17a6d62e34c4a48adb6d56e5de37b99448549c4f90cd636e131f50`.

The separate `tests-stock/` crate pins registry `midnight-proofs =0.8.3`, with
no Loki path dependency or patch. It deserializes the exported VK and verifier
SRS, accepts the reference proof, and rejects a corrupted proof. Verified with
`LOKI_CPU_FIXTURE="$PWD/target/cpu-reference" cargo test --offline --release --manifest-path tests-stock/Cargo.toml -- --test-threads=1`.

Grand-product denominators and numerators now only compute rows consumed by the
product, batch-invert independent chunks, and replace factors in place with
exclusive prefix products. The same allocation holds the final blinded product.
The cross-chunk scan preserves zero factors and the initial value of each
permutation set. `cmp target/cpu-reference/proof.bin target/cpu-products/proof.bin`
passes, including after preserving the original sequential blinding RNG calls.

The first full default `cargo test --offline --release -- --test-threads=1`
exposed a pre-existing doctest error: `RowSizer` unconditionally declared the
feature-gated `Circuit::Params` associated type. Adding the matching cfg to the
documentation example fixes it without changing library behavior. The rerun
passes 30 unit, 2 integration and 9 documentation tests.

## Rational conversion

Only nontrivial denominators other than ONE are stored in a contiguous vector.
Zero and trivial assignments are emitted directly, without multiplication by
ONE. Inversion remains zero-preserving. The characterization test covers empty
columns, empty batches, zero numerators, zero/ONE/repeated denominators, and
mixed assignment variants at 1 and 4 threads.

The isolated paired benchmark alternates original and optimized conversion
order over nine samples. Cloning the three k=16 input columns is excluded from
timing; conversion, output allocation and input-buffer destruction are included,
while output destruction is excluded. This exposes the local benefit even when
end-to-end samples fluctuate with host load.

| Rational cells | Original median (ms) | Compact median (ms) | Local ratio |
| --- | --- | --- | --- |
| 10% | 9.403 | 5.148 | 1.83x |
| 100% | 26.451 | 19.195 | 1.38x |

Run with `CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --lib time_rational_conversion_k16 -- --ignored --nocapture --test-threads=1`.
The original end-to-end sample series above remains recorded rather than
presenting a slower shared-host sample as a demonstrated overall speedup.

## Final end-to-end comparison

Five saved release binaries ran sequentially under DoctorWatson's shared heavy
lock, with `RAYON_NUM_THREADS=4` and `taskset -c 0-3`. Affinity also bounds the
original BLST backend to four CPUs; BLST honors affinity when sizing its pool.
There is no simultaneous heavy job, but the CPUs are still shared with ordinary
builds/processes. Setup and verification are excluded from these proof times.

| Cumulative stage | Three proof times (ms) | Median (ms) |
| --- | --- | --- |
| Preparation reference: BTreeMap, parallel inversion/scan, reused product buffers | 1767.024, 1694.194, 1793.609 | 1767.024 |
| Compact rational conversion | 1819.149, 1726.731, 1996.430 | 1819.149 |
| Single Rayon CPU budget for BLST | 1726.811, 1681.138, 1725.724 | 1725.724 |
| Hash counts, sorted distinct leftovers | 1602.044, 1631.500, 1720.650 | 1631.500 |
| Parallel stable input sort (final) | 1884.567, 1781.395, 1737.972 | 1781.395 |

The final median is 0.8% slower than the preparation reference. These three
samples per stage do **not** establish an end-to-end speedup. The kernel results
show local improvements, while the overall shared-host measurement remains
noisy. No production recursion or k=20 performance claim is made.

The run passes in 39.989 s, peak RSS 226,025,472 bytes. All 15 proofs have the
same bytes as the ordered-map reference; `cmp` also confirms identical VK and
verifier-SRS exports for all five stages. Evidence:
`target/cpu-runs/qa/sherlock/traces/20261025T000809Z-w3-cpu-paired.log` and
`target/cpu-validation/paired.log`. Saved binary hashes and the local comparison
script are under `target/cpu-validation/`.

## One CPU budget

BLST is built with `no-threads`; BLS G1 MSM partitions scalar/base pairs into
serial BLST Pippenger calls scheduled on the active Rayon pool. Nested callers
share that pool. Applications can set `RAYON_NUM_THREADS` or install a custom
Rayon pool; the library does not change the global pool configuration.

An isolated Linux test counts process threads around an MSM and nested MSMs in
a two-worker pool. The original backend grows from 4 to 24 OS threads. The new
backend creates no additional workers. A separate arithmetic test compares
against individual group multiplications at 1/2/4 workers, including empty and
all-zero scalars, identity bases, ONE, minus ONE, random scalars and chunk edges.

**Integration caveat:** Cargo feature unification applies `blst/no-threads` to
every user of the same resolved BLST instance, including direct G2 MSMs and
aggregate operations outside this crate. Those operations remain correct but
may lose their own parallelism. Only `msm_specific` schedules BLST chunks on
Rayon. Workloads relying on other BLST parallel operations need separate timing.

## Lookup sorting and region closures

Counting retains HashMap's cheap hashing. Only distinct leftover entries are
sorted in field order before placement, producing exactly the BTreeMap reference
permutation. This avoids expensive field comparisons on every table row. Input
field sorting uses Rayon's stable `par_sort`, preserving the value sequence of
stock's stable sort. RNG consumption stays sequential.

A paired k=16 kernel benchmark (65,536 rows, 256 table values, nine alternating
rounds) found the following medians. The original variants have nine samples
each; sorted HashMap counts have 18 samples:

| Operation | Median (ms) |
| --- | --- |
| Original HashMap counts | 0.991 |
| BTreeMap counts | 5.665 |
| HashMap counts plus sorted distinct leftovers | 1.081 |
| Original stable input sort | 18.245 |
| Serial unstable input sort | 15.413 |
| Parallel unstable input sort | 15.152 |
| Parallel stable input sort (selected) | 6.411 |

Thus deterministic iteration costs about 0.090 ms in this isolated workload;
parallel stable sorting is 2.85x faster locally. Counts include allocation and
map destruction; sorting excludes input cloning and output destruction. Earlier
samples during a heavily loaded workspace sweep gave different sort rankings,
which is another reason not to extrapolate these measurements to recursion.
Run `cargo test --offline --release --lib time_lookup_preparation_k16 -- --ignored --nocapture --test-threads=1`
with `CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4`.

Region-closure replay is deliberately retained. `SimpleFloorPlanner` first
executes a region closure against `RegionShape` to discover columns/row count,
then chooses the region start and executes assignments. It accepts arbitrary
`FnMut` closures, including cross-region copies, instance queries, constants and
side effects; skipping the first invocation cannot generally preserve stock
semantics. `RegionShape` already skips the value-supplier closures. Moving
expensive witness calculations into those suppliers is the safe circuit-side
optimization, outside this prover-only package. No speculative floor-planner
replacement or `loki-fast` path is included.

## Final verification and reproduction

`CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release -- --test-threads=1`
passes 32 unit, 2 integration and 9 documentation tests. Two manual timing tests
are ignored in that suite and were run explicitly as recorded above.
`cargo test --offline --release --no-default-features --lib --test cpu -- --test-threads=1`
also passes: 32 unit tests and the byte-reproducibility integration test, with
the two manual timing tests ignored. Arithmetic differentials and the isolated
CPU-budget regression are included in both runs.

Independent code reviews found no actionable issues in preparation, sparse
conversion, pool sharing, or sorted-hash-map placement.
Rust 1.76 API availability was reviewed, but that toolchain was not installed
for an actual MSRV build.

Generate a fresh public fixture and verify it without linking Loki into the
verifier process:

```sh
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo run --offline --release --example cpu-bench -- 16 3 target/cpu-fixture
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --manifest-path tests-stock/Cargo.toml -- --test-threads=1
```

The actual final verification used
`LOKI_CPU_FIXTURE="$PWD/target/cpu-final-paired"` and passed acceptance plus
corruption rejection. The standalone lockfile resolves the verifier to registry
0.8.3, with no path patch. Logs are saved in `target/cpu-validation/`.
