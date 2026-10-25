# Design

## Context

See proposal.md for motivation. The current standalone `fuzz/` workspace compares the path package with unmodified registry 0.8.3 and shares byte-input check functions between libFuzzer and proptest (`fuzz/Cargo.toml:10-24`; `fuzz/diff/src/lib.rs:1-14`). Existing W7 targets cover MSM, NTT, proof construction/soundness and proof parsing. The transcript trait exposes `init_from_bytes`, challenge, read/write and `assert_empty` (`src/transcript/mod.rs:47-79`). The optimized multiopen and lookup code are separate surfaces.

## Goals / Non-Goals

**Goals:** Add focused differential target coverage, deterministic regression replay, byte-only stock interoperability and bounded generated test cases. Preserve feature/default parity. Make each target's formal and documentation trace explicit.

**Non-Goals:** Change production arithmetic, proof formats, transcript behavior, public API or default features; replace stock's normative behavior; run GPU as a required CPU/platform gate; claim fuzzing proves the Lean contracts.

## Decisions

1. **Keep a separate differential workspace and stock dependency.** The independent workspace prevents the root `[patch.crates-io]` from replacing stock. Keep shared fixtures as serialized bytes and coordinate the stable fixture manifest with compat-proof. Alternative: invoke stock through a separate subprocess; rejected because it obscures same-process cross-verification and raises fixture coordination cost.
2. **Share byte-driven check functions between libFuzzer and proptest.** Each target gets a focused parser/generator and small structured domain. Corpus entries are minimized but paired with named deterministic regression cases when semantics matter. Alternative: fuzz only whole proofs; rejected because it misses subsystem-local edge behavior and whole-proof generation is slow.
3. **Keep GPU runs opt-in.** CPU tests and cross-target corpus replays do not load CUDA. GPU campaigns retain the current explicit feature and shared-lock path. Alternative: require GPU CI for all arithmetic targets; rejected because the required platform gate must work without GPU access.
4. **Keep host fuzzing distinct from platform replay.** Native libFuzzer is the exploration engine. WASM/browser/Node and iOS/Android gates replay bounded committed corpus cases through portable checks where their owners provide runners; they do not need libFuzzer runtime linkage. Confirm exact runner commands with WASM/mobile owners before apply.
5. **Pair every target with formal artifacts and docs.** Quint MBT consumes named trace/case fixtures, Lean owners identify the corresponding mathematical contracts, and W7 docs state any unmodeled component as a gap. No fuzz result substitutes for a proof or model check.

## Risks / Trade-offs

- [Proof generation dominates execution] → Use bounded circuits and focused component generators, plus seed bytes that directly drive structure; report executed cases and coverage rather than equating elapsed time with coverage.
- [Shared stock defects look like failures] → Classify against stock 0.8.3 and preserve an upstream reproducer; any Loki-only outcome remains a release blocker.
- [Mobile/WASM support differs by runner] → Require the platform owner to name supported CPU corpus replay before making that runner a task gate; keep host fuzz corpus-format portable.
- [Large serialized inputs exhaust resources] → Bound parser size, k, allocations, timeout and RSS; preserve large parameter parsing as an explicit excluded surface until safe limits are specified.
