# cp-practice: differential, serialization, transcript and version-matrix practice (haiku-v1-2)

Method: three WebSearch queries (standard mode). Claims below are from search-result text and link titles; the pages were not fetched in full. Mark each as search-verified.

## Sources and what they support

1. Wycheproof (C2SP): community-maintained JSON test vectors with expected results, negative (invalid) cases included, meant to run in CI against each library's API. Authoritative repo: https://github.com/C2SP/wycheproof . Third-party summary: https://trailofbits.com/skills/wycheproof/ . Supports: negative-case vectors as a standard practice; fixed expected-result schema; CI integration. Limit stated in the summary: Wycheproof does not test timing side channels and finds no unknown bugs, so it complements fuzzing rather than replacing it.

2. Ethereum execution-specs external KZG vectors: point-evaluation precompile tests run against external vectors, including a set from go-kzg-4844. https://steel.ethereum.foundation/docs/execution-specs/tests/cancun/eip4844_blobs/test_point_evaluation_precompile/test_external_vectors/ . Supports: cross-implementation KZG verification against a second codebase's vectors. Caveat: this is EIP-4844 verification, not PLONK proofs.

3. Microsoft Vega prover transcript documentation: Keccak256 transcript with new, absorb, squeeze and dom_sep operations; the spec defines byte-level behaviour, and serialization for scalars and points is a separate chapter. https://microsoft.github.io/vega-prover/building-blocks/transcript.html . Supports: specify the transcript at byte level so independent implementations can share vectors; any change to labels, encodings or squeeze positions changes all later challenges.

4. ark-transcript crate docs: explains postfix length framing versus Merlin's prefix length encoding, because serialization conventions break interoperability between ecosystems. https://docs.rs/ark-transcript . Supports: transcript framing must be pinned explicitly in a compatibility spec and tested.

5. Trail of Bits Decree (Merlin-based): enforces that each transcript input is specified exactly once. https://github.com/trailofbits/decree . Supports: a mechanical check that Loki's transcript absorb list matches stock's list (see recommendation 3).

6. Firo Lelantus changelog: transcript fixes that followed gaps: domain separator on every transcript, statement and public-key binding, transcript version number. https://forum.firo.org/t/lelantus-disabled-temporarily/1486/37 . Supports: include a version or domain separator in any negative-control matrix, and test transcript-binding failures.

7. Search did not find a public test-vector suite for PLONK/KZG proof interoperability between two implementations of the same protocol. Stated as a gap; the W2 harness is currently the only cross-implementation evidence for Loki.

## Actionable recommendations for compat-proof

1. Negative vectors: build a Wycheproof-style JSON table (valid, invalid, acceptable) for proof and VK bytes: truncation, trailing byte, bit flips at fixed offsets, wrong public input, swapped VK, empty proof, and length-mismatched multi-proof inputs. Each row has an expected result that both verifiers must match. Keep 0.8.1-specific rows separate (see cp-stock).
2. Serialization: add SerdeFormat matrix rows (`RawBytes`, `RawBytesUnchecked`, `Processed`) for VK, PK, SRS and verifier params, with a cross read in both directions and a PK round trip. Currently none of these except RawBytes and Processed are exercised (cp-code gaps).
3. Transcript: pin the absorb and squeeze sequence in a written spec (the Vega approach), and check it mechanically against the stock source list. Add a Blake2b256 round-trip case (stock `transcript/implementors.rs:41-45`). This is also the Quint MBT target (quint owner, area 7).
4. Version matrix: run every positive vector under 0.8.1, 0.8.2 and 0.8.3 verifiers. Expected deltas are only the 0.8.2 `InvalidInstances` rejection and no wire change (cp-stock).
5. Domain separation: give each harness fixture an explicit label recorded in the vector file, so a silent change of fixture label is visible in diffs.
6. Keep external-vector sourcing honest: record the source commit or URL for every vector set imported, as go-kzg-4844 does, and do not claim a vector set covers PLONK unless it does.

## Unverified

- Contents and exact coverage of the Wycheproof repo and the execution-specs vector files were not read; claims are from search summaries.
- Vega and ark-transcript pages were not fetched; the framing descriptions are from search snippets.
