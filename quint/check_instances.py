#!/usr/bin/env python3
"""Reproduce the stock mixed-instance order bug and its exact configuration boundary."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=10000)
    parser.add_argument("--update-counterexample", action="store_true",
                        help="Replace the checked-in symbolic counterexample with this run")
    args = parser.parse_args()
    output = ROOT / ".artifacts/w8"
    output.mkdir(parents=True, exist_ok=True)
    base = ["quint", "run", "quint/transcript.qnt", "--main", "transcript_instances",
            "--max-samples", str(args.samples), "--max-steps", "1", "--seed", "20261024",
            "--n-threads", "4", "--verbosity", "1"]
    commands = [
        ("typecheck", ["quint", "typecheck", "quint/transcript.qnt"], 0),
        ("tests", ["quint", "test", "quint/transcript_instances_test.qnt",
                   "--match", ".*Test$", "--max-samples", "1"], 0),
        ("classification", base + ["--invariant", "mismatchExactlyMixedBatch", "--witnesses",
                                    "allPlain", "allCommitted", "singleMixed", "multiMixed"], 0),
        ("counterexample", base + ["--init", "mixedCounterexample",
                                  "--invariant", "proverEqualsVerifier", "--out-itf",
                                  ".artifacts/w8/instance-order-counterexample.itf.json"], 1),
    ]
    records = []
    for name, command, expected in commands:
        start = time.monotonic()
        run = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT)
        log = output / f"instance-order-{name}.log"
        log.write_text(run.stdout)
        counts = {name: int(count) for name, count in
                  re.findall(r"(\w+) was witnessed in (\d+) trace", run.stdout)}
        if run.returncode != expected:
            raise SystemExit(f"Unexpected exit status: {log}")
        if name == "classification":
            if len(counts) != 4 or any(count == 0 for count in counts.values()):
                raise SystemExit(f"Missing configuration coverage: {counts}")
        if expected == 1:
            if "Invariant violated" not in run.stdout:
                raise SystemExit(f"Expected invariant violation: {log}")
        records.append(dict(phase=name, command=command, exit=run.returncode,
                            expectedExit=expected, seconds=round(time.monotonic() - start, 3),
                            witnesses=counts))
        print(f"instance-order {name}: expected exit {expected}, observed {run.returncode}", flush=True)
    trace = json.loads((output / "instance-order-counterexample.itf.json").read_text())
    audit = trace["states"][0]["audit"]
    if audit["prover"] == audit["verifier"]:
        raise SystemExit("Counterexample contains matching orders")
    if args.update_counterexample:
        (ROOT / "quint/instance-order-counterexample.itf.json").write_text(json.dumps(trace, indent=2) + "\n")
    (output / "instance-order-results.json").write_text(json.dumps(records, indent=2) + "\n")


if __name__ == "__main__":
    main()
