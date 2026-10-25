# cp-final: re-review of split compat-proof changes (haiku-v1-2)

Scope: v1-compat-proof-wire-matrix and v1-compat-proof-stock-matrix after luna-v1-2's split.

Strict validation (run now):
- `openspec validate v1-compat-proof-wire-matrix --strict`: valid.
- `openspec validate v1-compat-proof-stock-matrix --strict`: valid.

Earlier wire-matrix warning (one requirement over 500 characters, "VK, PK, and verifier parameter encodings SHALL interoperate") no longer appears in the strict output.

Prior review items checked against the new drafts:
- Finding 6 (differential.rs range): fixed in wire-matrix (`differential.rs` is 32 lines; spec cites within range). NOT fixed in stock-matrix, which still cites `loki-harness/tests/differential.rs:14-35` (see new finding A).
- Finding 4 (292 s run.sh budget for k20): not addressed in either change. Stock-matrix uses a new `./compat-proof-stock/run.sh --k 20 --serial` that does not exist yet; its timeout is unstated (see new finding C).
- Finding 5 (missing --release): fixed in wire-matrix (`--release` present in the bidirectional scenario).
- Finding 3 (transcript scenario verified against Quint only, not stock): fixed. Wire-matrix now names stock-generated transcript vectors as the oracle via `transcript_vectors_match_stock`.
- Finding 1/2 (undefined declared/supported sets): partly fixed. Wire-matrix defines 1..3 proofs and committed-prefix 0..2 shapes. Still open for "declared circuit family" (see new finding B).
- Finding 9 (aarch64-apple-ios vs simulator): fixed; the iOS simulator triple is named.

Verified citations:
- `loki-harness/src/lib.rs:98-103` (stock SRS roundtrip): correct.
- `loki-harness/src/backend.rs:202-215` and `:268-303` (setup/verify): in range.
- Stock `SerdeFormat::RawBytesUnchecked` exists at `midnight-proofs-0.8.3/src/utils/helpers.rs:27`; the spec's claim is accurate. The harness never uses it (matches the gap in cp-code).
- Stock `ProvingKey::read` at `midnight-proofs-0.8.3/src/plonk/mod.rs:405-427`: correct.
- `quint/transcript_instances_test.qnt` exists.

New findings:
A. stock-matrix scenario "T04 k20 acceptance" and its verification still cite `loki-harness/tests/differential.rs:14-35`. The file is 32 lines. Use `1-32`.
B. "Declared circuit family" appears in the W2 k-matrix requirement of wire-matrix and in the stock-matrix T04 scenario. No enumerated list of families exists in the repo, so the requirement is not testable. Name the families or cite a manifest file that lists them.
C. `./compat-proof-stock/run.sh` and its flags (`--versions`, `--serial`, `--k`, `--negative`) do not exist yet. The scenarios present them as runnable. Mark them as to-be-created in the spec or design. Also state a timeout for the k20 run: the existing `loki-harness/run.sh` hard-caps at 292 s including compilation (`run.sh` last line), which k20 will likely exceed. The new runner must set its own budget or detach k20 from the wrapper.
D. stock-matrix has no `#### Scenario` for the mixed committed/plain T03 row. It is covered in wire-matrix, so this is only a cross-reference gap. Add a pointer, or note that T03 is wire-matrix-only.
E. stock-matrix Requirement "Version-specific malformed multiproof behavior" cites `docs/v1-research/luna-v1-2/compat-proof.md` as the source of the 0.8.1 vs 0.8.2 diff. That note exists, but the primary source is stock `plonk/verifier.rs` (0.8.1 has no `InvalidInstances` guard; 0.8.2 adds it). Cite the stock file directly.
F. stock-matrix "Stock 0.8.1 SHALL have a separate locked lane" should state the acquisition URL: `https://static.crates.io/crates/midnight-proofs/midnight-proofs-0.8.1.crate` returned HTTP 200 in my check. The local registry does not contain 0.8.1.

Unverified in this review: no harness or cargo command was run. Command existence was checked by filesystem lookup only (`compat-proof-stock/` does not exist; `run.sh` and `transcript_vectors_match_stock` are proposals).
