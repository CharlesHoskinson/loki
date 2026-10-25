#!/usr/bin/env python3
"""Compact quint-q1 ITF traces for the Rust replay: drop consecutive duplicate states
(terminal stutter) and keep the static pPlan/vPlan only in the final state.
Usage: itf_compact.py OUT_DIR IN.itf.json... (inputs are deleted after compaction)."""
import json, os, sys

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
for path in sys.argv[2:]:
    t = json.load(open(path))
    states, prev = [], None
    for s in t["states"]:
        key = json.dumps(s["st"], sort_keys=True)
        if key != prev:
            states.append(s)
        prev = key
    for s in states[:-1]:
        s["st"] = {k: v for k, v in s["st"].items() if k not in ("pPlan", "vPlan")}
    t["states"] = states
    name = os.path.basename(path)
    json.dump(t, open(os.path.join(out, name), "w"))
    os.remove(path)
