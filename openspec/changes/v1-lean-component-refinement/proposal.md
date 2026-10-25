# Proposal

## Why

Loki has a substantial Lean model, but its component inventory and citations are pinned to earlier Loki revisions, and several model-to-Rust assumptions remain open. T09 requires synchronizing the Lean source pin and archiving check output at the released source revision; v1 needs that synchronization to be a reviewable release gate rather than an informal maintainer step.

## What Changes

- Define a component coverage contract for each Rust module and changed behavior in the release tree. Record proved scope, hypotheses, missing obligations, and justified N/A rows; require every changed semantic path to be accounted for.
- Tie the whole-prover stock refinement, including `loki_proof_eq_stock`, and its component contracts to a frozen Loki release SHA and stock `midnight-proofs` 0.8.3. Keep open proof/model assumptions explicit and blocking where they affect the compatibility claim.
- Make Rust citation drift a release gate. Run the drift checker against the exact release revision, review all changed/moved/unlocated citations, and archive its report with the Lean build and axiom audit. The current checker has a hard-coded main revision and a legacy `--base` option (`scripts/drift.py:4-13,41-45`), so the release workflow must support explicit release-SHA checking without losing per-citation historical baselines.
- Merge and reconcile the `gpu-spec` Lean worktree into the release-pinned source before the release audit; retain its GPU contracts, hypotheses, and uncovered runtime obligations.
- Add pinned Lean CI that builds the complete spec and checks for `sorry`, `admit`, and introduced axioms, while reporting the standard Lean kernel axioms used by named theorems.
- For every component, link differential fuzzing and Quint/MBT evidence where applicable; neither empirical evidence nor successful compilation may be presented as a proof of the Lean-to-Rust correspondence.
- Update the Lean coverage, source-pin, drift, and proof-CI documentation. This is planning only; no Lean proof or repository build is claimed as run by this change.

## Capabilities

### New Capabilities

- `lean-component-refinement`: release-pinned component coverage, stock refinement, citation-drift, GPU-spec integration, and proof/axiom CI gates.

### Modified Capabilities

- None. The v1 workspace has no existing OpenSpec capabilities (`openspec list --specs --json` returned an empty list).

## Impact

- Lean source and evidence: `/home/charles/repos/loki-lean/LokiLean/*.lean`, `SPEC.md`, `COVERAGE.md`, `AUDIT.md`, `SYNCED-LOKI-COMMIT`, `scripts/drift.py`, `scripts/citations.json`, and `scripts/AxiomSweep.lean`. Current coverage defines evidence statuses at `COVERAGE.md:7-21`; its component inventory starts at `COVERAGE.md:23`. The stock-refinement theorem is at `LokiLean/Refinement.lean:853`, with `lokiMainBackend_refines` at line 1195. The current open assumptions are listed in `SPEC.md:87-88`.
- Source synchronization: T09 is defined in `/home/charles/repos/loki-proofs/TODO.md:39-41`. At research time Loki main was `eaf09d8`, while the Lean `SYNCED-LOKI-COMMIT` was `a256a9a` and the separate `gpu-spec` worktree was pinned at `877114d`. These three revision references require reconciliation against the final release commit. A scout's read-only drift report found unresolved rows against `eaf09d8`; the report was not independently rerun in this planning pass.
- Compatibility: this change adds no Rust API or default behavior. Lean's claim remains refinement relative to stock 0.8.3, not a proof that stock itself is sound. Compat-proof owns bidirectional stock proof/key/transcript checks; Lean consumes the same stock version and release SHA.
- WASM and mobile: Lean CI runs on the host and does not substitute for `wasm32-unknown-unknown` browser/Node or iOS/Android CPU build/runtime gates. Component contracts and citations must still identify target-specific Rust paths; CPU proof gates do not depend on GPU availability.
- QA and docs: require pinned full `lake build`, a no-`sorry`/no-`admit`/no-new-axiom audit with theorem axiom output, release-SHA drift review, and linked differential fuzz plus Quint/MBT evidence. Update the Lean coverage/audit/source-pin instructions and release evidence index.
- Dependencies: compat-proof supplies the stock 0.8.3 oracle and bidirectional fixtures; docs-release supplies the final release SHA and release CI evidence destination; GPU owns the `gpu-spec` handoff and GPU assumption names; fuzz and Quint consume component identifiers and the same pinned source revision.
- Risks: a model theorem can pass while its Rust correspondence assumptions are false; drift tooling can misclassify citations when it guesses release tags; merging the GPU worktree can expose stale citations or regress coverage. A clean Lean build alone is not evidence that the Rust implementation refines the model.
