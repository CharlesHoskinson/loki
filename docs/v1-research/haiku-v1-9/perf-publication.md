# TASK perf-publication research: haiku-v1-9

## Loki current docs and artifacts
- Wiki closing condition and RESULTS.md (missing): ~/repos/Moriarty-factory/wiki-llm/loki/benchmark/README.md.
- SUITE.md (execution spec, status NOT READY): ~/repos/Moriarty-factory/wiki-llm/loki/benchmark/SUITE.md. Its requirements (pinned SHA, stock 0.8.3 verifier on every proof, separate driver workspaces, seven or ten process blocks, cold and warm split, four and twenty CPU masks) are the strongest methodology in the repo.
- Repo-side: loki-harness/README.md:60-70 (timer boundaries; one cold sample; "These timings do not establish a speed difference"). LOKI-PROFILE.md:16-36 and 151-175 (profile boundaries, stock acceptance distinct from byte equality). README.md:53 (W11 medians of three, end-to-end not established).

## External sources
- zk-Bench (SCN 2024; ePrint 2023/1503): motivation that claimed results are generated in isolation and omissions render them irreproducible; open-source platform; reports hardware sensitivity. https://eprint.iacr.org/2023/1503 (I read only the abstract; repetition and statistics not verified).
- Hulek, Peresini, Homoliak, ePrint 2026/1729: validate the proved statement, disclose security profile, report resource limits alongside proving time; compare only comparable workloads. https://eprint.iacr.org/2026/1729 (abstract only).
- ZKProof community proposal, "A Benchmarking Framework for (Zero-Knowledge) Proof Systems" (2020): apples-to-apples comparison, front-end/back-end split, pre-processing and batching modes. https://docs.zkproof.org/pages/standards/accepted-workshop3/proposal-benchmarking.pdf (not read in full; UNVERIFIED).
- Side-channel benchmarking review: report error rates, noise resilience and how they were computed. https://arxiv.org/abs/2609.03893 (abstract only; the 83-paper survey and 19 flaws are from the abstract).

## Publication checklist derived for Loki (from SUITE.md plus sources)
- Pin: full Loki SHA, stock version 0.8.3 with its checksum, all dependency digests, Rust version, build features per arm.
- Disclose: host (WSL2, 20 CPUs, 90 GiB per SUITE.md:29), masks, Rayon worker counts, cache state (cold/warm), GPU state.
- Report: sample count, order, dispersion (median plus IQR or CI), all raw samples at a stable path (T41), slowdowns beside speedups (wiki README closing condition 4).
- Verify: every Loki proof accepted by unmodified stock verifier before timing.
