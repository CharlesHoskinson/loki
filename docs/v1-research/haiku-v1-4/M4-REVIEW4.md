# M4-REVIEW4: fresh read of openspec/changes/v1-mobile-cpu-compat (4 files) and research note

Read 2026-10-25: proposal.md 46, design.md 49, tasks.md 35, specs/mobile-cpu-support/spec.md 68. `openspec validate v1-mobile-cpu-compat --strict` passes. Research note: docs/v1-research/luna-v1-4/mobile-research.md. No files edited.

## Checks requested
1. Stock feature-set clarification: CLOSED. tasks.md:16 pins `committed-instances` with the stock default set and says not to set `default-features=false`. design.md:27 agrees.
2. ProvingKey source-anchor precision: CLOSED in part. Anchors now name the impl and give two ranges: `src/plonk/mod.rs:354-370,372-465 (ProvingKey impls)` in tasks.md:17 and spec.md:21. design.md:27 still lists the two ranges without the impl label. Minor: align design.md:27 with the same label.
3. compat-api MSRV lock command alignment: CLOSED. Tasks 1.1 and 1.4 and spec.md:13 use `cargo +1.76.0 check --locked --manifest-path compat/msrv/Cargo.toml --target <triple>` and the sibling `compat/msrv/Cargo.lock`. The root lockfile is not used. Note: this path is proposed, not present; tasks.md:3 says so for other paths but not for `compat/msrv/`. Add it to that note.
4. Throughput units, index range and comparator directions: CLOSED. spec.md:32 gives maximum RSS bytes, maximum artifact bytes, maximum per-proof wall time in seconds, and minimum sustained throughput `7 / sum(proof_time_seconds[samples 4 through 10 inclusive, 1-based]) * 60` proofs/min. tasks.md:25 gives the same formula but does not state the directions. Add "maximum" and "minimum" to tasks.md 3.2 or cite the spec.
5. Prior findings: closed as in M4-REVIEW3 (throughput, T68, smoke crate, stock feature set, host arch, stock build, both directions, physical devices, budget freeze then qualify, process termination).
6. Research note snippet-level marking: NOT CLOSED. mobile-research.md:31 marks only the Apple Xcode page as a search-result pointer. Lines 30 (Android NDK guide), 33 (UniFFI) and 34 (Khronos Vulkan) cite pages as sources without a snippet-level or fetched label. In this lab the Android NDK and UniFFI results came from search snippets only. The Android emulator-acceleration page was fetched and supports design.md:31. Mark each external claim in the note as fetched or snippet-level.

## Remaining actionable findings
1. Research note lines 30, 33, 34: add snippet-level or fetched labels (see 6).
2. tasks.md:25 (task 3.2) omits the max/min directions that spec.md:32 gives. Add them.
3. design.md:27 anchor label: match the impl label used in tasks.md:17 and spec.md:21.
4. tasks.md:3 "proposed new" note: add `compat/msrv/` to the list of paths that do not exist yet.

## Verdict
Strict-valid. No structural blocker. Items 1-4 are edits. Item 1 is the only one that affects evidence claims.
