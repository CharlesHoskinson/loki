# Inventory: fuzz QA area (haiku-v1-6, area 6 "fuzz")

Snapshot taken 2026-10-25 from ~/repos/loki-proofs (read only). All paths relative to that repo.

## What exists

| Item | Evidence |
|---|---|
| 5 libFuzzer targets: msm_diff, ntt_diff, proof_roundtrip, soundness, proof_parse | `fuzz/Cargo.toml:37-59` (`[[bin]]` entries); `fuzz/fuzz_targets/*.rs` (one-line `fuzz_target!` each) |
| Harness crate `loki-diff` (Loki vs unmodified stock midnight-proofs 0.8.3) | `fuzz/diff/Cargo.toml` (`stock = { version = "=0.8.3" }`, `loki = path "../.."`) |
| Entry points | `fuzz/diff/src/msm.rs:180`, `ntt.rs:76`, `parse.rs:101`, `proof.rs:349` (`roundtrip`), `proof.rs:411` (`soundness`) |
| Structured input decoder | `fuzz/diff/src/lib.rs:30-98` (`Input`: u8/u16/u64/bool/range/bytes/rng); `proof.rs` `Case::generate` consumes the front of the input, then `input.u64()` seeds ChaCha |
| Witness-mutation port (Moriarty) | `fuzz/diff/src/mutation.rs:369` (`run_suite`), `fuzz/diff/tests/soundness_mutation.rs` |
| Explicit Lean-contract cases | `fuzz/diff/tests/cases.rs` (225 lines; includes `multi_proof_committed_instance_order`, `upstream_hostile_vk_panics`) |
| Proptest versions and corpus replay | `fuzz/diff/tests/quick.rs` (100 lines) |
| Stable gate | `fuzz/quick.sh` runs configs `""`, `truncated`, `truncated-loki-only`; optional `QUICK_GPU=1` adds `cuda-ntt`, `cuda` under `flock ~/.cache/doctorwatson/gpu.lock` |
| Campaign runner | `fuzz/run-target.sh:1-17`: pins `cargo +nightly-2026-10-11 fuzz run`, `-timeout=120 -rss_limit_mb=8192 -max_len=4096 -len_control=0` |
| GPU slicing | `fuzz/gpu-slices.sh` (<=600 s slices under the GPU lock) |
| Committed corpora | `git ls-files fuzz/corpus` = 2448 files: msm_diff 278, ntt_diff 331, proof_parse 324, proof_roundtrip 659, soundness 856 |
| Feature matrix | `fuzz/Cargo.toml` features `cuda`, `cuda-ntt`, `truncated`, `truncated-loki-only`; `fuzz/diff/Cargo.toml:9-20` |
| Recorded campaign results | `LOKI-FUZZ.md:87-123` (table of 30-min runs); findings `LOKI-FUZZ.md:125-164` (4 upstream defects + 1 fixed false positive, no Loki bug) |
| Documentation | `LOKI-FUZZ.md`, `docs/modules/fuzz.md`, `docs/ARCHITECTURE.md:221-237` |
| Toolchains present | nightly-2026-10-11, stable, 1.95, 1.98.1, 1.99.0 (`rustup toolchain list`) |

## Current state checks (this session)

- `cd fuzz && cargo metadata --locked --no-deps` exits 0. The fuzz lock is consistent today. This is evidence that TODO T38 (`TODO.md:156`, "fuzz lock mismatch") may be stale for the fuzz workspace. NOT verified: a full `cargo +stable test --locked` or `cargo fuzz build` run (out of my cargo-check-only allowance).
- No `.github/` directory exists in the repo. No CI workflow of any kind runs fuzz, quick.sh or the nightly targets. Grep for `fuzz` under `.github` returned nothing because the directory is absent.
- No WASM, Android or iOS fuzz configuration exists (grep over `fuzz/` for wasm/android/ios is empty).

## Gaps (for the v1 roadmap)

1. **Structured proof/soundness input is missing.** `LOKI-FUZZ.md:105-111` records that `soundness` and `proof_roundtrip` stayed at libFuzzer `lim: 4..6` bytes, so inputs reseeded ChaCha and did not mutate structured bytes. Open as T32 (`TODO.md:131-133`). `run-target.sh` now passes `-len_control=0`, but no re-run is recorded.
2. **No CI for fuzzing.** No `.github/` in the repo. `quick.sh` (3-min budget, `quick.sh:4`) and the 30-min nightly runs are manual. No continuous (OSS-Fuzz-style) campaign, no crash-triage policy, no corpus-minimisation schedule.
3. **Nightly toolchain pinned to a dated nightly** (`run-target.sh:9`, `nightly-2026-10-11`). No policy for bumping it.
4. **GPU fuzzing is serial and lock-bound.** `LOKI-FUZZ.md:91-106`: GPU msm got 2 of 3 slices; `cuda,cuda-ntt` proof run replayed 128 of 659 corpus inputs and was stopped early. `fuzz/quick.sh` `QUICK_GPU=1` path has NOT been run as a whole (`LOKI-FUZZ.md:118-122`).
5. **Coverage ceiling on `ParamsKZG::read_custom`**: fuzzed only for k <= 6 (`LOKI-FUZZ.md:185-186`). Large-k garbage headers are an unbounded allocation by design, so the v1 spec needs an explicit input bound.
6. **Circuits small** (k <= 10, `LOKI-FUZZ.md:182`). Large-k behaviour is claimed only via W2 harness, which is not a fuzzer.
7. **Targets absent for components the v1 scope names**: transcript/Fiat-Shamir replay under arbitrary bytes (only `proof_parse` touches transcript indirectly, `docs/modules/transcript.md:32`); lookup argument as its own target (only inside `proof_roundtrip`); multiopen as its own target; deserializers of VK and PK (proving-key) are not separate targets (`proof_parse` covers VK read; PK read not listed in `LOKI-FUZZ.md:29-86`, NOT verified: confirm in `fuzz/diff/src/parse.rs`).
8. **Sanitizers**: ASan used for CPU runs only (`LOKI-FUZZ.md:91-96`); no MSan/TSan/UBSan record; ASan and CUDA do not mix (`LOKI-FUZZ.md:189`, `-s none`).
9. **Four upstream defects** (multi-proof committed/plain instance ordering; VK k near 32 panic; VK fixed-commitments index panic; MockProver small-table panic) are reproduced only in `cases.rs`/`LOKI-FUZZ.md`. They are not yet pinned to a stock-version matrix (0.8.1 not tested by the harness; the stock dep is `=0.8.3` only, `fuzz/diff/Cargo.toml:14`).
10. **Open issue on oracle scope**: `docs/review/luna-r2.md:19-21` notes that W7 does not assert unconditional rejection of mutated witnesses; only MockProver-rejected mutants count as invalid. Needs spec wording.

## Pointers
- Recorded findings: `LOKI-FUZZ.md:125-164`.
- Open TODO items on this area: T32 (`TODO.md:131`), T38 (`TODO.md:156`).
