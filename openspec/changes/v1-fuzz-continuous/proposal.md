# Proposal

## Why

W7 has useful local quick and long-run scripts, but the quick script currently warns rather than fails when its three-minute test budget is exceeded, and no CI workflow is present in this checkout. A release-grade fuzz program needs repeatable scheduled campaigns, bounded sanitizer runs, retained crash artifacts and a clear triage policy.

## What Changes

- Make the CPU differential quick gate a hard ≤180-second test-time limit and replay minimized corpora in all supported compatibility configurations.
- Add weekly CPU-only libFuzzer campaigns of exactly 600 seconds per registered target (7200 CPU-second cap), plus release-candidate campaigns of exactly 1800 seconds for `soundness` and 600 seconds for every other target (8400 cap for up to 12 targets); use a pinned toolchain, 120-second per-input timeout, 8192 MiB RSS, 4096-byte input cap, corpus persistence and 90-day minimized crash artifact retention.
- Run bounded AddressSanitizer campaigns on the pinned x86_64 Linux/nightly CPU runner, separately from CUDA campaigns; do not claim UBSan support for Rust cargo-fuzz. Keep GPU jobs optional and locked while requiring shipped GPU backends to provide their own native sanitizer/safety evidence.
- Define crash triage by the designated release QA maintainer within two business days, upstream-shared failure classification, regression retention, release blocking rules and security-conscious artifact limits. An unassigned triage owner is a release blocker.
- Add the fuzz presubmit and weekly campaign to `.github/workflows/fuzz.yml`, coordinated with docs-release, which owns shared CI and release wiring.
- Publish `fuzz/artifacts/<release-sha>/manifest.json` plus referenced campaign and regression outputs required by docs-release `release.yml` / `release-preflight.sh`; missing or stale outputs fail release preflight.
- Document command lines, budgets, CI ownership, triage flow and unverified evidence in `LOKI-FUZZ.md` and `docs/modules/fuzz.md`.

## Capabilities

### New Capabilities

- `continuous-fuzz-operations`: CI scheduling, campaign budgets, sanitizer execution, artifact handling and crash triage for Loki fuzz targets.

### Modified Capabilities

None.

## Impact

Planning targets `fuzz/quick.sh:1-46`, `fuzz/run-target.sh:1-13`, `fuzz/gpu-slices.sh:1-16`, CI configuration (none is present in this checkout), `fuzz/corpus/`, `fuzz/logs/`, `LOKI-FUZZ.md`, and `docs/modules/fuzz.md`. Existing W7 reports previous run limits and gaps at `LOKI-FUZZ.md:87-123,178-186`. This is CI and QA tooling only: stock 0.8.3 remains the oracle, no public API or protocol/default changes are proposed. CPU-only schedules do not depend on GPU access; CUDA campaigns remain opt-in under the shared lock. Shipped GPU backends still require GPU differential and native sanitizer/safety evidence owned by the GPU area. Dependencies: docs-release owns shared CI/release pipeline and evidence publication; compat-proof owns fixtures; component targets are specified in `v1-fuzz-components`; WASM/mobile gates define cross-target CPU replay availability.
