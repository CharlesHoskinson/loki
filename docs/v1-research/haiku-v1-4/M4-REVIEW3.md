# M4-REVIEW3: fresh review of openspec/changes/v1-mobile-cpu-compat (complete directory)

Read 2026-10-25: proposal.md 46 lines, design.md 49, tasks.md 35, specs/mobile-cpu-support/spec.md 68. `openspec validate v1-mobile-cpu-compat --strict` passes. No files edited.

## Previous findings: status
- Throughput formula: CLOSED. "sum(proof_time[samples 4 through 10 inclusive, 1-based])" in tasks.md:24 and spec.md:32. With 4..10 inclusive this is seven samples, consistent with `7 / sum * 60`.
- T68 client-circuit decision: CLOSED. spec.md:63-68 has a SHALL requirement and a WHEN/THEN scenario; task 3.5 (tasks.md:27) is measurable with fixture IDs, pass/fail, local-versus-delegated decision, egress boundary and evidence owner.
- Smoke-consumer size metric: CLOSED. `loki-mobile-compat-smoke` is defined in proposal.md:15, tasks.md:11 (task 1.5, path mobile-compat/smoke/), tasks.md:25 (metric), spec.md:32 and design.md:31.
- Host architecture, stock build on target, both prover directions, physical device, budget bootstrap, Rust 1.76 jobs, process-termination wording: CLOSED (from M4-REVIEW and M4-REVIEW2).
- Client-side candidates: CLOSED as above.
- Stock feature set: OPEN. tasks.md:16 says "unmodified midnight-proofs =0.8.3 registry package" and "record its target feature tree", and spec.md:13 and tasks.md:7 say "stock default features". Nothing says the stock build uses the stock default set. tests-stock/Cargo.toml:16 uses default-features=false (plus committed-instances at line 11), so the runner could silently build a different feature set. State the feature set once and use it for both packages.
- Anchors: OPEN (edit only). src/plonk/mod.rs:364-465 is still the single anchor in tasks.md:17, spec.md:21 and design.md:27. Narrow it to the ProvingKey impl block (bytes_length at 364, write 387, read 409, to_bytes 447, from_bytes 454; the impl ends after get_domain at 470). Name the impl, and match compat-proof's anchor.

## Remaining actionable findings
1. Stock feature set (see above). Must-fix before freeze, because it decides whether the on-target stock verifier is the same configuration as stock's default API surface.
2. Anchor narrowing (see above). Accuracy edit.
3. External claims: design.md cites the Android NDK, Mozilla UniFFI, Khronos Vulkan and Apple pages. The Android emulator-acceleration rule was checked by fetch (ARM system images cannot use VM acceleration on Intel or AMD hosts; the page does not say they cannot run at all). The others remain snippet-level in M4-PRACTICE. Mark them as snippet-level, or fetch them before freeze.

## Verdict
Only one must-fix remains (stock feature set). The change is strict-valid and the other earlier findings are closed.

## Final check for astra DONE (latest files: proposal 46, tasks 35, spec 68 requirements 7)
- Stock feature set: CLOSED. tasks.md:16 pins `committed-instances` with no `default-features=false`; tasks.md:17 and design.md:27 match.
- Anchor: src/plonk/mod.rs:354-370 and 372-465 now used (design.md:27); the 354-370 range is new and overlaps get_vk (359). Accept, but name the impl block in one place. Minor.
