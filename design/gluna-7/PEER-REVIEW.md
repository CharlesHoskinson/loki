# Peer review: recursion scheduler design

**Evidence convention:** unless explicitly labeled `SOURCE`, `MEASURED`,
`ESTIMATE`, or `OPEN QUESTION`, normative design prose is `CODE (proposed)`;
it describes a contract or implementation proposal, not evidence that the
runtime already implements it.

Review scope: read-only review of `design/RECURSION.md` against the owner
brief, `design/ARCHITECTURE.md`, and the current Loki/recursion evidence. No
code, proof, or benchmark was run for this review.

## Findings

### Major — external stock verification of every intermediate may be on the critical path

- **File/lines:** `design/RECURSION.md:28-36, 127-130`
- **Issue:** The diagram places stock-compatible verification between every
  leaf/parent/decider stage, and the publication rule requires full transcript
  consumption by the stock verifier before a dependent proof can consume the
  child. Read literally, this adds an off-circuit verifier run to every
  intermediate node. That may repeat checks already performed by the next
  recursive relation and materially increase the chain latency this design is
  intended to reduce. The document does not distinguish production DAG
  readiness from qualification gates.
- **Fix:** Specify that each dependent relation must perform its existing
  in-circuit child-proof checks and that orchestration verifies the artifact's
  identity and completeness before scheduling the dependent node. Keep
  unmodified stock-verifier acceptance and transcript-exhaustion checks as
  mandatory promotion gates for each stage in qualification runs, and for the
  final decider in the measured production chain. If the production
  orchestrator intentionally verifies every intermediate off-circuit, name
  that cost and report it separately in the whole-DAG timer.
- **Severity:** Major for the recursion latency design; no correctness defect
  is asserted if all these checks are intentionally required.

## Confirmed properties

- The proposal keeps proof dependencies in orchestration, per-proof transcript
  ordering in Loki, and device resources in the backend runtime.
- It does not infer safe k19/k20 concurrency from the sequential k14 X6
  allocation probe, and starts with one heavy proof admitted.
- It separates the H0 and W10 A3 timer scopes and does not add the W12 model
  estimate to the W10 outside-`create_proof` interval.
- It treats device-only context loss as a proof failure and preserves
  generation invalidation rather than retrying stale residents.

## Limitation

The review did not inspect or run Moriarty's production chain driver. The
recommendation about intermediate verifier placement must be reconciled with
the actual child-proof checks in the parent and decider relations before the
orchestration contract is frozen.

## Resolution

The author revised `design/RECURSION.md:49-59,143-150` to make artifact
identity/readiness distinct from qualification-time stock verification. The
production-shaped chain now includes only off-circuit verifier calls the
production orchestrator actually performs, with terminal stock acceptance
recorded separately. This resolves the finding; the review remains limited by
not inspecting the production chain driver.
