# Design

## Context

See `proposal.md` for motivation and `specs/lean-component-refinement/spec.md` for externally observable release gates. Current Lean source pins Loki at `a256a9a`; the separate GPU worktree is based on `877114d`; Loki main was `eaf09d8` at research time. T09 requires updating the model/source correspondence at release (`/home/charles/repos/loki-proofs/TODO.md:39-41`). `scripts/drift.py` has hard-coded stock/main revisions and does not currently accept an exact release revision (`/home/charles/repos/loki-lean/scripts/drift.py:41-45`). The GPU worktree has open `sorry` proofs at `BackendMsm.lean:282` and `BackendQuotient.lean:182`, so those holes must be closed or the affected claims excluded before merge into a release tree.

The current component inventory distinguishes proved, hypothesized, missing, and N/A coverage (`/home/charles/repos/loki-lean/COVERAGE.md:7-21`). The whole-prover equality theorem is conditional on `Refines` and `SameOrder` (`/home/charles/repos/loki-lean/LokiLean/Refinement.lean:853-876`); open model assumptions remain in `SPEC.md:87-88`. The GPU branch audit already names areas whose runtime/code correspondence is limited (`/home/charles/repos/loki-lean-wt/gpu-spec/AUDIT.md:412-423,466-488`).

## Goals / Non-Goals

**Goals:**

- Make the release SHA a shared input to Lean source pinning, citation drift, coverage, and archived proof evidence.
- Keep formal claims, explicit hypotheses, external tests, and unsupported components distinguishable.
- Preserve host-only Lean proof CI as an independent gate from CPU compatibility runs on WASM and mobile targets.
- Require the GPU specification to be reviewed and present in the same release-pinned Lean tree.

**Non-Goals:**

- Prove stock 0.8.3 sound or formally verify Rust, CUDA, blst, sppark, drivers, or Lean itself.
- Change Loki production code, public API, default features, or target implementation.
- Treat compilation, fuzzing, Quint model checking, or cross-verification as a substitute for a Lean proof.

## Decisions

1. **Freeze one source SHA at release preparation.** The docs-release/release job publishes `LOKI_RELEASE_SHA`; it is the sole Loki source baseline for the merged Lean tree, `MAIN_REV`, `SYNCED-LOKI-COMMIT`, and all release citations. The GPU worktree's `877114d` value is historical merge input and every GPU row is reconciled against the same `LOKI_RELEASE_SHA`. Stock 0.8.3 remains a separate immutable oracle pin. Alternatives considered: keep each branch's own source pin or use a moving `main` tag; either makes the combined release evidence ambiguous or mutable.

2. **Preserve per-citation history while adding explicit release resolution.** Extend `drift.py` with an explicit release revision input and a deterministic JSON/text report. Keep recorded historical tree hashes so stock citations remain stock citations; do not use the legacy single-`--base` mode for all entries. Require a reviewed decision for every citation and replace guessed source-tree baselines with explicit tree hashes before allowing a clean gate; the scout's report found 353 guessed trees and two unresolved external/draft paths. The gate blocks unresolved changed, moved, unlocated, ambiguous, mismatched, bad, or missing citations. Alternatives considered: reset every citation to the release tree (loses meaningful historical baseline) or leave drift review manual (not reproducible).

3. **Use status-qualified component coverage.** At the frozen tree, compare Rust module/feature/target inventory with Lean coverage. Each row has a stable ID, source revision, theorem/contract, feature and target applicability, open assumptions, and supporting empirical gates. `proved`, `hypothesized`, `missing`, and justified `N/A` retain the definitions already in `COVERAGE.md`. An unresolved premise affecting shipped behavior blocks release; a hypothesis may be accepted only with precise scope, owner decision, and linked independent test evidence. Alternatives considered: infer coverage from imports or theorem count; neither demonstrates Rust correspondence.

4. **Keep theorem and oracle evidence distinct.** Lean checks `loki_proof_eq_stock` under explicit component and trace-order premises. Compat-proof publishes the supported fixture matrix and independently checks proof/key serialization and verifier interoperability against exact 0.8.3. The current harness covers k10, k14, and k17; it must not be represented as covering cases not in its matrix (`/home/charles/repos/loki-proofs/loki-harness/tests/differential.rs:19-32`). Proof-byte equality applies when lookup map order matches; other accepted map orders are checked for verifier acceptance. Alternatives considered: claim theorem compilation verifies Rust (incorrect) or rely only on empirical compatibility tests (does not discharge the formal contract).

5. **Audit proof trust from the Lean environment and a reviewed baseline.** Pin `axiom-audit` by immutable revision in `lakefile.toml`, run `lake build`, the upstream `lean4checker` CI action, and the allowlist-based environment audit. The allowlist is exactly `{propext, Classical.choice, Quot.sound}`; reject `sorryAx`, native-evaluation axioms, and custom axioms, and preserve `AxiomSweep.lean` per-theorem output. Validate the tool on Lean v4.34.1 with a negative-control proof before release; run `lean4checker` on the same pinned runner as the build and record elapsed time. If the auditor cannot run with the pinned toolchain, the existing sweep becomes a blocking equivalent. Alternatives considered: grep source text (misses aliases and produces comment false positives) or accept a clean build (does not reveal theorem axioms). The Lean Reference recommends `lean4checker` in CI: https://lean-lang.org/doc/reference/latest/ValidatingProofs/.

6. **Merge GPU specification before final pinning.** The GPU owner reviews a merge/rebase from `gpu-spec`, resolves semantic conflicts, proves each shipped proof obligation, and preserves coverage/audit notes before synchronizing to the immutable release manifest. If P6 is deferred, its sorry-bearing file must be absent from both the release source tree and built `LokiLean` library; a row marked deferred cannot leave a hole in the release project. P3/P4/P5/P7 claims need their relevant Lean contracts; external kernel correctness remains a named hypothesis with GPU-owned tests; GPU runtime lifetime claims remain outside Lean where documented. CPU compatibility and proof jobs do not require GPU hardware. Alternatives considered: leave GPU specification separate (release auditors can miss it) or merge blindly (risks dropping corrected limitations).

7. **Keep platform validation in platform-owned jobs.** Lean coverage records which Rust modules and cfg/feature branches affect WASM browser/Node and iOS/Android, but actual CPU compile/runtime proof interoperability belongs to the WASM/mobile and compat areas. Lean proofs run on pinned host CI and do not gate on WebGPU, Metal, Vulkan, or physical mobile devices. This avoids duplicating their target matrices or letting GPU infrastructure block the CPU contract.

## Risks / Trade-offs

- [The model-to-Rust bridge can remain weaker than theorem names imply] → Record assumptions beside each row, require independent stock/fuzz/Quint evidence, and block claims whose premise is neither proved nor accepted as an explicit release limitation.
- [Citation parser may misidentify a Rust tree or line range] → Keep per-entry hashes, make release SHA explicit, emit all unresolved rows, and require reviewed resolution before synchronization.
- [GPU merge can change theorem assumptions or baselines] → Review coverage and generated axiom report after merge; re-run build/drift at the frozen tree.
- [A host Lean CI pass says nothing about target compilation or runtime limits] → Keep target-aware coverage links and require the separately owned WASM/mobile CPU gates.

## Migration Plan

1. Receive the exact release SHA and stock 0.8.3 oracle identity from release/compat owners.
2. Merge and review the GPU Lean worktree; refresh component coverage and Rust citations at the candidate tree.
3. Build and audit the full Lean project; close every proof hole, resolve each source drift row, and assign every module a coverage status.
4. Run independent compatibility, fuzz, and Quint evidence at the same candidate SHA; archive the Lean outputs with the release evidence.
5. Re-run the same gates after any source change. Roll back release readiness by invalidating the evidence bundle if the SHA changes; do not reuse reports from the prior tree.

## Open Questions

None for the gate contract. Owner-provided release SHA, artifact location, and cross-area command names are integration inputs, not deferred design decisions.
