# Spec Delta

## Purpose

Makes security claims, source provenance, license compliance and dependency inventory reviewable for each Loki release revision.

## ADDED Requirements

### Requirement: T08 side-channel review gates constant-time claims
A release SHALL include a deployment-specific review of witness-dependent timing in rational inversion, and SHALL make no constant-time claim until the review is complete and required mitigations are verified.

#### Scenario: Constant-time status is checked
- **WHEN** `./scripts/release-preflight.sh --security --release-sha <sha>` runs
- **THEN** it requires a reviewer-signed T08 report tied to `<sha>` that analyzes the denominator-equals-one branches and records leakage, measurements, mitigations and residual limits; absent review or open critical mitigation blocks release and any constant-time claim. Source anchors are `src/poly/mod.rs:290-317`, `TODO.md:35-37`, and `docs/review/REVIEW.md:45`.

### Requirement: Release provenance and notices cover every shipped component
A release SHALL publish a clean-room provenance register and license/attribution notices for each shipped crate, backend and reused kernel fragment.

#### Scenario: Provenance and license review is complete
- **WHEN** `./scripts/release-preflight.sh --provenance --release-sha <sha>` checks `docs/security/PROVENANCE.md` and package manifests
- **THEN** each shipped component has a source boundary, source revision, authorship/provenance record, license identifier and required notice, and `cargo deny check licenses sources` passes; unresolved provenance or attribution blocks publication. Existing license declarations are `Cargo.toml:33-34` and `COPYING.md:1-15`; open backend and kernel audits are `TODO.md:159-161,371-373`, and claim boundaries are `docs/NOVELTY.md:3-13,37-49`.

### Requirement: Dependency risk and SBOM are bound to the release revision
Each release SHALL archive a machine-readable SBOM and dependency security results generated from the exact package lock and source revision.

#### Scenario: Supply-chain artifacts match the release
- **WHEN** `./scripts/release-preflight.sh --supply-chain --release-sha <sha>` runs `cargo deny check advisories bans licenses sources` and `cargo audit --locked`, and generates `sbom/loki.cdx.json`
- **THEN** unresolved disallowed advisories, licenses, sources or bans fail the gate, and the retained SBOM and tool reports identify the exact `<sha>` and lockfile digest; existing package/dependency scope is `Cargo.toml:12-16,33-43,45-74`, while security results are currently unconfigured per `TODO.md:159-161` and scout inventory.

### Requirement: Security disclosure and threat model are published
The release SHALL provide a threat model that names assets, trust boundaries, attacker capabilities, mitigations, residual risks and a security disclosure contact.

#### Scenario: Security documentation is ready to ship
- **WHEN** `./scripts/release-preflight.sh --security --release-sha <sha>` checks `SECURITY.md` and the release security report
- **THEN** the threat model covers proof inputs, randomness/transcripts, CPU/GPU fallback and target-specific memory/context failures, identifies T08 and any remaining exposure, and supplies a reachable disclosure route; the current runtime error and target-loss model is described in `docs/PLATFORMS.md:66-75,280-316`, and no root security policy was found in scout inventory.
