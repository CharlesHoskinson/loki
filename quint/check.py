#!/usr/bin/env python3
"""Run the four bounded models; retain commands, seeds, logs and witness counts."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent
MODELS = {
    "transcript": (180, ["complete", "batched", "thetaReached", "multiopenReached", "phased"]),
    "gpu_dispatch": (120, ["completed", "fellBack", "cacheHit", "concurrent", "staleRejected"]),
    "pk_cache": (120, ["used", "filled", "concurrentWaiters", "differentKeys", "sharedUse"]),
    "scheduler": (200, ["allDone", "overlap", "checkpointed", "resumed", "completed"]),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=10000)
    parser.add_argument("--model", choices=list(MODELS), action="append")
    args = parser.parse_args()
    output = ROOT / ".artifacts/w8"
    output.mkdir(parents=True, exist_ok=True)
    results = []
    for model in args.model or MODELS:
        steps, witnesses = MODELS[model]
        commands = [
            ["quint", "typecheck", f"quint/{model}.qnt"],
            ["quint", "test", f"quint/{model}_test.qnt", "--match", ".*Test$", "--max-samples", "1"],
            ["quint", "run", f"quint/{model}.qnt", "--backend", "rust",
             "--n-threads", "4", "--seed", "20261024", "--max-samples", str(args.samples),
             "--max-steps", str(steps), "--invariant", "safety", "--witnesses", *witnesses,
             "--verbosity", "1"],
        ]
        for phase, command in zip(["typecheck", "test", "simulation"], commands):
            print(f"{model}: {phase}", flush=True)
            start = time.monotonic()
            run = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT)
            log = output / f"{model}-{phase}.log"
            log.write_text(run.stdout)
            counts = {name: int(count) for name, count in
                      re.findall(r"(\w+) was witnessed in (\d+) trace", run.stdout)}
            record = dict(model=model, phase=phase, command=command, exit=run.returncode,
                          seconds=round(time.monotonic() - start, 3), witnesses=counts,
                          log=str(log.relative_to(ROOT)))
            results.append(record)
            (output / f"results-{model}.json").write_text(json.dumps(results, indent=2) + "\n")
            if run.returncode:
                raise SystemExit(f"FAILED: {log}")
            if phase == "simulation" and any(counts.get(w, 0) == 0 for w in witnesses):
                raise SystemExit(f"Missing reachable witness: {log}; {counts}")
            print(f"  passed in {record['seconds']}s {counts}", flush=True)


if __name__ == "__main__":
    main()
