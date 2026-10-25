# ghaiku-1 REPORT: ICICLE public documentation on MSM sorting, precomputation, async and residency

Requested by: astra-gpu (RESEARCH ASSIGNMENT on #lokigpu). Copy to gluna-10 for PROVENANCE.
Accessed: 2026-10-25 (UTC; bus timestamp).

## Source boundary (followed)
- Public ICICLE documentation pages only (dev.ingonyama.com, versions 2.8.0 and 3.0.0/3.6.0 as cited).
- No source repositories, no source snippets, no GitHub browsing, no open-icicle code, no lab-haiku-1/H21.md.
- Third-party blog or wiki pages surfaced by search (hackmd, gitbook, medium, pkg.go.dev) were NOT fetched. Search snippets from them are not used as findings.

## Sources
- S1 https://dev.ingonyama.com/3.6.0/icicle/primitives/msm (MSM config reference, 3.6.0)
- S2 https://dev.ingonyama.com/2.8.0/icicle/primitives/msm (MSM algorithm and is_async, 2.8.0; page states 2.8.0 is no longer maintained)
- S3 https://dev.ingonyama.com/2.8.0/icicle/rust-bindings/msm-pre-computation (precomputation, 2.8.0)
- S4 https://dev.ingonyama.com/3.0.0/icicle/programmers_guide/general (streams, memory, residency, 3.0.0)
- S5 https://dev.ingonyama.com/3.0.0/icicle/programmers_guide/rust (async memory calls, 3.0.0; found by search, not separately fetched; treat as UNVERIFIED)

Mixed versions: the API changed between 2.x, 3.x and 3.6. Every claim below is tied to the version it came from.

## Findings (labelled)

### 1. Bucket sorting and grouping (S2, 2.8.0) - DOCUMENTED
- "The scalars are split into smaller scalars of `c` bits each. These are the bucket indices."
- "The points are grouped according to their corresponding bucket index and the buckets are sorted by size."
- A bucket is "large" if its size is above the large-bucket threshold; "More than one thread is assigned to large buckets, in proportion to their size."
- GPU pipeline is described as four phases; the page names at least: accumulation (each bucket accumulated by a single thread) and final accumulation (double-and-add over the results).
- CONCEPT (not implementation): the sort-by-bucket then split-large-buckets idea is a known scheme. It is described in public docs and in the Pippenger literature, so it is independently justified for our design, not an ICICLE-specific licence question. Implementation detail is not disclosed by these pages.

### 2. Large-bucket threshold and config extension (S1, 3.6.0) - DOCUMENTED
- "The large bucket threshold is the expected average bucket size times the `large_bucket_factor` parameter."
- Guidance: "`large_bucket_factor` that is equal to 10 yields good results for most cases." The page gives no default.
- It is set through a CUDA-only ConfigExtension (`ext.set("large_bucket_factor", 15)`, then `msm_config.ext = &ext`). Backend-specific knobs are carried in `ext`, not in the core config.
- `is_big_triangle` is mentioned but its usage is not shown.
- SEARCH-SNIPPET CLAIM NOT CONFIRMED: a search summary said the default c is `min(20, max(1, degree-4))` with a cap of 20. S2 says instead: "Default value: 0 (the optimal value of c is chosen automatically)" and gives no cap. Treat the 20-cap claim as UNVERIFIED and do not use it.

### 3. Choice of c (S2, S1) - DOCUMENTED
- No precomputed points: "the best `c` value is usually around log2(msmSize)−4".
- With maximum precomputation: a c around log2(msmSize)−1 is suggested (S2 text, typo in source).
- S1: "The two most important parameters for performance are `c` and the `precompute_factor`."

### 4. Precomputation (S1, S3) - DOCUMENTED
- `precompute_factor` (S1): default 1; "controls how many precomputed points are stored", a memory-for-speed trade-off.
- S1 formula: number of bucket modules = ceil(ceil(bitsize / c) / precompute_factor).
- S1 advice: when points are known in advance, "it's best to use maximum precomputation (such that we end up with only a single bucket module)".
- S3 (2.8.0): each base P is extended with multiples 2^l·P, 2^(2l)·P, ..., 2^((precompute_factor−1)·l)·P, where l is set by precompute_factor.
- S3: "Its size must be `points.len() * precompute_factor`." The buffer "should be allocated on the device for GPU computations."
- S3: precomputed bases are used as the bases in the MSM call; the user allocates `output_bases` on host or device.
- Matching rule (precompute factor must match between the precompute call and the MSM config): found only in search snippets, NOT on S1/S3 text. Mark UNVERIFIED; check before relying on it.
- Points not known in advance: precomputation is not usable (S1 and search, partly UNVERIFIED).
- CONCEPT: precomputed multiples trade static memory for fewer online point additions. This is independently justified (standard fixed-base technique) and does not depend on ICICLE.
- Implication for us: our resident-bases model (SRS) makes precomputation a candidate design knob. Size cost is points * factor * point size, which must fit in the 32 GB card alongside NTT buffers and the 6.2 GiB/proof traffic noted in goal.md. Estimate only; not measured.

### 5. Async and streams (S1, S2, S4) - DOCUMENTED, with gaps
- S2 (2.8.0): "If set to true, the MSM function will be non-blocking" and "If set to false, the MSM function will block the current CPU thread."
- S4 (3.0.0): "For compute APIs, streams go into the `config.stream` field along with the `is_async=true` config flag." Streams are "associated with specific devices."
- S4: "Synchronization ensures that all previous operations on a stream or device are completed." S4 does NOT state who must synchronise after an async compute call. S1 does not explain `stream` or `is_async` at all. OPEN QUESTION: the caller must call a stream sync before reading results; this is inferred, not stated.
- S5 (search only): async allocation via `device_malloc_async` on a stream, then an explicit sync; async H2D and D2H copies. UNVERIFIED (not fetched).

### 6. Residency (S1, S4) - DOCUMENTED, with gaps
- S1 config flags: `are_scalars_on_device`, `are_points_on_device`, `are_results_on_device`, all default false. S1 does not say where results are written when the flag is false, beyond the default.
- S4: inputs "can reside either on the host (CPU) or on a device (GPU)"; output: "If true, the output is preserved on the device, otherwise on the host." Default host.
- S1 memory section gives size formulas for scalars, indices, points and buckets (not quoted in full here); it does not discuss residency policy.
- Implication: residency is per-call flags, not a resident-buffer model. Our design should keep residency explicit in the backend API (resident buffer handles), not rely on per-call flags.

### 7. Multi-GPU (2.8.0 search snippet) - UNVERIFIED
- A 2.8.0 Rust example sets a device, builds a per-device config and runs MSM with is_async. Not fetched; pre-v3 API. Not used.

## Concept vs implementation vs licence
- No licence text was read as part of this research. Licence of ICICLE is NOT established here; do not infer it from docs.
- Everything in sections 1-6 is API semantics or conceptual technique described in public docs. None of it is code. Implementation details of how ICICLE does sorting or accumulation are NOT disclosed by these pages and must not be reconstructed from source.

## Limits
- Docs describe API behaviour, not measured performance. No numbers in this report are benchmarks.
- Docs are a mix of 2.8, 3.0 and 3.6; the fields and defaults may differ by version. Versions are noted per claim.
- Pages that were only search snippets are marked UNVERIFIED.
- No GPU access; nothing run.

## Actionable implications for loki-gpu
1. Keep bucket sort, large-bucket splitting and c selection as our own design items (WP for MSM). Give the large-bucket threshold factor a tunable parameter with a default around 10, flagged as a starting point to be measured on our scalar distributions (repeated/boolean/zero from goal.md).
2. Make the c choice a function of msm size (log2(n)−4 without precompute, log2(n)−1 with), as a starting heuristic to benchmark.
3. Treat precomputation as an optional SRS-residency feature: its memory cost must be budgeted against 32 GB; gate it behind capability reporting in loki-backend-api.
4. Model stream and async explicitly in the backend trait: a stream handle in the call, a non-blocking mode, and an explicit completion/sync call. Do not depend on the caller guessing the sync point.
5. Replace per-call residency flags with resident buffer handles for SRS bases and NTT working sets; keep host/device choice explicit in types.
6. Open questions to resolve with a hardware-free check: (a) who synchronises after async compute; (b) the precompute/config factor-matching rule; (c) the default c and any cap. Each needs a primary source, not search snippets.

## Curve applicability (added after astra-gpu CURVE CORRECTION)
- Loki's deployed KZG uses BLS12-381. The ICICLE pages read above (S1-S4) are curve-generic in text and do not name a curve, so no BLS12-381 applicability is established by them. Mark as OPEN: confirm curve support and scalar ABI from the ICICLE public docs for the curve versions we would use, before any design relies on the figures. No BN254 figure is used in this report.
