# W8: executable state contracts and Rust replay

Branch: `loki/w8-quint`. Baseline: stock `midnight-proofs` 0.8.3 at `bd1f0cc`.
Quint: 0.32.0. Rust: 1.99.1. Simulation seed: `20261024`, Rust backend,
four simulator threads. Rust builds used `CARGO_BUILD_JOBS=4`; proof tests used
`RAYON_NUM_THREADS=2` and ran serially.

**Result:** all four models passed 10,000-sample safety campaigns and their
reachability witnesses. The isolated Rust suite passed 15 tests, including real
transcript replay, W4 host-cache replay and W6 PK-cache replay. This establishes
bounded sampled safety and the listed implementation checks. It does not prove
GPU arithmetic, scheduler liveness, or universal fixed-RNG proof-byte equality.

A separate stock prover/verifier order audit deliberately fails for mixed
committed/plain columns with multiple proofs; the real Rust verifier rejects
those cases. That stock defect is reproduced below, rather than hidden inside
the passing safety campaigns.

No production source, root Cargo feature, manifest or lockfile changed.

## Models and source correspondence

| Model | Source / state boundary | Safety properties |
| --- | --- | --- |
| `quint/transcript.qnt` | `src/plonk/prover.rs`: `compute_instances`, `parse_advices`, `compute_trace`, `finalise_proof`; lookup/permutation/trash/vanishing prover modules; KZG `multi_open` | Ordered stock prefix; absorbed sequence; each squeeze binds its preceding absorptions |
| `quint/gpu_dispatch.qnt` | Threshold routing, two provers, pointer reuse, immutable selected resident contents, GPU errors and CPU fallback | Result equals abstract CPU oracle; selected `(ptr,len,content)` matches request; CPU completion is total for fallback inputs |
| `quint/pk_cache.qnt` | Full `(circuit hash,k,params hash)` identity; fill ownership, build, publication and use | Used key matches requested binding; real build count at most one; no use before publication |
| `quint/scheduler.qnt` | LOKI-DESIGN L5 proposal; four jobs, two classes, admission, progress, checkpoint and resume | Budget and slot limits; one admission/completion per job; checkpoint progress preserved |

The transcript schedule includes common VK/instance absorptions, advice-phase
challenges, theta, permuted lookup commitments, beta/gamma, all permutation
products followed by all lookup products, trash challenge/commitments, the
vanishing random commitment, y, quotient commitments, x, evaluation groups, and
**all four** KZG multiopen challenges. `multiopen_x1/x2` follow the main
evaluations; f precedes x3; q evaluations precede x4; pi is last. Computation may
finish out of order within a phase; transcript publication remains ordered.

The model separates evaluation groups and per-proof query indexes. The fixtures
cover four circuit families: plain, lookup/permutation, second-phase advice, and
additive-selector trash. One/two proofs and committed/plain instances are varied.
The type supports three advice phases, but the concrete Rust fixtures exercise
the first two. The original random fixtures have one instance column. The additional
`transcript_instances` module in the same file models multiple committed/plain
columns and **both** stock loops. A separate Rust two-column fixture confirms
the mismatch. Arbitrary query rotations and arbitrary circuit shapes still need
additional fixtures.
The model does not establish equality of every commitment payload by itself;
the real stock-verifier comparison supplies that check for these fixtures.

## Reproduce the checks

Run from this worktree:

```sh
python3 quint/check.py

CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=2 cargo test --locked \
  --manifest-path quint/mbt/Cargo.toml \
  --features w4-source,w6-source -- --nocapture --test-threads=1

CARGO_BUILD_JOBS=4 cargo check --locked
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=2 cargo test --locked \
  --test plonk_api -- --test-threads=1

CARGO_BUILD_JOBS=4 cargo check --locked --manifest-path quint/mbt/Cargo.toml \
  --all-targets --features w4-source,w6-source
cargo fmt --manifest-path quint/mbt/Cargo.toml -- --check
```

`check.py` runs, for each model:

```sh
quint typecheck quint/MODEL.qnt
quint test quint/MODEL_test.qnt --match '.*Test$' --max-samples 1
quint run quint/MODEL.qnt --backend rust --n-threads 4 --seed 20261024 \
  --max-samples 10000 --max-steps STEPS --invariant safety \
  --witnesses WITNESS_NAMES --verbosity 1
```

Steps are 180 for transcript, 120 for dispatch/cache, and 1,000 for scheduler.
The `safety` conjunction checks every named safety invariant together. Test
selection deliberately ends in `Test$`: Quint's `--match '.*'` also selects
imported non-test definitions. All 27 main scenario tests ran: 5 transcript,
8 dispatch, 7 cache, 7 scheduler. Six additional instance-order audit tests also
ran, including a finite exhaustive check of all 81 layouts in the audit bounds. Machine-readable commands, results and counts
are in `quint/results.json`; local full logs are under `.artifacts/w8/`.

## Nonvacuity and fault controls

All witnesses were reached in the 10,000-sample campaigns:

| Model | Witness counts |
| --- | --- |
| Transcript | complete 10,000; simultaneous pending commitments 9,674; theta 10,000; multiopen x4 10,000; advice-phase challenge 4,991 |
| GPU dispatch | completed 10,000; completed after error 9,999; cache hit 3,526; two GPU requests 2,367; stale cache rejected and request completed 4,400 |
| PK cache | used 10,000; published 10,000; concurrent same-key waiters 9,761; different keys used 10,000; same key used by multiple jobs 9,553 |
| Scheduler | all jobs complete 10,000; overlap 10,000; checkpoint 9,997; resume 9,997; some completion 10,000 |

The first scheduler campaign also used 10,000 samples at 200 steps: all jobs
completed in all 10,000 traces; overlap occurred in 9,998. The 1,000-step campaign
extends the requested random simulation fallback. Terminal states explicitly
stutter, so extra terminal steps do not establish fairness.

`quint/mutations.qnt` contains deliberately unsafe alternatives. All five were
detected as invariant violations with seed `20261024`, at most 1,000 samples,
120 steps and four threads:

```sh
quint run quint/mutations.qnt --main mutation_NAME --step mutantStep \
  --invariant safety --max-samples 1000 --max-steps 120 \
  --seed 20261024 --n-threads 4 --verbosity 1
```

Names: `transcript` (early squeeze), `gpu` (content validation bypass), `pk_build`
(duplicate keygen), `pk_partial` (premature consumption), `scheduler` (over-admit).
These are expected failures, not passing safety runs. The individual negative
scenario tests also reject early publication, changed key components, duplicate
completion and admission of two heavy jobs.

## Stock prover/verifier instance-order counterexample

The coordinator supplied the Lean `stockTrace`/`verifierTrace` finding from
`LokiLean/Transcript.lean`. Source inspection and real Rust tests confirm it:
`compute_instances` processes committed then plain columns **within each proof**;
`parse_trace` absorbs committed columns **across all proofs first**, followed by
all plain lengths/values. For two proofs with one column of each kind:

```text
prover:   VK, C0, length(P0), P0, C1, length(P1), P1
verifier: VK, C0, C1, length(P0), P0, length(P1), P1
```

Run the complete audit with `python3 quint/check_instances.py` (also invoked by
`check.py` for transcript runs). Its checks are:

```sh
quint test quint/transcript_instances_test.qnt --match '.*Test$' --max-samples 1
quint run quint/transcript.qnt --main transcript_instances \
  --invariant mismatchExactlyMixedBatch --max-samples 10000 --max-steps 1 \
  --seed 20261024 --n-threads 4 \
  --witnesses allPlain allCommitted singleMixed multiMixed --verbosity 1
quint run quint/transcript.qnt --main transcript_instances \
  --init mixedCounterexample --invariant proverEqualsVerifier \
  --max-samples 10000 --max-steps 1 \
  --seed 20261024 --n-threads 4 \
  --out-itf .artifacts/w8/instance-order-counterexample.itf.json --verbosity 1
```

The classification invariant passes 10,000 samples and the equality invariant
returns **exit 1, expected**. The mismatch is exactly
`proofs > 1 && committedColumns > 0 && plainColumns > 0` within the checked symbolic layout
bounds, including empty plain columns (their length is still absorbed). Six
deterministic tests check controls and all 81 combinations of proofs 1..3,
committed/plain columns 0..2, and values per plain column 0..2. The checked ITF
counterexample is retained at `quint/instance-order-counterexample.itf.json` and
contains only symbolic public event identities. Use `check_instances.py
--update-counterexample` to refresh that fixture explicitly; ordinary checks write
new traces only under `.artifacts/w8`. Configuration-coverage counts,
not execution-progress witnesses, are recorded separately in `quint/results.json`.

`diagnose_stock_mixed_instance_order` checks the real stock Rust matrix of 1..3
proofs and committed prefix 0..2 for a two-column circuit. Seven controls are
accepted. The two mixed multi-proof cases differ at absorption index 2 and are
rejected by the unchanged stock verifier. The test passes by observing this
known defect; it does not fix it. Grouping all commitments globally in the
prover would change stock transcript/proof bytes, so no production fix is made.

## Rust model-based tests

The public `quint-connect` crate at commit
`4f018f54fc7dd4cef341d10111427bab59d3b307` built successfully. The initial
read-only registry download failure was resolved by the authorized host
`cargo fetch` runner. No fallback ITF parser was needed: quint-connect itself
generates `quint run --mbt --out-itf` traces and checks every modeled state field.
See `quint/mbt/README.md` for pinned-source requirements.

- **Transcript:** 24 generated traces, **1,022 transcript operations**, random
  small circuit families, witnesses, batch sizes, and committed-instance modes.
  Both wrapped and plain proofs passed the unchanged stock verifier. Wrapped
  prover operations and absorption bytes matched stock-verifier operations and
  bytes. Nine no-lookup traces also passed the fixed-RNG proof-byte equality
  assertion. Fifteen lookup-rich traces reported unequal bytes; see CRITICAL below.
  Four additional family diagnostics exercised every circuit family. Payloads
  remain in memory, and equality errors print operation indexes only. The
  `compute` transition annotates model readiness; it is not GPU scheduling telemetry.
- **W4 dispatch/resident cache:** 24 traces replayed the actual host Rust source
  from `fdef4c4d70b6a5ea040a8bc94638a57ecc1eb3b2`. CPU-backed FFI exercises real
  threshold dispatch, exact content validation, uploads, cache replacement,
  injected prepare/invocation errors and the circuit breaker. Output points are
  compared to an independent group sum; fallback/below-threshold completion
  executes the stock native `msm_specific`. Completion publishes a captured
  synchronous result. Eight concurrent host callers also matched CPU output;
  original W4 conversion tests passed. **No GPU kernel ran.** The model has one
  abstract cache entry; W4 has two LRU entries. The adapter projects the most
  recently selected key and permits W4's permanent-disable fallback refinement.
  Error transitions probe failure handling before exposing a result, rather than
  reproducing an asynchronous CUDA engine. The modeled/test dispatch threshold
  is normalized to four, with lengths zero/three/four/five. W4's production
  default is 16,384, read once from `LOKI_GPU_MSM_MIN`; this campaign does not
  test environment reload semantics. W5's 1,024..2^28 eligibility range is also
  outside this tiny-input MSM adapter.
- **W6 PK cache:** four traces replayed the real cache from recl9 commit
  `abe97019b099026698d10c97f57d490a9554018f`. Each cold build starts three callers
  behind a barrier, checks the actual keygen callback count and shared Arc,
  compares PK/VK bytes and verifies proofs. Model build/publication stage the
  exposure of synchronous `get` results. The original five W6 tests also passed,
  including disk reuse, cross-process cold fill, identity separation, corruption
  rejection and eviction with caller-owned keys. The public setup API reused its
  key. Temporary files contain only generated public proving material.
- **Mixed-instance diagnostic:** nine real two-column cases reproduce the
  seven accepted controls and two expected stock rejections described above.
- **Scheduler:** executable proposal and scenario tests only; there is no
  production scheduler implementation to replay.

The combined suite passed **15 tests in 15.00 seconds**, without compiler
warnings. The unchanged default `cargo check --locked` passed, and stock
`plonk_api` passed its one test. These are validation timings, not prover or GPU
performance benchmarks. In the tiny imported W6 k8 fixture, cold cache fill was
15.701 ms, four memory hits 0.393 ms, disk reload 14.548 ms, and keygen count one;
its two 2,528-byte proofs were generated and checked in 37.463 ms and 33.927 ms. These small-fixture
measurements do not predict recl9 parent-proof performance.

## Implementation mismatches and remaining obligations

1. **CRITICAL — mixed-instance multi-proof stock rejection.** Prover and
   verifier instance absorption disagree when multiple proofs contain both
   committed and plain columns. The model equality audit and real Rust matrix
   reproduce this; the exact source-order boundary is checked. Reported to
   `#loki` as `CRITICAL W8 mixed-instance stock rejection`, with acknowledgment
   to the coordinator. This is a pre-existing compatibility defect, not a GPU
   optimization introduced by W8. Changes require a protocol compatibility
   decision; no production source was changed.
2. **CRITICAL — stock fixed-RNG proof bytes are not a universal oracle.** Stock
   `src/plonk/lookup/prover.rs` constructs a randomized `HashMap` of leftover
   lookup table values and iterates it to place values in the permuted table
   polynomial. A fixed proof RNG and fixed Rayon count do not fix this ordering.
   In this run, every lookup-rich comparison differed, while both proofs verified.
   W3 independently confirmed the mechanism and that its BTreeMap optimization
   chooses a deterministic valid permutation, potentially changing proof bytes.
   Reported to `#loki` as `CRITICAL W8 fixed-RNG byte oracle`; no production fix
   or protocol change was made here. Verifier acceptance, encoding and operation
   ordering remain mandatory. Full-family byte equality is **not established**.
3. **W5 NTT error policy differs from the requested total fallback.** W5's
   `8e1a33c` adapter preserves input on error but the domain adapter panics on an
   eligible GPU failure. Its owner confirmed this deliberate fail-stop policy.
   The model specifies total CPU fallback; W8 does not claim W5 satisfies it.
   No NTT implementation MBT or GPU arithmetic differential ran in this package.
   A coordinator decision or a fallback/strict-mode integration change is needed.
4. **W6 use requires matching immutable relation/params.** The cache validates
   the full key and loaded PK/VK binding, including more identity fields than the
   model (source/type/relation/VK). Its public `prove` preserves stdlib's caller
   assumption that the relation and parameters match the key; it does not reject
   every same-type wrong-relation misuse. W6 confirmed current call sites borrow
   matching values. At-most-once assumes a successful retained disk publication;
   crashes, failed writes, manual deletion and eviction of disk files are outside
   this guarantee. Hash collisions are excluded by the model.
5. **Scheduler liveness has no temporal proof.** Apalache 0.56.1 is installed.
   `quint verify quint/scheduler.qnt --temporal admittedEventuallyFinish
   --max-steps 24` failed before checking with
   `Handling fairness is not supported yet!`. The specification retains explicit
   weak-fair service, finite work, and at most one checkpoint per job. Random
   completion evidence is not a fairness proof. The budget is ten abstract units
   with reservations seven/heavy and three/small, not measured VRAM. Actual
   allocation peaks, checkpoint persistence and cross-backend locking remain
   implementation obligations; W4 and W5 currently own separate backend locks.

Independent review found and corrected weak cache-content selection, a
non-incrementing build counter, partial-use checks, misleading witnesses, and
test serialization/contended-fill issues. The fault controls demonstrate that
the corrected safety predicates reject those unsafe behaviors.
