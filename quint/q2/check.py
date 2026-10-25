#!/usr/bin/env python3
"""quint-q2 campaign: invariants, mutation witnesses, fix witnesses, Apalache.

Every `quint run` uses the Rust backend, 10,000 samples and a fixed seed.
Mutants are produced by flipping one `pure val M_* = false` switch in a copy
of the model under .artifacts/ (the committed models keep every switch off).
Results go to results.json.  Usage: python3 check.py [--no-verify]
"""
import json, os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ART = os.path.join(HERE, ".artifacts")
SEED = "20261025"
SAMPLES = "10000"
STEPS = "20"

D, DS = ("dispatch.qnt", "q2_dispatch", "step"), ("dispatch.qnt", "q2_dispatch_seq", "seqStep")
L = ("legacy.qnt", "q2_legacy", "step")

# (model, invariant, expected) on the faithful model. "hold" or "violate".
FAITHFUL = [
    (D, "resultEqualsOracle", "hold"),
    (D, "noPublishAfterError", "hold"),
    (D, "poisonIsSticky", "hold"),
    (D, "msmCountersConsistent", "hold"),
    (D, "poisonCounterExact", "hold"),
    (D, "transformPerCallAccounting", "hold"),
    (D, "emptyMsmIsIdentityOnCpu", "hold"),
    (D, "invalidRejectedBeforeBackend", "hold"),
    (D, "backendOnlyWithCapabilityAndAbi", "hold"),
    (D, "noCancelledRetry", "violate"),                 # B1
    (D, "transformFallbackRecomputedOnCpu", "violate"), # B2
    (D, "noAccelPublishAfterDisable", "violate"),       # B3
    (D, "noAccelEntryAfterDisable", "violate"),         # B4
    (DS, "noAccelPublishAfterDisable", "hold"),
    (DS, "noAccelEntryAfterDisable", "hold"),
    (DS, "noCancelledRetry", "violate"),
    (DS, "transformFallbackRecomputedOnCpu", "violate"),
    (L, "legacyResultEqualsOracle", "hold"),
    (L, "noGpuMsmWhileSticky", "hold"),
    (L, "stickyIsPermanent", "hold"),
    (L, "defaultBuildNeverTouchesAccelerator", "hold"),
    (L, "acceleratorsExclusive", "hold"),
    (L, "noDeviceUseAfterFault", "violate"),            # L1
    (L, "stickyOnlyForDeviceFaults", "violate"),        # L2
    (L, "legacyFailuresVisibleInCounters", "violate"),  # L3
    (L, "envInjectionInert", "violate"),                # L4
    (L, "nttNeverRetried", "violate"),                  # witness: NTT retry
]

# (model, switch, invariant, expected): mutants must be caught; fixes must hold.
SWITCHED = [
    (DS, "M_NO_EMPTY_GUARD", "emptyMsmIsIdentityOnCpu", "violate"),
    (DS, "M_NO_DISABLED_CHECK", "noAccelEntryAfterDisable", "violate"),
    (DS, "M_NO_ABI_CHECK", "backendOnlyWithCapabilityAndAbi", "violate"),
    (DS, "M_PUBLISH_WRONG_LEN", "resultEqualsOracle", "violate"),
    (DS, "M_PUBLISH_WRONG_LEN", "noPublishAfterError", "violate"),
    (DS, "M_NO_POISON_WRONG_LEN", "poisonIsSticky", "violate"),
    (DS, "M_NO_CPU_BUMP_ON_FALLBACK", "msmCountersConsistent", "violate"),
    (DS, "M_NO_VALIDATE", "invalidRejectedBeforeBackend", "violate"),
    (D, "M_POISON_COUNT_EVERY", "poisonCounterExact", "violate"),
    (DS, "M_NO_TR_CPU_BUMP", "transformPerCallAccounting", "violate"),
    (D, "M_FIX_CANCEL", "noCancelledRetry", "hold"),
    (D, "M_FIX_RECHECK", "noAccelPublishAfterDisable", "hold"),
    (D, "M_FIX_RECHECK", "noAccelEntryAfterDisable", "hold"),
    (D, "M_FIX_COSET_ACCOUNTING", "transformFallbackRecomputedOnCpu", "hold"),
    (L, "M_NO_COMPILE_GUARD", "acceleratorsExclusive", "violate"),
    (L, "M_DEFAULT_LEAK", "defaultBuildNeverTouchesAccelerator", "violate"),
    (L, "M_NO_STICKY_CHECK", "noGpuMsmWhileSticky", "violate"),
    (L, "M_CLEAR_ON_NTT_SUCCESS", "stickyIsPermanent", "violate"),
    (L, "M_GPU_WRONG_ON_DECODE", "legacyResultEqualsOracle", "violate"),
    (L, "M_DOUBLE_NORM", "legacyResultEqualsOracle", "violate"),
    (L, "M_SHARED_POISON", "noDeviceUseAfterFault", "hold"),
    (L, "M_STICKY_DEVICE_ONLY", "stickyOnlyForDeviceFaults", "hold"),
    (L, "M_COUNT_LEGACY", "legacyFailuresVisibleInCounters", "hold"),
    (L, "M_NO_ENV_INJECTION", "envInjectionInert", "hold"),
]

# Bounded symbolic checks (Apalache, --max-steps=6).
VERIFY = [
    (DS, "resultEqualsOracle", "hold"), (DS, "noPublishAfterError", "hold"),
    (DS, "poisonIsSticky", "hold"), (DS, "msmCountersConsistent", "hold"),
    (DS, "poisonCounterExact", "hold"), (DS, "emptyMsmIsIdentityOnCpu", "hold"),
    (DS, "invalidRejectedBeforeBackend", "hold"),
    (DS, "backendOnlyWithCapabilityAndAbi", "hold"),
    (DS, "noAccelEntryAfterDisable", "hold"), (DS, "noAccelPublishAfterDisable", "hold"),
    (DS, "noCancelledRetry", "violate"), (DS, "transformFallbackRecomputedOnCpu", "violate"),
    (D, "noAccelPublishAfterDisable", "violate"), (D, "resultEqualsOracle", "hold"),
    (L, "legacyResultEqualsOracle", "hold"), (L, "noGpuMsmWhileSticky", "hold"),
    (L, "stickyIsPermanent", "hold"), (L, "defaultBuildNeverTouchesAccelerator", "hold"),
    (L, "acceleratorsExclusive", "hold"), (L, "noDeviceUseAfterFault", "violate"),
]


def run(path, module, step, inv):
    cmd = ["quint", "run", path, "--main", module, "--step", step, "--backend=rust",
           f"--max-samples={SAMPLES}", f"--max-steps={STEPS}", f"--seed={SEED}",
           f"--invariant={inv}"]
    t = time.time()
    p = subprocess.run(cmd, capture_output=True, text=True)
    out = p.stdout + p.stderr
    if "[ok] No violation found" in out:
        verdict = "hold"
    elif "[violation]" in out:
        verdict = "violate"
    else:
        verdict = "error: " + out[-400:]
    return verdict, round(time.time() - t, 1)


def mutant(model, switch):
    os.makedirs(ART, exist_ok=True)
    d = os.path.join(ART, switch)
    os.makedirs(d, exist_ok=True)
    for f in ("dispatch.qnt", "legacy.qnt"):
        shutil.copy(os.path.join(HERE, f), d)
    path = os.path.join(d, model[0])
    s = open(path).read()
    new, n = re.subn(rf"pure val {switch} = false", f"pure val {switch} = true", s)
    assert n == 1, f"switch {switch} not found exactly once in {model[0]}"
    open(path, "w").write(new)
    return path


def main():
    results = {"seed": SEED, "samples": int(SAMPLES), "max_steps": int(STEPS),
               "faithful": [], "switched": [], "verify": []}
    ok = True
    for m, inv, exp in FAITHFUL:
        v, secs = run(os.path.join(HERE, m[0]), m[1], m[2], inv)
        good = v == exp
        ok &= good
        results["faithful"].append({"module": m[1], "invariant": inv, "expected": exp,
                                    "got": v, "secs": secs})
        print(f"{'PASS' if good else 'FAIL'} faithful {m[1]:16} {inv:34} expect {exp:7} got {v}")
    for m, sw, inv, exp in SWITCHED:
        v, secs = run(mutant(m, sw), m[1], m[2], inv)
        good = v == exp
        ok &= good
        results["switched"].append({"module": m[1], "switch": sw, "invariant": inv,
                                    "expected": exp, "got": v, "secs": secs})
        print(f"{'PASS' if good else 'FAIL'} {sw:24} {inv:34} expect {exp:7} got {v}")
    if "--no-verify" not in sys.argv:
        for m, inv, exp in VERIFY:
            cmd = ["quint", "verify", os.path.join(HERE, m[0]), "--main", m[1], "--step", m[2],
                   "--max-steps=6", f"--invariant={inv}"]
            t = time.time()
            p = subprocess.run(cmd, capture_output=True, text=True)
            out = p.stdout + p.stderr
            v = ("hold" if "[ok] No violation found" in out else
                 "violate" if "[violation]" in out else "error: " + out[-300:])
            ok &= v == exp
            results["verify"].append({"module": m[1], "invariant": inv, "max_steps": 6,
                                      "expected": exp, "got": v,
                                      "secs": round(time.time() - t, 1)})
            print(f"{'PASS' if v == exp else 'FAIL'} verify {inv:34} expect {exp} got {v}")
    results["all_expected"] = ok
    json.dump(results, open(os.path.join(HERE, "results.json"), "w"), indent=1)
    print("ALL EXPECTED" if ok else "UNEXPECTED RESULTS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
