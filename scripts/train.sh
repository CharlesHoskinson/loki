#!/usr/bin/env bash
# train.sh [--base REF] [--gpu-repo PATH --gpu-ref REF] [--apply] BRANCH...
# Merge train: in a THROWAWAY worktree, rebase each BRANCH in the given order onto the running tip,
# run `verify-main.sh --tier cpu` after each, and (optionally) do the loki-gpu subtree pull first.
# Default is a dry run: result lands at refs/train/<stamp>/tip and per-branch refs/train/<stamp>/<n>; main is never moved.
# --apply fast-forwards the local branch given by $TRAIN_TARGET (default main) to the tip, ONLY if LOKI_TRAIN_APPLY=1 is
# also set in the environment (coordinator decision). Never pushes. Stops at the first failing branch.
# Order is yours to give; get it from `scripts/loki-dag order`.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; repo="$(git -C "$here" rev-parse --show-toplevel)"
base=main; gpu_repo=""; gpu_ref=main; apply=0; branches=()
while [[ $# -gt 0 ]]; do case "$1" in
  --base) base="$2"; shift 2;; --gpu-repo) gpu_repo="$2"; shift 2;; --gpu-ref) gpu_ref="$2"; shift 2;;
  --apply) apply=1; shift;; -*) echo "unknown flag $1" >&2; exit 64;; *) branches+=("$1"); shift;; esac; done
(( ${#branches[@]} )) || { echo "no branches" >&2; exit 64; }
stamp="$(date -u +%Y%m%dT%H%M%S)-$$"; ns="refs/train/$stamp"
wt="$(mktemp -d "${TMPDIR:-/tmp}/train-$stamp-XXXX")/wt"
verify="${TRAIN_VERIFY:-$here/verify-main.sh}"
cleanup(){ git -C "$repo" worktree remove --force "$wt" 2>/dev/null || true; }
trap cleanup EXIT
git -C "$repo" worktree add --detach "$wt" "$base" >/dev/null
tip="$(git -C "$wt" rev-parse HEAD)"
if [[ -n "$gpu_repo" ]]; then
  echo "== subtree pull loki-gpu from $gpu_repo $gpu_ref"
  git -C "$wt" subtree pull --prefix=loki-gpu "$gpu_repo" "$gpu_ref" -m "Pull loki-gpu $gpu_ref" >/dev/null \
    || { echo "TRAIN STOP: subtree pull conflict" >&2; exit 3; }
  tip="$(git -C "$wt" rev-parse HEAD)"
fi
n=0
for b in "${branches[@]}"; do
  n=$((n+1)); echo "== [$n/${#branches[@]}] rebase $b onto ${tip:0:8}"
  git -C "$wt" checkout -q --detach "$b"
  if ! git -C "$wt" rebase -q "$tip" 2>/tmp/train-rebase.err; then
    git -C "$wt" rebase --abort 2>/dev/null || true
    echo "TRAIN STOP: rebase conflict on $b (port/resolve it, then re-run); tip stays ${tip:0:8}" >&2; exit 4
  fi
  if ! "$verify" --tier cpu --tree "$wt"; then echo "TRAIN STOP: CPU tier failed on $b" >&2; exit 5; fi
  r="$(git -C "$wt" rev-parse HEAD)"
  git -C "$repo" update-ref "$ns/$n-$(basename "$b")" "$r"
  # --no-ff keeps the branch as a visible unit in history
  git -C "$wt" checkout -q --detach "$tip"
  git -C "$wt" merge -q --no-ff -m "Merge $b (train $stamp, MERGE-CPU green)" "$r"
  tip="$(git -C "$wt" rev-parse HEAD)"
done
git -C "$repo" update-ref "$ns/tip" "$tip"
echo "TRAIN OK: $ns/tip = ${tip:0:8} (main untouched)"
echo "NEXT: queue RECEIPT-NATIVE jobs against the merged sha; see docs/MERGE-GATES.md"
if (( apply )); then
  [[ "${LOKI_TRAIN_APPLY:-}" == 1 ]] || { echo "--apply needs LOKI_TRAIN_APPLY=1 (coordinator decision); not applied" >&2; exit 6; }
  tgt="${TRAIN_TARGET:-main}"
  git -C "$repo" merge-base --is-ancestor "$tgt" "$tip" || { echo "$tgt is not an ancestor of tip; not applying" >&2; exit 7; }
  git -C "$repo" update-ref "refs/heads/$tgt" "$tip" "$(git -C "$repo" rev-parse "$tgt")"
  echo "applied: $tgt -> ${tip:0:8} (local only, no push)"
fi
