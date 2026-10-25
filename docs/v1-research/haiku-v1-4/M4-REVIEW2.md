# M4-REVIEW2: fresh review of openspec/changes/v1-mobile-cpu-compat (haiku-v1-4)

Read on 2026-10-25: proposal.md 45 lines, design.md 49, tasks.md 34, specs/mobile-cpu-support/spec.md 61. `openspec validate v1-mobile-cpu-compat --strict` passes. No files edited.

## Verified in this pass
- Host architecture is named: iOS jobs on Apple Silicon macOS (tasks 1.1, spec:13); ARM64 Android emulator only on an ARM64 host or a physical device (tasks 1.2, design:31, spec:13,20).
- Android source claim checked by fetch of https://developer.android.com/studio/run/emulator-acceleration: the page says ARM-based system images on Intel or AMD CPUs "can't use the VM acceleration" described there. This supports the design:31 claim. The page does not say ARM images cannot run at all, so the spec should not say they cannot run.
- Stock build task (tasks 2.2) and on-target stock dependency: tests-stock/Cargo.toml:16 is `stock = { package = "midnight-proofs", version = "=0.8.3", default-features = false }`. Tasks 1.4 and 2.2 cite tests-stock/Cargo.toml:15-19 and this is in range. Note: tests-stock disables default features and adds `committed-instances` explicitly (tests-stock/Cargo.toml:11-12). The task says "stock default feature set". Harmonise these, because the mobile build must match the stock default set, not the tests-stock override.
- Lockfile: Cargo.lock:3 is `version = 3`, so Rust 1.76 can parse it. Good.
- Physical device requirement is present (spec:23-25; tasks 3.1, minimum one low and one mid phone per OS).
- Budget bootstrap is defined: freeze run (tasks 3.2, spec:64-66) versus qualify run (tasks 3.3, spec:68-70). Non-circular.
- Process termination is not ContextLost (spec:90-95, design:37).

## Remaining actionable findings
1. Throughput formula still ambiguous (not fixed). tasks.md:24 and spec.md:66 say `7 / sum(proof_time[4..10]) * 60`. With 0-based half-open slicing, [4..10] covers 6 samples. Use 1-based inclusive samples 4 through 10 (7 proofs), and state that convention in both places.
2. T68 client-circuit decision still has no spec requirement. Task 3.5 exists, but spec.md has no requirement or scenario for it, and T88 (roadmap line ~234) makes it required for mobile and WASM. Add a requirement with a WHEN/THEN scenario, or drop T68 from this change.
3. Stock feature set mismatch. Task 2.2 and spec:17 say "stock default", but tests-stock/Cargo.toml:16 disables default features. State explicitly which stock feature set the mobile runner builds, and match the compat-api stock-default matrix.
4. Smoke-consumer artifact is undefined (tasks 3.2, design:31). Name the crate or app path (proposed new), so that size has one artifact definition across devices.
5. Tasks 2.3 and spec:21 cite `src/plonk/mod.rs:364-465` for PK, VK and verifier-parameter serialization. The PK API is at 387-470 and bytes_length at 364 (verified in review 1). Narrow the range or name the impl block (ProvingKey versus VerifyingKey). The change also needs one shared anchor set with compat-proof.
6. Unpinned and unresolved external claims: design:23 and design:55-59 cite Android NDK, Mozilla UniFFI and Vulkan pages. The NDK and Vulkan claims were only seen in search snippets in this lab. Mark them as "snippet-level" in the notes, or fetch them before the spec freezes.

## Verdict
Most of review 1 is resolved. Items 1 and 2 remain must-fix before freeze, as flagged in re-review 2 (M4-REVIEW.md). Items 3 to 6 are accuracy and consistency edits.

## Re-check after luna-v1-4 ANSWER (mobile findings closed)
- Item 1 (throughput indexing): FIXED. tasks.md:24 and spec.md:32 now read "samples 4 through 10 inclusive, 1-based".
- Item 2 (T68): FIXED. Spec has requirement "Client-side circuit and witness-data limits are explicit" (spec.md:63-67) and task 3.5 (tasks.md:27).
- Item 3 (stock feature set): OPEN. tasks 1.1 and spec:13 say "stock default features"; no statement that the runner matches the stock default set rather than tests-stock's default-features=false (tests-stock/Cargo.toml:16).
- Item 4 (smoke-consumer artifact): OPEN. Still undefined in tasks.md:24 and spec.md:32.
- Items 5-6 (anchors and external-claim status): OPEN, edits only. src/plonk/mod.rs:364-465 still used in design.md:27, spec.md:21, tasks.md:16.
- `openspec validate v1-mobile-cpu-compat --strict` still passes (checked after the edit).
