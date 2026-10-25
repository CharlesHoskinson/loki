#!/usr/bin/env bash
# verify-main.sh --tier cpu|native [--tree DIR] [--heavy]
#   cpu    MERGE-CPU gate: needs no GPU and no gpu.lock. Source builds, fake-driver tests, CPU suite, stock verifier.
#   native RECEIPT-NATIVE gate: the GPU suites (delegates to scripts/verify-w9.sh). Run post-merge, under doctorwatson heavy+gpu.
# --heavy wraps each cpu step in the live factory `python -m factory heavy` (same form as the heavy order) when a build is large.
# LOKI_VERIFY_CMD overrides the cpu steps (used by train self-test).
set -uo pipefail
tier=""; tree="$PWD"; heavy=0
while [[ $# -gt 0 ]]; do case "$1" in
  --tier) tier="$2"; shift 2;; --tree) tree="$2"; shift 2;; --heavy) heavy=1; shift;;
  *) echo "usage: $0 --tier cpu|native [--tree DIR] [--heavy]" >&2; exit 64;; esac; done
[[ "$tier" == cpu || "$tier" == native ]] || { echo "--tier cpu|native required" >&2; exit 64; }
cd "$tree" || exit 66
if [[ "$tier" == native ]]; then exec "$(dirname "$0")/verify-w9.sh"; fi
if [[ -n "${LOKI_VERIFY_CMD:-}" ]]; then exec bash -c "$LOKI_VERIFY_CMD"; fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
# no cuda features: nothing here may touch a GPU. loki-gpu fake-driver tests run as plain cargo tests.
steps=(
  "cargo check --offline --workspace --all-targets"
  "cargo test --offline --release -- --test-threads=4"
  "cargo test --offline --release --manifest-path tests-stock/Cargo.toml"
)
[[ -f loki-gpu/Cargo.toml ]] && steps+=("cargo test --offline --release --manifest-path loki-gpu/Cargo.toml --workspace")
fail=0
for s in "${steps[@]}"; do
  echo "### $s"
  if (( heavy )); then
    d="${DW_FACTORY:-$HOME/repos/Moriarty-factory}/models/doctorwatson"
    ( cd "$d" && DW_REPO="$tree" /usr/bin/python3.14 -m factory heavy --id "verify-cpu-$(basename "$tree")" \
        --hypothesis "MERGE-CPU tier: $s" -- bash -c "cd '$tree' && $s" ) || fail=1
  else bash -c "$s" || fail=1; fi
  (( fail )) && { echo "CPU TIER FAILED at: $s" >&2; exit 1; }
done
echo "CPU TIER PASS"
