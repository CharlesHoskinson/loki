# Loki GPU implementation plan

> CODE (execution instruction): Opus implementers execute one package at a time with explicit file ownership, review, and recorded gates. Use the executing-plans workflow; the coordinator assigns implementers and integrates changes. This plan authorizes no push or deployment.

**Goal — SOURCE (owner brief):** provide an Apache-2.0 backend that plugs into Loki, preserves stock midnight-proofs 0.8.3 behavior and verifier acceptance, and reduces complete Sherlock recursion latency at k19–k20.

**Architecture — CODE (planned contract):** the CUDA-neutral API is a separate optional dependency. Loki owns stock CPU fallback and transcript/RNG behavior; `loki-gpu` owns validated operation execution and device resources. The initial submitted code is an Unsupported skeleton; the following packages implement it.

**Stack — CODE:** Rust workspace, Lean v4.34.1/Mathlib v4.34.1 specification, C ABI and cc/nvcc targeting sm_120, BLS12-381 Fr/G1, optional licensed sppark primitives. All CUDA features stay off by default.

## Global constraints and source pins

- SOURCE: correctness → full recursion latency → raw kernel speed is the owner priority order.
- CODE: default Loki uses stock CPU semantics, feature graph, RNG/thread behavior, map traversal and proof encoding. Preserve `default = ["committed-instances"]`; do not enable backend, `blst/no-threads`, profiling or optimization features transitively.
- CODE: source pin is Loki `877114ddc1da5508230e424ec06d55da1ba00117`; W10 worktree `053577f6083b329f2bbef1f3711f92cadc92a24f` includes production `000b3ed`. W13's tree has in-flight registry work; obtain its exact committed handoff before porting it. Recheck all line citations against these pins and record the implementer's new base.
- CODE: this design team edits only loki-gpu and gpu-spec. Loki production patches below are future coordinator-owned work. One owner per file; shared manifests/headers require an explicit handoff between packages.
- SOURCE: never open/copy open-icicle source, its vendored derivatives, H21.md or its sim scripts. Allowed memo/public papers/docs provide ideas; only licensed permitted code may be reused. Record every substantive algorithm and exact copied file in PROVENANCE; preserve notices and update NOTICE before copying sppark.
- CODE: no GPU timing on private witnesses or published witness-derived scalar distributions. Synthetic distributions and public phase/size metadata are sufficient for reports; in-process equality checks can avoid exporting values.
- CODE: use the runner for host Cargo/commit work, shared heavy/GPU locks in that order for device experiments, bounded jobs/threads and no simultaneous independent benchmarks. Never push.
- CODE: every implementation package must preserve original inputs until checked completion, keep transcript writes outside backend execution, and abort if device-only data is lost without a host checkpoint.

## Ordered packages and dependencies

ESTIMATE: effort is focused engineer-days including tests/review, excluding shared-host queue time and coordinator integration. These are planning ranges, not delivery promises. Multiple engineers may work on disjoint mathematical/kernel files after interfaces freeze; no shared-file concurrent edits.

| Order | Package | Owner role | Dependencies | Effort | Feature |
|---|---|---|---|---|---|
| 1 | P1 stock CPU plug-in seam | Rust integration + stock auditor | none | 3–5 days | `loki-backend-api`; default off |
| 2 | P2 transactional runtime and ABI | Rust/CUDA runtime + Lean reviewer | P1 | 4–7 days | `loki-gpu-backend`, crate `cuda` |
| 3 | P3 legacy adapter and SRS registry | Rust/SRS implementer | P1, P2, W13 handoff | 2–4 days | `loki-sppark` or new-backend `loki-gpu-srs-registry` |
| 4 | P4 resident NTT/coset pipeline | CUDA transforms + Lean NTT | P2, coherent domain contract | 4–7 days | `cuda-resident` |
| 5 | P5 quotient bytecode compiler/interpreter | Rust compiler + CUDA quotient + Lean | P3, P4, W10 handoff | 7–12 days | `cuda-quotient` |
| 6 | P6 balanced MSM, conditional | CUDA MSM + Lean MSM | P2, P3, remaining-budget admission | 5–8 days | `cuda-msm-balanced` |
| 7 | P7 complete-chain integration and one-heavy scheduling | Recursion integration + harness | P1–P5; P6 only if admitted | 3–6 days | explicit new-backend orchestration; `loki-profile` for measurement |
| 8 | P8 lookup compression/preparation, conditional | PLONK/CUDA compiler | P5, P7 reprofile | 4–7 days | `cuda-lookup-prepare` |
| 9 | P9 batching/concurrency/precompute, conditional | GPU performance/runtime | P7, actual lifetime/service traces | 4–8 days per admitted experiment | new-backend experimental config, default off |

CODE: `loki-backend-api` is the common base. `loki-gpu-backend` conflicts with `loki-sppark` and with direct legacy `cuda`/`cuda-ntt` flags. Preserve W13 `cuda-srs-registry = ["cuda"]` as legacy; the new registry uses `loki-gpu-srs-registry = ["loki-gpu-backend"]`. New `cuda-resident`, `cuda-quotient`, `cuda-msm-balanced`, and `cuda-lookup-prepare` imply the new backend. Extract W10's unchanged coefficient strategy under `loki-argument-commit = ["loki-backend-api"]`; retain `cuda-argument-commit = ["cuda", "loki-argument-commit"]` as its legacy compatibility alias. New-backend P5 uses `loki-argument-commit`, preserving W10 arithmetic without selecting legacy CUDA. Reject mixed selection at compile time; optional dependency feature union must not select two device runtimes. The exact table is in LOKI-PATCH-PLAN.md.

CODE: memo mapping is WP1 → P2/P4, WP2 → P5, WP3 → P3, WP4 → P6, WP5 → P8. P1 is deliberately first. P7 measurements begin in P1 and are completed after the latency packages, so recursion does not become an afterthought. P9 cannot block the single-chain release.

## P1 — Stock CPU plug-in seam

CODE (problem/result): introduce the optional API without changing default proof behavior; an API-enabled CPU implementation is identical to the stock operation sequence. This includes auditing pre-existing Loki drift rather than assuming current main is already byte-identical.

**Owner:** Rust integration engineer; independent stock auditor reviews default path.

**Exact files — CODE (planned):** Loki `Cargo.toml`, `Cargo.toml.orig`, `src/lib.rs`, new `src/backend/mod.rs`, `src/backend/cpu.rs`, `src/backend/dispatch.rs`; call sites `src/poly/kzg/msm.rs::msm_specific`, `src/poly/domain.rs::{fft,ifft,coeff_to_extended,extended_to_coeff}`, `src/plonk/prover.rs::compute_nu_poly`, `src/plonk/evaluation.rs::Evaluator::evaluate_numerator`; audit/default correction in `src/plonk/lookup/prover.rs::permute_expression_pair`. API changes only in `crates/loki-backend-api/src/lib.rs` under explicit handoff. Tests: new Loki `tests/backend_cpu_equivalence.rs`, `tests/backend_default_stock.rs`, existing W2 `loki-harness/`, independent registry-stock verifier project.

**Interfaces — CODE:** implement `MsmBackend<F,G>::msm`, `TransformBackend<F>::transform`; CPU-only unsupported resident/quotient/SRS methods return `BackendError::unsupported`. Dispatcher preserves `MsmLengthPolicy::ZipTruncate`, exact domain roots and one inverse normalization. CPU quotient calls the existing evaluator until its typed program compiler exists. Selection lives entirely in Loki; backend does not call back into dispatcher, avoiding recursive fallback.

- [ ] CODE: record stock crates.io 0.8.3 source checksum and compare relevant default paths/dependency features to Loki base. In particular, main `lookup/prover.rs:427-430` sorts leftover keys unconditionally; stock traversal differs. Restore the stock default branch and gate deterministic leftover sorting behind `loki-deterministic-lookup-order = []`, never enabled by a backend feature. Audit the other existing unconditional changes for RNG/order effects and record them; do not report current main as untouched stock.
- [ ] CODE: add negative/default fixtures first. Under fixed RNG and thread count, compare no-lookup proof bytes with registry stock and compare the API-enabled CPU path to feature-off. For lookup fixtures, require the exact default stock map implementation and controlled map-order tests; independently seeded stock HashMaps do not promise byte equality. Require all proofs accepted by unmodified stock and exact VK/SRS bytes.
- [ ] CODE: add optional dependencies and compile-time accelerator exclusivity. Use `loki-backend-api = ["dep:loki-backend-api"]`, `loki-sppark = ["loki-backend-api", "cuda", "cuda-ntt"]`, and `loki-gpu-backend = ["loki-backend-api", "dep:loki-gpu", "loki-gpu/cuda"]`; W2 forwards `loki-gpu-backend = ["loki/loki-gpu-backend"]`. Keep original default bodies under feature-off branches, including stock CPU routines and transcript/RNG code. Route only feature-on calls through typed requests.
- [ ] CODE: test empty/unequal MSM prefixes, zero/one scalars, arbitrary curve CPU handling, roots/coset/normalization, malformed requests and forced Unsupported. No device required.
- [ ] CODE: run `cargo check --locked`, `cargo test --test backend_default_stock`, `cargo test --features loki-backend-api --test backend_cpu_equivalence`, and the W2/independent stock commands defined in TEST-PLAN. Capture `cargo tree -e features` for default and seam configurations; inspect that backend/CUDA and `blst/no-threads` are absent from the default build.
- [ ] CODE: obtain review and commit the exact file list through the runner before P2 integration.

**Success — CODE:** exact CPU operation equality, stock default code/feature/RNG/order audit, controlled byte equivalence, full W2 and independent stock verification. **Measured success — ESTIMATE gate:** no significant default latency/memory regression in repeated matched fixtures; report actual median/range rather than a synthetic speedup target. **Kill:** any default-path byte/semantic drift, unexpected dependency feature, altered transcript event order, or inability to identify the stock reference. Fix P1 before any GPU work is promoted.

## P2 — Transactional device runtime and versioned ABI

**Owner:** runtime engineer, with a Rust safety reviewer and Lean backend reviewer. **Files — CODE:** `crates/loki-gpu/{Cargo.toml,build.rs}`, `src/{lib,ffi,runtime}.rs`, `cuda/include/loki_gpu_ffi.h`, new `cuda/runtime.cu`, new `tests/runtime_contract.rs`, `tests/ffi_abi.rs`; Lean `LokiLean/Backend.lean` in gpu-spec. Feature `loki-gpu-backend` forwards the crate `cuda` feature. The submitted v1 wire functions remain Unsupported until the corresponding operation is implemented.

**Interfaces — CODE:** `Backend::capabilities`, `ResidentBackend::{generation,runtime_snapshot,try_reserve,release_reservation,upload_scalars,upload_points,download_scalars,release_scalars,release_points}`; blocking public completion; versioned C handles carry context/generation and explicit status. `RuntimeSnapshot` is observational; `try_reserve` is the authoritative immediate admission attempt. All metadata/IDs from callers are untrusted until compared to backend-owned type/length/form/generation records.

- [ ] CODE: add fake-runtime failure tests for allocation, copy, launch, event, cancellation, stale generation, metadata forgery, duplicate release and context loss before replacing stubs.
- [ ] CODE: build the native ABI with optional cc/nvcc, native sm_120 and the documented PTX target; add ABI-size/status assertions and linked symbol tests. Keep no-toolkit cargo check working with defaults.
- [ ] CODE: implement one device coordinator, explicit lock order, checked shared budget, bounded owned staging, long-lived streams/events, and allocation registry. Retain allocations until the last event, including any private cancellation path; direct borrowed async DMA is prohibited.
- [ ] CODE: publish outputs only from successful return; internal cancellation may drain/retire but cannot write caller output. Poison generation on uncertain completion. Distinguish CPU fallback with host checkpoint from unrecoverable resident-only loss.
- [ ] CODE: run `cargo test -p loki-gpu --test runtime_contract`, `cargo test -p loki-gpu --features cuda --test ffi_abi`; `lake build LokiLean.Backend`. Use sanitizers/compute-sanitizer through host runner on the native lifetime tests when kernels are present.

**Success — CODE:** all injected failures preserve logical state, no use-after-free or stale success, no fake device generation, default builds without toolkit, native link tests pass. **Measured success — ESTIMATE gate:** bounded pinned fallback operates on WSL2 with reported bytes and event/wall timings; no latency gain required for this prerequisite. **Kill:** unsafe lifetime/aliasing, publication after error/drop, unbounded pinning, or fallback requiring lost input. Do not proceed with resident arithmetic until corrected.

## P3 — Legacy sppark adapter and validated SRS registry

**Owner:** SRS/runtime implementer after W13 handoff. **Files — CODE:** new Loki `src/backend/sppark.rs`; existing `src/poly/kzg/msm/cuda/{backend,convert,tests}.rs`, `cuda-runtime/src/lib.rs`, `cuda-ntt/src/lib.rs`; new-backend `crates/loki-gpu/src/srs.rs`, `tests/srs_registry.rs`; Lean extension under `BackendMsm.lean` or a separately assigned module. Feature `loki-sppark` selects legacy; `loki-gpu-srs-registry` selects the new backend's registry.

**Interfaces — CODE:** `SrsRegistry::{acquire_srs,srs_msm,release_srs}` with `SrsRequest`, `SrsCacheKey`, `SrsLease`; validated immutable bases plus exact consumed-prefix comparison. No address/digest-only trust. Domain/basis and generation must match; a larger Lagrange basis is not a smaller-domain basis.

- [ ] CODE: compare W13's committed registry against this contract; port/reuse its exact snapshot and byte accounting after ownership handoff. Record source notices and persistent-versus-scratch bytes.
- [ ] CODE: tests first: rewarm A immediately before same-address mutation, equal-content distinct-address hits, n/n−1 prefixes, k19/k20 coefficient versus Lagrange, corrupted/off-curve/non-subgroup input, leases across eviction/context loss and oversized entries.
- [ ] CODE: implement lease-counted byte-budget LRU, validated upload, exact-hit checks and shared device coordinator; no eviction while queued work may access a lease.
- [ ] CODE: adapt the existing sppark arithmetic without changing its outputs; legacy and new backend are separately selected and never race through independent guards.
- [ ] CODE: run `cargo test -p loki-gpu --features cuda --test srs_registry`, Loki `cargo test --features loki-sppark`, W2 and independent verifier gates; prove prefix/cache lemmas against `dispatchSeqW9_eq` assumptions.

**Success — CODE:** all prefix/mutation/lease tests pass and every cache hit proves the exact requested bases. **Measured success — ESTIMATE gate:** report cold/warm misses, persistent/scratch high-water and full-chain cache reuse with no claimed latency before measurement. **Kill:** identity ambiguity, subgroup-validation bypass, overflow/underaccounting, or cross-generation reuse. Registry correctness remains needed even if its isolated speed benefit is small.

## P4 — Resident NTT and exact cosets

**Owner:** transform implementer + Lean NTT reviewer. **Files — CODE:** `crates/loki-gpu/src/ntt.rs`, `cuda/ntt.cu`, new `tests/ntt_differential.rs`, `tests/resident_ntt.rs`; Loki `src/backend/dispatch.rs`, `src/poly/domain.rs`; Lean `LokiLean/BackendNtt.lean`. Feature `cuda-resident` implies new backend/runtime.

**Interfaces — CODE:** `TransformBackend::transform` and `ResidentBackend::transform_resident`, `TransformSpec`, `CosetSpec`, exact source/output lengths, direction/root/order and inverse normalization; a fresh result handle is published after completion. Start with permitted sppark device-pointer transforms and independently implemented stock ζ scaling/padding.

- [ ] CODE: add naive small DFT and CPU exact-forward/inverse/coset oracles; wrong-root roundtrip is a negative control. Cover zero/one/max field values, k19/k20 base and corresponding extended domains, rejected shapes, every domain conversion, and multi-wrap rotations.
- [ ] CODE: implement checked scalar wire conversion, natural-order wrappers, exact one normalization and coset multiply/pad/unshift. Keep source allocation immutable; reserve scratch/output before work.
- [ ] CODE: add resident transform chains through the runtime, with completion dependencies and explicit host readback only at required boundaries. Retain host checkpoint for every recoverable phase.
- [ ] CODE: run `cargo test -p loki-gpu --features cuda --test ntt_differential` and `--test resident_ntt`; `lake build LokiLean.BackendNtt`; full Loki domain/W2 stock gates.

**Success — CODE:** pointwise equality with exact stock roots/cosets and all injected-failure cases; no new butterfly proof assumed from roundtrip. **Measured success — ESTIMATE gate:** report full-call, conversion, H2D/D2H, kernel and allocation costs; promote residency together with P5 only if the combined A3/chain comparison passes. **Kill:** root/layout mismatch, double scaling, lost original input, or unbounded resident state. Do not claim independent savings from the entire historical NTT cost when P5 already includes it.

## P5 — Ordered quotient bytecode compiler and interpreter

**Owner:** Rust expression compiler + CUDA quotient engineer; independent PLONK/Lean reviewer. **Files — CODE:** new Loki `src/backend/quotient_compile.rs`, W10 `src/plonk/evaluation/cuda.rs` after handoff, `src/plonk/{evaluation,prover}.rs`; `crates/loki-gpu/src/quotient.rs`, `cuda/quotient.cu`, new `tests/quotient_differential.rs`, `tests/quotient_validation.rs`; Lean `LokiLean/BackendQuotient.lean`. Feature `cuda-quotient` implies `cuda-resident` and explicit new-backend selection.

**Interfaces — CODE:** lower the stock `GraphEvaluator` plus all argument terms into `QuotientProgram`/`QuotientInstruction`/`Operand`; `QuotientBackend::evaluate_quotient` returns the resident numerator. Inputs are validated frozen challenge constants and exact-domain resident columns. The method name does not authorize silently dividing by the vanishing polynomial twice: document numerator versus quotient at this boundary and keep the tail explicit.

- [ ] CODE: create an independent CPU interpreter and invalid-program tests before native execution: each opcode, register lifetime, output register, constants, column bounds, rotations, lengths and challenge snapshot. Implement the explicit version-1 40-byte instruction encoding in QUOTIENT.md, with golden ABI vectors, checked decoding and no Rust enum/struct layout casts.
- [ ] CODE: compile the entire circuit-instance/gate/permutation/lookup/trash sequence, preserving Horner order. Initially avoid cross-graph CSE; it has no demonstrated standalone A3 budget. Emit static register-liveness certificates or independently checked allocation mappings.
- [ ] CODE: implement one grid-stride row interpreter using bounded per-thread temporaries, with measured register/spill limits. Reject oversized programs before launch. Do not allocate one full-domain vector for every intermediate or assume all program data fits CUDA constant memory.
- [ ] CODE: retain needed fixed/permutation/advice/instance cosets, release completed dead inputs, evaluate numerator, divide by the exact periodic vanishing values, inverse coset transform and preserve stock quotient truncation/commitment sequencing. Keep W10 coefficient strategy fixed for comparison via `loki-argument-commit`; retain the old alias only on legacy runs.
- [ ] CODE: run `cargo test -p loki-gpu --features cuda --test quotient_validation` and `--test quotient_differential`; `lake build LokiLean.BackendQuotient`; compare full numerator and h coefficients on public A3 metadata/synthetic inputs, then W2 and actual A3 Poseidon stock-verifier acceptance.

**Success — CODE:** every row/instruction/rotation agrees with the CPU oracle; compiler and allocation obligations are explicit and discharged at their scope. **Measured success — ESTIMATE target:** repeated matched whole A3/chain median improves by at least 3%, without correctness/fallback failures; report quotient+transform combined cost. The memo's ≤4-second quotient-pipeline target is a stretch experiment, not a release promise. **Kill:** pointwise disagreement, excessive spills/memory, fallback hidden as success, or no meaningful whole-proof movement. Measure W12 outside-proof time separately; do not multiply unrelated W10/W12 gains.

## P6 — Skew-robust MSM (conditional)

**Owner:** CUDA MSM engineer + Lean MSM reviewer. **Files — CODE:** `crates/loki-gpu/src/msm.rs`, `cuda/msm.cu`, new `tests/msm_differential.rs`, `benches/msm_distributions.rs`; Loki `src/backend/dispatch.rs`; Lean `LokiLean/BackendMsm.lean`. Feature `cuda-msm-balanced` implies runtime/SRS prerequisites.

**Interfaces — CODE:** `MsmBackend::msm`, `SrsRegistry::srs_msm`; fixed unsigned 256-bit windows, nonzero `(window,digit,index)` records, sorted exact bucket runs, bounded segments and deterministic reduction; reduced normalized result. See MSM.md for the weighted bucket-chunk offset formula and memory estimates.

- [ ] CODE: first measure remaining eligible integrated A3/chain MSM full-call time with W10 coefficient commitments fixed. If the relevant call aggregate is below 3% of the contemporaneous whole-proof window, defer that performance scope. A large aggregate is only a necessary budget, not removable kernel time.
- [ ] CODE: test digit reconstruction and exact once-only partition/reduction with zero/boolean/repeated/sparse/full-width/r−1 scalars, top windows, identity/cancellation bases and n−1/n/n+1 lengths.
- [ ] CODE: implement independent digit emission, sort/RLE/scan, bounded segment partials, bucket reductions and window combination using licensed arithmetic primitives. Account all worst-case records and sort temporary storage. No cooperative-grid assumptions or imported hardware magic constants.
- [ ] CODE: run at least 10,000 public small differentials, then k16/k18/k20 distribution tests. `cargo test -p loki-gpu --features cuda --test msm_differential`; benchmark through runner. `lake build LokiLean.BackendMsm`; inspect axioms/obligations for the segmented theorem.
- [ ] CODE: complete W2 and actual A3/chain stock verification with nonzero successful new-backend dispatch counters and no fallback.

**Measured success — ESTIMATE targets:** structured medians ≤1.2× random, random ≤1.1× legacy sppark; separately ≥3% repeated matched whole-proof/chain improvement for latency promotion. Robustness alone may justify an explicitly labeled non-default specialist backend but is not a recursion speedup. **Kill:** insufficient remaining budget, mismatch, excessive scratch, or targets missed after bounded tuning. No credit for W10's already removed historical 43–61-second argument regression. Signed recoding/group-by-scalar/precomputation require separate proof/measurement gates.

## P7 — Whole-chain integration and one-heavy scheduler

**Owner:** Sherlock integration engineer + independent harness reviewer. **Files — CODE:** new `crates/loki-gpu/examples/chain_resource_trace.rs`, `tests/scheduler_contract.rs`; proposed `/home/charles/repos/Moriarty-loki/crates/mori-recursion/src/backend_chain.rs` and its `src/lib.rs` export (future external-owner handoff only); Loki `src/backend/mod.rs` and `loki-harness/Cargo.toml` feature forwarding. This repository stores integration contracts in RECURSION.md and TEST-PLAN.md. No standalone new recursive relation is introduced.

**Interfaces — CODE:** `RuntimeSnapshot` and immediate reservation tokens; ready DAG nodes bind role, completed proof/VK/SRS/relation identifiers and existing production validation; qualification independently verifies every stage. The caller owns `create_proof`, transcript, checkpoint and artifact lifecycle. Do not extend backend traits with circuit or ledger details.

- [ ] CODE: pin one complete cohort and parent-child artifact lineage; keep B3 and A102/A80→A3→A4 separate. Add negative wrong-child-VK, missing-dependency and cancelled-job tests.
- [ ] CODE: integrate operation resource traces and one-heavy FIFO admission, no large resident leases while queued, fixed shared CPU budget and heavy-before-GPU lock order. Test cancellation/error release and stale reservation rejection.
- [ ] CODE: wire actual features through W2 rather than an empty `cuda=[]` harness feature. Require attempted/succeeded/fallback/error counters per operation and selected backend identity.
- [ ] CODE: run `cargo test -p loki-gpu --test scheduler_contract`, then full leaf→parent→decider k19/k20 proof/stock acceptance. Capture a top-level timer plus nonoverlapping setup, PK reuse, synthesis, prove, verification and queue windows; include pinned-ledger acceptance when available from the established driver.
- [ ] CODE: alternate baseline/candidate order with at least three samples each, cold/fresh and warm/reused-key separately. Keep W12 revision fixed. Publish exact commands, source/binary/lock/artifact hashes, threads, host/toolchain/features, event/wall costs and peak memory.

**Success — CODE:** dependency-correct chain, all unmodified-stock checks pass, no dangling leases, exact transcript controls. **Measured success — ESTIMATE gate:** ≥3% matched complete-chain median reduction attributable to this backend, with shared-host spread reported; A3-only improvement is reported separately. **Kill:** mixed cohorts/scopes, unaccounted fallback, skipped verification, increased end-to-end latency, or context recovery without originals. No throughput claim before P9.

## P8 — Lookup compression/preparation (conditional)

**Owner:** PLONK/CUDA compiler engineer. **Files — CODE:** Loki `src/plonk/lookup/prover.rs`, `src/backend/quotient_compile.rs`; new `crates/loki-gpu/src/lookup.rs`, `cuda/lookup.cu`, `tests/lookup_differential.rs`; new assigned Lean `LokiLean/BackendLookup.lean`. Feature `cuda-lookup-prepare` implies P5.

**Interfaces — CODE:** reuse the validated quotient-expression compiler for Lagrange-row compression with rotation stride one, but preserve the selected CPU lookup ordering contract. Any GPU leftover placement must equal the explicitly selected deterministic CPU route; stock default remains stock map behavior. Output feeds existing W10 coefficient commitments without new transcript draws.

- [ ] CODE: profile compress/permute/commit separately after P5/P7. If remaining eligible complete-chain cost cannot meet 3%, defer. If permutation itself is below the memo's proposed 3-second A3 threshold, evaluate compression-only scope.
- [ ] CODE: tests first for duplicate table values, missing values, blinding rows and exact leftover pop order. Compare every a′/s′ element to the selected CPU algorithm.
- [ ] CODE: implement only the admitted compression/placement subset, with full multiplicity accounting and bounded sorting scratch; retain CPU fallback and checkpoint rules.
- [ ] CODE: run `cargo test -p loki-gpu --features cuda --test lookup_differential`, `lake build LokiLean.BackendLookup`, W2 and complete-chain gates.

**Success — CODE:** exact lookup columns/order and stock acceptance. **Measured success — ESTIMATE gate:** ≥3% repeated matched chain gain for the admitted scope. **Kill:** only valid-but-differently-ordered results where exact order was promised, low residual cost, or slower overall proofs. The memo's ≤3-second lookup phase is a hypothesis, not a delivered target.

## P9 — Measured batching, precompute, or concurrent-heavy admission (conditional)

**Owner:** runtime/performance engineer; safety and Lean reviewers remain independent. **Files — CODE:** `crates/loki-gpu/src/{runtime,srs,msm}.rs`, matching CUDA units, `tests/scheduler_contract.rs`, new `benches/chain_throughput.rs`; orchestrator module from P7. Keep experimental configuration opt-in; add features only for a separately reviewed behavior change.

**Interfaces — CODE:** existing generation-scoped leases and reservations; no new transcript API. Batched arithmetic returns original slot order. Fixed-base tables are defined by explicit powers of two and consume accounted persistent memory. Concurrent jobs remain independent ready DAG nodes.

- [ ] CODE: collect actual live allocation lifetimes and service intervals at k19/k20. Include nonpool allocations, context/module overhead, host pinned/RSS and worst-case simultaneous scratch; free-memory snapshots alone are insufficient.
- [ ] CODE: select one hypothesis at a time: phase MSM batch, shifted-base table, or second-heavy admission. Add missing mathematical/lifetime obligations before kernels or scheduling changes.
- [ ] CODE: test out-of-order completion, cancellation, starvation bounds, forced pressure, stale generations and exact transcript order. Verify complete chains, not only kernels.
- [ ] CODE: compare repeated chains/hour and p50/p95 ready-to-accepted latency against P7 at the same CPU/device budget, including cold initialization/precompute amortization.

**Success — ESTIMATE gate:** at least 10% throughput improvement for concurrency/precompute while p95 single-chain latency regresses ≤5%, with all correctness and memory gates. These are proposed admission policies, not measurements. **Kill:** no improvement, unsafe memory overcommit, starvation, unbounded reservations, or benchmark gains from skipped dependency/verification work. Keep one-heavy P7 policy if any gate fails.

## Review, verification, and handoff

CODE: the submitted skeleton must pass `cargo check --workspace`, CUDA feature compilation and linked stub checks, and `lake build` with all four new modules imported. Record actual results in REVIEW.md; do not confuse these with future implementation gates. Every Opus package adds meaningful negative/differential tests before implementation, runs the named targets and full applicable stock gates, gets independent review, and records its exact commit and outstanding assumptions.

CODE: PLAN READY means the specification, skeleton and implementation sequence are consistent and committed. It does not mean kernels, Loki integration, GPU performance, or whole-chain acceptance have been implemented. The coordinator launches Opus work from this plan; astra-gpu remains available on #lokigpu until STOP.
