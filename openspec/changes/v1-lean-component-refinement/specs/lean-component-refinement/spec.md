# Spec Delta

## Purpose

Defines the release-pinned Lean evidence required to account for Loki components and relate their behavior to stock midnight-proofs 0.8.3. It makes source drift and proof trust assumptions visible release inputs.

## ADDED Requirements

### Requirement: Release-pinned component coverage
The release Lean evidence MUST account for every Rust module and changed semantic path with a stable coverage row ID, source citation, feature gate, target applicability, and evidence status at the frozen release SHA.

#### Scenario: Every release component has an evidence status
- **WHEN** the coverage gate runs against the release SHA
- **THEN** the new `python3 scripts/coverage_gate.py --release "$LOKI_RELEASE_SHA"` passes only if the Rust inventory matches `COVERAGE.md` rows, each row has an ID/status/citation/feature gate/target field, and every applicable WASM/mobile path links its owner CPU gate; `MISSING` behavior or an uncovered shipped path fails absent a recorded release-owner deferral. It builds on `/home/charles/repos/loki-lean/COVERAGE.md:7-21,23-50`.

### Requirement: Stock prover refinement
The Lean refinement evidence MUST connect component contracts to `loki_proof_eq_stock` against stock midnight-proofs 0.8.3, identify its `Refines` and `SameOrder` premises, and block release for any unresolved premise affecting shipped behavior unless that behavior is explicitly deferred.

#### Scenario: Refinement and stock oracle are checked
- **WHEN** the release evidence is generated
- **THEN** `lake build LokiLean.Refinement` checks `loki_proof_eq_stock` under its `Refines` and `SameOrder` premises, and the compat-proof stock gate checks its published release fixture matrix in both directions (the current harness cases are k10, k14, and k17). Proof-byte equality is limited to matching lookup map order; other admissible orders are checked for stock verifier acceptance. Any open premise affecting a shipped feature blocks release. This builds on `/home/charles/repos/loki-lean/LokiLean/Refinement.lean:853-876`, `/home/charles/repos/loki-lean/SPEC.md:87-88`, and `/home/charles/repos/loki-proofs/loki-harness/tests/differential.rs:19-32`.

### Requirement: Release citation drift gate
The Lean citation check MUST resolve every Rust citation against the exact frozen Loki release SHA and MUST block release for unresolved drift or guessed baselines. Each citation record MUST include its explicit source tree, `reviewed` status, and reviewer decision; unresolved external/draft paths MUST be listed separately.

#### Scenario: Citation check is tied to the frozen release
- **WHEN** the release Lean job checks citations
- **THEN** the new `python3 scripts/drift.py --loki "$LOKI_RELEASE_TREE" --release "$LOKI_RELEASE_SHA" --all` emits a report, reads/writes `reviewed` plus `decision` metadata in `scripts/citations.json`, and returns success only when no guessed baseline or drift row is unresolved; the two current external/draft paths are reported rather than silently ignored. This extends `/home/charles/repos/loki-lean/scripts/drift.py:4-13,41-45,426-465` and fulfills `/home/charles/repos/loki-proofs/TODO.md:39-41`.

### Requirement: No holes or new axioms
The release Lean project MUST contain no `sorry` or `admit` proof holes, MUST add no project axiom declarations beyond a reviewed declaration inventory, and MUST use no transitive axiom outside `{propext, Classical.choice, Quot.sound}`; the audit MUST report named theorem dependencies.

#### Scenario: Proof trust audit passes
- **WHEN** proof CI audits the release tree
- **THEN** `lake build && lake exe axiom-audit --allow propext,Classical.choice,Quot.sound && lake env lean scripts/AxiomSweep.lean` plus an environment declaration-inventory comparison succeeds and publishes per-theorem dependencies; it fails on `sorryAx`, any new project axiom declaration, or any dependency outside the allowlist. It builds on `/home/charles/repos/loki-lean/scripts/AxiomSweep.lean:9-24` and `/home/charles/repos/loki-lean/scripts/audit_gen.py:11-43`.

### Requirement: GPU Lean specification is included
The release Lean source MUST include reviewed GPU-spec changes reconciled to the frozen source manifest and MUST contain no proof holes; conditional, non-shipped files containing holes MUST be absent from the release source tree and built `LokiLean` library, and their feature rows MUST be marked deferred. External kernel/runtime behavior not modeled by Lean MUST remain an explicit hypothesis.

#### Scenario: GPU specification is merged without losing evidence
- **WHEN** the GPU Lean handoff is evaluated at release freeze
- **THEN** `git diff --check && lake build LokiLean && lake exe axiom-audit --allow propext,Classical.choice,Quot.sound` passes on the merged release-pinned Lean tree with no `sorry` source in the release tree or built `LokiLean` library; the holes at `BackendMsm.lean:282` and `BackendQuotient.lean:182` are proved or their entire non-shipped files are absent from the release source and recorded deferred. The old `877114d` GPU pin is merge history and every retained citation is reviewed against the immutable release manifest. Coverage/audit retain the GPU limitations at `/home/charles/repos/loki-lean-wt/gpu-spec/AUDIT.md:412-423,466-488`; CPU gates do not require GPU availability.

### Requirement: Lean CI evidence is reproducible and target-aware
Release proof CI MUST pin Lean `leanprover/lean4:v4.34.1` and dependencies, publish build and trust-audit outputs, and map target-specific Rust paths without replacing WASM or mobile CPU compatibility gates.

#### Scenario: Proof CI records release evidence for target paths
- **WHEN** release CI runs for a candidate SHA
- **THEN** the pinned Lean job publishes `lake build`, `lean4checker`, and axiom audit output for that SHA, while coverage rows identify applicable WASM browser/Node and iOS/Android Rust paths and link their separate CPU gates; it builds on `/home/charles/repos/loki-lean/lakefile.toml:12-18`, `/home/charles/repos/loki-lean/lean-toolchain:1`, and `/home/charles/repos/loki-lean/COVERAGE.md:23-50`.
