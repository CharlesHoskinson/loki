# Stock Proof Compatibility

## Purpose

Specify reproducible stock 0.8.1 and 0.8.3 verifier evidence, including the k20 cases and version-specific negative controls.

## ADDED Requirements

### Requirement: Stock 0.8.1 SHALL have a separate locked lane

Release evidence SHALL include an unmodified stock 0.8.1 dependency in a separately locked workspace from the Loki path package and stock 0.8.3 lane. Acquire the package from `https://static.crates.io/crates/midnight-proofs/midnight-proofs-0.8.1.crate` and record its checksum. Stock 0.8.3 SHALL remain the release oracle.

#### Scenario: Version pins are independently recorded

- **WHEN** `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --serial` resolves and runs the stock workspaces
- **THEN** each result identifies the registry source, exact version, source pin, lockfile digest, and command; verification is the named version-matrix gate, building on `loki-harness/Cargo.toml:18-20` and `TODO.md:19-21`

### Requirement: T04 SHALL verify k20 proofs and keys in both directions

The pinned stock 0.8.1 lane SHALL cover k20 proof verification in both directions for `Fixture<false>` and `Fixture<true>`, independently generated VK byte equality, supported PK/verifier-parameter serialization round trips, and version-appropriate negative controls. The new runner SHALL enforce a 1800-second hard timeout for k20; timeout SHALL be reported as a failed, non-passing result.

#### Scenario: k20 T04 acceptance

- **WHEN** `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --k 20 --serial` runs k20 fixtures from the canonical manifest under the 1800-second hard timeout
- **THEN** both fixture forms verify in both directions, VK bytes match, key/parameter round trips pass, and all declared negative outcomes match their stock-version expectations; verification is the named T04 version gate, building on `loki-harness/README.md:1-10`, `loki-harness/tests/differential.rs:25-32`, `loki-harness/src/backend.rs:45-111`, `loki-harness/src/lib.rs:87-115`, and `TODO.md:19-21`

### Requirement: Version-specific malformed multiproof behavior SHALL be preserved

Negative-control results SHALL preserve the pinned stock version behavior. The malformed multi-proof length guard returning `InvalidInstances` SHALL be expected for 0.8.2 and 0.8.3; the matrix SHALL NOT assume that guard exists in 0.8.1. A mismatch SHALL block the version gate until explained.

#### Scenario: 0.8.1 and 0.8.3 malformed-input classification

- **WHEN** `./compat-proof-stock/run.sh --negative malformed-multiproof --versions 0.8.1,0.8.3 --serial` runs against the unmodified stock versions and matching Loki build
- **THEN** the manifest records each version's observed acceptance/error classification, including the 0.8.1 versus 0.8.2+ guard difference, and Loki matches the corresponding stock version where the shared API supports the case; verification is the versioned negative-control replay, building on `loki-harness/src/lib.rs:116-146`, `loki-harness/src/backend.rs:268-303`, and stock 0.8.3's guard at `midnight-proofs-0.8.3/src/plonk/verifier.rs:225-231`

### Requirement: Version evidence SHALL be reproducible and documented

The version gate SHALL retain exact source/lock pins, compiler/target, feature set, commands, canonical fixture IDs, logs, and machine-readable outcomes. Documentation SHALL distinguish source-level comparison from executed compatibility evidence and SHALL label absent 0.8.1/k20 runs unverified.

#### Scenario: Release reviewer replays T04

- **WHEN** a release reviewer runs `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --k 20 --serial` with the archived workspaces and canonical fixture manifest
- **THEN** the recorded k20 and negative-control classifications are reproducible and any missing run is shown as unverified; verification is the named T04 replay and documentation review, building on `loki-harness/README.md:1-10` and `TODO.md:19-21`
