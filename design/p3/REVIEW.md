# P3 — legacy sppark adapter and validated SRS registry: implementation record

Owner: loki-p3 (Opus implementer). Base: loki-gpu `eb27233` (P2-fix merge); Loki `0c4ab9d`;
Lean gpu-spec `49fa4ca`. Labels follow the repository convention (CODE / MEASURED /
OPEN QUESTION). Every figure below is from synthetic inputs; no production SRS was used.

## Commits

| Tree | Commit | Content |
|---|---|---|
| loki-gpu `p3-srs` | `fdfaaf3` | runtime seam: crate-private `Form::SrsPointWire`, `upload_wire` / `download_wire` / `release_wire` (also `design/p3/runtime-seam.patch`, handed to P4) |
| loki-gpu `p3-srs` | `ababd2c` | `src/srs.rs` registry, `tests/srs_registry.rs` (17 cases incl. native) |
| loki-gpu `p3-srs` | `d66f4c8` | quarantine a failed release of an upload that crossed a generation (lean-w1 finding 2) |
| loki-gpu `p3-srs` | (this record) | review, verification logs |
| gpu-spec | `ea839e5`, `ac95fe8` | OBLIGATION markers (srs-prefix-kernel, srs-codec-agreement, srs-submission-pinning) |
| Loki `loki/p3-srs` | `4190617` | shared W13 validation (`poly/kzg/msm/srs_validate.rs`) |
| Loki `loki/p3-srs` | `6b46cd2` | pre-existing `best_fft` import break under `loki-sppark` |
| Loki `loki/p3-srs` | `b14ee73` | `loki-sppark` typed adapter (`backend/sppark.rs`), G1 codec (`backend/gpu_srs.rs`), `loki-gpu-srs-registry` feature |
| Loki `loki/p3-srs` | `0d7d8f5` | kernel-prefix differential (lean-w1 finding 1) |
| Loki `loki/p3-srs` | `3dae2f5` | only device faults disable legacy GPU MSM (quint-q2 L2) |
| Loki `loki/p3-srs` | `b096a38` | adapter result validation (lean-w3), dispatcher post-return recheck (quint-q2 B3) |
| Loki `loki/p3-srs` | `f40cfcf` | legacy handles stay reachable on release/prepare failure (quint-q4 F2, F4) |
| Loki `loki/p3-srs` | `9fe5da0`, `85586f3` | checked `Dispatcher::try_msm`: Cancelled returned, never recomputed (quint-q2 B1, astra-gpu decision); CPU-only gate |
| gpu-spec | `49fa4ca` | `LokiLean/BackendSrs.lean` and its single import |

## W13 comparison

W13's committed registry (`src/poly/kzg/msm/cuda/backend.rs` at `b6af6f9`, SHA-256
`d84fbb23…a8d8a`) was compared clause by clause against the P3 contract
(`design/MSM.md`, `loki-backend-api` `SrsRegistry`):

| Contract clause | W13 | P3 |
|---|---|---|
| exact hit by content, address only a filter | yes (x/y/z limbs over the consumed prefix) | ported; same comparison through `SrsPointCodec::same_representation` |
| caller `content_identity` never trusted | n/a | never read for a hit decision |
| validated before upload (curve, subgroup) | yes, whole supplied capacity | ported and shared (`srs_validate.rs`, unchanged behaviour) |
| byte budget, refuse before evicting | yes (sppark `msm_t` constructor bytes) | wire bytes (96·n), refused before any victim is chosen when unleased entries cannot cover it |
| leases block eviction | no leases (single caller under the device guard) | lease + pin counts; LRU only over zero-lease, zero-pin entries |
| basis / domain binding | none | coefficient may serve a larger coefficient entry; Lagrange requires the exact domain |
| generation / context loss | none (process-sticky disable) | generation in every key and lease; stale use → `ContextLost`; purge never frees through an old context |

Ported and reused with notices preserved: the snapshot comparison, the four-worker scoped
validation (`parallel_chunks`), and the persistent-byte accounting approach. The lease,
generation and budget machinery is new. Source of each algorithm: W13 (Apache-2.0, this
project) and the P2 runtime; no open-icicle source, derivative or H21 material was opened.

## What was implemented (CODE)

- `loki-gpu/src/srs.rs` — `GpuSrsRegistry<G>` over a type-erased `SrsPointCodec<G>`.
  - `acquire`: pin candidates under the registry lock (runtime `LEVEL_REGISTRY`, never held
    across a runtime call) → exact compare outside the lock → hit lease, or miss → validate
    and encode the whole request → reserve budget and choose LRU victims → release victims →
    `upload_wire` → publish with the first lease.
  - Failed victim release, and (d66f4c8) failed release of an upload that landed in a newer
    generation, are never retried; their bytes stay charged (quarantined) until that
    generation ends.
  - `srs_msm` validates the lease and the length policy, then returns `Unsupported` (P6);
    the `srs_registry` capability stays false.
  - Budget: `LOKI_GPU_SRS_BUDGET_BYTES`, default 4 GiB; unparsable, empty, whitespace or
    non-Unicode values are errors. The budget is fixed at construction.
- Loki `backend/gpu_srs.rs` — `MidnightG1Codec`: 96 bytes per point, `x[6] ‖ y[6]` LE `u64`
  BLST Montgomery limbs, identity `(0, 0)`; validation is the shared W13 check.
- Loki `backend/sppark.rs` — `SpparkBackend`: the legacy `backend::msm` behind `MsmBackend`.
  Admission reproduces the legacy threshold; output validated for curve and G1 subgroup
  before publication; legacy failures classified (device → `SessionPoisoned`; invalid input,
  capacity and host-worker failures → `InputsPreserved`, no poison).
- Loki `backend/dispatch.rs` (MSM arm only): a result returning after a concurrent poison
  is discarded and recomputed on the CPU.

### Never-race argument (`loki-sppark` vs `loki-gpu-srs-registry`)

`src/lib.rs` has a `compile_error!` against `loki-gpu-backend` together with `cuda` /
`cuda-ntt`; `loki-sppark` implies both legacy features and `loki-gpu-srs-registry` implies
`loki-gpu-backend`. All four mixed combinations fail to compile (exit 101,
`verification/loki-exclusivity.txt`). The legacy sppark guard and the new runtime therefore
never coexist in one build, so the runtime's `cudaDeviceReset` on reinitialization (P2 risk
#7) cannot destroy live sppark state. P2-fix's process-unique `GpuRuntime::native` claim
additionally prevents two native runtimes in one process.

## Gates (MEASURED)

Environment: `CARGO_BUILD_JOBS=4`, CUDA 13.4.3, one WSL2 GPU; device jobs under
`flock gpu.lock` (heavy jobs `heavy.lock` first).

CPU / fake driver — on final commits (loki-gpu `d66f4c8`, Loki `85586f3`):

| Gate | Command | Result |
|---|---|---|
| registry contract (fake driver) | `cargo test -p loki-gpu --offline --locked --test srs_registry` | 17 passed |
| mutation | `verification/mutation.txt` | red stub 16/16 fail; M1-M8 each caught |
| Loki `loki-sppark` CPU | `cargo test --release --offline --locked --features loki-sppark -- --test-threads=4` | lib 49 passed / 41 ignored (GPU); integration 14 + 9 + 1 + 1; doctests 9 |
| Loki `loki-gpu-srs-registry` CPU | same with `--features loki-gpu-srs-registry` | lib 38 / 3 ignored; 14 + 9 + 1 + 1; doctests 9 |
| Loki default | `cargo test --release --offline --locked` | lib 34; 9 + 1 + 1; doctests 9 |
| backend-api CPU gate | `cargo test --release --offline --locked --features loki-backend-api --test backend_cpu_equivalence` | 15 passed (incl. `cancelled_msm_is_returned_not_recomputed`; mutant fails) |
| adapter mutants | output check, L2 classification, B3 recheck, Cancelled arm | each caught by its test |
| feature matrix | `cargo check --all-targets` for default, cuda, cuda-srs-registry(+loki-profile), loki-sppark(+cuda-srs-registry), loki-gpu-backend, loki-gpu-srs-registry | 0 new warnings |
| exclusivity | `verification/loki-exclusivity.txt` | all four mixed combinations exit 101 |
| W2 harness | `flock heavy.lock ./loki-harness/run.sh` (Loki `f40cfcf`) | k10/k14/k17 cross-verified, VK equal, no-lookup control byte-equal; exit 0 |
| independent stock verifier | `cpu-bench -- 14 1 target/cpu-fixture` then `cargo test --release --offline --manifest-path tests-stock/Cargo.toml` | 1 passed (proof sha256 `e74f650e…`) |
| Lean | `lake build LokiLean.BackendSrs` at gpu-spec `ac95fe8`; full build at `49fa4ca` | OK (8932 / 8940 jobs); axioms {propext, Classical.choice, Quot.sound} (`verification/lean-backend-srs.txt`) |

Native — measured before the final fixes (loki-gpu `ababd2c`, Loki pre-`3dae2f5`):

| Gate | Result |
|---|---|
| `LOKI_GPU_REQUIRE_DEVICE=1 cargo test -p loki-gpu --features cuda --test srs_registry` | 17/17 |
| compute-sanitizer memcheck (leak-check full), racecheck, synccheck, initcheck on `native_registry_on_device` | 0 errors, 0 leaks |
| Loki `loki-sppark` GPU: adapter vs legacy vs CPU, `gpu_proof`, `gpu_differential` (10,108 cases), `gpu_large_boundaries` | all pass |
| `tests/gpu-compat` stock verifier on the `loki-sppark` GPU proof | accepted; mutated input rejected |

**PENDING (W12 GPU hold):** rerun the native rows on the final commits, plus the W13
`gpu_srs_*` set (nine tests, including the new `gpu_srs_kernel_prefix_differential` and the
L2-updated budget / invalid-suffix tests) and `gpu_srs_registry_k19_k20_on_device`. Every job
is under 10 minutes; they are scripted in this order and have not been started.

## Synthetic figures (MEASURED)

Fake driver, debug build, 8-byte toy wire (`synthetic_cold_warm_figures`): cold k20 acquire
39.5-43.0 ms, warm k20−1 prefix 1.9-2.6 ms; misses 2, uploads 2, hits 6; uploaded
12,582,912 B; snapshot host bytes 12,582,912.

Native toy run (`native_registry_on_device`, 1 MiB budget): acquires 7, hits 1, misses 6,
uploads 4, uploaded 2,097,152 B, evictions 2 (1,048,576 B), rejected invalid 1 / capacity 1,
high-water 1,048,576 B = budget, no stale purge.

Real 96-byte G1 figures (`gpu_srs_registry_k19_k20_on_device`: expected uploads 5, hits 3,
rejected 1, resident (3·2^20 + 2·2^19)·96 B) are PENDING the GPU hold.

## Review findings and dispositions

| Source | Finding | Disposition |
|---|---|---|
| lean-w3 | dispatcher publishes an accelerator MSM point unchecked | FIXED in the adapter (`admit_result`); fake off-curve / order-3 device points → CPU + poison |
| lean-w1 1 | prefix soundness rests on unverified kernel prefix behaviour | native differential `gpu_srs_kernel_prefix_differential` |
| lean-w1 2 | stray-generation release failure neither quarantined nor charged | FIXED `d66f4c8`, test + mutant M8 |
| lean-w1 3 | unparsable budget must be rejected; recheck on hits | P3 rejects; budget immutable. Legacy W13 `parse_budget` still defaults (pinned by its W13 test) — left for the coordinator |
| quint-q2 L2 | any legacy error sticky-disables GPU MSM | FIXED: `FailureClass`; only device faults sticky |
| quint-q2 L3 | legacy failures invisible in fallback counters | FIXED for MSM by routing through the dispatcher; NTT legacy arm is P4's |
| quint-q2 B3 | in-flight result published after poison | FIXED in the MSM arm; transform arm is P4's |
| quint-q2 B1 | Cancelled retried on CPU | FIXED for MSM (`try_msm`; value shim unwinds); transform arm is P4's |
| quint-q4 F2 | legacy pop-then-release drops a live handle | FIXED: `release_entry` for every path |
| quint-q4 F3 | SM query fails closed / over-charges on mixed hosts | recorded (conservative) |
| quint-q4 F4 | prepare sync error leaks a live handle | FIXED: retained, never destroyed on the faulted context |
| quint-q4 F5 | forged lease copy released during `srs_msm` | P6 promotion gate (below) |

## Open risks (OPEN QUESTION)

1. The runtime seam (`fdfaaf3`) touches P4-owned `runtime.rs`; it is isolated and sent to P4
   as `runtime-seam.patch` for sequencing.
2. Concurrent identical misses may both upload (sound, wasteful); the second publishes a
   duplicate entry that ages out by LRU.
3. `srs_msm` stays `Unsupported` until P6. **P6 gate (F5):** kernel submission must pin the
   allocation internally until retirement, independent of any released metadata copy.
4. The host snapshot costs `size_of::<G>()` per point (144 B for `G1Projective`), and every
   hit does an O(n) exact compare (four native workers at ≥ 4096 points).
5. Quarantined bytes persist until the generation changes.
6. The Lean model is sequential; failures fold into `none`; list order stands for LRU; the
   kernel prefix contract is assumed (lean-w1 1 tests it natively for sppark).
7. The agentbus claim for the Lean files was refused (credential prefix); OWNERSHIP.md
   assignment confirmed by astra-gpu.
8. Loki shared-file edits (manifests, `dispatch.rs` MSM arm, `msm.rs`, `gpu_tests.rs`) were
   approved by the coordinator; whoever merges second rebases.
9. A PROVENANCE row is proposed to its owner rather than edited here.
10. P4 (`p4-ntt` 5ce8862) makes `GpuRuntime::with_driver` unsafe; `tests/srs_registry.rs` uses
    it, so whichever of P3/P4 merges second switches to `with_fake` or an `unsafe` block.
11. The legacy `cuda-srs-registry` budget parser still defaults on garbage (W13 compatibility,
    pinned by its W13 test); the P3 registry rejects it.
