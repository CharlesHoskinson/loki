# zkSync Shivini / era-boojum: public-doc research (ghaiku-3)

Access date: 2026-10-25 (UTC). Scope: public docs, READMEs and repo listings only. No open-icicle source, no H21.md, no GPU access, no code copied. Claim labels: SOURCE (public doc, quoted or paraphrased), CODE (not read here), MEASURED (none in this report), ESTIMATE, OPEN QUESTION.

## Sources (all fetched 2026-10-25)

- S1 https://github.com/matter-labs/era-shivini (README: archived 2024-08-16, read-only; development moved to zksync-crypto-gpu; "still in alpha state"; license Apache-2.0 OR MIT)
- S2 https://github.com/matter-labs/era-shivini/tree/main/src (file listing only)
- S3 https://github.com/matter-labs/era-boojum (README: archived 2024-08-15; Goldilocks field; PLONK-style copy-permutation + log-derivative lookup; FRI; Poseidon; "not intended for AIR arithmetization"; license MIT OR Apache-2.0)
- S4 https://github.com/matter-labs/era-boojum-cuda (README: archived; CUDA 12.x, CMake 3.24+, clang, Rust nightly; CUDAARCHS selects GPU arch, default = GPU present in system; license MIT OR Apache-2.0)
- S5 https://github.com/matter-labs/zksync-crypto-gpu (README: active successor; workspace crates boojum-cuda, criterion-cuda, cudart, cudart-sys, gpu-ffi, gpu-prover, shivini, wrapper-prover; needs CUDA >= 12.0, CMake >= 3.24, and era-bellman-cuda via BELLMAN_CUDA_DIR; CPU stub mode via `--cfg no_cuda` / ZKSYNC_USE_CUDA_STUBS, which makes CUDA calls fail at runtime; license Apache-2.0 OR MIT; no BN254/BLS12-381/KZG mention)
- S6 https://github.com/matter-labs/era-bellman-cuda (README: no archive notice, 16 commits on main; default compute arch 8.0, example 8.6; license Apache-2.0 OR MIT; no curve, operation or residency statement in README)
- S7 https://github.com/matter-labs/era-bellman-cuda/tree/main/src (search snippet only: msm.cu, msm_kernels.cu, ntt.cu, ntt_kernels.cu, finite-field modules)
- S8 https://matter-labs.github.io/zksync-era/prover/latest/ (prover docs: circuit prover and proof compressor are "GPU accelerated"; dev hardware: CUDA GPU with at least 24 GB VRAM, 16+ cores, 64 GB RAM, 200+ GB disk; production = prover gateway + house keeper + many witness generators / circuit provers / compressors, autoscaled; Old Prover Stack removed at protocol v27)
- S9 https://hackmd.io/@OrestTa/zksync-boojum-shivini-prover-setup (third-party setup note, Oct 2023, not official; "currently for testing purposes only"; 1 GPU with >= 24 GB; observed peak 38 GB RAM, 22232 MB GPU memory; stage labels from logs; L4 benchmark: 371 s on 8 CPUs, 273 s on 12 CPUs for a single-circuit test)

Not reached: zksync-crypto-gpu/shivini subdirectory (HTTP 404 on the path tried); any CODE-level description of quotient evaluation or residency. The gate.com Boojum article and crates.io metadata were seen only in search snippets and are not relied on except the crates.io license line (MIT OR Apache-2.0).

## Findings

1. SOURCE (S3). Boojum is not a KZG system (Loki is KZG over BLS12-381). It works over the Goldilocks field (2^64 - 2^32 + 1), uses FRI for polynomial commitments, and uses Poseidon hashing. Its README says it is "not intended for AIR arithmetization" and that FFT "is not optimized".
2. SOURCE (S1, S3, S4, S6). Shivini, era-boojum and era-boojum-cuda are archived. zksync-crypto-gpu (S5) is the active home for the GPU crates; era-bellman-cuda (S6) is still listed as a dependency. All of them are Apache-2.0 OR MIT, so reuse needs NOTICE-style attribution and no copyleft issue. Confirm the per-file headers before copying anything.
3. SOURCE (S8). Official docs call only two prover stages GPU-accelerated: the circuit prover and the proof compressor. Witness generation runs on CPU. Production is a process-level pipeline scheduled across many workers; this is orchestration, not in-GPU scheduling.
4. SOURCE (S5, S6). Curve or field support for the GPU kernels in zksync-crypto-gpu and era-bellman-cuda is not stated in the README. S7 shows MSM and NTT files, but no curve is named. OPEN QUESTION: does era-bellman-cuda target BLS12-381 or BN254 at all? Settling it needs the field-config source files, which I did not read.
5. SOURCE (S9, third party, 2023). Shivini's stage labels from the proving logs are: buffering resolvers, witness LDE, Merkle leaf hashing, second-stage LDE, "gates over general purpose columns contribution to quotient evaluation", quotient work and LDE, batched FRI opening computation. This is the only public stage breakdown found. Treat as ESTIMATE of the real structure until checked against code.
6. SOURCE (S2, names only). src/ contains quotient.rs, constraint_evaluation.rs, copy_permutation.rs, lookup.rs, fri.rs, oracle.rs, dvec.rs, context.rs and a static_allocator/ directory. Names suggest a separate quotient stage and an evaluator layer, and a static allocator. No public doc confirms a bytecode interpreter, a residency model or pool design. OPEN QUESTION.
7. SOURCE (S9). Memory is a hard constraint: about 24 GB VRAM minimum for the dev setup, and an observed 22 GB GPU peak for a single circuit. Host RAM was 38 GB peak, and an untested note says 32 GB plus swap "may work". This is third-party evidence of the scale of the working set, not a benchmark.
8. SOURCE (S4, S5). Build-time arch selection: CUDAARCHS, with default = the GPU present. CPU stub builds exist in zksync-crypto-gpu, but calls fail at runtime, so the stub path is not a usable CPU fallback.

## Which choices transfer to Loki's KZG on BLS12-381 and which do not

CORRECTION (astra-gpu, 2026-10-25): Loki's deployed KZG curve is BLS12-381, not BN254. I checked the source: `src/poly/kzg/params.rs` is generic over `E: Engine`, and the main path is `midnight_curves::Bls12` with scalar `Fq` (`quint/mbt/src/proof.rs`, `examples/loki-profile.rs`, `tests-stock/tests/verify.rs`). `midnight_curves::bn256` appears only in test or example code (`src/plonk/prover.rs` tests, `src/transcript/implementors.rs`). Earlier BN254 wording below is superseded: read "BN254" as BLS12-381, and the transfer conclusions are unchanged because none depend on the curve.

Transfers (ESTIMATE, to be checked against code):
- Staged pipeline with GPU-resident intermediate data, and quotient evaluation as a distinct stage with its own kernels. This matches the loki-gpu plan (resident buffers, quotient evaluation).
- Process-level orchestration: one long-lived GPU worker per device with a job queue, not a library call per proof. Matches the plan's scheduler for several proofs on one GPU.
- Compile-time arch selection through an env var (CUDAARCHS-style) and CPU-stub feature gating. Useful for our sm_120 build, but the stub must return Unsupported, not fail later at runtime as zksync-crypto-gpu does.
- Dual Apache-2.0 OR MIT licensing as a precedent for a permissive GPU library. It is not a licence source for our code; we still write our own kernels.

Does not transfer:
- Goldilocks field arithmetic, FRI, Poseidon Merkle commitments, and the batched FRI opening stage. Loki uses KZG over BLS12-381 (its deployed curve), so these are replaced by MSM and NTT/coset work, not reused.
- The copy-permutation and lookup argument layout of Boojum. midnight-proofs follows halo2-style PLONK, so the quotient and permutation shapes differ.
- The 24 GB VRAM and 16 GB-class memory numbers. They describe a FRI/STARK working set, not a KZG k19-k20 working set. Our A3 number (about 6.2 GiB NTT traffic each way per proof) is the figure to plan against.
- zksync-crypto-gpu's CPU-stub behaviour, which we should not copy.

## Limits

- Public docs and READMEs only. No source file was read, so every CODE-level question is open (quotient bytecode, residency, memory pools, stream model).
- Shivini stage labels come from a 2023 third-party note, not current code.
- Repo status can change; the active successor is zksync-crypto-gpu, and some links returned 404 or a partial page.
- Nothing here was measured on our hardware.

## Actionable implications

1. Do not plan on reusing Boojum or Shivini code paths for KZG. Use them only as a design reference for the staged pipeline and job model.
2. Ask astra-gpu for a decision on whether reading the Apache-2.0 OR MIT source of era-shivini, era-boojum-cuda or zksync-crypto-gpu is allowed for the quotient/residency questions. Under the brief's public-docs rule I stopped at the docs.
3. Check era-bellman-cuda's field-config files to settle the curve question (BLS12-381 or not). If it is BLS12-381, its MSM and NTT kernels are a candidate reference; if not, drop it from the transfer list.
4. Record in design/PROVENANCE.md: S1-S9 with access date 2026-10-25, licence Apache-2.0 OR MIT for all of them, and that nothing has been copied.
