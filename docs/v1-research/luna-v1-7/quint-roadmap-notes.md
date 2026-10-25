# Quint QA research and roadmap notes

Owner: luna-v1-7. Scope: Loki v1.0.0 models, source traceability, MBT/ITF replay, Apalache, CI contracts. This note distinguishes checked repository facts from future requirements. No production code or model campaign was run for this roadmap task.

## Current repository facts

- The top-level documented models cover transcript events, GPU dispatch, proving-key cache, and a planned scheduler. Safety invariants and witness names are in `docs/modules/quint.md:8-20`; the scheduler is explicitly not implemented.
- `quint/check.py:12-63` runs typecheck, unit tests, seeded Rust-backend simulation, required positive witness counts, then the instance-order audit. It does not dispatch the q1/q2/q4/q5 campaigns, mutations, Apalache, or Rust MBT.
- Haiku inventory Q7-CODE reports there is no CI workflow in this checkout; Quint gates are manual. `quint/mutations.qnt` is not run by `quint/check.py`.
- Component campaigns are separate: `quint/q1/check_q1.py`, `quint/q2/check.py`, `quint/q4/check.py`, and `quint/q5/check.py`. q2/q4 include Apalache checks; q1 includes an instance-order counterexample and lookup placement. Campaign results use separate JSON paths.
- Rust MBT exists in `quint/mbt/` and `quint/q2/mbt/`; current drivers cover transcript operations, PK cache behavior, W4 legacy cache, and P1 dispatch. The dispatcher driver uses real OS threads and a scripted fake accelerator. W4 uses CPU FFI surrogates, not a CUDA kernel.
- `quint/mbt/build.rs:64-74` reads W6 source from `/home/charles/repos/fenrir-loki` by default. Historical adapters are useful diagnostic reproductions but do not establish behavior for current release source. `quint/q2/mbt/build.rs:8-26` also retrieves code from a historical pinned snapshot.
- Current gaps include no Rust scheduler, no W5 NTT replay, mutations not connected to the top-level runner, limited transcript shapes, and expected stock mixed-instance rejection not wired as a release blocker. See `quint/q2/REPORT.md:124-130`, `quint/mbt/README.md:22-39`, and `LOKI-QUINT.md:280`.
- Current lookup default uses the stock randomized HashMap iteration. The deterministic-order feature is opt-in and can alter proof bytes. See `docs/ARCHITECTURE.md:29,100-104,128-160` and `src/plonk/lookup/prover.rs:393-429`.
- New cross-area fixture contract from luna-v1-2: `loki-harness/fixtures/v1/manifest.jsonl` references transcript vectors under `loki-harness/fixtures/v1/transcripts/<transcript_vector_id>.jsonl`. Event rows include ordinal, phase, event kind, artifact digest/value bytes, challenge label and challenge bytes; fixture records exclude witnesses. Compat-proof owns this schema and T03 case IDs.

## Proposed contract

Change: `v1-quint-model-coverage`.

- `quint/coverage.yaml` maps covered source paths to model, invariant, witnesses, mutation control, Rust replay, compat-proof fixture IDs, fuzz target, Lean status/contract, docs link and release SHA.
- `python3 quint/ci/run_models.py --release --seed 20261025` is the manifest-driven host model gate. It includes all in-scope model campaigns and fails on missing results, witnesses or mutation detections.
- `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features release-mbt -- --test-threads=1` is the release Rust replay contract. Every changed shipped behavior needs a real Rust replay; a missing seam blocks release. Historical snapshot replay alone does not satisfy it.
- Planned outputs are under `quint/release-results/<release-sha>/`, with trace/replay JSON, ITF, logs, versions, seed and bounds.
- Apalache bounded checks are recorded with their bound and are not reported as proofs unless completeness is established. Fair liveness is blocked for any shipped behavior until a temporal verifier supports the explicit fairness assumption. The current planned scheduler model is not shipped behavior and its sampled completion witness is not liveness evidence.
- Quint/Apalache run on host CI. WASM/mobile runtime gates consume compatibility fixtures on their own target jobs; Quint does not substitute for target execution.

## Outside best practice and source URLs

Official Quint property checking documentation:

- https://quint.sh/docs/checking-properties — distinguishes safety invariants from simulation, explains witness states are reachability checks rather than liveness, and documents bounded `quint verify`/inductive invariants.
- https://quint.sh/docs/model-based-testing — documents ITF trace generation, action/parameter replay, implementation/model state comparison, and says MBT does not prove code correctness.
- https://quint.sh/posts/quint_connect — documents Quint Connect's Rust driver/state mapping pattern and sampled replay limits.
- https://github.com/quint-co/quint-connect — upstream connector source and release history; repository pins revision `4f018f54fc7dd4cef341d10111427bab59d3b307` in `quint/mbt/Cargo.toml:20`.

Apalache temporal support reference:

- https://apalache-mc.org/docs/adr/017pdr-temporal.html — PDR temporal design notes describe temporal operators and fairness work; this is a design document, not proof that the pinned runtime supports fairness. Repository evidence at `LOKI-QUINT.md:280` says the current `quint verify` run rejects the scheduler weak-fairness expression with `Handling fairness is not supported yet!`.

## Verification limits

- Quint `run` is simulation. Fixed seeds and positive witness counts improve repeatability and reachability evidence but do not prove an invariant.
- Bounded verification only covers its declared bound. An exhaustive claim needs a finite-state completeness argument.
- An MBT pass covers only generated/replayed traces and the mapped Rust seams. It does not establish all-path equivalence, GPU arithmetic correctness, or liveness.
- Stock compatibility needs unmodified 0.8.3 and shared fixture execution; models do not replace bidirectional verifier/API gates.
- A model-only line item is acceptable for a genuinely deferred, unimplemented feature when plainly labeled. It cannot be a pass for changed shipped behavior.

## Scout review and roadmap convergence

Haiku-v1-7 reviewed the OpenSpec draft. Its findings were incorporated: the source-path scope points to the model inventory rather than the top-level runner's model list; Apalache checks are labeled with a bound and are not called exhaustive without a completeness argument; fake-backend MBT counts only for real dispatcher/host policy, not GPU arithmetic/device behavior; T03 expected stock rejection is explicit; and T20/T28/T29/T30/T31/T33 receive coverage rows or named blockers. The release gate now fails if any in-scope invariant lacks a registered mutation/counterexample control, and requires vendored or immutable dependencies for an offline MBT run. Haiku's final notes also identify T29 fairness support and release-source adapters as implementation blockers; those remain clearly listed in the OpenSpec tasks.

The author ACKed ROADMAP DRAFT R1 and R2. Quint ID matches `v1-quint-model-coverage`. T28 cancellation, T29 liveness, T30 transcript fixtures, T31 dispatch/NTT replay, T20 cache identity and T33 cache retention map to this area. Shared fixture schema is compat-proof-owned; release CI aggregation is docs-release-owned; WASM/mobile target execution is platform-owned. The current planned scheduler is not treated as shipped evidence, but P7 scheduling must acquire a model and current-source Rust replay if it ships. R2 clarifies that fake-driver MBT only qualifies the actual dispatcher/host seam, not GPU arithmetic or device behavior, and requires the current release source SHA.
