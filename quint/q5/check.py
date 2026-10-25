"""q5 model checks: invariants (must hold), witnesses and mutants (must be violated).
Usage: python3 -I check.py [--samples N]  -> writes results.json"""
import json, subprocess, sys, time, re, pathlib
here = pathlib.Path(__file__).resolve().parent
N = int(sys.argv[sys.argv.index("--samples") + 1]) if "--samples" in sys.argv else 10000
SEED = "0x51515"
def run(spec, main, inv, samples, steps, expect):
    cmd = ["quint", "run", spec, "--main", main, "--invariant", inv, "--max-samples", str(samples),
           "--max-steps", str(steps), "--seed", SEED, "--backend", "rust"]
    t = time.time()
    p = subprocess.run(cmd, cwd=here, capture_output=True, text=True, timeout=7200)
    out = p.stdout + p.stderr
    status = "violated" if "[violation]" in out else "ok" if "[ok]" in out else "error"
    passed = status == expect
    m = re.search(r"\((\d+)ms", out)
    r = {"spec": spec, "main": main, "invariant": inv, "samples": samples, "steps": steps, "seed": SEED,
         "expect": expect, "status": status, "pass": passed, "seconds": round(time.time() - t, 1)}
    print(("PASS " if passed else "FAIL ") + json.dumps(r), flush=True)
    if status == "error": print(out[-2000:])
    return r
res = []
L = ("lookup.qnt", 5)
res.append(run(*L[:1], "lookup_ok", "Safety", N, L[1], "ok"))
for w in ["WitnessNoError", "WitnessNoZeroDen", "WitnessOrderInsensitive", "WitnessNoMultiSet", "WitnessNoPermOne",
          "WitnessNoInvalidPerm", "WitnessNoRationalOne", "WitnessNoRationalZeroDen", "WitnessNotDone", "WitnessNoDet",
          "WitnessNoMultiChunk"]:
    res.append(run(L[0], "lookup_ok", w, 2000, L[1], "violated"))
MUTS = {"mut1": ["LookupEqualsStock"], "mut2": ["BlindingRowsUntouched", "SameDraws"],
        "mut3": ["HelpersEqualSequential", "LookupEqualsStock"], "mut4": ["HelpersEqualSequential", "LookupEqualsStock"],
        "mut5": ["RationalEqualsStock"], "mut6": ["LookupConditions"], "mut7": ["LookupTelescopes", "PermEqualsStock"],
        "mut8": ["BlindingRowsUntouched", "LookupTelescopes"], "mut9": ["LookupConditions"], "mut10": ["ErrorAgrees"], "mut11": ["PermTelescopes", "PermEqualsStock"]}
for m, invs in MUTS.items():
    for inv in invs:
        res.append(run(L[0], m, inv, 2000, L[1], "violated"))
if (here / "argcommit.qnt").exists():
    res.append(run("argcommit.qnt", "argcommit_ok", "Safety", N, 6, "ok"))
    for w in ["WitnessNoCoeffPath", "WitnessNoFallback", "WitnessNoDownsize", "WitnessNoBadSrsDivergence"]:
        res.append(run("argcommit.qnt", "argcommit_ok", w, 2000, 6, "violated"))
    for m, inv in [("acmut1", "W10EqualsStockOnValidSrs"), ("acmut2", "W10EqualsStockOnValidSrs"), ("acmut3", "TranscriptEqual")]:
        res.append(run("argcommit.qnt", m, inv, 2000, 6, "violated"))
def verify(spec, main, inv, steps, expect):
    t = time.time()
    p = subprocess.run(["quint", "verify", spec, "--main", main, "--invariant", inv, "--max-steps", str(steps)],
                       cwd=here, capture_output=True, text=True, timeout=7200)
    out = p.stdout + p.stderr
    status = "violated" if "[violation]" in out else "ok" if "[ok]" in out else "error"
    r = {"spec": spec, "main": main, "invariant": inv, "tool": "apalache", "steps": steps, "expect": expect,
         "status": status, "pass": status == expect, "seconds": round(time.time() - t, 1)}
    print(("PASS " if r["pass"] else "FAIL ") + json.dumps(r), flush=True)
    return r
if "--no-verify" not in sys.argv:
    res.append(verify("argcommit_bmc.qnt", "bmc_ok", "Equal", 2, "ok"))
    res.append(verify("argcommit_bmc.qnt", "bmc_mut1", "Equal", 2, "violated"))
ok = all(r["pass"] for r in res)
json.dump({"quint": subprocess.run(["quint", "--version"], capture_output=True, text=True).stdout.strip(),
           "all_pass": ok, "runs": res}, open(here / "results.json", "w"), indent=1)
print("ALL PASS" if ok else "SOME FAILED"); sys.exit(0 if ok else 1)
