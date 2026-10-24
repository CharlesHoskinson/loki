# CPU work (W3)

## Design and implementation plan

Preserve the 0.8.3 circuit, keys, transcript ordering, RNG consumption and proof
encoding. Deterministic table placement selects one valid lookup permutation;
stock previously selected a different valid permutation on each process run.

1. Add a public, synthetic lookup/copy circuit and a k=16 timing example. Export
   VK, verifier parameters and proof for a separate registry-only verifier.
   Establish the baseline and demonstrate the fixed-RNG reproducibility failure.
2. Use an ordered lookup count map. Compare the cost with the baseline and test
   repeated proofs, including across Rayon pool sizes.
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
| Ordered table counts | 1048.809, 1054.197, 1056.362 | 1054.197 | Identical |
| Parallel inversions and product-buffer reuse | 1181.276, 1009.816, 1033.446 | 1033.446 | Identical to ordered reference |

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
