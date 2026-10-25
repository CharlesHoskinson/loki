# TASK fuzz-code: current Loki fuzz QA state (file:line)

Read-only research, 2026-10-25, repo `~/repos/loki-proofs`. Nothing built or run except `cargo metadata --locked`. Full inventory: `/home/charles/repos/loki-wt/v1/docs/v1-research/haiku-v1-6/inventory.md`.

## Targets (cargo-fuzz, nightly)
- Bins: `fuzz/Cargo.toml:37-59`. Five targets: `msm_diff`, `ntt_diff`, `proof_roundtrip`, `soundness`, `proof_parse`.
- Each is a one-line `fuzz_target!` calling `loki_diff::*` (`fuzz/fuzz_targets/*.rs`):
  - `msm.rs:180` `run`: 1 in 4 inputs take `commit`, else `msm` (`msm.rs:183-186`). Sets `LOKI_GPU_MSM_MIN=1` under `cuda` (`msm.rs:64-65`).
  - `ntt.rs:76` `run`: sets `LOKI_NTT_CPU` process-wide (`ntt.rs:66-70`).
  - `parse.rs:101` `run`: VK, verifier params, SRS and proof decoders, modes 0..4 and default (`parse.rs:110-196`); `agree()` at `parse.rs:72` requires identical accept/reject and panic behaviour.
  - `proof.rs:349` `roundtrip`, `proof.rs:411` `soundness` (choice `input.u8() % 6`, `proof.rs:~440`). Both build a case with `Case::generate` (`circuit.rs:239`), then a seed from `input.u64()`.
- Mutation kinds: `proof.rs` `enum Mutation` (Witness, ProofByte, Truncate, Append, PublicInput, FixedCoeff).

## Harness and oracle
- `fuzz/diff` = crate `loki-diff`, depends on Loki (`loki = path "../.."`) and `stock = =0.8.3` (`fuzz/diff/Cargo.toml:17-18`).
- Verdict comparison in `proof.rs:roundtrip_case` (~`proof.rs:358-405`): proofs from both provers must agree on errors; no-lookup proof bytes must be identical (`proof.rs:~369`); cross-verify all four directions.
- `CROSS_MISMATCH = cfg!(feature = "truncated-loki-only")` (`proof.rs:278`) inverts the expectation.
- `order_mismatch` (`circuit.rs:282`) marks the upstream committed/plain ordering shape as expected-reject.
- Input decoder: `lib.rs:30-98` (`Input`). `arbitrary = "1"` is a dependency (`fuzz/diff/Cargo.toml`). NOT verified whether any target uses `arbitrary::Unstructured`; the decoder visible at `lib.rs:30-98` is hand-written.

## Deterministic gate
- `fuzz/quick.sh`: configs `""`, `truncated`, `truncated-loki-only` (`quick.sh:12`); `QUICK_GPU=1` adds `cuda-ntt`, `cuda` under flock (`quick.sh:6-7, 13-16`). Runs `--test cases` (single-threaded) and `--test quick --test soundness_mutation`.
- `fuzz/diff/tests/quick.rs`: proptest per target (`quick.rs:39-85`) plus corpus replay: `msm_corpus` `:54`, `ntt_corpus` `:59`, `roundtrip_corpus` `:88`, `soundness_corpus` `:93`, `parse_corpus` `:98`.
- `fuzz/diff/tests/cases.rs`: explicit cases `:24-192`, including `multi_proof_committed_instance_order` `:141`, `truncated_challenges_parity` `:168`, `upstream_hostile_vk_panics` `:192`.
- `fuzz/diff/tests/soundness_mutation.rs` (161 lines): port of Moriarty's mutation harness; `mutation.rs:369` `run_suite`.

## Campaign runner
- `fuzz/run-target.sh:1-17`: `cargo +nightly-2026-10-11 fuzz run`, `-timeout=120 -rss_limit_mb=8192 -max_len=4096 -len_control=0`, logs to `fuzz/logs/` (gitignored, `fuzz/.gitignore`).
- `fuzz/gpu-slices.sh`: 600 s slices under `flock ~/.cache/doctorwatson/gpu.lock`.
- Toolchain present: `nightly-2026-10-11` (`rustup toolchain list`).

## Committed corpus
- `git ls-files fuzz/corpus`: 2448 files. msm_diff 278, ntt_diff 331, proof_parse 324, proof_roundtrip 659, soundness 856.

## Results on record
- `LOKI-FUZZ.md:87-123` runs table. Clean except upstream defects (`LOKI-FUZZ.md:125-164`): (1) multi-proof committed/plain instance order; (2) VK k near 32 panics; (3) VK with too few fixed commitments indexes past end; (4) MockProver panic on small tables. No Loki-only bug recorded.
- `LOKI-FUZZ.md:105-111`: soundness and proof_roundtrip ran at `lim: 4..6` bytes, so the campaigns sampled seeded cases rather than mutating structured bytes. T32 open (`TODO.md:131-133`).
- `LOKI-FUZZ.md:118-122`: `QUICK_GPU=1` path was not run as a whole.

## Gaps against the v1 scope
1. No CI. `ls -d .github` returns nothing; no workflow runs `quick.sh` or any fuzz target.
2. No WASM, Android, iOS or wasm-threads fuzz configuration (grep of `fuzz/` for wasm/android/ios is empty).
3. Upstream stock dependency is only `=0.8.3`; the local registry also holds 0.7.2, 0.7.3 and 0.8.2 (`~/.cargo/registry/src/*/midnight-proofs-*`), but no 0.8.1. A stock-version matrix is not in the harness.
4. Lockfile: `cd fuzz && cargo metadata --locked --no-deps` exits 0 today. This suggests T38 (`TODO.md:156`) may be stale for `fuzz/`. NOT verified: `cargo +stable test --locked` or `cargo fuzz build`.
5. `ntt.rs:66-70` and `msm.rs:64-65` set process-wide env vars inside a fuzz target. `quick.sh:24-25` serializes GPU configs for this reason; the libFuzzer loop itself is single-threaded, so this is a note, not a bug.
6. `proof_parse` fuzzes VK and params decoders at small k only: `LOKI-FUZZ.md:185-186` (`read_custom` for k <= 6).
7. Large-k behaviour is covered only by W2 (`LOKI-FUZZ.md:182`, circuits k <= 10).
