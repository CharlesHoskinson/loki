#!/usr/bin/env python3
"""quint-q1 campaign runner. Writes quint/q1/results.json. Seed 20261024, Rust backend,
<= 4 simulator threads. Expected failures (mutations, the Quint capture repro and the
stock-defect counterexample) are recorded as such and checked to fail."""
import json, re, subprocess, sys, time
from pathlib import Path

Q = Path(__file__).resolve().parent
ROOT = Q.parent.parent
ART = ROOT / ".artifacts" / "q1"
ART.mkdir(parents=True, exist_ok=True)
SEED = "20261024"
MODEL = str(Q / "transcript_q1.qnt")
LEFT = str(Q / "lookup_leftover.qnt")
quick = "--quick" in sys.argv
only = next((a.split("=", 1)[1].split(",") for a in sys.argv if a.startswith("--only=")), None)
def want(section):
    return only is None or section in only
RES = Q / "results.json"
results = {"seed": SEED, "quint": "0.32.0", "runs": []}
if only is not None and RES.exists():
    old = json.loads(RES.read_text())
    results = old
    results["runs"] = [r for r in old["runs"] if r.get("section") not in only]
section = None

def run(name, args, expect_ok=True, timeout=7200):
    t0 = time.time()
    p = subprocess.run(["quint"] + args, capture_output=True, text=True, timeout=timeout)
    out = "\n".join(l for l in (p.stdout + p.stderr).splitlines() if not l.startswith("[DEBUG]"))
    (ART / f"{name}.log").write_text(out)
    ok = p.returncode == 0
    # An expected failure must be a genuine violation (or failing test), not a runtime/parse error.
    real_failure = ("[violation]" in out or "Tests failed" in out or "found an error" in out.lower())
    if expect_ok is False and not ok and not real_failure:
        ok = True  # count as unexpected below
    wit = {m.group(1): [int(m.group(2)), int(m.group(3))]
           for m in re.finditer(r"(\w+) was witnessed in (\d+) trace\(s\) out of (\d+)", out)}
    rec = {"name": name, "section": section, "exit": p.returncode, "expected_ok": expect_ok, "as_expected": ok == expect_ok,
           "seconds": round(time.time() - t0, 1), "witnesses": wit, "args": args}
    m = re.search(r"\[(ok|violation)\][^\n]*", out)
    rec["verdict"] = m.group(0) if m else out.strip().splitlines()[-1:]
    results["runs"].append(rec)
    tag = "INFO" if expect_ok is None else ("PASS" if rec["as_expected"] else "UNEXPECTED")
    print(f"{tag} {name}: exit={p.returncode} {rec['verdict']} {wit}", flush=True)
    return rec

def sim(model, inv, samples, steps, extra=()):
    return ["run", model, "--backend", "rust", "--n-threads", "4", "--seed", SEED,
            "--max-samples", str(samples), "--max-steps", str(steps), "--invariant", inv,
            "--verbosity", "1", *extra]

N = 500 if quick else 10000
if want("base"):
    section = "base"
    for f in ["transcript_q1.qnt", "transcript_q1_test.qnt", "lookup_leftover.qnt"]:
        run(f"typecheck_{f}", ["typecheck", str(Q / f)])
    run("scenario_tests", ["test", str(Q / "transcript_q1_test.qnt"), "--match", ".*Test$", "--max-samples", "1"])
    run("quint_capture_repro", ["test", str(Q / "quint_capture_repro.qnt"), "--match", ".*Test$"], expect_ok=False)

MAIN_INV = "safetyOutsideDefect and defectExact and plansEqual"
WIT = ["rComplete", "rDefect", "rManySets", "rPermChain", "rPhaseChallenge", "rTruncMultiopen",
       "rBatched", "rTrash", "rVerifierAhead"]
if want("main"):
    section = "main"
    run("main_safety", sim(MODEL, MAIN_INV, N, 700, ["--witnesses", *WIT]))
    run("alias_classified", sim(MODEL, "aliasClassified", N, 700, ["--init", "initAlias", "--witnesses", "rComplete"]))
    # The stock defect itself: `safety` must FAIL (mixed committed/plain multi-proof class).
    run("defect_counterexample", sim(MODEL, "safety", 2000, 700,
        ["--out-itf", str(ART / "defect_counterexample.itf.json")]), expect_ok=False)

MUTS = ["swapAdvice", "dropPhaseChallenge", "dropBeta", "swapProducts", "lookupMajor", "x3BeforeF",
        "x3BeforeFBoth", "x4AfterPi", "lokiTruncX2", "lokiUnsortedSets", "emitOnCompletion",
        "verifierInstanceOrder"]
if want("mutations"):
    section = "mutations"
    for m in MUTS:
        run(f"mutation_{m}", sim(MODEL, MAIN_INV, 2000, 700, ["--init", f"init_{m}"]), expect_ok=False)
# Attribution: which individual invariant catches each mutation (bounded at 300 samples).
INVS = ["challengesAgree", "readsAgree", "fsDependencies", "challengeSchedule", "completesCleanly",
        "plansEqual", "defectExact"]
if want("matrix"):
    section = "matrix"
    matrix = {}
    for m in MUTS:
        matrix[m] = {}
        for i in INVS:
            inv = i if i in ("defectExact", "plansEqual") else f"mixedBatch(st.cfg) or {i}"
            r = run(f"attr_{m}_{i}", sim(MODEL, inv, 300, 700, ["--init", f"init_{m}"]), expect_ok=None)
            matrix[m][i] = "caught" if r["exit"] != 0 else "not caught (300 samples)"
    results["attribution"] = matrix

if want("leftover"):
    section = "leftover"
    for rows in (4, 5, 6):
        mod = ["--main", f"lookup_leftover_r{rows}"]
        run(f"leftover_r{rows}_safety", sim(LEFT, "leftoverSafety and deterministic", 10000, 1,
            [*mod, "--witnesses", "rOrderMatters", "rTwoLeftoverKeys"]))
        run(f"leftover_r{rows}_control_popFront", sim(LEFT, "leftoverSafety and deterministic", 10000, 1,
            [*mod, "--init", "init_popFront"]))
        for m in ["skipLastKey", "fillFirstRows", "noFirstConsume", "detIgnored"]:
            run(f"leftover_r{rows}_mutation_{m}", sim(LEFT, "leftoverSafety and deterministic", 10000, 1,
                [*mod, "--init", f"init_{m}"]), expect_ok=False)

if want("apalache"):
    section = "apalache"
    # Exhaustive bounded check (all tables, inputs and HashMap orders) for 3 usable rows.
    APA_MAIN = "lookup_leftover_r3"
    for inv in ("leftoverSafety", "deterministic"):
        run(f"apalache_leftover_r3_{inv}", ["verify", LEFT, "--main", APA_MAIN, "--max-steps", "0",
            "--invariant", inv, "--verbosity", "1"], timeout=10800)
    run("apalache_leftover_r3_mutation_skipLastKey", ["verify", LEFT, "--main", APA_MAIN,
        "--init", "init_skipLastKey", "--max-steps", "0", "--invariant", "leftoverSafety", "--verbosity", "1"],
        expect_ok=False, timeout=10800)

bad = [r["name"] for r in results["runs"] if r["expected_ok"] is not None and not r["as_expected"]]
results["unexpected"] = bad
RES.write_text(json.dumps(results, indent=1) + "\n")
print("UNEXPECTED:", bad if bad else "none")
sys.exit(1 if bad else 0)
