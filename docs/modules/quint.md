# `quint/`: executable state models and Rust replay

## Purpose

Quint models of the stateful and ordered parts of Loki, plus a Rust crate that replays model traces against the real code. Safety checks use 10,000-sample simulation campaigns. They do not exhaust the state space.

## Contents

| Model | What it models | Safety properties |
|---|---|---|
| [`transcript.qnt`](../../quint/transcript.qnt) | The stock Fiat-Shamir event order (prover, lookup, permutation, trash, vanishing, all four multiopen challenges). Includes `transcript_instances` for multiple committed/plain columns. | `orderedPrefix`, `absorbedSequence`, `challengesBound` |
| [`gpu_dispatch.qnt`](../../quint/gpu_dispatch.qnt) | Threshold routing, two provers, pointer reuse, resident-cache contents, GPU errors and CPU fallback. GPU arithmetic equality is an assumed backend obligation. | `resultEqualsCpu`, `staleNeverUsed`, `fallbackTotal` |
| [`pk_cache.qnt`](../../quint/pk_cache.qnt) | Proving-key cache keyed by circuit hash, `k` and params hash. | `bindingCorrect`, `keygenAtMostOnce`, `noPartialUse` |
| [`scheduler.qnt`](../../quint/scheduler.qnt) | A **Planned** job scheduler (the L5 proposal in `LOKI-DESIGN.md`): four jobs, two classes, VRAM budget, checkpoint and resume. Not implemented in Loki. | `vramBudget`, `slotLimit`, `noLostOrDuplicated`, `checkpointIntegrity` |
| `*_test.qnt`, `mutations.qnt` | Reachability witnesses and model mutations that must be caught. | n/a |

Other files: `check.py` and `check_instances.py` (run the models and campaigns), `results.json`, `instance-order-counterexample.itf.json`.

[`quint/mbt/`](../../quint/mbt) is a separate crate (`quint-connect`, pinned commit, Quint 0.32.0) with `transcript_driver.rs`, `instance_order.rs`, `w4_driver.rs`, `pk_driver.rs`, `proof.rs`. It generates 24 model traces, creates randomized k5 proofs and checks every model state against logged operations. Both wrapped and plain proofs must pass the unchanged stock verifier. Features `w4-source` and `w6-source` read earlier source revisions from immutable local Git commits via `build.rs`; they fail if those commits are unavailable.

Run: `python3 quint/check.py`, then `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features w4-source,w6-source -- --nocapture --test-threads=1` with `RAYON_NUM_THREADS=2`.

## Difference from stock midnight-proofs 0.8.3

No production source, feature or manifest change. The transcript model's schedule is the stock sequence. A separate audit reproduces the stock defect for mixed committed/plain columns with several proofs and shows the real verifier rejects those cases.

## Lookup order and proof bytes

The W8 byte comparisons are historical and tied to the revisions recorded in [`LOKI-QUINT.md`](../../LOKI-QUINT.md). W3's BTreeMap change was a deterministic-order experiment. Current main uses stock `HashMap` iteration by default. The `loki-deterministic-lookup-order` feature is opt-in, sorts leftover values, and changes lookup proof bytes relative to stock. A fixed proof RNG and Rayon thread count do not make default HashMap iteration reproducible.

## Invariants and pitfalls

`Refinement.loki_proof_eq_stock` proves event equality for the structural Lean `prove` model, given backend operations satisfying `Refines` and commitment order satisfying `SameOrder`. It compares executions with the same parameters, proving key, circuit, witness, random stream, number of threads and fixed lookup-map order `ord`. This theorem does not connect all Rust or CUDA source to the model. `stock-trace-faithful` and `quot-inputs` remain open; the W8 trace and Rust replay results are empirical checks for selected paths.


- Results are bounded sampled safety plus listed implementation checks. They do not prove GPU arithmetic, scheduler liveness or universal fixed-RNG proof-byte equality.
- Fixtures cover plain, lookup/permutation, second-phase and additive-selector trash circuits, 1-2 proofs. Arbitrary query rotations and shapes need more fixtures.
- Use a fixed Rayon count for proof-byte comparisons.
- `scheduler.qnt` has no Rust counterpart. Do not read it as a description of shipped behavior.

## Notes

[`LOKI-QUINT.md`](../../LOKI-QUINT.md). Gates G5 and G6 in [`LOKI-DONE.md`](../../LOKI-DONE.md).
