"""Compare proof-emitter outputs (see run-w10.sh). Exit 1 on an unexpected divergence."""
import json, sys, pathlib
d = pathlib.Path(sys.argv[1])
load = lambda n: {(r["k"], r["coverage"], r["srs"], r["seed"]): r for r in map(json.loads, open(d / f"{n}.jsonl"))}
V = {n: load(n) for n in ["main", "main-det", "w10-head", "w10-head+arg", "w10-merged", "w10-merged+arg", "w10-merged+arg+det"]}
VALID = {"Exact", "Downsized", "Serialized"}
fail, summary = [], {}
def cmp(a, b, keys_pred, expect_equal=True, label=None):
    label = label or f"{a} vs {b}"
    eq = neq = 0
    for key, ra in V[a].items():
        if not keys_pred(key): continue
        rb = V[b][key]
        same = all(ra[f] == rb[f] for f in ["proof", "vk", "draws", "draw_digest", "word_pos"])
        eq += same; neq += not same
        if expect_equal and not same: fail.append(f"{label}: {key} differs")
        for f in ["vk", "draws", "draw_digest", "word_pos"]:
            if ra[f] != rb[f] and key[2] != "BadLagrange": fail.append(f"{label}: {key} {f} differs")
    summary[label] = {"equal": eq, "different": neq}
everything = lambda k: True
good = lambda k: k[2] != "BadLagrange"
single = lambda k: k[1] == "Single"
# 1. W10 strategy isolated: same W10 head source, feature off vs on.
cmp("w10-head", "w10-head+arg", good)
cmp("w10-head", "w10-head+arg", lambda k: k[2] == "BadLagrange", expect_equal=False, label="w10-head vs w10-head+arg [BadLagrange, invalid SRS]")
# 2. W10 head (sorted leftovers) vs main with the opt-in sorted traversal.
cmp("w10-head+arg", "main-det", good)
# 3. Auto-merged argument files on main: deterministic order, and default order on order-independent fixtures.
cmp("w10-merged+arg+det", "main-det", good)
cmp("w10-merged+arg", "main", lambda k: good(k) and single(k))
cmp("w10-merged", "main", single)
# 4. Known difference: W10 head default leftover order is the sorted one, main default is stock HashMap order.
cmp("w10-head", "main", lambda k: not single(k), expect_equal=False, label="w10-head vs main [multi-leftover, expected to differ]")
# 5. Stock verifier verdicts.
for n, rows in V.items():
    acc = {s: sum(r["stock_accepts"] for k, r in rows.items() if k[2] == s) for s in ["Exact", "Oversized", "Downsized", "Serialized", "BadLagrange"]}
    summary[f"stock_accepts[{n}]"] = acc
    for k, r in rows.items():
        if k[2] in VALID and not r["stock_accepts"]: fail.append(f"stock rejects {n} {k}")
        if r["stock_accepts"] != V["main"][k]["stock_accepts"]: fail.append(f"stock verdict differs from main: {n} {k}")
json.dump({"summary": summary, "failures": fail}, open(d / "w10-compare.json", "w"), indent=1)
print(json.dumps(summary, indent=1)); print("FAILURES:", len(fail)); [print(" ", f) for f in fail[:40]]
sys.exit(1 if fail else 0)
