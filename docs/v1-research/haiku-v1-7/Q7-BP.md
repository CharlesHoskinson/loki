# Q7-BP: Quint / Apalache best practice and a CI split (haiku-v1-7)

Sources were fetched 2026-10-25. Each claim cites its URL. Items marked UNVERIFIED are my inference or single third-party reports.

## 1. What the official docs establish

- Safety first. Quint's docs say: "It is much easier to write and check safety properties, which can be written as invariants." Source: https://quint.sh/docs/checking-properties
- Multiple invariants: `quint run file.qnt --invariants a b c` and `quint verify ... --invariants ...`. Source: same page. The repo instead uses one conjunction `safety` (quint/transcript.qnt:116, gpu_dispatch.qnt:119, pk_cache.qnt:73, scheduler.qnt:72) with `--invariant safety`. This is valid but a failing run names only the conjunction; splitting into named invariants would localize violations. Recommendation: keep the conjunction for the gate and also run the named parts on failure.
- Witnesses are "some state" sanity checks, not liveness. Quint's docs: "Quint's built-in support for witness properties is under design." and "It is not the same as a liveness property." Two forms: `--witnesses=NAME` (counts witnessed traces) or `--invariant="not(NAME)"` (a violation is the desired result). Source: https://quint.sh/docs/checking-properties. The repo's check.py (lines 33-35, 54-55) uses the counting form and fails on zero counts, which matches the documented use.
- `quint run` is simulation and cannot prove an invariant. The docs say: "Simulation (`quint run`) does not support inductive invariants." `quint verify` (Apalache) does bounded symbolic checking and supports inductive invariants (`--inductive-invariant`). Source: https://quint.sh/docs/checking-properties
- Inductive invariants need TypeOK-style constraints, each state variable constrained by `x == value` or `x.in(set)`; the docs warn that finding them "requires expertise". Same source.
- Liveness needs fairness, and the docs say these "often rely on assumptions about fairness to be useful." Same source.
- Apalache temporal support, planned design: PDR-017 lists `<>P`, `[]P`, `<A>_e`, `[A]_e` as "well-understood" preprocessing; fairness `WF_e`, `SF_e` and `ENABLED` are "not yet designed", with "To be detailed later…"; the search is lasso-based and "incomplete" for infinite-state systems. Source: https://apalache-mc.org/docs/adr/017pdr-temporal.html. This page is a design document, not shipped behavior.

## 2. Third-party evidence on Quint liveness with Apalache (not official; treat as anecdote)

- One project reports Apalache takes no `weakFair` and no quantified temporal formula, finds only lasso counterexamples, and needed a workaround `always(eventually(not(guard)))` for self-disabling steps; the note refers to Apalache 0.56.1. Source: https://github.com/phoban01/battery-operator/pull/153 (also PR #155 and issue #66 in the same repo). Quint 0.32.0 pins Apalache 0.56.1 (matches LOKI-QUINT.md "Apalache 0.56.1 is installed").
- A Zenn article claims Apalache does not support fairness and that TLC (`--backend tlc`) is needed. Source: https://zenn.dev/jupitris/articles/6b9da192db9af3?locale=en. It is roughly 200 days old and conflicts with the 0.56.1 reports above. UNVERIFIED which is current.
- The repo's own failure, `Handling fairness is not supported yet!` with `quint verify scheduler.qnt --temporal admittedEventuallyFinish` (LOKI-QUINT.md line 280), is consistent with the "not yet supported" reading for this Apalache version. Re-check the quint/Apalache release notes before a release claim.

## 3. Practical CI split for these finite-state models

Assumption: models are finite-state and small (repo steps 120/180/1000; ints abstract). Current state: no CI gate of any kind exists (no `.github/`), so everything below is a proposal.

Tier 0, every PR (target under 10 min, no quint run longer than about 60 s):
1. `quint typecheck` on all five top-level models and the `*_test.qnt` files (passes today, see Q7-CODE).
2. `quint test quint/<m>_test.qnt --match '.*Test$' --max-samples 1` (27 tests in check.py).
3. `quint run` safety, 1,000 samples, fixed seed `20261024`, with `--witnesses` and failing on any witness count of zero. Source for the flags: check.py lines 33-35.
4. Mutation kills at 1,000 samples: every mutant in quint/mutations.qnt MUST produce an invariant violation. Currently not wired (LOKI-QUINT.md:104-110). Gate it.
5. `quint/check_instances.py` at a small sample count, asserting the expected-failure pattern (the mismatch exists only for mixed multi-proof layouts).

Tier 1, nightly and on release branches:
6. Full check.py at 10,000 samples (existing behavior).
7. `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features w4-source,w6-source -- --test-threads=1` with the pinned quint-connect rev (quint/mbt/Cargo.toml:20). Requires the external `fenrir-loki` repo; make it a required input in CI, or vendor the pinned source (quint/mbt/build.rs:64-67).
8. Apalache bounded invariant checks: `quint verify` with `--invariant safety` and a small `--max-steps` for transcript, gpu_dispatch and pk_cache. Expect a bounded "no counterexample up to N" result, which is not a proof; record N. For inductive proofs, attempt `--inductive-invariant` on pk_cache first (smallest state) and report failures as open rather than gating on them. UNVERIFIED that these finish within budget; measure first.

Tier 2, release gate only:
9. Scheduler liveness. Do NOT gate on Apalache temporal checks until the fairness support is confirmed on the pinned Apalache version (see section 2). Until then, record `admittedEventuallyFinish` as "unchecked, sampled only" in the release notes, which matches the T29 wording ("retain sampled completion evidence as a separate result").
10. Store ITF/state logs and quint/results.json as release artifacts with quint version, Apalache version, seed and command line.

## 4. Things to avoid (for the release notes)

- Do not report a `quint run` pass as a proof. Quint's own docs say simulation "cannot prove" invariants (see section 1).
- Do not read a witness count of zero as "unreachable". Count >0 is the only evidence.
- Do not claim scheduler liveness from random completion counts (LOKI-QUINT.md item 5 already says so).

## Sources
- https://quint.sh/docs/checking-properties (official; Quint docs page at quint-lang.org redirects here)
- https://quint.sh/docs/model-based-testing (official)
- https://apalache-mc.org/docs/adr/017pdr-temporal.html (design document)
- https://github.com/phoban01/battery-operator/pull/153 (third-party, anecdotal)
- https://github.com/phoban01/battery-operator/pull/155 (third-party, anecdotal)
- https://zenn.dev/jupitris/articles/6b9da192db9af3?locale=en (third-party, likely outdated)
