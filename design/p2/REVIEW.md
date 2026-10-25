# P2 — transactional device runtime and versioned ABI: implementation record

Owner: loki-p2 (Opus implementer). Base: loki-gpu `df9584f`; Lean gpu-spec `9a32d2f`.
Labels follow the repository convention (CODE / MEASURED / OPEN QUESTION).

## Commits

| Tree | Commit | Content |
|---|---|---|
| loki-gpu `p2-runtime` | `ba28793` | runtime coordinator, native ABI, tests |
| loki-gpu `p2-runtime` | (this record) | P2 review record, verification logs, PROVENANCE row |
| gpu-spec `gpu-spec` | `21f208d` | `LokiLean/Backend.lean`: `Loki.Backend.Runtime` model and theorems |

## What was implemented (CODE)

- `src/runtime.rs` — one coordinator (`GpuRuntime`) over a `DeviceDriver` seam.
  - Lock order device(1) → registry(2) → telemetry(3), enforced at every acquisition by
    `OrderedMutex` (thread-local level mask; inversion or re-entry panics). The registry lock is
    never held across a driver call, so `try_reserve` / snapshots never wait for device work
    (tested with a paused in-flight copy).
  - Checked shared budget: `min(cap, free − headroom)` (saturating); every allocation reserved for
    its full size before `device_alloc`, released only at publication or retirement.
    Reservations are single-use, validated against id + generation + bytes.
  - Allocation registry: never-reused ids; every use/release validates generation, element
    `TypeId`, form, length and bytes. Older generation → `ContextLost/NotStarted`; poisoned
    current generation → `Poisoned/SessionPoisoned`; forged/unknown/duplicate → `InvalidInput`.
  - Staging: `staging_slots` (default 2, hard cap 4) × `staging_slot_bytes` (≤ 32 MiB), probed
    slot-by-slot with `cudaHostAlloc`; failure falls back to backend-owned pageable (malloc)
    staging, counted in `pinned_probe_failures`; `StagingCapability::async_overlap_available` is
    false when no slot is pinned. Caller slices are never DMA endpoints; slots are rewritten/read
    only after their event is synchronized.
  - Long-lived non-blocking streams (1 copy + 2 compute) and events created at init only.
  - Publication: upload handle / download vector / launch handle only after the completion event
    synchronizes; download decodes (`from_repr`) every element before returning.
  - Errors: driver errors are `Transient | Poisoned | ContextLost | Unsupported`. Before any
    enqueue → `NotStarted`. After an enqueue → drain the stream; success → retire and
    `InputsPreserved`; drain failure / any event-wait failure / sticky error → poison the
    generation, quarantine every allocation and staging buffer (never freed through that context,
    including at `Drop`) → `SessionPoisoned`. `reinitialize()` (only from poisoned) resets the
    driver and creates a new generation and epoch.
  - Cancellation (`CancelToken`): checked at chunk boundaries; drains and retires in-flight work,
    never writes caller output (`Internal/InputsPreserved`; `NotStarted` if nothing enqueued).
  - Host checkpoint distinction: `upload_scalars_with_checkpoint` (bounded by
    `checkpoint_budget_bytes`); `loss_class()` reports `Live | RecoverableFromHostCheckpoint |
    Unrecoverable | Unknown`; `recover_checkpoint()` returns retained host bytes only for lost
    checkpointed handles.
  - `duplicate_scalars`: runtime self-test launch (device byte copy) exercising
    reserve → alloc → launch → sync → publish. Not arithmetic.
  - `runtime::fake::FakeDriver`: deferred-execution fake that runs copies/launches only at
    event/stream sync and records `Violation`s for staging access while pending, free while
    pending, double free, use after free, stale-epoch use and destruction through a poisoned
    context. Mutation check: removing the slot wait (12+ tests fail), the drain-before-free
    (6), poisoned-Drop leak (2) or the length check (1) is caught.
- `cuda/runtime.cu` + header — `loki_gpu_rt_*` v1 ABI: versioned 32-byte handles
  `{abi_version, kind, context_epoch, raw, bytes}`, 16-byte status `{code, cuda_error, sticky}`,
  `loki_gpu_rt_abi_layout_v1`, new status codes 4–9, bounds/epoch/kind/version validation before
  any dereference, sticky-error classification, grid-stride copy kernel. Built with
  `-gencode=arch=compute_120,code=sm_120 -gencode=arch=compute_80,code=compute_80`
  (cuobjdump: `runtime.sm_120.cubin`, `runtime.sm_80.ptx`).
- `src/ffi.rs` — `#[repr(C)]` mirrors with const size assertions, `NativeDriver`. `ffi` is now a
  public module (mirrors/constants; raw externs stay crate-private).
- `LokiGpu::capabilities` delegates to the process runtime: only an explicitly initialized
  healthy runtime reports `resident_buffers`; no arithmetic capability is advertised.
- Lean `Loki.Backend.Runtime`: `upload_ok_completion`, `download_ok_completion`,
  `download_poisoned`, `*_keeps_poison`, `reinit_advances`, `stale_after_reinit`,
  `forged_rejected`, `release_twice_rejected`, `upload_new_identity`, `upload/download/release/
  tryReserve/reinit_inv` (`live + reserved ≤ budget`), `roundTrip_contract` (meets the existing
  `Contract`) and `roundTrip_dispatch_eq_host`. No `sorry`; axioms ⊆ {propext, Classical.choice,
  Quot.sound}.

## Gates (MEASURED, host RTX 5090 / WSL2, rustc 1.99.1, nvcc 13.4 V13.4.92)

| Gate | Command | Result | Log |
|---|---|---|---|
| no-toolkit default | `env -u CUDA_HOME cargo check --workspace --locked` | exit 0 | [log](verification/default-check.txt) |
| runtime contract | `cargo test -p loki-gpu --locked --test runtime_contract` | 39 passed | [log](verification/runtime-contract.txt) |
| native ABI | `flock gpu.lock cargo test -p loki-gpu --locked --features cuda --test ffi_abi` (`LOKI_GPU_REQUIRE_DEVICE=1`) | 10 passed, 1 ignored (measurement) | [log](verification/ffi-abi.txt) |
| CUDA lib unit tests | `flock gpu.lock cargo test -p loki-gpu --locked --features cuda --lib` | 3 passed | [log](verification/cuda-lib.txt) |
| Lean | `lake build LokiLean.Backend`; full `lake build`; `#print axioms` | exit 0 (8930 / 8939 jobs) | [log](verification/lean-backend.txt) |
| compute-sanitizer | memcheck `--leak-check full`, racecheck, synccheck, initcheck on the 7 native tests | 0 errors, 0 leaks, 0 hazards | [log](verification/compute-sanitizer.txt) |
| pinned budget | `measure_pinned_budget` (ignored test), heavy+gpu lock | 4096 MiB pinned (128 × 32 MiB) in 2.14 s, no failure | [log](verification/pinned-probe.txt) |

Timing (debug build, MEASURED, not a performance claim): 32 MiB upload 81 ms wall / download
243 ms wall through two 32 MiB pinned slots; init 20–42 ms. Host-side encode/decode in debug
mode dominates; no overlap or latency gain is claimed.

## Open risks and handoffs

- **API (coordinator)**: no `ErrorKind::Cancelled`; cancellation maps to `Internal`. Proposed
  diff to `loki-backend-api`: add `Cancelled` to `ErrorKind`.
- `upload_points` is `Unsupported`: generic `group::Curve` has no wire encoding; SRS points need
  P3's concrete adapter (or an API encoding bound).
- Element storage form is canonical `PrimeField::to_repr` bytes; P4+/P6 kernels expecting the
  documented Montgomery limb wire format must convert or add a new `Form`.
- WSL2 pinned limit not reproduced: 4 GiB pinned succeeded here (kernel 6.18, driver 13.04);
  the ~500 MB community report may apply to other WSL/driver versions. Pageable fallback is tested
  with both the fake and the native driver.
- Sticky-error list in `runtime.cu` is curated from the Runtime API enum; no real sticky fault was
  injected on hardware (would poison a shared GPU). Covered by the fake only.
- `reinitialize` calls `cudaDeviceReset`, which also destroys any other CUDA state in the process
  (e.g. legacy sppark). Only one `GpuRuntime` per process should own the native driver.
- Download holds a full private byte copy plus the decoded vector (2× host memory transiently).
- `BUILD.md`/`RUNTIME.md` (other owners) still describe the pre-P2 skeleton (only stubs, three
  `.cu` files); they need an owner update. `Cargo.lock` changed by two dev-dependency lines
  (`rand_core`, `subtle`, already locked transitively).
- Checkpoint bytes leak across poison: a checkpointed handle cannot be released while poisoned
  (`Poisoned`) or after reinit (`ContextLost`); only `recover_checkpoint` returns its
  `checkpoint_bytes`. If Loki aborts without recovering, repeated poison cycles can exhaust
  `checkpoint_budget_bytes`. Needs an explicit discard path (follow-up).
- Pageable staging leaks after a native reset: `cudaDeviceReset` frees pinned slots but not the
  `malloc`'d pageable ones, which the retired-context path never frees (bounded: ≤ 128 MiB per
  poison/reset event).
- `ffi_abi` now fails when no device is present unless `LOKI_GPU_ALLOW_NO_DEVICE=1`, so the
  TEST-PLAN gate command cannot pass silently on a toolkit-only host.
- Lean model abstracts bytes as element counts and does not prove staging/free lifetime
  (`runtime-lifetime` stays a test/sanitizer obligation).

## P2-fix — corrective work for the independent review (D1–D5 of cefecbc)

Branch `p2-fix` from main cefecbc. astra-gpu's reproducers are vendored unchanged as
`crates/loki-gpu/tests/review_regressions.rs` (4/4 failed before, pass after). Wider
coverage is in `crates/loki-gpu/tests/p2_fix_regressions.rs`.

| Finding | Fix |
|---|---|
| D1 runtime-local identity | Allocation ids, reservation ids and generations come from process-global counters (`NEXT_ID`, `NEXT_GENERATION`) through `checked_next`. That is a CAS loop that returns `None` at `u64::MAX` and never wraps; exhaustion is `Capacity`/`NotStarted`. Ids are issued before any device work, so exhaustion can't strand a completed transfer. A poisoned in-flight allocation goes to `Context::quarantined` and never consumes an id. Tests: unit `identity_counter_fails_instead_of_wrapping`; foreign download, release, duplicate, reservation and checkpoint are all rejected with no effect. |
| D2 teardown / setup cleanup ignored results | `Drop` runs `teardown()`, which checks every sync/free/destroy and returns at the first failure, leaking the rest. Setup cleanup also stops at the first failed destroy (`'cleanup` block). Tests: Poisoned, ContextLost and Transient injected at calls 1–8 of each teardown call kind, and at each setup-create × cleanup-destroy boundary; no violations and no double frees. |
| D3 timing results ignored | `optional()` ignores only Transient/Unsupported. Poisoned/ContextLost from a timing `record_event` or `event_elapsed_ms` poisons the generation, quarantines in-flight memory and aborts with `SessionPoisoned` before publication. Covered on upload, download and launch; transient timing failures are shown to be ignored. |
| D4 native driver public | `NativeDriver`/`native_driver` are `pub(crate)`. The only entry is `GpuRuntime::native(config)`, a process-unique claim (`NATIVE_CLAIMED`); a second live claim gets `InvalidInput`. The claim is released on healthy or never-initialized drop and kept on poisoned drop. `process()` uses it. `NativeDriver` records every handle it minted and rejects unknown or freed handles as `Poisoned` before calling CUDA; reset forgets all of them. compile_fail doctests (4) show that `native_driver`, `NativeDriver`, `DeviceMemory(handle)` and `DriverHandle::raw` are unreachable from outside the crate. |
| D5 per-thread device | `runtime.cu` keeps `g_ordinal` (set by a successful probe/reset; a device switch bumps the epoch). `ensure_device()` (`cudaGetDevice`/`cudaSetDevice`) runs on all 17 device-touching entry points after handle validation. Native tests: initialize, upload, download, duplicate and release on separate host threads; an out-of-range ordinal gives `Unsupported`/`NotStarted`, and ordinal 0 still works afterwards. On this single-GPU host a new thread already defaults to device 0, so the cross-thread test proves the path works but can't detect a D5 regression here; that needs a multi-GPU host. |
| API `Cancelled` | Cancellation reports `ErrorKind::Cancelled` (NotStarted before submission, InputsPreserved after a successful drain). A failed retirement or free reports the actual driver category with `SessionPoisoned`. |

Gates (logs in `verification/p2-fix-*.txt`):
- No GPU: review_regressions 4/4, p2_fix_regressions 10/10, runtime_contract 39/39, lib 3/3. Clippy is clean for loki-gpu with and without `cuda`.
- No-toolkit default check plus `cargo test --workspace`: green.
- Native (RTX 5090, under gpu.lock): ffi_abi 13 passed, 1 ignored (`measure_pinned_budget`); cuda lib 4/4; doctests 4/4.
- compute-sanitizer memcheck (leak-check full), racecheck, synccheck and initcheck: all exit 0, 0 errors, 0 bytes leaked.
- The vendored `review_regressions.rs` is byte-identical to `design/verification/p2-review-regressions.rs`.
- `cargo fmt --check` was not enforced: the base cefecbc is already not rustfmt-clean (including the vendored reproducer), so reformatting shared files is left to a separate change.
- Not natively tested: a poisoned drop keeping the native claim (that needs a real sticky CUDA error). It is covered only by inspection, and by fake-driver poisoned-drop tests for the destruction side.
- `lake build LokiLean.Backend` at gpu-spec 21f208d: green. The model is unchanged, since it abstracts identity allocation and native ownership.
