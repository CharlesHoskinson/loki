# quint-q2: accelerator dispatch and CPU fallback

**What was checked**
- **Commit:** loki-proofs `a256a9a`.
- **Code:**
  - the P1 seam `src/backend/{mod,cpu,dispatch}.rs`;
  - the legacy sppark arms in `msm_specific` and `EvaluationDomain::{fft,ifft}`;
  - the W9 sticky `failed` flag, the W13 registry and `cuda-ntt`;
  - the `src/lib.rs` exclusivity guard.
- **Contract:** `loki-backend-api` at loki-gpu `eb27233`; `Cancelled` semantics from `cefecbc`.
- **Current main:** loki-proofs main is now `0c4ab9d`. That commit changes only two dependency paths in `Cargo.toml`, so every finding also holds on current main.

**Files (all under `quint/q2/`)**
- **`dispatch.qnt`** models the P1 `Dispatcher`:
  - two callers, with admission and backend entry as separate steps;
  - every `ErrorKind × FailureEffect` combination (33);
  - wrong-length transform outputs and the coset hook.
  - Module `q2_dispatch_seq` is the one-caller version.
- **`legacy.qnt`** models the legacy sppark MSM/NTT arms that run before P1:
  - the sticky `failed` flag and the per-call environment reads;
  - the feature lattice and the `compile_error!` guard.
- **`check.py`** runs the campaign and writes **`results.json`**:
  - invariants, plus mutation and fix witnesses, at 10,000 samples with seed `20261025`;
  - bounded Apalache checks.
- **`mbt/`** is a test crate:
  - quint-connect model-based tests (MBT) of the real `Dispatcher`;
  - a replay of the pinned legacy source;
  - deterministic bug repros.

## Findings

"Latent" means unreachable with today's real accelerator. `LokiGpu::capabilities()` (loki-gpu `crates/loki-gpu/src/lib.rs:38-44`, `runtime.rs:1087-1098`) advertises neither `host_msm` nor `host_transform`. The legacy findings L1-L4 are live in `cuda`, `cuda-ntt` and `loki-sppark` builds.

| id | severity | where (a256a9a) | finding | evidence |
|---|---|---|---|---|
| B1 | medium, latent | `dispatch.rs:115-127` (`failed`), 153, 220-223 | `ErrorKind::Cancelled` is handled like any other error. It bumps `*_fallback` and the request is **recomputed on CPU**. The API says Cancelled "does not request automatic CPU retry of cancelled work" (`loki-backend-api` lib.rs:49-54, added in cefecbc). `design/BUILD.md:191` says "Cancellation never authorizes automatic CPU retry". `Dispatcher::msm` returns a bare `C::Curve`, so it cannot report cancellation at all. | `noCancelledRetry` violated in simulation and Apalache. Repros: `bug_repros::b1_cancelled_{msm,transform}_is_recomputed_on_cpu`. The MBT run against a model with the corrected semantics (`M_FIX_CANCEL`) **fails** on the real code. |
| B2 | low, latent | `dispatch.rs:333-350`; `domain.rs:283-296` → `fft` 412-422 | When `accelerated_coset` fails it bumps `transform_fallback`. The stock body then runs `fft`, whose inner NTT is **offered to the accelerator again**: a second call after an error in one logical operation. It can complete as `transform_accelerated`. That leaves `transform_fallback = 1` with `transform_cpu = 0`, contradicting the counter doc at `dispatch.rs:42-43` ("CPU recomputed them"). Values stay correct. | `transformFallbackRecomputedOnCpu` violated in simulation and Apalache; it holds under `M_FIX_COSET_ACCOUNTING`. Model only (see Limits). |
| B3 | low, latent | `dispatch.rs:143-152`, `200-213` | Time-of-check/time-of-use race: `disabled` is read once, at admission. Suppose another call poisons the dispatcher (for example with `ContextLost`) while a request is in flight. The in-flight `Ok` is still **published**, with `msm_accelerated` bumped after the permanent disable. A backend can also be **entered** after disable, between lines 143 and 148. The value is still the reference value if the backend honours its contract. | `noAccelPublishAfterDisable` and `noAccelEntryAfterDisable` are violated with two callers and hold with one. Apalache finds the 6-step trace. Repro: `bug_repros::b3_result_published_after_concurrent_poison` (deterministic, 2 threads). |
| L1 | low/medium, live with `cuda-ntt` | `backend.rs:392-411` vs `cuda-ntt/src/lib.rs:29-59`, `domain.rs:425-453` | A device error on the MSM side sets the sticky `failed` flag. The MSM side deliberately avoids "a potentially poisoned context" (backend.rs comment before line 381). `cuda-ntt` keeps no failure state, so every later NTT launches on the same device, after an MSM fault or after its own. Correctness then relies on sppark reporting every error. | `noDeviceUseAfterFault` violated in simulation and Apalache; replayed against the pinned source; holds under `M_SHARED_POISON`. |
| L2 | low (performance/liveness) | `backend.rs:404-408` | Any `run()` error sets `failed`, not only device faults. That includes an SRS budget overflow (326-330), an off-curve or out-of-subgroup base (165 registry, 308 legacy), and an SM-query or worker-spawn failure (220-, 108-). One oversized or invalid request disables GPU MSM for the rest of the process. P1 does not do this for `Capacity`. | `stickyOnlyForDeviceFaults` violated; replayed with `LBudget` and `LInvalidBase`. |
| L3 | low (observability) | `msm.rs:169-176`, `domain.rs:412-422`, `dispatch.rs:260-270` | With `loki-sppark`, the legacy arm runs before P1. A legacy GPU failure returns `None`, and P1 counts the request as `msm_cpu` or `transform_cpu`, never `*_fallback`. GPU gates that check for zero `*_fallback` cannot see legacy fallbacks. | `legacyFailuresVisibleInCounters` violated; replayed via `dispatch::global().counters()`. |
| L4 | low | `cuda-ntt/src/lib.rs:33-35`; `domain.rs:431` | The test-only variable `LOKI_NTT_FORCE_ERROR` is read unconditionally on every production NTT and silently disables GPU NTT. `LOKI_NTT_CPU` is also read on every call. | `envInjectionInert` violated; replayed (`setEnv`). |

**Not counted as bugs:**
- `BackendError::preserves_observable_state()` returns true for every `FailureEffect` (`loki-backend-api` lib.rs:93-101). So the assert in `Dispatcher::failed` (`dispatch.rs:116-119`) can never fire, and nothing in Loki checks the preservation claim. A control test (`bug_repros::preserves_observable_state_is_a_tautology`) confirms this.
- An accelerator MSM `Ok(point)` is published with no on-curve or subgroup check (`dispatch.rs:149-151`). A transform `Ok` is checked for length only (210, 214). Value correctness is left to the backend; lean-w3 reached the same conclusion.

**Checked and clean:** `dispatch::ifft` ignores the caller's `divisor` in release builds. It is only a `debug_assert` at `dispatch.rs:320-324`; 1/n is recomputed from `log_n`. Every caller passes exactly `1/2^log_n` (`domain.rs:254, 321-326, 346-351`, with divisors from 135-146), so this is value-safe.

## Properties

Each property below has a verdict for the real code:
- **"Holds"** means no violation in 10,000 samples × 20 steps, plus Apalache to 6 steps where listed in `results.json`.
- **Not vacuous:** every property that holds has a mutation that breaks it, and the campaign catches it.
- **Not trivially false:** every property the real code violates has a fix switch under which it holds.

| property | real code | witness |
|---|---|---|
| result equals the CPU oracle (`resultEqualsOracle`, `legacyResultEqualsOracle`) | holds | mutants `M_PUBLISH_WRONG_LEN`, `M_GPU_WRONG_ON_DECODE`, `M_DOUBLE_NORM` caught |
| the output of a failing call is never published (`noPublishAfterError`) | holds | `M_PUBLISH_WRONG_LEN` caught |
| a poisoning cause always leaves the accelerator disabled (`poisonIsSticky`) | holds | `M_NO_POISON_WRONG_LEN` caught |
| no backend entry or publication after disable | holds with 1 caller; **violated with 2 (B3)** | `M_NO_DISABLED_CHECK` caught with 1 caller; fixed by `M_FIX_RECHECK` |
| no GPU MSM after the legacy sticky flag is set; the flag is permanent | holds | `M_NO_STICKY_CHECK`, `M_CLEAR_ON_NTT_SUCCESS` caught |
| no device use after a device fault | **violated (L1)** | fixed by `M_SHARED_POISON` |
| no CPU retry of cancelled work | **violated (B1)** | fixed by `M_FIX_CANCEL` |
| counters: `msm_accelerated + msm_cpu = msm calls`; `msm_fallback ≤ msm_cpu`; `accelerator_poisoned` = 1 iff disabled; each P1 transform bumps cpu or accelerated | holds | `M_NO_CPU_BUMP_ON_FALLBACK`, `M_POISON_COUNT_EVERY`, `M_NO_TR_CPU_BUMP` caught |
| every transform fallback is recomputed on CPU | **violated (B2)** | fixed by `M_FIX_COSET_ACCOUNTING` |
| legacy failures are visible in the P1 fallback counters | **violated (L3)** | fixed by `M_COUNT_LEGACY` |
| an empty MSM never reaches a backend and returns the identity | holds | `M_NO_EMPTY_GUARD` caught |
| an invalid transform is rejected before any backend, with counters unchanged | holds | `M_NO_VALIDATE` caught |
| a backend is entered only with the capability, the size and the exact ABI | holds | `M_NO_ABI_CHECK` caught |
| a default build never touches an accelerator | holds | `M_DEFAULT_LEAK` caught |
| `loki-gpu-backend` is exclusive with `cuda`/`cuda-ntt` (`compile_error!`, lib.rs:14-18) | holds | `M_NO_COMPILE_GUARD` caught |
| only device faults disable GPU MSM permanently | **violated (L2)** | fixed by `M_STICKY_DEVICE_ONLY` |
| the injection env variable has no effect in production | **violated (L4)** | fixed by `M_NO_ENV_INJECTION` |
| (reachability witness) NTT is retried on the GPU after an NTT error | reached | `nttNeverRetried` violated, as intended |

`python3 check.py` reproduces every row and prints **ALL EXPECTED**: 27 faithful checks, 24 mutation/fix checks and 20 Apalache checks bounded at 6 steps. Apalache finds the B1, B2, B3 and L1 counterexamples.

## Model-based testing against the real code

To run it:

```
cd quint/q2/mbt
CARGO_BUILD_JOBS=4 cargo test --offline --release -- --test-threads=1
CARGO_BUILD_JOBS=4 cargo test --offline --release --features cuda-srs-registry legacy -- --test-threads=1
```

### P1 dispatcher (`replay_p1_dispatcher_0..4`)
- **Volume:** 5 runs × 2,000 traces × 30 steps = 10,000 traces, seeds `20261025`-`20261029`. A single 10,000-trace run exhausts Quint's 4 GB Node heap.
- **Setup:** quint-connect pin `4f018f5`, model `q2_dispatch` / `mbtStep`, driving the real `midnight_proofs::backend::dispatch::Dispatcher` (feature `loki-backend-api`).
- **Real interleaving:** each model caller is a real OS thread. A scripted fake accelerator blocks inside `msm`/`transform` until the trace chooses Ok, wrong length, or `Err(kind, effect)` (all 33 combinations). So two in-flight requests really interleave.
- **Inputs:**
  - ZipTruncate prefixes from unequal slice lengths, zero scalars and the empty MSM;
  - invalid roots, and inverse transforms with a single 1/n;
  - missing capabilities, the wrong ABI and an oversize `max_transform_log`.
- **Oracle:** every published value is compared with an independent naive MSM or O(n²) DFT.
- **State compared:**
  - `accelerator_disabled()` and all 7 counters;
  - each caller's phase, value and source.
  - The source is derived from that call's counter delta.
- **The replay discriminates:** model mutants `M_NO_POISON_WRONG_LEN` and `M_FIX_CANCEL` both make it **fail**. The second failure confirms B1 against Rust.

### Legacy (`replay_legacy_sppark`, `replay_legacy_sppark_registry`)
- **Volume:** 2,000 traces × 30 steps each.
- **Source:** `build.rs` snapshots the a256a9a source:
  - `cuda.rs`, `cuda/backend.rs`, `cuda/convert.rs` and `cuda-ntt/src/lib.rs`;
  - the `gpu_fft` body, cut verbatim from `domain.rs`.
- **Surrogates:** the snapshot is compiled unchanged against CPU FFI surrogates.
  - `loki_gpu_msm` computes a naive MSM over the prepared bases.
  - `loki_ntt` runs stock `best_fft`.
  - The real `loki-cuda-runtime` lock is used, and the snapshot's own `backend.rs`/`convert.rs` unit tests also run.
- **Call order:** each legacy result falls through to the real midnight-proofs P1 continuation, in the order of `msm.rs:169-176` and `domain.rs:412-422, 455-465`.
- **Injected faults:** FFI device errors, non-canonical GPU output, off-curve bases, a `LOKI_GPU_SRS_BUDGET_BYTES` overflow (registry variant) and both env variables.
- **Checked:** `Cache.failed`, FFI call counts, P1 counter deltas from `global()`, and equality of every value with stock CPU.

### Repros (`bug_repros.rs`)
- B1 for MSM and for transform, the B3 two-thread test, and the tautology control.
- Each test asserts the current behaviour and states the contract assertion next to it.

## Limits

- **B2 has no Rust replay.** `accelerated_coset` and `try_accelerated_transform` are `pub(crate)`. They are reachable only through `GLOBAL` with the real `LokiGpu`, which advertises nothing. B2 rests on the model, the source correspondence and the code citations.
- **The B3 entry window is not exercised in Rust.** It lies between admission (line 143) and `backend.msm` (line 148), inside one function, and cannot be forced without editing production code. The MBT collapses the model's Admitted and InAccel phases into one. The B3 *publication* case is exercised deterministically.
- **No GPU.** Accelerator `Ok` values are assumed correct, which is the backend's contract; the fake and the surrogates compute them on CPU. Nothing here is a GPU arithmetic comparison. Asynchronous CUDA execution, streams and context loss are reduced to a synchronous error return.
- **Off-curve bases have no CPU oracle.** blst `multi_exp` depends on evaluation order for off-curve points, because the addition law is not associative off the curve. In a probe, two identical stock CPU calls returned different values. The legacy replay therefore checks only sticky and counter behaviour for `LInvalidBase`. The P1 path does not validate bases either.
- **Not injectable.** `LSpawn` (registry worker-spawn or SM-query failure) is modelled but not injected. NTT replay sizes are 2^4 and 2^10; the 2^28 bound is modelled only.
- **The domain glue is reconstructed.** The legacy driver rebuilds the `fft`/`ifft` call order around the verbatim `gpu_fft` body; it does not compile `domain.rs` with `cuda-ntt`.
- **The feature lattice is simplified.** It covers {cuda, cuda-ntt, cuda-srs-registry, loki-backend-api, loki-gpu-backend} with Cargo's implications. Feature unification across a dependency graph is not modelled. The default-build property in Rust is covered by the existing `tests/backend_default_stock.rs`, which this work did not re-run.
- **Bounds:** Apalache runs to 6 steps, simulation to 20 steps.
