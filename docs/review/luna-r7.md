# luna-r7 performance review

## Concerns

- `README.md:51`, `README.md:170`, `LOKI-MULTIOPEN.md:137-143`: the README headlines the first W11 three-sample comparison (10.0% / 19.5% lower multiopen time and 0.7% / 3.9% lower whole-proof time). Later controlled results in `LOKI-MULTIOPEN.md:324-345` compare the optimized Run 2 with the serial control: multiopen is 12.83% / 19.00% lower, while `create_proof` is 2.74% lower at four threads and 0.11% higher at 20 threads. The compatibility rerun at `LOKI-MULTIOPEN.md:467-492` is one sample at four threads and should not replace the repeated-sample record. The headline numbers are traceable, but they omit the later comparison and may suggest a more stable whole-proof gain than the note supports.
  - Suggestion: identify the README figures as the initial three-sample result and summarize the controlled follow-up with its mixed end-to-end outcome, or state that the observed phase gain is repeatable while whole-proof improvement is not established. Keep the `k17` synthetic-fixture and sample-count context adjacent.

- `README.md:46-54`: the text says each result links to a note with method and raw samples, but the W10 row gives a wiki path as unlinked inline code; the actual local W10 evidence contains full matched-run details in `Moriarty-factory/wiki-llm/loki/gpu-lab/astra/RESULTS.md` and `RESULTS-DETAILS.md`, while `docs/GPU.md:415-421` cites session/claim notes. The values themselves trace: 294.285 s to 237.808 s, a 56.477 s / 19.1913% reduction, with 206.230721 s to 143.825054 s in `create_proof`, and both are single samples. The linked note should expose the detailed evidence path and exact interval definition more directly.
  - Suggestion: link the W10 row to a stable checked-in document or include a direct source link and label the measured interval as A3 prove-plus-verify. Preserve the single-sample qualifier.

- `docs/GPU.md:397-401` and `docs/GPU.md:385-395`: these figures trace to the controlled k17 median-of-three table in `LOKI-SRS-REGISTRY.md:292-300`. The note explicitly says this synthetic fixture is not a chain-speed estimate (`LOKI-SRS-REGISTRY.md:285-288`); GPU.md does too. No numeric correction found. The `2^20` cold legacy comparison is dominated by SRS validation and is explicitly separated from the warm-cache MSM table.

- `docs/GPU.md:202-205` says SRS upload validation is serial per upload, but the W13 registry path validates upload candidates on up to four native scoped workers (`LOKI-SRS-REGISTRY.md:22-27`, with measurements configured to use up to four at `LOKI-SRS-REGISTRY.md:281`). The serial statement can still describe the legacy cache path, but GPU.md does not limit it to legacy, so readers get conflicting behavior/performance expectations. W13's `k17 validation` measurement is 7,387.520 ms legacy vs 926.077 ms registry (`LOKI-SRS-REGISTRY.md:296-297`), and the same note attributes registry validation to worker-parallel candidate checks.
  - Suggestion: scope the statement to the legacy cache, and describe the registry's bounded parallel validation separately. Preserve the distinction that cold validation remains a major cost.

- `docs/GPU.md:360` and `LOKI-GPU-MSM.md` benchmark table: 13.17x and 16.58x recompute from the warm-mean measurements at `2^20` and `2^22` respectively. Both documents qualify these as shared-host MSM benchmark samples, not whole-prover gains. No numeric correction found.

- `docs/GPU.md:378-383` and `LOKI-GPU-NTT.md:128-135`: the ratios recompute from the median-of-three values. The complete proof is 1.11x (2172.838 / 1950.617), and the note says it is a synthetic fixture, not production recursion. No numeric correction found.

- `loki-gpu` `README.md:38-41` and `docs/OVERVIEW.md:184-191`: the old-path 220.8 s vs 220.2 s comparison and the W10 294.285 s to 237.808 s pair match the Loki GPU doc. The overview labels these as design motivation, not results of the new repository. No numeric correction found.

- `loki-gpu` `docs/OVERVIEW.md:131`: the claim that a naive quotient layout needs about 26 GB at k22 is presented without a direct source or calculation. I did not find that figure in the referenced `design/QUOTIENT.md` or elsewhere under `design/`. It may be a valid estimate, but the amount and model cannot be checked from the linked overview/design source as written.
  - Suggestion: add the memory calculation and assumptions to `design/QUOTIENT.md`, label it `ESTIMATE`, and link the overview sentence to it. If the derivation is unavailable, move this figure to TODO.

- `docs/ARCHITECTURE.md:323` repeats the W11 Run 1 headline (10.0% / 19.5% phase reduction and 0.7% / 3.9% end-to-end reduction) without the later controlled Run 2 result. The same context concern as the README applies; `LOKI-MULTIOPEN.md:339-345` gives the Run 2-vs-serial result and says the end-to-end comparison is 2.74% lower at four threads and 0.11% higher at twenty threads. `LOKI-MULTIOPEN.md:467-492` labels the later compatibility rerun as one sample and retains Run 2 as the repeated-sample record.
  - Suggestion: align the architecture summary with the README and note's explicit Run 1 / Run 2 distinction, plus that whole-proof gain is not established consistently.

- Architecture performance numbers were otherwise traceable in this pass: the W10 A3 pair at `docs/ARCHITECTURE.md:321` matches the exact clean single-sample comparison in the W10 evidence; W13 at `:322` matches `LOKI-SRS-REGISTRY.md:292-300`; the historical quotient profile and 78.4 s `optimal_k` / 117.3 s outside-proof values at `:106` and `:325` match `Moriarty-factory/wiki-llm/loki/LOKI-DESIGN.md:98-122` and are labeled historical/overlapped or estimates where appropriate. The 94 s outside-`create_proof` value recomputes from the W10 optimized pair (237.808 - 143.825 = 93.983 s). The repeated-scalar diagram values at `docs/diagrams/gpu-msm-skew.svg:11,66-72` match the corresponding GPU/MSM notes. NTT thresholds and the 4 GiB budget labels in the GPU diagrams match `docs/ARCHITECTURE.md:114-115,135`.

- `CHANGELOG.md:17`'s "about 7x" slowdown statement is supported by upstream PR #536, whose merged description says G1 MSMs above the `2^19` term cutover became about seven times slower and removes that cutover. The changelog's `2^19` threshold and k >= 20 circuit wording are consistent with that description: [midnightntwrk/midnight-zk#536](https://github.com/midnightntwrk/midnight-zk/pull/536).

## Questions

- Should the README W11 summary continue to headline Run 1, or should it foreground the later Run 2 comparison against serial? The note documents both and the whole-proof conclusion differs by comparison and thread count.
- Can the W10 evidence be linked from this repository at a stable path, or should the README retain the Moriarty-factory wiki reference?

## Suggestions

- Use consistent benchmark labels in top-level summaries: fixture or real A3 parent; operation, `create_proof`, prove-plus-verify, or whole chain; sample count/statistic; thread count; branch status. Current GPU docs generally provide this detail, while the README W11 row does not.
- Keep projected figures in the loki-gpu overview tied to their source and labeled as estimates. `docs/OVERVIEW.md:104-108` identifies the approximately 6 s NTT adapter figure as a weighted estimate. The approximately 26 GB k22 layout figure at `docs/OVERVIEW.md:131` is explicitly a naive-layout scenario, but its estimate basis is not present in the linked design.

## Novelty evidence

My performance lens found no numerical evidence of novelty to assess. The performance statements reviewed are measurements or estimates; a novelty claim needs a separate prior-art comparison.
