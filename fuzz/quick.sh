#!/usr/bin/env bash
# Stable, deterministic gate for the Loki differential fuzz checks.
# Runs the proptest versions of all five targets, the explicit Lean-contract
# cases and the mutation-harness port, in every feature configuration that
# changes prover/verifier behaviour. Test time (excluding builds) < 3 min.
#
#   fuzz/quick.sh            # CPU configurations
#   QUICK_GPU=1 fuzz/quick.sh  # also cuda-ntt and cuda (GPU MSM) configurations;
#                              # needs CUDA + GPU, takes the shared GPU lock per
#                              # configuration, and is outside the 3 minute budget.
set -euo pipefail
cd "$(dirname "$0")"
export RAYON_NUM_THREADS=${RAYON_NUM_THREADS:-4} CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
configs=("" "truncated" "truncated-loki-only")
if [[ -n "${QUICK_GPU:-}" ]]; then
  configs+=("cuda-ntt" "cuda")
fi
lock() {
  case "$1" in
    cuda*) flock "$HOME/.cache/doctorwatson/gpu.lock" "${@:2}" ;;
    *) "${@:2}" ;;
  esac
}
total=0
for features in "${configs[@]}"; do
  args=(--release --locked -p loki-diff)
  [[ -n "$features" ]] && args+=(--features "$features")
  cargo +stable test "${args[@]}" --no-run >/dev/null 2>&1 || cargo +stable test "${args[@]}" --no-run
  start=$(date +%s)
  if [[ "$features" == cuda ]]; then
    # GPU MSM: explicit Lean-contract MSM cases only (each GPU MSM re-uploads bases).
    lock cuda cargo +stable test "${args[@]}" --test cases -- msm partial_commit --test-threads=1
  else
    # cases.rs counts GPU calls, so it runs single-threaded.
    lock "$features" cargo +stable test "${args[@]}" --test cases -- --test-threads=1
    # GPU call counters and LOKI_NTT_CPU are process-wide: serialize GPU configs.
    threads=(); [[ "$features" == cuda* ]] && threads=(-- --test-threads=1)
    lock "$features" cargo +stable test "${args[@]}" --test quick --test soundness_mutation "${threads[@]}"
  fi
  secs=$(( $(date +%s) - start ))
  total=$(( total + secs ))
  echo "quick.sh: features='${features:-default}' passed in ${secs}s"
done
echo "quick.sh: all configurations passed; test time ${total}s"
if (( total > 180 )); then
  echo "quick.sh: WARNING test time exceeds the 3 minute budget" >&2
fi
