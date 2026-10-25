#!/usr/bin/env bash
# Build every circuits-family arm in its own workspace and target directory
# (SUITE.md "Configurations, CPU budgets and builds"), then archive metadata,
# feature trees and binary digests under campaign/builds/<stamp>/.
#   bench/build.sh [arm ...]   arms: stock loki-cpu loki-gpu verifier loki-cpu-profile loki-gpu-profile
set -euo pipefail
here="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$here"
export CARGO_BUILD_JOBS=4
export PATH="$HOME/.local/opt/cuda-13.4.3/bin:$PATH" CUDA_PATH="$HOME/.local/opt/cuda-13.4.3"
toolchain=+1.99.0
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="$here/campaign/builds/$stamp"
mkdir -p "$out"
[[ -f deps/STAGED ]] || { echo "run bench/stage.sh first" >&2; exit 2; }
cp deps/STAGED "$out/STAGED"
arms=("$@"); [[ ${#arms[@]} -gt 0 ]] || arms=(stock loki-cpu loki-gpu verifier)
for arm in "${arms[@]}"; do
  case "$arm" in
    stock)            ws=stock;    bin=bench-driver;   feats=circuits ;;
    loki-cpu)         ws=loki;     bin=bench-driver;   feats=circuits ;;
    loki-gpu)         ws=loki;     bin=bench-driver;   feats=circuits,g0 ;;
    loki-cpu-profile) ws=loki;     bin=bench-driver;   feats=circuits,profile ;;
    loki-gpu-profile) ws=loki;     bin=bench-driver;   feats=circuits,g0,profile ;;
    verifier)         ws=verifier; bin=stock-verifier; feats=circuits ;;
    *) echo "unknown arm $arm" >&2; exit 2 ;;
  esac
  target="$here/target-circuits-$arm"
  echo "== $arm: $ws --features $feats -> $target"
  cargo $toolchain metadata --locked --offline --format-version 1 --filter-platform x86_64-unknown-linux-gnu --manifest-path "$ws/Cargo.toml" --features "$feats" > "$out/$arm.metadata.json"
  cargo $toolchain tree --locked --offline --target x86_64-unknown-linux-gnu -e normal,build,features --manifest-path "$ws/Cargo.toml" --features "$feats" > "$out/$arm.tree.txt"
  CARGO_TARGET_DIR="$target" cargo $toolchain build --release --locked --offline \
    --manifest-path "$ws/Cargo.toml" --bin "$bin" --features "$feats" > "$out/$arm.build.log" 2>&1
  b="$target/release/$bin"
  {
    echo "arm=$arm workspace=$ws features=$feats"
    echo "binary=$b"
    echo "binary_sha256=$(sha256sum "$b" | cut -d' ' -f1)"
    echo "manifest_sha256=$(sha256sum "$ws/Cargo.toml" | cut -d' ' -f1)"
    echo "lock_sha256=$(sha256sum "$ws/Cargo.lock" | cut -d' ' -f1)"
    echo "adapter_sha256=$(cat adapter/*.rs | sha256sum | cut -d' ' -f1)"
    echo "rustc=$(rustc $toolchain -V)"
    echo "no_threads_in_tree=$(grep -c 'blst feature "no-threads"' "$out/$arm.tree.txt" || true)"
  } | tee "$out/$arm.binary.txt"
done
ln -sfn "$stamp" "$here/campaign/builds/latest"
echo "build receipts: $out"
