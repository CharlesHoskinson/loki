# Design

## Context

The current harness dependency at `loki-harness/Cargo.toml:18-20` pins only stock 0.8.3; README records k10/14/17 runs and says 0.8.1 and k20 have no recorded run (`loki-harness/README.md:1-10`). Source comparison by haiku-v1-2 reports 0.8.1→0.8.3 differences in `plonk/verifier.rs` and `poly/kzg/msm.rs`: 0.8.2 adds malformed multi-proof length validation, and 0.8.3 changes an MSM dispatch threshold. No proof-wire, key-encoding, transcript-order, or challenge change was found. This source comparison is not an executed compatibility run; details are in `docs/v1-research/luna-v1-2/compat-proof.md`.

## Goals / Non-Goals

**Goals:**

- Close T04 with a separate stock 0.8.1 lane, k20 evidence, bidirectional proof checks, key equality and version-aware negative controls.
- Preserve stock 0.8.3 as the release oracle and share fixture IDs with the wire-matrix change.

**Non-Goals:**

- Change the proof format, transcript, or API to make versions appear identical.
- Make 0.8.1 the release oracle or claim an unrun source comparison as compatibility evidence.
- Duplicate the canonical fixture schema, platform runner, fuzz campaign or Quint/Lean model owned by adjacent areas.

## Decisions

1. Use a dedicated Cargo workspace and lockfile for stock 0.8.1, separate from the 0.8.3 workspace. Keep each registry crate unmodified and resolve the Loki package through its path source; record source pin and lock digest in every result.
2. Reuse `loki-harness/fixtures/v1/manifest.jsonl` from `v1-compat-proof-wire-matrix`. Add k20 rows and stock-version fields there rather than creating a second fixture format. Keep 0.8.3 acceptance as the release gate; 0.8.1 is a required additional compatibility lane under T04.
3. Match negative controls to the versioned stock behavior. In particular, 0.8.1 does not contain the multi-proof length guard present in 0.8.2 and 0.8.3; do not force one rejection expectation across all versions. Wrong-input, malformed-byte, and proof acceptance controls are still recorded for each verifier.
4. Retain commands, source/lock pins, and logs as machine-readable release evidence. A failed or missing 0.8.1 resolution is a blocker until solved; it cannot silently remove T04.

The proposed runner entry point is `./compat-proof-stock/run.sh`; its `--versions`, `--k`, `--negative`, and `--serial` arguments are acceptance command names for implementation, not current commands. It SHALL invoke the k20 Cargo test directly and SHALL NOT reuse `loki-harness/run.sh`'s 292-second outer timeout. It records elapsed wall time and host details; do not claim a numeric runtime budget until a named release runner supplies a measured baseline. Shared fixtures are consumed from the wire-matrix manifest; this change must not introduce competing case IDs or serialization.

## Risks / Trade-offs

- The 0.8.1 crate was source-compared by the scout; the pinned package is available from `https://static.crates.io/crates/midnight-proofs/midnight-proofs-0.8.1.crate` but is not in the local registry observed during direct inspection. Cargo resolution and an actual compile/run remain unverified. Both 0.8.1 and 0.8.3 declare `midnight-curves` 0.3.1, so the source comparison did not find a curves-version difference.
- Version-specific invalid-input behavior can be mistaken for wire incompatibility; result rows must preserve the exact stock version and case classification.
- k20 is a larger CPU case and may need serial execution and the W2 harness resource controls; no run-time estimate is claimed.

## Migration Plan

Add the separate version workspace and manifest rows without altering the existing 0.8.3 harness. Run the k20 and negative-control matrix serially, archive both lanes, then update T04 and compatibility docs. Any unexplained difference blocks release; rollback is to the last unchanged stock-compatible implementation and its recorded fixture set.
