# quint-q5: lookup and permutation preparation, W10 argument commitments

## Verdict

Main a256a9a, default build, gives the same outputs, the same random draws in the same order, and the same transcript bytes as stock 0.8.3. Inputs tested include:
- zeros, ONE, the max element −1, duplicates and all-equal inputs;
- zero denominators and lookup values missing from the table;
- every injected HashMap iteration order;
- 1, 2, 3, 4 and 7 Rayon workers, hitting both the sequential and the chunked code branches.

The W3 changes covered are `par_sort`, `batch_invert_parallel`, `prefix_product`, products over usable rows only, and `batch_invert_rational` skipping denominators equal to one.

For the same witness, W10 `cuda-argument-commit` (5e37ead) produces the same commitments and transcript bytes as main on every SRS the crate itself builds.

## Findings (none is a bug)

| # | Severity | Finding |
|---|---|---|
| F1 | Merge hazard (W10) | W10 head `src/plonk/lookup/prover.rs:431-432` still sorts leftover table values unconditionally. This is pre-P1 code from its 877114d base; main restored stock HashMap order (886a9fb; a256a9a lines 424-434). `git merge-tree a256a9a 5e37ead` auto-merges this file to the P1 version plus W10's `commit_argument` call. But the merge conflicts in Cargo.toml, domain.rs, kzg/msm.rs and profile.rs. If whoever resolves it takes W10's side of this file, the default build silently stops matching stock. **Repro:** `quint/q5/run-w10.sh`. W10 head vs main default is equal on 0/60 multi-leftover proofs; W10 head vs main with `loki-deterministic-lookup-order` is equal on 64/64. |
| F2 | Low (W10 precondition) | `commit_argument` (5e37ead `src/poly/commitment.rs:128-151`) equals `commit_lagrange` only if `g_lagrange` is the inverse DFT of `g`. `unsafe_setup`, `downsize` (`kzg/params.rs:40-49`) and `from_parts(None)` (102-118) guarantee this. `read_custom` (154-230) and `from_parts(Some)` accept any `g_lagrange` unchecked. **Repro:** `replay/tests/end_to_end.rs::w10_commit_argument_equals_main_commit` with `Srs::BadLagrange` diverges on 151 of 396 commitments, and all 16 full proofs differ. The stock verifier rejects both sides, so this is a byte difference on an invalid SRS, not a soundness change. |
| F3 | Informational | `batch_invert_rational` (a256a9a `src/poly/mod.rs:290-317`) branches on a witness value (`denominator != ONE`). Stock is already not constant-time here, so this is not a new class of leak; I did not measure timing. |
| F4 | Known (stock) | Stock itself is nondeterministic once a lookup has two or more distinct leftover values: two stock runs with the same seed differ on 12/12 such fixtures, because of std `RandomState`. Byte equality can only be required on order-independent fixtures, or under one injected order (done here for every order). |
| F5 | Observation (stock behaviour, preserved) | An SRS larger than 2^k that is not `downsize`d gives proofs the stock verifier rejects: 0/16 accepted for stock, main and every W10 variant. W10 keeps the Lagrange path there, so it matches. |

## Properties

| Property | Status | Evidence |
|---|---|---|
| a′ and s′ are permutations of input and table; a′₀ = s′₀; a′ᵢ = s′ᵢ or a′ᵢ = a′ᵢ₋₁ | Holds | Checked on 10,200 real-code samples; model invariant `LookupConditions`; MBT replay |
| Blinding rows untouched | Holds | Exact checks that blinding rows hold the next draws: bf+1 for A′ before bf+1 for S′, bf for each z; model invariant `BlindingRowsUntouched`; MBT draw tags |
| Products telescope to 1 over usable rows | Holds unless a factor is zero; then z_u = 0 in both stock and Loki | 3,500 lookup and 6,545 permutation cases that telescope; model invariants `LookupTelescopes`, `PermTelescopes` |
| Same random draws in the same order as stock | Holds | RNG position after every call; a digest of every draw in every real proof (stock vs Loki, W10 vs main); model invariant `SameDraws` |
| Default-build leftover placement equals stock | Holds for every iteration order | Injected sorted, permuted, seeded and fixed-SipHash orders; byte-equal real proofs on order-independent fixtures |
| W3 changes equal stock for all inputs | Holds | Differential tests and model invariant `HelpersEqualSequential`. Why: batch inversion maps each element to its inverse, zero to zero, whatever the chunking; the prefix product is associative; Fq's `Eq` and `Ord` both compare limbs, so stable `par_sort` and `sort` give the same result |
| W10 commitments and transcript bytes equal main | Holds on every in-crate SRS; F2 is the only exception | W10 comparison matrix; Quint simulation; Apalache proof at k=1 |

## Evidence

**Differential replay of the real source** (`replay/`). `build.rs` copies the exact function text from the stock registry crate, `git show a256a9a` and `git show 5e37ead`, and compiles it against minimal stand-in types. The text edits all must apply exactly once: the lookup HashMap becomes a map whose iteration order the test sets, and feature cfgs are resolved per variant.

Sample counts:
- `batch_invert_parallel` and `prefix_product`: 10,880
- `batch_invert_rational`: 10,590
- `permute_expression_pair`: 10,200 (600 error cases, 6,499 order-sensitive)
- lookup z: 10,500 (7,000 with a zero denominator)
- permutation z: 10,500

**End-to-end proofs.** The fixture circuit has:
- three lookups, one with two columns;
- a table that includes −1;
- seven equality columns;
- rational advice with denominators ONE, ZERO and random.

At k=7 and k=11, in both the default and `det` builds, VKs and full draw logs match stock on all 16 configurations. The stock verifier accepts every Loki proof, and bytes match on the order-independent fixture.

**W10, CPU only.** `w10/setup.sh` builds two copies, neither of which enables `cuda` or needs nvcc: W10 head with only the feature's cuda dependency removed, and main plus W10's three auto-merged argument files. That second copy is not the final conflict-resolved merge.

| Comparison | Equal / total |
|---|---|
| W10 head, feature off vs on (exact, oversized-not-downsized, downsized, serialized SRS) | 64/64 |
| W10 head with feature vs main with `loki-deterministic-lookup-order` | 64/64 |
| Main + W10 argument files, deterministic order, vs main | 64/64 |
| Main + W10 argument files, default order, vs main (order-independent fixtures) | 16/16 with the feature, 20/20 without |
| Stock verifier verdicts | identical to main everywhere |

The real `commit_argument` also matches main's commit path directly on all 1,584 valid-SRS samples (k = 1 to 9). Main's W13 GPU base registry matches bases by exact coordinates, so it cannot confuse `g` with `g_lagrange` after the merge.

**Quint models** (seed 0x51515, 39/39 checks pass, in `results.json`):
- `lookup.qnt` runs stock and Loki side by side over GF(17): permutation of the lookup columns, lookup product, permutation product, rational inversion, and an RNG draw counter. HashMap order is a nondeterministic choice. Its `Safety` invariant holds over 10,000 samples.
- 11 reachability witnesses are each shown reachable.
- 11 mutants each break the invariant they target; every invariant is broken by at least one mutant.
- `argcommit.qnt` models W10's strategy: `Safety` holds over 10,000 samples, 4 witnesses are reachable, 3 mutants are caught.
- `argcommit_bmc.qnt` under Apalache proves equality for every τ and every value at k=1 with SRS size 2 or 4; the mutant without the `max_k == k` guard gives a counterexample.

**MBT replay** (`replay/tests/mbt.rs`, quint-connect 4f018f5): 2,000 model traces replayed against the real code with all states equal. The same driver is rejected on traces from mutants mut1, mut2 and mut9, so the replay can fail.

## Limits

- W10 was only run as a CPU build. The GPU multi-scalar multiplication over `g` was not executed; W10's own GPU gates cover that.
- The extracted functions run against stand-in types. The surrounding prover is covered by the end-to-end proofs, which byte-compare only where stock itself is deterministic.
- Real-code coverage reaches k=12 for helpers and products and k=11 for full proofs.
- Model bounds: at most 5 usable rows, 2 blinding factors, 6 values, n ∈ {4, 8}, 4 columns, 3 workers. argcommit uses k ≤ 2, and the Apalache proof is k=1 only.
- Apalache cannot check `lookup.qnt`, because its list lengths depend on the data; that model relies on simulation plus MBT.
- I did not ask anyone to confirm the model parameters, as the kit's verifier procedure says to; I chose them myself.

## Coordination

I sent lean-w2 (CpuKernels.lean and ArgumentCommit.lean) the evidence for its SRS-validity hypothesis, plus corrected line numbers: the first message had some slightly off.


**Model to source map** (a256a9a unless marked stock or W10):

| Model | Rust |
|---|---|
| `permuteStock` | stock `lookup/prover.rs:379-461` |
| `permuteLoki` | `lookup/prover.rs:374-466` (par_sort 391, leftovers 424-439, draws 442-443) |
| `lookupProductStock` | stock `lookup/prover.rs:155-295` |
| `lookupProductLoki` | `lookup/prover.rs:155-290` |
| `batchInvertChunked` | `utils/arithmetic.rs:172-180`, with `parallelize` 120-169 |
| `prefixChunked` | `utils/arithmetic.rs:184-221` |
| `permZ` (stock) | stock `permutation/prover.rs:36-~170` |
| `permZ` (Loki) | `permutation/prover.rs:35-158` |
| `rationalStock` | stock `poly/mod.rs:287-328` |
| `rationalLoki` | `poly/mod.rs:290-317` |
| `commitArgW10` | W10 `commitment.rs:128-151` |
| `commitLagrange` / `commitCoeff` | `kzg/mod.rs:86-95` / `77-84` |
| `downsize` | `kzg/params.rs:40-49` |
| W10 write order | W10 `lookup/prover.rs:137,140`; `permutation/prover.rs:156` |

**Mutant to invariant it breaks:**

| Mutant | Invariant broken |
|---|---|
| mut1, mut3, mut4 | LookupEqualsStock |
| mut2 | BlindingRowsUntouched, SameDraws |
| mut3, mut4 | HelpersEqualSequential |
| mut5 | RationalEqualsStock |
| mut6, mut9 | LookupConditions |
| mut7 | LookupTelescopes, PermEqualsStock |
| mut8 | BlindingRowsUntouched, LookupTelescopes |
| mut10 | ErrorAgrees |
| mut11 | PermTelescopes, PermEqualsStock |
| acmut1, acmut2 | W10EqualsStockOnValidSrs |
| acmut3 | TranscriptEqual |
| bmc_mut1 | Equal (Apalache counterexample) |

**Reproduce** (from `quint/q5`):
```
python3 -I check.py --samples 10000
cd replay && RAYON_NUM_THREADS=4 CARGO_BUILD_JOBS=4 cargo test --offline --release -- --nocapture --test-threads=1
cd replay && RAYON_NUM_THREADS=4 CARGO_BUILD_JOBS=4 cargo test --offline --release --features det --test end_to_end -- --nocapture
./run-w10.sh
```
