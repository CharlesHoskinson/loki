# Tasks

## 1. Presubmit CPU gate

- [ ] 1.1 Make `fuzz/quick.sh` fail on summed test time above 180 seconds and verify both green and forced-overrun outcomes using `fuzz/quick.sh` plus a bounded overrun fixture, building on `fuzz/quick.sh:25-46`.
- [ ] 1.2 Ensure all required targets and minimized corpora run in the CPU gate without GPU dependencies; verify with `fuzz/quick.sh` on a runner with CUDA hidden, based on `fuzz/quick.sh:7-17` and `fuzz/Cargo.toml:15-20`.

## 2. Scheduled campaigns and sanitizers

- [ ] 2.1 Add `.github/workflows/fuzz.yml` scheduled CPU campaigns for every registered target at exactly 600 seconds each, with a 7200 CPU-second weekly aggregate cap including AddressSanitizer executions, 120-second timeout, 8192 MiB RSS and 4096-byte input bound; verify workflow invocation and replay `fuzz/run-target.sh <target> 600 -s address`, based on `fuzz/run-target.sh:8-13`.
- [ ] 2.2 Add `fuzz/release-candidate.sh` to run every registered target for exactly 600 seconds and `soundness` for exactly 1800 seconds under AddressSanitizer, with an 8400 CPU-second aggregate cap for up to twelve targets; verify duration accounting and fail-closed behavior using a deterministic mocked runner over `fuzz/Cargo.toml:26-59`.
- [ ] 2.3 Add bounded CPU AddressSanitizer runs for all required targets on pinned x86_64 Linux/nightly and a separate opt-in CUDA job; verify sanitizer failure injection and GPU-lock serialization, and leave UBSan unclaimed for Rust cargo-fuzz, based on `fuzz/run-target.sh:11-13`, `fuzz/gpu-slices.sh:1-16`, and `LOKI-FUZZ.md:178-183`.
- [ ] 2.4 Add deterministic corpus merge/minimize and artifact retention for campaign logs, minimized inputs, command, target and revision; verify a synthetic crash survives upload and is replayed by `cargo +nightly-2026-10-11 fuzz run <target> <artifact>`, based on `docs/modules/fuzz.md:11-18`.
- [ ] 2.5 Publish `fuzz/artifacts/<release-sha>/manifest.json` with per-target command, revision, toolchain, feature mode, duration, sanitizer, result, coverage assertion and artifact references; verify `./scripts/release-preflight.sh --release-sha <sha>` rejects missing, stale or incomplete fuzz output, based on `fuzz/diff/tests/quick.rs:54-98` and docs-release's `release-governance/spec.md:19-20`.

## 3. Triage and release evidence

- [ ] 3.1 Implement `fuzz/triage.sh <target> <artifact>` with `LOKI_ONLY`, `STOCK_SHARED_ORDINARY_PANIC`, `SECURITY_BLOCKER`, and `HARNESS_FAILURE` outcomes, and name the area 6 fuzz maintainer in workflow config with a two-business-day response deadline; verify invalid-proof and memory-safety samples remain blockers even if stock reproduces them, based on `LOKI-FUZZ.md:125-159` and `fuzz/diff/src/parse.rs:72-99`.
- [ ] 3.2 Document privacy/size filtering, upstream escalation, replay commands, weekly budgets, skipped-target failure behavior and sanitizer limitations in `LOKI-FUZZ.md` and `docs/modules/fuzz.md`; verify each CI artifact points to the documentation, based on `fuzz/run-target.sh:8-13` and `LOKI-FUZZ.md:178-186`.
- [ ] 3.3 Coordinate `.github/workflows/fuzz.yml` ownership and release invocation with docs-release; verify release aggregation requires SHA-keyed fuzz outputs and fails for missing/stale results, while the CPU gate records the target matrix without GPU availability, based on `fuzz/quick.sh:7-17` and `docs/MERGE-GATES.md:5-11`.
- [ ] 3.4 Validate the change with `openspec validate v1-fuzz-continuous --strict` and attach the passing result to the release evidence for the exact release commit.
