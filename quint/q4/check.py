#!/usr/bin/env python3
"""quint-q4 campaign: typecheck, unit runs, >=10k-sample simulations with
fixed seeds, reachability witnesses, mutation kills and known-limit checks for
the W13 SRS cache model (srs_w13.qnt) and the P3 forward model (srs_p3.qnt).

Writes quint/q4/results.json and logs under .artifacts/q4/. Exit 1 on any
unexpected outcome. Simulation uses the Rust evaluator with 4 threads."""
import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SEED = "20261025"

W13_REG_WIT = ["wHit", "wPrefixHit", "wNminus1Hit", "wStaleRejected", "wEvict", "wTwoResident",
               "wBudgetReject", "wInvalidReject", "wQueryReject", "wReleaseFail", "wWorkers4",
               "wParPanic", "wDisabled", "wMixedOvercharge", "wCoefLagResident"]
W13_LEG_WIT = ["wHit", "wNminus1Hit", "wStaleRejected", "wEvict", "wTwoResident", "wInvalidReject",
               "wReleaseFail", "wParPanic", "wDisabled", "wCoefLagResident"]
P3_WIT = ["wHit", "wPublished", "wPrefixPublished", "wLagPublished", "wCapacity", "wLost", "wReject",
          "wForgedRejected", "wEvicted"]

# (file, module, step, steps, invariant, expect_violation, note)
CAMPAIGNS = [
    ("srs_w13.qnt", "w13_registry", "step", 24, "safety", False, "W13 registry, all faults"),
    ("srs_w13.qnt", "w13_registry", "stepMbt", 24, "safety", False, "W13 registry, MBT fault set"),
    ("srs_w13.qnt", "w13_legacy", "step", 24, "safety", False, "legacy two-slot, all faults"),
    ("srs_w13.qnt", "w13_registry", "step", 24, "chargeExactHomogeneous", False, "exact on homogeneous hosts"),
    ("srs_p3.qnt", "p3", "step", 30, "safety", False, "P3 forward contract"),
    # Known limits / findings (expected counterexamples).
    ("srs_w13.qnt", "w13_registry", "step", 24, "chargeExact", True, "F3: mixed-host overcharge"),
    ("srs_w13.qnt", "w13_legacy", "step", 24, "releaseQuarantined", True, "F2: legacy drops failed-release handle"),
    ("srs_w13.qnt", "w13_registry", "step", 24, "noUntrackedLive", True, "F4: sync-error allocation untracked"),
]
W13_MUTANTS = [
    ("mut_addr_only", "hitExact"), ("mut_addr_only_legacy", "hitExact"),
    ("mut_first_block", "hitExact"), ("mut_first_block_legacy", "hitExact"),
    ("mut_no_len", "hitExact"), ("mut_ordinal0_sm", "chargeConservative"),
    ("mut_charge_n", "chargeConservative"), ("mut_no_stale_evict", "onePerAddress"),
    ("mut_budget_used", "budgetRespected"), ("mut_release_drop", "releaseQuarantined"),
    ("mut_short_join", "workersDrained"), ("mut_floor_chunk", "workersBounded"),
    ("mut_validate_prefix", "residentValid"), ("mut_skip_failed", "noNativeAfterFail"),
    ("mut_evict_no_release", "noUntrackedWhileHealthy"), ("mut_release_keep", "noDangling"),
    ("mut_legacy_three", "legacyTwoSlots"), ("mut_refresh_snapshot", "snapshotMatchesUpload"),
]
BENIGN = [("mut_legacy_len", "safety")]
P3_MUTANTS = [
    ("p3_digest_only", "exactContent"), ("p3_evict_leased", "noEvictLeased"),
    ("p3_ignore_gen", "noCrossGen"), ("p3_ignore_basis", "basisMatch"),
    ("p3_trust_release", "refsExact"), ("p3_release_inflight", "pendingResident"),
    ("p3_leak_on_evict", "bytesExact"), ("p3_publish_stale", "noCrossGen"),
    ("p3_no_validate", "residentValid"),
]


def run(cmd, log):
    start = time.monotonic()
    p = subprocess.run(cmd, cwd=HERE, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    log.write_text(" ".join(cmd) + "\n" + p.stdout)
    return p.returncode, p.stdout, round(time.monotonic() - start, 3)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--samples", type=int, default=10000)
    args = ap.parse_args()
    out = ROOT / ".artifacts/q4"
    out.mkdir(parents=True, exist_ok=True)
    results, failures = [], []

    for f in ["srs_w13.qnt", "srs_w13_mutants.qnt", "srs_p3.qnt"]:
        rc, _, s = run(["quint", "typecheck", f], out / f"typecheck-{f}.log")
        results.append(dict(kind="typecheck", file=f, exit=rc, seconds=s))
        if rc:
            failures.append(f"typecheck {f}")
    rc, txt, s = run(["quint", "test", "srs_w13_test.qnt", "--match", ".*Test$"], out / "unit.log")
    results.append(dict(kind="unit", file="srs_w13_test.qnt", exit=rc, seconds=s,
                        passing=re.findall(r"(\d+) passing", txt)))
    if rc:
        failures.append("unit tests")

    def sim(file, main, step, steps, inv, witnesses=()):
        cmd = ["quint", "run", file, "--main", main, "--step", step, "--backend", "rust",
               "--n-threads", "4", "--seed", SEED, "--max-samples", str(args.samples),
               "--max-steps", str(steps), "--invariant", inv, "--verbosity", "1"]
        if witnesses:
            cmd += ["--witnesses", *witnesses]
        log = out / f"sim-{main}-{step}-{inv}.log"
        rc, txt, s = run(cmd, log)
        counts = {k: int(v) for k, v in re.findall(r"(\w+) was witnessed in (\d+) trace", txt)}
        return rc, counts, s, cmd, log

    for file, main, step, steps, inv, expect, note in CAMPAIGNS:
        wit = []
        if inv == "safety":
            wit = P3_WIT if file == "srs_p3.qnt" else (W13_REG_WIT if main == "w13_registry" else W13_LEG_WIT)
        rc, counts, s, cmd, log = sim(file, main, step, steps, inv, wit)
        violated = rc != 0
        ok = violated == expect and all(counts.get(w, 0) > 0 for w in wit)
        results.append(dict(kind="simulation", note=note, command=cmd, exit=rc, seconds=s,
                            expect_violation=expect, violated=violated, witnesses=counts,
                            samples=args.samples, seed=SEED, log=str(log.relative_to(ROOT)), ok=ok))
        print(f"{'ok ' if ok else 'BAD'} {note}: violated={violated} {s}s {counts}", flush=True)
        if not ok:
            failures.append(note)

    for file, mutants, steps, expect in [("srs_w13_mutants.qnt", W13_MUTANTS, 24, True),
                                         ("srs_w13_mutants.qnt", BENIGN, 24, False),
                                         ("srs_p3.qnt", P3_MUTANTS, 30, True)]:
        for main, inv in mutants:
            rc, _, s, cmd, log = sim(file, main, "step", steps, inv)
            ok = (rc != 0) == expect
            results.append(dict(kind="mutant", module=main, invariant=inv, expect_violation=expect,
                                violated=rc != 0, seconds=s, command=cmd,
                                log=str(log.relative_to(ROOT)), ok=ok))
            print(f"{'ok ' if ok else 'BAD'} mutant {main} vs {inv}: violated={rc != 0} {s}s", flush=True)
            if not ok:
                failures.append(f"mutant {main}")

    (HERE / "results.json").write_text(json.dumps(dict(seed=SEED, samples=args.samples,
                                                       results=results, failures=failures), indent=2) + "\n")
    if failures:
        sys.exit("FAILED: " + ", ".join(failures))
    print("all q4 campaigns behaved as expected")


if __name__ == "__main__":
    main()
