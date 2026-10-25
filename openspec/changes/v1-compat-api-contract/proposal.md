# Proposal

## Why

Loki keeps the `midnight-proofs` package name and 0.8.3 version so downstream users can patch crates.io, but the current evidence covers selected fixtures rather than the complete public API and build contract. v1 needs a repeatable release gate that proves the fork remains a usable superset of stock 0.8.3, including defaults and supported CPU targets.

## What Changes

- Preserve every stock 0.8.3 public item, signature, trait contract, feature name, package identity, edition, MSRV, and default-feature behavior; Loki-only behavior remains opt-in.
- Add a mechanical API compatibility gate combining semver analysis with a complete public API diff and feature metadata comparison against a stock 0.8.3 registry source pinned by checksum. Run it across the explicit stock feature × cfg/target matrix and record unsupported combinations.
- Add clean downstream `[patch.crates-io]` consumers for both exact `=0.8.3` and caret `^0.8` dependency requirements, then compile the upstream feature surface on the declared MSRV.
- Reuse the independent stock 0.8.3 cross-verification harness for behavior-level checks, with fixtures and protocol interchange owned by compat-proof.
- Audit the default-on arithmetic/prover rewrites separately from API shape, using controlled stock/Loki comparisons over one, two, and the maximum named Rayon worker count; classify wire-divergent Loki opt-ins and give proof-producing options stock-verifier tests without claiming stock byte identity.
- Require the API/default compatibility checks on each supported CPU target: native, wasm32-unknown-unknown browser and Node single-threaded/wasm-threads, iOS, and Android. Platform owners define runners, budgets, and limitations; the default-delta comparisons include the higher perf-owned matched worker budget, N=20.
- Keep proof-format changes, accelerator implementation, and platform-specific runner design in their owning changes.
- Tag the product release as v1.0.0 while retaining the patched crate's stock package identity (`midnight-proofs` 0.8.3); a separately published package/version requires an explicit owner decision.

## Capabilities

### New Capabilities

- `compat-api`: The stock 0.8.3 public API, Cargo feature, default, MSRV/edition, and patched-consumer contracts remain available and mechanically checked.

### Modified Capabilities

None.

## Impact

This change governs `Cargo.toml`, the complete `src/` public surface, CI configuration, downstream consumer fixtures, and API compatibility documentation. Current anchors include `Cargo.toml:13-74`, `src/lib.rs:11-30`, `tests/backend_default_stock.rs:1-74`, `loki-harness/README.md:3-19,21-58`, `README.md:81-114`, and `docs/PLATFORMS.md:1-18`. Existing checks verify selected default behavior and proof interchange; they do not yet prove the complete API or target matrix. The known stock self-rejection for mixed committed/plain instances in multi-proof runs remains an explicit compat-proof protocol decision (see `TODO.md:15-17`), not a reason to weaken the API contract. The release tag is v1.0.0; the `[patch.crates-io]` artifact preserves stock package identity and version 0.8.3.

**Out of scope:** repairing inherited proof/transcript behavior (owned by compat-proof), implementing WASM/mobile runners or WebGPU/Metal/Vulkan (owned by platform/GPU areas), and asserting exhaustive semantic equivalence from finite fixtures. The compatibility contract and release gates depend on compat-proof's stock interchange fixtures, WASM/mobile owners' CPU runner and budget contracts, QA owners' fuzz/Quint/Lean evidence, and docs-release's CI and public documentation integration.

**Risks:** semver tooling alone may miss additive-superset constraints such as feature names, target-gated items, or features accidentally added to defaults. The gate therefore combines semver analysis, public API and manifest diffs, a patched downstream build, and the stock behavior harness. Platform-specific dependency and linker failures may also appear after native API checks pass.
