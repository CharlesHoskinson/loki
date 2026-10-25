# `scripts/`: W9 validation drivers

These scripts reproduce the W9 hardening gate on the original development host. They hard-code host paths (`/home/charles/repos/loki-wt/w9-harden`, `~/.cache/doctorwatson/*.lock`, `~/.local/opt/cuda-13.4.3`, and an AgentBus identity). Treat them as records of how the gate ran. Edit the paths before using them elsewhere.

| File | Role |
|---|---|
| [`verify-w9.sh`](../../scripts/verify-w9.sh) | Full verification of Loki `main`: CPU suite, the W2 gate, then the GPU suites under the GPU lock. Stages are labeled; `W9_GATE_START` resumes at a named stage. Sets `CARGO_BUILD_JOBS=4` and the CUDA 13.4.3 `PATH` and library path. |
| [`w9-cpu-matrix.sh`](../../scripts/w9-cpu-matrix.sh) | Builds once per feature mode (default and `loki-cpu-budget`) and measures sequential verified synthetic proofs at k16/k17 and 20 or 4 Rayon workers. Takes the shared `heavy.lock`. `LOKI_BENCH_K20=1` adds k20. |
| [`w9-host-cargo.py`](../../scripts/w9-host-cargo.py) | Runs one cargo command through the host GPU runner (AgentBus) and preserves its exit status. |
| [`w9-gpu-gate/`](../../scripts/w9-gpu-gate) | Rust package whose `main.rs` runs the seven-stage GPU suffix of `verify-w9.sh` (`gpu msm boundaries`, `gpu proof`, ...) under one host GPU reservation. |

## Difference from stock midnight-proofs 0.8.3

New. No library effect.

## Invariants and pitfalls

- GPU stages must hold the shared lock. Do not run two heavy jobs at once: timings on this shared host are not comparable otherwise.
- Do not run these in documentation builds or CI without a CUDA device.

## Tests

The scripts are the test driver. Results are recorded in [`LOKI-HARDEN.md`](../../LOKI-HARDEN.md) ("Completed full gate") and [`LOKI-CPU.md`](../../LOKI-CPU.md) ("W9 feature isolation").
