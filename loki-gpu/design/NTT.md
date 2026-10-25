# Resident NTT and coset transforms

CODE (proposed contract): unless explicitly SOURCE, MEASURED, ESTIMATE or OPEN QUESTION, normative prose specifies planned behavior, not implementation evidence.

Status: design and contract for the first backend API; CUDA implementation is an
`Unsupported` stub. The current measured sppark adapter remains the oracle and
fallback until a resident implementation clears the integrated quotient gate.

## Scope and value

The transform layer provides natural-order NTT/INTT, coefficient-to-Lagrange
conversion, coset extension, and inverse coset conversion over the exact domain
chosen by Loki. The backend receives roots and coset constants from that domain;
it must not silently select a different root, coset, ordering, or normalization.

This is a prerequisite for keeping quotient operands resident, not an independent
speedup claim. MEASURED (historical): X2 warm full-adapter medians were of 32.866/34.751 ms forward /
inverse at 2^20 and 116.906/119.995 ms at 2^22. ESTIMATE: weighting the H0 census of 39
small and 40 large transforms gives an estimated 5.958–6.155 s of adapter time;
the separately weighted 4.370 s conversion cost is already included. That model
is below H0's 6.999 s A3 gate, so standalone NTT residency is deferred. Reassess
inside an integrated quotient pipeline against a fresh W10/W12 profile; do not
add the adapter estimate to the quotient phase or claim the old 10.5 s estimate.

## Public behavior

The public transform contract is expressed over typed, opaque resident buffers
and an explicit transform specification. Rust `TransformSpec` carries the
exact forward root, source and output log-sizes, direction, boundary input/output
order, inverse scale, and—when applicable—the exact coset generator and inverse.
For `ElementOrder::BitReversed`, physical slot `p` represents logical index
`bitrev(p)` on input and contains logical output index `bitrev(p)` on output.
The contract therefore describes data layout, not a different mathematical
transform.
Resident transforms consume an immutable context-tagged buffer and return a
fresh one. In v1, upload, download, and resident transform methods return only
after transfer or transform work has retired. The backend may use internal
streams and events, but it must not publish a handle whose writes are pending.
The initial API does not expose a caller-selected stream or async completion
token; a later API revision can add these if complete buffer lifetimes and
failure semantics remain explicit.

Inputs remain readable throughout the operation. Results are written to a new
resident output (or private scratch); they become caller-visible only after a
successful completion. On an error, the source and any previously published
output remain unchanged. An error may poison the backend generation and make
its handles unusable; it never exposes a partial result. Loki may fall back to
CPU only when it still has the original host input, or when a complete host
checkpoint was explicitly retained. A device-only value lost with its context
cannot be reconstructed by fallback.

Transcript and prover ordering remain Loki responsibilities. The backend may
schedule kernels asynchronously internally, but it must await dependent work
before returning a readable output; no backend operation may absorb transcript
data or reorder Fiat–Shamir events.

## Exact algebra and layouts

For a domain of size `N = 2^k`, the forward operation returns the natural-order
DFT

`out[j] = Σ (i = 0 .. N-1), input[i] * omega^(i*j)`.

The inverse uses `omega⁻¹` and multiplies by `N⁻¹` exactly once. This equals
`EvaluationDomain::ifft`; a CUDA library that already normalizes must not receive
a second host-side scale. Public results are natural-order. A kernel may use
bit-reversed or tiled layouts internally only if it permutes each operand and
result consistently before publication. Pointwise work between transforms may
remain in a non-natural layout only when every index-dependent table and rotated
read is translated through that layout. The quotient has rotated queries, so
“pointwise-only” reasoning cannot be assumed for the whole numerator.

For `coeff_to_extended`, let `n = 2^k`, `N = 2^K`, `omega_e` be the domain's
extended root, and `zeta` be its configured cube root of unity. The operation is
exactly:

1. For each source coefficient `i < n`, multiply by `zeta^(i mod 3)` using the
   global source index.
2. Zero-pad coefficients `[n, N)`.
3. Apply the forward NTT with `omega_e`, returning natural-order values
   `p(zeta * omega_e^j)` for every `j < N`.

For `extended_to_coeff`, apply the inverse NTT with `omega_e⁻¹` and exactly
`N⁻¹`, then multiply coefficient `i` by `zeta^(-i)`. With equal source/output
logs, return all `N` coefficient slots, including high zero slots where the
input is a valid extended evaluation of a degree-`< n` polynomial. A smaller
output log means truncate to the first `2^output_log_size` unshifted
coefficients; this covers only the truncation step of
`extended_to_lagrange`, whose final small-domain forward NTT is a separate
operation.

At the API boundary, an ordinary transform has `2^source_log_size` input and
`2^output_log_size` output elements with equal logs. A forward coset transform
may take a shorter coefficient vector and zero-extends it to the output length;
its scaling index begins at source index zero and uses the full index rather than
a per-chunk local index. An inverse coset transform consumes
`2^source_log_size` elements and returns the requested coefficient prefix.
Coset transforms use natural order at the boundary; layout permutations are
internal to the implementation.

`zeta` and its inverse are not interchangeable guesses: this domain defines the
inverse shift as `zeta^2` because `zeta^3 = 1`. Tests pin the supplied roots and
coset values. A forward/inverse round trip alone is insufficient to prove that
the selected root or coset is the stock one.

## Implementation boundary

The v1 `loki-gpu` NTT entry point is deliberately a stub. With CUDA disabled, or
while the kernel is not implemented, it returns `Unsupported` before allocating,
writing, or publishing output. Its CUDA translation unit exports the shared v1
byte-wire entry point and returns `UNSUPPORTED` without touching output bytes.
This stub does not claim device residency, asynchronous execution, or a working
NTT. The byte-wire FFI is scaffolding only; the public Rust traits stay typed.

The production implementation should first reuse the allowed sppark device-
pointer NTT path under Apache-2.0 attribution. It should accept caller-owned
device allocations and stream dependencies, keep inputs immutable, and publish
only after completion. Avoid per-call full host round trips and allocations.
The first admission limit remains one heavy k19–k20 proof on the shared device;
the scheduler may increase this only after measured residency and peak-memory
evidence. An integrated quotient pipeline must count each transfer and transform
once and be benchmarked end-to-end.

## Contracts and evidence gates

Lean contracts in `LokiLean.BackendNtt` compose the existing `Loki.Ntt` results:
`IsNaturalNtt`, `IsInverseNtt`, `IsCosetNtt`, and `IsICosetNtt`. Every successful
backend result equals the stock operation at each valid index. A failed
operation preserves caller-visible source/output state; generic backend
transaction and proof refinement are specified in `LokiLean.Backend`. These
contracts are checked by direct CPU/GPU differential tests with pinned roots,
coset, ordering, and inverse normalization, plus injected-error snapshots.

CODE (checked specification): `Request F d` fixes the domain and carries the exact source length. `validateRequest_rejects_wrong_length` rejects malformed shapes; `dispatch_output_length` fixes the returned extent. `KernelSet.Meets` contains the four exact forward/inverse kernel predicates. `kernelList_eq_stock` lifts them to finite results for all five conversions, and `kernelCall_contract` constructs the successful transactional contract. `backendWithKernels_proof_bytes_eq_stock` proves actual stock-backend replacement under these kernel assumptions. The length-two BLS12-381 witnesses exercise successful publication and Unsupported fallback. These abstract proofs do not verify the Rust wire conversion or native kernels.

Before enabling a real kernel, require:

- differential equality for sizes and domain roots used by Loki, including
  zero, one, minus one, and reduced field representatives;
- direct coefficient-to-evaluation comparisons against the CPU and an
  independent small naive DFT, not just round trips;
- inverse scaling applied exactly once and exact `zeta^(i mod 3)` indexing;
- nonzero GPU dispatch counters and no hidden CPU fallback in GPU tests;
- unchanged state after failures before launch, after enqueue, and before
  publication, with correct context-generation invalidation;
- W2 proof cross-verification by unmodified stock midnight-proofs 0.8.3, then
  matched A3 and complete recursion-chain benchmarks after integration.

## Source and code anchors

- **CODE:** stock Loki `src/poly/domain.rs:249-335,374-445` defines the coefficient,
  coset, and inverse transform sequences and CPU fallback.
- **CODE:** `LokiLean/Ntt.lean:696-790` defines natural, inverse, coset, and
  inverse-coset contracts; its root-pinning result demonstrates why roots must
  be explicit.
- **MEASURED / ESTIMATE:** X2 adapter medians and weighted estimates are recorded
  in `gpu-lab/astra/RESULTS.md:197-210`; these are historical adapter figures,
  not a new backend measurement or proof-level speedup.
- **SOURCE:** allowed implementation reference is sppark 0.1.16, Apache-2.0,
  specifically its public device-pointer NTT entry points; retain attribution in
  the repository `NOTICE`. No source from a backend with unclear licensing is
  used.
