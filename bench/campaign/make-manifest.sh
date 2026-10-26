#!/usr/bin/env bash
# Emit a launcher manifest from the latest build receipts.
#   campaign/make-manifest.sh COHORT HEAVY(dw|own) GPU_LOCK(block|gpu-arms) IDLE(record|enforce|off) BLOCK_LINE...
# Each BLOCK_LINE is "B000=N4: S0 L0 G0".
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cohort=$1 heavy=$2 gpulock=$3 idle=$4; shift 4
b="$here/campaign/builds/latest"
get() { grep "^$2=" "$b/$1.binary.txt" | cut -d= -f2-; }
srs=/home/charles/repos/Moriarty-loki/target/sherlock-srs/bls_midnight_2p20
cat <<M
schema = loki-bench-launch/1
cohort = $cohort
collection = false
out_root = $here/campaign/runs
srs = $srs
srs_sha256 = 1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1
params_dir = $HOME/.cache/midnight/zk-params
stock_verifier = $(get verifier binary)
stock_verifier_sha256 = $(get verifier binary_sha256)
timeout_s = 5400
idle_gate = $idle
idle_window_s = 60
idle_max_wait_s = 900
vk_policy = ${VK_POLICY:-record}
heavy = $heavy
gpu_lock = $gpulock
nvml = on
pass_env = PATH,LD_LIBRARY_PATH
staged = $here/deps/STAGED
source.bench_commit = $(git -C "$here" rev-parse HEAD)
source.build_receipts = $(readlink -f "$b")
source.moriarty_rev = $(grep ^moriarty_rev= "$here/deps/STAGED" | cut -d= -f2)
source.loki_rev = $(grep ^loki_rev= "$here/deps/STAGED" | cut -d= -f2)
source.loki_src_patch_sha256 = $(grep ^loki_src_patch_sha256= "$here/deps/STAGED" | cut -d= -f2)
source.rustc = $(get stock rustc)
budget.N4.mask = 0-3
budget.N4.workers = 4
budget.N20.mask = 0-19
budget.N20.workers = 20
arm.S0.binary = $(get stock binary)
arm.S0.sha256 = $(get stock binary_sha256)
arm.S0.gpu = false
arm.S0.features = circuits
arm.S0.workspace = stock
arm.L0.binary = $(get loki-cpu binary)
arm.L0.sha256 = $(get loki-cpu binary_sha256)
arm.L0.gpu = false
arm.L0.features = circuits
arm.L0.workspace = loki
arm.G0.binary = $(get loki-gpu binary)
arm.G0.sha256 = $(get loki-gpu binary_sha256)
arm.G0.gpu = true
arm.G0.features = circuits,g0
arm.G0.workspace = loki
arm.G0.env.LOKI_GPU_MSM_MIN = 16384
arm.G0.env.LOKI_GPU_SRS_BUDGET_BYTES = 4294967296
M
for line in "$@"; do echo "block.${line%%=*} = ${line#*=}"; done
