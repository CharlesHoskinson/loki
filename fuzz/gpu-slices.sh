#!/usr/bin/env bash
# Usage: fuzz/gpu-slices.sh <target> <total-seconds> [cargo-fuzz args...]
# Runs a GPU fuzz target in <=10 minute slices, taking the shared GPU lock per
# slice and releasing it between slices; the corpus accumulates across slices.
set -euo pipefail
cd "$(dirname "$0")"
t=$1; total=$2; shift 2
slice=${SLICE:-600}
done_s=0; i=0
while (( done_s < total )); do
  s=$(( total - done_s < slice ? total - done_s : slice ))
  i=$((i + 1))
  LOG_TAG="${LOG_TAG:-gpu}-slice$i" flock "$HOME/.cache/doctorwatson/gpu.lock" ./run-target.sh "$t" "$s" "$@" || exit $?
  done_s=$(( done_s + s ))
  sleep 20   # let queued GPU jobs take the lock
done
