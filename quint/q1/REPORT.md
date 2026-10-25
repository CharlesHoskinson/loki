# quint-q1: transcript order, Loki vs stock 0.8.3

**Loki at a256a9a does not change transcript order compared with stock midnight-proofs 0.8.3, on any of the six builds I checked.** The builds:
- default;
- `loki-multiopen`;
- `truncated-challenges`;
- `loki-multiopen` + `truncated-challenges`;
- `loki-deterministic-lookup-order`;
- `loki-backend-api` (CPU adapter).

The only place where prover and verifier disagree is a defect already in stock 0.8.3 (mixed committed/plain instances with several proofs). I reproduced it against the real stock verifier. I also found one new stock bug, low severity.

Commits on `loki/quint-v2`: `ac6153b` (all the work) and `229cac9` (`cargo fmt` of the Rust crate). I posted DONE to the coordinator and #lokiformal, plus the BUG notes as I found them.

**quint/q1/REPORT.md was not written.** The subagent harness blocks report files, and its rule outranks the brief, so I didn't work around it. This handback is the report; you can commit it as `quint/q1/REPORT.md` as is.

## What I found

The verifier side of Loki is byte-for-byte stock:
- `plonk/verifier.rs` and the four sub-verifiers;
- `transcript/`;
- `construct_intermediate_sets`.

On the prover side, Loki only adds:
- profiling guards;
- value-equivalent kernels (`batch_invert_parallel`, `prefix_product`, `par_sort`);
- the two feature-gated paths (`loki-multiopen`, `loki-deterministic-lookup-order`).

`construct_prover_sets` in `kzg/utils.rs:134-245` is the same as `construct_intermediate_sets` except for where the evaluations come from. I diffed them to confirm.

| # | Finding | Severity | Where (a256a9a) | Repro |
|---|---|---|---|---|
| F1 | **Stock defect, re-confirmed.** The prover hashes instances proof by proof (committed, then plain). The verifier hashes every proof's committed instances first, then all plain ones. With more than one proof and both kinds of column, every challenge from theta onward differs, and a valid batch is rejected. | Medium. Valid batches are rejected; no soundness impact. Present in stock. | `prover.rs:467-500` (stock `:410-443`) vs `verifier.rs:63-76` | Quint: `defect_counterexample` fails `safety`, as expected. `defectExact` passes 10,000 samples. Test: `mixedOrderTest`. Rust: 45 real cases over 6 builds, all rejected by stock. The first diverging challenge matched the model every time. |
| F2 | **New bug: shared commitment slices reject a valid batch.** If a caller passes the same committed-instance slice for two or more proofs, the verifier sees one commitment queried twice at x and returns `Error::DuplicatedQuery`. The verifier compares commitments by pointer. The same proof verifies when each proof gets its own `Vec`. | Low. API/liveness only; the prover is unaffected. Present in stock. | `verifier.rs:341-405`, `poly/query.rs:114-122` (`ptr::eq`), `kzg/utils.rs:37-63` | Quint: `initAlias` with `aliasClassified` passes 10,000 samples. Test: `aliasTest`. Rust: the replay checks that shared slices fail to prepare and separate slices are accepted. |
| F3 | **Quint 0.32.0 tooling bug.** A user higher-order `pure def` called inside its own lambda argument is evaluated wrongly, with no error, on both backends. | Tooling. Models can be silently wrong. | Quint 0.32.0 | `quint test quint/q1/quint_capture_repro.qnt`: `sameTest` and `sameConstTest` fail, `distinctTest` passes. The existing `quint/*.qnt` models don't use this pattern. My model uses a separate copy per nesting level. |
| F4 | **Stale doc.** It says "Loki already sorts lookup leftovers". P1 (`886a9fb`) restored stock HashMap order; sorting is now opt-in. | Docs | `LOKI-MULTIOPEN.md:109` | Rust replay (see the P1 section). |
| F5 | **Weak old model.** It builds its log from its own plan, so it can hardly fail. It hard-codes the point-set count (4 or 1) and has no verifier schedule beyond the instance audit. It now carries a pointer to the new model. | Model quality | `quint/transcript.qnt` | — |

No transcript bug introduced by Loki was found.

## The model and where each part comes from
`quint/q1/transcript_q1.qnt` builds the prover schedule and the verifier schedule separately, each from its own Rust code. The multiopen point sets are computed from each side's query list: run the `construct_intermediate_sets` algorithm, then sort by (size, index). They are not hard-coded. Every challenge also carries a tag (plain, truncated powers, or truncated scalar) so that `truncated-challenges` handling is checked.

How the state machine runs:
- the prover may finish computations out of order within a phase, but publishes in order;
- the verifier takes public inputs from its own arguments and reads everything else from the proof;
- challenge k agrees only if both sides hashed exactly the same prefix.

| Model part | Loki a256a9a | Stock 0.8.3 |
|---|---|---|
| VK hashed first | `prover.rs:99`; `mod.rs:288-295` | `:99` |
| Instances, prover order (proof by proof) | `prover.rs:467-500` (`:482`, `:487`, `:493`) | `:410-443` |
| Instances, verifier order (all committed, then plain) | `verifier.rs:63-76` | same |
| Advice per phase, then that phase's challenges | `prover.rs:558-660` (`:644`, `:653`); `verifier.rs:85-100` | `:501-588` |
| theta, then lookup permuted commitments | `prover.rs:108`, `:114-138`; `lookup/prover.rs:132`, `:135` | `:108-135` |
| beta, gamma, permutation products, then lookup products | `prover.rs:144-187`; `permutation/prover.rs:151`; `lookup/prover.rs:282` | `:138-171` |
| Trash challenge and trash commitments | `prover.rs:196-220`; `trash/prover.rs:63` | `:175-200` |
| Vanishing random, y, h pieces, x | `prover.rs:226`, `:229`, `:315`, `:325`; `vanishing/prover.rs:79`, `:138` | `:202-282` |
| Evaluations: instance → advice → fixed → random → sigma → permutation → lookup ×5 → trash | `prover.rs:327-366`, `:732-792`; sub-provers | `:284-323` |
| Verifier evaluations | `verifier.rs:240-320`; sub-verifiers | same |
| Query lists | `prover.rs:794-851`; `verifier.rs:341-405` | `prover.rs:720-777` |
| Point sets and the (size, index) sort | `kzg/utils.rs:37-133`; `utils.rs:134-245` (`loki-multiopen`) | `utils.rs:37-133` |
| Prover multiopen: x1, x2, f, x3, q evaluations, x4, pi | `kzg/mod.rs:111-216` (sort `:150`); `multiopen.rs:102-148` (sort `:127`) | `kzg/mod.rs:105-199` |
| Truncation: x1 truncated powers, x2 plain, x3 truncated, x4 truncated powers | prover `mod.rs:131`, `:171`, `:181`, `:195` and `multiopen.rs:115`, `:147`; verifier `mod.rs:258`, `:314`, `:296`, `:334`, `:347` | `:120`, `:160`, `:168`, `:182` |
| Verifier multiopen (`multi_prepare`) | `kzg/mod.rs:219-345` | `:202-328` |
| Lookup leftover order (`lookup_leftover.qnt`) | `lookup/prover.rs:374-465` (default `:425-426`, opt-in sort `:429-434`) | `:379-460` |

## Properties checked
| Invariant | What it says |
|---|---|
| `challengesAgree` | Prover and verifier challenge k come from the same hashed prefix and the same truncation tag. |
| `readsAgree` | The verifier reads every proof item as the item the prover meant. |
| `fsDependencies` | Items computed from a challenge are hashed after it. Challenges come after all their inputs. Checked against a separate table, not derived from either side. |
| `challengeSchedule` | Each challenge is drawn once, in the right order. |
| `completesCleanly` | The verifier consumes the whole proof without error. |
| `plansEqual` | Outside F1 and F2, the two separately built schedules are identical. |
| `defectExact` | The sides agree exactly when it is not the case that there is more than one proof with both committed and plain columns. |
| `aliasClassified` | A shared slice errors exactly when there is more than one proof and a committed column. |
| `leftoverSafety`, `deterministic` | The permuted table is a permutation of the table, it meets the lookup's adjacency rule, and nothing panics. With the opt-in feature, the output depends only on input and table. |

What the simulations sampled:
- 1–3 proofs;
- 0–2 committed and 0–2 plain instance columns, with 0–2 values each;
- advice in 1–3 phases, with 0–1 challenges after each of the first two phases;
- optionally a "next row" query;
- 0–2 lookups;
- 0–5 equality columns (0–2 permutation chunks);
- 0–1 trash arguments;
- both settings of `multiopen` and `truncated`.

## Quint campaigns
Seed 20261024, Rust backend, 4 threads. Results are in `quint/q1/results.json`: 125 runs, none unexpected. Logs are in `.artifacts/q1/`.

**Main and alias runs**
- **Main simulation:** 10,000 samples, 700 steps, no violation. How often each scenario was reached, out of 10,000:

  | Scenario | Count |
  |---|---|
  | complete | 10,000 |
  | F1 defect | 2,998 |
  | 4 or more point sets | 2,235 |
  | permutation chain | 3,341 |
  | later-phase challenge | 1,218 |
  | truncated + multiopen + 2 lookups | 834 |
  | out-of-order computation | 9,998 |
  | trash with 3 proofs | 1,642 |
  | verifier running ahead | 6,289 |
- **Shared-slice (alias) run:** 10,000 samples, no violation.
- **F1 counterexample:** fails `safety`, as intended.
- **Scenario tests:** 7/7 pass. They include a hand-derived point-set order and the truncation tags.

**Transcript mutations: 12/12 caught** (in at most 2,000 samples each). Which invariant catches each one (300-sample runs):

| Mutation | Caught by |
|---|---|
| `swapAdvice`, `swapProducts`, `lookupMajor`, `lokiUnsortedSets` | `readsAgree`, `plansEqual`, `defectExact` |
| `dropPhaseChallenge`, `lokiTruncX2` (multiopen path truncates x2) | `challengesAgree`, `plansEqual`, `defectExact` |
| `dropBeta` | `challengesAgree`, `challengeSchedule`, `plansEqual`, `defectExact` |
| `x3BeforeF` | `challengesAgree`, `fsDependencies`, `plansEqual`, `defectExact` |
| `x4AfterPi` | `challengesAgree`, `fsDependencies`, `completesCleanly`, `plansEqual`, `defectExact` |
| `emitOnCompletion` (publish in completion order) | `challengesAgree`, `readsAgree`, `plansEqual`, `defectExact` |
| **`x3BeforeFBoth`** (same mistake on both sides) | **only `fsDependencies`** |
| **`verifierInstanceOrder`** (prover uses the verifier's order) | **only `defectExact`** |

**Lookup leftover model**
- Sizes 4, 5 and 6 rows: 10,000 samples each, all pass.
- The stock HashMap order really changes the permuted table in 1,041 / 1,806 / 2,355 of those samples.
- A valid alternative fill order (`popFront`) passes, as it should.
- Mutations `skipLastKey`, `fillFirstRows`, `noFirstConsume` and `detIgnored` are all caught at every size.

**Apalache, exhaustive at 3 rows**
- `leftoverSafety` holds (211 s).
- `deterministic` holds (70 s).
- The `skipLastKey` mutation is found (20 s).

## Rust replay against real code (`quint/q1/mbt`)
**Setup**
- The crate is standalone. `cargo tree` shows two separate `midnight-proofs 0.8.3` packages: Loki (path) and stock (registry).
- One macro expands the same circuit, logging transcript, prover and verifier against both crates.
- The circuit fixes its minimum degree at 5, and the test asserts every shape fact the model relies on.
- `gen_itf.sh` writes 132 compact traces (120 random, 12 alias).

**What each trace must satisfy**
- The Loki and stock provers produce the model's prover operation sequence, and their VKs are byte-equal.
- Each labelled instance item has exactly the bytes its label names, on both provers and the stock verifier.
- The stock verifier's operation sequence matches the model, and it reads exactly the bytes the prover wrote.
- Real challenge k agrees exactly when the model's does: in the final state, and at every model step.
- Stock-on-Loki, stock-on-stock and Loki-on-Loki acceptance all equal the model's verdict.
- For F1 cases, the first diverging challenge matches the model.
- Proofs without lookups are byte-equal between Loki and stock.

**Negative control:** a deliberately injected transcript fault must be rejected. The fault re-hashes the first write, a middle write, or the last write before x4. Re-hashing `pi` cannot be detected, because no challenge follows it.

| Build | Traces | F1 | F2 | Lookup cases | Loki = stock bytes | Lookup proofs reproducible (Loki / stock) | Faults caught |
|---|---|---|---|---|---|---|---|
| default | 40 | 8 | 2 | 15 | 25 (all without lookups) | 0 / 0 | 12/12 |
| `loki-multiopen` | 31 | 10 | 0 | 21 | 10 | — | 12/12 |
| `truncated-challenges` (both crates) | 30 | 6 | 0 | 18 | 12 | — | 12/12 |
| multiopen + truncated | 31 | 5 | 0 | 24 | 7 | — | 12/12 |
| `loki-deterministic-lookup-order` | 40 | 8 | 2 | 15 | 25 | **15 / 0** | 12/12 |
| `loki-backend-api` (CPU adapter) | 40 | 8 | 2 | 15 | 25 | — | 12/12 |

To rerun:
```sh
python3 quint/q1/check_q1.py --matrix
./quint/q1/gen_itf.sh
cd quint/q1/mbt && export CARGO_TARGET_DIR=../../../.artifacts/q1/target RAYON_NUM_THREADS=2
for f in "" loki-multiopen truncated loki-multiopen,truncated det-lookup backend-api; do cargo test --offline --features "$f" -- --nocapture --test-threads=1; done
```

## P1: stock leftover order vs `loki-deterministic-lookup-order`
- **No transcript effect.** Either way the prover writes the same two commitments; the order only decides which repeated rows get which leftover values.
- **Model.** Every HashMap order gives a valid lookup permutation. With the opt-in feature, the result is deterministic.
- **Rust.** On the default build, Loki behaves like stock: 0 of 15 lookup proofs reproduce with the same seed. With the feature, 15 of 15 reproduce, and stock accepts all of them.
- **Lean.** This matches lean-lead's `lokiLookupOrd` and `lokiDeterministic_eq_sorted_stock`; I did not re-model it in Lean.

## Limits
- **Not run:** `cuda`, `cuda-ntt`, `loki-gpu-backend` and `cuda-srs-registry`. They need a GPU at runtime, the brief forbids `gpu.lock`, and the coordinator's GPU hold is in force. Their transcript calls are the same `prover.rs` lines, but that is a source argument, not a test. Unmerged W10 and W12 are also not covered.
- **Order, not algebra.** The model covers order and Fiat–Shamir structure only. Value-level equivalence is Lean's job.
- **Mutations are model-only.** I didn't mutate production code. In Rust, a reordered prover shows up only as stock rejecting the proof, which the fault-injection test demonstrates.
- **Small fixtures.** k = 6, at most 3 proofs, at most 2 lookups, at most 2 permutation chunks. Rotations used are cur, next, prev and last. Coinciding rotation points are not exercised.
- **One combination never sampled:** a phase-0 challenge with no later-phase advice. The model's config filter requires later-phase advice for any phase-0 challenge. The squeeze position would be the same.
- **Apalache.** It can't check the main state machine, because its loop bounds depend on the configuration ("Expected a constant integer expression"). Apalache covers only the leftover model, exhaustively at 3 rows.
- **Sampling, not exhaustive.** The main model is checked by simulation, not exhaustively.
- **Cosmetic:** `results.json` stores absolute command paths.
- **Shared Apalache server:** it runs on port 8822 and wrote another agent's argcommit counterexample into `quint/q1/_apalache-out/`. I did not commit it.
