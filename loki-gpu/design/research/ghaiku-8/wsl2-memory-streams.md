# ghaiku-8: WSL2 pinned memory, stream-ordered allocation, async errors (accessed 2026-10-25)

Labels: SOURCE = official page read; COMMUNITY = forum or issue; OPEN = unverified.

## Pinned memory
- SOURCE (Runtime API 13.4): cudaHostAlloc and cudaMallocHost can fail with cudaErrorMemoryAllocation. cudaHostRegister works only on I/O-coherent devices and can return cudaErrorNotSupported.
- SOURCE (Programming Guide 2.5.2.3): cudaMemcpyAsync from pageable memory falls back to synchronous.
- SOURCE (WSL User Guide 5.1): pinned memory availability is "limited". No number is given.
- COMMUNITY (microsoft/WSL#14078): PyTorch pin_memory under WSL2 failed between 500 and 600 MB, while native Windows handled about 4 GB. Raising memlock did not help.
- Recommendation:
  - Probe the pinned budget at init.
  - Use a bounded, chunked pinned pool (4–16 MiB chunks; an estimate).
  - On failure, fall back to pageable synchronous copies and record that as a capability flag.
  - Don't rely on cudaHostRegister on WSL2.
  - OPEN: whether memlock matters; measure it on gpu-runner-2.

## Stream-ordered allocation
- cudaMallocAsync and cudaMallocFromPoolAsync can return OOM.
- Use after free is undefined behaviour. Cross-stream use needs events.
- A release threshold of UINT64_MAX stops the pool from shrinking.
- OPEN: whether OOM is reported at call time or at stream time. Don't depend on either; use a resident arena that the backend owns.

## Errors
- Errors may surface only at sync. cudaGetLastError clears the error; cudaPeekAtLastError does not.
- Use non-blocking streams, not the legacy default stream.
- OPEN: which errors are sticky (see the cudaError enum, Runtime API 6.2.2).
- Transaction rules:
  - Publish results only after a successful sync on the whole dependency chain.
  - Any error fails the epoch.
  - Treat illegal-address and launch failures as poisoning the context: reset it, fall back to CPU and fail the proofs in flight.
  - Mark leaked slots instead of calling cudaFreeAsync on an unhealthy stream.

## Interface
- A PinnedBudget probe.
- An error split into Transient and Poisoned.
- Buffer handles that carry their owning stream and alloc/free events. No raw device pointer leaves the backend.

(Saved by the coordinator; the scout's Write was refused by its harness.)
