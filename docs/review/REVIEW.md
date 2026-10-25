# Documentation review decisions

Initial review baseline: Loki `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16`, loki-gpu main `d71b7cafd4308206cc7f283b07a4e67baae89305`, and loki-lean `a177d9114482911e082fe6fe4a3257e5d77fe4e5`. This review checks documentation against source and existing artifacts. No builds, GPU runs or theorem checks are represented as newly executed here. During review P1 merged, and the coordinator fast-forwarded the documentation worktree to `a256a9a15ff18ae12a8131b86cff576787fa5e3c`. Current-state prose uses that revision; historical measurements keep their original pins.

## Decisions

Reader reports retain the original observations and evidence; historical line references describe the text before edits. Screenshot feedback is in [the coordinator report](coordinator-haiku-arch.md). Reader reports: [r1](luna-r1.md), [r2](luna-r2.md), [r3](luna-r3.md), [r4](luna-r4.md), [r5](luna-r5.md), [r6](luna-r6.md), [r7](luna-r7.md), [r8](luna-r8.md), [r9](luna-r9.md), [r10](luna-r10.md). Overlapping findings are consolidated below. An accepted decision authorizes a documentation correction; implementation follow-ups remain in [TODO](../../TODO.md).

| ID | Decision | Concern or suggestion | Resolution | Owner |
|---|---|---|---|---|
| C01 | accept | README quick start omits command working directories and first proof route. | Name the Loki root, consumer patch context and independent harness. | r1 |
| C02 | accept | Toolchain and CUDA prerequisites are distant from setup. | Put declared versus validated Rust versions and nvcc visibility next to setup. | r1 |
| C03 | TODO | Public checkout URL and a validated minimal consumer example are missing. | T01-T02: publish a canonical remote and execute the consumer example. | release/integration |
| C04 | accept | CUDA toolkit variables and architecture values differ between adapters. | Document MSM numeric 120 versus NTT sm_120 and the supported toolkit variables. | r1/r3 |
| C05 | accept | Feature tables omit optional tabbycat. | Include the upstream optional feature in both tables. | r1/r4 |
| C06 | accept | Proof-byte equality, verifier acceptance and transcript equality are conflated. | State tested acceptance and exact equality assumptions; retain lookup-order limitation. | r1/r2/r5/r10 |
| C07 | accept | Mixed committed/plain multi-proof inputs trigger the inherited stock rejection defect. | Document the triggering shape beside compatibility claims; track protocol work in T03. | r1/r2/r5 |
| C08 | accept | Lean model refinement is presented as implementation-wide correctness. | Name Refines, SameOrder, equal inputs/randomness/threads/map order and code-binding gap. | r1/r2/r9/r10 |
| C09 | accept | W7 mutation rejection is stated without its MockProver condition. | Explain satisfying mutants and the short-input campaign limitation. | r1/r9 |
| C10 | TODO | G2 includes stock 0.8.1 and k20 beyond the recorded default harness gate. | T04: obtain independent verifier artifacts before marking these cases complete. | compatibility |
| C11 | accept | Multiopen parallelism has separate coefficient and set thresholds. | Document coefficient parallelism at length >=4096 and multi-set work threshold separately. | r5 |
| C12 | accept | unsafe_setup is public, despite its production prohibition. | Replace test-only API wording with public utility and production prohibition. | r5 |
| C13 | accept | utils::multicore is advertised but not declared by utils/mod.rs. | Describe multicore.rs as an uncompiled retained source file, not an importable module. | r4 |
| C14 | accept | Test package lockfile statement is inaccurate. | Identify gpu-compat and ntt-stock lockfiles; stock-profile-verifier lacks one. | r6 |
| C15 | accept | P2 runtime status is stale across both repositories. | Pin GPU d71b7ca and Loki a256a9a: P1/P2 merged, P4 in progress; new arithmetic remains unsupported. | r1/r3/r10 |
| C16 | accept | Pinned staging is described only as a plan. | Describe implemented P2 pool separately from unmeasured overlap and resident arithmetic still in development. | r3/r10 |
| C17 | accept | Platform diagram depicts planned backend integration as current. | Show the merged optional P1 seam and legacy CUDA path; mark unimplemented resident arithmetic; request screenshots. | r3 |
| C18 | accept | GPU fallback diagram promises unqualified same proof bytes. | Use the scoped stock-verifier acceptance contract; request screenshot review. | r3 |
| C19 | accept | Serial and parallel SRS upload validation are conflated. | Scope serial validation to legacy cache and parallel validation to cuda-srs-registry. | r3 |
| C20 | accept | CUDA runtime module line count is stale. | Remove the volatile source line count. | r3 |
| C21 | accept | W11 headline omits later mixed whole-proof measurements. | Report controlled run 2 phase reductions 12.83%/19.00%, whole-proof -2.74%/+0.11%, with fixture/statistic. | r1/r7 |
| C22 | accept | W10 evidence is not directly navigable from the README table. | Link a checked-in evidence summary; preserve A3 prove-plus-verify, branch and single-sample scope. | r1/r3/r7 |
| C23 | reject | Root-product division could be claimed as a Loki invention. | midnight-zk PR #565 is explicit prior art; retain attribution, no algorithm priority claim. | lead/r8 |
| C24 | accept | Axiom PR #37 is cited as root-product precedent. | Attribute it specifically to parallel inner products; cite Midnight #565 for product division. | r7 |
| C25 | reject | GPU Halo2/KZG proving or CUDA MSM/NTT could be claimed as first-ever work. | Axiom, Ingonyama and sppark establish prior art; keep only evidenced project-specific contributions. | lead/r8 |
| C26 | TODO | Exact Midnight version compatibility priority is not established. | T05: compare closest public implementations by version, curve, verifier and output conditions. | research |
| C27 | TODO | One W10 A3 sample does not establish complete-chain improvement or priority. | T06: repeated controlled complete-chain measurements and public comparison. | performance |
| C28 | accept | Identity page overstates equality and its platform reference is non-navigable. | Define same proof as compatibility intent, keep the canonical tagline, link scoped evidence and ../PLATFORMS.md. | r10 |
| C29 | accept | Platform source labels and unverified public claims need traceability. | Link known local sources; verify public claims or move them to TODO without invented URLs. | r8 |
| C30 | accept | Module index has no recommended reading path. | Link proof API, runnable examples and independent harness as starting points. | r4 |
| C31 | reject | Public unsafe_setup visibility or multicore exposure should be changed in this docs review. | Source/API changes are outside scope; documentation now records the actual API. | lead |
| C32 | accept | Prose contains decorative labels, dashes, stale status and attribution to automated reviewers. | Apply whole-file humanizer, preserve factual artifacts and remove attribution to automated reviewers. | all owners |
| C33 | accept | The quotient estimate of about 26 GB at k22 has no located derivation. | Remove it from the overview; T07 requires a buffer-by-buffer calculation. | r10 |
| C34 | accept | P1 merged during review, changing defaults, dependencies and module coverage. | Document a256a9a, four new features, stock lookup default, sibling layout and backend module; preserve historical pins. | r1/r2/r3/r4/r5/r6/r7/r9/r10 |
| C35 | TODO | Rational inversion branches on witness denominators equal to one. | T08: perform a deployment-specific side-channel review before any constant-time claim. | security |
| C36 | TODO | Lean source synchronization and named refinement obligations remain open. | T09-T21 separately track model/source, quotient, MSM, runtime and cache obligations; no implementation proof claim. | formalization |
| C37 | accept | Architecture says all four Quint models replay real implementations. | Limit implementation replay to implemented components; scheduler remains model-only. | r2 |
| C38 | accept | Architecture diagrams still show P1 features and dependencies as planned. | Update layers, features, pipeline and recursion against current main; request screenshots. | r2 |
| C39 | accept | The feature diagram excludes label covers an edge and pipeline labels are cramped. | Reposition labels, clarify CPU/GPU NTT and recheck rendered diagrams. | r2 |
| C40 | accept | Recursion diagram arity-4 explanation lacks a checked source. | Remove the unverified arity/wrapper footnote; make no unsupported fixture claim. | recursion/docs |
| C41 | TODO | P1 records remaining cancellation, lockfile and W10 integration gaps. | T28,T36,T38 track these separately; do not treat compile-only backend checks as acceleration. | runtime/integration |
| C42 | accept | Platform wording implies W10 measured a whole recursion chain. | Identify W10 as one A3 parent; complete-chain measurement remains T06. | r8 |
| C43 | accept | Historical profile estimates can read as current uncontended measurements. | Keep the 117.3 s outside-create_proof estimate attached to its overlapped-run provenance. | r2/r7 |
| C44 | accept | The conditional theorem also pins the abstract backend extensionally to stock. | State Refines.eq_stock and the shared-program composition explicitly in NOVELTY; no deployed-code inference. | lead |
| C45 | accept | The architecture feature table and graph mix a proposed portable argument flag into the actual W10 dependency. | Read the branch manifest: cuda-argument-commit enables only cuda; separate the proposed loki-argument-commit flag and remove the false edge. | lead/r2 |

## File ownership

The table records fix-phase ownership. After each Markdown handoff, the lead took ownership for final consistency and typography corrections. Diagram owners retained their files through screenshot review. The lead owns this file, `docs/NOVELTY.md` and root `TODO.md`.

| Owner | Files |
|---|---|
| r1 | `README.md`, `loki-harness/README.md` |
| r2 | `docs/ARCHITECTURE.md`, `LOKI-DONE.md`, `LOKI-HARDEN.md`; diagrams `pipeline.svg`, `layers.svg`, `correctness.svg`, `recursion.svg`, `features.svg` after author handoff |
| r3 | `docs/GPU.md`; module pages `cuda-runtime`, `cuda-kernels`, `cuda-ntt`, `loki-gpu`; `gpu-fallback.svg`, `platforms.svg` |
| r4 | Module index and pages `circuit`, `dev`, `examples`, `utils`, `backend`; `LOKI-P1-SEAM.md` |
| r5 | Module pages `plonk`, `plonk-arguments`, `poly`, `poly-domain`, `poly-msm`, `poly-kzg`, `poly-multiopen`, `transcript` |
| r6 | Module pages `benches`, `tests`, `tests-stock`, `loki-harness`, `fuzz`, `profile`, `scripts`, `probes` |
| r7 | `LOKI-CPU.md`, `LOKI-GPU-MSM.md`, `LOKI-GPU-NTT.md`, `LOKI-MULTIOPEN.md`, `LOKI-PROFILE.md`, `LOKI-SRS-REGISTRY.md` |
| r8 | `docs/PLATFORMS.md` |
| r9 | `LOKI-QUINT.md`, `LOKI-FUZZ.md`, `docs/modules/quint.md` |
| r10 | `docs/brand/IDENTITY.md`, `CHANGELOG.md`; loki-gpu `README.md`, `docs/OVERVIEW.md` |

## Review outcome

The ledger contains 45 consolidated concerns, questions and suggestions: 35 accepted corrections, 3 rejected proposals and 7 decisions requiring follow-up. The accepted Markdown corrections are applied. [TODO](../../TODO.md) expands unfinished work into 93 items, each with an owner role and acceptance gate. [NOVELTY](../NOVELTY.md) retains four evidenced project contributions and makes no first-ever priority claim.

All ten reader reports are present. Each file received a whole-file humanizer pass, followed by the lead's consistency pass. Fenced command and evidence blocks in every historical `LOKI-*.md` note match the current source revision's copies; prose corrections retain the original measurements and source pins.

## Verification

The final static audit checked 61 Markdown files across both documentation worktrees. All 433 local file targets and 18 Markdown section anchors resolve. The prose scan found zero em/en dashes and zero curly quotation marks. All nine SVGs parse as XML. Whitespace checks pass in both repositories. No build, GPU job, fuzz campaign or theorem check was run during this review.

The audit covers the root README, TODO, changelog, engineering notes, harness README, `docs/*.md`, module pages, brand guide, review reports, and loki-gpu README/overview. Historical planning files, legal license text and read-only design references remain source material. External citations were checked during the relevant source reviews; the local-link audit does not claim a complete external-site availability test.

Screenshot verification for the seven modified diagrams is handed to the coordinator under T93 in [TODO](../../TODO.md). VERIFY-REQUEST messages identify the current files, including the last W10 dependency correction; no final visual pass is claimed here. Initial visual findings are retained in [the coordinator report](coordinator-haiku-arch.md).
