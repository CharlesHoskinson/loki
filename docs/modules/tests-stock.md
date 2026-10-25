# `tests-stock/`: registry verifier check

## Purpose

A package whose only Loki-related dependency is the registry crate `midnight-proofs =0.8.3` (`default-features = false`; the `committed-instances` and `circuit-params` features are forwarded explicitly). It has its own `[workspace]` so a caller's `[patch.crates-io]` cannot replace the verifier with Loki.

## Public API

One test file, [`tests/verify.rs`](../../tests-stock/tests/verify.rs): `registry_verifier_accepts_loki_proof`. It reads `proof.bin`, `vk.bin` and `params.bin` from `LOKI_CPU_FIXTURE` (default `../target/cpu-fixture`), verifies with the stock `prepare` + `Guard::verify`, asserts acceptance, flips the last proof byte and asserts rejection.

The fixture is produced by `cargo run --release --example cpu-bench -- 16 3 target/cpu-fixture` ([examples](examples.md)). The test circuit is `tests/support/cpu_circuit.rs`.

## Difference from stock midnight-proofs 0.8.3

This package is the stock side of the comparison.

## Invariants and pitfalls

- Run the fixture producer first, or the test panics with "run cpu-bench with target/cpu-fixture output first".
- It checks one honest proof and one single-byte corruption only. Broader negative coverage is in [loki-harness](loki-harness.md) and [fuzz](fuzz.md).

## Tests

This directory is the test. It is part of the W9 gate in `LOKI-HARDEN.md` ("Registry stock verifier accepts the fixture and rejects corruption").

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md).
