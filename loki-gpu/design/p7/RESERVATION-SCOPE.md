# P7 reservation scope: redeeming an admission reservation into allocations

Status: DESIGN ONLY (P7 owns this file). P4 reviews the runtime parts by message.
Nothing here changes shared runtime code yet. It closes review finding S4 by
turning the scheduler's "admission prototype" into an implementable contract.

## Problem (CODE, base 0c4ab9d)

`GpuRuntime` accounts for device memory in its registry as follows (runtime.rs
`Registry`):

- `available = budget - live - reserved`;
- `try_reserve` adds the requested bytes to `reserved` and records a
  `ReservationRecord`;
- each internal allocation checks `bytes <= available`, adds `bytes` to
  `reserved` while in flight, and on publication moves them: `reserved -= bytes`
  and `live += bytes`;
- `release` does `live -= record.bytes`.

A `HeavyPermit` that holds a reservation of R bytes therefore lowers
`available` by R. The admitted job's own uploads and launches then need a
further R from the same budget. The job's workspace is counted twice, and a job
that reserves its full need excludes itself.

## Proposal: an explicit job scope

A scope is a reservation that allocations can spend. It is an explicit object,
not ambient thread-local state, because Rayon moves work across threads.

```rust
// loki-backend-api (new types; existing calls unchanged)
pub struct ScopeId(pub u64);
pub struct ScopeToken { id: ScopeId, generation: DeviceGeneration, granted: u64 } // untrusted metadata, like ReservationToken

pub trait ScopedResidentBackend<F, G>: ResidentBackend<F, G> {
    /// Atomically convert a held reservation into scope credit (no bytes move:
    /// `reserved` is unchanged; the ReservationRecord becomes a ScopeRecord).
    fn open_scope(&self, reservation: ReservationToken) -> Result<ScopeToken, BackendError>;
    /// All-or-nothing: grow the scope's credit from the shared `available`.
    fn extend_scope(&self, scope: &ScopeToken, bytes: u64) -> Result<ReservationResult, BackendError>;
    /// The existing v1 operations, charged to the scope instead of `available`.
    fn upload_scalars_in(&self, scope: &ScopeToken, values: &[F]) -> Result<ResidentBuffer<F>, BackendError>;
    // ... one `_in` variant per allocating v1 call (upload_bases, transform, msm workspace, copy)
    /// Close: free every scoped allocation not in `retain`, re-tag retained ones as
    /// unscoped (they persist and stay `live`), return the remaining credit.
    fn close_scope(&self, scope: ScopeToken, retain: &[AllocationId]) -> Result<ScopeClose, BackendError>;
    fn scope_snapshot(&self, scope: &ScopeToken) -> Result<ScopeSnapshot, BackendError>;
}

pub struct ScopeSnapshot { pub granted: u64, pub credit: u64, pub in_flight: u64, pub live: u64 }
pub enum ScopeClose { Closed { freed: u64, retained: u64, credit_returned: u64 }, Stale /* generation gone */ }
```

The registry gains a `scopes: HashMap<ScopeId, ScopeRecord { generation,
granted, credit, in_flight, live }>`. `AllocRecord` gains `scope:
Option<ScopeId>`.

### Charging rules (one registry critical section each)

1. **Charge (before allocation).** An op that needs a total of `b` bytes,
   summed over all of its allocations, takes the following path. If the scope
   is current and `b <= credit`: `credit -= b` and `in_flight += b`. `reserved`
   is unchanged, because those bytes are already inside it. Otherwise it returns
   `Capacity / NotStarted` without touching the scope. This is the all-or-nothing
   multi-allocation rule: a multi-buffer op either gets all of its bytes or
   none, and partial charges are impossible.
2. **Publish (success).** For each published allocation: `in_flight -= bytes`,
   `live += bytes` (scope), `reserved -= bytes` and `live += bytes` (global),
   and `AllocRecord.scope = Some(id)`.
3. **Abort before publication** (`NotStarted` / `InputsPreserved`): `in_flight
   -= bytes` and `credit += bytes`.
4. **Free a scoped allocation.** Only after `device_free` succeeds: global
   `live -= bytes`, then `reserved += bytes`, scope `live -= bytes` and `credit +=
   bytes` (reusable workspace credit). If `device_free` fails, the generation is
   poisoned as today and no credit is returned.
5. **Free an unscoped allocation:** unchanged from today.

### Invariants

- **I1:** `reserved = Σ unscoped reservations + Σ_open scopes (credit +
  in_flight) + Σ unscoped internal in-flight`.
- **I2:** for every open scope, `credit + in_flight + live == granted`, where
  `granted` grows only through `extend_scope`.
- **I3:** `live + reserved <= budget`, unchanged.
- **I4:** a byte is counted exactly once: in `available`, in one scope's
  credit/in_flight, or in `live`.
- **I5:** scoped ops never draw from `available`. Only `extend_scope` does, and
  it is atomic.

### Lifetime and release authority

- **Release authority.** Only the holder of the `ScopeToken` can close the
  scope, which in practice is the scheduler's `HeavyPermit`. Freeing an
  allocation (any holder of the `ResidentBuffer`) returns credit to its scope
  but never closes it.
- **Retained outputs and SRS persistence on close.** Allocations listed in
  `retain` become unscoped and stay `live`, so they outlive the job. Examples
  are a persistent SRS upload, or a resident output that the next job imports.
  Their bytes leave the scope (`granted -= retained`). Everything else in the
  scope is freed. Freeing is safe because every v1 call retires its work before
  returning.
- **Cancel** is `close_scope(token, &[])`. **Failure** is the same. A permit
  `Drop` calls `close_scope(token, &[])` and records the outcome.
- **Reset, poison and generation change.** A scope belongs to its generation.
  When the generation is poisoned, `require_healthy` fails every `_in` call
  (`Poisoned / SessionPoisoned`). After `reinitialize`, the scope record is
  dropped with the old registry generation (`reserved` restarts at 0, as today).
  Any `_in` or `close_scope` on the old token then returns `ContextLost`, and
  `close_scope` returns `ScopeClose::Stale`. Quarantined allocations are never
  freed or credited. The scheduler maps `Stale` to the S5 rule: no completion
  is published.

### Concurrent host callers

Every scope mutation happens under the registry mutex (lock level 2), and is
never held across a driver call. Device work stays serialized by the `device`
lock. Several Rayon threads can call `_in` operations on the same `&ScopeToken`
concurrently: the charges serialize at the registry, and rule 1 makes each
op's charge atomic. An unscoped caller (another process thread, or an
out-of-job warmup) keeps using `available`, and can never take a scope's
credit.

## Integration changes

### Scheduler (P7, `schedule.rs`)

- `ReservationSource` gains `open_scope` and `close_scope`.
- After `try_reserve` succeeds, admission calls `open_scope`. If that fails, the
  reservation is released and admission is `Deferred` or an error.
- `HeavyPermit` holds a `ScopeToken`, not a `ReservationToken`, and exposes
  `scope() -> &ScopeToken`.
- `complete(identity, retain)` follows S5:
  1. `check_current`;
  2. `close_scope(retain)`;
  3. publishes only on `Closed`.
- `fail`, `cancel` and `Drop` call `close_scope(&[])`.
- The prototype label in the module documentation is removed once these land.

### Dispatcher (Loki `backend::dispatch`)

It gets a scoped backend view, not ambient state:
`ScopedRuntime { runtime: Arc<GpuRuntime>, scope: ScopeToken }` implements
`ResidentBackend` by forwarding to the `_in` variants. The job installs it with
`Dispatcher::bind_job(ScopedRuntime) -> JobBinding`, an RAII guard. Binding
fails if another binding is live, which matches the one-heavy invariant. While
it is bound, all dispatch (from any Rayon thread) uses the scoped view. When
unbound, the dispatcher falls back to unscoped calls, as today.

### P2 runtime (P4-owned)

The P2 runtime needs:

- the registry fields and charging rules above;
- the `_in` variants (each a thin wrapper over the existing internal path,
  parameterized by a charge source: `Available` or `Scope(id)`);
- `open_scope`, `extend_scope`, `close_scope` and `scope_snapshot`.

The v1 operations are unchanged.

## Tests to add (implementation phase)

| Test | Proves |
|---|---|
| `scope_open_keeps_reserved_constant` | I1/I4 at open: `reserved`, `available` unchanged; `credit == granted`. |
| `scoped_upload_spends_credit_not_available` | rule 1/2; `available` unchanged across a scoped upload of exactly `granted` bytes (the S4 self-exclusion case now succeeds). |
| `scoped_overdraw_is_capacity_not_started` | rule 1; credit and `available` untouched. |
| `multi_buffer_op_is_all_or_nothing` | rule 1 with a two-allocation op whose sum exceeds credit by one byte. |
| `abort_before_publish_returns_credit` | rule 3 using FakeDriver `DeviceAlloc` / `LaunchCopy` faults. |
| `free_returns_reusable_credit` | rule 4; a second upload of the same size succeeds within the same scope. |
| `failed_device_free_poisons_and_returns_no_credit` | rule 4 failure branch. |
| `close_retains_listed_outputs_and_frees_the_rest` | retained allocations stay `live` and usable after close; the others are freed; credit returned; I1–I3 hold after close. |
| `cancel_and_drop_close_with_no_retention` | the scheduler's `fail`/`cancel`/`Drop` paths. |
| `scope_after_reset_is_stale_and_publishes_nothing` | the generation rule plus the S5 scheduler mapping. |
| `poisoned_scope_refuses_scoped_ops` | `SessionPoisoned` on `_in` after a sticky fault. |
| `concurrent_scoped_uploads_never_exceed_granted` | Rayon stress, N threads × random sizes; I2 checked after each step via `scope_snapshot`. |
| `unscoped_caller_cannot_spend_scope_credit` | I5. |
| `second_dispatcher_binding_is_refused` | `bind_job` exclusivity. |
| property: random op sequences on FakeDriver | I1–I5 after every step (the Quint Q3 runtime model can carry the same invariants). |

Every test runs on `FakeDriver` through the safe `with_fake` / `FakeControl`
API from corrected P4. One native receipt then repeats
`scoped_upload_spends_credit_not_available` and
`close_retains_listed_outputs_and_frees_the_rest` on the RTX 5090.

## Open questions for P4

1. Does any current internal path allocate more than once per v1 call outside a
   single registry section? If so, its `_in` variant must pre-charge the sum.
2. Checkpoint bytes are host memory. They should stay outside scope accounting.
   Please confirm.
3. Pageable-slot retention across reset (P4 REVIEW item 9) interacts with the
   rule that quarantined allocations are never credited. Is the proposed fix
   compatible with that?
