# W7: differential fuzzing of Loki against stock midnight-proofs 0.8.3

Every check is differential. Loki (the parent path package) is compared with
**unmodified crates.io `midnight-proofs =0.8.3`** (package-renamed `stock`, never
patched) and, where that is cheap enough, with a naive mathematical definition.
The SRS, VK and proofs cross between the crates only as bytes.

Fuzzed code: `loki/w7-fuzz` with `main` merged at `8eb0cee` (W1 profiling, W2
harness, W3 CPU, W4 GPU MSM `cuda`, W5 GPU NTT `cuda-ntt`, W8 Quint).

## Layout

| Path | Contents |
|---|---|
| `fuzz/Cargo.toml` | cargo-fuzz crate (libFuzzer, nightly), its own workspace with `fuzz/diff` |
| `fuzz/diff/src/*.rs` | all check logic, `fn run(&[u8])` per target; shared by libFuzzer and proptest |
| `fuzz/fuzz_targets/*.rs` | one-line libFuzzer wrappers |
| `fuzz/diff/tests/quick.rs` | proptest versions of all five targets + corpus replay |
| `fuzz/diff/tests/cases.rs` | explicit cases from the Lean contracts (coordinator NOTEs) |
| `fuzz/diff/tests/soundness_mutation.rs` | port of Moriarty's witness-mutation harness |
| `fuzz/quick.sh` | stable gate, CPU configurations in ~60 s of test time |
| `fuzz/run-target.sh`, `fuzz/gpu-slices.sh` | 30 min runs; GPU runs in 10 min lock slices |
| `fuzz/corpus/<target>/` | minimized (`cargo fuzz cmin`) corpora |

Features (on `fuzz/` and `fuzz/diff`): `cuda` (Loki GPU MSM), `cuda-ntt` (Loki GPU
NTT), `truncated` (truncated-challenges in both crates), `truncated-loki-only`
(deliberate prover/verifier feature mismatch: cross verification must fail).

## Targets and what they check

1. **`msm_diff`** (`msm.rs`). Loki `msm_specific` == stock `msm_specific` (== and
   compressed affine bytes) == naive sum (n <= 96), for n in 0..4096 with chunk and
   window boundaries (1023/1024/1025, 2047..4096); zero scalars, identity, duplicate
   and negated bases; scalars 0, 1, r-1, r-small, 2^254 + x, 2^254 + 2^253 + x, powers
   of two, canonical bytes. Zip truncation (extra scalars or extra bases). Every Loki
   output coordinate must be a fully reduced Montgomery residue (raw limbs < p) and on
   the curve. Repeated MSM on the same bases (GPU resident-cache hit) and on a changed
   base (stale-cache check). Partial `commit` / `commit_lagrange` of 2^k' columns
   against a 2^12 SRS (first 2^k' bases; naive prefix check). With `cuda`,
   `LOKI_GPU_MSM_MIN=1` routes every MSM to the GPU, and a tracing layer turns Loki's
   silent "GPU MSM failed; using CPU MSM" fallback into a failure (verified: with
   `CUDA_VISIBLE_DEVICES=-1` the watch fires; with the GPU it does not).
2. **`ntt_diff`** (`ntt.rs`). k = 1..16, extension j = 1..9 (extended <= 2^18).
   `coeff_to_lagrange`, `lagrange_to_coeff`, `coeff_to_extended`,
   `extended_to_coeff`, `extended_to_lagrange`, `divide_by_vanishing_poly`
   elementwise vs stock; vs naive DFT for extended <= 256 (forward at omega, inverse
   with 1/n, coset at ZETA * omega_ext^i). This pins the root, the coset generator
   and the single 1/n, which round trips alone do not. Round trips too. With
   `cuda-ntt`: also Loki with the GPU disabled (`LOKI_NTT_CPU`), and the GPU call
   counter (`loki_cuda_ntt::completed_calls`) must grow for eligible lengths and stay
   unchanged otherwise.
3. **`proof_roundtrip`** (`proof.rs`, `circuit.rs`). Random circuits: PLONK gate with
   fuzzed fixed coefficients (vacuous rows give free cells), selector-gated cube gate
   (quotient degree), lookup, second phase with a challenge, an all-zero equality
   column, copy constraints, 1-2 instance columns of which 0..all are committed, 1-2
   proofs, k in {4,5,6,7,9,10}. VK bytes from both keygens must be identical; Loki
   proof accepted by stock and stock proof accepted by Loki (and each by itself),
   transcript fully consumed; no-lookup proofs byte-identical for the same rng; prover
   errors identical. 
4. **`soundness`** (= negative_cases). Same circuits. One of: a single advice cell of
   the witness mutated (ported MutPlanner; Loki and stock MockProver must agree; if
   MockProver rejects, both verifiers must reject Loki's and stock's proofs of the
   mutant; if it accepts, they must accept), one proof byte xor-ed, truncation, a
   changed (plain or committed) public input or an extra one, a VK with one fixed
   coefficient changed, an appended byte (pairing passes, transcript not empty).
   Any acceptance of the invalid cases is a release-blocking `BUG`.
5. **`proof_parse`** (`parse.rs`). Arbitrary bytes into `VerifyingKey::read`
   (RawBytes and Processed), `ParamsVerifierKZG::read`, `ParamsKZG::read_custom`
   (k <= 6), raw proofs, edited honest proofs and honest proofs under fuzzed VKs.
   Loki and stock must agree on accept/reject and on panicking; an accepted edit of an
   honest proof must be that proof plus trailing bytes; decoded VKs must re-encode
   identically. Seeds: honest VK, verifier params, SRS and proofs.

`tests/cases.rs` (explicit, from the coordinator's Lean notes): MSM top window and
reduced output at n = 1..16385 (crossing the default 2^14 GPU threshold), zip
truncation and degenerate inputs, partial commit columns k = 1..12; NTT all k = 1..16
with j = 1,2,3,5 and the 2^9 / 2^10 GPU dispatch boundary; the multi-proof committed
instance order; truncated-challenges parity; upstream hostile-VK panics.

Note on the NTT threshold: `EvaluationDomain` only produces power-of-two lengths, so
the reachable boundary for "2^10 - 1" is n = 2^9 (CPU) vs 2^10 (GPU), and small k with
j >= 2 (extended 2^10..2^12 on the GPU while the base domain stays on the CPU).

## Runs (30 min per target, `-max_total_time=1800`, RAYON_NUM_THREADS=4)

| Target | Build | Execs | exec/s | cov | ft | corpus (after cmin) | Result |
|---|---|---|---|---|---|---|---|
| msm_diff | CPU, ASan | 67,415 | 37 | 1255 | 4488 | 418 (262) | clean |
| msm_diff | `cuda` GPU MSM, 3 x 10 min slices | MSM_GPU_EXECS | | MSM_GPU_COV | | | MSM_GPU_RESULT |
| ntt_diff | `cuda-ntt`, 30 min (one slot) | 18,196 | 10 | 879 | 2878 | 504 (331) | clean |
| proof_roundtrip | `cuda-ntt`, 3 x 10 min slices | 8,891 | ~5 | 8422 | 20105 | 1082 (659) | clean |
| soundness | CPU | 12,214 | 6 | 12323 | 37121 | 1580 (856) | clean |
| proof_parse | CPU, ASan | 678,721 | 376 | 6840 | 10804 | 656 (324) | 2 shared upstream panics, no divergence |

Logs: `fuzz/logs/` (not committed). Commands:
`fuzz/run-target.sh <t> 1800 [cargo-fuzz args]`, GPU:
`fuzz/gpu-slices.sh <t> 1800 -s none --features cuda-ntt|cuda --target-dir target/fuzz-cuda`.
GPU builds use `-s none` (ASan and CUDA do not mix).

Stable gate: `fuzz/quick.sh` -> default 23 s, truncated 19 s, truncated-loki-only 20 s
(62 s test time; all passed). `QUICK_GPU=1` adds `cuda-ntt` and `cuda` under the GPU lock.

## Findings

No Loki bug: no accepted invalid proof, no GPU/CPU or Loki/stock mismatch, no
Loki-only panic in any run or test. All findings below are upstream (identical in
stock 0.8.3 and Loki), each with a committed reproducer.

1. **Upstream completeness bug: several proofs with committed instances.**
   With >= 2 proofs in one `create_proof` and both committed and plain instance
   columns, stock rejects its own proof (`pairing: OpeningError`), as predicted by
   the Lean Transcript module: the prover absorbs instances per proof (committed,
   then plain; `src/plonk/prover.rs` `compute_instances`) while the verifier absorbs
   all committed commitments of all proofs first, then all plain ones
   (`src/plonk/verifier.rs:61-76`). Loki behaves identically (all four directions
   reject). Neighbours accept: 1 proof; 0 committed; all columns committed; one
   column. Reproducer: `cargo test --release --manifest-path fuzz/diff/Cargo.toml
   --test cases multi_proof_committed_instance_order`. The fuzz targets treat this
   shape as expected-reject (`Case::order_mismatch`) and still require Loki == stock.
2. **Upstream panic: VK with k close to 32.** `VerifyingKey::read` checks `k <= S`,
   then `EvaluationDomain::new` asserts `extended_k <= S`, so k = 31/32 with degree
   >= 3 panics instead of returning an error. Found by `proof_parse` (vk read and
   verify-with-fuzzed-VK modes). Reproducer: `cases.rs upstream_hostile_vk_panics`.
3. **Upstream panic: VK with too few fixed commitments.** `read_from_cs` takes
   `num_fixed_columns` from the bytes without comparing it with the constraint
   system, and verification then indexes past the end ("index out of bounds: the len
   is 4 but the index is 4"). Same reproducer test.
4. **Upstream dev-tool panic: MockProver on small tables.** `MockProver::run` picks
   its own minimal k; with a lookup table the floor planner's `fill_from_row` then
   asserts `row in usable_rows` and panics. Only the oracle is affected; the
   mutation harness treats a MockProver panic in both crates as "no oracle".
5. **Fuzzer false positive, fixed (not a bug).** The first `proof_parse` run stopped
   on a raw proof that was accepted but differed from the in-process honest proof. It
   was the honest *stock* seed proof from another process: stock's lookup permutation
   iterates a randomly seeded `HashMap`, so stock proofs are valid but not
   reproducible across processes (as SPEC.md notes). The forgery check now applies
   to edits of this process's honest proofs only.

Behaviour confirmed rather than found: `truncated-challenges` must match between
prover and verifier (both on or both off accept; Loki-only truncation is rejected in
both cross directions); trailing bytes pass the pairing check and are caught only by
`assert_empty` in both crates (the harness requires it).

## Moriarty harness port

`fuzz/diff/src/mutation.rs` ports `qa_mutation.rs` (wrapper circuit, MutPlanner,
MutAssign, `run_suite` with PlusOne/MinusOne/Zero/Random and swaps) to both crates,
without `circuit-params`, and counts applied substitutions so a plan silently not
applied (e.g. synthesis on another thread) fails instead of passing. Unlike the
original it also drives the real `create_proof`. The Moriarty fixtures (Sherlock
machine circuits on midnight-zk-stdlib/zkir) are not dependencies here; the test
keeps the oracle's positive control (`Leaky`: exactly the free cell is flagged, in
both crates) and runs the full suite on 12 generated circuits (identical accepted
mutant sets in Loki and stock) plus 10 real-proof mutation cases.

## Open risks / limits

- GPU targets ran in 10 minute lock slices; ASan is not used with CUDA.
- `proof_roundtrip` uses the default GPU MSM threshold (2^14), so with `cuda` the
  small fuzz circuits do not exercise GPU MSM inside proofs; GPU MSM correctness is
  covered by `msm_diff` with the threshold at 1 and W4's own tests.
- Circuits are small (k <= 10); large-k behaviour is covered by the W2 harness.
- `ParamsKZG::read_custom` is fuzzed only for k <= 6 (a garbage header requests 2^k
  points, an upstream unbounded allocation by design).
