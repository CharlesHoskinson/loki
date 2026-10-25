# `src/transcript`: Fiat-Shamir transcript

## Purpose

The transcript defines the sequence of absorbed and squeezed values. Prover and verifier need matching sequences for their supported input shapes, or their challenges differ and verification fails. Cross-verifier acceptance is documented for tested inputs and configurations in [loki-harness](loki-harness.md).

## Public API

[`mod.rs`](../../src/transcript/mod.rs):

- `TranscriptHash` (`:15`): `init`, `absorb`, `squeeze`. `Hashable<H>` (`:30`) and `Sampleable<H>` (`:42`) convert values to hash input and challenges.
- `Transcript` trait (`:48`): `init`, `init_from_bytes`, `squeeze_challenge`, `common`, `read`, `write`, `finalize`, `assert_empty`. `common` hashes without writing to the proof. `write` does both. `assert_empty` rejects trailing proof bytes.
- `CircuitTranscript<H>` (`:83`) with `buffer()`.
- `Blake2b256` ([`implementors.rs:41`](../../src/transcript/implementors.rs)): the default hash. It absorbs into a byte buffer and `squeeze` replaces it with the 32-byte BLAKE2b digest. `Hashable` is also implemented for `u32` and the curve and field types.

The recursion circuits in the Sherlock stack use a Poseidon `TranscriptHash` defined outside this crate. The `probes/a3-stock-verify` probe checks the full Poseidon transcript is consumed.

## Difference from stock midnight-proofs 0.8.3

None. Both files are identical to the registry source. Loki leaves the transcript implementation unchanged and preserves the prover's order for the tested supported cases. The multi-proof mixed committed/plain instance case below is an inherited exception.

## Invariants and pitfalls

- The Quint model `quint/transcript.qnt` lists the stock event sequence: common VK and instance absorptions, advice challenges per phase, `theta`, permuted lookup commitments, `beta`/`gamma`, all permutation products then all lookup products, trash, vanishing random commitment, `y`, quotient commitments, `x`, evaluation groups, then `multiopen_x1`, `x2`, `f`, `x3`, q evaluations, `x4`, `pi`. This summary abstracts the per-proof instance ordering. Quint replays the listed events against real proofs.
- In a batch of multiple proofs that includes both committed and plain instance columns, the prover absorbs columns per proof (`src/plonk/prover.rs:467-494`). The verifier absorbs all committed columns before all plain columns (`src/plonk/verifier.rs:63-75`). Stock therefore rejects its own proof for this shape, as recorded in `LOKI-QUINT.md:161-165`. Loki preserves the inherited behavior.
- By default, Loki and stock both use `HashMap` iteration order for leftover lookup rows. The opt-in `loki-deterministic-lookup-order` feature sorts them by value. This affects proof bytes without changing validity; W3's deterministic lookup measurements used source revision `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16`, where sorting was always enabled (`LOKI-CPU.md`).
- `assert_empty` rejects appended proof bytes. The harness checks this in both cross-verification directions.
- Challenge truncation (`truncated-challenges`) is a compile-time switch that must match.

## Tests

[quint](quint.md) `transcript_driver` replays 24 model traces against real k5 proofs with wrapped and plain transcripts. [loki-harness](loki-harness.md) requires full transcript consumption. [fuzz](fuzz.md) `proof_parse` feeds arbitrary bytes to the parser.

## Notes

[`LOKI-QUINT.md`](../../LOKI-QUINT.md), [`LOKI-FUZZ.md`](../../LOKI-FUZZ.md).
