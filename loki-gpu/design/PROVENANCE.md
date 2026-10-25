# Algorithm and source provenance

Status: design-time provenance ledger for `loki-gpu`. It identifies sources
behind the proposed algorithms and the observed baselines. It does not claim
that kernels, the scheduler, or a proof have been implemented or benchmarked.

## Evidence labels

- **SOURCE** — a paper, formula reference, or published documentation used for
  mathematical/API background.
- **CODE** — a specific existing implementation/API inspected as an integration
  reference. It is not a license to copy beyond its stated license.
- **MEASURED** — a result already recorded by the GPU lab, with its scope and
  limitations stated. No measurement was run for this document.
- **ESTIMATE** — arithmetic on explicitly named measured values or assumptions.
- **OPEN QUESTION** — not settled by the available source or measurement.

Unless explicitly labelled SOURCE, CODE, MEASURED, ESTIMATE, or OPEN QUESTION,
normative design statements in this document are **CODE (proposed contract)**;
they describe intended behavior and do not claim implementation or verification.

## Clean-room and licensing record

**CODE —** This repository is Apache-2.0. Its current `NOTICE` says the
implementation is clean-room and calls for third-party notices for any code
included. This design uses no GPU backend implementation source with unclear
licensing. The allowed clean-room input for GPU design is the own-words memo
`/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`.
That memo itself forbids using backend source and expressly derives its
proposals from the permitted materials listed there.

**CODE —** `sppark` is an Apache-2.0 source/dependency and already appears in
the Loki lockfile as version 0.1.16. Existing field, curve, MSM and NTT code
may be depended on or copied only within the applicable Apache-2.0 terms. Any
copied source must retain its copyright and license notices and add a specific
entry to this repository's `NOTICE`; dependency use and source copying should
be recorded separately. The v1 design prefers dependency reuse and keeps the
stock/CPU implementation as the oracle and fallback.

**CODE —** `midnight-proofs` 0.8.3 and Loki are MIT OR Apache-2.0. Their
current source files are integration and reference behavior, not an implicit
grant to copy code without preserving their applicable notices.

**OPEN QUESTION —** Before a release, the integrator must record the exact
resolved `sppark`, CUDA toolkit, CUDA runtime, and compiler versions/licenses
from `Cargo.lock`, build metadata, and distribution artifacts. If any CUDA
source is vendored, extend `NOTICE` with the upstream copyright and license
before merging it.

### Scout research and licensing audit

The following scout reports are research records, not implementation inputs.
Only the concepts expressly identified below inform this design. No code was
copied from these projects. Their local reports preserve the scope, date,
source links, and uncertainty of each scout's review:

| Report | Exact source/version and licensing boundary |
|---|---|
| `design/research/ghaiku-1/REPORT.md` | Public ICICLE docs: 2.8.0 MSM/precomputation, 3.0.0 stream/residency, and 3.6.0 MSM configuration. No source code was read. ICICLE's project/code license and BLS12-381 support were not established; no ICICLE code, API contract, or performance claim is used. Bucket sorting and segmentation are independently specified from published algorithm literature and the allowed clean-room memo. |
| `design/research/ghaiku-2/REPORT.md` | `sppark 0.1.16` (comparison cache `0.1.14`), Apache-2.0; package metadata and permitted local source were inspected. It records C++ template/FFI boundaries, BLS12-381 parameters, and no upstream sm_120 selection. This supports a pinned legacy dependency, not binary-layout compatibility or a successful CUDA 13.4 build. Any copied file needs its original header and a `NOTICE` entry. |
| `design/research/ghaiku-3/REPORT.md` | Public docs/README status as of 2026-10-25; no tagged release or commit was pinned. `era-shivini`, `era-boojum`, `era-boojum-cuda`, `zksync-crypto-gpu`, and `era-bellman-cuda` report Apache-2.0 OR MIT (order varies). No code was read or copied. Goldilocks/FRI and undocumented curve support do not establish applicability to Loki's BLS12-381 KZG; orchestration/architecture are context only. |
| `design/research/ghaiku-4/REPORT.md` | Public metadata/PRs, mostly unpinned branch/PR snapshots (accessed 2026-10-25): `snarkvm-algorithms-cuda 4.12.0` Apache-2.0; that manifest lists `sppark 0.1.5`, while PR #3579 discusses 0.1.16; pil2-proofman and ZisK Apache-2.0 OR MIT. A snarkVM mirror displays conflicting GPL badge text versus the fetched Apache license. No snarkVM code is copied or relied on; the version mismatch, curve use, and performance transfer remain open. |
| `design/research/ghaiku-5/REPORT.md` | Filecoin `ec-gpu 0.2.0` and `ec-gpu-gen 0.7.1` MIT OR Apache-2.0; bellperson is MIT OR Apache-2.0 but no exact release/commit was pinned. The report contains source-derived bellperson internals, several gathered through summaries; under the public-doc-only boundary these are historical research only and excluded as normative design inputs. No design decision relies on them, no code was copied, and fallback/lock behavior is not our scheduler. |
| `design/research/ghaiku-6/REPORT.md` | Public papers, artifact pages, and repository metadata (accessed 2026-10-25); repo commits/releases generally not pinned. Licenses reported: binius Apache-2.0; binius-gpu MIT; venus and era-bellman-cuda Apache-2.0 OR MIT; Yrrid GPU submission Apache-2.0; cuZK paper CC BY-SA 4.0 while cuZK code license is unchecked. No code was opened/copied. BLS12-377 ZPrize results and binary-field Binius kernels are excluded as non-transferable evidence; cuZK is paper-level context only. |
| `design/research/ghaiku-7/sm120-cuda13.md` | NVIDIA public CUDA compute-capability/compiler/tuning/release docs (accessed 2026-10-24); no implementation source copied. Target architecture evidence informs native sm_120 only. Toolkit patch-level claims and some PTX compatibility statements remain open; record actual compiler version and successful target build before claiming support. |
| `design/research/ghaiku-8/wsl2-memory-streams.md` | NVIDIA CUDA Runtime API 13.4, CUDA Programming Guide, and WSL User Guide 5.1, plus Microsoft WSL issue #14078 as community evidence. NVIDIA confirms pinned memory is limited under WSL2 but gives no numeric bound. The community 500–600 MB observation is not universal. Bounded pin probing and event/error rules are informed by primary docs; chunk sizes are estimates. No code copied. |
| `design/research/ghaiku-9/REPORT.md` | Candidate crates: `cudarc 0.19.10` and `cc 1.10.0`, both MIT OR Apache-2.0; `cust 0.3.2` MIT OR Apache-2.0, `rustacuda 0.1.3` Apache-2.0 OR MIT, and `cuda-sys 0.2.0` MIT are excluded stale/unselected options. `libloading 0.9.0` is a cudarc transitive candidate whose license was not checked; cudarc itself is not selected. Its recommendation is superseded by the workspace's `cc 1.10.0` + nvcc build and shared `cudart` choice. The current `build.rs` resolves/validates `CUDA_HOME` and `NVCC`, sets process `NVCC` before `cc::Build::new()`, selects shared cudart, and configures sm_120 plus compute_80 PTX. The separate cudarc probe is not a build result for this crate. |
| `design/research/ghaiku-10/complete-formulas-msm.md` | Formula/literature review (accessed 2026-10-24): RCB ePrint 2015/1060, Bosma–Lenstra DOI 10.1006/jnth.1995.1088, Pippenger (1976 record plus older 1980 reference not read), cuZK ePrint 2022/1321 (paper CC BY-SA 4.0; code license not established), GZKP DOI 10.1145/3575693.3575711 (abstract only), DistMSM DOI 10.1145/3620666.3651364 (metadata only), and SRE arXiv:2610.27452 (preprint abstract only). The cofactor is an arithmetic calculation, not a hardware measurement; accompanying `bls_check.py`/`bls_h2.py` scripts are diagnostic arithmetic/probable-prime experiments, not formal proofs or implementation sources. Full-curve RCB applicability was not checked and is irrelevant to v1. No formula text or code was copied. New RCB branchless formulas are excluded from v1; exact implementation uses existing sppark arithmetic. |

Additional source documents read for design context, not code reuse, include
the allowed clean-room memo at
`/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`,
the public-project/licensing survey reports listed above, and the recursion
literature cited in this ledger. A project license does not by itself establish
that a particular algorithm, curve, API behavior, or benchmark transfers to
Loki. Any new code dependency or copied implementation must receive a separate
license and compatibility review.

### Current Rust dependency license snapshot

**CODE —** This is the exact workspace lock snapshot at review time (`Cargo.lock`,
lockfile format v3). Direct API dependencies are `ff 0.13.1` (MIT/Apache-2.0)
and `group 0.13.0` (MIT/Apache-2.0). The opt-in CUDA build uses optional build
dependency `cc 1.10.0` (MIT OR Apache-2.0). Transitive registry packages and
their local `Cargo.toml` license identifiers are:

| Package | Locked version | Declared license |
|---|---:|---|
| `bitvec` | 1.1.1 | MIT |
| `find-msvc-tools` | 0.1.15 | MIT OR Apache-2.0 |
| `funty` | 2.0.0 | MIT |
| `radium` | 0.7.0 | MIT |
| `rand_core` | 0.6.4 | MIT OR Apache-2.0 |
| `shlex` | 2.0.1 | MIT OR Apache-2.0 |
| `subtle` | 2.6.1 | BSD-3-Clause |
| `tap` | 1.0.1 | MIT |
| `wyz` | 0.5.1 | MIT |

The lockfile itself does not record license metadata. These identifiers were
read from resolved registry manifests. Recompute this inventory from the
release lockfile, including newly resolved transitives, before release.
`cudarc` is not currently locked or a workspace dependency.

**SOURCE —** CUDA toolkit/runtime components have separate NVIDIA licensing and
redistribution terms. The NVIDIA CUDA EULA's Attachment A lists specified
runtime libraries, including `libcudart.so` and `libcudart_static.a`, for
distribution with applications subject to that EULA. This does not change the
Apache-2.0 license on this repository's source, cover every CUDA component or
the driver, or by itself establish rights for an arbitrary binary bundle.
Record the actual toolkit/runtime components and applicable EULA terms for
each release artifact. See the [NVIDIA CUDA EULA](https://docs.nvidia.com/cuda/eula/index.html).

## Algorithm provenance ledger

| Algorithm or mechanism | Provenance and intended use | Status / implementation rule |
|---|---|---|
| Signed-window Pippenger MSM | **SOURCE —** N. Pippenger, “On the Evaluation of Powers and Related Problems” ([1976 record](https://scholarship.claremont.edu/hmc_fac_pub/630/), [SIAM publication](https://epubs.siam.org/doi/abs/10.1137/0209022)). **CODE —** `sppark 0.1.16` provides the existing signed-window GPU implementation and BLS12-381 field/point templates. | Legacy baseline/reference only; this is not the v1 recoder. No replacement performance is assumed. |
| Unsigned fixed-window v1 MSM recoding | **SOURCE —** positional radix decomposition over the canonical 256-bit scalar integer; the exact digit equation, top partial-window mask, zero filtering, and recombination are specified independently in [`design/MSM.md`](MSM.md). Pippenger is cited for the bucketed MSM background, not as a source for this exact unsigned schedule. | **PROOF OBLIGATION —** establish the canonical scalar conversion and `s = Σ d_j 2^(jw)` for every valid scalar, including the masked top window; differential-test reduced group output against CPU and stock. No signed carry, external code, or performance result is claimed. |
| Shifted fixed-base table (deferred P9) | **SOURCE —** the permitted clean-room memo `gpu-lab/icicle-study/DESIGN.md` identifies shifted SRS bases as a later, separately resident optimization (WP3, with precompute mechanics surveyed in its MSM background); [`design/MSM.md`](MSM.md) independently defines `T[i,g] = 2^(w*q*g) P[i]` for `q = ceil(m/p)`. `sppark 0.1.16` is the allowed BLS12-381 arithmetic primitive, not the source of a fixed-table API or an assumed speedup. | **PROOF GATE —** prove `T[i,g]` equals the stated group multiple and that grouping `m` scalar windows into `p` blocks preserves `Σ_i s_i P_i`; validate every table entry from the same immutable, validated SRS prefix and generation. **PERFORMANCE GATE —** P9 only: account persistent bytes and cold-build cost; compare table-on/off over matched production chains including amortization. Require P9's ≥10% throughput improvement with p95 single-chain latency regression ≤5% and all correctness/memory gates. Deferred; excluded from v1/default dispatch. |
| Balanced bucket accumulation and segmented reduction | **SOURCE —** clean-room WP4 design in the permitted `gpu-lab/icicle-study/DESIGN.md` (bucket sort, segment partials and tree reduction), informed by Pippenger and public GPU MSM literature such as [cuZK, ePrint 2022/1321](https://eprint.iacr.org/2022/1321). | **ESTIMATE —** no speed claim. Implement independently from the memo and papers; retain `sppark` as fallback/oracle. Do not inspect or port any vendor GPU backend. |
| Low-cardinality scalar grouping | **SOURCE —** proposed in the same clean-room memo: group equal nonzero scalars, sum their corresponding points, then run the reduced MSM. The algebraic identity follows from distributivity of scalar multiplication over point addition. | **OPEN QUESTION —** sorting and grouping overhead may dominate; use only after measured scalar-distribution dispatch criteria and exact differential tests. No threshold or production dispatch is settled. |
| BLS12-381 curve arithmetic | **SOURCE —** Renes, Costello, Batina ([ePrint 2015/1060](https://eprint.iacr.org/2015/1060)) and the [Explicit-Formulas Database](https://www.hyperelliptic.org/EFD/oldefd/jacobian.html) are literature references only; this design has not established full-curve RCB applicability. **CODE —** use existing licensed `sppark 0.1.16` BLS12-381 field/point primitives for the GPU path. | Full-curve RCB applicability is irrelevant to v1; no new curve formula is proposed. Do not copy or substitute formulas; validate the chosen existing representation and output conversion against the stock CPU operation and Lean contract. |
| NTT / inverse NTT and coset conversions | **CODE —** use `sppark 0.1.16` device-pointer NTT entry points where the crate supports them; Loki's CPU contract is `/home/charles/repos/loki-proofs/src/poly/domain.rs` plus `LokiLean/Ntt.lean` in the assigned Lean worktree. | Preserve exact root, natural ordering, coset `ZETA` distribution, zero padding, and one inverse `1/n`. Round-trip alone is not a correctness oracle. A new mixed-radix NTT is not the v1 design. |
| Numerator bytecode interpreter | **CODE —** compile Loki's `GraphEvaluator` and the argument evaluation loops in `/home/charles/repos/loki-proofs/src/plonk/evaluation.rs` into a per-row instruction stream. **SOURCE —** semantics are stated by `LokiLean/Fusion.lean`, `Plonk.lean`, and the clean-room WP2 section of the design memo. | Preserve each circuit's custom-gate → permutation → lookup → trash schedule and Horner accumulation order. Register allocation and rotations need explicit compiler correctness obligations; no external interpreter code is copied. |
| P5 numerator compiler, v1 bytecode codec, CPU interpreter and allocation certificate (implemented, CPU-only) | **CODE —** independent lowering of Loki `src/plonk/evaluation.rs` (`GraphEvaluator`, `Calculation`, `Evaluator::evaluate_numerator` at base `0c4ab9d`) in `src/backend/quotient_compile.rs`; wire format exactly as `design/QUOTIENT.md` v1; codec/validator/interpreter written independently in `crates/loki-gpu/src/quotient.rs`. Linear-scan register allocation and simulation-based translation validation of the allocation are textbook techniques, not copied code. The synthetic differential fixture follows the *test structure* of W10's `src/plonk/evaluation/argument_commit_gpu_tests.rs` (Loki lineage: random unconstrained committed polynomials suffice for raw numerator equality); the W2 harness circuit is ported from `loki-harness/src/backend.rs`. No external interpreter, open-icicle source or H21 material was consulted. | Compiler correctness is checked by pointwise differential against Loki's stock-derived `Evaluator::evaluate_numerator` (audited equal in numerator arithmetic to midnight-proofs 0.8.3 with `loki-harden` off; tested feature set default (`committed-instances`) + `loki-backend-api`) on every extended row (synthetic all-family cases, harness k10-k14); the external crate's private evaluator is not invoked; interpreter correctness by per-opcode/rotation tests; allocation by `check_allocation`, which `compile_numerator` and `bind_host_columns` (via `CompiledQuotient::verify`) run on every compile and submission. These are tests, not the Lean `BytecodeImplementsPasses` proof. |
| Coefficient-basis argument commitments | **CODE / MEASURED —** W10's `cuda-argument-commit` reuses coefficient-form polynomials with an exact-domain SRS condition. First A3 sample changed 233.284 s to 175.256 s wider prove+verify, but a matched integrated repeat and actual-A3 independent stock gate remained pending in `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md` at the captured revision. | Historical W10 evidence only, not a general backend result. Require coherent coefficient/Lagrange SRS, same domain/root, preserved RNG/transcript order, W2 and stock acceptance before promotion. |
| SRS residency and exact-prefix cache matching | **CODE —** current Loki cache is `/home/charles/repos/loki-proofs/src/poly/kzg/msm/cuda/backend.rs`; associated Lean obligations include `residentCache_sound`, `stockHit_sound`, and `msmSpecific_take_min`. The proposed exact-prefix identity rule is described in the clean-room WP3 memo. | Cache residency is a resource optimization, not a new proof rule. Do not drop base validation or key by pointer/address alone. Mutation-at-same-address, length, curve, and device-generation cases require tests. |
| Stream ordering, pinned copies, and stream-ordered allocation | **SOURCE —** NVIDIA [CUDA C++ Best Practices Guide 13.4](https://docs.nvidia.com/cuda/archive/13.4.2/cuda-c-best-practices-guide/index.html), [CUDA Programming Guide](https://docs.nvidia.com/cuda/cuda-programming-guide/index.html), and [CUDA Runtime API](https://docs.nvidia.com/cuda/pdf/CUDA_Runtime_API.pdf). | These specify API behavior and constraints, not a throughput guarantee. Keep host borrows/leases alive to completion; fall back to pageable staging if a bounded pinned pool cannot be allocated. WSL2 behavior and actual overlap must be measured on the target host. |
| P2 transactional runtime, native runtime ABI and fault-injecting fake driver (implemented) | **SOURCE —** CUDA Runtime API 13.4 reference (streams with `cudaStreamNonBlocking`, events, `cudaMemcpyAsync` pageable/pinned semantics, `cudaHostAlloc`, `cudaDeviceReset`, error codes and stickiness) and CUDA Programming Guide async-copy notes; WSL2 pinned-limit context from `design/research/ghaiku-8/wsl2-memory-streams.md`; sm_120 targets from `design/research/ghaiku-7/sm120-cuda13.md`; contracts from `design/RUNTIME.md`/`ARCHITECTURE.md`. **CODE —** written independently in `crates/loki-gpu/src/{runtime,ffi}.rs`, `cuda/runtime.cu`, `cuda/include/loki_gpu_ffi.h`; no third-party source copied (no sppark, no open-icicle). Test fixture `tests/support/mod.rs` implements the Goldilocks field from its definition. | Sticky-error classification is an explicit list from the Runtime API error enum and remains a review item. Lifetime correctness is a Rust/test obligation (`runtime-lifetime`), checked by the fake driver and compute-sanitizer, not by Lean. MEASURED results in `design/p2/REVIEW.md`. |
| Proof-job DAG scheduling and admission | **CODE —** topology and dependency facts come from the pinned Sherlock/Moriarty parent and decider implementation plus the cohort audit in `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`. **SOURCE —** scheduling constraints are grounded in the task's own DAG and CUDA stream documentation above; they are not copied from a prover scheduler. | Initial policy is one heavy k19/k20 proof, ready-node dependencies, byte-budget admission, and independent timing of proof/verification/setup. Two-heavy admission remains unproven. Scheduler work does not alter proof bytes or transcript semantics. |
| Recursion accumulation, alternative terminals, and folding | **SOURCE —** literature surveyed in `/home/charles/repos/Moriarty-factory/wiki-llm/loki/speedups/recursion-structure.md` includes CycleFold ([ePrint 2023/1192](https://eprint.iacr.org/2023/1192)), HyperNova ([ePrint 2023/573](https://eprint.iacr.org/2023/573)), and ProtoGalaxy ([ePrint 2023/1106](https://eprint.iacr.org/2023/1106)). | Research context only. These techniques are not part of the current backend or scheduler design; they change relation/protocol/key semantics and require a separate soundness, key-lineage, and stock-ledger review. |

## Measurement provenance

All values in `design/RECURSION.md` are transcribed from the clean-room lab
records below; they were not produced by this author. Keep the cited raw logs
and hashes with their original owners.

| Result | Source identity and scope | Qualification |
|---|---|---|
| **MEASURED —** H0 A3: 233.284 s wider prove+verify; `create_proof` 145.982583427 s; outside interval 87.301416573 s. | `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`, “Provenance and H0 audit”; Sherlock parent k20, Moriarty circuit commit `6ee918696ab907244c703bbd94bfea9acde7ddd1`, Rayon 4, BLST 1, WSL2 / RTX 5090 / CUDA 13.4.3; raw logs are named there. | One shared-host sample. Outside interval is a difference between wrapper scopes, not a decomposed phase. |
| **MEASURED —** W10 A3 coefficient candidate: 175.256 s wider interval; `create_proof` 85.910208915 s; outside interval 89.345791085 s. | `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`, “W10 coefficient-basis result”; candidate reported as `4267d75`, feature `cuda-argument-commit`; same circuit and host, Rayon 4 / BLST 1; raw log and audit hash are recorded there. | First sample only; exact launch identity was still being consolidated, reverse-order matched repeat and full promotion gates pending. |
| **MEASURED —** B3 synthetic leaf/parent/decider proof-only values and process-scope totals. | `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`, “Recursion baseline correction” and X9; dirty build commit, binary and lock hashes recorded there. | Separate B3 cohort. Child-process timing includes setup/model/witness; not a measured unified top-level chain. |
| **MEASURED —** X5/X6 scheduler and memory instrumentation. | `/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md`, sections X5, X6 and “X6 measured GPU allocation probe”; probe commands and audit files recorded there. | Accounting and k14 instrumentation only; neither proves GPU service time, k20 memory capacity, or safe concurrent-heavy admission. |

## Source handling checklist for implementers

1. Record each algorithm's source, version/revision, license, copied paths (if
   any), and any local adaptation in this table before merging it.
2. Preserve upstream copyright/license headers for every copied file and add
   all applicable entries to `NOTICE`.
3. Keep public papers and documentation as citations; do not copy code unless
   its license is clear and compatible.
4. Keep CPU/reference implementations independent enough to provide a useful
   differential oracle.
5. Label performance evidence by exact scope, cohort, source revision, and
   confidence. Estimates must include assumptions and arithmetic; estimates
   are not measurements.

## P4 resident NTT (loki-p4, 2026-10-25)

| Item | Label | Source and boundary |
|---|---|---|
| `cuda/vendor/sppark-0.1.16/ff/mont_t.cuh` | CODE (vendored) | sppark 0.1.16 from crates.io, Apache-2.0, copied verbatim, header kept; sha256 `f6f89b574507a6b07dec5e5f62d4d4d705a4a3b062187d7cda9d003809b96473`. Transitive includes: `<cstddef>`, `<cstdint>`, `pow.hpp`. Entered in `NOTICE`. |
| `cuda/vendor/sppark-0.1.16/ff/pow.hpp` | CODE (vendored) | Same package, verbatim; sha256 `487b8c43b7c64956f5ba5a45413c218c5a1848c2b56141f643b124bddb49611f`. Transitive include: `<cassert>` only. |
| `cuda/ntt_field.cuh` | CODE (own) | Instantiates `mont_t` with BLS12-381 Fr constants (r, R² mod r, R mod r, 2r, m0' = 0xffffffff), each recomputed independently in Python from the modulus; canonical↔Montgomery helpers are own code. |
| `cuda/ntt.cu` radix-2 network | SOURCE | Iterative decimation-in-time Cooley–Tukey with a bit-reversed load: Cooley & Tukey, "An algorithm for the machine calculation of complex Fourier series", Math. Comp. 19 (1965); Cormen, Leiserson, Rivest, Stein, *Introduction to Algorithms*, ch. 30 (iterative FFT). Shared-memory fusion of the first ≤10 stages and the per-stage global kernel are own design. sppark's NTT driver, twiddle tables, streams and device selection were not used (they cannot adopt the runtime's streams or budget). No open-icicle source, derivative or H21.md was opened. |
| Coset / normalization semantics | CODE (reference) | Behaviour matched to `midnight-proofs` 0.8.3 `EvaluationDomain` (MIT OR Apache-2.0) used as a dev-only test oracle via crates.io; no source copied. Formal statement: `gpu-spec/LokiLean/BackendNtt.lean` (P4 section). |
| Dev-only oracle dependencies | CODE | `midnight-proofs =0.8.3`, `midnight-curves 0.3.1` as `[dev-dependencies]` of `loki-gpu`; the Cargo.lock delta is dev-only (recorded in `design/p4/REVIEW.md`). |
