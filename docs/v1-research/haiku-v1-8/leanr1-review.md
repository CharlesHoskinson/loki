# haiku-v1-8 LEANR1: review of v1-lean-component-refinement (read-only)

Reviewed: `~/repos/loki-wt/v1/openspec/changes/v1-lean-component-refinement/{proposal.md,design.md,tasks.md,specs/lean-component-refinement/spec.md}`. No OpenSpec file was edited. `openspec validate --strict` was not run here (the lab instruction for REVIEW requires it; see finding 0).

## Verified citations (true as written)

- `LokiLean/Refinement.lean:853` `loki_proof_eq_stock`, signature `(r : Refines b sp ord) (hs : SameOrder b)`: confirmed.
- `LokiLean/Refinement.lean:1195` `lokiMainBackend_refines`: confirmed.
- `SPEC.md:87-88` open assumptions: `stock-trace-faithful`, `quot-inputs`, `stock-complete-any-order`, `keygen-deterministic`, `digest-binds`: confirmed as open comment-level assumptions.
- `scripts/audit_gen.py:11` `ALLOWED = {propext, Classical.choice, Quot.sound}`: confirmed (the spec cites `:11-43`, which is correct).
- `scripts/drift.py:43` hard-codes `MAIN_REV = a256a9a`; the usage block is at `:1-30`: confirmed. `drift.py` has no `--release` flag yet (the spec needs one).
- `loki-harness/README.md:24-27` holds the `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked ...` command: confirmed.
- `gpu-spec` worktree `SYNCED-LOKI-COMMIT` = `877114ddc1da5508230e424ec06d55da1ba00117`: confirmed. Its `AUDIT.md` has 489 lines; the cited ranges 412-423 and 466-488 exist.

## Findings

0. **Blocker (process):** the review requires `openspec validate v1-lean-component-refinement --strict`. Not run by me; luna-v1-8 should run it and attach the output.
1. **Blocker, release gate contradicts current tree:** the spec's "No holes" requirement (MUST contain no `sorry`) cannot pass on the merged tree today. `~/repos/loki-lean-wt/gpu-spec/LokiLean/BackendMsm.lean:282` (`segmentedValue_eq_pippengerList`, OBLIGATION msm-segmented-partition) and `BackendQuotient.lean:182` (`valid_program_eval_some`, OBLIGATION quotient-interpreter-safety) are both `sorry`. Task 1.2 should name these two explicitly as "prove or fail the gate". The proposal's wording "retain its GPU contracts" does not say so.
2. **Blocker, two different trees disagree:** the main Lean repo pins `a256a9a` (`SYNCED-LOKI-COMMIT`, and drift `MAIN_REV`), while the gpu-spec worktree pins `877114d` (its `SYNCED-LOKI-COMMIT` and `SPEC.md:13,17`). The design says "the separate GPU worktree is based on `877114d`" (correct), but the merge decision needs an explicit rule for which citation baseline wins and whether `drift.py`'s `MAIN_REV` moves. Otherwise task 1.2 merges two baselines with no rule.
3. **Spec commands reference scripts that do not exist:** `scripts/coverage_gate.py` and `scripts/check_proofs.py` are not in `~/repos/loki-lean/scripts/` (listing: `AxiomSweep.lean`, `audit_gen.py`, `citations.json`, `drift.py`). The `--release` flag does not exist on `drift.py`. The spec's WHEN lines read as current behavior. Mark each as "new, to be added" in the spec and in tasks 2.1, 3.1, 3.2.
4. **Review-decision mechanism is undefined:** the drift scenario says the gate "returns success only after each drift row is resolved" with a "review decision". There is no defined place to record a decision (for example a `decision` field in `citations.json`), and no defined way for the gate to read one. The current SPEC decision tables cover only `8eb0cee` and `877114d` syncs (`SPEC.md:48-67`), so the 81 CHANGED rows at eaf09d8 (`drift.py` run, read-only) have no decision. Specify the format and where it lives.
5. **Guessed trees:** 353 citations carry guessed trees (drift output). The spec's preservation of "historical baselines" (design decision 2) depends on fixing these first. Add a task: correct the `tree` values in `citations.json` before the release gate is enforced, and list the two UNRESOLVED external/draft paths.
6. **Stock harness scope is narrower than the spec says:** the spec says the harness checks stock 0.8.3 "in both directions". Per the lab's own read of `loki-harness` (luna-v1-2 proposal), it pins only 0.8.3 and recorded k10/14/17, not k20 and not 0.8.1. The Lean spec should not claim more than the harness covers. Either cite the compat-proof gate's actual scope or leave the "both directions" wording to compat-proof.
7. **Axiom gate wording:** the spec says "introduced axioms" are rejected and "the standard Lean kernel axioms" are reported. The current `audit_gen.py` rejects any theorem depending on axioms outside `{propext, Classical.choice, Quot.sound}` (`audit_gen.py:34`). That does reject `sorryAx`, which is correct, but the spec should say so and state that `sorryAx` is never on the allowlist. Also, `check_proofs.py` is said to "reject unreviewed axiom declarations", which is a different check (declarations in the environment, not dependencies). Name both checks.
8. **Target-path mapping (requirement 5):** the spec says the coverage manifest "identifies applicable WASM and iOS/Android Rust paths", but no column or field for target applicability exists in `COVERAGE.md`. Add one to the coverage gate definition.
9. **Quint/fuzz binding (task 4.1):** no artifact or identifier format is given for linking fuzz and Quint results to Lean component IDs. Name the join key (for example the `COVERAGE.md` row ID A1..D3) so the gate can check it.
10. **Nit:** tasks 2.2 "prove any in-scope bridge obligation or explicitly enumerate it" lets release accept an enumerated open obligation. The spec should say whether enumerated open obligations are blockers or allowed limitations. Currently "Keep open proof/model assumptions explicit and blocking where they affect the compatibility claim" (proposal) is the only rule, and it is not operational.
11. **Nit:** `lakefile.toml:12-18` and `lean-toolchain:1` are cited for pinning. The lakefile `lean_lib` is at roughly line 17; accurate enough, but the toolchain pin `leanprover/lean4:v4.34.1` should be quoted in the spec so the pin is checkable.

## Coverage of required areas

- Compatibility: stated (no Loki API change, refinement relative to stock 0.8.3). Good.
- WASM and mobile: stated as host-CI-only with target mapping; see finding 8.
- QA: fuzz and Quint linkage present; see finding 9.
- Docs: `COVERAGE.md`, `SPEC.md`, `AUDIT.md` updates listed (task 4.3).
- Drift and axiom-gate feasibility: feasible only after findings 1, 3, 4, 5 are fixed.

## Unverified

- `openspec validate --strict` result (finding 0).
- Whether `lake build` and `AxiomSweep` still pass on main (not run).
