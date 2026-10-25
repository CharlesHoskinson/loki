# cp-review: review of v1-compat-proof-interoperability (haiku-v1-2)

Reviewed: proposal.md, design.md, tasks.md, specs/proof-interoperability/spec.md under ~/repos/loki-wt/v1/openspec/changes/v1-compat-proof-interoperability/.

Mechanical check: `openspec validate v1-compat-proof-interoperability --strict` returns "Change 'v1-compat-proof-interoperability' is valid". Passes.

Four-point check:
1. Testable requirements with WHEN/THEN: every requirement has at least one `#### Scenario:` in WHEN/THEN form. Several THEN clauses are not testable as written (findings 1-3).
2. Named verification: every scenario names a command or gate. Several commands are not runnable as written (findings 4-5).
3. file:line citations: most resolve. Several do not (findings 6-8).
4. Backward compatibility and WASM/mobile: consistent. Stock 0.8.3 is the oracle, default features are unchanged, GPU is excluded, WASM browser/Node and iOS/Android are covered (Requirement "CPU compatibility SHALL run on WASM and mobile targets"). One gap is noted in finding 9.

## Numbered findings

1. Requirement "W2 SHALL cover the declared circuit and k matrix" (and the same in its scenario) refers to "every declared circuit family". No list of declared circuit families exists in the repo or the change. Not testable until the families are enumerated in the spec or a named manifest. Suggested fix: name the families (for example `Fixture<LOKI>` from `loki-harness/src/backend.rs:62-111` only, which is currently the sole family) or cite an enumerated list file.

2. Requirement "W2 SHALL classify plain, committed, and batched instances" and the T03 requirement use "supported multi-proof shapes" and "supported fixture" without a definition. The THEN clause ("records instance mode, proof count, both verifier outcomes") is testable, but the set of shapes it applies to is undefined. Suggested fix: enumerate the shapes in the manifest schema (design.md Decision 2 already lists `plain`, `committed`, `mixed`).

3. Requirement "Transcript events and challenges SHALL match stock 0.8.3" has a verification (`quint test quint/transcript_instances_test.qnt` + Rust MBT replay) that compares Loki against the Quint model, not against stock 0.8.3. The requirement's SHALL is about stock equality, so the scenario does not verify it. Suggested fix: add a stock-side transcript vector generator (stock `CircuitTranscript` event dump) as the oracle, and make Quint replay a second check.

4. Command runnability, the `RAYON_NUM_THREADS=1 ./loki-harness/run.sh 10 14 17 20` form in tasks 3.1 and 3.4 and the determinism scenario: `run.sh` wraps the gate in `timeout --kill-after=3s 292s` (`loki-harness/run.sh`, last line). `loki-harness/README.md` states the k20 test is `#[ignore]` and that the default gate budget is 292 seconds including compilation. k20 proving plus stock proving plus compilation will very likely exceed 292 s. Suggested fix: state a separate timeout or invocation for k20 (for example, the `--ignored` test run with `--test-threads=1`, which has no timeout wrapper), and record the expected runtime.

5. Scenario "Bidirectional proof verification" command: `RAYON_NUM_THREADS=1 cargo test --locked --manifest-path loki-harness/Cargo.toml --test differential -- --test-threads=1` omits `--release`. The harness README uses `--release` for every proof run. Without it, k17 and k20 are impractically slow. Suggested fix: add `--release` to match `loki-harness/README.md:21-30`.

6. Citation `loki-harness/tests/differential.rs:14-35` (used in several requirements) and `:1-35` (design.md Context): the file is 32 lines long (`wc -l`). Correct range is `1-32`; the test cases are at `differential.rs:3-27` (macro `cases!` plus the k10/k14/k17 invocations at lines 19-21 and the ignored k20 at 27-31).

7. Citation `loki-harness/src/lib.rs:22-78` in the manifest and fixture requirements: lines 22-78 hold `Fault`, `Metrics`, `Sample` and `Difference`. None of this is a manifest schema. The citation implies existing schema code that does not exist. Suggested fix: cite `lib.rs:22-78` as "existing result structs that the manifest row extends", or drop the citation.

8. Citation `loki-harness/src/backend.rs:150-195` for "negative controls" in the negative-control requirement: lines 150-195 are the circuit synthesis with fault injection (`if row == 1 && self.fault == Fault::Lookup`). The actual negative controls are at `backend.rs:306-324` (`negative_controls`). Also `lib.rs:116-146` is correct for the wrong-input/empty/trailing checks. Suggested fix: cite `backend.rs:306-324` for MockProver controls.

9. Coverage gap in the WASM/mobile requirement: "Required targets are WASM browser and Node in single-threaded and wasm-threads modes, aarch64-apple-ios, iOS simulator, and aarch64-linux-android." The scenario does not distinguish `aarch64-apple-ios` (device) from `aarch64-apple-ios-sim` (simulator). Astra's REVIEW of luna-v1-4's mobile proposal raised the same triple distinction. Suggested fix: list both triples and require a runtime on each.

10. Citation accuracy for the stock PK API: the spec cites `midnight-proofs-0.8.3/src/plonk/mod.rs:363-431`. The stock file is 580 lines. `ProvingKey` itself starts at line 338, its `bytes_length`, `write` and `read` methods are at lines 360, 383 and 405. The cited range is close, but it starts after `bytes_length` on line 360. Suggested fix: cite `:360-431`.

11. Spec Requirement "Stock 0.8.1 SHALL have an independent compatibility lane" and task 1.2 require locked, separate workspaces. Stock 0.8.1 source is available from `https://static.crates.io/crates/midnight-proofs/midnight-proofs-0.8.1.crate` (HTTP 200 in my check), and I extracted it read-only under the scratchpad. The design's Risks section says 0.8.1 is absent from the local registry, which is correct for `~/.cargo/registry`, but the crate is fetchable. Suggested fix: name the crates.io URL as the acquisition path, and note that its midnight-curves pin (`0.3.1`) matches 0.8.3, so curves resolution is not a matrix variable.

12. Requirement "Negative controls SHALL classify invalid proofs and keys" cites `docs/v1-research/luna-v1-2/compat-proof.md` as the stock version source diff. That file exists, but the InvalidInstances diff is at stock `src/plonk/verifier.rs` (0.8.1 vs 0.8.2), which the spec should cite directly as a source reference rather than a research note. Minor.

13. Spec Requirement "T04 SHALL include k20 and bidirectional negative controls" scenario uses "malformed-key" controls but the verification command is a placeholder ("the dedicated stock 0.8.1 k20 command"). It is not runnable. Suggested fix: name the command and the workspace path (task 3.3 names it only as "pinned 0.8.1 and 0.8.3 workspace commands").

Positive notes: the spec correctly keeps the T03 mixed multi-proof case as an expected stock rejection and forbids documenting it as supported. The lookup-determinism split (no-lookup bytes mandatory, lookup bytes diagnostic) matches the existing `loki-harness/README.md:31-60` policy. The PK API cited in the spec (`ProvingKey::{bytes_length,write,read}`) exists in stock 0.8.3.

## Unverified in this review

- I did not run any harness case, `cargo test` or build. Command runnability is inferred from scripts and README text.
- k20 runtime was not measured; finding 4 is an estimate based on the 292 s wrapper budget.
