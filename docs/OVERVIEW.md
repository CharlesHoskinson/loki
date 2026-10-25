# loki-gpu overview

This page summarizes the design in `design/`; it is a reading guide, not the
specification. Loki status is pinned to `main` at
`a256a9a15ff18ae12a8131b86cff576787fa5e3c`; loki-gpu status is pinned to
`main` at `d71b7cafd4308206cc7f283b07a4e67baae89305`. P1 and P2 are merged;
P4 is in progress. The optional backend feature is wired into Loki, but
arithmetic operations remain unsupported.

## The idea in one paragraph

Loki produces a PLONK/KZG proof on BLS12-381. Some proof steps perform heavy
arithmetic: multi-scalar multiplication (MSM), number-theoretic transforms
(NTT), and quotient evaluation. The planned `loki-gpu` arithmetic path moves
these operations to a GPU. Loki retains the transcript, random numbers, challenges
and proof encoding. The compatibility contract is that the unmodified stock
verifier accepts backend-assisted proofs. The W2 harness checks both verifier
directions and verifying-key byte equality; its default cases are k10, k14
and k17. It does not establish byte equality with every independently run
stock prover. Stock lookup preparation can choose different valid
permutations between runs, so lookup proof bytes can differ while both proofs
verify. Loki's default lookup order follows stock HashMap leftovers. The
optional `loki-deterministic-lookup-order` feature sorts those leftovers and
changes lookup proof bytes relative to stock.

The Lean theorem `Refinement.loki_proof_eq_stock` establishes equality of
abstract prover outputs under `Refines` and `SameOrder`, with equal inputs,
randomness, thread count and fixed map order. It does not prove that the Rust
or CUDA implementation satisfies those assumptions. The code-binding
obligations remain open.

## Architecture

(`design/ARCHITECTURE.md`)

| Part | Does | Does not |
|---|---|---|
| `loki-backend-api` | Typed requests, capabilities, errors, resident handles, SRS contracts | Touch CUDA, the RNG, or the transcript |
| Loki adapter (in Loki) | Stock CPU code, the old sppark adapter, midnight-curves conversion, feature selection, fallback | Add new CUDA algorithms |
| `loki-gpu` runtime and SRS | Own device context, memory and validated SRS reuse | Decide anything about the circuit or proof |
| `loki-gpu` MSM, NTT, quotient | One operation family each, over validated data | Change lookup order, relations or the transcript |
| Recursion orchestrator | Schedule leaf, parent and decider jobs | Skip child verification |

The default Loki build has none of this. The backend is chosen by Cargo
features, and its dependencies never turn on CUDA, profiling, BLST thread
changes or deterministic-map changes by accident.

Two rules shape everything else.

- **Same arithmetic as stock.** MSM uses the first `min(scalars, bases)`
  entries, like stock `zip`. NTT uses the exact domain root, coset and one
  inverse normalization. Quotient terms keep stock order.
- **Errors never change the proof.** A failed call leaves the caller's inputs
  and the transcript untouched. If the host still holds a copy, Loki reruns
  the operation on the CPU. Fallback does not repeat RNG draws or transcript
  writes.

## Runtime

(`design/RUNTIME.md`)

One process-wide device coordinator serves MSM, NTT and the quotient. It
selects one CUDA device, checks its compute capability at run time, and fixes
a lock order: device coordinator, then the SRS or allocator lock, then stream
bookkeeping.

- **Resident buffers.** A device value carries its owning context, a
  generation number, a backend-owned allocation identity, length, element
  representation and domain. Safe code cannot treat a device address as a host
  slice. Every use re-checks these fields. A made-up numeric handle is
  rejected.
- **Generations.** An unrecoverable CUDA error poisons the generation. Nothing
  is freed while queued work can still reach it.
- **Streams and staging.** One copy stream and two compute streams, with
  events for dependencies. Public calls block until the result is complete.
  Bulk copies go through a small bounded ring of pinned host buffers, with a
  pageable fallback because pinned memory is limited under WSL2.
- P2 implements the bounded pinned staging pool and pageable fallback.
  Transfer overlap has not been measured.
- **Memory budget.** Budgets count persistent bases and twiddles, inputs,
  outputs, scratch, sort storage, pinned staging and delayed frees. A 32 GB
  card is not 32 GB free. One heavy k19 or k20 proof runs at a time at first.
- **SRS leases.** A cached SRS is identified by its exact validated points,
  basis, length and curve. An address or a caller-supplied digest alone is
  not accepted as proof of identity.

## MSM

(`design/MSM.md`)

The existing sppark MSM slows down badly when many scalars are equal, because
one bucket gets most of the points and one thread sums them serially. On a
warm k18 test, repeated scalars took about 2.4 s against about 25 ms for
random ones. Lookup and permutation columns are full of repeats.

The planned v1 algorithm avoids long serial chains:

1. Convert scalars out of Montgomery form and cut them into fixed windows.
2. Emit one record per nonzero digit and sort records by window and digit.
3. Split every bucket into segments of at most `L` points. Sum each segment
   with a balanced block reduction.
4. Fold each bucket's partial sums with a fixed pairwise tree.
5. Combine buckets into window sums with the running-sum identity, then
   combine windows by doubling.

Zero scalars cost nothing. A repeated scalar produces many short segments
instead of one long chain. Window width (8 to 16) and `L` (64 to 256) are to
be chosen by measurement on `sm_120`. This package (P6) is conditional: it is
admitted only if the remaining time budget after the quotient work justifies
it. No kernel exists yet.

## NTT

(`design/NTT.md`)

The transform layer gives natural-order forward and inverse NTTs, conversion
between coefficient and Lagrange form, coset extension and inverse coset
conversion. The backend receives the root, coset constants and scaling from
Loki's own domain. It does not choose its own.

Its job is residency: columns stay on the device between transforms so the
quotient can use them without a round trip. The design is explicit that this
is not a standalone speedup. A weighted estimate puts all NTT adapter time in
the A3 parent at about 6 s, below the project's gate for doing it alone. It
is built as part of the quotient pipeline (P4).

## Quotient

(`design/QUOTIENT.md`)

The quotient is the largest CPU phase on the real circuit. The backend
evaluates the numerator over the extended coset as a **bytecode program**.

- The host compiles the circuit's gate, permutation, lookup and trash
  expressions into a small instruction set (`Add`, `Sub`, `Mul`, `Square`,
  `Double`, `Negate`) with registers, constants and column operands with
  signed rotations.
- A validator checks register definitions, indices, table lengths and the
  output register before anything launches.
- Each instruction is a fixed 40-byte little-endian record in a versioned
  envelope. Rust structs are never copied into CUDA.
- A CUDA interpreter runs the program row by row. Terms keep the stock order
  `acc = acc * y + term`, and rotations always read original columns.
- It returns the numerator only. Division by the vanishing polynomial, the
  inverse transform, splitting, commitment and transcript writes stay with
  Loki.

The memory required for quotient intermediates needs a derivation before it
can guide the P5 layout. Track that derivation in the Loki [TODO list](../../../loki-wt/docs/TODO.md),
using the local paired-worktree layout.

## Recursion

(`design/RECURSION.md`)

Fenrir proves in a chain: leaf, then parent, then decider. The goal is the
wall time from a ready leaf to final acceptance by the stock verifier.

Three owners stay separate. The Sherlock orchestrator owns the job graph,
inputs, keys and publication order. Loki owns one proof's transcript and
fallback. The backend owns device state and reports capabilities and resource
needs. The scheduler never sees CUDA details.

- Heavy admission is first-in first-out, with cancellation and bounded
  reservations. A second heavy proof on the same GPU is admitted only after
  memory traces and repeated whole-chain runs show it helps.
- The chain, not a single kernel, is the benchmark. Setup, key generation
  and verification are timed too, because the historical sample spent more
  time outside `create_proof` than inside it.
- Child proofs are still verified, and keys, SRS and relation identity are
  matched, exactly as production does today.

## Packages

(`design/PLAN.md`; implementation status at `d71b7ca`, with other packages planned unless noted)

| Package | Content |
|---|---|
| P1 | Stock CPU plug-in seam in Loki (merged) |
| P2 | Transactional runtime and versioned C ABI (merged) |
| P3 | Legacy sppark adapter and validated SRS registry behind the API |
| P4 | Resident NTT and exact cosets (in progress) |
| P5 | Quotient bytecode compiler and interpreter |
| P6 | Skew-robust MSM (conditional) |
| P7 | Whole-chain integration and one-heavy scheduler |
| P8 | Lookup compression and preparation (conditional) |
| P9 | Batching, precompute and concurrent heavy jobs (conditional) |

## How correctness is gated

(`design/TEST-PLAN.md`)

A result is accepted only if it is checked against the CPU. The plan has
differential tests on boundary and edge inputs, injected failures (allocation,
copy, launch, context loss, stale handles), strict "the GPU really ran"
counters, and stock-verifier acceptance of the resulting proofs, including the
k20 parent. A Lean specification models the backend contract. A passing Lean
model does not prove the Rust or CUDA code.

## Evidence the design starts from

All measurements below come from one host (RTX 5090, CUDA 13.4.3, WSL2); see
Loki's `docs/GPU.md`.

- GPU MSM, random scalars: 13x to 17x faster than CPU at 2^20 to 2^22.
- GPU MSM, repeated scalars: slower than CPU.
- GPU MSM and NTT in the k20 parent: 220.8 s against 220.2 s stock.
- Coefficient-form argument commitments (W10) on the old path: 294.285 s to
  237.808 s on the A3 parent, single samples.

These measurements inform the design. None comes from this repository.
