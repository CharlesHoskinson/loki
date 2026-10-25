# Q7-MBT: Quint MBT / ITF replay, Rust integration, fuzz overlap (haiku-v1-7)

Sources fetched 2026-10-25. Claims without a URL are my own analysis and are labelled.

## 1. Official MBT workflow

- Generate traces: `quint run bank.qnt --mbt --n-traces=10000 --out-itf=traces/out.itf.json`. `--mbt` adds `mbt::actionTaken` (action that reached the state) and `mbt::nondetPicks` (nondeterministic choices). `--n-traces` keeps the longest traces when there is no violation; more than 10k traces also needs `--max-samples`. Output is numbered files. Source: https://quint.sh/docs/model-based-testing
- Replay: the driver reads each ITF trace, calls the implementation method matching `actionTaken` with parameters from the state or `nondetPicks`, and compares implementation state with the trace state. The "init" action happens only at the first state. Source: same page.
- Assertions suggested by the official guide: state equality, error iff the trace records an error, and model properties. Source: same page.
- Limits as stated by the official guide: "Both of these won't give you a proof that your code is correct." The Rust instructions on that page are marked deprecated in favour of Quint Connect. Source: same page.

## 2. Quint Connect (the library the repo pins)

- `quint-connect` is a Rust MBT framework for Quint + Rust. Macros: `#[quint_run]` (random simulation, e.g. `max_samples = 1000`) and `#[quint_test]` (replay a named Quint test). Requires Rust 1.70+ and Quint on PATH. Sources: https://github.com/quint-co/quint-connect and https://lib.rs/crates/quint-connect
- Glue: you map implementation state to a spec-state type (`from_driver`, a `State` trait) and dispatch transitions with a `switch!` in `Driver::step`. The library reports missing transitions/parameters. Source: https://quint.sh/posts/quint_connect
- Trace sources: random scenarios, witness-derived scenarios, or Quint runs. Source: same page.
- Production trace validation is not available; the authors call it "future work" and "a hard problem". Source: same page.
- Authors' practice advice: happy-path first, then add failure transitions; use Quint runs for precise scenarios and random simulation for coverage; "we believe it is better to use Quint Connect than to do free-form connection". Source: same page.
- Quint's FAQ says the connector consumes ITF directly. Source: https://quint.sh/faq (cited via search summary; UNVERIFIED wording).

Repo fit: quint/mbt/Cargo.toml:20 pins quint-connect at rev 4f018f5 (0.1.2 per quint/mbt/README.md). It follows the recommended pattern; LOKI-QUINT.md says the library generates `quint run --mbt --out-itf` traces itself, so no hand-written ITF parser is needed. Consistent with the docs.

## 3. Evidence limits (what a green MBT run does and does not show)

- It shows that the replayed implementation paths match the model's state after each transition, for the sampled traces and fixed seeds. It does not show coverage of all model transitions or all implementation paths.
- Repo-specific limits, from LOKI-QUINT.md (verified in-repo): the W4 replay runs CPU-backed FFI and "No GPU kernel ran" (item for W4); the W6 replay uses small fixtures on midnight-circuits 7.2.4; the scheduler has no implementation to replay; W5 NTT is not exercised (T31 open); the transcript has 24 traces / 1,022 operations; fixed-RNG byte equality is not universal (item 2).
- Non-hermetic inputs: quint/mbt/build.rs:64-67 defaults to `/home/charles/repos/fenrir-loki` at commit abe9701. CI cannot reproduce this without the repo.
- Quint is a pinned 0.32.0. UNVERIFIED whether the quint-connect rev is compatible with a newer Quint; test before bumping.

## 4. Overlap with fuzzing (own analysis, not sourced)

- MBT tests sequences of model actions (state machine level) against the model's states. Fuzzing (fuzz/ at repo root, LOKI-FUZZ.md) tests byte-level inputs (deserializers, proof bytes) and differential behaviour against stock. The two overlap only at the transcript/absorption layer, where a fuzzed proof and an MBT-generated sequence both hit the same absorption order. Recommendation: MBT owns ordering and state invariants; fuzz owns byte acceptance and stock differential. Each should cite the other where they touch the same code.
- The instance-order mismatch is a case where both matter: MBT (`transcript_instances` audit) shows the mismatch in the model; a fuzzer over multi-proof mixed layouts would also show stock rejection. A release gate should require the stock-rejection diagnostic to remain green (documented expected failure), not silently pass.

## 5. Suggested release gate (for the v1 roadmap)

Gate (blocking, release branches and tags):
1. ITF generation from every top-level model with `quint run --mbt --out-itf` at a fixed seed; store traces with the quint version and seed.
2. `cargo test --locked --manifest-path quint/mbt/Cargo.toml --features w4-source,w6-source -- --test-threads=1` green, with the pinned quint-connect rev and RAYON_NUM_THREADS fixed (LOKI-QUINT.md: stock vanishing randomness depends on worker count).
3. Expected-failure diagnostic `diagnose_stock_mixed_instance_order` (quint/mbt/src/instance_order.rs:16) must still show the documented stock rejection; a change in that result is a compatibility event and needs an explicit decision (T03).
4. Every trace state checked, and the count of replayed operations recorded (currently 1,022).

Non-blocking (report only):
5. Trace validation against production logs (not available; not tracked in TODO.md).
6. Full W5 NTT and scheduler replay: blocked on T31 and T26.

## Sources
- https://quint.sh/docs/model-based-testing (official)
- https://quint.sh/posts/quint_connect (official blog)
- https://github.com/quint-co/quint-connect (library)
- https://lib.rs/crates/quint-connect (crate metadata)
- https://quint.sh/faq (official FAQ, partially verified via search summary)
