# Segmented MSM contract

**Evidence convention:** unless explicitly marked SOURCE, MEASURED, ESTIMATE, or OPEN QUESTION,
normative design prose here is CODE (proposed contract). This file specifies future behavior; it is
not evidence that a CUDA MSM has been implemented or measured.

## Scope

This note specifies the correctness boundary for the new bucket-sorted MSM path. Loki owns the
prover call order and transcript. The backend receives scalars and bases, may use device-resident
bases, and returns either a completed point or an explicit error. The caller may use its reference
CPU operation after an error only when the operation contract says the caller-visible state and
inputs were preserved. An error is not counted as GPU success and the backend does not silently
run CPU code itself.

The semantic operation is the stock Loki `msm_specific`: zip scalar and base slices to the shorter
length, discard or retain zero-scalar terms as convenient, and return the additive sum. This keeps
the existing truncation behavior for the low-level operation; callers with aligned protocol vectors
should use equal-length inputs. `RequireEqual` is a separate API policy and must reject a mismatch
before changing caller-visible buffers. See `loki-proofs/src/poly/kzg/msm.rs:163-190` and the Loki
dispatch at `src/poly/kzg/msm.rs:164-175`.

## V1 kernel model

Use unsigned fixed-width radix windows. For canonical scalar integer `s` and width `c > 0`, let
`m = ceil(256/c)` and `d(j,s) = floor(s / 2^(cj)) mod 2^c` for `j < m`. The canonical scalar is
represented by four little-endian `u64` limbs and is strictly less than the BLS12-381 scalar modulus;
the 256-bit window layout therefore has a zero high tail where needed. The identity is
`s = sum_{j < m} d(j,s) 2^(cj)`.

For each input pair and each window with a nonzero digit, emit `(window, digit, point_index)`. Stable
sort by `(window, digit, point_index)`; run-length encoding yields one contiguous key range per
nonempty bucket. Partition each key range into disjoint half-open segments of at most `L` entries.
Every emitted point index appears once across those segments and every segment belongs to exactly
one key. Reduce each segment by point addition, then reduce the segment partials for a key with a
deterministic pairwise tree. An empty bucket is the identity. The window sum is
`sum_{d=1}^{2^c-1} d * bucket(window,d)` and the final result combines all `m` windows with the
`2^c` Horner recurrence. The last window must be processed even when it contains only high zero
digits.

If an implementation partitions the digit axis into local chunks `[a,b]`, a local reverse running
sum gives coefficient `d-a+1` for bucket digit `d`; correct it by adding
`(a-1) * sum_{d=a}^b B[window,d]`. Lean's `segmentedWindow` states the full weighted sum directly,
so this chunk-offset identity is an implementation-level alternative that must be covered by the
kernel differential tests. A chunk may not restart the running sum at coefficient one and omit the
offset.

Stable sort order and a deterministic reduction tree are operational choices, not semantic
requirements for an exact group operation. Their contracts are made explicit so the Rust
implementation can be differentially tested and so segment omissions or duplicates become
detectable. This design does not add repeated-scalar preaggregation to v1.

## Contract layers

1. **Input normalization:** `ZipTruncate` matches `msm_specific`; `RequireEqual` may reject a
   length mismatch before output publication. Scalar parsing/reduction must preserve the exact
   `Fr` values passed by Loki.
2. **Digit coverage:** every nonzero digit of every zipped scalar emits exactly one key/value pair;
   zero digits emit none. Boolean, zero, repeated, maximum canonical, and random scalars all obey
   the same decomposition.
3. **Sorted runs and segments:** sorted keys are monotone; each RLE range is exactly one key;
   segments are a disjoint exhaustive partition of that range with size in `[1,L]`; no pair is
   assigned to another key.
4. **Reduction:** segment sums, bucket sums, weighted window sums, and Horner composition equal the
   mathematical MSM. Empty input and all-zero input return identity. Identity bases remain valid
   entries and contribute identity.
5. **Transactional call:** success publishes only a value equal to the reference MSM; error leaves
   caller-visible inputs, outputs, and transcript state unchanged. Private backend state may poison
   a generation or invalidate a cache entry. The caller decides whether and when CPU fallback runs.
6. **Proof composition:** per-call equality instantiates the MSM field of Loki's existing
   `Refinement.Refines`; the separate stock-order predicate preserves transcript order. Together
   with `LokiLean.Backend.proof_events_eq_stock`, the prover event stream and encoded proof bytes
   equal stock. This module does not claim that a CUDA kernel satisfies these contracts.

`BackendMsm.BatchCallContract` treats one shared-base batch as a single publication unit and
preserves result slot order. `dispatchBatch_msmBatch_exact` matches the `OperationContracts` MSM
field. `stockBackendWithBatchDispatch_meetsInterface` replaces that field in `StockBackend` while
leaving every other operation and transcript order stock; its byte-equality corollary is
`stockBackendWithBatchDispatch_proofBytes_eq_stock`. The separate native kernel composition starts
from a successful output tied to `SegmentedPlan` and `segmentedValue`, then reaches the dispatch
contract through `segmentedKernelCall_meetsContract` and
`segmentedKernelDispatch_meetsMsmSpecific`. The remaining open Lean proof is precisely
`OBLIGATION(msm-segmented-partition)`, `segmentedValue_eq_pippengerList`; it is not hidden in an
assumption about successful kernel output.

## Required tests

Differential tests compare each GPU result against `msm_specific` and an independent CPU group-sum
oracle. Inputs cover empty and mismatched lengths, all-zero scalars, boolean scalars, repeated
scalars and repeated bases, random scalars, canonical maximum scalar, identity bases, cancellation,
and mixed zero/nonzero vectors. Adversarial partition tests inject a missing index, duplicate index,
cross-bucket index, unsorted key, empty segment inside a nonempty RLE run, and an oversized segment;
each mutation must be rejected or produce a failing invariant. Forced `Unsupported`, allocation,
launch, and synchronization errors must prove that the reference fallback receives the original
inputs and that transcript events are unchanged. See `design/TEST-PLAN.md` for the project-wide
mapping and stock-verifier gates.

## Provenance

The unsigned bucket identity and window reduction are formalized in the existing
`loki-lean-wt/gpu-spec/LokiLean/Msm.lean` (`digit`, `bucket`, `windowSum_eq`,
`pippenger_eq_msmNat`, `pippengerList_eq`, and `pippenger_meetsMsmSpecific`). The new segmented
partition obligation is an implementation refinement of those definitions. Scalar range and
canonical limb representation are grounded in `LokiLean/Field.lean` and the `Fq` source cited
there. The call/error boundary is grounded in `LokiLean/Backend.lean` and the Rust dispatch citations
above. The algorithm is specified independently; no CUDA backend source is copied.
