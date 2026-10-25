#!/usr/bin/env bash
# Process receipt for the narrowed SIGKILL guarantee: SIGKILL the controller
# while the REAL bench-driver is waiting on its verifier worker; both the
# driver (PDEATHSIG from the controller) and the worker (PDEATHSIG from the
# driver) must die. No proving happens: the fake worker never reports ready.
# Private HOME, so host locks are never touched.
#   campaign/sigkill-receipt.sh DRIVER_BINARY
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
driver=$1
t=$(mktemp -d "$here/campaign/sigkill.XXXXXX")
mkdir -p "$t/home" "$t/params" "$t/out"
cat > "$t/fake-verifier.sh" <<V
#!/bin/sh
echo \$\$ > "$t/verifier.pid"
echo \$PPID > "$t/verifier.ppid"
exec sleep 1000
V
chmod +x "$t/fake-verifier.sh"
echo srs > "$t/srs"
h() { sha256sum "$1" | cut -d' ' -f1; }
cat > "$t/m.conf" <<M
schema = loki-bench-launch/1
cohort = sigkill
collection = false
out_root = $t/out
srs = $t/srs
srs_sha256 = $(h "$t/srs")
params_dir = $t/params
stock_verifier = $t/fake-verifier.sh
stock_verifier_sha256 = $(h "$t/fake-verifier.sh")
timeout_s = 60
idle_gate = off
idle_window_s = 1
idle_max_wait_s = 1
vk_policy = record
heavy = own
gpu_lock = block
nvml = off
pass_env = PATH
budget.N1.mask = 0
budget.N1.workers = 1
arm.L0.binary = $driver
arm.L0.sha256 = $(h "$driver")
arm.L0.gpu = false
arm.L0.features = circuits
arm.L0.workspace = loki
block.B000 = N1: L0
M
HOME="$t/home" "$here/target-control/release/bench-launch" --manifest "$t/m.conf" --block B000 2>"$t/launcher.err" &
launcher=$!
for _ in $(seq 100); do [ -s "$t/verifier.pid" ] && break; sleep 0.1; done
vpid=$(cat "$t/verifier.pid"); dpid=$(cat "$t/verifier.ppid")
echo "launcher=$launcher driver=$dpid driver_cmd=$(tr '\0' ' ' </proc/$dpid/cmdline | cut -c1-80) verifier=$vpid"
kill -9 "$launcher"; wait "$launcher" 2>/dev/null || true
alive() { [ -e /proc/$1 ] && ! grep -q ') Z' /proc/$1/stat 2>/dev/null; }
for _ in $(seq 50); do alive "$dpid" || alive "$vpid" || break; sleep 0.1; done
rc=0
alive "$dpid" && { echo "FAIL driver $dpid survived"; kill -9 "$dpid"; rc=1; }
alive "$vpid" && { echo "FAIL verifier $vpid survived"; kill -9 "$vpid"; rc=1; }
[ $rc = 0 ] && echo "PASS driver and verifier died after controller SIGKILL"
rm -rf "$t"
exit $rc
