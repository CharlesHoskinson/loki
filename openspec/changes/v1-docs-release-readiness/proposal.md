# Proposal

## Why

Loki has detailed technical documentation and several compatibility gates, but target support, package release identity, security review and root release automation are not yet closed. The v1 release needs a single evidence-based contract that preserves the stock 0.8.3 consumer experience and makes missing evidence a visible release blocker.

## What Changes

- **Must-have documentation:** keep README, architecture, GPU, platform, module and identity pages aligned with tested behavior; build rustdoc for supported feature combinations; publish a feature-to-documentation-and-QA coverage map.
- **Must-have performance documentation:** publish the complete Sherlock leaf-parent-decider stock-versus-Loki table with owner-provided fixtures, raw observations, per-run limits and reproducible methodology; preserve the boundary between kernel/phase results and end-to-end claims.
- **Must-have platform release documentation:** record browser and Node WASM CPU support in single-threaded and wasm-threads lanes, plus iOS and Android CPU support. Each row links to its owner’s compile/runtime compatibility job, named runner, measured memory/time/size budgets, CPU recursion-chain outcome or hard timeout/unsupported workload, matched BLST native/WASM baseline, and limitations. Mobile evidence includes a physical leaf baseline, approved client-circuit/data boundary and ten-proof thermal soak. Unknown budgets block release until frozen from measurements.
- **Must-have compatibility claims:** document the exact stock `midnight-proofs` 0.8.3 fixtures and commands, two-way acceptance and byte-level serialization results, the inherited mixed-instance multi-proof rejection, lookup-order nondeterminism, and all unsupported shapes. Never expand a result beyond its archived gate.
- **Must-have release engineering:** define product tag versus Cargo package version, SemVer/MSRV policy, changelog entries, clean patched-consumer packaging, root release CI and release checklist. The mandatory artifact keeps package `midnight-proofs 0.8.3` and is distributed as a git/path patch; it is not uploaded over the existing crates.io version. Any additional registry identity is optional, separately owner-decided, and cannot block the compatible git/path release.
- **Must-have security and supply chain:** close T08 with a deployment-specific side-channel review before making any constant-time claim; publish threat model, disclosure route, clean-room provenance and attribution; run dependency advisory/license/source checks and emit an SBOM tied to the release revision.
- **Nice-to-have:** external hosted fuzz-service enrollment, automated per-target size dashboards, broader consumer cookbook, and comparative platform performance dashboards beyond the required release evidence.

## Out of Scope

- Implementing new proof-system behavior, accelerators, WebGPU, Metal or Vulkan. These are owned by GPU/platform changes; CPU-only WASM and mobile release gates must not depend on GPU.
- Changing stock 0.8.3 public API, transcript, serialization, defaults or proof semantics. A protocol fix to the inherited mixed-instance rejection requires a separate compatibility decision.
- Claiming universal byte equality, universal API-shape support, constant-time behavior, formal refinement of deployed code, or performance wins without the corresponding owner evidence.
- Uploading the existing package/version over upstream crates.io `midnight-proofs 0.8.3`. The project `v1.0.0` tag is a product milestone; the compatible `0.8.3` crate remains the required git/path patch artifact. Any other registry distribution is optional and separately decided.

## Compatibility, platforms, QA and documentation

The release retains Cargo package name/version `midnight-proofs 0.8.3` and the stock-compatible API/default feature contract. The mandatory release artifact is a git/path patch, which supports both exact `=0.8.3` and caret `^0.8` consumers; no crates.io overwrite is attempted. The release workflow consumes compat-api’s mechanical public-surface/feature/MSRV/downstream-patch gate and compat-proof’s independent unpatched 0.8.3 two-way proof/VK/PK serialization and transcript fixtures. It records which checks compare acceptance and which require byte equality; a randomized HashMap lookup order is not described as proof-byte equality. No Loki-only behavior is enabled by default.

WASM browser and Node lanes, with single-threaded and opt-in wasm-threads variants, and iOS/Android CPU lanes are release-critical. Platform areas own their target builds, runtime checks, devices, and numeric budgets. This change owns the aggregate release workflow and the README/platform documentation that links to their artifacts. WebGPU, Metal and Vulkan acceleration remain explicitly deferred unless the GPU/platform owners separately close them.

Each feature’s coverage row names its README or module page, differential fuzz command/corpus, Quint model and Rust MBT replay, Lean contract and exact-release SHA status, and compatibility command. Fuzz, Quint, Lean, perf and platform areas own their semantic gates; release CI aggregates their required outputs and fails on missing/stale evidence. Rustdoc must run with supported feature combinations because `loki-gpu-backend` is mutually exclusive with legacy CUDA features.

## Risks and dependencies

- **Risks:** the project release tag and crates.io package identity may be conflated; upstream already owns `midnight-proofs 0.8.3`, so re-upload is prohibited and any future registry identity is optional. Existing target docs are roadmaps, not proof of working targets. T08 may identify leakage requiring mitigation. License and clean-room claims need source-level provenance and attribution evidence. Missing MSRV and CI evidence can invalidate an otherwise successful stable build.
- **Dependencies:** compat-api owns API/default/MSRV/patch-resolution gates; compat-proof owns shared stock 0.8.3 fixtures and wire gates; WASM/mobile own target CI and measured limits; GPU owns accelerator scope and attribution; fuzz, Quint, Lean and perf own component commands and release-pinned artifacts; release maintainer owns any optional registry decision and credentials. This change owns release workflow aggregation, publication checklist and docs.

## Existing evidence

- README scopes recorded compatibility to k10/k14/k17 and lists open k20/0.8.1 cases (`README.md:38-42,168-178`), keeps the package patch requirement at `0.8` (`README.md:81-92`), and maps primary documentation (`README.md:209-220`).
- The manifest declares `midnight-proofs` `0.8.3`, Rust 1.76, and dual licensing (`Cargo.toml:12-16,33-38`); the Unreleased changelog is empty (`CHANGELOG.md:8-13`).
- Public docs and module map exist (`docs/modules/README.md:30-58`); Rust denies missing docs and broken intra-doc links (`src/lib.rs:6-9`). Mutually exclusive GPU feature sets are explicit (`src/lib.rs:11-18`).
- Mobile and WASM are currently TODO (`docs/PLATFORMS.md:264-327`); mobile measurement T67/T71 and browser compatibility T74 remain open (`TODO.md:271-309`).
- T08, provenance, release reproducibility and portability follow-ups are open (`TODO.md:35-37,155-169`); platform notice audit T92 is open (`TODO.md:371-373`).

## Capabilities

### New Capabilities

- `release-documentation`: evidence-bounded docs, rustdoc coverage, cross-target documentation and per-feature QA/doc traceability.
- `release-governance`: package identity/SemVer/MSRV policy, changelog, CI, release evidence and publication checklist.
- `security-supply-chain`: threat model and T08 review, license/provenance, dependency policy and release SBOM.

### Modified Capabilities

None.

## Impact

Planning artifacts only in this change. Later implementation will add or update root release CI/workflow, scripts, `SECURITY.md`, provenance/SBOM artifacts, release checklist, CHANGELOG and user-facing docs. No production Rust API or proof behavior changes are authorized by this proposal. Proposed required checks are future acceptance gates and have not been run.
