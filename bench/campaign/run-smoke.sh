#!/usr/bin/env bash
# Serial smoke blocks through the launcher (heavy then gpu; queues politely).
set -uo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
manifest=$1; shift
export PATH="$HOME/.local/opt/cuda-13.4.3/bin:/usr/local/bin:/usr/bin:/bin" LD_LIBRARY_PATH=/usr/lib/wsl/lib
for b in "$@"; do
  echo "== block $b start $(date -u +%FT%TZ)"
  "$here/target-control/release/bench-launch" --manifest "$manifest" --block "$b"
  echo "== block $b exit $? $(date -u +%FT%TZ)"
done
echo "== all done"
