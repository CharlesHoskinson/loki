# quint-q4: GPU SRS caching (W13 registry, legacy cache, P3 contract)

Agent quint-q4, branch `loki/quint-v2`, commit `fcba4c2`. Subject: loki-proofs
main `a256a9a15ff18ae12a8131b86cff576787fa5e3c`; loki-gpu main `eb27233` (API
and stub); P3 draft read-only at `~/repos/loki-gpu-wt/p3-srs` `d66f4c8`; sppark
0.1.16 from the Cargo registry. Lean counterpart: lean-w1
`LokiLean/SrsRegistry.lean` (lean-components `23a85af`, `3ab6f4f`).

## Verdict

No soundness bug was found. On every reachable trace, every GPU-path MSM used
exactly the group elements of the requested prefix. This was checked three
ways: in the model; in the real pinned `backend.rs` against an independent
group sum (10,000 replay steps, CPU surrogate); and on the RTX 5090 (51
device results plus a directed test). The exact-prefix comparison, the
same-address mutation miss, whole-capacity validation, one entry per address,
LRU eviction, budget accounting with a fixed budget, quarantine after a failed
registry release, the ≤ 4 native workers and full worker joins all hold.

There are three low-severity findings (F2–F4) with repros, and one latent P3/P6
advisory (F5).

| # | Severity | Where | Finding |
|---|---|---|---|
| F1 | none | — | Hit exactness holds. The address is only a filter; content compare is exact x/y/z limbs over the consumed prefix. |
| F2 | low | `backend.rs:350-354` (legacy) | A failed eviction release drops the Entry first, so the possibly-live handle is neither quarantined nor tracked. |
| F3 | low | `backend.rs:220-243` (registry) | The SM query fails closed if any CUDA ordinal fails, including devices sppark ignores. On mixed hosts the charge over-approximates (never under). |
| F4 | low | `cuda/msm.cu:31-35` + `backend.rs:359` | A prepare error after allocation (sync failure) leaks a live resident outside entries and budget. |
| F5 | latent | P3 `srs.rs:427-454,718-745`, API `lib.rs:588-604` | A forged copy of a live lease (public safe constructor) can be released while `srs_msm` runs. P6 should pin across device work (defense in depth; the API makes "release after dependent work" a caller obligation). |

Already reported by others; I confirm them and do not re-file:
- quint-q2: any `run()` error is sticky (`backend.rs:404-408`).
- lean-w1: the budget is re-read only on misses (`:324-325`); `parse_budget` silently falls back to 4 GiB (`:186-190`); the W13 prefix argument needs a new sppark assumption (runtime n ≤ capacity) beyond W9's `KernelOkOnValid`.

## Artifacts

| Path | Content |
|---|---|
| `quint/q4/srs_w13.qnt` | W13 registry (`REGISTRY=true`) and legacy two-slot model, native ground truth, worker join, fault injection; `w13_registry`, `w13_legacy` |
| `quint/q4/srs_w13_test.qnt` | literal byte vectors, worker bound over ranges, join semantics, basis shape, rewarm/mutate run |
| `quint/q4/srs_w13_mutants.qnt` | 18 killing mutants + 1 benign control |
| `quint/q4/srs_p3.qnt` | P3 forward contract + 9 mutants |
| `quint/q4/check.py`, `results.json` | campaign runner and its last results |
| `quint/mbt/q4-srs/` | quint-connect MBT crate: pinned source snapshot, CPU-surrogate and real-GPU native layers, directed repros |
| `quint/q4/evidence/` | GPU replay logs (2 runs), CPU replay log, Apalache log |

## Source correspondence (a256a9a unless noted)

| Model element | Rust / native source |
|---|---|
| `msmBody` entry: `n = min(|scalars|,|bases|)`, `n == 0` → identity | `backend.rs:392-397` |
| `Disabled` (sticky) | `backend.rs:266-268`, set at `:406-408` |
| registry hit `isHit` (address, `n <= len`, exact prefix) | `backend.rs:271-284`; `same_prefix` `:151-159`; `same_coordinates` `:96-102` |
| legacy hit (address, `len == n`, full compare of `bases[..n]`) | `backend.rs:285-294`, `:405` |
| hit moves entry to MRU | `backend.rs:295-297`, `:380-381` |
| registry whole-capacity validation | `backend.rs:302`, `validate` `:162-183` |
| legacy serial validation of `bases[..n]` | `backend.rs:304-319` |
| `queriesOk`, `chargedSm` (max over ordinals) | `device_sm_count` `backend.rs:220-243` |
| `residentBytes` | `resident_bytes` `backend.rs:197-217` = sppark `pippenger.cuh:351-375` (also driver `sppark_bytes`, from C++) |
| oversize rejection | `backend.rs:323-330`, `parse_budget` `:186-190` |
| `evict`: same address first, then LRU while `used > budget - bytes` | `backend.rs:331-347` |
| registry release failure → quarantine | `release_entry` `backend.rs:247-263` |
| legacy `pop_front` then release | `backend.rs:350-354` |
| `PrepareCtorErr` / `PrepareSyncErr` / no eligible device | `cuda/msm.cu:25-44` (alloc `:31-32`, sync `:35`); `all_gpus.cpp:39-54` |
| `selectedSm` (first eligible: major ≥ 7, cooperativeLaunch) | `sppark/util/all_gpus.cpp:14-28` |
| upload, null check, push | `backend.rs:355-381` |
| `InvokeErr` (entry kept) | `backend.rs:377-388`; `cuda/msm.cu:46-74` |
| `workerCount`, `joinWorkers` | `parallel_chunks` `backend.rs:108-148`; serial below 4096 `:112-114` |
| P3 key compatibility | API `loki-backend-api/src/lib.rs:549-579`; draft `srs.rs:155-164` |
| P3 lease, forged metadata, release | API `lib.rs:581-655`; draft `check_lease` `srs.rs:427-454`, `release` `:718-745` |
| P3 eviction only of unleased/unpinned | draft `srs.rs:166-168`, `:575-640`; `design/PLAN.md:75-92` |
| P3 generation/context loss | draft `srs.rs:227-236`, `:446`; `design/MSM.md` |
| P3 stubs on main | loki-gpu `eb27233` `crates/loki-gpu/src/srs.rs:108-129` |

## Properties

| Invariant (W13) | Meaning | Killed by |
|---|---|---|
| `hitExact` | GPU result = requested prefix's elements | `mut_addr_only`(+legacy), `mut_first_block`(+legacy), `mut_no_len` |
| `residentValid` | resident handles hold validated points | `mut_validate_prefix` |
| `snapshotMatchesUpload` | snapshot = upload | `mut_refresh_snapshot` |
| `noDangling` | no entry refers to a released handle | `mut_release_keep` |
| `onePerAddress` | ≤ 1 capacity per address (registry) | `mut_no_stale_evict` |
| `legacyTwoSlots` | legacy ≤ 2 entries | `mut_legacy_three` |
| `budgetRespected` | healthy ⇒ Σ bytes ≤ budget (fixed budget) | `mut_budget_used` |
| `chargeConservative` | charge ≥ sppark's actual allocation | `mut_ordinal0_sm` (astra's original), `mut_charge_n` |
| `noUntrackedWhileHealthy` | healthy ⇒ live native = cached | `mut_evict_no_release` |
| `noNativeAfterFail` | no native calls once disabled | `mut_skip_failed` |
| `workersBounded` | ≤ 4 native workers | `mut_floor_chunk` (5 at 4097) |
| `workersDrained` | all workers joined; no unwind | `mut_short_join` (pre-fix collect) |
| `releaseQuarantined` | failed release keeps the handle (registry) | `mut_release_drop`; **legacy violates (F2)** |
| `chargeExactHomogeneous` | exact on homogeneous hosts | holds |
| `chargeExact` | exact on all hosts | **violated on mixed hosts (F3)** |
| `noUntrackedLive` | live allocations always cached | **violated by sync error (F4)** |

Benign control `mut_legacy_len` keeps `safety`: the legacy exact-length rule is
a performance choice. Basis non-confusion: W13 has no basis tag and relies on
content equality, which is sound. Confusion only arises when contents coincide,
and then it is harmless; witness `wCoefLagResident`. In P3 the tag is
load-bearing (`lag(0) == coef(1) == [G]`, so `p3_ignore_basis` violates
`basisMatch`).

P3 invariants: `noEvictLeased`, `pendingResident`, `refsExact`, `bytesExact`,
`residentValid`, `exactContent`, `noCrossGen`, `basisMatch`. Mutants:
`p3_digest_only`, `p3_evict_leased`, `p3_ignore_gen`, `p3_publish_stale`,
`p3_ignore_basis`, `p3_trust_release`, `p3_release_inflight`,
`p3_leak_on_evict`, `p3_no_validate`; each violates its target. The model's
Lagrange rule (entry length = request count) is stricter than the draft's
`compatible`, which also lets a same-domain Lagrange prefix hit
(`point_count <= snapshot.len()`). Both are sound because content is compared
exactly.

## Campaigns (`python3 quint/q4/check.py --samples 10000`; seed 20261025; Rust evaluator, 4 threads)

All behave as expected (2 m 40 s). Witness counts are per 10,000 traces:

- **W13 registry, all faults** (24 steps): safety holds.
  - Hits and misses: hit 4136, prefix hit 3381, n−1 hit 2484, same-address stale miss 3335, two resident 4069, coef+lag resident 3718.
  - Evictions and errors: evict 3805, budget reject 1720, invalid reject 1904, query reject 1802, release fail 117, disabled 7188.
  - Workers and accounting: 4 workers 8747, worker panic 1736, mixed overcharge 2783.
- **W13 registry, MBT fault set**: holds.
- **Legacy, all faults**: holds.
  - hit 4118, n−1 hit 784, stale miss 4839, evict 7512, two resident 8320, release fail 437.
- **P3**: holds.
  - hit 6754, published 2012, coefficient prefix 1007, Lagrange 978, capacity 3782, lost after context loss 172, forged/stale rejected 10000.
- Expected counterexamples found: F2, F3, F4.
- 27/27 mutants killed.
- `quint test srs_w13_test.qnt`: 5 passing (byte vectors 2,971,648 / 55,409,664 / 109,755,392 and the doc total 330,330,112).
- Apalache 0.56.1: P3 `safety` holds at depth 1. Depth 2 did not finish (crashed after about 10 min). W13 was not attempted.

## MBT / ITF replay against the real code (`quint/mbt/q4-srs`)

The build script snapshots `src/poly/kzg/msm/cuda/{cuda.rs,backend.rs,convert.rs}`
from `a256a9a` via `git show` and compiles them unchanged. The driver is
appended as a child module so it can read `CACHE`. quint-connect (pinned
`4f018f5`) generates traces from `srs_w13.qnt` and steps the real `msm()`.
Each model point expands to 1024 identical real G1 points, so the real 4096
worker threshold and byte formula apply. Representation variants are Jacobian
rescalings (same element, different limbs).

After every step, the driver checks:
1. The full state against the model.
2. Every `Ok` result against an independent Σ (1 + i mod 3)·e_i·G.
3. The native contract: no use or double release of a released or quarantined handle, no n above capacity.

| Mode | Command | Traces × steps | Result |
|---|---|---|---|
| legacy, CPU surrogate | `cargo test --release --offline --features cuda -- --test-threads=1` | 200 × 24 | pass; 871 results (132 hits, 739 misses), 471 evictions, 36 stale misses |
| registry, CPU surrogate | `... --features cuda-srs-registry ...` | 200 × 24 | pass; 631 results (283 hits, 348 misses; 176 prefix, 89 n−1), 123 stale misses, 159 evictions; budget/query/release/invoke/prepare/invalid errors; 233 worker probes, never unwound |
| registry, RTX 5090 | `--features cuda-srs-registry,gpu` under `flock heavy.lock flock gpu.lock timeout 600` | 12 × 20 + directed | pass (11 tests); 51 device results, 17 evictions, 14 stale misses; directed rewarm/mutate passes on device |

The surrogate replaces only `loki_gpu_{prepare,msm,release}` and the two CUDA
runtime queries. It provides fault injection and a fake device table.

GPU mode links the pinned `cuda/msm.cu` and sppark 0.1.16, with the C symbols
renamed `*_native` and counting shims in Rust. The C++ bodies and the Rust
under test are unchanged.

Directed tests:
- `q4_legacy_release_failure_quarantines_handle` (F2, ignored, fails with `--ignored`).
- `q4_mixed_host_accounting` (F3).
- `q4_prepare_sync_error_leaks_untracked_allocation` (F4).
- `q4_rewarm_then_mutate_same_address_misses`.
- `q4_sppark_formula_matches_backend`.
- `q4_point_encoding_matches_model`.

## Findings

**F2 (legacy, low).** In `backend.rs:350-354`, `pop_front()` runs before
`loki_gpu_release(...)?`. On a release error, the possibly-live handle and its
snapshot are gone and never quarantined. The registry path (`:247-263`) keeps
them. `failed` is then set, so this is a leak only. Repro: `cargo test --release
--offline --features cuda -- --ignored
q4_legacy_release_failure_quarantines_handle`.

**F3 (registry, low, mixed hosts).** `device_sm_count` errors if any ordinal
query fails (`:233-239`), while sppark uses only its first eligible device. The
charge is the max SM count: never under (astra ordinal-0 = `mut_ordinal0_sm`,
killed), but over on mixed hosts. Example: [200 ineligible, 84 eligible] with
4096 points charges 3,309,568 B against 3,131,392 B. Repro:
`q4_mixed_host_accounting`.

**F4 (low).** `msm.cu:31-35` allocates, then `sync()` can throw;
`backend.rs:359` returns via `?` without releasing or recording the handle.
Repro: `q4_prepare_sync_error_leaks_untracked_allocation`.

**F5 (P3/P6, latent).**
- `SrsLease::from_untrusted_metadata` is public and safe; `check_lease` accepts an exact copy.
- Duplicate releases are safe (keyed by lease id), but releasing a copy during an in-flight `srs_msm` permits eviction under the kernel.
- `srs_msm` is `Unsupported` today (`srs.rs:827-835`). P6 should pin under the lock for the device duration, as `acquire` does for comparison (`:517-543`).
- Model: `pendingResident`, `p3_release_inflight`.

**Notes.**
- Panic containment applies only to the ≥ 4096-point path. Below it, the closure runs on the caller thread holding DEVICE and CACHE (`:112-114`); a panic would poison both and unwind into the prover. The closures are not known to panic on any input.
- W13 keys on address, so equal content at a distinct address misses where P3 requires a hit; that is performance only.
- Misses validate the whole capacity before the oversize check (`:302` before `:323-330`), wasting CPU on entries that will be rejected.

## Limits

- The surrogate checks cache logic, not sppark arithmetic. Device arithmetic was checked only on ≤ 5120 points at 170 SMs.
- The sppark runtime-n ≤ capacity assumption (`pippenger.cuh:448-565`) is exercised, not proved (lean-w1 `w9Kernel_not_enough`).
- Not modeled: host-thread concurrency (the DEVICE+CACHE locks serialize; W4's replay covers concurrent callers), budget environment changes mid-process, NTT/scratch/driver memory, and real thread-spawn failure (model only).
- Mixed-GPU behavior is replayed only via a fake device table.
- P3 is a forward model; the draft was read, not replayed.
- Apalache: P3 depth 1 only.
- Process note: the two GPU runs (07:42:56–07:43:03Z and 07:53:00–07:53:44Z) fell inside the coordinator's 07:16 W12 HOLD, which I had missed. No W12 job was queued or running in those windows; this was reported to the coordinator.

## Reproduce

```sh
cd ~/repos/loki-wt/quint-v2
python3 quint/q4/check.py --samples 10000
cd quint/mbt/q4-srs
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=2 cargo test --release --offline --features cuda -- --test-threads=1
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=2 cargo test --release --offline --features cuda-srs-registry -- --test-threads=1
cargo test --release --offline --features cuda -- --ignored --test-threads=1   # F2 repro (fails)
export CUDA_HOME=$HOME/.local/opt/cuda-13.4.3 PATH=$CUDA_HOME/bin:$PATH
cargo test --release --offline --features cuda-srs-registry,gpu --no-run
flock ~/.cache/doctorwatson/heavy.lock flock ~/.cache/doctorwatson/gpu.lock \
  timeout 600 cargo test --release --offline --features cuda-srs-registry,gpu -- --test-threads=1
```
