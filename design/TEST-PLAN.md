# Loki GPU backend test plan

Status: proposed gates for Opus implementation packages. This file describes tests to add or run; it does not claim that the new `loki-backend-api` or `loki-gpu` implementation has run. The existing W2 and Loki GPU tests are baselines, not evidence for this new backend.

**Evidence convention:** unless a statement is explicitly labelled SOURCE, MEASURED, ESTIMATE, or OPEN QUESTION, normative test requirements in this document are **CODE (planned contract)**. No planned gate below is reported as run or passed.

## Acceptance rule

Correctness has three distinct outcomes and all applicable ones must pass:

1. **Kernel equality:** successful backend calls equal the stock arithmetic result exactly, including canonical/reduced field representation and output ordering. Compare CPU oracle, stock 0.8.3, and the candidate backend on the same inputs.
2. **Proof compatibility:** proofs from the candidate are accepted by the unmodified crates.io `midnight-proofs =0.8.3` verifier using the same SRS and VK. The mutated-input and malformed-proof controls must still be rejected. Verify complete transcript consumption.
3. **Transcript refinement:** on deterministic fixtures, transcript events and proof bytes match the stock reference where byte equality is promised. Report verifier acceptance separately from byte equality. Stock lookup placement is HashMap-order-sensitive, so lookup proof byte inequality alone is diagnostic; pin iteration order (or use no-lookup controls) before making byte-equality claims.

Every result must be labelled **SOURCE**, **CODE**, **MEASURED**, **ESTIMATE**, or **OPEN QUESTION**. A feature build or a successful fallback is not evidence of GPU execution: assert per-operation dispatch/completion counters and require zero fallback in GPU-only cases. A CPU-only host must still pass with the default CPU backend.

The no-feature/default build must preserve the pre-integration stock-compatible path mechanically: no CUDA dependency activation, backend dispatch, altered feature unification, transcript operation, or additional output changes from this module. Keep this as an explicit build and source-diff gate. **CODE (known baseline drift):** Loki's current leftover-lookup ordering sorts rows in `loki-proofs/src/plonk/lookup/prover.rs:427-430`, unlike stock 0.8.3's randomized `HashMap` order. P1 must audit and close/gate this pre-existing difference before anyone claims default byte identity. Keep this distinct from the no-GPU-path gate. For byte comparisons, retain no-lookup controls and a lookup fixture with matched controlled map order; also record stock-vs-stock repeatability. Stock 0.8.3 has a documented multi-circuit committed-instance transcript-order caveat (prover and verifier absorption order differ); preserve and label that upstream limitation instead of treating its failing case as a backend regression or silently modifying only one side.

## Test matrix

| Gate | Inputs and comparison | Required evidence / failure condition |
|---|---|---|
| API contract | Each backend operation with valid and invalid lengths, domains, resident handles, and capabilities | Capability rejection is explicit; backend errors leave caller-visible values and transcript state unchanged and publish no result. V1 operations complete synchronously at the API boundary; inject private runtime stream/event failures and verify upload/download/handle publication only returns after checked completion. Exercise each published error state (`NotStarted`, `InputsPreserved`, `SessionPoisoned`). No panic or published partial output. Backend does not perform policy fallback; Loki's caller decides whether fallback is safe. |
| MSM exactness | CPU `msm_specific` and stock 0.8.3 vs backend on the deployed BLS12-381 curve; G1 and G2 if supported, batches and resident/nonresident bases | Exact reduced point equality, output count/order preserved, all errors surfaced. Include zero/identity points, scalar zero/one/modulus-minus-one/top bits, empty and mismatched lists. `MsmLengthPolicy::ZipTruncate` must preserve stock semantics; `RequireEqual` rejects mismatched direct API inputs. The Loki adapter must form effective paired slices first, then dispatch equal-length slices. Pin scalar modulus, point encoding and Montgomery/canonical representation from Loki code; do not substitute BN254 benchmark fixtures. |
| MSM scalar distributions | Random full-width; every scalar equal; repeated blocks; boolean 0/1; small range 0..15; sparse zeros; adversarial bucket concentration | Differential equality for every case. Record n, scalar-generation seed, distribution, algorithm/capability path, transfers and dispatch counts. Repeated/boolean/zero are mandatory because random-only Pippenger tests miss skew. If signed-digit recoding is used, include carry/top-window cases proving no scalar bits or points are dropped. Never record witness-derived scalar data in shared artifacts. |
| NTT and coset exactness | CPU and stock operations vs backend at supported domain sizes and thresholds | Compare every coefficient; pin field modulus, primitive root, domain size, coset generator `ZETA`, direction/order, and inverse normalization. Round-trip alone is insufficient. Check all five Loki domain conversions and zero/small domains. |
| Quotient evaluation | CPU stock numerator/evaluation vs backend bytecode interpreter on real and generated PLONK expression trees | Compare numerator/quotient elements and length exactly; cover rotations, selector/query types, challenge-dependent terms, zero denominators, quotient degree bounds and chunk boundaries. Require the Lean quotient contract assumptions to be executable preconditions, not assertions that the kernel itself can force. |
| Resident memory and SRS | Upload/reuse/evict/invalidate bases, domains, twiddles, and polynomial buffers; compare each resident result with fresh-upload CPU result | Test private `ResidentBuffer` handles and getters for context/generation/allocation/length/bytes, plus `SrsCacheKey`, `SrsRequest`, and `SrsLease` curve/ABI, basis, length, generation and content identity. Treat externally constructed metadata as untrusted; validate registry type, generation, length and bytes on every use. Reject corrupted/off-curve/non-subgroup bases before use. Exercise eviction, stale handle, concurrent use, allocation failure, and device reset. A sticky GPU-disable or invalidated generation is allowed: stale or partial cache state must never produce a successful result. Verify the caller's CPU fallback policy continues safely and GPU-required mode fails explicitly. |
| Async internals and failure atomicity | Inject allocation, launch, copy, synchronization, and device-loss failures at every internal stream/event boundary | The public V1 API blocks through checked completion. Every returned error preserves caller-visible output/transcript and publishes no new resident handle; resources and owned staging retire safely. Loki may fall back only when a retained host checkpoint exists and its configured policy allows. Exercise errors after internal partial work and before/after event joins; session poison must be explicit. |
| Transcript and proof | Fixed-RNG W2 circuits; candidate Loki against independent stock crate in both verification directions | Same VK and SRS bytes; both verifiers accept both proofs; negative public-input, wrong-VK, truncation, append-byte, and malformed controls reject. Require transcript exhaustion. For deterministic byte fixtures, control lookup map order and Rayon thread count (which affects vanishing randomness); separately record stock-vs-stock repeatability. Check fixed-order/no-lookup byte equality; report lookup differences with first offset and classify against stock-vs-stock repeatability. |
| Recursion chain | Pinned Sherlock leaf → parent → decider cohort(s), each stage and end-to-end | In correctness validation, independently check each leaf, parent, and decider proof with the unmodified stock verifier under its pinned VK/SRS; require final-chain acceptance. Do not insert extra off-circuit verification into the production chain or its timing unless that is part of the production path. Record stage proof, model/setup, witness, keygen, existing in-circuit/local verification and whole-process intervals separately. Never sum nested timers or mix different cohorts. |

## W2 and stock-verifier harness

Extend the existing independent harness in `/home/charles/repos/loki-proofs/loki-harness` only through its assigned Loki owner. Keep crates.io `midnight-proofs =0.8.3` unpatched as the oracle. The harness currently has k10/k14/k17 cross-verification and determinism cases, an ignored k20 case, mutation controls, and a reserved `cuda` feature that currently builds CPU code. A GPU feature gate must therefore assert actual backend dispatch and completed calls; compiling with a GPU feature is not a pass.

Required W2 runs, serialized (`RAYON_NUM_THREADS=1`, `--test-threads=1` for proof tests):

- CPU default: existing k10/k14/k17 gates and explicit k20 when resources permit.
- New backend: same fixtures with GPU required and fallback forbidden; run supported k10/k14/k17 and k20 separately.
- Recursion: actual Sherlock artifacts and production-shaped circuits, not only the synthetic W2 circuit.
- For each run: compare VK and full SRS bytes; candidate→stock and stock→candidate verification; mutated public input and malformed/extended proof rejection; transcript fully consumed; backend dispatch counts match expected operation counts.
- Keep lookup byte comparison diagnostic until P1 closes the existing ordering drift. Use no-lookup and matched-map-order lookup controls for strict stock byte equality; separately test candidate repeatability with fixed proof RNG and pinned thread count.

The current harness reports a single cold sample and includes insecure test-only SRS setup; those numbers are not performance baselines. Its documented k20 case is ignored by default. Preserve these qualifications when reusing its outputs.

## Planned executable targets and commands

These are **CODE (planned)** target names agreed with the `loki-gpu` crate owner; they do not exist yet. Run CPU/API checks first, then GPU checks through the host runner with `CARGO_BUILD_JOBS=3` and the shared GPU lock:

Standalone backend checks:

```sh
CARGO_BUILD_JOBS=3 cargo check --workspace
cargo test -p loki-backend-api
cargo test -p loki-gpu --test runtime_contract
CUDA_HOME="$HOME/.local/opt/cuda-13.4.3" CARGO_BUILD_JOBS=3 cargo check -p loki-gpu --features cuda
cargo test -p loki-gpu --features cuda --test ffi_abi
cargo test -p loki-gpu --features cuda --test srs_registry
cargo test -p loki-gpu --features cuda --test ntt_differential
cargo test -p loki-gpu --features cuda --test resident_ntt
cargo test -p loki-gpu --features cuda --test quotient_differential
cargo test -p loki-gpu --features cuda --test quotient_validation
cargo test -p loki-gpu --features cuda --test msm_differential
cargo test -p loki-gpu --features cuda --test scheduler_contract
```

Loki and W2 checks after the P1 integration lands:

```sh
cargo test --test backend_default_stock
cargo test --features loki-backend-api --test backend_cpu_equivalence
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=3 cargo test --release --locked --manifest-path /home/charles/repos/loki-proofs/loki-harness/Cargo.toml -- --test-threads=1 --nocapture
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=3 cargo test --release --locked --manifest-path /home/charles/repos/loki-proofs/loki-harness/Cargo.toml --features loki-gpu-backend -- --test-threads=1 --nocapture
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=3 cargo test --release --locked --manifest-path /home/charles/repos/loki-proofs/loki-harness/Cargo.toml --features loki-gpu-backend --test differential k20_cross_verify_and_determinism -- --ignored --test-threads=1 --nocapture
```

The planned `loki-gpu` integration files are `crates/loki-gpu/tests/runtime_contract.rs`, `ffi_abi.rs`, `srs_registry.rs`, `ntt_differential.rs`, `resident_ntt.rs`, `quotient_differential.rs`, `quotient_validation.rs`, `msm_differential.rs`, and `scheduler_contract.rs`. The fuzz targets are `crates/loki-gpu/fuzz/fuzz_targets/msm_diff.rs`, `ntt_diff.rs`, `quotient_program.rs`, and `residency_state.rs`. Add W2 identifiers `gpu_k10_cross_verify`, `gpu_k14_cross_verify`, `gpu_k17_cross_verify`, `gpu_k20_cross_verify`, `default_cpu_path_unchanged`, `no_lookup_stock_bytes`, `lookup_matched_order_stock_bytes`, and `stock_default_dependency_audit`. Add `loki-gpu-backend = ["loki/loki-gpu-backend"]` to `loki-harness/Cargo.toml`; Loki's planned `loki-gpu-backend` feature forwards to `loki-gpu/cuda`. The default-dependency audit fails until P1 accounts for the existing lookup-order drift. Each GPU test must assert positive backend dispatch and fail on unexpected fallback. Until this forwarding lands, W2's legacy `cuda` placeholder remains CPU-only.

## Differential and fuzz targets

Build CPU-reference property tests first, then feed the same corpus to the GPU implementation. Seed and print deterministic public synthetic cases; minimize failing inputs. Fuzz targets should cover:

- `msm_diff`: scalar patterns above; points at identity, duplicates, negations and edge encodings; empty/partial batches and final windows. Compare backend, CPU Loki, stock MSM and a naive scalar-multiplication sum.
- `ntt_diff`: all supported domain sizes, roots, coset generators, layouts and conversions; compare backend vs stock and naive DFT on small sizes. Assert GPU call counts when GPU is selected.
- `quotient_program` (`crates/loki-gpu/fuzz/fuzz_targets/quotient_program.rs`): generated expression programs and PLONK fixtures; compare interpreter output to stock numerator/evaluation and check bytecode bounds/type validation.
- `residency_state` (`crates/loki-gpu/fuzz/fuzz_targets/residency_state.rs`): model upload, retain, use, evict, invalidate, failure and retry sequences; compare each observable result and cache identity with a fresh uncached path.
- `proof_roundtrip` / `proof_parse`: generated valid circuits and hostile proof/VK/SRS encodings; candidate and stock acceptance decisions agree; parser does not panic. Include negative public/committed input, truncation, bit flips and trailing bytes.
- `scheduler_contract` (`crates/loki-gpu/tests/scheduler_contract.rs`): multiple concurrent proof requests on one device under a bounded memory budget; replay completion, cancellation and failure traces. Check no cross-proof buffer aliasing and no proof-order/transcript interleaving.

Use short CPU fuzz slices on every change, then longer GPU slices under the shared runner/lock. Preserve minimized regressions as deterministic cases. Fuzz evidence does not replace stock verifier checks or Lean obligations.

## Lean-to-test mapping

Names below refer to the current Lean spec in `/home/charles/repos/loki-lean-wt/gpu-spec`; backend-specific modules will refine these with explicit state and error contracts.

| Lean contract | Executable evidence |
|---|---|
| `Backend.error_preserves_public_state`, `Backend.InterfaceContracts.refines`, `sameOrder`, `proof_events_eq_stock`, `proof_bytes_eq_stock`, `verifier_decision_eq_stock`, `byte_verifier_decision_eq_stock` | W2 cross-verification in both directions and fixed-order transcript/byte comparisons; injected failures snapshot caller-visible values and transcript. These are abstract contracts, not native-runtime proofs. |
| `BackendMsm.dispatch_eq_msmSpecific`, `transactionalDispatch_meetsMsmSpecific`, `segmentedValue_eq_pippengerList`, `segmentedKernel_success_correct`, `segmentedKernelDispatch_meetsMsmSpecific`; `stockBackendWithBatchDispatch_proofBytes_eq_stock` | `msm_differential`, `srs_registry`; exact reduced output, stock zip semantics, resident sequence/retry cases, and actual backend dispatch counters. `segmentedValue_eq_pippengerList` carries the explicit `OBLIGATION(msm-segmented-partition)` (currently a marked sorry); a passing Rust differential gate does not discharge it. |
| `BackendNtt.validateRequest_rejects_wrong_length`, `dispatch_output_length`, `kernelList_eq_stock`, `kernelCall_contract`, `backendWithKernels_proof_bytes_eq_stock` | `ntt_differential` and `resident_ntt`; pin omega/ZETA, direct coefficient comparison, injected internal error and caller CPU fallback policy. |
| `BackendQuotient.validate_iff`, `lower_add_correct`, `lower_sub_correct`, `lower_mul_correct`, `lower_square_correct`, `lower_double_correct`, `lower_negate_correct`, `valid_program_eval_some` (**OBLIGATION(quotient-interpreter-safety)**), `runRows`, `runProgram`, `runProgram_valid_eq_bytecodeVector`, `runProgram_eq_bytecodeVector_of_simulation`, `bytecodeVector_eq_stock_numerator`, `bytecodeVector_evaluates_numerator`, `ordered_source_passes_eq_stock`, `ordered_source_passes_evaluate_numerator`, `compiledBytecodeCall_meetsContract`, `compiledBytecodeProofBytes_eq_stock`, `dispatch_eq_stock`, `error_preserves_public`, `stockBackendWithDispatch_meetsInterface`, `proof_bytes_eq_stock`; formal source simulation remains **OPEN OBLIGATION(quotient-compiler-lowering)**. `bytecodeOutputs` uses the fail-closed Option-returning runner; test invalid bytecode and row-evaluation failures preserve public state. | `quotient_differential`, `quotient_validation`; real circuit evaluations, operand/range validation, scaled rotations, transaction failure, and bytecode values compared at each chunk boundary. The abstract Lean bridge does not prove the Rust compiler/decoder or CUDA execution. |
| Scheduler sequential refinement | `scheduler_contract` and W2 concurrent-request traces check per-proof transcript order and output equality against serialized CPU execution. **OPEN QUESTION / unformalized:** the shared Lean modules do not currently prove scheduler trace refinement. |

The current Lean tree has two marked `sorry` declarations: `BackendMsm.segmentedValue_eq_pippengerList` (`OBLIGATION(msm-segmented-partition)`) and `BackendQuotient.valid_program_eval_some` (`OBLIGATION(quotient-interpreter-safety)`). The source-to-bytecode compiler simulation remains an explicit open obligation without a `sorry`. The Lean proof establishes contracts only under their stated hypotheses; kernel differential tests validate concrete behavior against those hypotheses, and neither evidence substitutes for the other.

## Recursion-chain benchmark protocol

Report two cohorts separately and do not fabricate a composite chain from their stages:

1. **A102/A80 → A3 → A4 heterogeneous Sherlock cohort.** A3 is the k20 parent under active Loki/W10 measurements; include the exact child proofs/VKs and terminal decider inputs. **MEASURED (historical single sample; SOURCE:** `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`): the H0 run used Core Ultra 9 285K, WSL2, RTX 5090 32 GB, CUDA 13.4.3, Rayon=4, BLST=1 and has no variance estimate. The earlier 233.3 s GPU MSM+NTT and 175.3 s W10 `cuda-argument-commit` values are different implementation snapshots; W10's first sample is not a matched repeat. Treat both as historical, not the acceptance baseline.
2. **B3 synthetic 16-transaction leaf → single-child parent → decider cohort.** **MEASURED (historical; incomplete host provenance; SOURCE:** `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`): receipts report leaf k20, parent k19 and decider k20. These are S0 v3 local-staging measurements, not ledger calls. The source commit, binary hash, Cargo.lock hash, Rust version, threads and stock 0.8.3 verifier are recorded; CPU/GPU and BLST policy are not fully pinned. Re-run with complete hardware/software provenance before claiming an improvement.

For each new baseline/candidate pair, record: circuit/relation source commit and dirty status; input proof/VK/SRS digests and key lineage; prover and stock verifier source/version; Cargo.lock and binary digest; compiler/toolchain, features, GPU architecture/driver/CUDA, host CPU/RAM/OS; thread, BLST and stream policies; RNG seed only for public synthetic/deterministic fixtures (production/private-witness runs record RNG policy and run ID, never secret prover/blinding seeds); warm/cold status; run order; sample count, median and spread; proof/model/keygen/witness/verification/child-process scopes; peak host/device memory; operation counts, residency hit/miss and fallback counts. Store raw logs and a machine-readable stage manifest outside versioned artifacts when sensitive witnesses could be present.

Measure each stage and full dependency-DAG wall time; report proof-only sum separately from process wall time. Do not add nested phase timers to their parent, or include model/setup time in proof time. Keep one heavy proving job for latency comparisons; separately measure throughput with multiple proofs. **MEASURED (historical; SOURCE:** `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`): A3's 85.9 s `create_proof`, 47.2 s `quotient_evaluation`, approximately 89 s outside `create_proof`, 58 ms warm 2^20 random MSM, and 6.2 GiB each-way NTT traffic. These are supplied measurements, not fresh results from this test plan. Re-evaluate them under the matched candidate/baseline protocol.

Promotion requires (a) all correctness gates, including stock verification and no unplanned fallback, and (b) a repeated, matched improvement to end-to-end chain latency. Kernel speed alone is insufficient. State track explicitly: **Track A** preserves relation/VK/key lineage; **Track B** changes relation/circuit and requires regenerated keys and stock verifier acceptance of the new chain. For Track B record changed VK hashes and deployment lineage; do not compare only against an incompatible old terminal key.
