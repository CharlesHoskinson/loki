# luna-r9 review: formal methods and prior art

## Scope

Reviewed `README.md`, `docs/modules/quint.md`, `LOKI-QUINT.md`, `LOKI-SRS-REGISTRY.md`, the supplied `loki-lean` tree and `loki-lean-wt/gpu-spec`, plus the `loki-gpu` README/overview as read-only context. I did not run Lean, Cargo, GPU, or model campaigns. Public comparison used project documentation and public repositories; I did not open open-icicle source or `H21.md`.

## Concerns

- `README.md:154` accurately describes a conditional theorem but can be read as a proof of the deployed prover. In `loki-lean/LokiLean/Refinement.lean:640-651`, `prove` is explicitly a structural model, not a line-by-line transcription; permutation/trash, quotient carry, full evaluation/multiopen behavior are abstracted, and `stock-trace-faithful` remains an obligation. `loki_proof_eq_stock` at `:844-858` composes the same abstract `prove` program under `Refines` and `SameOrder`; it does not itself establish the source-to-model relation or prove concrete GPU arithmetic. The README does correctly say component obligations are not completed, but should name this boundary next to the theorem claim.
  - Proposed edit: "The Lean theorem proves that the abstract prover model emits the same events when a backend satisfies the stated component contracts and preserves event order. It does not prove that the Rust/CUDA implementation satisfies those contracts; W2 and the component tests provide empirical checks." Put the unclosed `stock-trace-faithful` and `quot-inputs` items in TODO until the model is tied to the full source behavior.

- The transactional runtime in `loki-lean-wt/gpu-spec/LokiLean/Backend.lean:262-280` is an abstract model, not an extraction or proof of Rust runtime behavior. The comments explicitly leave resource lifetime to `runtime-lifetime` tests and compute-sanitizer. The current tree also has theorem holes at `BackendMsm.lean:270-282` and `BackendQuotient.lean:176-182`, and an open real-compiler lowering obligation at `BackendQuotient.lean:387-393`. Avoid calling the GPU specification fully verified or saying its runtime model proves the shipped runtime. The P2 runtime contract can be described as a model plus implementation tests, with those remaining obligations listed.

- The exact-prefix SRS registry has strong empirical and source evidence, but the W13 note says a Lean prefix-refinement lemma was requested and had not been proved or checked (`LOKI-SRS-REGISTRY.md:178-186`). `stockHit_sound`/`cachedMsmSeq_eq` cover the modeled cache claims; the documented residual prefix refinement remains open. Keep this proof gap separate from the measured W13 performance result.

- The separate `loki-gpu` documentation checkout reviewed during the initial pass had stale status text: its `README.md:15-17` called P1 and P2 in progress, and `docs/OVERVIEW.md:1-4` described a compiling skeleton. The lead later confirmed P2 is merged in `loki-gpu` at `d71b7ca`. Treat those earlier checkout statements as historical; they do not describe current main.

- `docs/modules/quint.md:5,19-32` is appropriately bounded: it calls out 10,000-sample simulation, the GPU arithmetic assumption, absent scheduler implementation, and limited fixtures. No correction is needed there. The replay technique already appears in public Quint Connect material: the official Quint Connect project describes generated Quint traces replayed against Rust implementations with state validation. Loki's contribution is the particular prover transcript, dispatch/cache and proving-key-cache coverage, including its reported stock-verifier diagnostics, and does not establish a new Quint-to-Rust replay method.

## Current source update (2026-10-25)

The docs worktree now points to Loki main `a256a9a15ff18ae12a8131b86cff576787fa5e3c`, with P1 merged and its source changes present. The lead reports P2 merged in `loki-gpu` at `d71b7ca` and P4 in progress. W8 measurements and source replays remain tied to the older revisions recorded in their reports.

I checked the current `/home/charles/repos/loki-proofs` source. `Cargo.toml` exposes `loki-backend-api`, `loki-sppark`, `loki-gpu-backend`, and `loki-deterministic-lookup-order`. In `src/plonk/lookup/prover.rs:425-433`, the default path iterates leftover lookup values in `HashMap` order. The optional feature sorts them by field order, and the source comment says this changes lookup proof bytes relative to stock. Keep W8's byte comparisons tied to their recorded revisions; do not present the optional ordering as current default behavior.

## Questions

- Which synchronized Lean snapshot should the final novelty note cite? Both local Lean trees report `SYNCED-LOKI-COMMIT` as `877114d`, while the docs and runtime source statuses refer to later changes.
- Is the novelty note intended to claim first-of-kind results, or to document the distinctive combination and measured implementation? The public search below rules out broad priority claims; the narrower combinations need a more complete, dated comparison before claiming novelty.

## Suggestions

- Define each formal claim with three layers: theorem over the Lean model; separate obligation connecting the model to source; and empirical gate for deployed Rust/CUDA. Keep "proved" for the first layer only.
- Describe Quint as "Quint Connect model-based replay applied to Loki's transcript and cache paths," with the exact trace/test counts and historical source revisions adjacent to the statement. Keep "bounded sampled safety" and "no GPU kernel ran" visible where dispatch replay is summarized.
- For the exact-prefix registry, describe the implementation facts and measurements. Avoid an unsupported priority claim for GPU SRS caches. Its measured comparison is clear in `LOKI-SRS-REGISTRY.md:274-344`; the note also says the corresponding Lean prefix refinement has not been proved (`:178-186`).
- Record a TODO for any first/novelty claim that lacks a completed prior-art comparison. The shortlist in the brief (ICICLE, halo2 GPU forks, Ingonyama, sppark, zkSync, Scroll, Axiom, PSE, papers) has not been exhausted by this lens.

## Outstanding formal obligations for TODO

Read each item as a distinct open assumption or source-to-model gap. The base Lean snapshot proves the stock-compatibility theorem itself; these items bound its scope.

1. `stock-trace-faithful` (`loki-lean/LokiLean/Refinement.lean:640-651`): tie the structural `prove` model to the full stock prover event trace. The model omits parts of permutation/trash behavior, quotient carry handling, the full evaluation section, and the real multiopen rounds.
2. `quot-inputs` (`Refinement.lean:841-842,1101-1107`; GPU spec `Backend.lean:178-180`): establish that the actual prover adapter gives the quotient backend the intended extended cosets and challenges.
3. `stock-complete-any-order` (`Refinement.lean:887-902`): establish stock verifier completeness for every admissible lookup-map order before using the event equality to conclude acceptance for Loki's deterministic order.
4. `keygen-deterministic` (`Refinement.lean:1313-1315`): the cache model treats keygen as a pure function; the evidence cited is source inspection and W2 byte comparison, not a Lean proof.
5. Exact-prefix cache refinement (`LOKI-SRS-REGISTRY.md:178-186`): prove the requested-prefix/capacity relation against the shipped W13 registry; the note says this was not checked. The separate `srs-identity-immutable` assumption (`GpuSystem.lean:340-344`) concerns a future digest-keyed cache and is not needed for the shipped exact-coordinate comparison.
6. `msm-segmented-partition` (GPU spec `BackendMsm.lean:270-282`): prove that segmented bucket reduction equals `pippengerList`; the current theorem body is `sorry`.
7. `quotient-interpreter-safety` (GPU spec `BackendQuotient.lean:176-182`): prove every valid quotient bytecode program has initialized operands and output; the current theorem body is `sorry`.
8. `quotient-compiler-lowering` (GPU spec `BackendQuotient.lean:387-393`): prove that the real compiler lowers every ordered source pass, challenge binding, column and rotation to a validated bytecode program.
9. P2 runtime source refinement (GPU spec `Backend.lean:264-280`): relate `runtime.rs` transitions, native stream/event completion, and the Lean `Runtime` model. The model takes completion as an input and is not an extraction of Rust.
10. `runtime-lifetime` (GPU spec `Backend.lean:278-280`): prove staging reuse and free-after-retire lifetime rules; these currently have fault-injection/runtime tests and compute-sanitizer evidence, not Lean proofs.
11. `digest-binds` (GPU spec `Pipeline.lean:65-80`): prove the proving-key disk-cache identity and file-integrity assumptions, including the relation/config binding; the finite digest collision-resistance premise is not provable in Lean and the current evidence is a W12 fresh-vs-cached byte gate.

The base `loki-lean` source tree has no current `sorry` in its files, but that does not close the named model/source assumptions above. The separate `gpu-spec` snapshot has two actual `sorry` bodies at items 6 and 7; historical prose in its `AUDIT.md` also records earlier revisions, so the live theorem files are the authority for this count.

## Novelty evidence

- **GPU prover:** Axiom's public `halo2-gpu` repository already describes a Halo2 prover with CUDA acceleration on the prover path and credits sppark for Pippenger MSM kernels. It therefore rules out "first GPU-accelerated Halo2 prover." Loki's narrower evidence is the midnight-proofs 0.8.3 / BLS12-381 KZG integration and its independent stock-verifier checks, not GPU acceleration alone. [Axiom halo2-gpu README](https://github.com/axiom-crypto/halo2-gpu)
- **Reusable GPU bases / SRS:** ICICLE's public MSM docs describe precomputing base points and reusing them for repeated MSMs. That is prior art for reusable MSM-base preparation. The opened docs do not establish whether ICICLE has Loki's address/capacity filter plus exact requested-prefix coordinate comparison, or its registry policy. Describe the local contribution as Loki's measured, opt-in exact-prefix registry with exact validation and bounded LRU behavior; leave any "first" claim in TODO pending a wider source and documentation comparison. [ICICLE MSM docs](https://dev.ingonyama.com/3.1.0/icicle/primitives/msm)
- **Lean / Halo2:** A public Lean 4 repository already formalizes Halo2-related PLONKish gate soundness and gadgets. It is distinct from Loki's abstract prover-backend refinement model. This prevents a general "first Lean formalization of Halo2" claim. The reviewed sources do not establish priority for Loki's narrower conditional event and proof equality theorem. [halo2-formal](https://github.com/oxarbitrage/halo2-formal)
- **Quint replay:** Quint's own documentation and `quint-connect` repository explicitly cover generating model traces and replaying them against Rust code while checking state. Loki's specific trace corpus and prover/cache integration may be distinctive; neither "first Quint Connect replay" nor a method-level novelty claim has support. [Quint model-based testing docs](https://quint.sh/docs/model-based-testing), [quint-connect](https://github.com/quint-co/quint-connect)
- **Transactional runtime model and clean-room backend:** The Lean runtime section is explicitly a model of observable rules, with runtime-lifetime checks external to the proof. Clean-room status records provenance and licensing. It does not establish novelty on its own. The reviewed public sources do not establish whether similar models or backends exist. Keep a priority claim in TODO until a source-backed comparison is completed. The design's own provenance register and exact code boundary are more suitable evidence for the clean-room statement than an unsupported first claim. The local read-only checkout was reviewed; no public remote URL was supplied.

## Public primary sources reviewed

- Axiom `halo2-gpu`: CUDA Halo2 prover and sppark acknowledgement: <https://github.com/axiom-crypto/halo2-gpu>.
- ICICLE MSM documentation: <https://dev.ingonyama.com/3.1.0/icicle/primitives/msm>.
- Lean Halo2 formalization repository: <https://github.com/oxarbitrage/halo2-formal>.
- Quint MBT documentation and Rust adapter: <https://quint.sh/docs/model-based-testing>, <https://github.com/quint-co/quint-connect>.
- sppark public repository: <https://github.com/supranational/sppark>.
