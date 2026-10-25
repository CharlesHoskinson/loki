#!/usr/bin/env bash
# Build once per feature mode, then measure sequential verified synthetic proofs.
set -euo pipefail
cd /home/charles/repos/loki-wt/w9-harden
export CARGO_BUILD_JOBS=4
# Acquire before publishing the benchmark marker so a running gate cannot
# wait on a benchmark that is itself waiting for the heavy-job lock.
exec 9>/home/charles/.cache/doctorwatson/heavy.lock
flock 9
mkdir -p target/w9-validation
touch target/w9-validation/cpu-benchmark.running
cleanup() {
    python3 -c 'from pathlib import Path; Path("target/w9-validation/cpu-benchmark.running").unlink(missing_ok=True)'
}
trap cleanup EXIT
cargo build --release --offline --example cpu-bench
cp target/release/examples/cpu-bench target/w9-validation/cpu-bench-default
cargo build --release --offline --features loki-cpu-budget --example cpu-bench
cp target/release/examples/cpu-bench target/w9-validation/cpu-bench-budget
sha256sum target/w9-validation/cpu-bench-{default,budget}
# Affinity remains at all 20 CPUs for both configurations. RAYON_NUM_THREADS=4
# limits Rayon only; stock BLST retains its own independent pool by design.
python3 - <<'PY'
import os
from pathlib import Path
import statistics
import subprocess

out = Path('target/w9-validation')
print('affinity:', sorted(os.sched_getaffinity(0)), flush=True)
for threads in [20, 4]:
    sizes = [16, 17] + ([20] if threads == 4 and os.environ.get('LOKI_BENCH_K20') else [])
    for k in sizes:
        modes = ['default', 'budget'] if k == 16 else ['budget', 'default']
        for mode in modes:
            env = dict(os.environ, RAYON_NUM_THREADS=str(threads))
            log = out / f'cpu-{mode}-t{threads}-k{k}.log'
            result = subprocess.run([str(out / f'cpu-bench-{mode}'), str(k), '3'],
                                    env=env, text=True, capture_output=True)
            log.write_text(result.stdout + result.stderr)
            result.check_returncode()
            times = [float(line.split('prove_ms=')[1].split()[0])
                     for line in result.stdout.splitlines() if 'prove_ms=' in line]
            assert len(times) == 3
            print(f'{mode} t={threads} k={k} ms={times} median={statistics.median(times):.3f}', flush=True)
PY
