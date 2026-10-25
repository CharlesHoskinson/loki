#!/usr/bin/env bash
# Self-test for train.sh on a throwaway repo. Runs in seconds, no cargo, no locks.
set -euo pipefail
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$T"; git init -q -b main r; cd r
git config user.email t@t; git config user.name t
mkdir scripts; cp "$here/train.sh" scripts/; echo base > a; echo x > shared; git add .; git commit -qm base
for b in p4 p3 p5; do git checkout -q -b loki/$b main; echo $b > f-$b; git add .; git commit -qm $b; done
git checkout -q main; echo main2 > m; git add .; git commit -qm main2   # main moved: every branch needs a rebase
cat > ok.sh <<'V'
#!/usr/bin/env bash
exit 0
V
cat > bad.sh <<'V'
#!/usr/bin/env bash
[[ -f "$4/f-p3" ]] && { echo p3 fails; exit 1; } || exit 0
V
chmod +x ok.sh bad.sh
before=$(git rev-parse main)
TRAIN_VERIFY=$T/r/ok.sh bash scripts/train.sh loki/p4 loki/p3 loki/p5
tip=$(git for-each-ref --format="%(objectname)" "refs/train/*/tip" | head -1)
for f in f-p4 f-p3 f-p5 m; do git cat-file -e "$tip:$f"; done
[[ $(git rev-parse main) == "$before" ]] || { echo FAIL main moved; exit 1; }
# failure stops the train and leaves no tip
git for-each-ref --format='%(refname)' refs/train | xargs -n1 git update-ref -d
if TRAIN_VERIFY=$T/r/bad.sh bash scripts/train.sh loki/p4 loki/p3 loki/p5; then echo FAIL expected stop; exit 1; fi
[[ -z $(git for-each-ref "refs/train/*/tip") ]] || { echo FAIL tip written on failure; exit 1; }
# --apply without the env var refuses
if TRAIN_VERIFY=$T/r/ok.sh bash scripts/train.sh --apply loki/p4 2>/dev/null; then echo FAIL apply unguarded; exit 1; fi
[[ $(git rev-parse main) == "$before" ]] || { echo FAIL main moved on refused apply; exit 1; }
LOKI_TRAIN_APPLY=1 TRAIN_VERIFY=$T/r/ok.sh bash scripts/train.sh --apply loki/p4 >/dev/null
[[ $(git rev-parse main) != "$before" ]]
echo SELFTEST PASS
