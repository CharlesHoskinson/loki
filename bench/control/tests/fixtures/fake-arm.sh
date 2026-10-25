#!/bin/sh
# Fake arm for launcher contract tests. Behaviour is selected by FAKE_MODE,
# which the test writes into the file next to this script (the launcher clears
# the environment, so it cannot be passed as a variable).
here=$(dirname "$0")
mode=$(cat "$here/mode" 2>/dev/null || echo ok)
case "$1" in
  level)
    # Nested escape chain: each level leaves the session and spawns the next.
    echo $$ >> "$here/deep"
    [ "$2" -gt 0 ] && setsid "$0" level $(($2 - 1)) &
    exec sleep 1000 ;;
  identity)
    g0=false; [ "$(cat "$here/g0" 2>/dev/null)" = true ] && g0=true
    echo "{\"schema\":\"fake\",\"features\":{\"g0\":$g0}}"
    exit 0 ;;
  chain) ;;
  *) exit 2 ;;
esac
out=""
block=""
while [ $# -gt 0 ]; do
  [ "$1" = "--out" ] && out=$2
  [ "$1" = "--block" ] && block=$2
  shift
done
# The SUITE seeds a real driver reports (mode wrongseed reports another block's).
seeds() {
  b=$block; [ "$mode" = wrongseed ] && b="other"
  s=""
  for ps in proof:A3 witness:DEC.comm_rand proof:DEC; do
    p=${ps%%:*}; st=${ps#*:}
    h=$(printf '%s' "loki-bench-v1|CHAIN|$b|$p|$1|$st" | sha256sum | cut -d' ' -f1)
    s="$s${s:+,}\"$p|$st\":\"$h\""
  done
  printf '{%s}' "$s"
}
mkdir -p "$out/sample-0-cold/children"
env | sort > "$here/env.$$"
grep Cpus_allowed_list /proc/self/status > "$here/affinity.$$"
echo "$$" > "$here/pid"
case "$mode" in
  fail) echo '{"schema":"loki-bench-error/1","error":"injected"}'; exit 1 ;;
  hang) sleep 1000 & echo $! > "$here/child"; wait ;;
  escape) setsid sleep 1000 & echo $! > "$here/child" ;;
  deep) setsid "$0" level 8 & ;;
esac
cat "$here/vk" 2>/dev/null > "$out/sample-0-cold/children/a.vk" || echo vk-bytes > "$out/sample-0-cold/children/a.vk"
echo "{\"event\":\"sample\",\"cache\":\"cold\",\"seeds\":$(seeds cold)}"
echo '{"event":"gate","before":"warm"}'
read go
[ "$go" = go ] || exit 3
echo "{\"event\":\"sample\",\"cache\":\"warm\",\"seeds\":$(seeds warm)}"
echo '{"event":"exit","record":{}}'
