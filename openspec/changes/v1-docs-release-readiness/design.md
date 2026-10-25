# Design

## Context

See `proposal.md` for motivation. The repository has extensive markdown, an independent stock harness, and separate merge/native receipt gates, but no root release workflow. `README.md:38-42,168-191` restricts compatibility claims to recorded cases; `docs/MERGE-GATES.md:5-20` ties native receipts to an integrated SHA. The current Cargo manifest is `midnight-proofs 0.8.3`, MSRV 1.76, with dual license metadata (`Cargo.toml:12-16,33-38`). Browser/mobile work remains TODO (`docs/PLATFORMS.md:264-327`).

## Goals / Non-Goals

**Goals:**

- Aggregate owner-owned checks into release CI without redefining their proof, API, target, fuzz, Quint, Lean or performance semantics.
- Keep evidence immutable and bound to one release source SHA, feature set, lockfile and named runner.
- Fail closed on missing platform numbers, T08 review, package-resolution decision, provenance, license result or SBOM.
- Keep all Loki-only behavior opt-in and keep CPU WASM/mobile gates independent of accelerators.

**Non-Goals:**

- Implement production Rust code, a new backend, a JS/FFI API or an accelerator.
- Treat rustdoc generation as proof of API compatibility or treat a passing stock verifier as security review.
- Invent memory, size, runtime or performance thresholds before platform/perf owners measure them.

## Decisions

### Aggregate, do not duplicate, component gates

A root `release-preflight` command and root CI workflow consume named outputs from compat-api, compat-proof, platform, fuzz, Quint, Lean, perf, GPU and security owners. Component changes continue to own their tests, vectors and acceptance thresholds. The release report records each command, source SHA, toolchain, features, result path and owner. This matches the existing split between `MERGE-CPU` and post-merge `RECEIPT-NATIVE` in `docs/MERGE-GATES.md:5-20`; the release SHA is the integration key.

Alternatives considered: place all tests in one mega-script, which would duplicate contracts and make platform jobs hard to isolate; or leave every result in disconnected packages, which cannot establish an exact release candidate. The aggregator is only a completeness and provenance gate.

### Separate project tag from package version

The Git/project release uses the tag `v1.0.0`; the mandatory Cargo artifact retains package identity `midnight-proofs 0.8.3` (`Cargo.toml:12-16`). A git/path patch with that version satisfies both exact `=0.8.3` and caret `^0.8` consumers in the supported patch route. The upstream registry already owns `midnight-proofs 0.8.3`, so no overwrite or registry upload is part of this release. Any additional registry package is optional, separately owner-decided, and cannot replace or block the git/path compatibility artifact. The release report records the patch consumer result and the registry non-publication decision.

Alternative: publish Cargo `1.0.0` under the existing name and rely on a patch declaration. This fails for downstream requirements constrained to `^0.8` and violates the drop-in promise. Another alternative is publishing a different registry identity, which is outside the mandatory release artifact and needs its own owner decision and migration story.

### Verify docs on valid feature configurations

Run rustdoc with named, mutually compatible feature sets. Do not use `--all-features` for the root crate because `loki-gpu-backend` cannot be combined with legacy `cuda` or `cuda-ntt` (`src/lib.rs:11-18`). Stable CPU rustdoc and docs tests use the documented CPU feature set; each CUDA/backend combination has a separate owner-valid job. Existing `#![deny(missing_docs)]` and broken-link lint (`src/lib.rs:6-9`) enforce compiler-level coverage, while the module index and coverage manifest ensure the human docs are navigable (`docs/modules/README.md:30-58`).

### Treat WASM and mobile as required CPU release lanes

WASM has separate browser and Node lanes, single-threaded by default and opt-in wasm-threads under the owner’s isolation requirements. iOS and Android lanes use target-owner runtimes and device/emulator evidence. The release overview links exact job output; it does not infer support from a compile-only cross-check or from another platform. Numeric size, memory and time ceilings are owner measured and frozen before release. The release docs include a WASM CPU chain workload result or a per-lane hard timeout/unsupported size, plus a matched native/WASM BLST baseline with SIMD128/thread variants. Mobile documents the physical CPU leaf baseline and ten-proof thermal soak. WebGPU, Metal and Vulkan acceleration do not gate CPU builds and remain excluded unless another change supplies its own proof and compatibility gates.

### Security, source provenance and dependency inventory are independent checks

T08 review must analyze `batch_invert_rational`’s denominator-equals-one branch (`src/poly/mod.rs:290-317`) against a named deployment threat model. No constant-time claim is emitted before signed review and mitigation closure. Provenance records source boundaries, revisions and licenses for root and backend crates, including required notices for future reused kernels; current follow-ups are `TODO.md:159-161,371-373`. The locked dependency graph is checked for advisories, licenses, sources and bans, then emitted as a CycloneDX SBOM and tied to the exact source and lock digests. This separates legal/provenance evidence from arithmetic correctness.

### Use a deterministic completeness check plus retained outputs

The future release-preflight command checks required files and result metadata, while the workflow retains logs, SBOM, package archive checksum and owner artifacts. It fails on missing numeric budgets and missing owner outputs rather than treating TODOs or estimates as a pass. This keeps evidence reviewable and reproducible without hard-coding invented cross-device thresholds.

Relevant practice references: Cargo [SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html), [publishing](https://doc.rust-lang.org/cargo/reference/publishing.html), and [manifest metadata](https://doc.rust-lang.org/cargo/reference/manifest.html); [rustdoc](https://doc.rust-lang.org/rustdoc/); [cargo-deny](https://github.com/EmbarkStudios/cargo-deny), [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit), [CycloneDX Rust Cargo](https://github.com/CycloneDX/cyclonedx-rust-cargo), and [SLSA provenance](https://slsa.dev/spec/v1.0/provenance).

## Risks / Trade-offs

- [Optional registry identity conflicts with upstream package] → do not upload the required `0.8.3` artifact; any separately named/numbered package requires explicit ownership and must not become the only compatibility route. A fork published in the upstream `0.8.x` series can conflict with later upstream releases.
- [Platform runner/resource variation] → publish named runner measurements and freeze a separate numeric limit per target; missing values fail closed.
- [T08 identifies a timing leak] → make remediation or an explicitly narrowed security claim a release decision; do not waive the review.
- [Third-party license/provenance gap] → block distribution of the affected optional component until its notices and source boundary are resolved; do not silently remove upstream attribution.
- [Feature combinations fail documentation build] → use per-feature valid configurations and avoid docs.rs all-features as a substitute for the release matrix.
- [Historical artifacts are not portable] → mark them unavailable and keep any related performance or novelty claim out of release claims.

## Migration Plan

1. Freeze the project tag `v1.0.0`, required Cargo package `midnight-proofs 0.8.3`, stock oracle and release SHA.
2. Close owner changes and produce compatibility, platform, fuzz, Quint, Lean, performance and security evidence for the integrated SHA.
3. Run docs, MSRV, package, supply-chain and root release-preflight jobs; update user docs, changelog, provenance and release checklist from passing outputs.
4. Create and inspect the release archive and SBOM. Distribute the required artifact through the compatible git/path patch route; make no crates.io upload. Tag the product release `v1.0.0`.
5. If any required receipt fails, do not publish; fix the source/evidence and rerun against the new SHA.

## Open Questions

- Platform and performance owners must supply named runners and numeric budgets; this design intentionally defines no unmeasured thresholds.
