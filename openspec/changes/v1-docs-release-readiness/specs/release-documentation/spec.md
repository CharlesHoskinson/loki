# Spec Delta

## Purpose

Defines the evidence and user documentation required to describe Loki’s supported behavior and release targets accurately.

## ADDED Requirements

### Requirement: Public compatibility claims stay within recorded evidence
Release documentation SHALL distinguish a verified result from a planned target and SHALL state the stock 0.8.3 test scope and known limits.

#### Scenario: Release compatibility statements are checked
- **WHEN** `./scripts/release-preflight.sh --docs` checks a release candidate
- **THEN** the documentation identifies the exact owner-provided `./loki-harness/run.sh` result and fixture set, records two-way verifier acceptance separately from byte-equality claims, and names unverified k20/0.8.1 and mixed-instance limits; source anchors are `README.md:38-42,168-191` and `TODO.md:15-21`.

### Requirement: Rust API documentation builds for supported feature sets
The release SHALL build rustdoc for every documented supported feature configuration and SHALL keep a navigable module map for every shipped crate module.

#### Scenario: Rustdoc release check runs
- **WHEN** the release job runs `cargo doc --no-deps --features committed-instances,loki-backend-api,loki-multiopen,loki-harden,loki-profile` and `cargo test --doc --features committed-instances,loki-backend-api,loki-multiopen,loki-harden,loki-profile`
- **THEN** both commands succeed without missing-doc or broken-link diagnostics, each module maps to a source-backed page in `docs/modules/README.md:30-58`, and docs.rs metadata selects this valid feature set instead of `all-features`; the lint contract is `src/lib.rs:6-9`, while `Cargo.toml:36-43` and `src/lib.rs:11-18` show the existing docs.rs all-features conflict.

### Requirement: Platform documentation reports measured CPU support and limits
The release SHALL document WASM and mobile CPU support only when the platform owner’s runtime compatibility job and numeric resource measurements are archived for the named target and runner.

#### Scenario: Cross-target documentation is release-ready
- **WHEN** `./scripts/release-preflight.sh --platforms` aggregates WASM and mobile owner artifacts, including `mobile-compat/run.sh --target aarch64-apple-ios`, `mobile-compat/run.sh --target aarch64-apple-ios-sim`, and `mobile-compat/run.sh --target aarch64-linux-android`
- **THEN** `docs/PLATFORMS.md` records browser and Node single-threaded and wasm-threads lanes with both Loki and stock 0.8.3 verifiers running on each WASM mode, plus iOS device (`aarch64-apple-ios`), iOS simulator (`aarch64-apple-ios-sim`) and Android CPU lanes; it documents bidirectional proof, VK, PK and verifier-parameter interchange, runner/toolchain/emulator-host identity, a physical mid-range mobile CPU leaf baseline, approved client-circuit/data boundaries, ten-proof physical thermal soak and sustained throughput, measured memory/time/binary-size budgets, and process-level memory termination limits without promising recoverable `ContextLost` behavior. WASM reports include 4/8/16-worker scaling or explicit unsupported counts, a CPU recursion-chain fixture result or a named per-lane numeric hard wall-clock timeout with the unsupported workload/size recorded, and matched BLST native/WASM baselines with and without SIMD128/threads. Missing target evidence or an unfrozen numeric budget fails the gate. Existing anchors are `docs/PLATFORMS.md:264-327`, `TODO.md:271-309` (T67/T68/T71/T72/T73/T75/T76), `TODO.md:303-305` (T75), `TODO.md:323-325` (T80), `TODO.md:359-361` (T89), and `src/plonk/mod.rs:372-457`.

### Requirement: Every shipped feature has a documentation and QA trace row
The release SHALL provide a row for each shipped feature that links user/module documentation to its compatibility, differential-fuzz, Quint/MBT, Lean and platform evidence or records a reasoned non-applicability.

#### Scenario: Feature coverage is complete at the release SHA
- **WHEN** `./scripts/release-preflight.sh --coverage --release-sha <sha>` checks the coverage manifest
- **THEN** every enabled-by-release feature has a README or module-doc path, owner command and immutable result for compatibility, fuzz, Quint/MBT and Lean, with platform evidence where applicable, all bound to `<sha>`; the Quint row links `quint/coverage.yaml` and `quint/release-results/<sha>/`, and cache rows explicitly include T20 identity/integrity and T33 retention/caller assumptions. The current feature surface is `Cargo.toml:45-74`, docs index `docs/modules/README.md:30-58`, and cache follow-ups `TODO.md:83-85,131-133`.

### Requirement: Performance documentation reports the complete-chain evidence
Release documentation SHALL publish the owner-qualified stock-versus-Loki complete Sherlock recursion-chain results and target budget records without treating isolated kernels as end-to-end performance.

#### Scenario: Performance table and platform records match owner artifacts
- **WHEN** `./scripts/release-preflight.sh --perf --release-sha <sha>` checks the owner’s `docs/benchmarks/RESULTS.md` output and target budget manifests
- **THEN** the README and platform docs link a complete leaf-parent-decider table with fixture, source/lock/features, named runtime, raw cold/warm observations, median/IQR/min/max and the stock/Loki median ratio; each target row has actual latency, peak RSS, artifact size, frozen limits and pass/NOT READY status, and no unqualified speedup claim is made. Existing baseline claims and limitations are `README.md:49-63,193-207` and open portability work is `TODO.md:27-29,167-169`.
