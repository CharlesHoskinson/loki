# Design

## Context

See proposal.md for motivation and scope. Existing checks are spread across `quint/check.py`, q1/q2/q4/q5 campaigns, and several MBT crates. The top-level runner does not run all component campaigns or Rust replay; the scheduler model has no implementation; and historical source adapters include local-path assumptions. The design must preserve useful historical reproductions while keeping release evidence attached to current source.

## Goals / Non-Goals

**Goals:** produce one manifest-driven release view of stateful/ordered source changes, mandatory Rust replay for shipped behavior, and auditable host-side model results with explicit bounds and tool pins.

**Non-Goals:** modeling all arithmetic, proving CUDA kernels, replacing stock compatibility or fuzz gates, claiming a planned scheduler is implemented, or using Quint output as WASM/mobile runtime evidence.

## Decisions

1. **Use a version-controlled coverage manifest as the index.** Each source-path row identifies the model/module, invariants, mutation controls, required positive witnesses, Rust replay target, stock fixture IDs, source and tool pins, fuzz target, Lean contract/status, documentation link, and result artifacts. The release checker compares the release diff against this reviewed map and blocks unmapped in-scope paths. Alternatives were an unstructured report (cannot enforce completeness) and deriving semantics from filenames (cannot identify behavior reliably).

2. **Keep model code and component campaigns; add a single release orchestrator.** `quint/ci/` commands run existing campaigns and new adapters, aggregate versioned JSON/log/ITF output, and fail on omitted results. A fast local subset may exist, but it cannot produce a release pass. This avoids discarding current W8/q1/q2/q4/q5 evidence while making the full gate reproducible.

3. **Replay the current release source, not historical source snapshots.** MBT adapters should call public or test-injectable seams from the exact checkout and use compat-proof fixture IDs. Historical snapshot tests can remain labeled diagnostic reproductions, but they cannot satisfy a release row. When no injectable seam exists, report the row blocked until the owner supplies an adapter; a Quint-only result does not close it.

4. **Separate invariant safety, reachability, and liveness evidence.** Bounded exhaustive verification checks finite-state safety when feasible; seeded simulations provide robustness and witness reachability, not proof. Mutation controls check invariant sensitivity. A liveness claim must name fairness assumptions and use a temporal checker that supports them. If the tool cannot check the model, the claim stays blocked. Alternatives that infer progress from sampled `completed` witnesses are rejected because they do not establish eventual progress.

5. **Use stock 0.8.3 as a separate compatibility oracle.** Model branches name stock defaults and Loki-only opt-ins separately. Fixture identifiers and serialized proof fixtures come from compat-proof; Quint owns its event/state model and Rust adapter mapping. The default lookup branch preserves the valid but nondeterministic stock HashMap order; deterministic order is modeled and replayed only as an opt-in. Model agreement alone is never a compatibility verdict.

6. **Keep target lanes independent.** Quint and Apalache run as CPU-only host checks. Platform owners consume the shared compatibility fixtures in wasm/browser/Node and iOS/Android jobs. This avoids requiring CUDA or a model checker on constrained targets and avoids claiming host MBT covers target runtime behavior.

## Risks / Trade-offs

- [Finite abstractions omit real state] → require source mapping review, fixture replay, mutations, and explicit bounds; uncovered behavior blocks release.
- [State spaces grow beyond CI budget] → split by component and keep fixed bounded presubmit configurations; longer campaigns remain separately reported and cannot replace required checks.
- [Compatibility fixtures and model state drift] → fixture IDs are owned by compat-proof; model-to-fixture adapters are tested and recorded in `quint/coverage.yaml`.
- [Historical findings rely on old source revisions] → retain those as diagnostic evidence while requiring the current-source adapter for a release pass.
- [Temporal checker cannot handle fairness] → do not claim scheduler liveness until an appropriate checker and model are available; the existing scheduler model stays planned-only.

## Migration Plan

1. Inventory all current and roadmap source paths that affect covered behaviors; fill the coverage manifest and get component-owner review.
2. Pin toolchains and refactor/create current-source MBT adapters using shared compat fixture IDs; preserve older snapshot campaigns as diagnostics.
3. Add manifest validation, release orchestration, machine-readable output, and separate finite safety/temporal jobs.
4. Wire release CI after CPU, platform, compat, fuzz, Lean, and documentation owners publish their gate interfaces. The coordinator runs the complete gate at the selected release SHA; failures remain blockers.

Rollback disables the new release gate only by marking the v1 release incomplete; it does not change production defaults or remove the standalone component checks.
