# Proposal

## Why

Loki already has Quint models for transcripts, dispatch/fallback, caches and a planned scheduler, plus several Rust replay crates. Those checks are manual and split across campaigns; there is no release coverage manifest or CI gate, and some replay adapters use historical or machine-local source snapshots. A v1 release needs evidence tied to the exact release tree for each shipped change in stateful scheduling, caching, dispatch, fallback, transcript, and lookup-order behavior.

## What Changes

- **Must-have:** add a coverage manifest that maps every in-scope changed behavior and source path to model properties, positive witnesses, mutation checks, Rust MBT replay, shared compatibility fixtures, Lean contract status, and documentation.
- **Must-have:** gate release on typechecking, model tests, invariant checks, required witnesses and mutation controls, Rust MBT/ITF replay against the exact release source, and retained machine-readable results. A shipped path without a real Rust replay is an unmet release blocker.
- **Must-have:** check finite models with Apalache where supported. Temporal liveness claims require explicit fairness assumptions and temporal verification; sampled completion witnesses do not establish liveness.
- **Must-have:** separate stock 0.8.3 default behavior from opt-in Loki behavior in models and replay. Preserve the stock randomized lookup-order contract by default; model deterministic lookup only behind its opt-in feature.
- **Must-have:** remove machine-local and stale source snapshot assumptions from release replay. Pin all external tools and dependencies, and identify the source tree by the release SHA.
- **Nice-to-have:** broaden generated circuit shapes and run longer seeded model/fault campaigns outside the blocking presubmit gate.
- **Nice-to-have:** add models for a future scheduler only when a Loki-owned scheduler implementation exists; the existing scheduler model is not evidence for shipped behavior.

## Capabilities

### New Capabilities
- `quint-release-qa`: release traceability, model checking, Rust replay and evidence retention for behavior that changes execution order or state.

### Modified Capabilities
- None.

## Impact

This is a planning change for `quint/`, its test-only Rust MBT crates, and release CI; it changes no production API or behavior. Current foundations are `docs/modules/quint.md:1-28`, `quint/check.py:12-63`, `quint/mbt/README.md:1-39`, `quint/transcript.qnt:6-116`, `quint/pk_cache.qnt:1-84`, `quint/gpu_dispatch.qnt:1-125`, and `src/backend/dispatch.rs:29-47,113-159,185-224`. The existing scheduler model is explicitly planned-only (`docs/modules/quint.md:12-20`, `quint/scheduler.qnt:1-83`).

Compatibility remains anchored to unmodified stock `midnight-proofs` 0.8.3. Quint evidence cannot replace the stock API/default-feature and bidirectional proof/VK/PK checks owned by compat-api and compat-proof; it must use their fixture IDs and keep Loki-only configurations opt-in. Lookup ordering is specifically sensitive: the default still uses stock `HashMap` iteration, while `loki-deterministic-lookup-order` may change proof bytes (`docs/ARCHITECTURE.md:29,100-104,128-142,158-160`).

Quint and Apalache run on ordinary host CI and must not require CUDA. WASM browser/Node and iOS/Android gates remain owned by platform areas; they consume shared compatibility fixtures where applicable. Quint results do not count as cross-target runtime evidence. Feature coverage must identify differential fuzz targets owned by fuzz and Lean contract IDs/status owned by Lean, and link module documentation for every covered feature. Docs-release owns aggregate release-CI integration and publication.

Risks include bounded models missing unrepresented behavior, state-space growth, fixture/model drift, and unavailable injectable seams. Any affected shipped behavior without Rust replay, a source-to-model mapping, or an approved external contract remains a release blocker rather than being reported as covered. Dependencies: compat-proof owns stock fixture IDs and wire oracle; GPU/cache/lookup/transcript owners identify changed source seams; fuzz and Lean owners provide their contract identifiers; WASM/mobile owners retain target-specific verification; docs-release owns the release pipeline.
