# Proposal

## Why

The Loki W2 harness currently pins only stock `midnight-proofs` 0.8.3 and has no recorded k20 or stock 0.8.1 run. TODO T04 requires the missing versioned verifier evidence before the release can claim the requested compatibility matrix.

## What Changes

- Add an independently locked, unmodified stock 0.8.1 lane alongside the 0.8.3 release oracle.
- Complete k20 bidirectional proof verification, VK byte equality, and negative controls under the existing public formats.
- Record version-specific malformed multi-proof expectations: 0.8.1 predates the 0.8.2 `InvalidInstances` length guard; 0.8.2 and 0.8.3 include it.
- Use the shared fixture/result contract from `v1-compat-proof-wire-matrix`; preserve each stock version's public API behavior and do not introduce new wire formats.
- Archive source and lock pins, commands, and machine-readable results; document verified and unverified cases.

## Capabilities

### New Capabilities

- `stock-proof-compatibility`: stock version lanes, k20 acceptance, and version-aware negative-control classification.

### Modified Capabilities

None. The project has no existing OpenSpec capabilities.

## Impact

Current evidence and gaps are described at `loki-harness/README.md:1-10`, the independent stock pin is at `loki-harness/Cargo.toml:18-20`, and the open T04 acceptance gate is at `TODO.md:19-21`. The release oracle remains unmodified stock 0.8.3; stock 0.8.1 is supplementary historical evidence. No production code or new serialization format is introduced by this planning change.

This lane reuses the `loki-harness/fixtures/v1/manifest.jsonl` and canonical vectors proposed by `v1-compat-proof-wire-matrix`. It does not alter the baseline WASM/mobile requirement: target owners execute the stock and Loki verifier on target, with no GPU dependency. QA includes versioned differential tests/fuzz cases, Quint/Rust MBT classification, Lean source mapping and release docs for the stock-version difference. Risks are unavailable 0.8.1 dependency resolution and mismatched negative expectations if version guards are ignored. Dependencies: wire-matrix owns fixture schema and W2 runner contract; fuzz, Quint, Lean, WASM/mobile and docs-release consume the same IDs and pins.
