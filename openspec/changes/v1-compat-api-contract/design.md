# Design

## Context

See `proposal.md` for motivation. The crate manifest already preserves edition 2021, Rust 1.76.0, package identity, and the upstream default feature, while adding Loki features (`Cargo.toml:12-74`). Existing default tests check selected behavioral areas (`tests/backend_default_stock.rs:1-74`), and the independent harness checks selected proof/key/SRS interoperability against registry 0.8.3 (`loki-harness/README.md:3-19,21-58`). These checks are useful evidence, not an exhaustive public API or target matrix.

## Goals / Non-Goals

**Goals:** define the stock-compatible public/build contract and make release CI produce repeatable, reviewable evidence for it.

**Non-Goals:** implement API changes, platform runners, accelerators, or protocol repairs in this planning change. Their implementation and owner decisions belong to compat-proof, WASM/mobile, and GPU changes.

## Decisions

1. **Use stock 0.8.3 as an immutable, unpatched baseline.** CI resolves the registry source independently, pins and records its checksum, and tests whether the semver checker accepts that source as a baseline root. `cargo public-api` item listings from both source roots are diffed directly; this avoids relying on a version-keyed diff when both products are 0.8.3. Semver analysis diagnoses incompatible surface changes; the item-level diff makes additions/removals reviewable. If the checker's same-version baseline behavior is unsuitable, build stock rustdoc JSON once and supply it as the pinned baseline instead. A Cargo manifest feature comparison is separate because API tooling does not prove feature-name/default parity. Generated snapshots may be retained as CI artifacts, not as the sole source of truth.

2. **Compare the default and feature-gated surfaces explicitly.** Run the API/feature gate for default, no-default, each stock feature (including implicit optional dependency features), supported stock feature combinations, and target-gated APIs across native, WASM, iOS device/simulator, and Android. `--all-features` is not a valid configuration here because `loki-gpu-backend` conflicts with legacy `cuda`/`cuda-ntt` (`src/lib.rs:11-18`); use the explicit valid-combination matrix instead. Each Loki-only feature is checked opt-in and must not leak into the default configuration. Feature unification can change enabled dependency features, so record resolved features and test real downstream dependency graphs with both `=0.8.3` and `^0.8` requirements. Unsupported combinations need an owner and explicit reason.

3. **Keep behavior-level oracles independent.** The downstream patch fixture proves that normal Cargo resolution selects Loki without source changes. The independent harness keeps crates.io stock unpatched and runs bidirectional proof verification, VK/PK/SRS serialization, and negative controls. Compat-proof owns fixture shape and protocol-specific pass/fail decisions; this change consumes that contract rather than defining a second one. A separate default-delta inventory calls out unfeaturegated rewrites and compares controlled results under one, two, and maximum named Rayon worker counts. Wire-divergent opt-ins receive explicit classifications and stock-verifier acceptance tests; they are excluded from stock byte-identity claims.

4. **Require target-specific stock execution as well as API compilation.** This area requires the stock-default API and downstream fixture to compile on native CPU, wasm32, iOS device/simulator, and Android. The WASM and mobile owners run the stock and Loki proving/verifying fixtures on their named runners, including both WASM thread modes and mobile simulator/emulator. Compatibility failures block release; runtime limits and numeric budgets come from those owners' contracts. Keep GPU features off in these CPU jobs.

5. **Pin the tooling runtime, not the product MSRV.** `cargo-semver-checks` and `cargo public-api` rely on rustdoc JSON, whose format changes with Rust. Their stable/nightly toolchain and tool versions must be pinned in CI independently of the crate's Rust 1.76.0 compile check. The MSRV job uses a dedicated lockfile that resolves to dependency versions supporting Rust 1.76; it does not imply the main development lock supports Rust 1.76 and does not raise the declared crate MSRV.

## Risks / Trade-offs

- **[Semver lint is incomplete for this contract or rejects a same-version baseline]** → Pair semver checks with source-root public API listings, manifest feature/default comparison, target-specific compilation, and patched consumer; smoke-test its expected diagnostics with a removal negative control and use pinned stock rustdoc JSON if root baseline semantics do not fit.
- **[Rustdoc JSON or checker upgrades break CI]** → Pin a known-compatible checker/nightly pair, record its version, and update it in a reviewed tooling change.
- **[Feature combinations grow quickly]** → Cover every stock declared and implicit feature and exposed target-gated API, plus validated upstream combinations. Treat unsupported combinations as explicit manifest errors rather than silently omitting checks.
- **[Main lock resolves dependencies newer than the declared MSRV]** → Keep 1.76.0 in package metadata, add a dedicated MSRV dependency lock, and fail the MSRV job if no compatible resolution can be built; do not claim the main lock is valid at 1.76.
- **[WASM and mobile can compile while runtime limits fail]** → Require platform-owned runtime jobs and numeric budgets; an undecided budget is a release blocker.
- **[Stock 0.8.3 rejects an inherited multi-proof mixed-instance case]** → Keep the behavior and diagnostic owned by compat-proof; do not use it to waive public API, manifest, or default compatibility checks (`TODO.md:15-17`).

## Migration Plan

There is no downstream migration: this is an additive replacement contract. Add the compatibility jobs before release and require them on the release commit. If a gate reveals an API, feature, or default regression, restore stock behavior before shipping; any intentional incompatibility requires an explicit product decision and cannot be marked as v1 full compatibility.

## Open Questions

- Platform owners must provide the exact browser/Node harness, iOS/Android runner availability, and numeric size/memory budgets. These values may be finalized in their changes, but unresolved values block v1 release.
- The exact compatible pinned versions of the API tools and the CI runner/toolchain that can build the stock baseline at Rust 1.76.0 must be selected when release CI is implemented.
