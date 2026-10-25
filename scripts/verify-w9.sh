#!/usr/bin/env bash
# Full verification of loki-proofs main: CPU suite, W2 gate, GPU suites (under the GPU lock).
set -uo pipefail
cd /home/charles/repos/loki-wt/w9-harden
# Keep proof jobs sequential across agents as well as within this gate.
exec 9>/home/charles/.cache/doctorwatson/heavy.lock
flock 9
mkdir -p target/w9-validation/gate
failed=0
export CARGO_BUILD_JOBS=4 PATH=$HOME/.local/opt/cuda-13.4.3/bin:$PATH LD_LIBRARY_PATH=/usr/lib/wsl/lib:$HOME/.local/opt/cuda-13.4.3/lib64
step(){
    local label="$1"; shift
    local log="target/w9-validation/gate/${label// /-}.log"
    printf '\n### %s\n' "$label"
    "$@" > "$log" 2>&1
    local rc=$?
    tail -15 "$log"
    echo "rc=$rc log=$log"
    if (( rc != 0 )); then failed=1; fi
}

step "cpu default tests" cargo test --release --offline -- --test-threads=4
step "W2 gate" ./loki-harness/run.sh
step "cpu fixture" cargo run --release --offline --example cpu-bench -- 16 3 target/cpu-fixture
step "stock verifier (tests-stock)" cargo test --release --offline --manifest-path tests-stock/Cargo.toml
G="python3 scripts/w9-host-cargo.py"
step "gpu lib all features" $G test --release --offline --features cuda,cuda-ntt,loki-profile --lib -- --test-threads=1
step "gpu msm differential" $G test --release --offline --features cuda,cuda-ntt --lib gpu_differential -- --ignored --test-threads=1
step "gpu msm boundaries" $G test --release --offline --features cuda,cuda-ntt --lib gpu_large_boundaries -- --ignored --test-threads=1
step "gpu proof" $G test --release --offline --features cuda,cuda-ntt --lib gpu_proof -- --ignored --nocapture --test-threads=1
step "gpu proof stock-verified" cargo run --release --offline --manifest-path tests/gpu-compat/Cargo.toml -- target/gpu-msm-proof
step "ntt k17 stock-verified" $G run --release --offline --manifest-path tests/ntt-stock/Cargo.toml --features gpu -- 17 3

step "gpu concurrent proofs" $G test --release --offline --features cuda,cuda-ntt,loki-harden --lib gpu_concurrent_proofs -- --ignored --nocapture --test-threads=1
step "gpu budget harden lib" $G test --release --offline --features cuda,cuda-ntt,loki-harden,loki-cpu-budget --lib -- --test-threads=1
step "gpu budget concurrent proofs" $G test --release --offline --features cuda,cuda-ntt,loki-harden,loki-cpu-budget --lib gpu_concurrent_proofs -- --ignored --nocapture --test-threads=1
exit "$failed"
