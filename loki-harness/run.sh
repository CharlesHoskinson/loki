#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
export CARGO_BUILD_JOBS=4
export RAYON_NUM_THREADS=1
# Bound the complete invocation, including compilation. A cold build can need
# prewarming on a busy host: cargo build --release --locked --bins
exec timeout --kill-after=3s 292s cargo run --release --locked --bin gate -- "$@"
