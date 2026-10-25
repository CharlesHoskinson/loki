# Device runtime, residency, and SRS leases

**Status:** design for the first `loki-gpu` runtime. CUDA operations remain
stubs until the kernel packages are implemented. The Rust types in this
package describe ownership and failure rules; they do not claim measured
overlap or concurrent-proof throughput.

**Evidence convention:** unless explicitly labeled `SOURCE`, `MEASURED`,
`ESTIMATE`, or `OPEN QUESTION`, normative design prose is `CODE (proposed)`;
it describes a contract or implementation proposal, not evidence that the
runtime already implements it.

## Runtime ownership

`loki-backend-api` owns the backend-neutral operation and resident-buffer
contracts. `loki-gpu` owns device discovery, CUDA streams/events, allocations,
pinned staging, and its internal SRS registry. Loki owns backend selection,
proof ordering, transcript operations, CPU fallback, and admission of proof
jobs. The CPU implementation remains the default and has no dependency on a
CUDA runtime. The legacy sppark path remains separately opt-in; the new backend
is also opt-in and must not change Loki's stock default feature graph.

At runtime, one process-wide device coordinator is shared by MSM, NTT, and
quotient modules. It selects one CUDA device/context, verifies the runtime
compute capability, and provides a single lock order:

1. device coordinator / context guard;
2. SRS registry or allocator metadata lock;
3. per-operation stream/event bookkeeping.

Do not wait for Rayon work while holding any of these locks. Do not call CUDA
or release a device handle while holding a registry-only lock. The current
sppark integration serializes device calls; keep that safe policy until
multi-stream and multi-proof measurements establish a better one. Independent
CPU witness preparation may overlap a GPU operation when the proof scheduler
can guarantee that it does not mutate the operation's immutable inputs.

## Device generations and resident buffers

Every initialized context has a monotonically increasing generation. A
resident handle is an opaque allocation identifier paired with
`(device_id, context_generation)`, element type, logical length, capacity, and
accounted bytes. The private allocation record also binds the data
representation and, where relevant, polynomial basis/domain or SRS basis. The
raw CUDA pointer is private to the FFI module and never crosses the API
boundary. Operations reject a handle from another device, a stale generation,
or mismatched type/form/domain metadata before enqueueing work. Context reset,
fatal device error, or backend reinitialization advances the generation and
invalidates all old handles.

ResidentBuffer handles are opaque, non-owning references; the runtime registry
owns device allocations and validates every handle against its generation,
allocation id, type, length and byte count. A handle does not free an allocation
when dropped. Explicit release marks it eligible for deferred destruction
after its last operation and SRS lease complete. The public v1 API is blocking
at its host-ownership boundary: upload copies the caller slice into
backend-owned staging and returns only after the transfer retires; download
returns a fresh host vector after transfer and validation; resident transforms return a new handle
only after successful completion. Caller slices are never DMA targets. The
runtime may use async streams internally, but a completion registry owns each
staging allocation and event until retirement. Direct asynchronous DMA to or
from caller slices is deferred; a future async public API needs an owned
completion object whose safety does not rely on its destructor running.
Device-error cleanup that cannot prove completion marks the context unhealthy
and retains uncertain handles until process teardown.

Host outputs are transactional. Kernels write into private device scratch;
the backend copies into a caller-visible host result only after the completion
event succeeds and output decoding/validation succeeds. On any error, the
caller-visible output and all immutable host inputs retain their prior values.
Loki may then recompute from those original inputs using the stock CPU path.
If the only source is a resident buffer, CPU fallback is permitted only while
the backend retains a complete host checkpoint. Without that checkpoint, a
device fault is unrecoverable for the current proof and the prover must abort;
the backend must not claim that lost device data can be recomputed. An error
after enqueue may change context health and generation, but does not publish a
partial output or advance transcript state. No partial GPU output is used as a
CPU fallback input.

## Streams, copies, and pinned staging

The initial stream topology is one copy stream and two compute streams per
device. Operations record explicit events for copy-to-compute, compute-to-copy,
and cross-stream dependencies. The public v1 calls wait for their completion
boundary before returning; the runtime still uses async streams internally to
batch transfers and kernels. The scheduler batches columns within a proof
phase and preserves the program order of results; stream concurrency never
reorders transcript events. The first production policy may serialize all
backend calls under the process coordinator even though the runtime has
multiple streams.

Use two pinned-host slots initially, each at most 32 MiB, for bulk transfers;
the hard cap is four slots (128 MiB total), and expansion requires measured
benefit and a separate host-memory-budget review. Pinned host staging is
accounted independently from the CUDA device-allocation budget. Check every
pinned allocation. If allocation fails, use backend-owned pageable staging
and report that overlap is unavailable for that transfer; pageable
`cudaMemcpyAsync` is not evidence of asynchronous overlap. Never call
`cudaHostRegister` on a Rust `Vec`. A slot cannot be reused until its
completion event has fired and its host copy has returned. Large transfers are
chunked through the ring rather than increasing its size without a memory
budget review.

The runtime reports copy bytes, pinned/pageable path, allocation high-water,
stream wait time, and CUDA event duration separately from wall time. Initialization
and first-use/JIT time are reported separately from warm calls. Error handling
checks every CUDA API result, including teardown. `sm_120` native code and the
portable PTX fallback are build properties, while actual compute capability is
queried at runtime; unsupported devices return `Unsupported` and allow Loki's
CPU path.

## Memory budget and proof scheduling

All modules charge device allocations to a shared runtime budget: persistent
SRS and twiddle data, live inputs/outputs, operation scratch and sorting
temporaries, and delayed frees. Pinned host staging is charged to its separate
bounded host-memory budget, not to device allocation. At initialization,
available device memory is queried; the usable device budget is the lower of
an explicit operator cap and available memory minus reserved headroom. Every
device allocation uses `try_reserve` for its full accounted size before launch
and releases the reservation only after completion. A failed reservation
returns an error before mutating inputs or publishing an output. This prevents
quotient scratch, NTT buffers, MSM scratch, and resident SRS data from
independently assuming the same free memory.

Expose a snapshot to the backend-neutral API containing device generation,
health, hard budget, live bytes, and reserved bytes. The snapshot is
observational only; scheduler admission must use authoritative `try_reserve`
and must not assume capacity from a snapshot. Loki must not read CUDA
internals or rely on a stale free-memory sample. If the context generation
changes, all reservations and resident handles from the prior generation are
invalid.

Keep the separately configurable SRS budget at or below 4 GiB by default. SRS
accounting includes persistent point storage, Pippenger buckets, and histogram
tables from the pinned sppark version, not only the 96-byte affine point wire
array. Invocation scratch is charged to the shared runtime budget, not silently
to the SRS allowance. Reject an entry whose estimated persistent footprint
exceeds the budget before upload. Keep accounting conservative and label any
constructor-derived footprint as an estimate until allocator usage is measured.

The proof scheduler is outside the runtime crate. Until concurrent k19/k20
proofs have been measured on the shared RTX 5090, admit at most one heavy proof
to the device at a time. Other proofs queue; they must not hold SRS leases or
large witness buffers while waiting. A proof releases phase-local scratch as
soon as its completion is observed. A future scheduler can use runtime
capabilities and live-byte reservations to admit several proofs, but that
policy requires measured peak allocation and end-to-end chain throughput, not
just successful sequential allocations.

## SRS registry and lease semantics

The registry is per device generation. Identity binds device identity, context
generation, basis kind (coefficient or Lagrange), point representation,
logical capacity, and validated source contents. Host address may index a
candidate quickly, but it is neither sufficient identity nor a substitute for
content equality; digest-only identity is also insufficient. A hit compares
the exact requested prefix against an immutable snapshot of every projective
x/y/z coordinate. A changed value at the same address is a miss. The unused
suffix of a larger cached basis need not match a shorter request.

Each entry tracks persistent device bytes, last-use sequence, and active lease
count. Acquire returns an immutable lease bound to the device generation and
requested prefix length. Release the lease after all dependent stream work
completes. A lease that is dropped without a confirmed release stays pinned in
the registry and may cost capacity, but it cannot be evicted while work may
still reference it. Eviction is LRU among entries with zero active leases; if
the budget cannot fit a new entry without evicting a leased one, return an allocation
error before upload. Upload candidates are checked for curve and subgroup
membership before they become visible in the registry. Validation and upload
failures publish no cache entry. A fatal device error makes the registry
unhealthy; uncertain allocations are retained rather than destructed through a
possibly poisoned context.

The first compatible implementation may use host snapshot equality rather than
a cryptographic digest. It must preserve exact comparison semantics even if
the internal data representation changes. The registry is an optimization:
on a cache miss, capacity refusal, or unsupported device, Loki can execute the
same operation on CPU from its unchanged host bases.

## Capability and error behavior

Capability discovery is informational and conservative. `BackendCapabilities`
contains operation support, supported arithmetic ABIs, a maximum transform
log-size, and an optional device-memory budget. Device generation and current
health belong to `RuntimeSnapshot`. Async-copy availability and pinned-ring
initialization are internal runtime telemetry, not current API capabilities.
Capability absence returns `Unsupported`; malformed lengths/roots return an
input error; exhausted budget returns an allocation error; stale handles return
a generation error; CUDA failures return a device error and mark the context
unhealthy if completion is uncertain; fatal context errors disable dispatch
until explicit reinitialization. Loki owns policy: it may fall back to
CPU for `Unsupported`, allocation, or recoverable setup failures. It must not
accept a GPU result after any failure, and fallback must preserve transcript
draw order and proof-visible ordering.

## Integration boundary

The Loki adapter routes only the arithmetic calls through the backend API:
MSM dispatch (`src/poly/kzg/msm.rs::msm_specific`), domain FFT/iFFT
(`src/poly/domain.rs::{fft,ifft}`), and the quotient/evaluation path
(`src/plonk/prover.rs` and `src/plonk/evaluation.rs`). SRS leases and resident
polynomials may cross phase boundaries inside one proof session; the transcript
and proof scheduler do not. Default CPU dispatch must continue to call the
stock operations without constructing this runtime.

## Evidence and limits

- **SOURCE:** the clean-room WP1/WP3 design in
  `Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md` specifies a
  bounded pinned ring, resident buffers, and shared-memory motivation.
- **CODE:** W13's `cuda-runtime/src/lib.rs` serializes sppark operations, and
  its `src/poly/kzg/msm/cuda/backend.rs` acquires the process guard before the
  registry lock, validates uploaded bases, and keeps uncertain handles alive
  after device errors. The W13 registry plan specifies exact projective
  snapshot comparison, a 4 GiB default persistent-SRS budget, and accounting
  for persistent sppark allocations.
- **CODE:** W5's `LOKI-GPU-NTT.md` reports that the old NTT adapter stages a
  second host vector and copies only after synchronization; MSM and NTT
  currently have separate locks in that worktree, so shared serialization is
  an integration requirement.
- **MEASURED:** the lab's X6 result covers sequential k14 calls and pool
  high-water deltas only; it does not establish safe concurrent heavy proofs
  or a k20 peak-memory bound.
- **OPEN QUESTION:** whether the pinned sppark release exposes exact persistent
  allocation sizes. If it does not, the adapter must use a version-pinned
  conservative footprint estimator and keep the measurement limitation
  visible.
