#!/usr/bin/env bash
# Build CPU-only copies of W10 for differential replay. Never touches the W10
# worktree (reads only git objects) and never enables `cuda`.
#  head:   W10 head 5e37ead verbatim, except `cuda-argument-commit = []`
#          (the feature's only effect becomes the commit_argument cfg).
#  merged: main a256a9a plus the three W10 argument files as `git merge-tree`
#          auto-merges them (lookup/permutation provers, poly/commitment.rs),
#          plus `cuda-argument-commit = []`. The full W10 merge has conflicts in
#          Cargo.toml, domain.rs, kzg/msm.rs and profile.rs; this copy does NOT
#          resolve them and is not the final merge.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
gpu=$(cd "$repo/../loki-gpu" && pwd)
W10=5e37ead2fd03a9e476f0e7b4d80d3e644be4683f
MAIN=a256a9a
rm -rf "$here/copies"; mkdir -p "$here/copies/head" "$here/copies/merged"
git -C "$repo" archive "$W10" | tar -x -C "$here/copies/head"
git -C "$repo" archive "$MAIN" | tar -x -C "$here/copies/merged"
tree=$(git -C "$repo" merge-tree --write-tree "$MAIN" "$W10" | head -1 || true)
for f in src/plonk/lookup/prover.rs src/plonk/permutation/prover.rs src/poly/commitment.rs; do
  git -C "$repo" show "$tree:$f" > "$here/copies/merged/$f"
  if grep -q '^<<<<<<<\|^>>>>>>>' "$here/copies/merged/$f"; then echo "conflict in $f"; exit 1; fi
done
for d in head merged; do
  m="$here/copies/$d/Cargo.toml"
  grep -q '^cuda-argument-commit = \["cuda"\]' "$m" && sed -i 's/^cuda-argument-commit = \["cuda"\]/cuda-argument-commit = []/' "$m" \
    || sed -i 's/^loki-deterministic-lookup-order = \[\]/loki-deterministic-lookup-order = []\ncuda-argument-commit = []/' "$m"
  sed -i "s#path = \"../loki-gpu/#path = \"$gpu/#" "$m"
  grep -q '^cuda-argument-commit = \[\]' "$m"
  # The fixture crate is the only consumer; keep the copy out of any parent workspace.
  grep -q '^\[workspace\]' "$m" || printf '\n[workspace]\n' >> "$m"
done
echo "W10 head=$W10 merged-tree=$tree"
