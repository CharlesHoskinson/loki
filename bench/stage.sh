#!/usr/bin/env bash
# Stage immutable, read-only source snapshots for the benchmark workspaces.
#
#   bench/stage.sh [LOKI_REV]
#
# deps/moriarty  <- `git archive` of the pinned Sherlock commit (read-only git
#                   access to ~/repos/Moriarty-sherlock; nothing there is written)
# deps/loki      <- `git archive` of LOKI_REV from this repository (default HEAD)
#
# The prover workspaces depend on these snapshots by relative path, never on a
# live checkout. Snapshots are made read-only and their tree hashes recorded in
# deps/STAGED. Re-staging an existing snapshot is refused unless the recorded
# revision is identical. Nothing here is committed (bench/.gitignore).
set -euo pipefail

here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo="$(git -C "$here" rev-parse --show-toplevel)"
moriarty_repo="${MORIARTY_SHERLOCK:-$HOME/repos/Moriarty-sherlock}"
moriarty_rev="f2ea193777897869c2e45c08601cc412e00c3897"
loki_rev="$(git -C "$repo" rev-parse "${1:-HEAD}")"
deps="$here/deps"
mkdir -p "$deps"

stage() { # name repo rev
  local name="$1" src="$2" rev="$3" out="$deps/$1"
  local tree
  tree="$(git -C "$src" rev-parse "$rev^{tree}")"
  if [[ -e "$out" ]]; then
    if [[ "$(cat "$out/.staged-rev" 2>/dev/null)" == "$rev" ]]; then
      echo "staged $name $rev (already present)"
      return
    fi
    echo "refusing to replace $out (staged $(cat "$out/.staged-rev" 2>/dev/null || echo unknown), wanted $rev)" >&2
    echo "remove it explicitly (chmod -R u+w $out && rm -rf $out) to re-stage" >&2
    exit 2
  fi
  local tmp="$out.tmp.$$"
  mkdir "$tmp"
  git -C "$src" archive --format=tar "$rev" | tar -x -C "$tmp"
  echo "$rev" > "$tmp/.staged-rev"
  echo "$tree" > "$tmp/.staged-tree"
  chmod -R a-w "$tmp"
  mv "$tmp" "$out"
  echo "staged $name $rev tree $tree"
}

stage moriarty "$moriarty_repo" "$moriarty_rev"
stage loki "$repo" "$loki_rev"

{
  echo "moriarty_rev=$moriarty_rev"
  echo "moriarty_tree=$(cat "$deps/moriarty/.staged-tree")"
  echo "loki_rev=$loki_rev"
  echo "loki_tree=$(cat "$deps/loki/.staged-tree")"
  echo "loki_main_base=0c4ab9d0961a008dee6340d8eab9dcc90fd64ab4"
  echo "loki_src_patch_sha256=$(git -C "$repo" diff 0c4ab9d0961a008dee6340d8eab9dcc90fd64ab4 "$loki_rev" -- src Cargo.toml | sha256sum | cut -d' ' -f1)"
} > "$deps/STAGED.tmp"
chmod u+w "$deps/STAGED" 2>/dev/null || true
mv "$deps/STAGED.tmp" "$deps/STAGED"
cat "$deps/STAGED"
