# Proposal

## Why

Loki v1 must be usable as a drop-in replacement for stock `midnight-proofs` 0.8.3, including proofs, verifying and proving key encodings, transcript bytes, and challenges. The current differential harness gives useful CPU evidence but does not close the documented instance/batch gap (T03) or establish a shared, portable wire matrix.

## What Changes

- **Must-have:** define and enforce bidirectional proof and key serialization interoperability against an independent, unmodified stock 0.8.3 dependency, including checked read/write paths and transcript/challenge vectors.
- **Must-have:** extend W2 coverage across supported circuit families and k=10, 14, 17; plain and committed instances; single proofs and batch shapes. Explicitly classify the T03 mixed committed/plain multi-proof case and preserve the observed stock rejection until a protocol decision permits a wire-compatible change.
- **Must-have:** distinguish protocol interoperability from byte determinism. Require fixed-seed no-lookup proof equality; report lookup byte differences because stock leftover lookup placement has randomized `HashMap` iteration.
- **Must-have:** qualify default-path permutation and lookup product rewrites against stock 0.8.3 across randomized sizes, fixed seeds, and declared worker counts; compare arithmetic outputs and transcript/proof behavior under the existing determinism policy.
- **Must-have:** define one shared, target-neutral fixture and result schema, including fixture origin, for W2 and its fuzz, Quint/MBT, Lean, WASM, mobile, and performance consumers.
- **Nice-to-have:** broaden deterministic seeds/property cases beyond the required W2 matrix.
- Add proof/key/transcript malformed-input and wrong-instance negative controls, and retain source pins, commands, logs, and a machine-readable result manifest.
- Feed the same fixture schema to differential fuzzing, Quint/Rust MBT, Lean, and WASM/mobile CPU compatibility gates; document the supported matrix and its known limitations.

## Capabilities

### New Capabilities

- `proof-interoperability`: bidirectional stock 0.8.3 compatibility, wire fixtures, proof/key serialization, transcript/challenge behavior, the W2 instance/batch matrix, determinism policy, and rejection behavior.

### Modified Capabilities

None. The project has no existing OpenSpec capabilities (`openspec list --specs` returned none).

## Impact

Planning scope is the independent harness at `loki-harness/Cargo.toml:18-20`, its cross-verification and negative controls at `loki-harness/src/lib.rs:87-153`, its circuit at `loki-harness/src/backend.rs:45-195`, and its documented coverage at `loki-harness/README.md:1-10`. T03 is open at `TODO.md:15-17`; its observed mixed-instance counterexample and stock rejection are recorded at `LOKI-QUINT.md:120-166`. No production implementation is changed by this proposal.

Stock 0.8.3 remains the release oracle. Keep the reference crate unmodified and independently resolved; no Loki-only feature may change the default proof or wire behavior. The compatibility fixtures and CPU gate must be portable to WASM browser/Node, WASM single-threaded/threads, iOS, and Android, with GPU acceleration excluded from this gate. The independent 0.8.1 and k20 T04 lane is specified separately in `v1-compat-proof-stock-matrix`.

QA for each behavior includes differential tests and malformed-input fuzzing, Quint models with Rust MBT replay for transcript/order and batch cases, and Lean event-equality coverage with the T03 counterexample retained. Documentation must state the exact support matrix, version pins, deterministic versus randomized lookup comparison policy, and runnable acceptance commands. These commands are proposed gates, not claims of completed runs.

Risks include lookup proof byte differences caused by stock nondeterminism and the inherited T03 transcript-order defect. The compatibility owner owns W2 fixture/schema and runner contracts. Dependencies: compat-api for default-feature assumptions; fuzz, Quint, Lean, and the stock-matrix change for shared fixture consumption; WASM/mobile for CPU gate execution; docs-release for publication. GPU and performance gates remain with their owners.
