# W9 correctness and robustness

W9 retains stock default BLST threading. Enable `loki-cpu-budget` to opt into
serial BLST with Rayon MSM partitioning; measurements and feature-unification
limits are in `LOKI-CPU.md`. Enable `loki-harden` for checked domain construction
in key generation/key reading and widened rotation arithmetic. GPU corrections
apply when their existing `cuda` / `cuda-ntt` features are enabled.

The contracts are `/home/charles/repos/loki-lean/SPEC.md` and
`LokiLean/{Ntt,Msm,Refinement,Plonk}.lean`. These runtime checks provide empirical
evidence for component obligations; they do not complete the Lean proofs.

## GPU behavior

NTT errors return to `best_fft`, with one warning per process. The adapter
transforms a temporary limb buffer, so a failure leaves the CPU input intact.
The inverse returns early only after GPU success: sppark supplies `1/n`, while
the CPU branch applies its divisor. `gpu_ntt_forced_error_falls_back` injects an
error in a subprocess and compares all five domain conversions to CPU oracles.

MSM and NTT acquire the same `loki-cuda-runtime` mutex. Resident allocation,
invocation and release use device-then-cache lock order. Upload validation
checks curve membership before normalization and subgroup membership before
FFI. Cache hits compare all coordinates to the validated source copy. The
corruption regression rejects both an off-curve point and an order-three point.
The four-thread proof regression verifies every proof and requires exactly four
times the successful MSM and NTT dispatches of a reference proof, excluding
setup. It runs in both default and CPU-budget modes.

## Arithmetic edges

The original rotation regression panicked when a `2^32` domain size truncated
to zero in `i32`. The hardened path retains `usize` dimensions and uses `i128`
for the index plus signed rotation times stride before modular reduction.
Its test exercises large dimensions without allocating a large domain.

`ConstraintSystem::degree()` has a floor of three (`Plonk.degree_ge_three`),
so ordinary degrees cannot underflow `j - 1`. However, a public
`set_minimum_degree(2^32)` on a 64-bit host truncates to zero at the old `u32`
cast. The original key-reader regression panicked in domain construction in
release mode. Hardened keygen and key reading call `EvaluationDomain::try_new`
before truncation, returning `Error::Synthesis` / `io::ErrorKind::InvalidData`.
It also rejects zero degree, invalid shifts, size multiplication overflow,
unsupported field domains and vector address-capacity overflow. The stock
`new(u32, u32)` signature and behavior remain available for compatibility.

Negative-control logs from the initial run are retained in
`target/w9-validation/{rotation-red,degree-red}.log` and
`target/gpu-runs/*w9-red-{injected,upload}.log`. No production witnesses are
used in these fixtures.

## Reproduction

Build with `CARGO_BUILD_JOBS=4`. `scripts/verify-w9.sh` is the supplied full
gate adapted to this worktree, with complete logs and failure propagation.
It routes GPU commands through `scripts/w9-host-cargo.py`, adds both concurrent
proof configurations and the combined hardened CPU-budget GPU suite, and holds
the shared heavy-job lock. Logs are under `target/w9-validation/gate/`; runner
logs are under `target/gpu-runs/`.

Additional CPU configurations:

```sh
RAYON_NUM_THREADS=4 cargo test --release --offline --features loki-harden,loki-cpu-budget -- --test-threads=1
RAYON_NUM_THREADS=4 cargo test --release --offline --no-default-features --features loki-harden --lib --test cpu -- --test-threads=1
```

The resumed run passed the combined CPU configuration (40 unit, two integration,
nine documentation tests) and no-default-features configuration (39 unit, one
integration test). Logs are `target/w9-validation/resume-cpu-suite.log` and
`resume-no-default-suite.log`. The fresh `cuda,cuda-ntt,loki-harden` library run
passed 44 tests, with 40 ignored; the forced-error and invalid-base regressions
both passed. `resume-gpu-suite.log` records 462.671 seconds queued and 12.504
seconds running, so queue delay is not a test or proof timing.

Existing stock limitations remain: mixed committed/plain multi-proof transcript
ordering can cause stock rejection, and lookup-rich stock proofs are not a
universal fixed-RNG byte oracle. See `LOKI-QUINT.md`. Rust 1.76 and 32-bit target
builds have not been executed. The common GPU mutex covers these adapters in
this Cargo dependency graph; it does not coordinate separate OS processes.

Integration note from W10: profiler commit `0486f79` also edits `gpu_fft`.
Resolve that merge by retaining W9's error-to-CPU path and recording CPU fallback
as failed GPU attribution. BLST thread metadata must follow the opt-in
`loki-cpu-budget` policy. W10 is outside this branch's verification scope.
