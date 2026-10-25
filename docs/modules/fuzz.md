# `fuzz/`: W7 differential fuzzing

## Purpose

Every check compares Loki with unmodified registry `midnight-proofs =0.8.3` (renamed `stock`, never patched), and where practical, with a naive mathematical definition. The SRS, VK and proofs cross between the crates only as bytes.

## Layout

| Path | Contents |
|---|---|
| `fuzz/Cargo.toml` | cargo-fuzz crate (libFuzzer, nightly), workspace members `.` and `diff`. Features `cuda`, `cuda-ntt`, `truncated`, `truncated-loki-only`. |
| `fuzz/fuzz_targets/*.rs` | One-line wrappers: `msm_diff`, `ntt_diff`, `proof_roundtrip`, `soundness`, `proof_parse`. |
| `fuzz/diff/src/` | `msm.rs`, `ntt.rs`, `proof.rs`, `circuit.rs`, `mutation.rs`, `parse.rs`, `lib.rs`: all check logic, `run(&[u8])` per target, shared with proptest. |
| `fuzz/diff/tests/` | `quick.rs` (proptest versions and corpus replay), `cases.rs` (explicit Lean-contract cases), `soundness_mutation.rs` (port of Moriarty's witness-mutation harness). |
| `fuzz/diff/examples/` | `committed`, `gpu_msm_probe`, `mock`, `probe`, `seeds`. |
| `fuzz/quick.sh` | Stable gate: CPU configurations `""`, `truncated`, `truncated-loki-only`, under 3 minutes of test time. `QUICK_GPU=1` adds `cuda-ntt` and `cuda` under the shared GPU lock. |
| `fuzz/run-target.sh`, `fuzz/gpu-slices.sh` | 30-minute runs. GPU runs go in 10-minute lock slices. |
| `fuzz/corpus/<target>/` | Minimized corpora. |

## What each target checks (summary of `LOKI-FUZZ.md`)

1. `msm_diff`: Loki `msm_specific` equals stock and the naive sum, lengths 0..4096 with chunk and window boundaries, edge scalars, zip truncation, stale-cache and partial-prefix commits. Output must be a fully reduced residue and on the curve. With `cuda`, `LOKI_GPU_MSM_MIN=1` and a tracing layer turn a silent GPU fallback into a failure.
2. `ntt_diff`: the six domain conversions equal stock for k 1..16 and extension 1..9, and a naive DFT for extended size up to 256 (pins root, coset generator and single `1/n`). With `cuda-ntt` the GPU call counter must grow for eligible lengths.
3. `proof_roundtrip`: random circuits (custom gate, cube gate, lookup, second phase, copy constraints, committed instances, 1-2 proofs, k in 4..10). The checks compare VK bytes, cross-verify in both directions, and require byte equality for lookup-free proofs.
4. `soundness`: mutated witness, flipped byte, truncation, changed or extra public input, altered VK, appended byte. For a mutated witness, Loki and stock `MockProver` first determine whether the changed circuit satisfies its constraints. Both verifiers must reject proofs for an invalid mutant; they must accept proofs for a satisfying mutant. The other corrupted-proof and public-input cases must be rejected.
5. `proof_parse`: arbitrary bytes into VK, verifier-param, SRS and proof readers. Loki and stock must agree on accept/reject and on panicking.

## Difference from stock midnight-proofs 0.8.3

Fuzzing runs from a separate workspace and does not change the library. `truncated-loki-only` deliberately mismatches prover and verifier features and must fail cross verification.

## Invariants and pitfalls

- The recorded campaigns found no accepted invalid proof, GPU/CPU or Loki/stock mismatch, or Loki-only panic (`LOKI-FUZZ.md`, "Findings"). Four upstream defects reproduce in both crates: the multi-proof committed/plain instance ordering defect, a panic for VK with `k` near 32, a panic for a VK with too few fixed commitments, and a `MockProver` panic on small tables. Stock rejects proofs when multiple proofs use a mixture of committed and plain instance columns; the Quint audit records this inherited limitation. See T03 in the root TODO.
- Stock proofs are valid but not reproducible across processes, so forgery checks apply to this process's honest proofs only.
- The campaigns ran for bounded periods. The proof-roundtrip and soundness CPU runs used inputs of 4 to 6 bytes, which mostly reseeded the remaining structured case instead of mutating its bytes. The GPU runs also ended early in places (`LOKI-FUZZ.md` records the early stop).
- GPU targets need the host GPU lock and a CUDA device.

## Tests

`cargo test` in `fuzz/diff` (see `quick.sh`). Related: [loki-harness](loki-harness.md), [quint](quint.md).

## Notes

[`LOKI-FUZZ.md`](../../LOKI-FUZZ.md).
