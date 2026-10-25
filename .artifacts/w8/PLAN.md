# W8 Quint implementation and validation plan

The user-authorized scope is four executable specs, implementation-connected tests,
10,000-sample safety runs, reachability witnesses, and a reproducible report. Work
stays on loki/w8-quint; stock production sources and default features stay unchanged.

## Source correspondence and assumptions

* Transcript: `src/plonk/prover.rs` compute_trace, parse_advices, finalise_proof;
  lookup/permutation/trash/vanishing prover modules; `src/poly/kzg/mod.rs` multi_open.
  One event is atomic. Model event identity, common/write distinction, phase and
  proof/column order, with symbolic payloads. Rust wrapper compares actual payload
  order and proof bytes with a plain transcript and checks the stock verifier.
* GPU: no implementation at base bd1f0cc. Model two concurrent requests, threshold,
  upload/cache lookup, pointer reuse, immutable in-flight handles, errors and CPU
  fallback. CPU/GPU arithmetic equality is a backend obligation, not proved by
  symbolic equalities. Obtain W3 API before claiming an implementation differential.
* PK: no implementation at base. Model full (circuit hash,k,params hash) keys,
  single-flight claims, concurrent waiters, publication and use. No eviction,
  process crash or hash collision. At most once means successful keygen per cache
  lifetime; failure/retry must be documented separately if implemented.
* Scheduler: design L5, no implementation. Finite jobs, measured reservations
  abstracted to integer units, at most one heavy plus one small resident. Checkpoint
  and resume are separate actions. Completion requires fair service and finitely
  many checkpoints. Random progress is evidence, not a temporal proof.

## Steps and verification gates

- [ ] Read requested kit skills, agent methods and guidelines; inspect source.
- [ ] Create types, then executable actions, checking typecheck and short simulation
  after each model. Put deterministic scenarios in separate `_test.qnt` files.
- [ ] Run all model tests with `--match '.*'`; add negative guard and mutation checks.
- [ ] Attempt pinned public quint-connect in a separate `quint/mbt` crate. If it
  cannot build, record the error and use `quint run --mbt --out-itf` replay instead.
- [ ] Connect real create_proof using a logging transcript, random small circuits,
  fixed-randomness proof-byte comparison, and stock verifier acceptance. Never log
  witnesses, transcript payloads or setup secrets.
- [ ] Check each safety conjunction using 10,000 samples, a fixed seed, bounded
  steps and four simulator threads; require positive witness counts.
- [ ] Attempt scheduler temporal verification if Apalache works; otherwise perform
  long simulations and explicitly retain the temporal-proof limitation.
- [ ] Review final source mapping, run targeted Rust/default checks, write
  `LOKI-QUINT.md`, commit verified milestones through host runner, send DONE.

## Reviewable deliverables

`quint/{transcript,gpu_dispatch,pk_cache,scheduler}.qnt`, matching scenario tests,
`quint/check.py` for reproducible checks and summarized machine results,
`quint/mbt` for the isolated Rust test driver, and `LOKI-QUINT.md` for evidence and gaps.
Generated traces/logs live under `.artifacts/w8` and contain only symbolic model
states; cryptographic values remain in memory.
