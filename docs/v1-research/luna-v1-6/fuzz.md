# Area 6: Fuzzing QA research

Snapshot: 2026-10-25, roadmap lab. Repo inspected read-only; no fuzz campaign or test was run for this proposal. OpenSpec strict validation was run on the two drafted changes.

## Current Loki state

- W7 already has five libFuzzer targets: `msm_diff`, `ntt_diff`, `proof_roundtrip`, `soundness`, and `proof_parse` (`fuzz/Cargo.toml:37-59`). It is a standalone workspace that compares the Loki path package with an unmodified registry `midnight-proofs =0.8.3` package, and moves keys/proofs across implementations as bytes (`fuzz/Cargo.toml:10-24`; `fuzz/diff/Cargo.toml:17-20`).
- The shared check functions are reused by proptests and corpus replay (`fuzz/diff/src/lib.rs:1-14`; `fuzz/diff/tests/quick.rs:39-98`). There are 2,448 checked-in corpus files across the five targets (scout inventory `docs/v1-research/haiku-v1-6/inventory.md`).
- `fuzz/quick.sh` runs default, `truncated` and `truncated-loki-only` CPU matrices. It measures 180 seconds but warns instead of failing on overrun (`fuzz/quick.sh:11-17,25-46`). GPU configurations are optional and locked (`fuzz/quick.sh:7-17`; `fuzz/gpu-slices.sh:1-16`).
- The long-run script pins `nightly-2026-10-11`, a 120-second per-input timeout, 8192 MiB RSS and 4096-byte max input, and sets `-len_control=0` (`fuzz/run-target.sh:8-13`). The W7 report documents prior short-input campaigns, incomplete GPU campaign, known shared upstream defects, `ParamsKZG::read_custom` bound k ≤ 6, and no Loki-only issue in those runs (`LOKI-FUZZ.md:99-123,125-159,178-186`).
- Component gaps: no transcript-level target; no direct proving-key deserializer target; multiopen and lookup are covered only indirectly through full proofs; no separate FFT target. Current parser target handles VK, verifier params, SRS and proof (`fuzz/diff/src/parse.rs:101-211`). Stock proving-key read/write is `src/plonk/mod.rs:368-415`; SRS custom read is `src/poly/kzg/params.rs:153-162`; transcript API is `src/transcript/mod.rs:47-79,96-149`.
- No `.github/` directory or fuzz workflow exists in this checkout. No WASM, Android or iOS fuzz config was found. This means fuzzing is manually invoked; the platform owners must own device/browser runtime gates.
- Open task T32 explicitly records structured proof/soundness input weakness (`TODO.md:131-133`): prior W7 runs remained at 4–6 byte fuzz inputs, sampling seeded cases instead of mutating structured bytes (`LOKI-FUZZ.md:105-111`). T38 might be stale: scout reports `cargo metadata --locked --no-deps` succeeds for `fuzz/`; I did not execute it or a build.
- W7 compares ordinary panics against stock as shared behavior (`fuzz/diff/src/parse.rs:72-99`). That is useful compatibility evidence but cannot clear invalid-proof acceptance or memory-safety findings; stock itself has known panics and a multi-proof committed/plain instance ordering defect (`LOKI-FUZZ.md:125-159`).

## Proposed roadmap outcome

`v1-fuzz-components` adds focused targets and deterministic case assertions while preserving the stock oracle and cross-target CPU scope. Every target maps to at least three named deterministic cases (positive, negative and boundary); soundness maps all six mutation kinds. The T32 acceptance also requires a 600-second soundness run to reach an accepted structured input over 64 bytes with a mutation ID distinct from the old 4–6 byte seeds. `v1-fuzz-continuous` adds a 180-second hard CPU quick gate, weekly campaigns of exactly 600 seconds per target (maximum 7,200 CPU-seconds), and a release-candidate campaign with exactly 600 seconds per target and 1,800 seconds for soundness (maximum 8,400 CPU-seconds for up to 12 targets). Both use the pinned nightly, 120-second timeout, 8,192 MiB RSS and 4,096-byte inputs; crash artifacts are retained for 90 days and triaged within two business days. Docs-release's release aggregation requires SHA-keyed campaign outputs and fails if evidence is missing or stale. The two changes separate test behavior from CI operations; continuous CI depends on the registered target list from the component change.

Fuzz is host-side input exploration. On WASM and mobile, the platform owners provide actual runtime compatibility and deterministic corpus replay gates; the release proof/key compatibility runs the unmodified stock side on target as required by platform reviews. No GPU job is required for CPU/WASM/mobile acceptance, but any shipped GPU backend requires area 5's differential and native sanitizer/safety evidence.

## Outside best practice and sources

Primary/upstream sources:

- LLVM libFuzzer flags include explicit maximum input size, timeout and RSS limits; `-len_control=0` lets fuzzing begin at the configured maximum size; corpus merge/minimize and individual-file replay are supported: <https://llvm.org/docs/LibFuzzer.html>.
- The Rust Fuzz Book documents structure-aware generation/mutation, including using `Arbitrary` and custom mutators to reach deeper structured behavior: <https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>.
- The Rust `cargo-fuzz` project documents nightly requirements, LLVM sanitizer/OS/architecture support, `cargo fuzz tmin`, `cmin`, and coverage: <https://github.com/rust-fuzz/cargo-fuzz>.
- OSS-Fuzz's Rust integration specifically expects cargo-fuzz and currently supports only the `libfuzzer` engine with the `address` sanitizer; its generic C/C++ sanitizer matrix must not be assumed to apply to Rust: <https://google.github.io/oss-fuzz/getting-started/new-project-guide/rust-lang/>.
- Rust's sanitizer support is unstable and target-specific; supported sanitizers listed by rustc include address, leak, memory and thread, but not UBSan in the current list: <https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html> and <https://rustc-dev-guide.rust-lang.org/sanitizers.html>.
- OSS-Fuzz's ideal integration guidance recommends checked-in maintainable targets, a high-coverage minimized seed corpus, continuous seed regression under sanitizers, and fast targets without OOMs: <https://google.github.io/oss-fuzz/advanced-topics/ideal-integration/>.
- OSS-Fuzz CI describes short pull request fuzz sessions, reproducer artifact upload, and a 600-second minimum for CIFuzz sessions: <https://google.github.io/oss-fuzz/getting-started/continuous-integration/>.
- OSS-Fuzz Rust integration currently documents `cargo fuzz` with `libfuzzer` and `address` sanitizer: <https://google.github.io/oss-fuzz/getting-started/new-project-guide/rust-lang/>. Do not assume its generic C/C++ sanitizer matrix applies to Rust. UBSan, MSan or TSan support must be probed against the pinned Rust toolchain/dependency graph before being promised.
- OSS-Fuzz disclosure policy publishes reported issues at 90 days or after fix release, whichever is earlier: <https://google.github.io/oss-fuzz/getting-started/bug-disclosure-guidelines/>. The proposed 90-day CI artifact retention is a separate repository policy, not derived from this disclosure deadline.

## Open interfaces

- Compat-proof owns the portable byte fixture schema and target-independent proof/PK/VK/SRS identifiers. Fuzz consumes fixture bytes and may add per-target minimized corpus cases; it does not define the shared fixture contract.
- Quint and Lean owners name exact model/replay artifacts per component. Existing Lean case mapping in the fuzz suite names MSM, NTT and transcript (`fuzz/diff/tests/cases.rs:1-2`). Fuzzers expose counterexamples but do not discharge proof/model obligations.
- WASM and mobile owners specify the commands that replay deterministic fuzz corpus items on their target runtimes. Native libFuzzer remains a host runner.
- Docs-release owns the workflow path, runner pools and release evidence layout; fuzz owns the target list, gate budget, red/green semantics and triage rule.

## Verification record

- Read: `context.md`, W7 and module docs, the fuzz manifests, target code, scripts, current stock source and compatibility harness.
- Proposed OpenSpec validation run: `openspec validate v1-fuzz-components --strict` and `openspec validate v1-fuzz-continuous --strict`; both pass after the follow-up duration, numeric coverage and release-manifest changes at 2026-10-25. The two specs contain 20 requirements total (10 per change).
- Not run: builds, tests, fuzz campaigns, sanitizers, GPU campaigns, platform target builds or formal model/proof checks.
