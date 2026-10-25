# Docs and release inventory

Baseline: `/home/charles/repos/loki-wt/v1` checkout on 2026-10-25. This branch contains docs currently referencing Loki main `a256a9a`; haiku's inventory also examined the sibling source main at `eaf09d8`. No build, test, proof, GPU or target execution was run for this inventory.

## Current documents and release facts

- Root README documents identity and compatibility scope (`README.md:19-23`), recorded W2 k10/k14/k17 status and unrun k20/0.8.1 (`README.md:38-42`), a feature matrix (`README.md:94-127`), patch-based install with `midnight-proofs = "0.8"` (`README.md:81-92`), cross-verification command (`README.md:168-178`), QA evidence (`README.md:180-191`), and the documentation map (`README.md:209-220`). It explicitly says mobile/WASM/Apple are TODO (`README.md:65-69`).
- Architecture/GPU/platform/module/identity docs exist: `docs/ARCHITECTURE.md`, `docs/GPU.md`, `docs/PLATFORMS.md`, `docs/modules/README.md`, `docs/brand/IDENTITY.md`. The module index lists source-backed pages (`docs/modules/README.md:30-58`).
- Root Rust crate denies broken intra-doc links and missing docs (`src/lib.rs:6-9`). Feature docs.rs metadata enables all features (`Cargo.toml:36-43`), but `loki-gpu-backend` is incompatible with legacy `cuda`/`cuda-ntt` features (`src/lib.rs:11-18`); all-features is therefore not a valid CI doc command.
- Current manifest says package `midnight-proofs` version `0.8.3`, edition 2021, MSRV 1.76 and `MIT OR Apache-2.0` (`Cargo.toml:12-16,33-38`). README patch docs preserve the `0.8` requirement (`README.md:81-92`). A product `v1.0.0` tag and crates.io package version are separate decisions. Exact upload of already-published `midnight-proofs 0.8.3` is unavailable; any registry plan must resolve a compatible 0.8.x version or a new package identity. The local README says the local patch form is tested, while standalone consumer validation is open (`README.md:90-92`; `TODO.md:11-13`).
- `CHANGELOG.md` uses Keep a Changelog and SemVer headings but the Unreleased sections are empty (`CHANGELOG.md:1-15`).
- Dual licensing is stated at `COPYING.md:1-15` and manifest `Cargo.toml:33-34`. Haiku found the sibling `loki-gpu` crate is Apache-only and has its own LICENSE/NOTICE. A provenance register/notice audit is open as `TODO.md:159-161` and platform-kernel notices as `TODO.md:371-373`.
- Release and CI gaps from scout inventory: no root `.github/` release workflow, `SECURITY.md`, cargo-deny config, cargo-audit config, MSRV job, SBOM workflow, or release checklist were found. Sibling loki-gpu workflow is under `loki-gpu/.github/workflows/rust.yml`, so it does not serve as the root crate workflow. `docs/MERGE-GATES.md` distinguishes merge CI from release receipt gates.
- WASM/mobile docs are plans, not evidence of support. Mobile CPU/device measurement and thermal gates are open (`TODO.md:271-288`); WASM browser proof and compatibility work is open (`TODO.md:295-309`). Browser threading requires isolation headers and its own lane (`docs/PLATFORMS.md:318-327`). Target areas own numerical budgets; docs-release integrates the release gate.
- Security: T08 is open for witness-dependent branching in `batch_invert_rational` (`TODO.md:35-37`), called out in the review ledger (`docs/review/REVIEW.md:45`). No constant-time claim should be made until a deployment-specific review is recorded. Crate-level unsafe code is denied except in feature-specific CUDA files; that is not a side-channel review.
- Release follow-ups assigned by astra: T01 canonical URLs/revision policy (`TODO.md:7-9`); T05 exact-target novelty only if claimed (`TODO.md:23-25`); T08 side-channel review; T38 lockfile reproducibility (`TODO.md:155-157`); T39 clean-room backend provenance (`TODO.md:159-161`); T40 narrower novelty comparison (`TODO.md:163-165`); T41 portable benchmark evidence (`TODO.md:167-169`); T92 kernel notices (`TODO.md:371-373`). T93 is already complete (`TODO.md:375-378`).

## Outside practice sources

- Cargo SemVer and MSRV: https://doc.rust-lang.org/cargo/reference/semver.html
- Cargo publishing/packaging: https://doc.rust-lang.org/cargo/reference/publishing.html and https://doc.rust-lang.org/cargo/commands/cargo-publish.html
- Cargo manifest/package metadata: https://doc.rust-lang.org/cargo/reference/manifest.html
- rustdoc: https://doc.rust-lang.org/rustdoc/ and Rust API documentation guidance: https://rust-lang.github.io/api-guidelines/documentation.html
- cargo-deny (advisories, bans, licenses, sources): https://github.com/EmbarkStudios/cargo-deny
- cargo-audit: https://github.com/RustSec/rustsec/tree/main/cargo-audit
- CycloneDX Rust Cargo SBOM generator: https://github.com/CycloneDX/cyclonedx-rust-cargo
- SLSA provenance concepts: https://slsa.dev/spec/v1.0/provenance
- Keep a Changelog 1.1.0: https://keepachangelog.com/en/1.1.0/
- SemVer 2.0.0: https://semver.org/spec/v2.0.0.html

## Roadmap R1 and release identity decision

Astra’s R1 decision resolves the package conflict: product `v1.0.0` tag with required `midnight-proofs 0.8.3` git/path patch artifact; no overwrite or registry upload. This artifact is meant to support both exact `=0.8.3` and caret `^0.8` patch consumers. Any other registry identity is optional, separately decided and cannot block the required patch route. Release governance and design artifacts now match this.

ACKed roadmap R1 with two detail requests: explicitly replace docs.rs `all-features=true` because root `src/lib.rs:13-18` rejects the GPU feature combination; name `SECURITY.md` and threat-model output in milestone exit gates. The docs-release tasks cover both.

Haiku review of first draft found: docs.rs all-features conflict; missing SECURITY/threat-model task; missing release-preflight bootstrap ordering; missing explicit iOS device/simulator targets; missing mobile PK/verifier-parameter interchange; missing stock verifier compiled to WASM; omitted T37/T72/T73/T75/T76 dispositions; and upstream 0.8.x series collision risk. These were incorporated into the revised specs/tasks/design and strict validation was rerun successfully. A follow-up draft review is pending.

A further roadmap review from WASM owner identified that T80 is a CPU WASM chain workload, not WebGPU, and T89 is the matched WASM/native BLST baseline. The release documentation contract now requires both results (or explicit per-lane CPU chain timeout/unsupported workload) and binds them to platform/perf owner artifacts. Requested R1 ledger correction: T80 and T89 are required and co-owned by WASM/perf.
