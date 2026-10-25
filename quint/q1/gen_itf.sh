#!/bin/sh
# Generate compact ITF traces for the Rust replay (quint/q1/mbt). Seeds are fixed.
set -e
cd "$(dirname "$0")/../.."
OUT=.artifacts/q1/itf; RAW=.artifacts/q1/raw
rm -rf "$OUT" "$RAW"; mkdir -p "$OUT" "$RAW"
for b in $(seq 0 23); do
  quint run quint/q1/transcript_q1.qnt --backend rust --seed $((20261024 + b)) \
    --max-samples 5 --n-traces 5 --max-steps 700 \
    --invariant 'safetyOutsideDefect and defectExact' \
    --out-itf "$RAW/main_b${b}_{seq}.itf.json" --verbosity 0
  python3 quint/q1/itf_compact.py "$OUT" "$RAW"/main_b${b}_*.itf.json
done
for b in 0 1 2; do
  quint run quint/q1/transcript_q1.qnt --init initAlias --backend rust --seed $((20261024 + b)) \
    --max-samples 4 --n-traces 4 --max-steps 700 --invariant aliasClassified \
    --out-itf "$RAW/alias_b${b}_{seq}.itf.json" --verbosity 0
  python3 quint/q1/itf_compact.py "$OUT" "$RAW"/alias_b${b}_*.itf.json
done
ls "$OUT" | wc -l
