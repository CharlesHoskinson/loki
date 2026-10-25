# P4 review record: resident NTT and exact cosets

Owner: loki-p4. Base: loki-gpu `eb27233` (P2-fix merged). Shared runtime files
(runtime.rs, ffi.rs, loki_gpu_ffi.h, runtime.cu) handed to P4 by the
coordinator at eb27233.

## Commits

| Repo | Commit | Subject |
|---|---|---|
| loki-gpu `p4-ntt` | `3839e6b` | Add resident NTT launches with exact cosets and stock differential tests |
| loki-gpu `p4-ntt` | `f716257` | Run the process-runtime trait entry test in its own binary |
| loki-gpu `p4-ntt` | (this commit) | Record P4 gates, measurements and open risks (touches `design/p4/**` only) |
| loki-lean `gpu-spec` | `88150ff` | Model the P4 resident launch pipeline and prove its stock contract |

Every gate below ran on **f716257** with a clean `crates/` and `Cargo.lock`
(the gate script refuses a dirty tree). The record commit changes only
`design/p4/**`, so the binding holds for the merge commit.

`design/p4/shared-files.patch` (the held proposal for P2-owned files) was
retired: after the handoff it was reapplied onto eb27233, reconciled with the
corrected helpers (below) and landed in 3839e6b.

## What P4 adds

- `cuda/ntt.cu`: one launch per transform: load (canonical to Montgomery,
  optional bit-reversal, forward-coset `g^i` on the global index, zero
  padding), radix-2 DIT stages (first min(K,10) fused in shared memory, then
  global), finish (single scale, inverse-coset `g^-i`, optional output
  bit-reversal, Montgomery to fully reduced canonical, truncation to
  `output_len`). Native validation of params, handles (via the runtime's
  check), exact byte sizes and aliasing; `ensure_device` before device work.
- `cuda/ntt_field.cuh` + verbatim sppark 0.1.16 `ff/mont_t.cuh`, `ff/pow.hpp`
  (hashes in `cuda/vendor/sppark-0.1.16/README.md`, NOTICE, PROVENANCE).
- `src/ntt.rs`: shape/root/coset validation, wire-field check (BLS12-381 Fr
  only), `transform_resident`, `transform_host`, `transform_chain`
  (intermediates released, source untouched), `DomainTransforms`, and the
  `TransformBackend` impl for `LokiGpu` on the process runtime.
- `src/runtime.rs`: `TransformLaunch`, driver `launch_transform` /
  `supports_transform` (default Unsupported / false), `launch_transform_fresh`
  modelled on `duplicate_scalars`; fake-driver transform op.
- ABI (additive, `LOKI_GPU_RT_ABI_VERSION` unchanged): `loki_gpu_ntt_params_v1`
  (2112 bytes), `loki_gpu_ntt_scratch_bytes_v1`, `loki_gpu_ntt_launch_v1`,
  `loki_gpu_rt_check_handle_v1`, `loki_gpu_rt_report_cuda_v1`,
  `loki_gpu_rt_ensure_device_v1`. Rust never sees a raw device pointer; the
  native helpers are not public safe Rust.

### Reconciliation with P2-fix (D1–D5)

- Allocation identity via `fresh_id(op)` before any device work.
- Timing calls via `optional()`: a sticky timing failure poisons and publishes
  nothing (test `sticky_timing_failure_poisons_and_publishes_nothing`).
- **Bug fixed during reconciliation:** a scratch free failure after a drained
  launch error previously returned `InputsPreserved`; it now always returns
  `SessionPoisoned` with the driver's error kind and quarantines scratch
  (tests `scratch_free_failure_after_drained_launch_error_poisons`,
  `scratch_free_failure_after_success_publishes_nothing`).
- astra-gpu follow-up: `ensure_device` returns `report_cuda` on a failed
  `cudaGetDevice` instead of letting `cudaSetDevice` overwrite the first
  error. **No native test**: a `cudaGetDevice` failure cannot be injected
  safely on the shared GPU.
- Native tests use `GpuRuntime::native(cfg)` (per test, claim released on
  healthy drop) or `GpuRuntime::process()` (separate binary).

## Gates (f716257)

| Gate | Command | Result | Log |
|---|---|---|---|
| Differential | `flock heavy flock gpu cargo test -p loki-gpu --features cuda --test ntt_differential -- --test-threads=1` | 18/18 (73 s; includes k19/k20 base+extended) | [device-tests](verification/device-tests.txt) |
| Trait entry | `... --test ntt_process_entry` | 1/1 | same |
| Resident | `... --test resident_ntt` | 17/17 + 1 ignored (measurement) | same |
| P2 suites with cuda | `... --test ffi_abi / runtime_contract / p2_fix_regressions / review_regressions` | 13 (+1 ignored) / 39 / 10 / 4 | same |
| No device | `cargo test -p loki-gpu` | all pass (resident_ntt fake 15/15) | same |
| Lean | `lake build LokiLean.BackendNtt` and `lake build` in gpu-spec at 88150ff | success; `#print axioms` on the P4 theorems: propext, Classical.choice, Quot.sound only | — |
| compute-sanitizer | memcheck `--leak-check full`, racecheck, synccheck, initcheck on `resident_ntt native::`, `ntt_differential --skip k19_k20`, `ntt_process_entry` | 0 errors, 0 hazards, 0 leaks on the first two for every tool; see note | [compute-sanitizer](verification/compute-sanitizer.txt) |

Sanitizer note: memcheck leak-check on `ntt_process_entry` exits 99 with
**67 108 864 bytes in 2 allocations, expected**: both are `cudaHostAlloc`
pinned staging slots from `GpuRuntime::build_context`, owned by the
process-wide runtime, a never-dropped static (no public teardown). All other
tools pass on that binary. `k19_k20` was skipped under the sanitizer
(instrumented size beyond the 10-minute device budget); it passes
uninstrumented. racecheck covers the shared-memory fused stages at K ≤ 10 and
K > 10 (k up to 12, extended up to 2^17).

Oracles: stock `midnight-proofs =0.8.3` `EvaluationDomain` with
`midnight-curves 0.3.1`, a naive O(n²) DFT for N ≤ 256, pinned ω and ζ hex,
wrong-root negative control, zero/one/max/impulse/mixed patterns,
canonical↔Montgomery boundary values (R⁻¹, −R⁻¹, 2^255·R⁻¹, R, R², limb
edges), every conversion and both bit-reversed orders, rotations including
multi-wrap, single normalization, inverse-coset prefixes, rejected shapes.

## Measurements (synthetic random inputs, release, RTX 5090, f716257)

ms; full call = upload + launch + download through `transform_host`; first
call and median of 5 warm calls; conversion = host `to_repr`/`from_repr` over
the input; H2D/kernel/D2H = GPU-event deltas around a resident upload,
`transform_resident` and download.

| op | k | ext k | first | warm | to_repr | from_repr | H2D | kernel | D2H |
|---|---|---|---|---|---|---|---|---|---|
| coeff_to_lagrange | 16 | 19 | 8.2 | 3.8 | 1.3 | 1.9 | 0.72 | 0.08 | 0.08 |
| lagrange_to_coeff | 16 | 19 | 4.5 | 3.7 | 0.7 | 1.2 | 0.65 | 0.14 | 0.07 |
| coeff_to_extended | 16 | 19 | 9.0 | 8.4 | 0.7 | 1.2 | 0.70 | 0.15 | 0.15 |
| extended_to_coeff | 16 | 19 | 10.3 | 9.1 | 2.2 | 3.3 | 1.75 | 0.15 | 0.15 |
| coeff_to_lagrange | 19 | 22 | 34.5 | 30.6 | 10.8 | 15.4 | 5.35 | 0.47 | 0.59 |
| coeff_to_extended | 19 | 22 | 67.9 | 63.4 | 5.5 | 14.8 | 5.96 | 0.98 | 1.40 |
| extended_to_coeff | 19 | 22 | 74.6 | 78.5 | 20.7 | 33.1 | 12.8 | 1.03 | 1.24 |
| coeff_to_lagrange | 20 | 23 | 79.2 | 78.8 | 21.7 | 30.7 | 12.4 | 0.90 | 1.27 |
| lagrange_to_coeff | 20 | 23 | 86.2 | 77.2 | 23.8 | 31.9 | 11.0 | 0.90 | 1.17 |
| coeff_to_extended | 20 | 23 | 134.9 | 133.2 | 23.9 | 32.9 | 11.9 | 1.97 | 2.58 |
| extended_to_coeff | 20 | 23 | 137.1 | 134.8 | 39.4 | 67.5 | 23.2 | 2.27 | 2.46 |

Full rows (including k20/ext21) in [measurements](verification/measurements.txt).
Peak scratch: 32 MiB at ext k 23. Reading: kernels are 1–2 % of a host call.
The call is dominated by host-side work: canonical conversion plus the P2
staging path (H2D event time includes chunked staging at ~2.7 GB/s effective;
D2H ~25 GB/s). The win is in resident chains (one upload/download per chain),
not single host calls.

## Cargo.lock delta

+854 lines, 90 new packages (14 → 104 `name =` entries), all reached only
through `[dev-dependencies]` (`midnight-proofs =0.8.3`, `midnight-curves 0.3.1`).
Checked with `cargo tree -p loki-gpu -e normal --features cuda`: no midnight
crate in the normal tree.

## Open risks

1. Header is additive with ABI version unchanged, contradicting P2's "header
   unchanged" note; API owner (astra-gpu) must accept the additions.
2. `ensure_device` error-preservation fix has no native test.
3. The process runtime holds pinned slots for the process lifetime (no public
   teardown); visible to leak checkers.
4. Lean models the launch arithmetically over any `IsBitrevInNtt` core
   (instantiated by `bfly`); the CUDA stage schedule, the Montgomery wire
   conversion and native memory safety are covered by tests and the
   sanitizer, not by proof.
5. The device trusts canonical inputs: `validate_params` checks constants,
   but element data comes from runtime uploads in canonical `to_repr` form;
   non-canonical bytes written by a future producer would not be re-checked.
6. Debug builds compile CUDA with `-G` (slow); measurements are release only.
7. Host-call path is conversion/staging-bound; resident chains need P5+
   consumers to realise the speedup.
8. `cudaDeviceReset` in recovery invalidates handles by epoch; chains across a
   reset rely on host checkpoints (tested with the fake driver only).
