# luna-r1: newcomer lens

## Concerns

- `README.md:73-88,118-174`: The initial quick start showed a downstream `[patch.crates-io]` stanza, then gave build and test commands without naming the working directory. It did not show a first proof check. The fix identifies the Loki repository root for crate commands and presents `./loki-harness/run.sh` as the first end-to-end check. A minimal patched consumer example remains open under TODO T02.
- `README.md:139-154`: The initial CUDA setup treated the MSM and NTT build variables as if they were shared. `build.rs:16-29` reads `CUDA_HOME`, `CUDA_PATH`, `CUDA_TOOLKIT_PATH`, `NVCC`, and `LOKI_CUDA_ARCH`. `cuda-ntt/build.rs:3-10,24-36` reads `CUDA_HOME`, `NVCC` or `PATH`, and the separate `LOKI_NTT_CUDA_ARCH`. The fix lists the variables per adapter, shows the distinct `120` and `sm_120` architecture forms, and checks the compiler under `CUDA_HOME`.
- `README.md:73-82`: P1 added sibling-relative optional path dependencies in `Cargo.toml:253-259`. Cargo resolves them even when backend features are off, so the default CPU build needs a sibling `loki-gpu` checkout. The README now shows the expected checkout layout and patch path.

## Questions

- What public repository URL and supported release or commit should readers use for the git patch alternative in `README.md:88-89`? The placeholder does not provide a usable location. This remains open under TODO T01.
- Should the recommended first example be `examples/simple-example.rs` or the stock-verifier harness? The README now names the harness as the first end-to-end check. A minimal consumer example still needs validation.

## Suggestions

- The module index suggestion is resolved: `docs/modules/README.md:5` now points readers to the proof API, runnable examples and stock-verifier harness.

## Novelty evidence

No novelty claim was assessed in this newcomer lens.
