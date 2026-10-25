# W13 SRS registry

Worktree: `/home/charles/repos/loki-wt/w13-srs-registry`, branch
`loki/w13-srs-registry`, starting revision `877114d`.

## Design and implementation plan

Source: WP3, baseline §1 and risks §d of
`/home/charles/repos/Moriarty-factory/wiki-llm/loki/gpu-lab/icicle-study/DESIGN.md`
(2026-10-24). Implementation reads Loki and Apache-2.0 sppark 0.1.16 only;
no files under `~/research/open-icicle-src` are consulted.

- [ ] Instrument the existing two-slot cache with per-phase backend counters;
  measure three k17 fixture proofs and three synthetic cold/full/prefix/warm
  sequences at capacity 2^20, n=2^20−1 before changing cache behavior.
- [ ] Add the opt-in `cuda-srs-registry` feature (implies `cuda`) and forward
  harness features. Keep the legacy path behind its existing feature.
- [ ] Add differential tests that assert prefix reuse, same-address mutation
  detection, corrupt/torsion rejection, LRU eviction, and four interleaved
  coefficient/Lagrange arrays at k19/k20 against stock CPU MSM.
- [ ] Replace two slots, only under the feature, with address/capacity entries,
  exact requested-prefix comparisons, and LRU eviction under
  `LOKI_GPU_SRS_BUDGET_BYTES` (default 4294967296). Account for sppark's
  persistent points, buckets and histogram, not just the 96-byte base payload.
- [ ] Validate upload candidates on a four native scoped workers; never use
  Rayon cooperative waits while holding the device/cache locks.
- [ ] Repeat measurements; run W2 with the feature, stock verification,
  and every copied full-gate step. GPU commands go through gpu-runner with
  slices at most 600 seconds. Preserve commands, runner logs and results here.
- [ ] Review exact-hit refinement with `lean-speedups`, reconcile shared-file
  additions with W10, commit tested changes, publish DONE with remaining risks.

The registry keeps the validated projective coordinate snapshot rather than
only affine wire coordinates: this preserves the existing exact coordinate
hit predicate without normalizing or trusting a mutable host pointer. It uses
144 bytes per host point (versus 96 for affine); the device wire remains 96.
A match checks all x/y/z limbs of the requested prefix; unused suffixes need
not match. No digest, pinned host identity or generation assumption is used.

The memory budget covers persistent sppark handle allocations, with the
rounded point count, window/bucket count and device SM count used by its
constructor. Invocation scratch and other CUDA backends remain outside this
registry budget. An entry larger than the budget returns an error before
upload; existing sticky backend disable and CPU fallback semantics apply.

## Evidence

All timestamps below refer to runner log names or command logs; no GPU
measurement has yet completed. CUDA toolkit is the supplied 13.4.3; GPU jobs
are submitted to `gpu-runner`, never executed directly in this sandbox.

Completed CPU checks on the instrumented legacy cache:

| Command | Result | Log |
|---|---|---|
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --lib -- --test-threads=4 --skip dev::cost_model::tests::cost_model` | 33 passed, 2 ignored, 1 filtered | `target/w13/baseline-cpu.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --features loki-profile --lib profile::tests -- --test-threads=1` | 1 passed; preceding red run failed for missing counter API | `target/w13/counters-{red,green}.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline -- --test-threads=4 --skip dev::cost_model::tests::cost_model` | 33 unit, 2 integration, 9 doctests passed | `target/w13/full-cpu.log` |
| `./loki-harness/run.sh` | k10/k14/k17 cross-verification and equal VK passed; lookup proof bytes differ as on baseline | `target/w13/w2-cpu.log` |
| `CARGO_BUILD_JOBS=8 RAYON_NUM_THREADS=4 cargo run --release --offline --example cpu-bench -- 16 3 target/cpu-fixture` | Three proofs verified, identical bytes | `target/w13/cpu-fixture.log` |
| `CARGO_BUILD_JOBS=8 cargo test --release --offline --manifest-path tests-stock/Cargo.toml` | 1 passed after fixture generation | `target/w13/tests-stock-after-fixture.log` |

The first standalone tests-stock invocation failed because `target/cpu-fixture`
was absent (`target/w13/tests-stock.log`). Generating it in the supplied gate
script's order fixed that invocation; no verifier code was changed.

Baseline source snapshots: `target/w13/baseline-source/{backend,profile,tests}.rs`;
patch: `target/w13/baseline-instrumentation.patch`. SHA256:

- backend.rs: `66c849e35c2a4a44c1c70db0824d8cd46fb715f9bb4ce6bc6fcef4e7900de2c1`
- profile.rs: `55a8082e56e03ee70fbdc3347c8de43aa24db4fee81b24c16b7c1f85b3212a3b`
- tests.rs: `13ff50468dc1ac1276a29cbb2cc61b656feb8e7f9736e730046ff9a2d31d0b12`

The exact full-gate copy with only `cd` changed is
`target/w13/verify-main.sh` (SHA256
`f689b4736e64500e4cb7d1f92f6b976f318039ab1712111be62327ad447665e0`).
`target/w13/verify-main-via-runner.sh` substitutes `runner-cargo` for `flock`
and adds the registry feature to GPU tests, preserving each gate command.
The proxy submits one Cargo command and waits for its runner result; all GPU
slices must stay at most 600 seconds.

