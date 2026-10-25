# `loki-harness/`: the W2 differential gate

## Purpose

An independent Cargo workspace links Loki (path dependency, package-renamed `loki`) and unmodified crates.io `midnight-proofs =0.8.3` (renamed `stock`) side by side. Its synthetic cases check whether each verifier accepts the other crate's proofs and whether their verifying-key bytes match. The results apply to the cases the harness runs. Do not patch crates.io `midnight-proofs` in this workspace, because that would replace the independent verifier.

## Public API

- [`src/lib.rs`](../../loki-harness/src/lib.rs): `cross_verify(k)` (`:167`), `determinism(k)` (`:172`), `negative_controls()` (`:194`); result types `Comparison` (`:70`), `Metrics` (`:23`), `Difference` (`:39`, first differing offset and counts), `Fault` (`:14`).
- [`src/backend.rs`](../../loki-harness/src/backend.rs): one macro defines both circuits and their per-backend operations.
- Binaries: `gate` (`src/bin/gate.rs`; sizes default to 10, 14, 17) and `bench` (JSON lines `{backend,k,keygen_s,prove_s,verify_s,proof_bytes}`).
- [`run.sh`](../../loki-harness/run.sh): runs `gate` with `RAYON_NUM_THREADS=1`, `CARGO_BUILD_JOBS=4` and a 292 s process-group timeout (exit 124 on timeout, 137 if killed).
- Features: `cuda` (forwards `loki/cuda`), `cuda-srs-registry`, `loki-multiopen`, and `loki-gpu-backend` (forwards `loki/loki-gpu-backend`). `LOKI_STRICT_LOOKUP_BYTES=1` makes any lookup-proof byte difference fail.

## What the gate checks

For each k, independent key generation in both crates produces identical VK bytes. Full SRS bytes round-trip in both directions through checked `RawBytes` reads. Each crate's proof verifies with the other crate's verifier. Both reject wrong public inputs, empty proofs and proofs with an appended byte, and each transcript is fully consumed. Four `MockProver` controls mutate each constraint family independently. The fixture fills `2^k - 32` rows with a nonlinear gate, copy constraints, a public input, an 8-bit lookup and a second-phase column that depends on a challenge. Seeds use fixed ChaCha20. The synthetic SRS comes from the public `unsafe_setup` utility, which must not be used in production.

## Difference from stock midnight-proofs 0.8.3

The harness is a separate package for comparing Loki with stock.

## Invariants and pitfalls

- Proof-byte equality is checked for the no-lookup control (1,136 bytes at k10). The recorded W2 lookup comparison (1,280 bytes) first differed from stock at byte 192. That result records the `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16` source baseline, where Loki sorted leftover rows and stock used `HashMap` order. Current main uses `HashMap` order by default. The opt-in `loki-deterministic-lookup-order` feature sorts leftover rows and changes lookup proof bytes. The harness reports byte differences while still requiring cross-verification and VK equality.
- The harness covers its listed synthetic circuit cases. Multiple proofs with a mixture of committed and plain instance columns trigger an inherited stock absorption-order defect, and stock rejects the proofs. See `LOKI-QUINT.md` and T03 in the root TODO.
- Run serially. One Rayon worker is exported because the vanishing randomness is split by pool size.
- Each timing is a single cold measurement with Loki first. These samples do not establish a speedup.
- `README.md` in this directory describes `cuda = []` as a reserved placeholder. The current `Cargo.toml` forwards `cuda = ["loki/cuda"]`.

## Tests

`tests/differential.rs`: `cross_verify` and `determinism` for k10, k14, k17 (macro-generated), ignored `k20_cross_verify_and_determinism`, and `reject_broken_constraints`.

## Notes

[`loki-harness/README.md`](../../loki-harness/README.md), [`LOKI-DONE.md`](../../LOKI-DONE.md) (gate G2), [`LOKI-MULTIOPEN.md`](../../LOKI-MULTIOPEN.md) and [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md) for gate results.
