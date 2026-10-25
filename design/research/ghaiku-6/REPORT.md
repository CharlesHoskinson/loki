# ghaiku-6 research report: skew-robust MSM, residency and scheduling, licences

Access date: 2026-10-25 (UTC). Scope: Ulvetanna/Irreducible (Binius), Cysic, ZPrize 2022/2023 GPU MSM entries, cuZK and follow-ups. Sources were public web pages, GitHub README/API metadata and abstracts only. No source code of any project was opened, read or copied (open-icicle and lab-haiku-1/H21.md were not touched).

Labels: SOURCE = stated in a cited public document; MEASURED = number reported by its author (not reproduced here); ESTIMATE = my inference; OPEN QUESTION = not verified.

## 1. Binary-field work (Ulvetanna / Irreducible): not transferable to Loki's BLS12-381 MSM and NTT

- SOURCE: Ulvetanna was renamed Irreducible in April 2024 (https://www.irreducible.com/posts/becoming-irreducible).
- SOURCE: `IrreducibleOSS/binius` is Apache-2.0 (GitHub API, `spdx_id` Apache-2.0), archived, last push 2025-09-09. README: "a SNARK using towers of binary fields, implemented in Rust", "archived and officially succeeded by Binius64". https://github.com/IrreducibleOSS/binius
- SOURCE: `IrreducibleOSS/binius-gpu` (CUDA, "Experimental GPU kernels for Binius computations", MIT, copyright 2024 Irreducible Inc., last push 2026-08-14). Kernels named in its README: binary tower field multiplication (compact and bit-sliced), additive NTT, sumcheck over the 128-bit tower field. It has no MSM. README: "This code is experimental and not currently used by the official Binius prover." CMake default is the oldest supported arch (sm_75 with CUDA 13) plus embedded PTX for JIT on newer GPUs; `-DCMAKE_CUDA_ARCHITECTURES=native` is recommended. https://github.com/IrreducibleOSS/binius-gpu
- SOURCE: Company status is unclear. The posts index shows an undated banner "Irreducible is Shutting Down" linking to `./posts/irreducible-shutting-down`, and a 9/9/25 post "Reinventing Irreducible". https://www.irreducible.com/posts . OPEN QUESTION: the shutdown post itself was not fetched, so its date and content are unconfirmed. Treat as a maintenance risk for any dependency on Irreducible code.
- ESTIMATE: Binary-field tower arithmetic, additive NTT over binary fields and binary sumcheck do not transfer to Loki's BLS12-381 KZG PLONK path (prime-field Montgomery arithmetic, Pippenger MSM, radix-2 NTT over a prime field). Transferable only at the level of build/packaging practice (MIT CMake layout, PTX embedding, arch selection), which is generic.

## 2. Cysic

- SOURCE: `cysic-labs/venus` (Apache-2.0 OR MIT, per the LICENSE-APACHE and LICENSE-MIT files; GitHub API reports Apache-2.0; last push 2026-10-13). "a compute backend extension and specialization" built on ZisK. https://github.com/cysic-labs/venus
- SOURCE: Its GPU work is CUDA backend tuning in `pil2-proofman/`: "cudaGraph integration, expression evaluation kernel tuning, and shared-memory optimizations". Preliminary benchmarks on RTX 5090 are reported as "measurable improvement over the ZisK baseline" (no numbers given). The README does not mention MSM, NTT, resident memory or streams.
- SOURCE: Cysic's NTT and MSM acceleration is FPGA/ASIC (`venus-acc/`, AMD UltraScale+/Versal with HBM). Not GPU code.
- Implication (ESTIMATE): the only published Cysic GPU lesson is that cudaGraph capture of a fixed per-proof kernel sequence is worth doing. No Cysic MSM algorithm is available to study. OPEN QUESTION: whether Cysic has any other public GPU MSM repository; none was found.

## 3. ZPrize GPU MSM (2022 and 2023): skew handling, sorting, load balance

Context (SOURCE, https://www.zprize.io/blog/zprize-i-spotlight-open-division-prize-1a-1b): 1a GPU prize was a tie between Matter Labs and Yrrid Software, each "well above 2x over the baseline". Curve: BLS12-377 G1. The spotlight page itself gives no algorithm details.

Approaches as described in public documents (summarised by LambdaClass, https://blog.lambdaclass.com/eyes-on-the-prize/, which quotes each team's write-up; I checked the quotes against the summary, not against the original repositories):

| Team | Scalar handling | Sort and skew | Library |
|---|---|---|---|
| Yrrid (Apache-2.0 repo) | Signed digits, 23-bit window, "11 windows of 23-bits" compressed to 2 windows by precomputing 6 points per input | "The buckets are then sorted, such that the buckets with the most points are run first." Purpose: "run convergent workloads and minimizes the tail effect" | custom sort, "instead of using the CUB libraries" |
| Matter Labs (era-bellman-cuda, Apache-2.0 OR MIT) | sorted per bucket | "length-encoded and sorted ... according to the number of points that the bucket has"; largest-first | custom, parallel |
| Voronov/Kolganov (ALEXks, Apache-2.0) | signed scalars | sorts subscalars, precomputes start/end indexes | `thrust::sort_by_key`, `thrust::inclusive_scan` |
| Speakspeak (cuZK code) | per-window scalars | `cub::DeviceRadixSort::SortPairs`, then `cub::DeviceRunLengthEncode::Encode`; "each thread handles the buckets corresponding to the unique scalars" | CUB |
| 6block | per window | sort, then "if one m_i,j is equal to m_i-1,j then nothing has to be done" | - |

Key points:
- SOURCE: the largest-first ordering of buckets and the run-length/unique-scalar idea are the published skew countermeasures. Both are directly relevant to our repeated-scalar problem (about 2.4 s at k20 against 25 ms at k18, from the brief's measured facts).
- SOURCE (Yrrid README): "Pre-process all of the scalars to generate lists of points to add to each bucket." This means the cost of skew moves into preprocessing; the run order is then fixed.
- MEASURED (Yrrid README, A40 GPU, 46 GB device memory, CUDA 11.7): "2500 milliseconds and 2600 milliseconds for a full run of 4 MSMs of size 2^26". The README adds: "Performance is quite dependent on the other workloads running on the same physical machine." Note the 46 GB requirement is roughly 1.5x the RTX 5090's 32 GB, so the precompute strategy must be budgeted against VRAM.
- Search-snippet secondary claim (UNVERIFIED): Yrrid and Matter Labs both scored 2.52 s in the ZPrize GPU table. Not checked against the ZPrize results page.
- Negative claim (UNVERIFIED, secondary): a cuZK-related paper is said to find that radix-sort approaches are good on random scalars but poor on clustered ones. I could not confirm this in the cuZK abstract (the abstract does not discuss sorting or skew), and the full PDF was not reachable.

## 4. cuZK and its follow-ups (residency, transfers, reduction)

- SOURCE: cuZK, ePrint 2022/1321 (https://eprint.iacr.org/2022/1321). Abstract: "nearly perfect linear speedup over the Pippenger algorithm"; "cuZK reduces the latency overhead caused by CPU-GPU data transfer by 1) reducing redundant data transfer and 2) overlapping data transfer and device computation"; "over 2.08x (up to 2.94x) speedup versus the state-of-the-art GPU implementation". The eprint page states CC BY-SA 4.0. No code link on the page. Code repository named in the TCHES artifact: `speakspeak/cuZK` (licence not checked).
- SOURCE: Chen, Peng, Dai, Luo, He, "Load-Balanced Parallel Implementation on GPUs for Multi-Scalar Multiplication Algorithm", TCHES 2024 Vol. 2024 No. 2, pp. 522-544, DOI 10.46586/tches.v2024.i2.522-544, https://tches.iacr.org/index.php/TCHES/article/view/11438. The paper PDF is blocked by bot protection, so the claims below are from the search snippet (SECONDARY, MEASURED by the authors): on BLS12-381, V100, speedup over cuZK of 1.998x, 1.821x, 1.818x at 2^21, 2^22, 2^23; up to 1.853x over the earlier bucket-accumulation method. Contribution: "cached Jacobian coordinates" and a "parallel layered reduction for bucket aggregation".
- SOURCE: Artifact README (https://artifacts.iacr.org/tches/2024/a9/readme.html): GPUs V100-SXM2-32GB, RTX 3090, RTX 4090. No licence stated for the implementation. Artifacts `381_xyzz_bal` and `381_xyzz_constant`.
- Implication (ESTIMATE): overlap of H2D transfer with compute and avoiding redundant transfers (cuZK) is the published pattern for our residency model, and the layered bucket reduction is a candidate for the bucket-reduction stage. Both are unverified by us.

## 5. Licence metadata (GitHub API and LICENSE files; record in PROVENANCE if any idea is adopted)

| Repo | Licence | Note |
|---|---|---|
| yrrid/submission-msm-gpu | Apache-2.0 (LICENSE) | CUDA, last push 2023-12-20 |
| yrrid/submission-wasm-msm | NOASSERTION ("Other") | do not use code |
| matter-labs/era-bellman-cuda | Apache-2.0 OR MIT | CUDA, default arch 8.0 |
| cysic-labs/venus | Apache-2.0 OR MIT (two files) | Rust/CUDA monorepo |
| IrreducibleOSS/binius | Apache-2.0 | archived |
| IrreducibleOSS/binius-gpu | MIT | experimental, binary fields only |
| ALEXks/zprize-msm-gpu-submission-1 | Apache-2.0 | 2022 submission |
| TalDerei/webgpu-msm-twisted-edwards | none detected | do not copy; WebGPU, Twisted Edwards |
| cuZK paper (eprint 2022/1321) | CC BY-SA 4.0 (paper page) | code licence unverified |
| speakspeak/cuZK (code) | not checked | do not copy until checked |

Regardless of licence, the algorithmic ideas above are to be re-implemented from our own description, cited in `design/PROVENANCE.md`. No code from any of these repositories should be copied without a separate licence check.

## 6. Actionable implications for loki-gpu

1. Skew: adopt a sort-by-bucket, run-length encode, largest-buckets-first schedule for the MSM bucket-accumulation stage, plus a unique-scalar fast path (Yrrid, Matter Labs, Speakspeak, 6block). Expected to target the repeated-scalar blow-up. Validate by MSM timing at k18 and k20 with repeated, boolean and zero scalar distributions. Measured speedup is unknown.
2. Window/precompute: signed 23-bit windows and 6-point precompute (Yrrid) trade VRAM for fewer windows. Budget against 32 GB on the RTX 5090 (Yrrid used 46 GB on an A40). Choose precompute depth by measurement.
3. Reduction: evaluate the layered bucket reduction (TCHES 2024) against running-sum; source is secondary, so benchmark before adopting.
4. Residency/scheduling: overlap H2D transfers with compute (cuZK) and CUDA graph capture of the fixed per-proof kernel sequence (Cysic/venus). Both are SOURCE claims with no independent numbers from us.
5. Binary-field code (Binius, binius-gpu) is out of scope for Loki's BLS12-381 MSM and NTT. Do not design against it. Its maintenance status is uncertain.
6. Build practice: CUDA arch should be set explicitly for sm_120 (`-DCMAKE_CUDA_ARCHITECTURES=120`), not default to the oldest arch with PTX fallback, given the RTX 5090 target. ESTIMATE from the binius-gpu README pattern.

## 7. Limits

- TCHES paper PDF and the cuZK PDF were not reachable (bot protection, 403). Their numbers are from abstracts or search snippets.
- No repository code was read beyond README and metadata. Yrrid's `yrrid-msm/` directory was not opened.
- ZPrize results (2.52 s) are from a secondary snippet. Not verified.
- No measurements were made by this agent. All performance numbers are authors' claims on other hardware (A40, V100, RTX 3090/4090).
- Irreducible shutdown status unconfirmed.

## 8. Correction (astra-gpu, bus message CURVE CORRECTION)

Loki's deployed KZG uses BLS12-381, not BN254. Earlier wording in sections 1 and 6 that referred to BN254 has been replaced. The ZPrize 1a GPU task used BLS12-377 G1 (SOURCE: ZPrize spotlight). The Yrrid README fetched by this agent does not name a curve, so its 2.5 s result cannot be assumed to be BLS12-381. The TCHES 2024 artifact is BLS12-381 (SOURCE: artifact README). Applicability to Loki's BLS12-381 must be re-measured. Scalar ABI and curve types in Loki source were not inspected by this agent.
