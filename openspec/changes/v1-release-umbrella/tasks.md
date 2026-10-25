# Tasks

These are future release implementation/qualification tasks, not claims that the planning lab ran production gates. Commands named in the spec that do not exist yet are deliverables below.

## 1. Scope and evidence contracts

- [ ] 1.1 Freeze the final roadmap change registry and T01–T92/branch dispositions; verify set equality against active `openspec/changes/v1-*`, TODO.md and deps.yaml, with no unmapped row.
- [ ] 1.2 Capture ten current author ACKs and twenty author/scout DONEs with resolved review findings; verify the bus ledger and `openspec validate --all --strict` exit zero, and document planning completion separately from implementation status.
- [ ] 1.3 Implement `scripts/release/check-coverage.py` and the feature/QA/docs ledger; verify missing change, feature, TODO, verifier or actual-source MBT rows fail with a named owner, and document the schema/commands.
- [ ] 1.4 Implement the immutable source/evidence manifest and `scripts/release/check-manifest.py`; verify wrong SHA/digest, missing receipt and unrun gate fixtures fail, complete fixtures pass, and document source/evidence commit separation.

## 2. Compatibility and platform qualification

- [ ] 2.1 Collect compat-api/compat-proof results for stock 0.8.3 API/defaults/MSRV, exact/caret downstream consumers and bidirectional proof/key/transcript cases; verify all owner commands pass and expected stock-rejection rows are documented.
- [ ] 2.2 Implement `scripts/release/check-platforms.py` over platform-owned results; verify a missing browser/Node thread mode or iOS/Android runtime receipt fails, with physical-device budgets and resource envelopes linked in platform docs.
- [ ] 2.3 Freeze target runner, fixture and numeric memory/size/time budgets from baseline measurements before qualification; verify every required platform has measured values and an executable comparison rule, including physical thermal evidence.

## 3. Integration and complementary QA

- [ ] 3.1 Reconcile the current branch DAG with GPU integration owners and collect MERGE-CPU results after each merge; verify P4/P3/W10/P5/P7 order and fake-runtime ports against deps.yaml, documenting merge versus release gates.
- [ ] 3.2 Collect native receipts at the final integrated SHA for all shipped CUDA paths, including arithmetic equality, positive dispatch, compute-sanitizer, resource limits, transactional failure and cancellation; verify stale branch receipts fail manifest validation.
- [ ] 3.3 Audit each shipped feature's differential fuzz corpus/campaign, current-source Quint model/replay, Lean contract/status and docs links; verify no required method is replaced by another method and every N/A has a specific reviewed justification.
- [ ] 3.4 Collect the release Lean full-build, no-hole/no-new-custom-axiom audit, source-drift report and required obligation closure; verify introduced gpu-spec holes and unreviewed citation drift prevent M4 promotion. Require manifest Loki SHA equality with SYNCED-LOKI-COMMIT and MAIN_REV/release drift plus fresh receipts; retain nonsegmented BackendMsm/P3 contracts when extracting deferred segmented P6 source and all transitive hole-dependent declarations.
- [ ] 3.5 Implement `scripts/release/check-findings.py` with shared-stock security findings still blocking; verify invalid-proof acceptance, memory safety, untriaged crashes and unresolved required model/proof defects fail the release gate, and document disposition ownership.

## 4. Performance and distribution evidence

- [ ] 4.1 Collect perf-owned repeated stock/Loki complete-chain results and publication validation; verify canonical RESULTS.md links raw data, source/toolchain/fixture pins, sample dispersion and independent stock acceptance, with required target budget results linked separately.
- [ ] 4.2 Collect docs-release security/provenance/license/SBOM/audit and documentation evidence; verify every shipped feature and actual target limitation is described, including valid rustdoc feature combinations.
- [ ] 4.3 Qualify the compatible git/path artifact and document installation/rollback; verify unchanged `=0.8.3` and `^0.8` consumers work. If an additional registry identity is selected, collect its separate archive checks without claiming replacement of an existing crates.io version.

## 5. Final integration acceptance

- [ ] 5.1 Evaluate M0–M6 against one immutable manifest and rerun every invalidated receipt; verify no missing runner, unknown budget, stale artifact or unresolved blocker is interpreted as passing.
- [ ] 5.2 Run `openspec validate --all --strict`, final scope/coverage checks and owner review of the release evidence; verify zero validation failures and a documented publication decision before marking implementation complete.
