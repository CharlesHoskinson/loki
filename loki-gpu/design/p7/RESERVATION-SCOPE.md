# P7 reservation scope: redeeming an admission reservation into allocations

Status: DESIGN ONLY, revision 2 (P7 owns this file). P4 reviews the runtime
parts by message, and no shared runtime code changes. This closes review
finding S4 by giving the scheduler's "admission prototype" an implementable
contract. Revision 2 answers astra-gpu's review of 4c8a50c, points (1) to (5).

Target: corrected P4 plus P5, as seen in the standalone loki-gpu
`p5-native-dev` `runtime.rs`, which includes the sticky `local_backing`. It does
not target 0c4ab9d.

## Problem (CODE, p5-native-dev runtime.rs)

The registry accounts for device memory as follows:

- `available = budget - live - reserved - local_backing`;
- `try_reserve` adds the requested bytes to `reserved`;
- each internal allocation checks that the bytes fit `available`, adds them to
  `reserved` while in flight, and on publication moves them: `reserved -= b` and
  `live += b`;
- the quotient launch also charges `local_growth = local_bytes -
  local_backing`. That charge is sticky for the whole context, never refunded,
  and reset to 0 only by `reinitialize`.

A `HeavyPermit` holding R reserved bytes lowers `available` by R. The admitted
job's own allocations, and its first quotient's `local_growth`, need further
budget on top of R. The workspace is double-counted and the job can exclude
itself.

## Model

A **scope** is registry-owned state that a job's allocations spend. Every
quantity is authoritative only in the registry. Tokens are untrusted, copyable
metadata: `{ id, generation }` and nothing else, so no stale `granted` field
can travel with them. The registry validates every use and every close against
its record. Holding a token is not proof of exclusive authority (see "Release
authority").

```rust
// loki-backend-api additions (existing v1 calls unchanged)
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ScopeId(pub u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScopeToken { id: ScopeId, generation: DeviceGeneration }   // untrusted metadata

pub enum ScopeState { Open, Closing, Closed }
pub struct ScopeSnapshot {
    pub state: ScopeState,
    pub granted: u64,        // total ever credited (open + extend), minus retained-out
    pub credit: u64,         // spendable now
    pub in_flight: u64,      // charged, not yet published
    pub live: u64,           // published scoped allocations
    pub context_spent: u64,  // credit transferred to sticky local_backing (never refunded)
    pub active_ops: u32,     // scoped calls currently inside the runtime
}
pub struct ScopeClose { pub freed: u64, pub retained: u64, pub credit_returned: u64, pub context_spent: u64 }
```

The public surface is ONE scope-control trait plus a scoped view that
implements the EXISTING `ResidentBackend` facets. No per-operation `_in`
methods are added to any public trait. The charge source (`Available` or
`Scope(id)`) is a private parameter of the runtime's internal allocation
helpers.

```rust
pub trait ScopeControl {
    /// Borrowed: on success the ReservationRecord is atomically replaced by a
    /// ScopeRecord with credit = its bytes (no counter moves). On any error
    /// nothing changes and the reservation stays releasable.
    fn open_scope(&self, reservation: &ReservationToken) -> Result<ScopeToken, BackendError>;
    /// Atomic, draws only from `available`. No separate reservation exists.
    fn extend_scope(&self, s: &ScopeToken, bytes: u64) -> Result<Extend, BackendError>;
    fn close_scope(&self, s: &ScopeToken, retain: &[AllocationId]) -> Result<ScopeClose, BackendError>;
    fn scope_snapshot(&self, s: &ScopeToken) -> Result<ScopeSnapshot, BackendError>;
}
pub enum Extend { Granted { credit_now: u64 }, Rejected { requested_bytes: u64, available_bytes: u64 } }

// loki-gpu: the only way to spend a scope.
pub struct ScopedRuntime { runtime: Arc<GpuRuntime>, scope: ScopeToken }
impl GpuRuntime { pub fn scoped(self: &Arc<Self>, s: ScopeToken) -> ScopedRuntime }
impl<F, G> ResidentBackend<F, G> for ScopedRuntime { /* each call -> internal helper with ChargeSource::Scope(id) */ }
```

Every `ScopedRuntime` call validates its token at entry against the registry,
using the same checks as `release_reservation`: id known, generation equal,
and the record belongs to this runtime instance.

The registry gains `scopes: HashMap<ScopeId, ScopeRecord { generation, state,
granted, credit, in_flight, live, context_spent, active_ops }>`. `AllocRecord`
gains `owner: Owner`, where `Owner` is one of:

- `Scope(ScopeId)`: freed or retained at close;
- `Persistent`: never freed by any scope, e.g. the SRS registry and retained
  outputs;
- `Unscoped`: today's behaviour.

## Charging rules

Each rule is one registry critical section, taken with the device lock already
held where the existing path holds it.

The *charge* of an op is everything it can newly consume:

- the sum of its output and scratch bytes;
- plus, for a quotient launch, `local_growth = local_bytes -
  local_backing`.

1. **Charge.** The scope must be `Open` and in the current, healthy generation,
   and `charge <= credit`. Then:
   - `credit -= charge`;
   - `in_flight += outputs + scratch`;
   - `local_growth` is transferred immediately: `context_spent +=
     local_growth`, and global `local_backing += local_growth`, `reserved -=
     local_growth`;
   - `active_ops += 1`.

   Otherwise the op returns `Capacity / NotStarted` and nothing changes. This
   rule is all-or-nothing for multi-buffer ops and for the first quotient
   (outputs, scratch and local growth together). It also covers the
   self-exclusion case: a scope funded for exactly that total succeeds.
2. **Publish (success).** For each published allocation: `in_flight -= b` and
   scope `live += b`; global `reserved -= b` and `live += b`; `owner =
   Scope(id)`. Then `active_ops -= 1`.
3. **Abort without uncertainty** (`NotStarted` or `InputsPreserved`, all
   private resources retired): `in_flight -= b`, `credit += b`, `active_ops -=
   1`. `context_spent` is never refunded, because once a launch may have been
   submitted the driver may hold the backing until reset (the P5 Q2 rule).
4. **Abort with uncertainty** (`SessionPoisoned`): `in_flight` bytes stay
   charged against the poisoned generation, the same as today's quarantine.
   Nothing returns to credit, and `active_ops -= 1`. The generation's
   accounting is discarded wholesale at `reinitialize`.
5. **Free a `Scope(id)` allocation** through `release`: global `live -= b`
   **only after** `device_free` succeeds. Then, if the scope is `Open` and
   current, `reserved += b`, scope `live -= b` and `credit += b` (reusable
   workspace credit). Otherwise the bytes simply leave the scope and return to
   `available`. A failed `device_free` poisons the generation and returns no
   credit. `Persistent` and `Unscoped` allocations are unchanged from today.

### Invariants (registry, under its mutex)

- **I1:** `reserved = Σ unscoped reservations + Σ_{Open/Closing scopes} (credit
  + in_flight) + Σ unscoped internal in-flight`.
- **I2:** per scope, `credit + in_flight + live + context_spent == granted`.
  `granted` grows only through `open_scope` and `extend_scope`, and shrinks only
  by bytes retained out at close.
- **I3:** `live + reserved + local_backing <= budget`. Quarantined allocations
  stay inside `live`, and their in-flight bytes inside `reserved`, until
  `reinitialize` zeroes the generation's counters.
- **I4:** a byte is in exactly one place: `available`, one scope's `credit` or
  `in_flight`, `live`, or `local_backing`.
- **I5:** only scoped calls on a matching, validated token spend scope credit.
  Unscoped and default calls use `available` and can never touch a scope.

## Lifecycle: Open, then Closing, then Closed

`close_scope(s, retain)` follows these steps in order. The device lock is taken
first and then the registry lock, as in the existing lock order.

1. **Validate before any change.** The token must name a scope of the current
   generation in state `Open`. If it names an older generation, the call
   returns **`Err(ContextLost)`**: one choice for every stale use, with no
   `Ok(Stale)` variant. An unknown or `Closed` scope returns
   `Err(InvalidInput)`. Every `retain` id must be a live, non-quarantined
   allocation with `owner == Scope(id)`. If any id is not, the call returns
   `Err(InvalidInput)` and nothing changes. An allocation already `Persistent`
   cannot be in `retain` and is never freed by a close.
2. **Closing.** `state = Closing`. New scoped calls and `extend_scope` on this
   scope are refused with `InvalidInput / NotStarted`.
3. **Retire active ops.** While `active_ops > 0`, release BOTH locks (an
   active op needs the device lock to finish), wait on the scope's condition
   variable, which is signalled whenever `active_ops` drops, then retake the
   device lock and then the registry lock, and re-check. `Closing` already
   blocks new ops, so the wait cannot starve. Every v1 call retires its own work before returning, so
   `active_ops` reaches 0 in bounded time unless the process is wedged.
4. **Retained allocations.** For each retained id, set `owner = Persistent`
   and move its bytes out of the scope: scope `live -= b` and `granted -= b`.
   The bytes stay globally `live`.
5. **Free the rest.** Every other `Scope(id)` allocation is freed with rule 5,
   minus the credit return. If a `device_free` fails, the generation is
   poisoned, the remaining allocations are quarantined, and the scope goes to
   `Closed` with `Err(SessionPoisoned)`. The scheduler treats that as failure
   (S5): nothing is published.
6. **Return the credit.** `reserved -= credit`, so the bytes return to
   `available`. `context_spent` stays in `local_backing`, the same as today
   without scopes. Set `state = Closed` and return the `ScopeClose` totals.

After `reinitialize`, every scope of the old generation is dropped along with
that generation's counters. Any later `ScopedRuntime` op, `extend_scope`, `close_scope` or
`scope_snapshot` on its token returns `Err(ContextLost)`.

### Release authority

Tokens are reconstructible metadata, so the runtime does not treat possession
as authority. It validates every call against the registry record.
Exclusivity comes from the scheduler: only `HeavyPermit` is given a token, it
never hands it out except through the explicit backend view below, and
`complete`, `fail`, `cancel` and `Drop` each call `close_scope` exactly once.
A second close of the same scope gets `Err(InvalidInput)` (state `Closed`).
This is a defined error, not undefined behaviour.

### SRS cache and borrowed handles

- Allocations made by the SRS registry (P3) are `Persistent` from creation.
  They are never charged to a scope, even when the registry populates them
  during a scoped job. This keeps a job close from freeing a cached SRS. If an
  SRS upload must be charged to the job's budget, it is uploaded through `ScopedRuntime`
  and listed in `retain`, which moves it to `Persistent`.
- A job may *borrow* a `Persistent` or `Unscoped` handle (read it as an
  input). Borrowing never changes ownership, and close never frees it.

## Plumbing: an explicit per-job backend view (no global binding)

```rust
// loki-gpu
pub struct ScopedRuntime { runtime: Arc<GpuRuntime>, scope: ScopeToken }
// (defined above; implements the existing ResidentBackend facets)

// Loki src/backend/dispatch.rs (feature-gated)
pub struct Dispatcher<B> { backend: B, counters: ... }     // already generic in P1
pub fn scoped(rt: ScopedRuntime) -> Dispatcher<ScopedRuntime>;  // explicit per-job dispatcher
```

The global `dispatch::global()` stays unscoped. A job's dispatcher is passed
explicitly to its feature-on call sites. This replaces the revision 1
`Dispatcher::bind_job`, under which every concurrent caller would implicitly
spend the job's scope. The exact plumbing:

1. **Prover entry.** Add `create_proof_with_backend(.., backend: &dyn
   AcceleratorBackend)` behind `loki-gpu-backend`. `create_proof` calls it with
   `global()`.
2. **Call sites.** These sites take a `&Dispatcher` parameter instead of
   calling `global()` under the feature:
   - `poly::kzg` commit and open MSMs (`msm.rs` / `msm/cuda.rs` dispatch);
   - `poly::domain` FFT/coset;
   - the quotient (`plonk::prover` h(X) evaluation);
   - multi-open.

   The default build keeps the current direct calls.
3. **Rayon closures** capture `&Dispatcher`, which is `Sync`. No thread-local
   state is involved.
4. **P7 driver.** It receives `permit.scoped_runtime()` and calls
   `create_proof_with_backend`.

## Scheduler changes (P7, `schedule.rs`)

- `ReservationSource` gains `open_scope(&ReservationToken)` and
  `close_scope(&ScopeToken, &[AllocationId])`.
- Admission order stays heavy lock, GPU lock, `try_reserve`, and adds
  `open_scope(&token)`. If `open_scope` fails, the scheduler releases the still
  valid reservation with the same token, then unwinds the locks.
- `HeavyPermit` holds a `ScopeToken` and exposes `scoped_runtime()`.
- `complete(identity, retain)`:
  1. `check_current`;
  2. `close_scope(retain)`;
  3. publishes `Completed` only on `Ok(ScopeClose)`. Any error is recorded as
     `Failed` (S5).
- `fail`, `cancel` and `Drop` call `close_scope(&[])`, record the result and
  never publish.
- The "admission prototype" label is removed when this lands.

## Tests (implementation phase)

All tests run on FakeDriver through the safe `with_fake` / `FakeControl` API.

| Test | Proves |
|---|---|
| `open_scope_moves_no_counters` | I1/I4 at open; `reserved`/`available` unchanged; reservation record gone; scope `credit` = bytes |
| `open_scope_failure_leaves_reservation_releasable` | borrowed token; release succeeds after a failed open |
| `scoped_upload_of_exact_grant_succeeds` | S4 self-exclusion fixed; `available` unchanged |
| `first_quotient_funds_outputs_scratch_and_local_growth_atomically` | rule 1 with P5 local growth; `context_spent` set; I2/I3 hold |
| `local_growth_is_never_refunded_on_abort_or_close` | rules 3 and 6 |
| `overdraw_is_capacity_not_started_and_atomic` | multi-buffer sum one byte over credit; nothing changes |
| `inputs_preserved_abort_returns_credit` / `poisoning_abort_returns_none` | rules 3 and 4 |
| `free_returns_reusable_credit_only_after_device_free` | rule 5; failed `device_free` poisons with no credit |
| `close_refuses_invalid_retain_without_mutation` | step 1 validation (persistent, foreign or quarantined id) |
| `close_waits_for_active_ops_and_refuses_new_ones` | Closing state; a concurrent `ScopedRuntime` op gets `InvalidInput`; close completes after the in-flight op retires |
| `close_retains_outputs_as_persistent_and_frees_rest` | steps 4 to 6; retained handle usable after close and never freed by a later close |
| `srs_registry_allocations_are_never_freed_by_close` | Persistent ownership |
| `stale_token_is_context_lost_everywhere` | every scoped call after `reinitialize` |
| `double_close_is_invalid_input` | release-authority edge |
| `unscoped_and_global_dispatch_never_spend_scope_credit` | I5; a concurrent global-dispatcher MSM during a scoped job |
| `concurrent_scoped_ops_preserve_i2` | Rayon stress; snapshot after every step |
| `foreign_runtime_scope_token_is_invalid_input` | a token minted by runtime A is refused by runtime B (ids are per-instance) |
| `forged_scope_metadata_is_invalid_input` | wrong id, or right id with wrong generation, on every ScopeControl call and every ScopedRuntime op |
| `closed_scope_token_is_invalid_input` | spending or extending after close |
| `duplicate_open_of_one_reservation_is_invalid_input` | the second `open_scope` on the same reservation token fails without mutation; the first scope is intact |
| `scope_id_exhaustion_is_checked` | `fresh_id` overflow returns an error, never wraps or reuses an id (D1/D4 lesson) |
| property: random op/fault sequences | I1–I5 after each step (mirrors Quint Q3) |

Scheduler-side tests:

- `admission_unwinds_reservation_when_open_scope_fails`;
- `complete_publishes_only_on_clean_close`;
- `drop_closes_scope`.

One native receipt on the RTX 5090 repeats
`first_quotient_funds_outputs_scratch_and_local_growth_atomically` and
`close_retains_outputs_as_persistent_and_frees_rest`.

## Open questions for P4

1. Does any current path allocate more than once per v1 call outside a single
   registry section? If so, its scoped variant must pre-charge the sum under
   rule 1.
2. Checkpoint bytes are host memory. They should stay outside scope
   accounting. Please confirm.
3. The pageable-slot retention fix (P4 REVIEW item 9) must not return credit
   for quarantined bytes. Please confirm it only affects host handles.
