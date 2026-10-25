# haiku-v1-10 inventory: docs-release (current state, Loki main eaf09d8)

Scope: ~/repos/loki-proofs (branch main, eaf09d8). Unverified items are marked. No builds run.

## Documentation present
- README.md (261 lines). Sections 1-8. Feature table at README.md:94-127; "Build without CUDA" at :129. Status "Intel, AMD, mobile, WASM and Apple remain TODO" at README.md:68. Licence section 7 at README.md:240.
- docs/ARCHITECTURE.md (360 lines). Feature flags section 6 at :242. Loki-vs-stock differences section 4 at :124. Correctness section 5 at :169.
- docs/GPU.md (602 lines). Not inspected in depth (T10 scope for GPU docs).
- docs/PLATFORMS.md (468 lines). Every platform is a TODO. WASM section 9 at :318 (9.1 CPU build, wasm-bindgen-rayon, getrandom js, 4 GiB ceiling). Mobile section 8 at :264. Still a roadmap, not a support statement.
- docs/MERGE-GATES.md (24 lines): merge-gate tiers (MERGE-CPU, RECEIPT-NATIVE).
- docs/NOVELTY.md (49 lines), docs/brand/IDENTITY.md (identity: name, metaphor, claim limits).
- docs/modules/: 27 files, 1060 lines total, index README.md. Covers backend, circuit, plonk, poly-*, transcript, utils, dev, profile, benches, examples, fuzz, quint, scripts, tests, tests-stock, loki-gpu, loki-harness, cuda-*, probes.
- docs/diagrams/*.svg (9 files). docs/review/ (REVIEW.md + luna-r1..r10, coordinator-haiku-arch.md).
- Root LOKI-*.md design notes (LOKI-CPU, -FUZZ, -GPU-MSM, -GPU-NTT, -HARDEN, -MULTIOPEN, -P1-SEAM, -PROFILE, -QUINT, -SRS-REGISTRY) and LOKI-DONE.md (definition of done, gates G1-G9), TODO.md (378 lines; open items T01..T29+).
- Rustdoc: src/lib.rs:1 has a crate doc comment. src/lib.rs:6-9 enforce `#![deny(rustdoc::broken_intra_doc_links)]`, `#![deny(missing_docs)]`, `#![deny(missing_debug_implementations)]`, `#![deny(unsafe_code)]`. So missing_docs is a compile error at the crate root for all public items, so rustdoc coverage is enforced by the compiler (current status unverified, since no build was run).
- docs.rs: Cargo.toml.orig:12-13 sets `all-features = true` with `--cfg docsrs`. The crate uses `#![cfg_attr(docsrs, feature(doc_cfg))]` at src/lib.rs:4. Note: docs.rs builds all features, including the cuda ones, so the feature docs appear on docs.rs only if the cfg is applied to items (not checked).

## Release-engineering state
- Version: Cargo.toml.orig:3 and Cargo.toml:16 both say `version = "0.8.3"`, name `midnight-proofs` (Cargo.toml.orig:2). The fork uses the upstream crate name and version. Publishing it to crates.io as-is would collide with upstream 0.8.3. Unverified: whether the Cargo.toml.orig is the real manifest (it is the source that Cargo normalises, so yes).
- rust-version: Cargo.toml.orig:5 and Cargo.toml:14 say 1.76.0. CHANGELOG.md:45 says "Updated Rust toolchain to 1.90.0" (upstream 0.8.0 entry). loki-gpu/Cargo.toml:15 says rust-version = "1.76". No CI job checks the MSRV. Inconsistent/unverified.
- CHANGELOG.md:8 `## [Unreleased]` is empty (Added/Changed/Removed headings with no entries). All Loki-only work (features at Cargo.toml:45+) has no changelog entry. The changelog is inherited upstream text (Keep a Changelog 1.1.0 header, CHANGELOG.md:3-4).
- Licence: LICENSE-APACHE, LICENSE-MIT, COPYING.md (dual). Cargo.toml license = "MIT OR Apache-2.0". loki-gpu/Cargo.toml:14 is Apache-2.0 only and loki-gpu/LICENSE, NOTICE exist. README.md:248 says the loki-gpu module is Apache-2.0 and clean-room. Mismatch to resolve: the top-level crate is dual, loki-gpu is Apache-only.
- Security/policy files: NO SECURITY.md, CONTRIBUTING.md, CODE_OF_CONDUCT.md, deny.toml, rust-toolchain, clippy.toml, rustfmt.toml found at repo root. No cargo-deny/cargo-audit config. No SBOM tooling found (unverified: not searched beyond root and loki-gpu).
- CI: NO .github/ directory in loki-proofs. The only workflow is loki-gpu/.github/workflows/rust.yml:1-14, which runs `cargo check --workspace --locked` on stable only. Because it sits in a subdirectory, GitHub does not run it for loki-proofs. Local gates are scripts/verify-main.sh (--tier cpu/native) and scripts/train.sh (merge train), per docs/MERGE-GATES.md.
- deps.yaml: merge-DAG for branches (not a dependency manifest).
- Lean pin: TODO.md T09 (line ~39) says SYNCED-LOKI-COMMIT 877114d; repo main is eaf09d8.
- Open T02 (downstream consumer, TODO.md:11) and T04 (G2 cases, TODO.md:19). T06 (perf, TODO.md:27), T08 (constant-time review of batch_invert_rational, TODO.md:35, src/poly/mod.rs), T09 (Lean pin, TODO.md:39).

## Unsafe / side-channel surface
- `#![deny(unsafe_code)]` at src/lib.rs:9, with `#[allow(unsafe_code)]` on src/poly/kzg/msm/cuda.rs:33 and src/poly/kzg/msm/cuda/backend.rs:2 (unsafe FFI to CUDA and to loki_gpu_*). cuda.rs:45-46 transmutes slices. backend.rs:516 uses transmute on G1Projective in a test. The T08 target is src/poly/mod.rs:290-318 (batch_invert_rational; the denominator==ONE branch).

## Platform and dependency facts
- Cargo.toml:227-229: wasm32-unknown-unknown (target_os = "unknown") pulls getrandom 0.2 with the `js` feature. So the getrandom wiring exists in the manifest, but no WASM build or CI is recorded.
- Rayon: Cargo.toml:184 (dependency). Loki-only `loki-cpu-budget = ["blst/no-threads"]` (Cargo.toml feature list at :45+). src/poly/kzg/msm.rs:273 gates on target_os = "linux" and feature loki-cpu-budget.
- build.rs:1-6 runs CUDA only under feature `cuda`. LOKI_CUDA_ARCH defaults to 120 (build.rs:10-11).
- No mobile (iOS/Android) code, FFI, or uniffi bindings found in src (grep for android/ios/wasm: only Cargo.toml and docs).

## Gaps (what is missing for v1 docs and release)
1. No CHANGELOG entries for Loki features; the Unreleased section is empty.
2. No root SECURITY.md (no threat model, no disclosure route).
3. No CI workflow at repo root, no MSRV job, no cargo-deny/cargo-audit, no SBOM, no release checklist.
4. Crate name/version collide with upstream midnight-proofs 0.8.3; no crates.io plan.
5. rust-version inconsistency (1.76.0 manifest vs 1.90.0 changelog).
6. Licence split (dual top-level vs Apache-only loki-gpu) needs a provenance statement.
7. No WASM or mobile docs beyond TODO sections; PLATFORMS.md 9.1 is the only concrete WASM checklist.
8. No module docs check against the actual code (not run).
