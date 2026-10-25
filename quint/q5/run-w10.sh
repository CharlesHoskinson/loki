#!/usr/bin/env bash
# Emit proofs for main and W10 CPU copies, then compare. Fixed Rayon count:
# stock vanishing randomness depends on the worker count.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
export RAYON_NUM_THREADS=${RAYON_NUM_THREADS:-4} CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
out="$here/results"; mkdir -p "$out"
[ -d "$here/w10/copies/head" ] || "$here/w10/setup.sh"
run() { local name=$1 dir=$2; shift 2; (cd "$here/$dir" && cargo run --offline --release -q "$@") > "$out/$name.jsonl"; echo "$name: $(wc -l < "$out/$name.jsonl") proofs"; }
run main replay --bin emit
run main-det replay --bin emit --features det
run w10-head w10/head
run w10-head+arg w10/head --features arg
run w10-merged w10/merged
run w10-merged+arg w10/merged --features arg
run w10-merged+arg+det w10/merged --features arg,det
python3 -I "$here/compare_w10.py" "$out"
