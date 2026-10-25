# Design review and verification record

CODE: the initial sections record the design-team review of the API, Unsupported skeleton and Lean specification. Later dated sections review implementation branches and their scoped evidence. They do not establish stock acceptance or end-to-end speedups for the new backend. Unless explicitly labeled otherwise, findings are CODE observations or proposed contracts.

## Current implementation review status — 2026-10-25 10:23 UTC

SOURCE: revisions below are branch-specific; source review is not a claim that
all branches are merged or that the combined prover is qualified.

| Area | Latest reviewed evidence and remaining gate |
|---|---|
| P2/P4 runtime | CODE: P4 `4b2977a` corrects the additional L1/L2/N1/N2 paths, probe classification and native handle guard; `59464c4` moves that guard before every native call and pins the field dependency exactly. MEASURED: the earlier main `eb27233` rerun passed 53 tests. SOURCE: P4 final corrected-source native qualification remains pending. |
| P3 checked MSM dispatch | CODE: Loki `9fe5da0`/`85586f3` returns cancellation without CPU retry; stock-signature shim is explicitly non-cancellable. MEASURED (P3): CPU-only suite 15 passed, cancellation mutant failed. |
| P4 transform dispatch | CODE: Loki `7804fa6` propagates cancellation, admits the exact ABI, rejects results after sticky-disable and settles a failed coset offer once on CPU. MEASURED (P4): CPU-only suite 18 passed and stock-default suite 9 passed. OPEN QUESTION: combined P3/P4 merge gate and feature-on stock differential. |
| P5 quotient | CODE: draft `9d7efb4` corrects Q1/Q3 and the subsequent local-query lock/poison and snapshot-charge findings. MEASURED (P5): fake suite 19 passed; native summary receipt reports the full CUDA suite and three explicit quotient cases passing. OPEN QUESTION: final sanitizer/integration gates and measured qualification of the local-memory estimate. |
| P5 CPU compiler | CODE: Loki `3f964da` addresses domain coherence and enforced certificate validation. Source review found no well-formed term-order mismatch. MEASURED (P5): 11 tests passed, two ignored; final receipt/mutation gates remain separate. |
| P7 scheduler/benchmark | CODE: source findings S1/S2/S3/S5/S6 addressed through `9eee2b0`/`fc4ddb2`; cleanup receipt `44cea53` inspected and its file hashes independently matched. Reservation redemption remains an explicitly unimplemented prototype integration. OPEN QUESTION: corrected-P4 rebase and actual GPU/stock-chain qualification. |
| Lean | CODE: lead independently reviewed closure `0416b73` in components `2a9e9f6`; both original backend statements are retained with substantive proofs. SOURCE: components reports full build and zero `sorry`; this is a separate branch. OPEN QUESTION: reconcile with current gpu-spec modules and run the combined build/axiom audit. |
| Lean SRS | CODE: `49fa4ca`/`ea839e5` specifies sequential prefix/lease refinement with explicit kernel, codec and concurrent-pinning obligations. MEASURED (P3): targeted module build and three-theorem standard-axiom audit passed. |

CODE: R9/R10 repeated-recovery leaks, reservation redemption, actual SRS
submission pinning and native/Lean refinement limits remain under their
assigned package gates. No row above closes them.

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


## Subsequent lifecycle review — 2026-10-25 07:24 UTC

CODE: these are additional findings after the 53-test correction pass; that
pass closes its named reproductions and does not cover the cases below.
P4 owns the shared runtime files and the corrections before resident-transform
promotion. SOURCE: quint-q3 reported L1/L2 through the coordinator; astra-gpu
confirmed the Rust paths independently. The requested Quint trace and model
assumptions are pending, so no model-execution result is claimed here.

| Finding | Source evidence and required gate |
|---|---|
| L1 — failed setup loses uncertainty | In `eb27233`, `initialize` and `reinitialize` store `device.current` only after `build_context` succeeds. Setup cleanup returns `NotStarted` even for sticky failure or failed cleanup. With `current == None`, recovery retries initialization without reset, and `Drop` releases the native claim. Retain an explicit failed/uncertain setup state, preserve the actual failure/effect, require reset before retry, and retain the claim on uncertain drop. Test initial setup failure, cleanup failure and failed rebuild after reset. |
| L2 — injected driver ownership | Public safe `with_driver(Arc<dyn DeviceDriver>)` permits two runtimes to share one resettable driver; a retained alias can also call safe `reset` outside either coordinator. The private native constructor excludes this route for NativeDriver, but the injected-driver boundary does not enforce exclusivity. Constrain ownership or make raw injection an explicitly unsafe/internal hook with precise lifecycle preconditions and a safe restricted fake-control interface. An Arc-address duplicate check alone does not prevent external reset. |
| N1 — release failure hidden by transform helper | In the P4 draft reviewed on this date, `transform_chain` ignores release of the previous intermediate after computing the next output. A failed final release can poison the context while the function returns `Ok` with a resident handle. `transform_host` also ignores final output release. Propagate cleanup failure on the success path; publish nothing after poisoning. Test final intermediate and final host-output release failures, including error precedence when the operation already failed. |
| N2 — representation sampling is not a complete wire contract | The P4 draft infers canonical little-endian representation of arbitrary `PrimeField` implementations from modulus, width and four sentinel values. A different bijective representation can agree on those samples and differ elsewhere. Establish a supported concrete-type/codec boundary or a defined normalization codec before kernel submission. Keep sentinel checks as regression checks, not a universal representation proof. |

CODE: the P4 draft correctly propagates `cudaGetDevice` failure immediately
and binds the NTT entry to the selected device, resolving the narrow D5 source
follow-up once that source is committed/integrated. A smaller error-precedence
issue was also reported: transform allocation cleanup discarded a failed
`device_free` error kind in favor of the earlier allocation error (while already
reporting `SessionPoisoned`). P4 should preserve the cleanup error consistently.

SOURCE: the coordinator paused new native/GPU jobs for W12's reserved window;
CPU/fake-driver work may continue. Native results must identify the final
integrated source, including these corrections and the P3 point-wire seam.


## P4 correction source review and cancellation decision — 2026-10-25

CODE: astra-gpu reviewed P4 `eb0d299` and `4b2977a`. The reported L1/L2/N1/N2
paths are corrected in that branch: failed setup and sticky probe failures
persist uncertainty through retry/drop; failed reset preserves it; raw driver
injection is explicitly unsafe with sole-owner lifetime preconditions; safe
fake controls cannot reset/free/launch; transform cleanup errors prevent
success publication; and NTT accepts the concrete supported field type rather
than inferring its encoding from samples. Native NTT submission now checks
all minted handles. C++ field-offset assertions and matching Rust checks cover
the new NTT params structure. Original D5 device-query error preservation is
also corrected there. This is source-review closure; final native gates on
the corrected revision remain pending, and the earlier `f716257` receipts are
not evidence for these later changes.

CODE (API decision to P3/coordinator): checked MSM dispatch must return
`Result<Curve, BackendError>` and return `Cancelled` without CPU work or
fallback-counter increments. A poisoned effect still disables acceleration.
The stock-signature shim remains synchronous/non-cancellable; unexpected
cancellation aborts/unwinds that call instead of recomputing or inventing a
point. P7 must propagate errors through a cancellable prover boundary before
user cancellation is advertised. The backend trait already returns `Result`,
so this decision requires no backend-API crate signature change. Tests must
cover all cancellation effects and verify no CPU retry.

SOURCE: lean-components `2a9e9f6` reportedly proves the two inherited backend
obligations with a full build and zero `sorry`. That separate-branch report
does not yet establish a merged gpu-spec build; reconcile it with P4/P3
modules and repeat the combined build/axiom audit before updating that claim.


## P5 quotient launch review — draft abc8480

CODE: astra-gpu reviewed the proposed native launch, Rust driver wrapper,
resident validation and resource admission. The independent C++ decoder
checks envelope/arity/canonical unused fields, bounds and define-before-use;
column metadata and native minted-handle checks precede submission. The
following gates remain open before promotion.

| Finding | Required correction/evidence |
|---|---|
| Q1 — temporary host copy lifetime | `NativeDriver::launch_quotient` creates a temporary constants byte vector. C enqueues constants and program copies, but returns immediately if the second copy or synchronization fails. The constants vector then drops before the Rust runtime can drain; sticky failure may prevent a completion proof entirely. Use runtime-owned bounded staging retained through completion/reset, including uncertain-failure quarantine. Test a queued first copy plus failed second copy, failed synchronization, and caller descriptor destruction. Successful-path synchronization alone is insufficient. |
| Q2 — implicit local-memory resource admission | Scratch accounting covers constants and instructions, while dynamically indexed `fr_t regs[R]` uses local memory (implementer reports up to 8208 bytes/thread) and the grid can reach 2^20 blocks. Bound/account kernel local/stack backing or use explicit budgeted workspace and a capped grid-stride plan. Establish a conservative bound/headroom policy; no exact CUDA backing-allocation formula is asserted here. |
| Q3 — caps checked after validation allocation | `evaluate_resident` calls `validate_program`, allocating a boolean vector of `register_count`, before checking the native 256-register cap. Check native resource caps before allocating validator state, so malformed large counts return a bounded error. |

SOURCE: NVIDIA says pageable asynchronous transfers may synchronize with the
host; this is not a guarantee that temporary host buffers may be dropped on
all error paths. [CUDA synchronization behavior](https://docs.nvidia.com/cuda/cuda-runtime-api/api-sync-behavior.html).
The documented `localSizeBytes` attribute is per thread; it is not a complete
allocation-budget guarantee. [CUDA function attributes](https://docs.nvidia.com/cuda/cuda-runtime-api/cuda_runtime_api/structcudaFuncAttributes.html).
CODE: the by-value column pointer table avoids retaining the host handle
array; its large kernel-parameter requirement must remain tied to the supported
CUDA/toolchain target. Header shape alone does not approve the launch lifetime.


## P5 correction review — 9d7efb4

CODE: astra-gpu reviewed `daec08e`, `b655e34` and `9d7efb4`. Q1 is corrected
by staging bytecode/constants through runtime-owned slots; the native launch
reads caller arrays synchronously for validation only. Slot reuse waits for
retirement, recoverable pre-wait errors drain the copy stream, and uncertain
errors retain staging and quarantine private device outputs. Q3 native caps
now precede validator allocation.

CODE: follow-up review found that the new local-memory query ran outside the
device lock and could return `SessionPoisoned` without changing runtime health.
It also found that snapshots omitted the retained local-memory charge.
`9d7efb4` moves the query under the device lock/outside the registry lock,
poisons on uncertainty and includes the charge in both snapshots' reserved
bytes. Regression cases cover refusal until reset, repeated-launch accounting
and rebuilding the context. MEASURED (P5): removing the poison transition or
snapshot charge makes regression cases fail.

ESTIMATE: native admission now charges a context-wide high-water estimate,
`round_up(localSizeBytes + 1 KiB, 1 KiB) * SMs * maxThreadsPerSM`, in addition
to configured device headroom. This is implementation guidance with margin,
not a CUDA API upper-bound guarantee. [Context-wide local memory](https://forums.developer.nvidia.com/t/local-memory-management/289339),
[stack allowance and allocation granularity](https://forums.developer.nvidia.com/t/memory-required-for-kernel-launch/39558).
OPEN QUESTION: qualify that estimate on CUDA 13.4/sm_120 with measured memory
changes, and align trait/native wording so it does not claim an already-proved
upper bound or completed qualification.

MEASURED (P5; summary receipt inspected by lead, not a lead execution):
[`native-9d7efb4`](verification/p5-native-9d7efb4.txt) records `HEAD 9d7efb4
dirty=0`, passing CUDA suites and quotient oracle cases. Synthetic shape replay
at 2^19 rows is pointwise-equal to the CPU interpreter; the 2^22-row replay is
**timing-only**. Neither is a real A3 witness/stock-numerator qualification or
an end-to-end recursion result. The reported local-memory estimate is 510 MiB
for R=8. Final receipts should include commands, toolchains and clean build
input binding before/after build and run.

SOURCE: P5 disclosed that its native runs occurred during the coordinator's
W12 hold and then acknowledged no further native jobs until RELEASE. W12
confirmed at 08:51 that its final fixed-seed CPU/GPU pair was starting, with
no release yet; the earlier P5 completion was relayed for provenance. This
review does not treat an available lock as permission to bypass that window.

## Runtime formal counterexample mapping

SOURCE/CODE: quint-q3 supplied `quint-runtime` commits
`eaf8f0a`/`53385e0`/`172b348`, based on `eb27233`, with
`crates/loki-gpu/tests/quint_findings.rs`, `quint_replay.rs`,
`quint/runtime.qnt` and `quint/logs/`. Lead read the reproductions and relevant
logs. The two sticky-setup sequences and shared-driver reset sequence match
L1/L2. Native-claim release after failed setup is source inspection, not a
real sticky fault injected on the shared device.

MEASURED (q3 logs): the old model has counterexamples for recoverability and
stale shared-driver use. Its candidate shell-context fix reports no violation
in 10,000 sampled traces and recovery in 994; absence of a sampled violation
is not a proof of the current Rust code. CODE: P4 uses a separate
`setup_failed` state, so formal closure requires an explicit mapping. The old
safe shared-driver sequence is excluded by P4's unsafe injection precondition;
do not wrap that old test in unsafe and call the resulting violation a current
safe-API bug. Preserve old-revision evidence and use compile-fail/current safe
construction coverage when integrating it.

## P5 CPU compiler review — Loki 3f964da

CODE: lead read the compiler, register allocator, independent allocation
checker, host binder and evaluator loops at `40d267e`, then the corrections
at `3f964da`. No arithmetic or term-order mismatch was found for well-formed
inputs. The review requested two enforced boundaries: domain metadata must
match before conversion, and certificate validation must actually run at the
compile/submission boundary. The correction makes compiled fields private,
verifies every compilation, checks base/extended domain logs, extent and
rotation scale before binding, and revalidates the certificate on binding.
The checker rejects inconsistent logical-register counts before allocation.

CODE: this earns source approval for the CPU-only compiler slice, not a proof
of compiler correctness or native integration. MEASURED (P5 report): the
release compiler suite passed 11 tests with two ignored. Raw source/feature
receipts and guard mutation results remain final integration evidence.

SOURCE/CODE: the differential oracle is the source-audited Loki
`crate::plonk::evaluation::Evaluator`, not an externally invoked private stock
function. The stock 0.8.3 evaluator file hashes to
`b5fde24d4203d1d026f535dd908d071a49b3c469d80129f3ff8394add2a89beb`;
Loki's `40d267e` file hashes to
`5f41fd851a795b6ac1d85661530b4ba006cd1324808d139a6c9e192beb69dfe6`.
The inspected differences are optional hardened rotation indexing, profile
instrumentation and hardened tests. The reported compiler tests use default
committed-instances plus `loki-backend-api`, with hardening off. P5 corrected
the provenance wording. External stock-verifier acceptance remains a later
whole-proof gate.

## P7 initial review — Loki 18adf0f

CODE: findings below concern benchmark qualification and scheduler integration,
not a demonstrated proof-verification mismatch. They were reported to P7 and
the coordinator; corrections and receipts are pending.

| ID | Finding | Required disposition |
|---|---|---|
| S1 — high qualification risk | `bench/control/src/main.rs` leaves block status `COMPLETE` and exit 0 after arms marked `FAILED`, `TIMEOUT` or `INVALID`. Existing fixtures deliberately assert this collector behavior; VK mismatch is also only recorded. | Separate collection completion from qualification success. Unusable arms/required VK mismatch must be visible to an automated gate through explicit qualification status and nonzero exit. Continuing collection is compatible with failing qualification. |
| S2 — high qualification risk | `bench/adapter/driver.rs` records telemetry but never requires successful GPU dispatch or zero fallback/error for G0 samples. The separate `loki-harness::require_dispatch` does not cover this executable. | Enforce per-stage accelerated-operation evidence and zero fallback/error, with CPU-only and failure negative controls, before a GPU performance sample is accepted. |
| S3 — cleanup gap | Normal group cleanup can leave `orphans_after_kill` nonempty yet retain `ACCEPTED`. Launcher SIGKILL only triggers direct-child PDEATHSIG; the fixture's `sleep 1000` grandchild is not covered by its arm-PID-only assertion. | Invalidate surviving-process cases, test the saved descendant PID, and supply reliable group cleanup or explicitly narrow the guarantee. Close the pre-exec parent-death race by checking expected parent identity after arming PDEATHSIG. |
| S4 — integration open | `HeavyPermit` holds a P2 reservation but offers no redemption path to operations that independently reserve their own allocations. Reserving a full workspace can exclude that same job's allocations. | Establish an enforced reservation-consumption scope or explicitly label the scheduler as an admission prototype until runtime integration. Do not silently double-count workspace. |
| S5 — contract open | `complete()` publishes `Completed` even if release returns `Stale`/`Retained`; the stale-generation test calls `check_current()` then `fail()`, not completion. | Define and enforce whether completion requires current healthy generation or an independently verified host artifact. The caller obligation must not be hidden behind an unconditional completion claim. |
| S6 — timing scope | `run_chain` starts its chain timer before the children and includes all three additional stock-verifier requests before stopping it. | Label that value qualification-inclusive and report production-stage time separately, retaining the production driver's existing checks. Extra qualification overhead must not silently dilute the primary recursion-speed metric. |

CODE: S1's required VK comparison is cohort-specific. A same-VK experiment
must control key artifacts or establish deterministic construction. Independent
stock lookup-map traversal is not universally byte-deterministic; do not sort
the default path to make this gate pass. Stock acceptance and exact child-key
binding remain mandatory independently of that measurement policy.

CODE: scheduler tests target the earlier safe `with_driver` API. When rebasing
onto corrected P4, port ordinary fake-runtime tests to `with_fake`/`FakeControl`;
do not bypass the new exclusive-lifecycle boundary with blanket unsafe blocks.

SOURCE/CODE: the benchmark verifier calls registry `midnight-zk-stdlib` 2.3.5
`batch_verify`, whose inspected implementation requires full transcript
consumption and invokes the final accumulated KZG verification. Its separate
manifest pins registry stock `midnight-proofs =0.8.3`, with disclosed
`circuit-params`. This source check is not a newly executed chain acceptance
receipt. The verifier additionally checks stage linkage and carried pairing.

## Lean SRS review — 49fa4ca / ea839e5

CODE: lead read exact-prefix refinement, acquisition/eviction/budget proofs,
dispatch composition and concrete examples. The model explicitly assumes
resident kernel correctness and codec agreement. Its acquire-kernel-release
step is atomic; it does not prove concurrent native submission lifetimes.
`ea839e5` adds the requested `OBLIGATION(srs-prefix-kernel)`,
`OBLIGATION(srs-codec-agreement)` and `OBLIGATION(srs-submission-pinning)`
markers, with no proof change. The last explicitly leaves quint-q4 F5 open.

MEASURED (P3 report): `LEAN_NUM_THREADS=2 lake build LokiLean.BackendSrs`
succeeded (8932 jobs); `dispatchSeq_eq`, `acquire_sound` and
`prefixHit_msm_eq` report only `propext`, `Classical.choice`, `Quot.sound`.
This targeted receipt does not replace the pending combined gpu-spec/components
build and axiom audit. Preserve `ea839e5` and P4's `7d3e6a8` during reconciliation.

## Further correction reviews — 2026-10-25 10:15 UTC

CODE: P4 `5ce8862` adopts the quint-q3 sticky-setup cases as regression tests
and restates the shared-driver case over safe, independently owned fake
runtimes. Lead reviewed the port. The old invalid shared ownership sequence
is not hidden inside an unsafe block. The two known leak tests retain their
open-risk status. A compile-fail check for safe invocation of `with_driver`
was requested: independent-fake behavior alone would not detect accidental
reintroduction of safe raw-driver injection. MEASURED (P4 report): no-CUDA
suites pass; corrected-source native qualification remains pending.

CODE: lead read the `9a32d2f..2a9e9f6` differences for `BackendMsm` and
`BackendQuotient`. Closure `0416b73` retains both original statements. The
segmented proof reduces exact partition/permutation sums to the radix digit
identity. Interpreter safety follows from an initialized-register invariant
over instruction prefixes, with read-before-write semantics. No substitute
axioms or assumptions were introduced in those differences. This source
approval does not discharge the separate native, compiler or lifetime contracts.

### P7 corrections — 19637e3

CODE: lead reviewed `25618ff`/`19637e3` and gave scoped source approval:

- S1: collection completion is distinct from qualification; failed/invalid
  arms and a violated explicit VK policy yield nonzero exit.
- S5: completion requires current healthy generation and clean reservation
  release; otherwise the job fails and descendants become unavailable.
- S6: qualification-inclusive chain time is labelled explicitly, with
  production-stage time and extra stock-receipt time separately reported.
- S4: the scheduler now declares itself an admission prototype with no
  reservation redemption. This is an honest scope correction, not completed
  runtime integration.

CODE: S2 now rejects missing telemetry, any fallback/error and wholly CPU
stages, but permits a stage with no MSM acceleration if it has NTT success
and another stage supplies MSM success. The follow-up requires both expected
operations in each eligible A2/A3/DEC stage, with negatives for an otherwise
positive chain whose parent has zero MSM or zero NTT success. Counters cover
stage keygen/proof work; they are not proof-only counters.

CODE: S3 adds a subreaper, escape detection and parent-identity checks after
arming PDEATHSIG. Follow-up review found unchecked subreaper setup, blocking
`waitpid(..., 0)`, only five adoption sweeps, and then unbounded reader join.
Cleanup needs a deadline, nonblocking reap and explicit remaining-descendant
reporting, including a deeper escaped-tree regression. The SIGKILL guarantee
is now limited to the known driver/verifier pair; actual pair coverage remains
to be demonstrated separately from the arm-only fixture.
SOURCE: [Linux PDEATHSIG semantics](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html)
support the fork-inheritance and setup-race findings.

MEASURED (P7 report, not lead execution): failure/timeout, VK-policy mismatch,
escape and stale-completion regressions were red before the corrections and
green afterward. Final source-bound receipts and live forced-GPU-error
qualification remain pending. No full P7 readiness approval is implied.

SOURCE: `bench/README.md` now records the adapted Moriarty source files and
revision `f2ea193777897869c2e45c08601cc412e00c3897`. Lead inspected that
revision's workspace Apache-2.0 declaration and root LICENSE; it has no root
NOTICE. The review copied no implementation source.

### P7 source-review closure — 9eee2b0 / fc4ddb2 / 44cea53

CODE: `9eee2b0` requires both MSM and NTT success in each eligible recursion
stage, checks subreaper setup, and bounds post-exit descendant cleanup with
nonblocking reaping and EOF-conditional reader joining. Remaining descendants
or missing EOF fail qualification. The post-exit deadline does not promise
successful cleanup of an unkillable process; survivors are reported explicitly.

CODE: the first deep-tree regression killed recorded PIDs before checking
survival, masking a possible cleanup failure. At lead's request, `fc4ddb2`
records and asserts the survivor verdict before fixture cleanup, with RAII
cleanup on panic/timeout. MEASURED (P7 report): the targeted rerun passed 1/1;
the preceding launcher suite passed 13 tests plus 10 unit tests, and dispatch
qualification passed 7 tests. These are owner results, not lead executions.

MEASURED: lead inspected `44cea53`'s
`bench/campaign/receipts/sigkill-20261025T102035Z.txt` and independently matched
all three current-file SHA-256 values to the receipt:

| File | SHA-256 |
|---|---|
| Controller | `3b7b76a0b3c480b68d592be25c55ec8078dd44255ec7614ff24d485e3294baf3` |
| L0 driver | `39b4e5c968a804c79043e4969cff09ee7957506069fd2120d949c5a5056dc6cf` |
| Process-test script | `e8481d854a963dbc3aa8f9dd191d12da89315be87a7c36dcc7d85c39510b105c` |

SOURCE/MEASURED (P7 receipt): the real driver plus its fake verifier worker
both died after controller SIGKILL, exit 0; no proof or GPU work was performed
by that case. Source findings S1/S2/S3/S5/S6 are addressed. S4 reservation
redemption, the safe-fake API rebase on corrected P4, combined tests and actual
GPU/stock-chain qualification remain separate gates. This is not full P7
integration completion or an end-to-end performance result.

### P7 reservation scope proposal — 7263c15, revision 4

CODE: lead reviewed successive design-only revisions in P7's
`loki-gpu/design/p7/RESERVATION-SCOPE.md`. Revision 4 proposes a scoped facade
over existing runtime traits, atomic reservation-to-allocation accounting,
explicit retained-output promotion, and a close barrier for active operations.
Its healthy-close and failed-close examples conserve the original grant,
including CUDA local-memory backing that remains charged to the context.
This is a proposal; no runtime reservation redemption was implemented by this
review, and S4 remains an admission prototype.

OPEN QUESTION: four refinements remain before source implementation:

- Store quarantined reserved/live components explicitly, make poison
  idempotent, and include every global accounting category in the partition.
- Release the condition-variable predicate mutex atomically when waiting;
  notify on reset and recheck generation/state after reacquiring locks.
- Give each job its own non-generic Dispatcher policy and counters. Scoped
  failures must not permanently disable the global dispatcher for later jobs.
- Use bounded scope records and documented generation-first stale-token
  classification instead of an unbounded tombstone history.

SOURCE: P7 acknowledged these items on AgentBus at 11:02 UTC and reported the
coordinator's freeze of revision 4 pending P4's runtime API review. Further
revisions are paused. A specification-first ledger/lifecycle gate was proposed
for a later implementation package; no Lean owner or implementation was
assigned by this review. Native lifetime and kernel refinement obligations
remain separate from a bookkeeping proof.

### P4 runtime advice on the reservation proposal — 2026-10-25 11:47 UTC

CODE: P4 reviewed revision 4 against the actual runtime, and lead checked the
transform allocation/retirement, release and P5 charge paths. The proposal
omits successful scratch retirement: scratch must move from scope in-flight
bytes back to credit once freed. Abort settlement must not refund it twice,
and operation retirement must decrement `active_ops` once per call.

CODE: current operations hold the device lock from charge through settlement.
Close can therefore validate and complete under one device-lock critical
section, without condition-variable waiting or retain pins. The scope's Open
state must be rechecked at the charge linearization point. A nonzero active
count observed by close is an invariant failure. Any future operation that
drops this lock before settlement requires a new concurrency contract. This
recommendation supersedes the proposed wait/pinning refinements above.

CODE: today's allocation-failure paths release internal reservation counters
before poisoning, and `release_impl` removes live/checkpoint records before
attempting native free. Quarantined raw memory is not retained in those global
budget counters. The recommended minimal design therefore states global
accounting/admission invariants for healthy generations only, preserves a
scope-local conservation ledger through poison, and prohibits spending until
reset. Scoped and unscoped release should share the existing ordering. This
supersedes the proposed requirement to preserve poisoned global charges;
changing that ordering would be a separately sequenced shared-runtime change.

CODE: checkpoints and retired pageable staging use separate host-memory
accounting. Open must read the grant from the validated reservation record;
token metadata is untrusted. IDs are process-global and interleave between
runtimes. Documented generation-first stale classification can reject old
metadata without an unbounded tombstone map or invalid ID-range inference.
Per-job Dispatcher policy/counters remain recommended. These are review
recommendations pending the coordinator's draft-freeze decision.

CODE: P4 `d23a119` adds an unsafe-constructor compile-fail doctest and a matching
positive unsafe control. Lead reviewed both snippets. The existing execution
receipt was requested; source review alone is not a test-pass claim, and final
corrected-source native qualification remains pending.
