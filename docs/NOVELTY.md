# Loki's contributions and prior art

Loki adds a tested CUDA integration for the BLS12-381 prover in `midnight-proofs` 0.8.3, an exact-prefix SRS registry, and prover-specific formal and replay artifacts. The four contribution statements below have source or recorded-test evidence. The comparison establishes their concrete differences from the cited projects. It establishes no first-ever or global priority claim; those claims remain in [TODO](../TODO.md).

The review used Loki `1db6ab8`, then checked P1 changes on main `a256a9a`; loki-gpu main `d71b7ca`; and loki-lean `a177d9114482911e082fe6fe4a3257e5d77fe4e5`. The Lean tree's `SYNCED-LOKI-COMMIT` is `877114d`, so its correspondence with current main remains an explicit obligation. Measurements below are existing recorded results, not runs performed during this documentation review.

## 1. CUDA integration with the Midnight 0.8.3 verifier

Loki integrates sppark CUDA MSM and NTT into the BLS12-381 `midnight-proofs` 0.8.3 prover. Its GPU-required k8 MSM proof fixture produced a 416-byte proof that the independent, unmodified crates.io 0.8.3 verifier accepted. Changing the public input made verification fail. The fixture forces `LOKI_GPU_MSM_MIN=1` and treats fallback as a test failure. [The W4 note](../LOKI-GPU-MSM.md) records the commands, verifier isolation and logs; [the NTT note](../LOKI-GPU-NTT.md) records the separate transform and proof checks.

The target differs from the two closest reviewed CUDA Halo2 implementations. Axiom's `halo2-axiom-gpu` manifest names a BN254 KZG prover. Ingonyama's Halo2 fork declares `halo2_proofs` 0.3.0 and uses `icicle-bn254`. Loki's demonstrated result is integration with the Midnight 0.8.3 BLS12-381 verifier. [Axiom manifest](https://github.com/axiom-crypto/halo2-gpu/blob/main/halo2_proofs/Cargo.toml), [Ingonyama manifest](https://github.com/ingonyama-zk/halo2/blob/main/halo2_proofs/Cargo.toml).

This is verifier compatibility for the recorded fixtures. Independently run lookup proofs can differ in bytes because stock uses randomized map traversal. P1 restores that traversal by default; `loki-deterministic-lookup-order` selects sorted leftovers. The inherited mixed committed/plain multi-proof absorption defect remains a documented limitation. [Compatibility harness](../loki-harness/README.md), [P1 audit](../LOKI-P1-SEAM.md), [instance-order diagnostic](../LOKI-QUINT.md).

## 2. Exact requested-prefix SRS reuse

Loki's opt-in registry uses address and capacity to find candidates, compares the requested prefix's coordinates exactly, and retains validated GPU allocations under an LRU budget. It handles shorter prefix requests without treating a reused host address as proof of identity. This policy and its mutation/prefix regressions are implemented in `src/poly/kzg/msm/cuda/backend.rs` and documented in [the W13 note](../LOKI-SRS-REGISTRY.md).

For implementation `b6af6f9`, the controlled k17 synthetic fixture reduced median SRS preparation from 7460.926 ms to 968.290 ms over three processes per mode. The host used an RTX 5090, CUDA 13.4.3, four Rayon workers and `loki-cpu-budget`; registry validation also used up to four native workers. These measurements compare Loki's legacy cache with its registry on one host. They do not measure a complete recursion chain or a competing implementation.

Reusable MSM bases already exist in ICICLE's public interface, including precomputation and device-resident points. Loki's evidence concerns its exact-prefix validation policy and measured integration. A claim that this cache policy is the first of its kind requires further comparison; the corresponding Lean prefix-refinement obligation also remains open. [ICICLE MSM documentation](https://dev.ingonyama.com/3.1.0/icicle/primitives/msm), [formal follow-ups](../TODO.md).

## 3. Conditional prover-backend refinement in Lean

Loki's Lean artifact proves `Refinement.loki_proof_eq_stock`: the abstract `prove` model emits equal event sequences for a backend satisfying `Refines` and `SameOrder` and for the same inputs, randomness, worker count and lookup-map order. `loki_proof_bytes_eq_stock` derives byte equality using the same codec. The source is `loki-lean/LokiLean/Refinement.lean`, at the Lean revision named above; [the architecture document](ARCHITECTURE.md) explains the model boundary.

The public `halo2-formal` project formalizes Halo2 gadgets and PLONKish gate soundness. Loki's artifact addresses replacement of backend operations inside a structural prover model. That is a specific difference in theorem subject, not evidence of first-ever formalization. [halo2-formal](https://github.com/oxarbitrage/halo2-formal).

`Refines.eq_stock` also proves that `Refines` and `SameOrder` make the abstract backend extensionally equal to `StockBackend`. The main result composes those operation-equality assumptions through the shared `prove` definition. It does not establish that deployed Rust or CUDA satisfies those contracts. `stock-trace-faithful` and `quot-inputs` remain open. The separate GPU specification also contains two unfinished theorem bodies, for segmented MSM and quotient-interpreter safety, and an open compiler-lowering obligation. Its runtime model takes completion as an input; it is not a proof of native stream or resource-lifetime behavior. These are individually tracked in [TODO](../TODO.md).

## 4. Prover-specific replay and compatibility diagnostics

Loki applies Quint models and Rust replay to transcript order, GPU dispatch/cache behavior and proving-key-cache behavior. W8 records four 10,000-sample safety campaigns and 15 Rust tests. A separate instance-order audit reproduced stock rejection for mixed committed/plain columns across multiple proofs. The source and immutable replay boundaries are documented in [the W8 note](../LOKI-QUINT.md) and [the Quint module page](modules/quint.md).

Quint Connect already supplies model-trace replay against Rust implementations. Loki's contribution is its prover-specific models, adapters, trace corpus and diagnostic. The method itself is established prior art. The recorded campaigns provide sampled safety evidence; historical dispatch replay does not execute GPU kernels, and the scheduler model has no shipped scheduler counterpart or temporal liveness proof. [Quint model-based testing](https://quint.sh/docs/model-based-testing), [Quint Connect](https://github.com/quint-co/quint-connect).

## Prior art and claims excluded from the contribution statements

| Candidate | Public evidence | Review decision |
|---|---|---|
| First GPU Halo2/KZG prover | [Axiom CUDA Halo2](https://github.com/axiom-crypto/halo2-gpu) and [Ingonyama Halo2](https://github.com/ingonyama-zk/halo2) already provide CUDA integrations. [PSE Halo2](https://github.com/privacy-ethereum/halo2) documents the KZG lineage. | Reject broad priority. Retain the exact Midnight target and fixture result in claim 1. |
| New CUDA MSM or NTT primitives | [sppark](https://github.com/supranational/sppark) provides the kernels Loki uses. The [cuZK paper](https://eprint.iacr.org/2022/1321.pdf) predates Loki's GPU integration. | Reject primitive invention claims. Credit the libraries and algorithms used. |
| New root-product multiopen algorithm | [Midnight PR #565](https://github.com/midnightntwrk/midnight-zk/pull/565) describes division by the product of the point factors. Loki's [W11 note](../LOKI-MULTIOPEN.md) explicitly records the port. | Reject algorithm priority. Preserve the upstream attribution and Loki's implementation evidence. |
| Established complete-chain speedup | W10 records one matched A3 parent prove-plus-verify pair, 294.285 s to 237.808 s, on `loki/w10-gpu-quotient`. [GPU measurements](GPU.md). | Retain the single-parent observation; complete-chain improvement remains TODO. |
| Best GPU prover performance | [zkSync's bellman CUDA library](https://github.com/matter-labs/era-bellman-cuda) and [Scroll's GPU zkVM workflow](https://github.com/scroll-tech/scroll/blob/develop/docs/testing/docker-compose-e2e-guide.md) address other prover systems. [Scroll's archived prover](https://github.com/scroll-tech/scroll-prover) documents a KZG proving system. | No matched cross-project benchmark exists here; make no ranking. |
| First reusable SRS or Quint replay implementation | ICICLE documents base reuse; Quint documents model-based replay, as cited above. | Reject broad method priority. Keep the concrete implementation and diagnostic evidence. |
| First clean-room GPU backend or first transactional runtime proof | Apache-2.0 licensing and provenance are project properties. The P2 implementation and abstract runtime model have separate evidence boundaries. | No priority claim is established. Provenance review and source refinement remain TODO. |

[The review ledger](review/REVIEW.md) records decisions and owners. [TODO](../TODO.md) contains the unfinished comparisons, proof obligations and measurement gates.
