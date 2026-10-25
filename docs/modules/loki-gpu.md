# loki-gpu backend modules

`loki-gpu` is the Apache-2.0 clean-room GPU workspace in
[`loki-gpu/`](../../loki-gpu/). P1, the Loki-side backend API seam, is merged in Loki
main at `a256a9a15ff18ae12a8131b86cff576787fa5e3c`. Its P2 transactional
runtime is implemented in loki-gpu main at
`d71b7cafd4308206cc7f283b07a4e67baae89305`, and P4 resident NTT work is in progress.
P1 typed MSM/transform dispatch is integrated in Loki main. The opt-in
`loki-gpu-backend` feature exposes the runtime, but the backend MSM, NTT,
quotient, and SRS arithmetic operations still return `Unsupported`. P4 resident
NTT work is in progress. The
backend is intended to replace the legacy sppark adapters described on [poly-msm](poly-msm.md),
[cuda-ntt](cuda-ntt.md), and [cuda-kernels](cuda-kernels.md). Feature names
below come from the plan and may change.

| Package | Planned module and feature | Status |
|---|---|---|
| P1 | Stock CPU plug-in seam: Loki `src/backend/{mod,cpu,dispatch}.rs`, feature `loki-backend-api`, default off. `MsmBackend::msm`, `TransformBackend::transform`. | Merged in Loki main |
| P2 | Transactional device runtime and versioned ABI: crate `loki-gpu`, feature `loki-gpu-backend`. | Implemented; available through an opt-in Loki feature |
| P3 | Legacy adapter and validated SRS registry (`loki-sppark`, or `loki-gpu-srs-registry`). | Planned |
| P4 | Resident NTT and coset pipeline, `cuda-resident`. | In progress |
| P5 | Quotient bytecode compiler and interpreter, `cuda-quotient`. | Planned |
| P6 | Balanced MSM, `cuda-msm-balanced` (conditional). | Planned |
| P7 | Whole-chain integration and one-heavy-job scheduling. | Planned |
| P8, P9 | Lookup preparation on the GPU, batching and concurrency (conditional). | Planned |

Constraints from the plan: default Loki keeps stock CPU semantics; the new backend conflicts with the legacy `cuda` / `cuda-ntt` flags; transcript writes stay outside backend execution.

For other platforms (Intel, AMD, Apple, WASM, mobile) no code exists here. They are tracked as TODOs in the platform documentation.
