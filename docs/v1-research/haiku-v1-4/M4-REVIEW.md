# M4-REVIEW: review of openspec/changes/v1-mobile-cpu-compat (haiku-v1-4)

Validation: `openspec validate v1-mobile-cpu-compat --strict` -> "Change 'v1-mobile-cpu-compat' is valid". No files edited.

## Verified citations (correct)
- Cargo.toml:13-16 edition 2021, rust-version 1.76.0, version 0.8.3 (matches proposal).
- src/lib.rs:14-18 compile_error for loki-gpu-backend vs cuda; src/lib.rs:9 deny(unsafe_code); src/lib.rs:20-30 module list.
- src/utils/multicore.rs:1-7 Rayon re-exports.
- src/plonk/prover.rs:397-447 create_proof (spec cites 397-448, acceptable).
- docs/PLATFORMS.md:264-316 mobile section; 402-435 summary and open questions; O7 at 429-431.
- docs/modules/fuzz.md exists (45 lines); loki-harness/README.md exists (158 lines, 71-89 in range); docs/ARCHITECTURE.md (360 lines, 56-61 in range); tests-stock/Cargo.toml (19 lines, 7-19 in range).

## Findings (numbered)
1. CRITICAL, emulator architecture. Spec SHALL-runtime requirement (spec.md:127-132) and task 2.2 run an aarch64-linux-android artifact on an "Android emulator", and the spec calls for an iOS simulator run of aarch64-apple-ios-sim. An arm64 Android emulator needs an arm64 host (believed: Apple Silicon Mac or arm Linux). A GitHub-style x86_64 Linux runner can only run x86_64 images, which would test a different Rust target. Require the runner spec to name the host arch: Apple Silicon macOS for both iOS simulator and an arm64 Android emulator, or a physical arm64 device. (UNVERIFIED against Android emulator docs; cite before spec freeze.)
2. No stock build task for mobile. Task 2.2 runs "unmodified registry stock 0.8.3" on target, but no task compiles midnight-proofs 0.8.3 for aarch64-apple-ios, -ios-sim or -linux-android. Stock has no ios/android cfg (registry Cargo.toml:172-178 only gates wasm32 getrandom and ci_build). Add a task that builds the stock package with the same NDK/Xcode pins and records its feature tree (blst build is the same risk as Loki).
3. Direction is underspecified on-device. Spec.md:131-132 says stock verifies Loki proofs and Loki verifies stock proofs on the device. It is not stated which side proves on-device. Require: Loki proves and stock verifies, and stock proves and Loki verifies, each on the target, or state that the stock prover is host-only and the on-device stock check is verification-only. Decide and name it.
4. Device-side iOS and Android compatibility is missing from the requirement. Astra's review (REVIEW PROPOSAL mobile) and luna-v1-4's own proposal call for real-device spot-checks for runtime compatibility. The spec runs compat only on simulator/emulator. Physical devices appear only in the budget task (3.2). Add a physical-device compat run per OS, or record the exception.
5. Measurability of "sustained-throughput". Spec.md:135 and 139 require sustained-throughput limits, and task 3.2 says "ten consecutive proof timings/thermal behavior", but no metric is defined. Define it: e.g. ratio of the last proof time to the first, a max drift threshold, plus recorded device thermal state (OS thermal level or temperature source). Define peak RSS source (phys footprint vs max RSS) and library-size measurement (stripped .a vs .so, per target).
6. Circularity of budgets. Requirement at spec.md:134-135 says absent thresholds fail the gate, and task 3.3 says budgets freeze from measurements. That is consistent, but the scenario should say the first release run freezes budgets from a named measurement set and later runs gate against them. As written, the first run has no pass/fail path. Add a bootstrap rule.
7. MSRV not checked on mobile. Tasks 1.1 use the local toolchain implicitly. rust-version 1.76.0 (Cargo.toml:14) must be checked on the three targets, or the task must state that MSRV is verified only on x86_64. Add a 1.76 cargo check job.
8. Citation range drift. design.md:60 and spec.md:132 cite src/plonk/mod.rs:372-465 (PK write/read/to_bytes/from_bytes). In the file, bytes_length for the PK is at 364, write 387, read 409, to_bytes 447, from_bytes 454, get_domain 470. The range should be 364-470 and the impl owner named (the pk impl starts after get_vk at 359). Also luna-v1-2 cites 359-431 and 372-457 for the same API; align on one anchor set.
9. CI host not named. Tasks say "CI" and "runner" without naming an OS/arch. loki-proofs has no .github/ directory (verified), so the workflow file does not yet exist. Name the host for each lane (Apple Silicon macOS for iOS and arm64 Android; the compile lanes can be Linux).
10. Interruption requirement is fine but depends on injection. spec.md:155-160 relies on `--inject-terminate-during-prove`, which does not exist. Mark as a new runner flag (already implied by task 2.5). Accepted with that note.
11. Scope consistency with research. docs/v1-research/luna-v1-4/mobile-research.md is present (42 lines). The M4-STOCK recommendation (device-produced bytes verified on host, option A) is a cheap complement and is not mentioned in the spec. Add it as the fallback gate when the on-device stock build is blocked, or state why it is rejected.
12. New paths are proposed, not present. mobile-compat/run.sh, measure.sh, devices.toml, budgets.toml, scripts/check-mobile-docs.sh, and loki-harness/fixtures/v1/manifest.jsonl do not exist yet. Mark them as new in tasks.md so the release checker does not look for them as existing.

## Priority
Fix 1, 2, 3, 4 before spec freeze. 5 and 6 block budget tasks. 7 and 9 are CI-gate gaps. 8, 10, 11, 12 are accuracy edits.

## Re-review (proposal.md revised, now 45 lines; design/tasks/spec unchanged at 49/29/49)
- Proposal now splits must-have, nice-to-have and out-of-scope, and adds a risks list. It resolves the earlier scope ambiguity.
- Findings 1-4, 6, 7 and 9 remain open. They live in spec.md and tasks.md, which were not changed. Finding 4 is partly addressed by proposal line 14 ("named low- and mid-range phone matrix") but still has no physical-device compatibility run.
- New issue: proposal line 13 says "simulator/emulator" gate with no host arch. Same as finding 1.
- Proposal line 43 says fuzz adds "deterministic seed replay on both mobile runners", consistent with tasks 2.3.
- Proposal line 44 names the platform guide for FFI and GPU deferrals and interruption behavior. Consistent with spec.md:148-156.

## ROADMAP-V1.md R1 mobile review (astra-v1 request)
Source: ~/repos/loki-wt/v1/openspec/ROADMAP-V1.md (255 lines). Lines cited below.
1. Missing T68 task (client-side circuit envelope, O8). Roadmap row 214 (line ~214) assigns T68 to v1-mobile-cpu-compat as "freeze client workload/data boundary", and T88 (line ~234) makes it a required mobile+wasm decision. The change has no task for it (tasks.md has none). Add a task or change the roadmap disposition.
2. T72 reinterpretation is silent. TODO.md:291 says "confirm clean ContextLost handling". Roadmap line 218 maps it to "CPU process/OOM artifact integrity; GPU ContextLost path deferred". Record the rewritten acceptance text in the TODO ledger, so the old wording is not counted as satisfied.
3. T71 requires "G6 reported as sustained throughput" (roadmap line 217). The spec does not define sustained throughput (see M4-REVIEW finding 5). Blocker for T71 disposition until defined.
4. Physical devices in M2 (line 70). M2 needs iOS device/simulator and Android runtime gates plus physical-device observations in 3-6 weeks. Nothing in this lab has an NDK, Xcode, macOS runner or phones. The roadmap's own risk row (line 126) names runner availability, but the M2 duration does not reflect procurement. Flag the schedule as an estimate that depends on hardware.
5. Emulator architecture (see M4-REVIEW finding 1). M2's "Android runtime stock gate" needs an arm64 host or device for an aarch64-linux-android emulator. Name the host in the target runner matrix (roadmap M0 says "target runner matrix" is frozen, so this belongs there).
6. Line 13 names iOS device aarch64-apple-ios as first-class. The spec's runtime gate covers only the simulator (spec.md:131). Either add a device runtime lane or downgrade the device to "budget evidence only" in line 13.
7. Accepted: deferrals for T69, T70, T86 (lines 215-216, 232) match proposal out-of-scope and GPU deferral. No conflict found.
8. Accepted: MSRV 1.76 and the 0.8.3 patch-artifact identity (line 9) match astra's ANSWER release identity. The mobile change must still add the 1.76 check (M4-REVIEW finding 7).
Decision: no ACK yet. Items 1, 3 and 6 need an edit before I ACK.

## Re-review 2 (mobile revisions by luna-v1-4; proposal 45, design 49, tasks 34, spec 61; strict valid)
Resolved from the earlier 12: emulator host arch (tasks 1.2, spec:47,54), stock build task (2.2), proof directions (spec:55), physical-device compat (spec:57-59), throughput and RSS/size definitions (3.2, spec:66-70), freeze-then-qualify (3.3), Rust 1.76 jobs (1.1, 1.4, spec:47), PK anchors (2.3, spec:55 cite 364-465), new paths labelled (tasks.md:3), termination not ContextLost (spec:91). Lockfile is version 3 (Cargo.lock:3), so cargo 1.76 can parse it.
Remaining:
1. Throughput formula indexing is ambiguous. `7 / sum(proof_time[4..10]) * 60` (tasks.md:24, spec:66). With 0-based half-open slicing [4..10] covers 6 samples, not 7. Write it as 1-based inclusive samples 4 through 10 (7 proofs), or change the numerator to 6. Pin the convention in the spec, since the freeze and qualify runs must agree.
2. T68 client-circuit decision has a task (3.5) but no spec requirement or scenario. Under the OpenSpec rules, each SHALL needs a requirement. Add a requirement, or move the decision to the platform guide and cite it.
3. design.md was not re-read in this pass (line count unchanged at 49). Confirm no edits since M4-REVIEW.
4. Smoke-consumer artifact for size (tasks.md:24) is not defined. Name the consumer crate path (proposed new) so the size metric has one artifact.
Verdict: conditionally acceptable. Items 1 and 2 are needed before freeze.

Re-review 2 detail is in M4-REVIEW2.md (same directory). Status: items 1 and 2 still open.
