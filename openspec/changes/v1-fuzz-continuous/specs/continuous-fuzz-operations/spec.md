# Continuous Fuzz Operations

## Purpose

Provide bounded, reproducible continuous fuzz campaigns and actionable crash triage for Loki release qualification.

## ADDED Requirements

### Requirement: The CPU quick gate enforces a hard time budget

The deterministic CPU fuzz gate SHALL replay minimized corpora and fail when its total test execution exceeds 180 seconds. Its current configurations and warning-only limit are at `fuzz/quick.sh:11-17,25-46`.

#### Scenario: CPU gate stays within budget
- **WHEN** `fuzz/quick.sh` completes its default, `truncated`, and `truncated-loki-only` configurations
- **THEN** `fuzz/quick.sh` SHALL exit 0 and print the measured total when summed test time is at or below 180 seconds, and exit nonzero when it exceeds 180 seconds; the existing measurement and warning are at `fuzz/quick.sh:25-46`.

### Requirement: Scheduled CPU campaigns are bounded and reproducible

The workflow at `.github/workflows/fuzz.yml` SHALL run every registered target for exactly 600 seconds weekly, with all scheduled fuzz execution (including sanitizer runs, excluding build time and the separate 180-second presubmit gate) capped at 7200 CPU-seconds per week. It SHALL enforce pinned `nightly-2026-10-11`, 120-second per-input timeout, 8192 MiB RSS and 4096-byte maximum input; at most twelve targets fit this budget. Current campaign controls are at `fuzz/run-target.sh:1-13`.

#### Scenario: Scheduled campaigns record bounded run evidence
- **WHEN** the scheduled CPU workflow runs every registered target
- **THEN** `.github/workflows/fuzz.yml` SHALL invoke `fuzz/run-target.sh <target> 600 -s address` for exactly 600 seconds on every registered target, enforce those limits, retain command, revision and statistics, and fail the schedule if any target is skipped, more than twelve targets are registered, or the 7200 CPU-second aggregate cap is exceeded; existing control surface is `fuzz/run-target.sh:8-13` and target declaration is `fuzz/Cargo.toml:26-59`.

### Requirement: Release-candidate campaigns preserve the soundness duration gate

The release-candidate CPU campaign SHALL run every registered target for exactly 600 seconds and `soundness` for exactly 1800 seconds, with sanitizer runs included in a maximum 8400 CPU-second aggregate for up to twelve registered targets. The release-candidate runner SHALL use the pinned nightly, input, timeout and memory limits from the weekly campaign. Existing soundness entry point is `fuzz/diff/src/proof.rs:399-417` and runner limits are `fuzz/run-target.sh:8-13`.

#### Scenario: Release-candidate soundness campaign meets its minimum
- **WHEN** the release-candidate gate runs the complete registered CPU target inventory
- **THEN** `fuzz/release-candidate.sh` SHALL invoke each target for exactly 600 seconds, invoke `soundness` for exactly 1800 seconds under AddressSanitizer, stay within 8400 aggregate CPU-seconds for at most twelve registered targets, and fail when a target, duration or pinned resource limit is missing; verification records the invocation and run statistics against `fuzz/Cargo.toml:26-59` and the soundness target at `fuzz/diff/src/proof.rs:399-417`.

### Requirement: Sanitizer campaigns are separated from incompatible GPU runs

Every CPU target SHALL run under AddressSanitizer on a pinned x86_64 Linux runner with the pinned nightly toolchain; CUDA fuzz SHALL remain an optional separately scheduled job and SHALL NOT claim cargo-fuzz sanitizer coverage. Current CUDA separation is documented at `LOKI-FUZZ.md:178-183`.

#### Scenario: CPU sanitizer findings fail CI
- **WHEN** the sanitizer workflow runs the parser and component corpus with a supported sanitizer toolchain
- **THEN** `fuzz/run-target.sh proof_parse 600 -s address` and the same pinned AddressSanitizer run for every required CPU target SHALL fail CI on a report, while GPU campaigns remain separately locked and their sanitizer/safety evidence is owned by area 5; the current limitation is `LOKI-FUZZ.md:178-183`.

### Requirement: Crashes and minimized inputs are preserved as regressions

Every actionable crash, differential mismatch, invalid-proof acceptance or sanitizer finding SHALL produce a minimized reproducer retained as a corpus item or deterministic regression test with target, revision and oracle classification. Current corpora and test replay are described at `docs/modules/fuzz.md:11-18`.

#### Scenario: Crash artifact is reproducible and retained
- **WHEN** a continuous campaign discovers a crash or differential mismatch
- **THEN** the workflow SHALL retain the minimized input, campaign log and `cargo +nightly-2026-10-11 fuzz run <target> <artifact>` replay command for 90 days, and CI SHALL run that reproducer on subsequent changes; verification artifact is the committed regression corpus/test under `fuzz/corpus/<target>/` or `fuzz/diff/tests/`, building on `docs/modules/fuzz.md:11-18`.

### Requirement: Campaign evidence is bound to the release source revision

Each completed required fuzz gate SHALL publish a machine-readable manifest under `fuzz/artifacts/<release-sha>/manifest.json` containing target inventory, per-target command, source revision, toolchain, feature mode, duration, sanitizer, result, coverage/case assertion result and retained artifact references. Existing source-pinned corpus replay is at `fuzz/diff/tests/quick.rs:54-98`.

#### Scenario: Release preflight consumes the exact fuzz receipt
- **WHEN** `./scripts/release-preflight.sh --release-sha <sha>` checks owner evidence
- **THEN** it SHALL require `fuzz/artifacts/<sha>/manifest.json`, reject a different SHA, missing target, incomplete campaign or expired crash artifact reference, and archive the manifest and referenced outputs for that release; source input/corpus replay is `fuzz/diff/tests/quick.rs:54-98`.

### Requirement: The area 6 maintainer triages findings promptly

The named area 6 fuzz maintainer SHALL classify each finding within two business days using the documented triage entry point. Existing differential panic classification is at `fuzz/diff/src/parse.rs:72-99`.

#### Scenario: Minimized artifact is assigned a triage state
- **WHEN** a campaign produces a minimized crash, mismatch or sanitizer report
- **THEN** within two business days the area 6 maintainer SHALL run `fuzz/triage.sh <target> <artifact>` and record `LOKI_ONLY`, `STOCK_SHARED_ORDINARY_PANIC`, `SECURITY_BLOCKER` or `HARNESS_FAILURE`; current classification baseline is `fuzz/diff/src/parse.rs:72-99`.

### Requirement: Invalid-proof acceptance and memory-safety findings block release

Any invalid-proof acceptance or memory-safety finding SHALL block release regardless of whether stock 0.8.3 also reproduces it. Existing W7 result classifications are at `LOKI-FUZZ.md:125-159`.

#### Scenario: Shared security finding remains blocking
- **WHEN** a minimized repro shows invalid-proof acceptance or memory unsafety in either implementation
- **THEN** `fuzz/triage.sh <target> <artifact>` SHALL classify it as `SECURITY_BLOCKER` and keep the release gate red even if the same artifact affects stock; the shared-defect evidence is `LOKI-FUZZ.md:125-159`.

### Requirement: Ordinary shared panics are recorded without implying safety

An ordinary panic reproduced identically by stock SHALL be recorded as inherited behavior with a reproducer, and SHALL NOT count as security clearance. Current shared panic classification is at `fuzz/diff/src/parse.rs:72-99`.

#### Scenario: Same ordinary panic occurs in stock and Loki
- **WHEN** `fuzz/triage.sh <target> <artifact>` reproduces an ordinary panic in both implementations and no security finding is present
- **THEN** the command SHALL record `STOCK_SHARED_ORDINARY_PANIC` with the minimized reproducer while leaving unrelated release blockers unchanged; current classifier baseline is `fuzz/diff/src/parse.rs:72-99`.

### Requirement: GPU campaigns do not gate CPU release availability

GPU fuzz campaigns SHALL be opt-in and serialized through the shared GPU lock, while required CPU fuzz gates SHALL run without GPU hardware. Existing lock slicing is at `fuzz/gpu-slices.sh:1-16` and `fuzz/quick.sh:7-17`.

#### Scenario: CPU CI succeeds without CUDA hardware
- **WHEN** a CPU CI runner has no visible CUDA device
- **THEN** the required quick and scheduled CPU commands SHALL complete without invoking a GPU target, and only explicit GPU jobs SHALL acquire the lock through `fuzz/gpu-slices.sh`; the separation is based on `fuzz/quick.sh:7-17` and `fuzz/gpu-slices.sh:1-16`.
