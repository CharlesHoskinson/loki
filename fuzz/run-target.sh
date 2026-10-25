#!/usr/bin/env bash
# Usage: fuzz/run-target.sh <target> <seconds> [extra cargo-fuzz args...]
# Runs one libFuzzer target with the shared options and logs to fuzz/logs/.
set -euo pipefail
cd "$(dirname "$0")"
t=$1; secs=$2; shift 2
mkdir -p "corpus/$t" logs
export RAYON_NUM_THREADS=${RAYON_NUM_THREADS:-4} CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
log="logs/$t-$(date +%Y%m%dT%H%M%S)${LOG_TAG:+-$LOG_TAG}.log"
echo "log: $log"
cargo +nightly-2026-10-11 fuzz run "$@" "$t" "corpus/$t" -- \
  -max_total_time="$secs" -timeout=120 -rss_limit_mb=8192 -max_len=4096 \
  -print_final_stats=1 -print_pcs=0 >"$log" 2>&1
