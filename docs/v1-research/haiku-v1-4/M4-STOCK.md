# M4-STOCK: stock midnight-proofs 0.8.3 compat-gate options for iOS and Android

## Facts (file:line)
- Stock oracle pinned as dev-dependency: /home/charles/repos/loki-proofs/Cargo.toml:217-219 (`=0.8.3`, package midnight-proofs).
- W2 gate is host-only: /home/charles/repos/loki-proofs/loki-harness/README.md and loki-harness/run.sh:1-9 (x86_64 `cargo run --release --bin gate`, k10/k14/k17, 292 s timeout).
- Stock 0.8.3 target gates: ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/midnight-proofs-0.8.3/Cargo.toml:172-178 (getrandom js on wasm32-unknown-unknown; midnight-curves portable under cfg(ci_build)). No ios or android gate.
- Probe log: ~/repos/loki-wt/v1/docs/v1-research/haiku-v1-4/probe-android-check.txt (blst cc-rs fails without NDK).

## Options
A. Device produces bytes, host verifies (cheapest, recommended for v1 G3). Run the prover on the phone, save proof, VK and transcript bytes, then verify with stock 0.8.3 natively on the host and with the Loki verifier. This is the approach docs/PLATFORMS.md:355 already states ("run natively on the produced bytes"). Needs no NDK for stock.
B. Cross-compile stock 0.8.3 to the device target and run the verifier on device. Needs the NDK (for blst) and iOS SDK (macOS). Use for the on-device stock check once the toolchain exists.
C. Both A and B: A in CI on Linux, B on a macOS and NDK runner. Recommended end state.

## Proposed test matrix (v1)
| Case | Host | Device | Stock check |
|---|---|---|---|
| k10 leaf prove on device, verify host stock | Linux CI | aarch64-linux-android emulator or phone | stock 0.8.3 native |
| same, iOS | macOS runner | iPhone or simulator aarch64-apple-ios-sim | stock native |
| VK byte equality | host | device | cmp |
| negative control (flipped byte) | host | device | stock must reject |
| stock 0.8.1 vs 0.8.3 | host | n/a | (owned by compat-proof, area 2) |

Gate definitions follow docs/PLATFORMS.md:27-43 (G1 arithmetic, G3 stock verifier, G4 failure, G6 chain latency).

## Caveats
- Option B is UNVERIFIED on this host: no NDK, no Xcode. Blocked until installed.
- Blocked item for area 2: W2 harness fixtures do not cover mixed committed and plain instances (loki-harness/README.md, TODO T03). The mobile matrix must inherit this exclusion.
- Recommend the device run emit fixed seeds (harness uses ChaCha20 synthetic randomness, loki-harness/README.md) so host and device bytes can be compared.
