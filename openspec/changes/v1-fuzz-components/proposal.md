# Proposal

## Why

W7 already provides valuable stock 0.8.3 differential fuzzing, but its five targets concentrate on MSM, NTT, whole proofs and parsing. Loki v1 needs explicit coverage of transcript semantics, FFT, multiopen and lookup, along with regression replay that fits CPU-only WASM and mobile compatibility gates.

## What Changes

- Extend W7 with dedicated differential targets for proving/verifying key and parameter deserializers/transcript behavior, FFT, KZG multiopen, and lookup proof behavior; retain existing MSM, NTT, proof-roundtrip and soundness targets.
- Use the unmodified registry `midnight-proofs =0.8.3` crate as the independent oracle and exchange keys/proofs only as serialized bytes. Consume compat-proof's target-neutral `loki-harness/fixtures/v1/manifest.jsonl` and transcript vectors; its rows identify stock/Loki pins, fixture IDs, serialized artifact hashes and expected outcomes.
- Add deterministic corpus replay and small bounded structured inputs suitable for normal CI and cross-target CPU compatibility smoke gates. Keep `ParamsKZG::read_custom` input bounded at k ≤ 6 unless a separate safe allocation contract is approved (`LOKI-FUZZ.md:185-186`). WASM consumes `wasm:fixtures`; mobile consumes `mobile-compat/replay-corpus.sh --target <triple> --commit <sha>`; both execute the stock and Loki verifier on target through their platform-owned gates.
- Specify matched default and opt-in feature configurations and record any shared stock behavior as an upstream result rather than changing compatibility expectations.
- Trace the new target classes to Quint MBT and Lean contracts, and update `LOKI-FUZZ.md` and `docs/modules/fuzz.md` with commands, coverage, limits and evidence status. Separate ordinary shared upstream panics from security findings; stock parity does not clear invalid-proof acceptance or memory-safety findings.
- Require at least three named deterministic cases per target (positive, negative and boundary) and all six soundness mutation cases, so structured coverage is a checked minimum rather than a qualitative campaign claim.

## Capabilities

### New Capabilities

- `differential-fuzz-coverage`: Component-level fuzz targets and portable deterministic replay against stock 0.8.3 and mathematical oracles.

### Modified Capabilities

None.

## Impact

Planning targets `fuzz/Cargo.toml`, `fuzz/fuzz_targets/`, `fuzz/diff/`, and `fuzz/corpus/`. Relevant production surfaces include transcript framing (`src/transcript/mod.rs:47-79,96-149`), domain transforms (`src/poly/domain.rs:35-63`), multiopen polynomial work (`src/poly/kzg/multiopen.rs:13-24,100-149`), lookup (`src/plonk/lookup/prover.rs:44-75`), and the public lookup argument (`src/plonk/lookup.rs:10-36`). Existing W7 targets and the unpatched stock dependency are recorded at `fuzz/Cargo.toml:10-24,26-59` and `fuzz/diff/Cargo.toml:17-20`. This area pins its oracle to 0.8.3; any older-version matrix (including 0.8.1, which is absent from the local registry) is owned and sourced by compat-proof, not inferred from W7. This adds no production API and does not alter default features or wire formats. Dependencies: compat-proof owns shared fixtures; compat-api owns the compatibility feature matrix; Quint/Lean own their respective formal contracts; WASM/mobile areas own runtime gates; docs-release owns publication conventions.
