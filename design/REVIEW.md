# Design review and verification record

CODE: this records the design-team review of the API, Unsupported Rust/CUDA skeleton, Lean specification, and implementation packages. It does not claim implemented GPU arithmetic, integration into Loki, stock acceptance of new-backend proofs, or new performance measurements. Unless explicitly labeled otherwise, review findings below are CODE observations or proposed contracts.

## Review disposition

| Finding | Resolution |
|---|---|
| Loki main was treated as already stock-default equivalent | P1 now audits all default differences and restores stock lookup-map traversal; deterministic sorting has an independent opt-in feature. Independent stock HashMaps are not a universal byte-repeatability oracle. |
| Public borrowed async DMA relied on a destructor waiting | Public v1 operations block through completion; private staging owns DMA storage. Future async APIs must remain safe when completion objects are forgotten. |
| Errors allowed partial public effects or implied lost residents could fall back | All errors preserve public logical values and publish no result. Private state may poison a generation. CPU fallback requires a complete retained host checkpoint. |
| Generic trait bounds implied unsupported curves could reach native code | Stable field/curve/wire identities are explicit capabilities; concrete Loki adapters validate them and encode fields without layout casts. |
| Numeric resident/SRS handles could be mistaken for authority | Metadata is private but explicitly constructible as untrusted data. Every use and release requires live registry validation of generation, type, extent, representation, allocation and lease. |
| Unavailable runtime counters looked like measured zero | Internal unavailable counters are `None`; public runtime/generation calls return an error until initialized. Reservations are authoritative admission, snapshots observational. |
| Transform direction and root could be inverted twice | Requests always carry the canonical forward root for the full transform extent; inverse normalization occurs exactly once. |
| Recursion qualification added implied off-circuit verification to every production stage | Qualification stock-verifies all stages. Production timing retains exactly the driver’s existing checks and reports extra qualification separately. |
| cc build selected nvcc as the host compiler | Build validates the toolkit, sets process `NVCC` before `cc::Build::new()` (child-only `Build::env` does not select it in cc 1.10), then uses `.cuda(true)`, shared cudart and explicit targets. A linked three-symbol stub test checks the ABI scaffolding. |
| Existing W10/W13 feature names would pull legacy CUDA into the new backend | Preserve legacy aliases, name the new registry `loki-gpu-srs-registry`, extract neutral `loki-argument-commit`, and reject direct legacy flags with the new backend. |
| NTT range predicates did not imply the total-function call contract | Requests and results now have finite exact lengths; kernel predicates construct the call contract and actual stock-backend replacement. Lead independently reviewed the transferred revision. |
| Quotient model omitted rotation stride and differed from Rust register semantics | It now models finite column shapes, signed scaled rotations, all six opcodes, initialized overwrites and an initialized output register. |
| Quotient interpreter was disconnected from the stock call, and a total helper could hide row failure | Compiled artifacts carry explicit source/compiler contracts; Option traversal fails on any row error, and its actual call constructor composes to stock bytes. |
| Generic Lean error preservation was an unused premise | Fallback now consumes the returned public state; preservation is required to prove equality. A concrete mutating-error counterexample demonstrates the need. |

CODE: each Luna received a read-only cross-review assignment recorded in OWNERSHIP.md. Nine responded; gluna-6 remained unavailable and its generic Backend review was recovered by gluna-1. Findings and rechecks are in the nine available `design/gluna-N/PEER-REVIEW.md` files. gluna-5 delivered an initial review/draft, then became unresponsive to correction requests; lead transferred the blocking NTT revision explicitly to gluna-3 and independently reviewed it. The lead additionally reviewed the shared API, every design draft, operation skeletons, source boundaries and theorem assumptions. Final review-ring completion and build evidence are recorded below.

## Verification evidence

| Check | Result and receipt |
|---|---|
| MEASURED: `cargo check --workspace --locked -j3` | Exit 0 through host runner; latest frozen native-source check log `target/gpu-runs/20261024T233221-gluna8-final-default-check.log`. This checks the separate skeleton workspace, not Loki integration. |
| MEASURED: `lake build` in gpu-spec, all four modules imported | Exit 0, 8939 jobs. Linter warnings and the two declared theorem placeholders remain. [Receipt](verification/lean-build.txt). |
| MEASURED: targeted `#print axioms` audit | Exit 0. Generic dispatch equality is axiom-free; generic, MSM-batch, NTT and compiled-quotient stock-byte composition uses only standard Lean axioms. [Full audit](verification/lean-axioms.txt). |
| MEASURED: Lean runner commit | `9a32d2f`, “Specify GPU backend contracts and stock refinement”; five exact files. |
| MEASURED: CUDA build and linked three-symbol stub check | Both exit 0 through runner using a temporary toolkit-only Cargo configuration, removed before commit. [Build log](verification/cuda-check.txt); [test log](verification/cuda-stub-test.txt): 1 passed, 0 failed. NVCC 13.4 V13.4.92, rustc 1.99.1. No device opened. |
| CODE: GPU repository commit | This reviewed tree is submitted through the host runner; its resulting hash is recorded in the coordinator handoff. |

CODE: the final new modules contain exactly two `sorry` bodies, both in explicitly marked theorem obligations, and none in definitions:

| Declaration | Obligation and consequence | Implementer gate |
|---|---|---|
| `BackendMsm.segmentedValue_eq_pippengerList` | `msm-segmented-partition`: prove exact sort/segment reindexing into unsigned Pippenger. `segmentedKernelDispatch_meetsMsmSpecific` inherits `sorryAx`; the independent contractual batch-to-stock-byte theorem does not. | P6 must discharge the partition proof before claiming verified segmented arithmetic. |
| `BackendQuotient.valid_program_eval_some` | `quotient-interpreter-safety`: prove static validation guarantees every row has initialized operands/output. Valid-path safety corollaries inherit `sorryAx`; source-simulation and compiled call/stock-byte composition do not. Runtime Option traversal remains fail-closed. | P5 must discharge interpreter safety; independently construct the compiler simulation fields from actual emitted programs. |

CODE: `quotient-compiler-lowering`, `quot-inputs`, the existing stock-trace-faithfulness boundary, native arithmetic/wire conversion and resource-lifetime correctness remain explicit implementation obligations/hypotheses. They are not silently discharged by a successful `lake build`. The compiled quotient theorem assumes a request-associated `CompiledQuotient` artifact and the stock-input binding; native code must earn those contracts through the P5 proof/tests.

## Limits carried into implementation

- CODE: the mathematical model is not an extraction or verification of Rust, CUDA, FFI lifetime rules, byte encodings, or the production transcript implementation. The existing stock-trace-faithfulness obligation remains explicit.
- CODE: kernel contracts and host compiler/register-allocation contracts must be discharged by their assigned implementation packages. Differential tests and independent unmodified stock verification are mandatory even when abstract Lean theorems build.
- OPEN QUESTION: concurrent-heavy scheduler refinement is not formalized. P7 starts with one heavy proof, explicit reservations and trace tests; P9 concurrency requires new proof/lifetime and measurement gates.
- MEASURED (historical only): baseline values are sourced in PROVENANCE.md and TEST-PLAN.md. No new speedup is claimed. Matched complete recursion-chain measurements determine promotion.
- CODE: no third-party implementation source was copied into the submitted skeleton. Public-project research is context; source-derived bellperson scout details are explicitly excluded as normative inputs. Any future permitted sppark copying requires preserved headers and NOTICE updates.

## Independent P2 review — d71b7ca, 2026-10-25

CODE: astra-gpu reviewed the merged runtime, native ABI, FFI visibility,
transfer/publication paths, registry checks, cleanup and P2 evidence against
RUNTIME.md. The implementation supplies substantial runtime scaffolding, but
**resident arithmetic promotion was blocked at that revision** by the
findings below. The correction status is recorded in the later P2-fix section. Existing
39-test and sanitizer passes do not cover these cases. P4 may develop kernels
in isolation; integration approval requires the P2 corrections first.

| ID | Finding and evidence | Required correction/gate |
|---|---|---|
| D1 — blocking | MEASURED: two public `GpuRuntime::with_driver` instances start allocation IDs and generations at 1. Equal device/epoch/type/extent metadata lets runtime B accept A's handle and return B's different data. | P2 follow-up: bind identities to the actual owning runtime and enforce native coordinator uniqueness. Cover foreign allocations, releases, checkpoints and reservations; handle-counter exhaustion must fail rather than recycle. |
| D2 — blocking | MEASURED: healthy `Drop` ignores a newly sticky stream-sync failure and continues device/event/host destruction. Partial-init cleanup likewise ignores a sticky destroy failure and continues. Fake driver records destruction through poisoned context in both cases. | P2 follow-up: check teardown results, stop unsafe destruction immediately and quarantine uncertain resources. Tests must inject errors at each cleanup boundary, including initialization failure; preserve the distinction between safe retirement and uncertain destruction. |
| D3 — blocking | MEASURED: sticky `EventElapsed` error is discarded; upload still returns `Ok(ResidentBuffer)` and keeps metadata healthy. | P2 follow-up: timing is optional telemetry, but sticky errors are mandatory health transitions. No output publication after such failure. Cover upload/download/launch timing and setup/record/query paths. |
| D4 — blocking | CODE: public `ffi::NativeDriver` and `native_driver()` expose safe reset/copy/free methods outside the coordinator. Driver handles are not bound to their issuing driver; a fake-created pageable HostBuffer can reach native `std::free` with a fake raw ID. This dangerous native route was not executed. An unsafe trait declaration constrains implementations, not safe calls. | P2 follow-up: restrict the native driver/factory to the coordinator or provide an enforced exclusive unsafe boundary and driver-bound handles. No safe caller may reset an active runtime or submit a foreign/freed handle. Add public-API compile-fail/ownership gates without provoking unsafe hardware faults. |
| D5 — integration blocker | CODE/SOURCE: native device selection occurs only in probe/reset, while later operations may run on other host threads. CUDA current device is thread-local. The shared host's ordinal-0 checks do not qualify configured nonzero devices. | P2 follow-up before multi-threaded/native promotion: bind or verify device on each entry or serialize through a dedicated device thread. Test host-thread handoff and selected-device identity; do not infer coverage from the global Rust mutex. |

SOURCE: D5 follows NVIDIA's [device-management contract](https://docs.nvidia.com/cuda/cuda-runtime-api/cuda_runtime_api/group__CUDART__DEVICE.html)
and [multithreading guidance](https://developer.nvidia.com/blog/cuda-pro-tip-always-set-current-device-avoid-multithreading-bugs/).
The other findings are direct code observations or local test results.

MEASURED: host-runner `cargo test -p loki-gpu --locked -j3 --test
review_regressions -- --nocapture` against `d71b7ca` produced four expected
regression failures, exit 101, without CUDA or device access. The second run
completed in 0.215 s. [Source](verification/p2-review-regressions.rs), SHA256
`a54cf66021ab1c841383889532b8e488f1cfdd353c9921acbc539f1b4abce8ba`;
[raw log](verification/p2-review-regressions.txt), SHA256
`39554c2337fffa0a5449ab99679ac3174cc7a1662178790d07cd3fdea86b53ce`.
To reproduce on that revision, copy the source to
`crates/loki-gpu/tests/review_regressions.rs` (using existing `tests/support`)
and run the command. The source is retained as review evidence outside the
normal test discovery path; correction owners must incorporate the cases into
passing regression tests. Their removal from automatic discovery does not
close the findings.

CODE (API decision): add `ErrorKind::Cancelled`. A cooperative stop before
submission is `NotStarted`; a stop after successful drain/retirement is
`InputsPreserved`. Failed synchronization/free must retain the actual error
category and `SessionPoisoned`; cancellation does not authorize automatic CPU
retry. There is no native ABI or enum-layout serialization change. P1/P2 have
been notified. P2 follow-up owns changing its current `Internal` mappings and
cancellation tests; no native runtime implementation was edited by this review.

### P2 open-risk assignments

CODE: numbering below follows the bullet order in [P2's handoff](p2/REVIEW.md).
The first ten requested risks are all mapped; its two additional gate/model
notes are retained as R11/R12. An assignment is a requirement, not a claim the
risk is resolved.

| Risk | Owner package and promotion gate |
|---|---|
| R1 — cancellation classification | API variant accepted in this review; P2 follow-up maps successful cancellation and tests failure-precedence. P7 propagates cancellation without CPU restart. |
| R2 — generic point upload unsupported | P3 implements the concrete curve wire adapter with subgroup validation and exact SRS identity; no `resident_buffers` claim implies point support. |
| R3 — canonical scalar bytes versus Montgomery kernel representation | P4 defines checked representation transitions and independent round trips; P6 reuses the field wire contract. Never reinterpret P2 bytes as Montgomery limbs. |
| R4 — WSL2 pinned limit not reproduced | P7 records actual per-host staging and host-memory peaks; P9 alone may propose ring expansion. Keep bounded fallback and distinguish the reported earlier 4 GiB success from the later lock-timeout rerun. |
| R5 — curated sticky classification, no real hardware fault injection | P2 follow-up audits native classification and all error paths (D2/D3). P3–P7 add operation-specific fake failures; an isolated hardware qualification may be scheduled later, never on the shared production GPU. No hardware fault test is claimed. |
| R6 — reset destroys other native CUDA state | P2 follow-up enforces exclusive native ownership and closes D1/D4/D5 before P3/P4 integration. P1 retains legacy/new-runtime feature exclusion; P7 must not reuse a poisoned generation. |
| R7 — download holds encoded and decoded copies | P7 measures and accounts complete host-memory peaks; P4 may reduce staging copies with correctness evidence. P9 concurrency cannot assume a one-copy host budget. |
| R8 — stale build/runtime docs and dev lockfile changes | Lead updates BUILD.md/RUNTIME.md in this review. P2's dev dependencies were already transitively locked; default check remains required. |
| R9 — abandoned checkpoint retention across poison | P2 follow-up supplies validated host-only discard, or P7 requires process termination rather than repeated recovery until it exists. P7 soak tests must prove checkpoint budget returns after aborted sessions. Never drop a recoverable checkpoint implicitly on a failed operation. |
| R10 — pageable staging retained after reset | P2 follow-up supplies cleanup only after reset/completion proves it safe; P7 repeated reset/abort tests bound cumulative host memory. The existing per-reset 128 MiB bound is not a process-lifetime bound. |
| R11 — native test must require a device | P4/P6 GPU qualification preserves default device-required `ffi_abi`; `LOKI_GPU_ALLOW_NO_DEVICE=1` is only for explicit toolkit-only checks and cannot count as GPU evidence. |
| R12 — Lean runtime abstraction omits native lifetime/bytes | P2/P3/P4/P7 retain explicit native-lifetime, encoding and scheduler obligations plus fault/sanitizer gates. Abstract proofs at `21f208d` do not close D1–D5. |

CODE (additional scheduler interface gap): P2 public reservation tokens are
not consumed by upload/launch, which reserve independently. P7 must connect
admission to allocation ownership (or a scoped proof budget) before promising
that an admitted job can allocate. A snapshot or release-before-allocation
sequence is not atomic admission. P2's budget invariant alone does not prove
this progress property.

CODE (P1 review follow-up): `886a9fb`/`be63386` restore stock lookup traversal
and add the CPU seam; no additional arithmetic/fallback blocker was found in
source review. `e63e893` closes the self-skipping default-feature sentinel
finding with an unconditional manifest-default gate. The resolved transitive
feature graph remains an independent gate. P1 reports W2, stock-verifier,
byte-equivalence and feature-tree passes in `LOKI-P1-SEAM.md`; raw evidence
paths have been requested before final review sign-off. SOURCE (coordinator): P1 merged into Loki main `a256a9a`; the coordinator
selected sibling-relative `../loki-gpu/crates/*` dependencies, with a
`loki-wt/loki-gpu` symlink for worktree builds. A pinned git dependency is
planned once remotes exist. This resolves the immediate absolute-path issue;
it is not a published-package dependency arrangement.


MEASURED (post-API-change compatibility): lead host-runner
`cargo check --workspace --locked -j3` exits 0, and
`cargo test -p loki-gpu --locked -j3 --test runtime_contract` passes 39/39.
[Check receipt](verification/p2-api-check.txt);
[contract receipt](verification/p2-existing-contracts.txt).
These passes do not close D1–D5. No runtime implementation was changed in this
review; P2 follow-up owns their fixes and cancellation mapping.


CODE (P1 audit / side-channel item): the pre-existing
`poly/mod.rs::batch_invert_rational` optimization branches on whether a
witness-derived denominator equals one. Skipping those inversions preserves
field values and proof encoding but adds data-dependent host work relative
to stock. No constant-time claim is made for this path or inferred from the
stock-verifier passes. P7's host/prover integration owner must document the
accepted timing threat model and escalate any required constant-time mode to
the coordinator; proof-byte equality alone does not resolve this item.


## P2-fix source review — 09fd92a / merge eb27233

CODE: astra-gpu independently reviewed `09fd92a` after the coordinator merged
it as `eb27233`. The original four reproductions are preserved byte-for-byte
as executable `tests/review_regressions.rs`; the broader
`tests/p2_fix_regressions.rs` covers foreign release/duplicate/reservations/
checkpoints, cleanup boundaries and timing faults.

| Finding | Disposition |
|---|---|
| D1 | Original collision closed by process-global checked allocation/reservation/generation counters. Exhaustion returns Capacity instead of wrapping. IDs are acquired before device work. |
| D2 | Original setup/Drop destruction-through-poison cases closed by stopping cleanup at the first failed destruction or synchronization; uncertain resources are retained. This does not close the separately tracked abandoned-checkpoint/pageable-reset leaks. |
| D3 | Original optional-timing publication bug closed: the shared optional-call handler ignores only transient/unsupported failures and aborts/poisons on sticky errors. Upload, download and duplicate use it. |
| D4 | Public native-driver escape closed: NativeDriver/factory are crate-private. `GpuRuntime::native(config)` owns an exclusive native claim; `process()` uses the same route. Native minted-handle checks provide additional defense. Public API compile-fail cases and native single-owner tests are recorded by the implementer. Callers still must use runtime allocation IDs; the private native pointer set is not a substitute for registry identity. |
| D5 | Per-thread device binding implemented at all native entry points. One error-preservation follow-up remains assigned to P4: `ensure_device` currently discards a failed `cudaGetDevice` result before trying `cudaSetDevice`; propagate the first error immediately. The single-GPU tests do not qualify nonzero-device switching. |

CODE: cancellation now uses `Cancelled` on the requested-stop path; failed
retirement/free uses the actual failure kind and poisons. No additional API
layout or native ABI change was needed.

MEASURED (implementer evidence): native ABI tests 13 passed/1 ignored, CUDA
unit tests 4 passed, privacy doctests 4 passed, sanitizer suites exit 0.
[Native receipt](p2/verification/p2-fix-ffi-abi.txt),
[sanitizer receipt](p2/verification/p2-fix-compute-sanitizer.txt).
No real sticky fault was injected on the shared GPU. The implementer explicitly
notes that a single ordinal-0 device cannot detect regression of device
selection on another ordinal. The Lean model at `21f208d` is unchanged.

SOURCE (coordinator): main merge `eb27233` independently passed 4 original
regressions, 10 expanded regressions and 39 existing runtime cases. Lead's
own runner rerun also passed all 53 cases (exit 0, 1.067 s execution after
487.093 s queueing), with no CUDA feature: [raw log](verification/p2-fixed-regressions.txt). The coordinator handed
runtime.rs/ffi.rs/native runtime/header ownership to P4 at this merge; P3 must
send its point-wire seam through that owner or wait for explicit transfer.
R9/R10 (repeated recovery leaks), reservation redemption and the abstraction
limits remain open under their assigned package gates.
