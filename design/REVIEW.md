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
