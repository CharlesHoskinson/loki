# Design

## Context

See proposal.md for motivation. `fuzz/quick.sh` currently tests three CPU feature configurations under a 180-second budget but only warns on overrun (`fuzz/quick.sh:11-17,25-46`). `fuzz/run-target.sh` already supports bounded campaigns, pinned nightly, per-target logs, timeouts, RSS and input-length controls (`fuzz/run-target.sh:1-13`). GPU campaigns are serialized in slices (`fuzz/gpu-slices.sh:1-16`). No CI workflow files are present in this checkout.

## Goals / Non-Goals

**Goals:** A deterministic hard-budget CPU gate, weekly corpus-backed CPU campaigns, supported CPU sanitizers, persistent minimized regressions, and consistent triage states.

**Non-Goals:** Require a specific external fuzz hosting service, run CUDA under incompatible sanitizers, make GPU access a release dependency, upload unredacted user proof material, or claim a campaign proves absence of defects.

## Decisions

1. **Presubmit budget is 180 seconds of measured test time.** This preserves the existing budget; overruns fail. Build time is reported separately so build-cache variation does not make test budget nondeterministic. Alternative: raise the gate to accommodate future targets; rejected until profiling shows a need and compat/perf owners approve a measurable new cap.
2. **Scheduled CPU campaigns run each registered target for exactly 600 seconds weekly; release-candidate campaigns run `soundness` for exactly 1800 seconds and every other target for exactly 600 seconds.** The weekly aggregate fuzz execution budget, including AddressSanitizer runs but excluding build and presubmit time, is at most 7200 CPU-seconds. The release-candidate budget is 8400 CPU-seconds for up to twelve registered targets. With existing settings, each run uses a 120-second per-input timeout, 8192 MiB RSS cap and 4096-byte input cap. More than twelve targets blocks until both budgets are explicitly increased. Alternative: nightly 30-minute runs; rejected for unnecessary CI cost until campaign evidence justifies it.
3. **Run AddressSanitizer on a pinned x86_64 Linux/nightly runner; keep sanitizer and CUDA workflows separate.** The official Rust OSS-Fuzz integration currently documents libFuzzer plus AddressSanitizer. Rust's sanitizer set differs from the generic C/C++ matrix, so do not claim UBSan coverage for cargo-fuzz. MSan/TSan remain optional until the full target and dependency graph are validated. CUDA runs use their existing optional lock slices and are labeled as covered by area 5's native safety receipts. Alternative: disable sanitizer checks for all jobs; rejected because parser and CPU arithmetic are suitable for ASan coverage.
4. **Use CI artifacts plus corpus cache.** Retain minimized crash inputs, logs, command, commit, target and oracle classification. An artifact size cap and secret scan precede upload; corpus merging is deterministic and reviewed through normal changes. Alternative: commit all newly discovered corpus files automatically; rejected because unreviewed generated files can bloat and destabilize the repository.
5. **Triage uses a named command and fixed states.** `fuzz/triage.sh <target> <artifact>` reruns both implementations and emits `LOKI_ONLY`, `STOCK_SHARED_ORDINARY_PANIC`, `SECURITY_BLOCKER`, or `HARNESS_FAILURE`. Stock parity never clears invalid-proof acceptance or memory-safety findings. The area 6 fuzz maintainer owns the first response within two business days; release CI records the named maintainer.
6. **CI ownership is shared with docs-release.** Fuzz defines commands, resource bounds, outcome semantics and target matrix. The shared workflow is `.github/workflows/fuzz.yml`; docs-release owns release workflow wiring and runner pool, while fuzz owns its target job and criteria. Fuzz publishes `fuzz/artifacts/<release-sha>/manifest.json`; release preflight rejects missing or stale manifests.

## Risks / Trade-offs

- [Coverage may remain seed-driven] → Use generated structured bytes and track coverage/exec counts; require weekly campaigns and corpus replay, not only smoke duration.
- [Sanitizer support varies by target/toolchain] → Pin the x86_64 Linux/nightly AddressSanitizer runner and fail clearly if it cannot run; never imply that stock parity or an unsupported sanitizer provides security clearance. Document CUDA evidence under area 5.
- [Artifact retention can expose private inputs] → Cap input/log size, scan artifacts, avoid accepting user-submitted proving keys as seeds, and retain only minimized synthetic reproductions.
- [Aggregate CI budget can be exceeded as targets grow] → Enforce 7200 weekly and 8400 release-candidate CPU-seconds with twelve-target maximums; release cannot silently skip a target.
- [Findings may affect security disclosure] → Keep sensitive reproducer access restricted and follow the project's security disclosure process; the two-business-day interval is internal triage, not a public disclosure deadline.
