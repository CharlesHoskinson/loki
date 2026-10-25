## Purpose

Defines when Loki can promote a candidate to v1.0.0 and which traceable evidence the coordinator must have before claiming the release or the planning lab is complete.

## ADDED Requirements

### Requirement: Release registry is exhaustive
The release manifest SHALL enumerate every active v1 change, every in-scope roadmap feature, every open baseline TODO and every named unmerged workstream with its owner and accepted implementation gate or reasoned deferral.

#### Scenario: Missing scope row prevents qualification
- **WHEN** the proposed `python3 scripts/release/check-coverage.py --roadmap openspec/ROADMAP-V1.md --changes openspec/changes --todo TODO.md` audits the release inventory
- **THEN** it reports zero missing or duplicate mappings for T01–T92 and p3/p4/p5/p7/w10/W12, and exactly the on-disk active v1 change set; a missing feature, unknown ID or unreasoned deferral exits nonzero. Sources: `TODO.md:5`, `deps.yaml:11`, `docs/ARCHITECTURE.md:343`.

### Requirement: Planning completion requires independent review
The planning lab SHALL be declared complete only after all ten area authors ACK the current roadmap, all ten authors and ten scouts post DONE, and all OpenSpec changes pass strict validation with no unresolved review blockers.

#### Scenario: Supervisor closes the lab
- **WHEN** the supervisor evaluates the current roadmap revision and bus review ledger
- **THEN** `openspec validate --all --strict` exits zero, the ledger contains ten current author ACKs and twenty DONEs, every blocker has a resolution, and the Status section identifies planning completion separately from implementation acceptance. Verification: strict validation and `agentbus log` review-ledger audit. Sources: `docs/MERGE-GATES.md:11`, `TODO.md:5`.

### Requirement: Milestone promotion fails closed
The release coordinator SHALL promote through M0–M6 only when every exit gate of the milestone in ROADMAP-V1.md has complete passing evidence; absent runners, unknown budgets, unresolved findings and unexecuted commands SHALL not count as passes.

#### Scenario: A candidate lacks a required result
- **WHEN** proposed `python3 scripts/release/check-manifest.py --milestone M4 --manifest <path>` encounters a missing required model replay or proof audit
- **THEN** it exits nonzero naming the gate and owner and leaves M4 incomplete; the same rule applies to each milestone. Sources: `LOKI-DONE.md:3`, `docs/MERGE-GATES.md:11`.

### Requirement: Stock replacement compatibility is mandatory
Every shipped configuration SHALL preserve the stock 0.8.3 API, feature/default and wire compatibility contracts applicable to that configuration; additive Loki behavior SHALL require opt-in, and the required patch artifact SHALL resolve unchanged exact 0.8.3 and caret 0.8 consumers.

#### Scenario: Release compatibility evidence is complete
- **WHEN** the candidate passes the compat-api and compat-proof changes' named gates
- **THEN** mechanical API/feature results, Rust 1.76 consumer checks, both directions of proof/VK/PK/parameter interchange, transcript checks and negative controls are present; inherited rejected shapes retain the stock outcome. Verification: compatibility owner commands linked by proposed release manifest checker. Sources: `Cargo.toml:13`, `Cargo.toml:45`, `README.md:81`, `loki-harness/README.md:3`.

### Requirement: Required CPU platforms execute their compatibility gates
Release qualification SHALL include browser and Node single-thread and opt-in threaded WASM, iOS device/simulator and Android CPU evidence with stock and Loki execution, target-specific runtime results, and measured numeric memory, size and time budgets.

#### Scenario: Compilation cannot substitute runtime acceptance
- **WHEN** proposed `python3 scripts/release/check-platforms.py --manifest <path>` evaluates a candidate with successful cross-compiles but no Android runtime result
- **THEN** it fails Android qualification and the release; successful qualification requires all platform-owned runtime gates, physical-device budget evidence and frozen workload/runner limits, with no CUDA prerequisite. Sources: `docs/PLATFORMS.md:406`, `TODO.md:271`, `TODO.md:295`.

### Requirement: Merge evidence and native release evidence remain distinct
CPU-only merge acceptance SHALL not require a GPU receipt, while a release containing CUDA acceleration SHALL require passing native receipts for the integrated source revision in addition to MERGE-CPU.

#### Scenario: Branch receipt is stale after integration
- **WHEN** `scripts/verify-main.sh --tier cpu` passes after a merge but the only native receipt names a pre-merge source revision
- **THEN** merge acceptance can stand, but proposed `python3 scripts/release/check-manifest.py --manifest <path>` fails GPU release qualification until `scripts/verify-main.sh --tier native` and applicable package receipt commands pass on the integrated source. Sources: `docs/MERGE-GATES.md:8`, `docs/MERGE-GATES.md:9`, `deps.yaml:17`.

### Requirement: Shipped features have complete QA and documentation coverage
Every shipped feature SHALL map to a measurable acceptance requirement, stock compatibility evidence, differential fuzz coverage, applicable Quint/real-Rust replay, Lean contract/status and user/module documentation; inapplicable methods SHALL have a specific reviewed reason.

#### Scenario: Feature coverage audit detects a gap
- **WHEN** proposed `python3 scripts/release/check-coverage.py --manifest <path>` evaluates a new scheduling feature with a Quint model but no current Rust replay
- **THEN** it fails that feature's release gate; a CPU-backed fake driver counts only for actual Rust routing/fallback policy, never device arithmetic or asynchronous GPU lifetime. Sources: `LOKI-DONE.md:10`, `LOKI-DONE.md:11`, `quint/mbt/README.md:22`.

### Requirement: Formal qualification discloses trust and closes proof holes
The release Lean tree SHALL build without proof holes or new custom axioms, bind citations to the source manifest, and state external assumptions separately from proved component obligations; compiling a conditional theorem SHALL not be reported as proving its Rust correspondence premises.

#### Scenario: GPU specification introduces an unfinished proof
- **WHEN** the GPU-spec merge introduces `sorry`, `admit` or a transitive `sorryAx` dependency
- **THEN** the Lean owner's build/trust audit fails M4 until the proof hole is closed; citation drift and unresolved shipped-feature obligations also remain blockers. Verification: the Lean change's full-build, axiom-audit and release-drift commands plus manifest audit. Sources: `TODO.md:39`, `TODO.md:63`, `TODO.md:67`, `docs/ARCHITECTURE.md:57`.

#### Scenario: Historical Lean pins disagree
- **WHEN** proposed `python3 scripts/release/check-manifest.py --milestone M4 --manifest <path>` compares the merged Lean source pin, release drift baseline and proof receipts to `LOKI_RELEASE_SHA`
- **THEN** any mismatch or missing reviewed citation, full-build, coverage or axiom report fails M4; changing only `SYNCED-LOKI-COMMIT` cannot pass. Stock 0.8.3 retains a separately pinned oracle identity. Verification: manifest comparison plus the Lean change's release drift/build/trust gates. Sources: `/home/charles/repos/loki-lean/SYNCED-LOKI-COMMIT:1`, `/home/charles/repos/loki-lean-wt/gpu-spec/SYNCED-LOKI-COMMIT:1`, `/home/charles/repos/loki-lean/scripts/drift.py:41-45`.

#### Scenario: Deferred P6 shares a module with required P3 contracts
- **WHEN** the release Lean inventory excludes unfinished segmented MSM while P3 remains in scope
- **THEN** the inventory and trust audit reject any retained hole-dependent declaration or missing nonsegmented BackendMsm/P3 contract; split/extract the segmented-only source or prove it, and audit both the full release source and built library. Verification: Lean coverage, full build and no-hole/axiom audit. Source: `/home/charles/repos/loki-lean-wt/gpu-spec/LokiLean/BackendMsm.lean:132-154,270-282`.

### Requirement: Performance closing evidence measures the complete chain
Release qualification SHALL include the perf-owned repeated matched stock 0.8.3/Loki complete Sherlock chain experiment and a published head-to-head table, with independent stock acceptance, raw samples, dispersion and all CPU phases; target resource budgets SHALL independently pass.

#### Scenario: Only historical phase data exists
- **WHEN** proposed `python3 scripts/validate-perf-publication.py wiki-llm/loki/benchmark/RESULTS.md` finds only a single parent timing or isolated kernel measurements
- **THEN** it fails the closing condition until complete leaf-parent-decider results and reproducible evidence are published; unsupported target speedup claims cannot replace measured target budgets. Verification: the named perf publication check and owner result manifest. Sources: `TODO.md:27`, `README.md:56`, `docs/GPU.md:431`.

### Requirement: Release evidence is revision-bound and reproducible
The release manifest SHALL record immutable Loki, GPU, Lean, stock, fixture and toolchain identities, gate commands, exit status and artifact digests; any source or dependency change SHALL invalidate affected receipts before release promotion.

#### Scenario: Evidence is copied from a different revision
- **WHEN** proposed `python3 scripts/release/check-manifest.py --manifest <path>` compares a gate receipt's source/dependency digests to the release manifest
- **THEN** a mismatch fails qualification and identifies the required rerun; historical evidence can remain labeled history but cannot satisfy current acceptance. Sources: `docs/MERGE-GATES.md:16`, `TODO.md:167`, `TODO.md:39`.

### Requirement: Unresolved correctness and security findings block release
The coordinator SHALL reject release promotion for untriaged crashes, invalid-proof acceptance, memory-safety findings, unclosed required formal/model failures, or unsupported compatibility claims, including findings shared with stock.

#### Scenario: Stock shares a security finding
- **WHEN** the fuzz/security ledger records invalid-proof acceptance or memory corruption in both stock and Loki
- **THEN** proposed `python3 scripts/release/check-findings.py --manifest <path>` fails release qualification and records an assigned disposition owner; matching stock behavior alone cannot close the finding. Sources: `LOKI-DONE.md:9`, `TODO.md:35`, `LOKI-FUZZ.md:178`.

### Requirement: Distribution claims preserve the compatible artifact
The v1.0.0 project release SHALL retain a compatible git/path patch artifact and reviewed installation/rollback documentation; an additional registry publication SHALL require an owner-selected identity and archive qualification without replacing the stock-compatible artifact.

#### Scenario: Registry identity remains undecided
- **WHEN** docs-release has qualified the git/path `midnight-proofs` 0.8.3 artifact but no new registry identity is approved
- **THEN** release notes state that distribution mode, do not claim an upload of the already published name/version, and leave additional registry publication unexecuted. Verification: docs-release clean-consumer, archive/distribution and release-checklist audits. Sources: `Cargo.toml:15`, `README.md:81`, `TODO.md:7`.
