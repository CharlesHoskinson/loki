# TASK fuzz-compat: stock 0.8.3 surface relevant to fuzzing, and the gaps

Read-only, 2026-10-25. Stock oracle: `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/midnight-proofs-0.8.3` (the `=0.8.3` pin in `fuzz/diff/Cargo.toml:18`). Loki: `~/repos/loki-proofs`.

## Stock serialization surface (what fuzz must cover)
- `SerdeFormat` enum: `src/utils/helpers.rs:16` (Processed; `:20-24` RawBytesUnchecked and the others). The parse fuzzer maps the format byte at `fuzz/diff/src/parse.rs:64` (`format(b)`).
- SRS: `src/poly/kzg/params.rs:136` `write_custom`, `:154` `read_custom`, `:251` `write`, `:257` `read`. `read_custom` is the unbounded-allocation path (2^k points from a header), so fuzzing it is limited to k <= 6 (`LOKI-FUZZ.md:185-186`).
- VK: `src/plonk/mod.rs:95` `write`, `:124` `read`, `:152` `read_from_cs`. The `read_from_cs` index bug (no check of `num_fixed_columns` against the constraint system) is upstream finding 3 (`LOKI-FUZZ.md:138-143`). Stock and Loki both hit it.
- Proof and PK-level `write`/`read` at `src/plonk/mod.rs:383` and `:405`. NOT verified whether the proving-key read is fuzzed directly; `parse.rs` covers VK, verifier params, SRS and proof (`parse.rs:1-6`).
- Transcript: `src/transcript/mod.rs:83` `CircuitTranscript`, `:91` `buffer`; hash in `src/transcript/implementors.rs:41` (`Blake2b256`). Loki-side transcript at `src/transcript*` is covered only through `proof_parse` and `proof_roundtrip` (`docs/modules/transcript.md:32`).

## Existing stock differential gates
- Stock cross-verify harness: `loki-harness/src/lib.rs:87` `compare<LOOKUPS>` (SRS bytes `:99-101`, VK and verifier params `:107-111`, cross-verify `:115`, wrong-input `:120-123`, empty proof `:129-132`, trailing bytes `:140-145`), `:167` `cross_verify`, `:172` `determinism`, `:194` `negative_controls`. Stock dep is `=0.8.3` (`loki-harness/Cargo.toml:20`).
- Registry verifier check: `tests-stock/tests/verify.rs` reads `target/cpu-fixture/{proof,vk,params}.bin` and runs the unmodified stock verifier. It is a single fixture with one corrupted-byte check.
- Fuzz diff: `fuzz/diff/src/proof.rs:349-405` (both provers, four-direction verdicts), `proof.rs:411+` soundness; `fuzz/diff/tests/cases.rs` with explicit Lean-contract cases.

## Gaps for the compatibility surface
1. Stock-version matrix: the fuzz harness is pinned to 0.8.3 only (`fuzz/diff/Cargo.toml:18`). The local registry has `midnight-proofs-0.7.2`, `0.7.3`, `0.8.2`, `0.8.3`, but no 0.8.1. The compat-proof area asks for 0.8.1 and 0.8.3; 0.8.1 must be fetched before it can be tested.
2. Transcript-level fuzzing: no target feeds arbitrary bytes into `CircuitTranscript::init_from_bytes` and compares stock/Loki accept/reject (only indirect, via `proof_parse`).
3. Byte-level interchange in both directions is checked for generated cases (`proof.rs:358-405`), not for stored external fixtures from other stock versions.
4. Trailing bytes are accepted by the pairing check and caught only by `assert_empty` in both crates (`LOKI-FUZZ.md:~145-148`). The v1 spec should state this as the acceptance rule.
5. Known upstream asymmetry: multi-proof committed/plain instance ordering is rejected by both (`LOKI-FUZZ.md:127-134`). The fuzz oracle treats it as expected-reject (`circuit.rs:282`). Any v1 "accept what stock accepts" rule must exclude this shape explicitly.
6. `ParamsKZG::read_custom` cannot be fuzzed at large k without an explicit input bound (`LOKI-FUZZ.md:185-186`).

## Verification status
- Read: stock source at the paths above; Loki and harness source at the cited lines.
- NOT run: any build, test or fuzz campaign. No 0.8.1 source checked.
