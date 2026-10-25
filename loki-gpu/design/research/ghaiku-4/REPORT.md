# ghaiku-4 REPORT: Polygon GPU prover and Aleo snarkVM public docs

Scope: backend seam, MSM batching and skew handling, NTT residency. Clean-room: no open-icicle source, no lab-haiku-1/H21.md, no sim scripts were opened. Only public pages, PR text, and package metadata were read. No code was copied. Access date: 2026-10-25 (UTC).

Labels: SOURCE = stated in a fetched primary page; CODE = read from a repo file; MEASURED = a number reported by a source (not re-run by us); ESTIMATE = our inference; OPEN QUESTION = unverified.

## 1. Licences (SOURCE, from official metadata)

| Item | Licence as found | Where |
|---|---|---|
| snarkVM (ProvableHQ, mirror aleo-labs-protocol) | Apache-2.0 per repo sidebar and LICENSE.md text ("Apache License, Version 2.0, January 2004") | https://github.com/ProvableHQ/snarkVM ; https://raw.githubusercontent.com/aleo-labs-protocol/snarkVM/mainnet/LICENSE.md |
| snarkVM README badge | Shows GPL v3 alt text in one mirror, Apache-2.0 image badge; conflicting on search snippets | https://github.com/aleo-labs-protocol/snarkVM |
| snarkvm-algorithms-cuda 4.12.0 | `license = "Apache-2.0"` in Cargo.toml and crates.io metadata | https://crates.io/api/v1/crates/snarkvm-algorithms-cuda ; https://raw.githubusercontent.com/ProvableHQ/snarkVM/refs/heads/mainnet/algorithms/cuda/Cargo.toml |
| sppark 0.1.16 (supranational) | Apache-2.0 (crates.io; README "Apache License Version 2.0") | https://crates.io/api/v1/crates/sppark ; https://github.com/supranational/sppark |
| pil2-proofman (Polygon) | "Apache-2.0, MIT licenses found"; monorepo dual-licensed, user may choose either | https://github.com/0xPolygonHermez/pil2-proofman |
| ZisK (Polygon, 0xPolygonHermez/zisk) | Apache-2.0 OR MIT (user's choice) | https://github.com/0xPolygonHermez/zisk |

Note on snarkVM licence conflict: the snarkVM repo page shows both an Apache-2.0 badge and a "License: GPL v3" alt text. The LICENSE.md fetched from the mainnet branch is Apache-2.0. Treat Apache-2.0 as the stated licence, but confirm against the exact commit before any vendoring. OPEN QUESTION: which commit/branch the GPL alt text comes from.

Implication: snarkVM's CUDA crate is Apache-2.0 and can be read as a licence-clean reference for architecture, subject to the normal rule of citing it and recording it in design/PROVENANCE.md. Do not copy kernel code without provenance.

## 2. Polygon GPU prover: what is verifiable

- Polygon's public GPU-relevant work is in **pil2-proofman** (not a standalone "GPU MSM/NTT library"). No dedicated Polygon MSM/NTT library was found in search. ZisK's repo page does not state its GPU stack (SOURCE: fetched page has no GPU section). OPEN QUESTION: whether ZisK's GPU path uses sppark or its own kernels; not confirmed.
- PR #630 (pil2-proofman, "GPU quotient: evaluate Q on the smallest subgroup that holds it") SOURCE: the GPU quotient evaluates Q only on NExt >> qs points, with qs = log2(blowup) − ceil(log2 qDeg). The iNTT runs over the same reduced size. At blowup 2 / max degree 5, qs = 0 (no change). At blowup 4 / max degree 8, 8N points instead of 16N.
- PR #630 MEASURED (author's benchmark, not re-run): test recursion with blake3 at 2^19, blowup 4, warm GPU, two alternating rounds: proof 225 ms → 204 ms (−9.4%); quotient step 73 ms → 53 ms (−28%). The PR says the gain is not 50% because the kernel is memory-bound with strided rows. GPU model NOT named.
- PR #630 SOURCE: the quotient step is stored on `ExpressionsGPU` because a replayed CUDA graph skips host code. This is a CUDA-graph replay fact relevant to our async/stream model.
- PR #610 (pilfflonk) SOURCE: the BN128 fflonk/SHPLONK final-SNARK GPU path is refused unless the process's first GPU is CUDA device 0. Implication: device-selection policy must be explicit in our backend API, not implicit.
- PR #657 (pilfflonk perf) SOURCE (search snippet, PR text): MSM work uses sppark's pool; adaptive MSM sizing; the review flagged a CUDA build failure in an existing test using a removed API. Treat as review finding, not confirmed status.
- PR #593 SOURCE (snippet): "processing eight NTT positions per CUDA block" among kernel changes. No numbers.
- Not found for Polygon: a published skew/repeated-scalar analysis, NTT residency across proofs, or a backend trait seam. OPEN QUESTION for any follow-up.

## 3. Aleo snarkVM CUDA path: what is verifiable

Sources: PR #3579 (aleo-labs-protocol mirror) text; PR #3578 text (snippet only); snarkvm-algorithms-cuda 4.12.0 Cargo.toml.

SOURCE (PR #3579 description, fetched):
- **Backend seam / MSM inputs:** `VariableBase::msm_field` takes field elements; the device does digit extraction and Montgomery reduction, so the CPU does not convert each coefficient to a big integer. KZG10 commit/open use `msm_field`; `skip_leading_zeros` returns a slice (no copy).
- **Base upload:** bases cross to device as one contiguous copy; a kernel widens them into sppark's slots. The PR states sppark's per-row strided `cudaMemcpy2DAsync` is "much slower, especially under confidential computing" (claim by PR author; MEASURED-by-author, no numbers given).
- **Streams:** NTT runs on the stream of the device-pool slot it holds (not sppark's shared `zero` stream), using that slot's preallocated device memory as PolyMul does. Each MSM worker has its own stream so two MSMs on one device do not serialise.
- **Env knobs:** `SNARKVM_MSM_WBITS` (10–18, or `auto` for sppark heuristic; default 14, fitted on H100; sppark heuristic picks 10 there). `SNARKVM_MSM_THREADS` (1–1024; default two workers per GPU, fitted on L4).
- **Device-side rounds:** Round 3 `lineval_interpolate_mul` avoids returning interpolated polynomials between steps. Round 4 `fourth_round_chain` runs on device; only a, b, g, quotient, and f's constant term return to host. Opening `accumulate_open` forms the combination, divides by (X − z), commits on device; only the commitment returns. PR notes sppark 0.1.16 fixes a shared-memory race in that division.
- **Failure policy:** a non-parsing env value aborts when the device pool is built (no silent default). PR #3578 (snippet): device failures panic rather than silently fall back to CPU; adds GPU CI.
- **Determinism:** PR says proof bytes are identical with and without `cuda` (pinned proof).
- **Residency:** PR #3579 text does NOT describe cross-proof residency; it links PRs #3587 and #3588 for that. NOT FETCHED. OPEN QUESTION.
- **Skew / repeated scalars:** PR #3579 does NOT mention skew. Scalars are sent in Montgomery form with reduction on device. No skew handling documented.
- **Benchmarks:** PR #3579 gives NO timings or speedups (only tuning values and the L4/sm_89 test setup).

SOURCE (snarkvm-algorithms-cuda 4.12.0 Cargo.toml):
- deps: sppark 0.1.5 (crates.io, no git pin), blst 0.3.11; build-deps cc ^1.0.83, which ^8.0. No bindgen.
- Build: CUDA sm_70 or later; CUDA Toolkit with nvcc (SOURCE: mainnet algorithms/cuda/README.md).

Discrepancy (SOURCE): the mainnet README fetched has no env-var, pool, or failure-policy sections, although PR #3579 says `algorithms/cuda/README.md` documents them. Either they live on the PR branch or the PR text is ahead of mainnet. OPEN QUESTION: fetch the PR head README before relying on those env var names.

Note: sppark 0.1.5 is pinned by crates.io version, while PR text says 0.1.16 fixes a race. Version mismatch between mainnet Cargo.toml and PR text is an OPEN QUESTION.

## 4. Third-party background (not Polygon/Aleo)

- sppark (supranational, Apache-2.0, v0.1.16) SOURCE README: MSM and NTT CUDA templates; BLS12-381 and Pasta curves; "primarily supports x86_64 with Nvidia's Volta+ GPU"; limited AMD RDNA/CDNA support. No stated compute capability number; no statement on window sizing, multi-GPU, or streams in README.
- ZKProphet, arXiv:2509.22684 (Verma, Yuan, Talati, Austin; Univ. of Michigan; IEEE IISWC 2025) SOURCE (abstract page via fetch summary): NTT kernels can account for "up to 90% of the proof generation latency on GPUs" once MSM is optimised; NTT implementations "under-utilize GPU compute resources"; ZKP arithmetic runs on the 32-bit integer pipeline with limited ILP. The paper's GPU model, library list, and figures were NOT retrievable (PDF binary only). Search snippet says sppark was fastest MSM at 2^20 and bellperson best NTT at 2^15–2^17. UNVERIFIED (snippet only).

## 5. Actionable implications for loki-gpu

1. **Seam:** snarkVM's pattern (device pool slot owns stream and preallocated memory; field-element input to MSM; device-side round chain returning only what the host needs) matches our planned resident-buffer model. Adopt the "return only what the host needs" rule as a testable contract in the backend API.
2. **Config:** make MSM window and worker count explicit and validated at backend construction; fail loudly on bad values (snarkVM policy). Do not silently default.
3. **Failure:** adopt "device error panics or returns Err, never silent CPU fallback" only if our API makes the fallback an explicit, recorded choice. This is a design decision for astra-gpu.
4. **Quotient:** PR #630's reduced-subgroup evaluation is relevant only for blowup > 2 or degree > 5; for k19–k20 Sherlock recursion (blowup 2, deg 5) it changes nothing. Do not prioritise unless our blowup changes.
5. **Device selection:** the pilfflonk "device 0 only" restriction shows explicit device policy is needed for multi-GPU or multi-proof scheduling.
6. **Skew:** neither snarkVM nor pil2-proofman documents repeated-scalar handling in the sources read. Our own measurements (A3 skew ~2.4 s vs 25 ms at k18) remain the only evidence; no external design to copy.
7. **Residency:** the cross-proof residency design in snarkVM (PRs #3587/#3588) is the next thing to fetch; not done here.

## 6. Limits

- No GPU access; no benchmarks re-run. All numbers are author-reported.
- Mainnet README vs PR text discrepancy (section 3) and sppark version discrepancy (0.1.5 vs 0.1.16) unresolved.
- ZKProphet body text not retrieved.
- Polygon's GPU MSM/NTT stack not identified beyond pil2-proofman PR text; ZisK GPU status unknown.
- Search-result snippets used where noted; they are not primary.

## 7. Sources (access 2026-10-25)

- https://github.com/aleo-labs-protocol/snarkVM/pull/3579
- https://github.com/aleo-labs-protocol/snarkVM/pull/3578 (snippet only)
- https://github.com/aleo-labs-protocol/snarkVM/blob/mainnet/algorithms/cuda/README.md
- https://raw.githubusercontent.com/ProvableHQ/snarkVM/refs/heads/mainnet/algorithms/cuda/README.md
- https://raw.githubusercontent.com/ProvableHQ/snarkVM/refs/heads/mainnet/algorithms/cuda/Cargo.toml
- https://raw.githubusercontent.com/aleo-labs-protocol/snarkVM/mainnet/LICENSE.md
- https://github.com/ProvableHQ/snarkVM
- https://crates.io/api/v1/crates/snarkvm-algorithms-cuda
- https://crates.io/api/v1/crates/sppark
- https://github.com/supranational/sppark
- https://github.com/0xPolygonHermez/pil2-proofman
- https://github.com/0xPolygonHermez/pil2-proofman/pull/630
- https://github.com/0xPolygonHermez/pil2-proofman/pull/610 (snippet)
- https://github.com/0xPolygonHermez/pil2-proofman/pull/657 (snippet)
- https://github.com/0xPolygonHermez/pil2-proofman/pull/593 (snippet)
- https://github.com/0xPolygonHermez/zisk
- https://arxiv.org/abs/2509.22684 and https://arxiv.org/pdf/2509.22684 (abstract summary only)

## 8. Curve applicability for Loki (added after astra-gpu CURVE CORRECTION)

- Target curve for Loki's deployed KZG is BLS12-381 (per astra-gpu, LOKI-DESIGN). No BN254 assumption is made in sections 1-7; the only curve-specific statements above are about sources.
- sppark (v0.1.16 README, SOURCE): MSM templates for "BLS12-381 and the Pasta curves"; crates.io/GitHub topics also list bls12-377. BLS12-381 is therefore a listed instantiation; the exact field/curve instantiation and its performance were NOT verified.
- snarkvm-algorithms-cuda 4.12.0 (SOURCE, Cargo.toml): depends on `blst` 0.3.11, which is a BLS12-381 library. The curve(s) actually used by the snarkVM CUDA path (e.g. whether BLS12-377 or BLS12-381) were NOT verified; OPEN QUESTION.
- Applicability to Loki BLS12-381 KZG: sppark's MSM template is the only source read that explicitly lists BLS12-381; the snarkVM "msm_field" pattern (field-element input, device-side Montgomery reduction) is curve-neutral in its description. Any use must be re-checked against Loki's own scalar ABI in the Loki source (not opened by this scout).
