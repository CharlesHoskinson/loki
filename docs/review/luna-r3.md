# GPU engineer review

## Findings

The review compared the assigned GPU documentation with the implementation and evidence pinned by the lead. The status descriptions have since been updated for Loki main `a256a9a15ff18ae12a8131b86cff576787fa5e3c` and `loki-gpu` main `d71b7cafd4308206cc7f283b07a4e67baae89305`.

- The P2 runtime is implemented in `loki-gpu`, including bounded pinned staging. P1 typed dispatch is integrated in Loki main. The runtime is available through an opt-in feature; backend arithmetic operations remain unsupported, and P4 resident NTT work is in progress.
- `LOKI_CUDA_ARCH` accepts digits such as `120`; `LOKI_NTT_CUDA_ARCH` is passed to NVCC as `-arch=<target>`, such as `sm_120` or `compute_80`. The documentation now states the formats separately.
- SRS upload validation is serial in the legacy `cuda` cache. With `cuda-srs-registry`, candidates of length `n >= 4096` use up to four native scoped workers.
- The GPU fallback diagram now describes stock-verifier acceptance in the tested cases rather than claiming byte-identical CPU and GPU proofs.
- The platform diagram now limits verifier acceptance to tested GPU proofs and states the W2 verification-key byte result separately.
- The runtime module page no longer gives a volatile source line count.
- W10 A3 timing values are identified as single prove-plus-verify samples on the recorded branch. They are linked to their checked-in result files; the independent verifier probe remains a separate reference.

## Evidence limits

The review found no novelty claim in its assigned GPU lens. The GPU path uses sppark 0.1.16 for MSM and NTT; this review did not establish a prior-art comparison. Stock verifier acceptance in tested cases does not establish CPU/GPU proof-byte equality. The Lean theorem is conditional on its stated model assumptions and does not bind the Rust, FFI, or CUDA implementation to the model.
