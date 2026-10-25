<p align="center">
  <img src="docs/brand/loki-hero.png" alt="Loki: Same proof. Any shape." width="100%">
</p>

# loki-gpu

*Same proof. Any shape.*

`loki-gpu` is the GPU backend for **Loki**, the prover of the Fenrir stack.
Loki is a fork of `midnight-proofs` 0.8.3 (a halo2/PLONK prover with KZG
commitments on BLS12-381). Its proofs are accepted by the unmodified stock
0.8.3 verifier in the W2 cross-verification cases. `loki-gpu` is the planned
module for computing parts of a proof on an NVIDIA GPU while preserving the
stock-verifier compatibility contract.

"Same proof" describes compatibility with the stock verifier. The W2 harness
checks both verifier directions and verifying-key byte equality for its
listed cases. It does not establish byte equality with every independently
run stock prover. Stock lookup preparation can choose a different valid
permutation between runs, so lookup proof bytes can differ while both proofs
verify. See the [Loki README](../../loki-wt/docs/README.md) for the test scope.

## Status

The design is written and implementation is in progress. Loki status is
pinned to `main` at `a256a9a15ff18ae12a8131b86cff576787fa5e3c`; loki-gpu
status is pinned to `main` at `d71b7cafd4308206cc7f283b07a4e67baae89305`.

- P1, the stock CPU plug-in seam in Loki, is merged. P2, the transactional
  runtime and versioned C ABI in loki-gpu, is merged. It implements scalar
  resident upload, download and release, with a bounded pinned staging pool
  and pageable fallback. Transfer overlap has not been measured.
- P4, the resident NTT and coset pipeline, is in progress. The
  `loki-gpu-backend` Cargo feature connects Loki to the backend crate, but its
  arithmetic operations remain unsupported.
- MSM, NTT, quotient and SRS arithmetic operations remain unsupported. No
  kernel in this repository has produced a proof, and no performance result
  has been measured for this backend.

The GPU acceleration that works today is Loki's own sppark-based MSM and NTT
(`cuda` and `cuda-ntt` features in Loki). It is documented in Loki's
`docs/GPU.md`. That path remains the working baseline and correctness oracle
until this backend passes the same gates.

## Why a new backend

Measurements on the current path show that faster MSM and NTT kernels barely
move the end-to-end time. On the k20 Sherlock parent, Loki with GPU MSM and
NTT took 220.8 s against 220.2 s for stock. The time sits in the quotient,
lookup preparation and other CPU phases. The owner's priority order is:

1. correctness;
2. latency of the whole Sherlock recursion chain;
3. raw kernel speed.

The new backend is designed around that order. It keeps data on the device
between operations, runs the quotient on the GPU, and handles the repeated
scalar skew that makes the sppark MSM slow on lookup columns.

## License and clean-room status

Apache-2.0 (see `LICENSE` and `NOTICE`). This is a clean-room implementation.
No code from open-icicle or from sources with unclear licenses is included or
derived. The design used only a written design memo and public documentation.
sppark (Apache-2.0) may be reused with exact file and revision attribution
in `NOTICE`. `design/PROVENANCE.md` records every source behind each
algorithm.

## Crate layout

```
crates/
  loki-backend-api/   CUDA-neutral contracts: typed requests, capabilities,
                      errors, resident handles, SRS contracts
  loki-gpu/           the backend
    src/lib.rs        LokiGpu, capability advertisement
    src/runtime.rs    device context, generations, budgets
    src/srs.rs        validated SRS registry                  (stub)
    src/msm.rs        G1 multi-scalar multiplication          (stub)
    src/ntt.rs        resident NTT and coset transforms       (stub)
    src/quotient.rs   quotient bytecode interpreter           (stub)
    src/ffi.rs        versioned C ABI boundary
    cuda/             CUDA sources and the C header
design/               design documents (see below)
docs/                 overview and brand assets
```

`loki-backend-api` depends only on `ff` and `group`. It has no CUDA
dependency. `loki-gpu` has one optional feature, `cuda` (pulls in `cc` to
build the native part). Both are Apache-2.0.

## How it plugs into Loki

The backend integration is opt-in. The default Loki build does not depend on
this repository and stays on the stock path, with the same features and RNG
use.

- Loki owns backend selection, proof order, the transcript, randomness and
  the CPU fallback.
- `loki-gpu` owns validated operation execution and device resources.
- A backend never sees the transcript or the RNG, and never changes the
  proof encoding.
- Loki features include `loki-backend-api`, `loki-sppark`,
  `loki-gpu-backend` and `loki-deterministic-lookup-order`. The GPU backend
  feature is mutually exclusive with the legacy `cuda` and `cuda-ntt` flags.
  Future planned features from `design/PLAN.md` include `cuda-resident`,
  `cuda-quotient`, `cuda-msm-balanced` and `cuda-lookup-prepare`.

See `design/LOKI-PATCH-PLAN.md` for the exact Loki-side patch plan.

## Build

```sh
cargo check            # no CUDA toolkit needed
cargo build --release
cargo build --release --features cuda   # needs nvcc (CUDA 13.4, sm_120)
```

The workspace uses Rust edition 2021, minimum Rust 1.76. The target hardware
is an RTX 5090 (`sm_120`) with CUDA 13.4.3 under WSL2. Nothing else is
tested. `design/BUILD.md` covers the toolchain.

## Documentation

Start with [`docs/OVERVIEW.md`](docs/OVERVIEW.md), a plain summary of the
design. Then read the design documents:

| Document | Topic |
|---|---|
| [`design/ARCHITECTURE.md`](design/ARCHITECTURE.md) | Module boundaries, arithmetic boundary, errors, fallback |
| [`design/RUNTIME.md`](design/RUNTIME.md) | Device context, resident buffers, streams, memory budget, SRS leases |
| [`design/MSM.md`](design/MSM.md) | Skew-robust G1 MSM |
| [`design/NTT.md`](design/NTT.md) | Resident NTT and exact cosets |
| [`design/QUOTIENT.md`](design/QUOTIENT.md) | Quotient bytecode and CUDA interpreter |
| [`design/RECURSION.md`](design/RECURSION.md) | Chain latency, scheduling, benchmarks |
| [`design/PLAN.md`](design/PLAN.md) | Packages P1 to P9, with gates |
| [`design/TEST-PLAN.md`](design/TEST-PLAN.md) | Differential, fault-injection and stock-verifier gates |
| [`design/BUILD.md`](design/BUILD.md) | Build and toolchain |
| [`design/LOKI-PATCH-PLAN.md`](design/LOKI-PATCH-PLAN.md) | Patches Loki needs |
| [`design/PROVENANCE.md`](design/PROVENANCE.md) | Sources and clean-room record |
| [`design/OWNERSHIP.md`](design/OWNERSHIP.md), [`design/REVIEW.md`](design/REVIEW.md) | File ownership and review notes |

Design documents label each statement as SOURCE, CODE, MEASURED, ESTIMATE or
OPEN QUESTION. Unlabelled normative text is a proposed contract, not shipped
behavior.

The Loki [novelty record](../docs/NOVELTY.md) and [TODO list](../TODO.md)
live at the root of this repository.
