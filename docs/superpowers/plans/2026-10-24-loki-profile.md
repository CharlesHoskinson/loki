# Loki phase profiling implementation plan

**Goal:** expose optional, protocol-neutral per-proof wall timing and MSM/FFT sizes.

**Architecture:** cfg-gated guards update a thread-local active proof. Nested phases
charge exclusive wall time, with residual work charged to `other`. Completed
proofs enter a mutex-protected global report; `take_report()` drains completed
proofs atomically. JSONL append is serialized by that mutex. Existing serde derives
provide serialization; JSONL output uses std only. No arithmetic, randomness,
transcript, or verifier behavior changes.

**Constraints:** crate name/version remain midnight-proofs 0.8.3; no added runtime
dependencies; no hooks or env access compiled without `loki-profile`; four build
jobs and only one heavy proof at once; stay on the provided worktree branch.

- [x] Add a feature-required example/test with a real two-phase circuit, lookup,
  permutation and additive-selector trash argument. Assert each required phase,
  MSM/FFT histograms, serialization, destructive take, verifier acceptance and
  phase-sum agreement with the externally measured proof duration. Run it red.
- [x] Implement `src/profile.rs`: serializable report types, per-thread proof and
  phase guards, counters, atomic drain and synchronized std JSONL output. Add
  concurrent-proof and error/unwind coverage.
- [x] Instrument create_proof, synthesis per phase, advice commitments, lookup
  preparation/products, permutation/products, trash, compute_nu_poly, evaluator,
  quotient commitment, evaluations, multiopen (including its KZG implementation),
  MSM entry and every FFT entry. All hooks carry cfg attributes.
- [x] Run targeted release tests with/without the feature, JSONL append checks and
  default/feature checks. Review the diff, then commit the implementation.
- [x] Run the example at k16 with four Rayon workers. Record the full command,
  phase table, operation histograms, circuit shape and limits in LOKI-PROFILE.md.
  Verify documentation and commit it. Send final AgentBus status.
