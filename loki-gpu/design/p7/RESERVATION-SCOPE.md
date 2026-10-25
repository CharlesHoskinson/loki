# P7 reservation scope: redeeming an admission reservation into allocations

Status: DESIGN ONLY, revision 4 (P7 owns this file). P4 reviews the runtime
parts by message, and no shared runtime code changes. This closes review
finding S4 by giving the scheduler's "admission prototype" an implementable
contract.

Revision history:

- revision 2 answered astra-gpu's review of 4c8a50c;
- revision 3 reduced the public API;
- revision 4 answers the edc7896 review: the conservation ledger on close,
  quarantine, retain pinning, SRS promotion, the actual dispatcher shape and
  error precedence.

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

pub enum ScopeState { Open, Closing, Closed, Quarantined }
pub struct ScopeSnapshot {
    pub state: ScopeState,
    pub granted: u64,        // total ever credited (open + extend); monotone, never decreases
    pub credit: u64,         // spendable now
    pub in_flight: u64,      // charged, not yet published
    pub live: u64,           // published allocations still owned by the scope
    pub context_spent: u64,  // moved to sticky local_backing (never refunded)
    pub retained_out: u64,   // promoted to Persistent at close (still globally live)
    pub returned: u64,       // given back to `available` (freed allocations at close + unspent credit)
    pub quarantined: u64,    // stuck in a poisoned generation until reinitialize
    pub active_ops: u32,     // ScopedRuntime calls currently inside the runtime
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
- plus, for a quotient launch, `local_growth =
  local_bytes.saturating_sub(local_backing)`, matching the P5 code.

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
5. **Free a `Scope(id)` allocation** through `release` while the scope is
   `Open`: the free is refused if the allocation is pinned (see close step 1).
   Global `live -= b` happens **only after** `device_free` succeeds. Then
   `reserved += b`, scope `live -= b` and `credit += b` (reusable workspace
   credit). A failed `device_free` poisons the generation, and rule 7 applies.
   While the scope is `Closing`, a non-pinned free is allowed and books
   `returned += b` instead of credit (no `reserved` change). `Persistent` and
   `Unscoped` allocations are unchanged from today.
6. **Promote (close only):** scope `live -= b` and `retained_out += b`, with
   `owner = Persistent`. Globally the bytes stay `live`.
7. **Poison.** Every open, closing or quarantined scope of the generation
   moves `credit + in_flight + live` into `quarantined`, and its state becomes
   `Quarantined`. The global counters do not move: those bytes stay in
   `reserved` or `live`, as they do for today's quarantine. `reinitialize`
   discards the whole generation ledger.

### Invariants (registry, under its mutex)

- **I1:** `reserved = Σ unscoped reservations + Σ_{every scope record of the
  current generation} (credit + in_flight + quarantined_reserved) + Σ unscoped
  internal in-flight`. Here `quarantined_reserved` is the part of `quarantined`
  that came from `credit + in_flight`. Summing over every record, not only open
  ones, keeps a poisoned close inside I1.
- **I2 (conservation):** per scope, at every step,
  `granted == credit + in_flight + live + context_spent + retained_out +
  returned + quarantined`. `granted` only increases, through `open_scope` and
  `extend_scope`.
- **I3:** `live + reserved + local_backing <= budget`. Quarantined allocations
  stay inside `live`, and their in-flight bytes inside `reserved`, until
  `reinitialize` zeroes the generation's counters.
- **I4:** a byte is in exactly one place: `available`, one scope's `credit` or
  `in_flight`, `live`, or `local_backing`.
- **I5:** only scoped calls on a matching, validated token spend scope credit.
  Unscoped and default calls use `available` and can never touch a scope.

## Lifecycle: Open, then Closing, then Closed (or Quarantined)

`close_scope(s, retain)` takes the device lock and then the registry lock, as
in the existing lock order, and follows these steps:

1. **Validate and pin, before any change.** Validation follows the error
   precedence below. It requires state `Open`, and every `retain` id must be:
   - distinct (a duplicate is `Err(InvalidInput)`);
   - live and not quarantined;
   - `owner == Scope(id)`.

   If any check fails, nothing changes. Then every retain id is marked
   **pinned**: until close ends, `release` of a pinned allocation returns
   `Err(InvalidInput / NotStarted)`. This lets validation survive the lock
   release in step 3.
2. **Closing.** `state = Closing`. New `ScopedRuntime` ops and `extend_scope`
   on this scope are refused with `InvalidInput / NotStarted`.
3. **Retire active ops.** While `active_ops > 0`:
   - release BOTH locks (an active op needs the device lock to finish);
   - wait on the scope's condition variable, which is signalled whenever
     `active_ops` drops;
   - retake the device lock, then the registry lock, and re-check.

   `Closing` blocks new ops, so the wait cannot starve. If the generation was
   poisoned meanwhile (rule 7), the scope is now `Quarantined`, and close
   returns `Err(SessionPoisoned)` after unpinning.
4. **Revalidate pins.** Each pinned id is still live, scope-owned and not
   quarantined, because pinning blocked its release and quarantine would have
   moved the scope to `Quarantined`. A failure here is `Err(Internal)` and
   poisons the generation (rule 7). It is an invariant breach, not a normal
   result.
5. **Promote** every retained id (rule 6), then unpin.
6. **Free the rest.** For each other `Scope(id)` allocation: `device_free`,
   then global `live -= b`, scope `live -= b`, `returned += b`. If a
   `device_free` fails, the generation is poisoned (rule 7: the remaining live,
   credit and in-flight bytes become `quarantined`), the state is
   `Quarantined`, and the result is `Err(SessionPoisoned)`. Nothing is
   published (S5).
7. **Return the unspent credit.** `reserved -= credit`, `returned += credit`,
   `credit = 0`. `context_spent` stays in `local_backing`. Set `state = Closed`
   and return `ScopeClose { freed, retained, credit_returned, context_spent }`.

### Numeric traces (every row satisfies I2; `budget = 1000`, P5 `local_backing = 0` at start)

Healthy close:

| Step | credit | in_flight | live | ctx | ret_out | returned | quar | global reserved / live / local_backing |
|---|---|---|---|---|---|---|---|---|
| `open_scope` (reservation 100) | 100 | 0 | 0 | 0 | 0 | 0 | 0 | 100 / 0 / 0 |
| charge quotient 60 out + 10 local | 30 | 60 | 0 | 10 | 0 | 0 | 0 | 90 / 0 / 10 |
| publish 60 | 30 | 0 | 60 | 10 | 0 | 0 | 0 | 30 / 60 / 10 |
| `close(retain=[40-byte out])`: promote 40 | 30 | 0 | 20 | 10 | 40 | 0 | 0 | 30 / 60 / 10 |
| free remaining 20 | 30 | 0 | 0 | 10 | 40 | 20 | 0 | 30 / 40 / 10 |
| return credit 30 → Closed | 0 | 0 | 0 | 10 | 40 | 50 | 0 | 0 / 40 / 10 |

At the end, `granted = 100 = 10 + 40 + 50`, and `available = 1000 - 40 - 0 -
10 = 950`.

Failed close (the second free fails):

| Step | credit | in_flight | live | ctx | ret_out | returned | quar | global reserved / live |
|---|---|---|---|---|---|---|---|---|
| after publish two 30-byte buffers | 30 | 0 | 60 | 10 | 0 | 0 | 0 | 30 / 60 |
| `close(retain=[])`: free first 30 | 30 | 0 | 30 | 10 | 0 | 30 | 0 | 30 / 30 |
| second `device_free` fails → poison (rule 7) | 0 | 0 | 0 | 10 | 0 | 30 | 60 | 30 / 30 |
| state `Quarantined`; `Err(SessionPoisoned)` | — | — | — | — | — | — | — | unchanged until reinitialize |

At the end, `granted = 100 = 10 + 30 + 60`. I1 holds because the 30 bytes that
came from credit stay in `reserved` as `quarantined_reserved`, and the record
still exists.

## Error precedence (every ScopeControl call and every ScopedRuntime op)

1. Runtime has no driver or generation: `Unsupported`.
2. `token.id` was never issued by THIS runtime instance (foreign or forged id):
   `InvalidInput / NotStarted`. Ids are per-instance and never reused, and
   issuing one is checked: exhaustion returns `Err(Internal)`, never a wrap.
3. The id is known, but `token.generation` differs from the record's generation
   (forged metadata): `InvalidInput`.
4. The record's generation is older than the current one (a genuine stale
   token): `ContextLost`. On `reinitialize` the registry keeps a tombstone
   `ScopeId -> generation`, so step 4 stays decidable after the record's
   counters are discarded.
5. The current generation is poisoned, or the scope is `Quarantined`:
   `Poisoned / SessionPoisoned`.
6. Wrong state: `Closing` or `Closed` for spend, extend or close (double close),
   or a pinned release: `InvalidInput / NotStarted`.
7. Capacity (charge greater than credit, or extend greater than available):
   `Capacity / NotStarted`, or `Extend::Rejected`.

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
  during a scoped job. This keeps a job close from freeing a cached SRS.
- **The SRS cache accepts only `Persistent` handles.** A scope-funded upload
  cannot be inserted into, or published through, the SRS cache before
  promotion: insertion of a `Scope(_)` handle is `Err(InvalidInput)`. To fund
  an SRS upload from the job, upload it through `ScopedRuntime`, list it in
  `retain`, and insert the now `Persistent` handle after `close_scope` returns
  `Ok`. The promotion (rule 6) is the atomic ownership transfer. It happens
  under the registry lock, before the close result is visible.
- A job may *borrow* a `Persistent` or `Unscoped` handle (read it as an
  input). Borrowing never changes ownership, and close never frees it.

## Plumbing: an explicit per-job backend view (no global binding)

This section covers the actual P1 shape of `src/backend/dispatch.rs`.
`Dispatcher` is a non-generic struct holding counters and policy. Its generic
methods `msm<C, B>(&self, accelerator: Option<(&B, &BackendCapabilities)>, ..)`,
`transform<F, B>(..)` and `try_accelerated_transform(..)` take the accelerator
explicitly. The free helpers `msm_specific`, `fft`, `ifft` and
`accelerated_coset` construct `loki_gpu::LokiGpu` (a unit struct) and call
`GLOBAL.<method>(Some((&gpu, &caps)), ..)`. That shape stays. Only the
accelerator argument changes.

```rust
// loki-gpu: accelerator view over a scope (implements the SAME traits LokiGpu does)
pub struct ScopedAccelerator { runtime: Arc<GpuRuntime>, scope: ScopeToken }
impl<F, G> Backend<F, G> for ScopedAccelerator { /* capabilities as LokiGpu; ops via ScopedRuntime */ }

// Loki src/backend/dispatch.rs (feature loki-gpu-backend)
pub struct BackendContext { accel: loki_gpu::ScopedAccelerator }   // new; explicit, Sync
pub(crate) fn msm_specific_with<C>(ctx: Option<&BackendContext>, coeffs, bases) -> C::Curve;
pub(crate) fn fft_with<F>(ctx: Option<&BackendContext>, a, omega, log_n);
pub(crate) fn ifft_with<F>(ctx: Option<&BackendContext>, a, omega_inv, log_n, divisor);
pub(crate) fn accelerated_coset_with<F>(ctx: Option<&BackendContext>, values, spec) -> Option<Vec<F>>;
// existing msm_specific/fft/ifft/accelerated_coset == *_with(None, ..): unscoped LokiGpu, unchanged
```

With `Some(ctx)`, each `*_with` helper passes `(&ctx.accel, &caps)` to the same
`GLOBAL` method. Counters stay on `GLOBAL`, so attribution is per process,
which is unambiguous under one-heavy. With `None`, it constructs `LokiGpu` as
today. No global or thread-local binding exists. A call site spends the scope
only if it is explicitly handed `Some(&BackendContext)`.

Exact call sites to thread `Option<&BackendContext>` through, all behind
`loki-gpu-backend`, with default builds unchanged:

1. `src/poly/kzg/mod.rs:83` (`commit`) and `:94` (`commit_lagrange`) call
   `msm_specific_with`. These are reached through new defaulted trait methods
   `PolynomialCommitmentScheme::{commit_with, commit_lagrange_with}`; the
   defaults call the existing methods.
2. `src/poly/kzg/msm.rs:146` (`MSMKZG::eval` during multi-open) calls
   `msm_specific_with`, via `multi_open_with`, a new defaulted method.
3. `src/poly/domain.rs:420` (`fft`), `:472` (`ifft`), and `:284` / `:313` via
   `:488`/`:517` (`accelerated_coset`) get new
   `EvaluationDomain::{lagrange_to_coeff_with, coeff_to_extended_with,
   extended_to_coeff_with}`. The existing methods call these with `None`.
4. Prover call sites that switch to the `_with` forms when given a context:
   - `src/plonk/prover.rs`: 10 sites, covering advice/instance
     `lagrange_to_coeff`, commits, h(X) `coeff_to_extended` /
     `extended_to_coeff`, and `multi_open`;
   - `src/plonk/lookup/prover.rs`: 4 sites;
   - `src/plonk/permutation/prover.rs`: 2;
   - `src/plonk/vanishing/prover.rs`: 1;
   - `src/plonk/trash/prover.rs`: 2.

   Keygen (`keygen.rs`, `permutation/keygen.rs`) stays unscoped. It runs
   outside the proof job, or else in its own scope through the same `_with`
   entry (`keygen_*_with`, a later step).
5. Entry point: `create_proof_with_backend(.., ctx: Option<&BackendContext>)`.
   `create_proof` calls it with `None`. The P7 driver passes
   `Some(&BackendContext::new(permit.scoped_accelerator()))`.
6. Rayon closures capture `Option<&BackendContext>`, which is `Copy` and
   `Sync`.

## Scheduler changes (P7, `schedule.rs`)

- `ReservationSource` gains `open_scope(&ReservationToken)` and
  `close_scope(&ScopeToken, &[AllocationId])`.
- Admission order stays heavy lock, GPU lock, `try_reserve`, and adds
  `open_scope(&token)`. If `open_scope` fails, the scheduler releases the still
  valid reservation with the same token, then unwinds the locks.
- `HeavyPermit` holds a `ScopeToken` and exposes `scoped_accelerator()`.
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
| `stale_token_is_context_lost_everywhere` | genuine old-generation token (tombstone, precedence 4) on every call after `reinitialize` |
| `double_close_is_invalid_input` | release-authority edge |
| `unscoped_and_global_dispatch_never_spend_scope_credit` | I5; a concurrent global-dispatcher MSM during a scoped job |
| `concurrent_scoped_ops_preserve_i2` | Rayon stress; snapshot after every step |
| `healthy_close_trace_matches_table` | each row of the healthy trace: snapshot and global counters after every step; I1–I4 checked each time |
| `failed_close_trace_matches_table` | FakeDriver `DeviceFree` fault on the second free; `Quarantined`; I1/I2 hold until reinitialize |
| `duplicate_retain_id_is_invalid_input_without_mutation` | close step 1 |
| `pinned_retain_cannot_be_released_during_close_wait` | a concurrent release during the `active_ops` wait gets `InvalidInput`; the promotion succeeds |
| `scope_funded_handle_cannot_enter_srs_cache_before_promotion` | SRS cache insertion of a `Scope(_)` handle is refused; the same handle is accepted after close |
| `error_precedence_is_total` | each precedence row 1–7 with a case that satisfies all lower-numbered checks |
| `foreign_runtime_scope_token_is_invalid_input` | a token minted by runtime A is refused by runtime B (ids are per-instance) |
| `forged_scope_metadata_is_invalid_input` | unknown id, or known id with a generation that differs from its record (precedence 2–3), on every call |
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
