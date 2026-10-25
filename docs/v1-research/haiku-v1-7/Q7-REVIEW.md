# Q7-REVIEW: review of openspec/changes/v1-quint-model-coverage (haiku-v1-7)

Files read: proposal.md, design.md, tasks.md, specs/quint-release-qa/spec.md. `openspec validate v1-quint-model-coverage --strict` returns "Change 'v1-quint-model-coverage' is valid". Strict validation passes, so the findings below are content findings. Line refs were checked against the loki-proofs main checkout (eaf09d8).

## High priority

1. **Spec line 18 cites a scope that does not exist.** It says "the affected-path scope in `quint/check.py:12-17`". Lines 12-17 are the MODELS dict (transcript 180, gpu_dispatch 120, pk_cache 120, scheduler 1000 steps). check.py has no path scoping. Fix: cite the MODELS dict for model list, and state that the affected-path scope is new (created by 1.1).

2. **"Exhaustively" contradicts "bounded" (spec lines 46 and 48-50).** The requirement says safety "SHALL receive bounded exhaustive verification with Apalache where feasible", and the scenario title says "Finite dispatch safety is checked exhaustively". Apalache checks within a step bound. Quint's docs describe this as symbolic bounded checking that gives "incomplete results" (https://quint.sh/docs/checking-properties). The existing dispatch check uses `--max-steps=6` (quint/q2/check.py:145-155). Fix: say "bounded verification up to a declared step bound". Also make "where feasible" testable, for example "for every model listed in coverage.yaml with a declared bound and a time budget of N minutes; infeasible models must be listed with their reason".

3. **"Each invariant SHALL have a mutation control" is not met today (spec line 21).** quint/mutations.qnt has 5 mutants: transcript `earlySqueeze`, gpu `staleHit`, pk_build `duplicateBuild`, pk_partial `prematureUse`, scheduler `overAdmit` (lines 9, 24, 37, 50, 61). Safety invariants with no mutant include transcript `absorbedSequence`, `challengesBound`; gpu `resultEqualsCpu`, `fallbackTotal`; pk `bindingCorrect`; scheduler `noLostOrDuplicated`, `checkpointIntegrity`. Fix: the requirement should say "each invariant in the `safety` conjunction (or each named invariant) SHALL have at least one violated mutant", and tasks 2.2 should list the expected count. Right now "valid defect mutations" (line 25) has no definition of validity.

4. **"Real Rust replay" is undefined for GPU and fallback paths (spec lines 27-36).** W8 replays GPU dispatch through CPU-backed FFI surrogates, with "No GPU kernel ran" (LOKI-QUINT.md, W4 item; quint/q2/REPORT.md:124-130). An implementer could mark a GPU row "replayed" with surrogates. Fix: add a definition of "observable Rust replay" that says which seams count (production call path, real dispatcher, GPU-backed surrogate allowed only if labelled), and forbid a surrogate from satisfying a GPU-correctness row.

5. **Stock-disagreement scenario collides with the known stock defect (spec lines 63-65).** Stock rejects its own mixed-instance multi-proof output (T03; LOKI-QUINT.md "Implementation mismatches" item 1). As written, this scenario fails the release gate because of stock, not Loki or Quint. Fix: add a carve-out that names the T03 layouts as an expected stock-rejection outcome, recorded per the fuzz "shared outcome" policy, and say the observation mechanism for "transcript event order" (stock exposes no event log; the W8 wrapper logs absorption).

## Medium priority

6. **Missing edge cases the proposal names.** Cancellation no-retry (tasks 2.2) has no requirement; it depends on T28, which is still open. NTT fallback (W5 fail-stop vs total fallback, LOKI-QUINT.md item 3; T31) has no row category. Third advice phase and arbitrary query rotations (T30) are open, so transcript replay coverage is partial. Each should either get a scenario or be stated as "blocked by T##".

7. **Liveness scenario lacks evidence and a command (spec lines 52-54).** Add the observed error, `Handling fairness is not supported yet!` on Apalache 0.56.1 for `quint verify quint/scheduler.qnt --temporal admittedEventuallyFinish --max-steps 24` (LOKI-QUINT.md:280), and the command `python3 quint/ci/run_temporal.py --release` from task 4.2. Note that PDR-017 lists fairness as "not yet designed" (https://apalache-mc.org/docs/adr/017pdr-temporal.html).

8. **Replay in a clean checkout is not yet possible (spec lines 38-43).** quint/mbt/Cargo.toml:20 pins quint-connect from a git URL, so a clean CI needs network or a vendored copy. Also quint/mbt/build.rs and the q2 build.rs read sources from external commits. Task 3.1's `--features release-mbt` does not exist yet (current features: `w4-source`, `w6-source`). Say explicitly that the feature is added, and require vendoring or a pinned git mirror.

9. **Mis-cited line ranges.** `quint/transcript.qnt:124-170` (spec line 65 and 14) should be 127-171 (the `transcript_instances` module is lines 127-171; `transcript` is lines 6-122). `quint/check.py:20-56` is correct for `main`. `quint/q4/check.py:28-58` (tasks 4.1) points at a comment and the CAMPAIGNS table header (line 28); the Apalache-relevant entries are the q2 `quint verify` call (q2/check.py:145-155), so re-cite.

10. **"Preserve the stock randomized lookup-order contract" (proposal.md, What Changes, fourth bullet; design.md decision 5) overstates stock.** Stock's leftover order is unspecified randomness, not a documented contract. Wording: "preserve stock's default unspecified (HashMap) order". This also matters because no test can demand a particular order.

## Low priority

11. Spec line 9 says "each in-scope model SHALL define... reachable witnesses". Only transcript, gpu_dispatch, pk_cache, scheduler have witnesses in check.py (lines 13-16 for transcript, gpu_dispatch, pk_cache, scheduler). Add the same list for q1/q2/q4/q5 models or say that they are out of the v1 set.

12. Scenario "Feature docs" says "the precise result artifact for the current SHA" without naming an artifact path. Name it, for example `quint/evidence/<SHA>/results.json`.

13. Proposal impact cites `docs/ARCHITECTURE.md:29,100-104,128-142,158-160`. These lines match the stock lookup sentence and the commitment path. Spot-checked, correct.

## Verified as correct (no change needed)
- `quint/q2/mbt/build.rs:8-26` (REV constant and git show helper)
- `quint/q2/REPORT.md:124-130` (B2 no Rust replay, no GPU, not injectable limits)
- `docs/modules/quint.md:12-20` (scheduler marked Planned)
- `src/backend/dispatch.rs:113-159,185-224` (failed, msm, transform, try_accelerated_transform)
- `quint/q2/check.py:145-155` (quint verify --max-steps=6)
- `quint/q4/check.py:76-129` (srs campaigns)
- `quint/q1/check_q1.py:48-90` (sim and MUTS)
- `quint/mbt/README.md:7-39` (pins and W4/W6 sources)

## Not checked
- `docs/modules/README.md:1-67` and `docs/ARCHITECTURE.md:158-160` beyond the spot-check above.
- Whether `quint/ci/` scripts exist (they do not yet; expected, as the change is a plan).

Notes path: /home/charles/repos/loki-wt/v1/docs/v1-research/haiku-v1-7/Q7-REVIEW.md
