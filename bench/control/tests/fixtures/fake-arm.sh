#!/bin/sh
# Fake arm for launcher contract tests. Behaviour is selected by FAKE_MODE,
# which the test writes into the file next to this script (the launcher clears
# the environment, so it cannot be passed as a variable).
here=$(dirname "$0")
mode=$(cat "$here/mode" 2>/dev/null || echo ok)
case "$1" in
  identity)
    g0=false; [ "$(cat "$here/g0" 2>/dev/null)" = true ] && g0=true
    echo "{\"schema\":\"fake\",\"features\":{\"g0\":$g0}}"
    exit 0 ;;
  chain) ;;
  *) exit 2 ;;
esac
out=""
while [ $# -gt 0 ]; do [ "$1" = "--out" ] && out=$2; shift; done
mkdir -p "$out/sample-0-cold/children"
env | sort > "$here/env.$$"
grep Cpus_allowed_list /proc/self/status > "$here/affinity.$$"
echo "$$" > "$here/pid"
case "$mode" in
  fail) echo '{"schema":"loki-bench-error/1","error":"injected"}'; exit 1 ;;
  hang) sleep 1000 & echo $! > "$here/child"; wait ;;
esac
echo vk-bytes > "$out/sample-0-cold/children/a.vk"
echo '{"event":"sample","cache":"cold"}'
echo '{"event":"gate","before":"warm"}'
read go
[ "$go" = go ] || exit 3
echo '{"event":"sample","cache":"warm"}'
echo '{"event":"exit","record":{}}'
