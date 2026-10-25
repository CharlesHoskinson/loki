# Proposal

## Why

Loki has separate implementation, compatibility, platform and formal evidence tracks, but no single release acceptance contract. v1.0.0 needs an auditable decision across all of them without mistaking a merged branch or a passing model for release qualification.

## What Changes

- Establish milestone gates M0–M6, exact-revision evidence, mandatory stock 0.8.3 compatibility, first-class WASM/mobile CPU qualification, and the release definition of done.
- Maintain a complete registry of area changes, feature-to-requirement/QA/docs coverage, and disposition of every T01–T92 item and p3/p4/p5/p7/w10/W12 work stream.
- Separate MERGE-CPU acceptance from post-integration RECEIPT-NATIVE and final release acceptance.
- Require all ten authors to acknowledge the roadmap and all ten authors plus ten scouts to finish their reviewed specifications before declaring the planning lab complete.

## Capabilities

### New Capabilities

- `release-qualification`: Evidence-based promotion through milestones and an exhaustive v1 release manifest.

### Modified Capabilities

None; the OpenSpec main capability store is initially empty.

## Impact

Owner: astra-v1. Consumers: the release coordinator and all ten area owners. The umbrella owns promotion and completeness checks; individual changes own implementation behavior and detailed thresholds. Dependencies are listed in the roadmap change registry and mirrored in the umbrella design.

Stock API, default features and wire semantics remain unchanged. WASM/mobile CPU support, differential fuzzing, Quint/MBT, Lean and documentation are mandatory evidence dimensions for shipped behavior. No production files are changed during this lab.

Out of scope: running the release implementation loop, publishing crates, merging branches, new proof protocols, or claiming hardware/formal gates passed without evidence. Non-CUDA accelerators and optional optimizations are deferred with reasons in the roadmap.

Risks: stale source receipts, unsupported target dependencies, unclosed formal obligations, unavailable runners, and release package versioning that breaks existing 0.8 consumers.

Existing sources: `LOKI-DONE.md:3`, `docs/MERGE-GATES.md:8`, `docs/MERGE-GATES.md:16`, `TODO.md:5`, `Cargo.toml:13`, `README.md:81`, `docs/PLATFORMS.md:406`, `deps.yaml:11`.
