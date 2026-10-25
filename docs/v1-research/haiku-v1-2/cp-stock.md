# cp-stock: stock midnight-proofs 0.8.1 / 0.8.2 / 0.8.3 interface and format deltas (haiku-v1-2)

Sources (primary):
- 0.8.1 crate: `https://static.crates.io/crates/midnight-proofs/midnight-proofs-0.8.1.crate` (HTTP 200; extracted read-only under the scratchpad, diffed only, not built).
- 0.8.2 and 0.8.3: `~/.cargo/registry/src/*/midnight-proofs-0.8.{2,3}/` (local registry copies).
- Changelog entries (in-crate `CHANGELOG.md`): 0.8.1 cites midnight-zk PR #434; 0.8.2 cites #482; 0.8.3 cites #536. PR URLs are of the form `https://github.com/midnightntwrk/midnight-zk/pull/<n>`. I did not fetch them; the claims below rest on the local source diffs.

## Source diffs

- 0.8.1 vs 0.8.3, `src/` differing files: exactly two. `plonk/verifier.rs` and `poly/kzg/msm.rs`.
- 0.8.2 vs 0.8.3, `src/` differing files: exactly one. `poly/kzg/msm.rs`.
- 0.8.1 vs 0.8.2: exactly one. `plonk/verifier.rs`.

### plonk/verifier.rs (0.8.1 to 0.8.2, changelog #482)

0.8.2 adds, before reading vanishing commitments, a length check that returns `Error::InvalidInstances` when `committed_instances`, `advice_commitments`, `permutations`, `lookups` or `trashcans` lengths differ from `num_proofs`. 0.8.1 has no such check. The check does not change any accepted-proof bytes; it only rejects malformed multi-proof inputs. Loki's `src/plonk/verifier.rs` is identical to 0.8.3 (not in the Loki-vs-0.8.3 diff list), so Loki includes the check.

### poly/kzg/msm.rs (0.8.1 to 0.8.3, changelog #536)

0.8.1 uses blst Pippenger for BLS12-381 G1 only when `coeffs.len() <= (2 << 18)`, otherwise `msm_best`. 0.8.3 always uses blst for G1. This is a performance change; the group result is the same MSM. Loki's `msm.rs` default path follows 0.8.3 (asserted by `tests/backend_default_stock.rs:141`, `default_msm_equals_stock`).

### Serialization, transcript, challenge derivation

None of the three versions change VK/PK/SRS serialization, transcript absorption order or challenge derivation. The diffs above are the complete set for `src/`. Not checked: `Cargo.toml` feature sets beyond `default` (diffed only for 0.8.3 vs Loki).

## Implications for the test matrix

1. Every proof produced under 0.8.3 shape should verify under 0.8.1 and 0.8.2 (no wire change found). The W2 harness currently checks only 0.8.3; adding 0.8.1 and 0.8.2 verifiers is the T04 work.
2. 0.8.1 is more permissive than 0.8.2+ on malformed multi-proof inputs (missing `InvalidInstances`). Negative controls must be version-conditional: a multi-proof length-mismatch rejection is expected under 0.8.2 and 0.8.3, not under 0.8.1. Do not write a single negative-control assertion that applies to all three.
3. Performance differences (msm cutover) do not affect acceptance; do not use them to explain any proof-level mismatch.
4. Dependency pin: `midnight-curves = "0.3.1"` in both 0.8.1 and 0.8.3 manifests (`Cargo.toml.orig:34`), so a fresh 0.8.1 build resolves the same curves crate as 0.8.3. Not a matrix variable (checked).
5. `create_domain` in `src/plonk/keygen.rs:27` is `pub(crate)` in both Loki and 0.8.3, so Loki's `loki-harden`-gated `Result` return is not a public API break. No CONFLICT needed.

## Unverified

- 0.8.1 has not been built or run against any Loki proof. The diffs are source-level only.
- PR contents for #434, #482, #536 were not fetched; the changelog text was used.
