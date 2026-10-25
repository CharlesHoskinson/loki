# Design

## Context

See proposal.md for the motivation. Existing `LOKI-DONE.md` defines correctness/performance goals, `docs/MERGE-GATES.md` separates merge and native release evidence, and `TODO.md` records 92 open obligations. None alone establishes an exhaustive v1 release decision. This change supplies that decision contract; implementation and detailed checks remain owned by the area changes.

## Goals / Non-Goals

Goals: exhaustive scope accounting; one explicit promotion decision per milestone; immutable, replayable evidence; independent author/scout review; honest unsupported and deferred states.

Non-goals: running release commands in the planning lab, publishing packages, implementing missing target support, or converting conditional model claims into unconditional Rust claims.

## Decisions

### Qualification is a manifest over owned evidence

Use one immutable source manifest and a feature coverage ledger. Each record identifies owner, change/requirement, source revision, target/feature configuration, stock package checksum, fixture digest, verifier command, result and artifact digest. The coordinator checks completeness; owners implement the gates. Separate source and evidence commits avoid circular hashes. A script-based checker is proposed work, not an existing executed command.

Alternative: one monolithic test command. Rejected because it obscures target availability, source drift and distinctions between proof/model/runtime evidence.

### Contracts freeze first; acceptance follows the dependency DAG

The roadmap's M0–M6 gates and change registry are authoritative. QA/docs infrastructure proceeds alongside implementation; final receipts consume the integrated source. `deps.yaml` owns actual merge order, while older GPU design prerequisites explain conceptual dependencies. Native receipts follow the merge train and cannot be waived at release.

Alternative: native evidence before every merge. Rejected because branch receipts become stale and GPU lock contention prevents unrelated CPU checks; existing repository policy already separates them.

### Required targets ship CPU support within measured envelopes

WASM and mobile gates run stock/Loki on target and measure resource envelopes. GPU features remain opt-in and cannot leak into those builds. WebGPU/Metal/Vulkan/AMD/Intel arithmetic is deferred; this does not defer CPU target support. Unknown budgets fail closed until measured and frozen.

Alternative: include every accelerator in v1. Rejected because unqualified kernels multiply the arithmetic, lifetime and hardware evidence burden without satisfying the immediate CPU platform requirement.

### Product release identity remains compatible

The project tag is v1.0.0 while the required patched package retains `midnight-proofs` 0.8.3. Package/publication identity remains an open owner decision. Additional registry distribution requires an independently approved identity. Existing registry versions cannot be replaced, and exact-version consumers cannot be silently excluded.

### Formal evidence retains explicit boundaries

No proof holes/new custom axioms enter the release Lean project. External kernel/compiler/runtime assumptions remain named hypotheses. Required shipped-feature obligations cannot be relabeled as passing tests; actual-source model replay and native receipts are complementary evidence. gpu-spec proof holes are not silently dropped during merge. If P6 remains deferred, split/extract its segmented-only declarations and transitive hole-dependent source while retaining the nonsegmented BackendMsm/P3 contracts. The entire release source and built library remain hole-free. Bind merged Lean SYNCED-LOKI-COMMIT and MAIN_REV/release drift to the manifest LOKI_RELEASE_SHA; historical a256a9a/877114d/eaf09d8 pins are inventory only, and pin edits require fresh citation, coverage, build and trust evidence.

## Referenced change registry

The following list must equal all active v1 changes, including this umbrella. The fourteen IDs have been reconciled against the active on-disk changes; archive directories are excluded.

- `v1-compat-api-contract`
- `v1-compat-proof-wire-matrix`
- `v1-compat-proof-stock-matrix`
- `v1-wasm-cpu`
- `v1-mobile-cpu-compat`
- `v1-gpu-core`
- `v1-gpu-qualification`
- `v1-fuzz-components`
- `v1-fuzz-continuous`
- `v1-quint-model-coverage`
- `v1-lean-component-refinement`
- `v1-perf-benchmarks`
- `v1-docs-release-readiness`
- `v1-release-umbrella`

## Risks / Trade-offs

- Formal closure has uncertain duration → start it alongside target probes; retain an estimate, not a deadline promise.
- Missing physical runners → explicit M2 failure, never compile-only substitution.
- Stale historical evidence → exact source/digest checking and separate historical labels.
- Duplicate thresholds across areas → each detailed contract has one owner; umbrella consumes its result.
- Optional scope creep → deferred work requires a new reviewed change and full QA/docs coverage before re-entry.

## Migration Plan

First freeze the planning registry and author/scout reviews. Implement manifest/coverage checks, then populate evidence through the coordinator's release loop in the roadmap. Existing release artifacts remain historical. If a check fails, retain the candidate as unqualified and rerun after correction; no publication happens in this lab.

## Owner Decisions Before Execution

Select named platform runners and measured workload budgets at M2; assign sustained formal owners; choose any additional registry distribution identity. These are explicit later milestone decisions, with pass/fail procedures already specified; they do not authorize weakening the release contract.
