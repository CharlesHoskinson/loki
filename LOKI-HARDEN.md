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
proof configurations and the combined hardened CPU-budget GPU suite. It holds
the shared heavy-job lock for each CPU stage, releasing it before host GPU
queue waits. `W9_GATE_START` resumes at a named stage; earlier results must be
retained and checked separately when using it. Logs are under
`target/w9-validation/gate/`; runner
logs are under `target/gpu-runs/`.

For the resumed run, the first six stages completed before the GPU suffix was
batched. `scripts/w9-gpu-gate/` runs the same seven remaining Cargo commands
sequentially under one host reservation, writing each stage's log and
`batch-results.tsv`. Its dependency-free driver passes `cargo check`; all seven
argument lists were compared with `verify-w9.sh`. Submit it through the runner:

```text
run --offline --manifest-path /home/charles/repos/loki-wt/w9-harden/scripts/w9-gpu-gate/Cargo.toml --target-dir /home/charles/repos/loki-wt/w9-harden/target/w9-gpu-driver --features cuda
```

The driver's `cuda` flag requests the runner's device lock; it does not add a
library dependency or change the default prover. This avoids repeated queue
waits between short verification stages.

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

## Completed full gate

All 13 stages passed across the controlled resume. The original idle local
wrapper was stopped to release its CPU reservation; its completed results and
already-submitted host jobs were retained. The seven-stage host suffix returned
exit 0 after 1,301.889 seconds queued and 601.473 seconds running. A transient
AgentBus receive error required reattaching the result collector; no host test
was cancelled. The superseded waiter also left one extra boundary job queued,
which passed separately. The table below uses the definitive batch logs.

| Stage | Result |
| --- | --- |
| Default `cargo test --release --offline -- --test-threads=4` | 34 unit, two integration, nine documentation tests pass; two timing tests ignored |
| `./loki-harness/run.sh` | k10/k14/k17 cross-verify and equal VKs; no-lookup proof bytes equal; documented lookup-order differences remain |
| `cargo run --release --offline --example cpu-bench -- 16 3 target/cpu-fixture` | Three verified proofs with identical hashes |
| `cargo test --release --offline --manifest-path tests-stock/Cargo.toml` | Registry stock verifier accepts the fixture and rejects corruption |
| GPU library, `cuda,cuda-ntt,loki-profile` | 40 pass, 40 ignored; forced NTT fallback and invalid-SRS rejection pass |
| GPU MSM differential | Pass, 210.93 s test time |
| GPU MSM large boundaries | Pass, 508.43 s test time |
| GPU MSM proof and separate stock verifier | Both accept the 416-byte proof and reject mutated public input |
| k17 NTT with separate stock verifier | Three proofs accepted, mutations rejected; 17 completed GPU transforms per proof |
| Four concurrent provers, `cuda,cuda-ntt,loki-harden` | Pass, 1.58 s; every proof verifies and both successful-dispatch counts match the reference |
| GPU library, `cuda,cuda-ntt,loki-harden,loki-cpu-budget` | 45 pass, 40 ignored |
| Four concurrent provers with CPU budget | Pass, 1.64 s; same verification and dispatch assertions |

The proof/stock-verifier row covers two separate gate stages. A final audit
checked all 13 expected stage names and exit codes. Complete evidence is
`target/w9-validation/resume-full-gate.log`, `gate/batch-results.tsv`,
`gate/batch-runner-result.json` and the individual stage logs. The standalone
NTT verifier lockfile now records the common runtime and restored BLST dependency.
`cargo tree --offline --locked --manifest-path tests/ntt-stock/Cargo.toml
--features gpu -i loki-cuda-runtime` checks that final lockfile without rebuilding.

Curve/subgroup validation is serial on each new upload and has a cold-cache
cost. The boundary regression includes CPU MSM oracles and does not isolate
that cost; no GPU performance improvement is claimed here. Manual tests still
ignored by the supplied gate were not implicitly run by the library suites.

Existing stock limitations remain: mixed committed/plain multi-proof transcript
ordering can cause stock rejection, and lookup-rich stock proofs are not a
universal fixed-RNG byte oracle. See `LOKI-QUINT.md`. Rust 1.76 and 32-bit target
builds have not been executed. The common GPU mutex covers these adapters in
this Cargo dependency graph; it does not coordinate separate OS processes.

Integration note from W10: profiler commit `0486f79` also edits `gpu_fft`.
Resolve that merge by retaining W9's error-to-CPU path and recording CPU fallback
as failed GPU attribution. BLST thread metadata must follow the opt-in
`loki-cpu-budget` policy. W10 is outside this branch's verification scope.
