# haiku-v1-8 LEANR2: re-review of v1-lean-component-refinement (read-only)

`openspec validate v1-lean-component-refinement --strict` run by me: "Change 'v1-lean-component-refinement' is valid".

Checked against the 9 points in my LEANR1 review. All of them are addressed in the revised text:
- (a) both GPU sorries named (`tasks.md:6`, spec "GPU Lean specification" scenario);
- (b) one release SHA for merged tree, `MAIN_REV`, `SYNCED-LOKI-COMMIT` (`tasks.md:5`);
- (c) `coverage_gate.py` and `drift.py --release` marked new (`tasks.md:10`, `tasks.md:16`);
- (d) `citations.json` fields `tree`, `reviewed`, `decision` (`tasks.md:16`, spec drift scenario);
- (e) 353 guessed baselines and both external/draft paths triaged (`tasks.md:16`);
- (f) stock harness claim limited to compat-proof's matrix (k10, k14, k17) (spec stock scenario);
- (g) project declaration inventory separated from transitive allowlist audit (spec "No holes or new axioms");
- (h) target applicability and stable coverage row IDs (spec coverage requirement);
- (i) open premises blocking unless owner-accepted with linked evidence (tasks 2.2).

## Remaining nits (none blocking)

1. **Exclusion must mean removal from the release build.** `specs/.../spec.md` "GPU Lean specification" allows "explicit scope exclusion of the current holes", and the "No holes" requirement says the release Lean project MUST contain no `sorry`. The spec should say that excluded declarations or files are absent from the `LokiLean` lib that CI builds (not only from the feature claims), or the two gates contradict each other on the release tree.
2. **`axiom-audit` needs a lakefile entry.** Task 3.2 pins the auditor, but `lakefile.toml` currently has only mathlib as a `[[require]]` (`lakefile.toml`, `[[require]]` block). Name the `require` line and the revision in the task, or the `lake exe axiom-audit` command cannot run as written.
3. **`lean4checker` cost not stated.** The reference recommends it in CI (https://lean-lang.org/doc/reference/latest/ValidatingProofs/), but the full-project check time is not budgeted. Add a time ceiling or a note that it runs on the same pinned runner as `lake build`.
4. **Citation of AUDIT.md:213.** The spec still cites `AUDIT.md:213` as the axiom baseline. That line is the earlier build log (`lake build` OK, 8931 jobs) at an older snapshot, not the release baseline. Cite it as historical, or cite the new GATE-AXIOMS output once it is regenerated.

## Verdict

No hard blocker remains in the Lean change. Items 1 and 2 should be fixed before task 3.2 is accepted, because they decide whether the no-holes gate can run as written.

## Unverified

- Whether `lake exe axiom-audit` runs on Lean v4.34.1 (not run; the task says to verify it).
- Current `lake build` status on either tree (not run).
