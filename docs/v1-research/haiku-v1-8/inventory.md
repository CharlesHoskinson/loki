# haiku-v1-8 inventory: lean area (Lean specification)

Scope: state of the Lean specification in `~/repos/loki-lean` (branch `main`, clean) and its `gpu-spec` worktree, as read on 2026-10-25. Nothing was built: no `lake build` was run, so every "builds" claim below is unverified. Line numbers are from file reads at the time of this inventory.

## 1. What exists

- `LokiLean/` on `main`: 20 modules, about 18.7k lines (`wc -l`). Modules: Field, Ntt, Msm, Transcript, Plonk, Fusion, Refinement, Multiopen, Profile, Pipeline, SrsRegistry, GpuSystem, Backend, BackendMsm, BackendNtt, BackendQuotient, CpuKernels, ArgumentCommit, Dispatch. Root import `LokiLean.lean`.
- `SPEC.md` (157 lines): the theorem-to-Rust-citation table. Entries are tagged with the Loki revision they cite (`bd1f0cc` for stock, `a256a9a` for Loki main).
- `COVERAGE.md`: component matrix A1-A28, B1-B4, C1-C11, D1-D3 (`COVERAGE.md:1-60` shows the legend and the A rows). Status legend: PROVED / HYPOTHESIZED / MISSING / N/A.
- `AUDIT.md`: audit log by `lean-auditor`, with `#print axioms` results (`AUDIT.md:1-30` reference facts).
- `scripts/drift.py` with `scripts/citations.json` (citation drift checker), `scripts/AxiomSweep.lean`, `scripts/audit_gen.py`.
- `lakefile.toml`: Lean `v4.34.1`, mathlib `v4.34.1` (`lean-toolchain`, `lakefile.toml`).
- `.github/workflows/lean_action_ci.yml`: runs `leanprover/lean-action@v1` and docgen-action. It has no drift check, no axiom sweep and no sorry gate in the workflow.
- `SYNCED-LOKI-COMMIT` = `a256a9a15ff18ae12a8131b86cff576787fa5e3c`.

## 2. Gaps and risks found

1. **Pin is stale against Loki main.** Loki `main` is `eaf09d8` (`git -C ~/repos/loki-proofs log -1`). `SYNCED-LOKI-COMMIT` is `a256a9a`. `TODO.md:39-40` (T09) still names `877114d`, which is older than the file. The Lean pin, the TODO text and the Loki head disagree three ways. Only `a256a9a` is in the file, so the sync is partial at best.
2. **Open `sorry` lives only in the gpu-spec worktree, not on `main`.** `~/repos/loki-lean-wt/gpu-spec/LokiLean/BackendMsm.lean:270-282` (`segmentedValue_eq_pippengerList`, OBLIGATION(msm-segmented-partition)) and `BackendQuotient.lean:176-182` (`valid_program_eval_some`, OBLIGATION(quotient-interpreter-safety)). The gpu-spec worktree also has `BackendSrs.lean` (OBLIGATIONs srs-prefix-kernel, srs-codec-agreement, srs-submission-pinning, lines 33-40, 222), which `main` does not contain. `TODO.md:64-69` tracks the two sorries. A `grep` for `sorry` found no others under `main/LokiLean`.
3. **Open obligations are listed in prose, not gated.** On `main`: 2 in Pipeline, 2 in GpuSystem, 6 in Refinement, 2 in BackendQuotient, 1 in Backend, 3 in Plonk, 1 in BackendMsm (grep count of `OBLIGATION(`). Nothing fails CI when one is closed or added. `OPEN OBLIGATION(quotient-compiler-lowering)` (`BackendQuotient.lean:387`) has no `sorry`, so it cannot be seen by a sorry gate.
4. **Coverage matrix is mostly HYPOTHESIZED.** Counting the A-block rows in `COVERAGE.md` (awk over the status column, approximate): 12 PROVED, 21 HYPOTHESIZED, 2 MISSING, 8 N/A. The MISSING rows include A23 (W12 Moriarty parent fast path, "none", `COVERAGE.md` row A23). The A-block shows the W10 argument-commit row A19 as HYPOTHESIZED with `SrsOk` trusted for `downsize`/`from_parts(None)`.
5. **Headline theorem is conditional.** `Refinement.loki_proof_eq_stock` assumes `Refines` and `SameOrder` (`SPEC.md:119`; `README.md:185`). It is not an unconditional byte-equality proof. The README and `docs/review/luna-r10.md` already say so.
6. **The SRS registry prefix claim is known false.** `COVERAGE.md` A12 (`KernelPrefixOk` is stronger than W9 `KernelOkOnValid`; the `PLAN.md:85` claim is marked FALSE, coordinator to fix). This is an open correction, not a proof.
7. **Lean-side gaps the v1 roadmap must cover:** scheduler trace refinement is unformalized (`~/repos/loki-lean-wt/...` referenced by `loki-gpu/design/TEST-PLAN.md:103`); PK cache keygen-determinism `keygen-deterministic` is open (COVERAGE A22); `quotient-compiler-lowering` is open (source-to-bytecode simulation); W12 is MISSING.
8. **Stock-compat exposure is only as wide as `Refines`.** Any new Loki-only v1 behaviour (WASM, mobile CPU paths, the new `cuda-srs-registry` gate) needs a `Refines` instance or it falls outside `loki_proof_eq_stock`.

## 3. Evidence not yet checked (unverified)

- `lake build` status on `main` and gpu-spec (`AUDIT.md` claims per-module builds; not re-run here).
- `#print axioms` output for any theorem (`scripts/AxiomSweep.lean` exists; not run).
- `scripts/drift.py` result against Loki `main` (not run).
- Whether `COVERAGE.md` B-, C- and D-block rows agree with their status counts (only the A-block was counted).

## 4. Items for luna-v1-8 to decide

- Whether the v1 Lean gate is: zero `sorry` on the release branch, zero new axioms beyond `propext`/`Classical.choice`/`Quot.sound`, and drift.py exit 0 against the release commit (T09).
- Whether `BackendSrs.lean` and the two open sorries are in scope for v1 or moved to post-v1 TODO.
- Whether to add `OBLIGATION(...)` counting to CI so open obligations cannot silently grow.
