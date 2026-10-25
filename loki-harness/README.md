# Loki differential harness

Independent Cargo workspace comparing the parent Loki path package with
**unmodified crates.io `midnight-proofs =0.8.3`**. Cargo accepts the two sources
with package renaming; no stock source was vendored. Do not patch crates.io
`midnight-proofs` in this workspace: that would destroy the independent oracle.

```sh
./loki-harness/run.sh
# Individual cases (always serialize proof tests):
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked \
  --manifest-path loki-harness/Cargo.toml -- --test-threads=1 --nocapture
# Optional large fixture:
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked \
  --manifest-path loki-harness/Cargo.toml -- --ignored --test-threads=1 --nocapture
# JSON Lines: one {backend,k,keygen_s,prove_s,verify_s,proof_bytes} per backend:
RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo run --release --locked \
  --manifest-path loki-harness/Cargo.toml --bin bench -- 10 14 17
```

`run.sh` runs k10, k14, k17 in sequence, sharing each proof pair between cross
verification and determinism checks. Explicit numeric arguments override sizes.
It exports one Rayon worker before startup and sends a process-group timeout
after 292 seconds, with a three-second kill grace period (exit 124 on timeout,
or 137 if forced to kill), including compilation. Prebuild with
`cargo build --release --locked --manifest-path loki-harness/Cargo.toml --bins`
when measuring runtime separately. Avoid running proof tests concurrently with
another heavy job on this shared host. The k20 test is ignored by default.

The macro in `src/backend.rs` defines both circuit implementations and their
backend operations. Each circuit fills `2^k - 32` rows with a nonlinear custom
gate, equality copies on every row, a public input, an 8-bit table lookup, and a
second-phase column depending on a challenge drawn after first-phase advice.
The table contains every 8-bit value twice, leaving many distinct leftover
values so the fixture exercises stock's randomized lookup-table placement.
Both backends independently keygen. The gate asserts byte-identical VKs,
roundtrips full SRS bytes in both directions with checked `RawBytes` reads, and
reads the other backend's VK/verifier parameters before verifying its proof.
Wrong public inputs and empty proofs must be rejected by both verifiers.
The harness also requires complete transcript consumption and rejects an
otherwise valid proof with an appended byte in both cross directions.
MockProver controls mutate each of the four constraint families independently.

All witnesses and randomness are synthetic. Fixed ChaCha20 seeds are used for
test-only `unsafe_setup` and proof generation. These SRS parameters are insecure
and must never be used in production. No proof or witness artifacts are saved.

`keygen_s` includes VK and PK generation, `prove_s` includes synthesis and proof
creation, and `verify_s` includes checked VK/verifier-parameter decoding,
transcript parsing and final pairing verification. SRS generation/serialization,
full SRS reads, and extra cross/negative verification are outside these timings.
These are single cold measurements, with Loki first; no speedup is implied.

## Determinism policy

Every differential compares proof bytes using fixed RNG seeds and reports
lengths, first differing offset (zero based), byte values, and total differing
bytes. Lookup inequality alone is diagnostic. Stock
`src/plonk/lookup/prover.rs:399-430` iterates a randomly seeded `HashMap` while
placing leftover lookup table entries. A fixed proof RNG and one Rayon worker
do not fix that map's iteration order, so even stock versus stock can differ.
The first differing prefix is reported; a match before that offset is not
automatically a claim about which transcript objects are deterministic.

A no-lookup control retains the nonlinear gate, permutation, public input and
challenge-dependent second phase. It must produce byte-identical proofs.
Cross-verifier acceptance and VK/SRS identity are mandatory for every case.
`LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10` additionally fails on any
lookup proof byte difference; this is intentionally likely to fail on stock.
Unexpected differences still need investigation: lookup nondeterminism is not
evidence that arbitrary prover changes are correct.

## CUDA handoff

The harness reserves `cuda = []`, so `--features cuda` compiles before W4
provides the parent feature. **It currently executes CPU code.** Once W4 lands,
change exactly that line in this manifest to `cuda = ["loki/cuda"]` and refresh
the lockfile if W4 adds dependencies. Stock stays on its default CPU features.
Then the GPU runner can run the same `gate`, `bench`, and differential tests:

```sh
cargo run --release --manifest-path loki-harness/Cargo.toml --features cuda --bin gate -- 10
```

GPU runs still need `RAYON_NUM_THREADS=1` for determinism; ensure the host runner
exports it before running `gate` or `determinism` tests. A CUDA build alone does
not prove GPU dispatch; confirm the W4 backend's dispatch telemetry separately.

## Recorded validation (2026-10-24)

The final `./loki-harness/run.sh` completed with exit 0 under DoctorWatson's
global heavy lock: **72.78 seconds wall, 508,002,304 bytes peak RSS** (warm
release build). All k10/k14/k17 proofs were accepted in both cross directions;
full SRS roundtrips and independently generated VK bytes matched. All lookup
proofs were 1,280 bytes, first differing at offset 192. All no-lookup controls
were byte-identical at 1,136 bytes. No k20 or GPU proof was run.

Single samples from this gate, in seconds:

| k | Backend | Keygen | Prove | Verify |
|---|---|---:|---:|---:|
| 10 | Loki | 0.01456 | 0.03937 | 0.00202 |
| 10 | Stock | 0.01676 | 0.03772 | 0.00210 |
| 14 | Loki | 0.15766 | 0.36503 | 0.00208 |
| 14 | Stock | 0.15119 | 0.36364 | 0.00199 |
| 17 | Loki | 2.53931 | 4.89722 | 0.00289 |
| 17 | Stock | 2.16600 | 3.95878 | 0.00225 |

These shared-host single samples do not establish a speed difference.
The gate includes expensive test SRS generation outside the table's timings.
The full trace is under
`target/dw/qa/sherlock/traces/20261024T234718Z-W2-final-gate.log`;
extracted JSON records and targeted logs are under `target/validation/`.
Generated validation files are ignored by Git.

Additional commands actually run:

- `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --offline --locked --release
  --manifest-path loki-harness/Cargo.toml --test differential k10:: --
  --test-threads=1 --nocapture`: two tests passed after the transcript fix.
- The same test command filtered to `reject_broken_constraints`: one test
  passed, exercising valid witnesses and four isolated mutations on each backend.
- `CARGO_BUILD_JOBS=4 cargo check --offline --locked --manifest-path
  loki-harness/Cargo.toml --features cuda --all-targets`: passed; CPU placeholder.
- The benchmark command above with `-- 10`: exit 0; both JSON records validated
  for backend name, k, timing fields, and proof length.
- `LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10`: exit 1 as expected,
  reporting a lookup byte difference at offset 192.
- Formatting, `bash -n loki-harness/run.sh`, and `git diff --check`: passed.

The appended-byte regression failed before the transcript fix and passed after
it. A read-only review confirmed both transcript exhaustion and timeout child
cleanup fixes; no remaining blockers were identified.
