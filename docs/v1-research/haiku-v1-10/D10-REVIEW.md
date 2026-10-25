# D10-REVIEW: review of openspec/changes/v1-docs-release-readiness (haiku-v1-10)

`openspec validate v1-docs-release-readiness --strict` reports: valid.
Anchors checked against the repo: README 38-42, 81-92, 168-191, 209-220; Cargo.toml 12-16, 34, 36-43; CHANGELOG 8-13; src/lib.rs 6-9, 11-18; docs/PLATFORMS.md 66-75, 264-327; docs/review/REVIEW.md:45; TODO 7-9, 11-13, 15-21, 23-25, 35-37, 151-161, 271-309, 371-373; docs/modules/README.md (67 lines, 30-58 in range). All matched. No wrong anchors found.

Findings

1. [blocking] docs.rs metadata will fail. Cargo.toml:37 and Cargo.toml.orig:15 set `[package.metadata.docs.rs] all-features = true`. src/lib.rs:13-17 compile_error!s when loki-gpu-backend is combined with cuda or cuda-ntt. Add a task (and a scenario in release-documentation) that replaces this with the valid feature list used by the rustdoc lane. Tasks.md:4 only cites the baseline; it does not change the metadata.
2. [gap] Threat model and disclosure have no task. The security-supply-chain requirement "Security disclosure and threat model is published" (SECURITY.md, threat model) has no task in tasks.md. Tasks 8-10 cover T08, provenance and SBOM only. Add a task that creates SECURITY.md and the threat-model file.
3. [gap] Files named in scenarios are never created. Scenarios cite docs/security/PROVENANCE.md, SECURITY.md, scripts/release-preflight.sh and sbom/loki.cdx.json. None exist (checked). Tasks 1-4 run `./scripts/release-preflight.sh --docs/--platforms` before task 5 creates the script. Move script creation to the first task, or say that tasks 1-4 are blocked until then.
4. [gap] Mobile device vs simulator. The spec and tasks say "iOS" only. Name aarch64-apple-ios (device) and aarch64-apple-ios-sim (simulator) separately, as astra asked. Both targets are installed on this host (rustup target list --installed); no build was run.
5. [gap] Mobile PK interchange. The platform scenario in release-documentation/spec.md requires only "stock-compatible runtime results". Require proof, VK, PK and verifier-param byte interchange on the mobile lane, per luna-v1-4 and luna-v1-2 (ProvingKey::write/read/to_bytes/from_bytes at src/plonk/mod.rs:372-457).
6. [gap] WASM stock verifier. Require that the stock 0.8.3 verifier also compiles to wasm32 and runs on each target mode (browser and Node, single and wasm-threads). Native stock verification is only an extra cross-check. Luna-v1-3 has ACKed this; the draft does not state it. Astra's REVIEW requires it too.
7. [gap] Task IDs not cited. T37 (declared Rust minimum, TODO.md:151-157) should be cited by the MSRV requirement in release-governance. T73, T75 and T76 (WASM audit, wasm-bindgen-rayon scaling, memory ceiling) are not cited. T72 (mobile memory-kill; astra asked for honest process-level failure, not ContextLost recovery) is not cited. Add them so each TODO maps to a disposition.
8. [note] Package identity. The draft correctly blocks 1.0.0 for ^0.8. Its "compatible newer 0.8.x" option has a risk: a fork published as 0.8.x shares the upstream series, so a later upstream 0.8.x release would collide or shadow it. Say so in the identity decision, or prefer a new package name.
9. [note] Each SHALL has at least one scenario, and the four requirements per capability are consistent with the design. Validation passes. No other ambiguous SHALL found.

Proposed next step for luna-v1-10: fix findings 1-7 in the change files. I have not edited any spec.
