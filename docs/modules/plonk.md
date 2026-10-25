# `src/plonk`: prover, verifier, key generation, evaluation

The PLONK proving system uses a generic `PolynomialCommitmentScheme` (KZG on BLS12-381 in practice). This page covers the top-level flow. See [plonk-arguments](plonk-arguments.md) for its sub-arguments.

## Purpose

- `keygen_vk` / `keygen_pk` derive the verifying and proving keys from a circuit and SRS.
- `create_proof` produces a proof. `prepare` / `parse_trace` / `verify_algebraic_constraints` verify one.
- `Evaluator` evaluates the combined constraint numerator on the extended domain.

## Public API

| Item | Where |
|---|---|
| `VerifyingKey`, `ProvingKey`, `PinnedVerificationKey` | [`mod.rs:62`, `:342`, `:333`](../../src/plonk/mod.rs). Serialization: `write`/`read`/`to_bytes`/`from_bytes` with `SerdeFormat`. `read_from_cs` (`:152`) rebuilds a VK from a known constraint system. |
| `keygen_vk`, `keygen_vk_with_k`, `keygen_pk`, `k_from_circuit` | [`keygen.rs:212`, `:231`, `:294`, `:203`](../../src/plonk/keygen.rs). `create_domain` (`:27`) is `pub(crate)`. |
| `create_proof` | [`prover.rs:397`](../../src/plonk/prover.rs). Internals: `commit_to_instances` (`:36`), `compute_trace` (`:57`), `finalise_proof` (`:266`). |
| `prepare`, `parse_trace`, `verify_algebraic_constraints` | [`verifier.rs:414`, `:16`, `:180`](../../src/plonk/verifier.rs). `prepare` returns a `Guard` that is finished by the KZG pairing check. |
| `ProverTrace`, `VerifierTrace` | [`traces.rs:14`, `:35`](../../src/plonk/traces.rs). Split proof construction and parsing so that recursion circuits can reuse the trace. |
| `Circuit`, `ConstraintSystem`, `Expression`, `Column<_>`, `Phase`, `Challenge`, `Assignment` | [`circuit.rs`](../../src/plonk/circuit.rs) (2609 lines). The circuit-definition API. |
| `Error`, `TableError` | [`error.rs:8`, `:103`](../../src/plonk/error.rs). |
| `ValueSource`, `Calculation`, `Evaluator`, `GraphEvaluator` | [`evaluation.rs:38`, `:115`, `:192`, `:203`](../../src/plonk/evaluation.rs). `evaluate_numerator` is at `:305`. |
| `plonk::bench::prover` | [`bench/prover.rs`](../../src/plonk/bench/prover.rs), behind `bench-internal`; re-runs trace stages for criterion (`benches/zswap_output.rs`). |

## Difference from stock midnight-proofs 0.8.3

Files that differ from registry 0.8.3 include `prover.rs`, `evaluation.rs`, `keygen.rs`, `mod.rs`, `lookup/prover.rs`, `permutation/prover.rs`, `vanishing/prover.rs` and the optional `src/backend` dispatcher. The files `verifier.rs`, `circuit.rs`, `error.rs` and `traces.rs` remain byte-identical to registry 0.8.3. See [loki-gpu](loki-gpu.md) for the dispatcher. On the tested supported inputs and configurations, the unmodified stock verifier accepts Loki proofs. The evidence covers the cases in [loki-harness](loki-harness.md) and the GPU-specific cases in [GPU](../GPU.md).

| Change | Feature | Where |
|---|---|---|
| Phase timers and proof guard (`PhaseGuard`, `ProofGuard`) | `loki-profile` | `prover.rs` (many `cfg` blocks), `vanishing/prover.rs`, `evaluation.rs:326` |
| `tracing` spans `loki_ntt_quotient`, `loki_ntt_numerator`, `loki_ntt_quotient_recovery` | `cuda-ntt` | `prover.rs:295`, `:686`, `vanishing/prover.rs:99` |
| Checked domain construction: `create_domain` returns `Result`, `EvaluationDomain::try_new` rejects oversized degrees; `VerifyingKey::read_from_cs` returns `InvalidData` instead of panicking | `loki-harden` | `keygen.rs:43-46`, `mod.rs:176-179` |
| Rotation index arithmetic widened to `usize`/`i128` (`RotationIndex`), no `i32` overflow on large domains | `loki-harden` | `evaluation.rs:11-33` |
| Parallel batch inversion (`batch_invert_parallel`), parallel prefix product (`prefix_product`), compact rational conversion and `par_sort` of lookup input | none (always on) | `permutation/prover.rs`, `lookup/prover.rs`, `poly/mod.rs`, `utils/arithmetic.rs` |
| Sort leftover lookup table rows by value for deterministic lookup proofs | `loki-deterministic-lookup-order` (opt-in) | `lookup/prover.rs` |

The always-on changes are W3 CPU work. They change no byte of a lookup-free proof. The opt-in `loki-deterministic-lookup-order` feature sorts leftover table rows by value, selecting one valid permutation; default behavior uses `HashMap` iteration order, as stock does. The W3 measurements for sorted lookup rows use the earlier revision documented in [`LOKI-CPU.md`](../../LOKI-CPU.md).

## Invariants and pitfalls

From "Stock midnight-proofs 0.8.3 pitfalls" in `wiki-llm/lessons/correctness.md`, as they apply here:

- Multi-proof mixed instances: In a batch with multiple proofs that includes both committed and plain instance columns, the prover absorbs columns in per-proof order. The verifier absorbs all committed columns first, then all plain columns (`src/plonk/prover.rs:467-494`, `src/plonk/verifier.rs:63-75`). This inherited stock mismatch means the stock verifier rejects its own proof for this input shape; Loki preserves the same behavior (`LOKI-FUZZ.md`, finding 1). Quint's `transcript_instances` model reproduces the defect ([quint](quint.md)).
- Byte reproducibility: By default, Loki and stock both place leftover lookup rows using `HashMap` iteration order. The opt-in `loki-deterministic-lookup-order` feature sorts those rows by value. The W3 lookup measurements came from the earlier source revision `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16`, where sorting was always on; see [`LOKI-CPU.md`](../../LOKI-CPU.md). Vanishing randomness is split by `current_num_threads()` (`vanishing/prover.rs:49-71`, unchanged), so byte comparisons must use the same Rayon pool size. The harness records cross-verifier acceptance and narrower byte-equality cases separately ([loki-harness](loki-harness.md)).
- No `Z_H | numerator` check: A bad witness produces a truncated garbage quotient, not an error. Use `MockProver` and the stock verifier.
- Limb-wise `Fq` equality: `==`, `Hash` and `is_zero` compare Montgomery limbs. GPU outputs must be fully reduced (checked in the MSM and NTT fuzz targets).
- Batch inversion skips zeros: `batch_invert_parallel` preserves this behavior (test `parallel_inversion_preserves_zero_denominators`).
- `truncated-challenges` must be set identically on prover and verifier. `fuzz` has a `truncated-loki-only` configuration that asserts a mismatch fails.
- `committed-instances` is a default feature and changes the `create_proof`/`prepare` signatures.

## Tests

- `tests/plonk_api.rs` (`plonk_api`) and `tests/cpu.rs` (`fixed_rng_lookup_proofs_are_reproducible`).
- Unit: `keygen.rs` `oversized_degree_returns_keygen_error`, `evaluation.rs` `rotation_large_domains_and_extreme_rotations`, `quotient_degree_has_a_positive_floor`, `oversized_degree_is_rejected_when_reading_key` (all under `loki-harden`).
- Cross-implementation: [loki-harness](loki-harness.md) (W2), [tests-stock](tests-stock.md), [fuzz](fuzz.md) (`proof_roundtrip`, `soundness`), [quint](quint.md) transcript replay.

## Notes

[`LOKI-CPU.md`](../../LOKI-CPU.md), [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md), [`LOKI-PROFILE.md`](../../LOKI-PROFILE.md), [`LOKI-QUINT.md`](../../LOKI-QUINT.md).
