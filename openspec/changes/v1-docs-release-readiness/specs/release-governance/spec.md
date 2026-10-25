# Spec Delta

## Purpose

Defines a reproducible release process that preserves Cargo resolution for existing stock 0.8.3 consumers while separating the project milestone from package versioning.

## ADDED Requirements

### Requirement: Release identity preserves compatible Cargo resolution
The release SHALL tag the project `v1.0.0` while preserving the mandatory Cargo patch artifact as `midnight-proofs 0.8.3`; it SHALL NOT attempt to upload over the existing crates.io version.

#### Scenario: Compatible project tag and patch artifact are recorded
- **WHEN** `./scripts/release-preflight.sh --identity` checks the release manifest and clean downstream consumers
- **THEN** it records product tag `v1.0.0`, Cargo package `midnight-proofs 0.8.3`, and passing git/path patch resolution for both exact `=0.8.3` and caret `^0.8`; it records registry upload as out of scope unless separately approved and never attempts an upstream-version overwrite. Baseline identity and patch contract are `Cargo.toml:12-16` and `README.md:81-92`.

### Requirement: Release CI checks MSRV, compatibility and platform owner gates
The release SHALL provide a root CI workflow and deterministic release checklist that require the declared MSRV, stock API/default and wire compatibility gates, and all required WASM/mobile CPU owner gates.

#### Scenario: Release preflight requires all release artifacts
- **WHEN** `./scripts/release-preflight.sh --release-sha <sha>` runs on the release candidate
- **THEN** it fails on a missing root CI result, unvalidated declared MSRV, missing compat-api/compat-proof owner result, missing browser/Node or iOS/Android runtime compatibility result, or absent frozen platform budget; it archives the owner logs against `<sha>`. Existing policy anchors are `Cargo.toml:13-16,45-74`, `docs/MERGE-GATES.md:1-24`, and open gates `TODO.md:151-157,271-309,295-309,303-309` (T37, T73-T76).

### Requirement: Changelog and release checklist describe every shipped change
Each release SHALL have a non-empty changelog entry and a completed checklist covering scope, migration, compatibility, platform support, QA, provenance, SBOM and publication identity.

#### Scenario: Changelog and checklist are frozen
- **WHEN** `./scripts/release-preflight.sh --checklist` validates a release candidate
- **THEN** the changelog’s Unreleased section is converted into the selected release heading, every shipped Loki-only feature has an entry, and checklist items reference the matching evidence artifact; the current empty section is `CHANGELOG.md:8-13` and feature flags are listed at `Cargo.toml:45-74`.

### Requirement: Patch package is verified from its distributable archive
The mandatory git/path patch package SHALL pass Cargo packaging and clean downstream resolution checks before distribution, and the release record SHALL preserve its archive checksum and source revision.

#### Scenario: Package archive and patch consumers pass
- **WHEN** the release pipeline runs `cargo package --locked --list`, `cargo package --locked`, and compat-api clean consumers for git/path patch dependencies `=0.8.3` and `^0.8`
- **THEN** every packaged file is intentional, both consumers resolve to Loki while the package retains version `0.8.3`, no registry upload occurs, and the release record links the archive checksum and source revision; only a separately owner-approved additional registry identity may run its own publish dry run. Current patch instructions and the unvalidated standalone consumer are `README.md:81-92` and `TODO.md:11-13`.
