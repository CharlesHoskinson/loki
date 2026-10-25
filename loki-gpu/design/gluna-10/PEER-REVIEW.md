# Peer review: `design/TEST-PLAN.md`

Reviewer: gluna-10  
Scope: read-only review against the owner brief and `design/ARCHITECTURE.md`.  
Status: findings for gluna-9 and astra-gpu; no test implementation was run.

## Findings

### P2 — Recursion gate conflates per-stage qualification with production verification timing

**Location:** `design/TEST-PLAN.md:29`.

The row says “Every final proof” must be accepted by the stock verifier, while
also naming each stage and the whole chain. It does not distinguish the
qualification requirement to stock-check every leaf/parent/decider artifact
from the production-shaped latency path. Requiring an extra off-circuit stock
verification before every dependent can change production latency if that
check is not part of production; omitting per-stage checks from qualification
would leave intermediate proof compatibility untested.

**Fix:** State that qualification independently stock-verifies each leaf,
parent, and decider artifact. For the production-shaped chain benchmark, run
the off-circuit verifier checks production actually uses and always record
terminal stock-verifier acceptance separately. Define DAG readiness using
completed artifact and relation/VK/instance identity checks, not an implicit
extra verifier invocation.

**Re-review:** Resolved in current line 29 and benchmark protocol lines 222–225:
each proof is independently stock-checked for correctness, while the
production-shaped chain excludes additional off-circuit intermediate checks
unless production uses them and reports terminal stock acceptance separately.

### P2 — Public API stream cases are mixed with internal runtime failure cases

**Location:** `design/TEST-PLAN.md:21`.

The API-contract row lists “streams” alongside public operation arguments,
although the architecture deliberately keeps CUDA streams private and
backend-neutral API calls do not accept stream handles. This makes the planned
test boundary unclear and may encourage a CUDA-specific public API.

**Fix:** Keep public invalid-input tests to lengths, domains, resident handles,
and capabilities. State separately that private runtime tests inject failures
at stream/event boundaries, as the existing async-failure row already
requires.

**Re-review:** Resolved in current line 21: stream/event faults are private
runtime injections, while public API inputs remain backend-neutral.

### P2 — Resident-state fuzz target has inconsistent names and is omitted from the target list

**Location:** `design/TEST-PLAN.md:56,65,67`.

The planned fuzz path is named `residency_state.rs` at line 56; the bullet calls
it `resident_state` at line 65; and line 67 adds `scheduler_replay` without a
corresponding fuzz target path in the planned-target list. An implementer
cannot tell which targets and names are part of the deliverable.

**Fix:** Choose one resident-state target name and use it consistently. Add an
explicit `scheduler_replay.rs` path to the planned fuzz-target list, or
identify `scheduler_trace.rs` as an integration test and remove the unsupported
fuzz-target claim if scheduler traces are not intended to be fuzzed.

**Re-review:** Resolved in current lines 67 and 78: `residency_state.rs` is
named consistently and scheduler replay is identified as the
`scheduler_trace.rs` integration test.

## Review summary

The plan covers the principal correctness gates in the brief: stock arithmetic
differential checks, repeated/boolean/zero scalar distributions, NTT and
quotient comparisons, error-state preservation, unmodified stock verification,
Lean-to-test mapping, and separately provenance-pinned recursion cohorts. It
correctly states that Lean obligations do not verify native CUDA and that
feature compilation/fallback alone does not prove GPU execution. No claim of
CUDA or Lean verification is made by this review.
