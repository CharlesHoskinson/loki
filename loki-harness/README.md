# Loki differential harness

This independent Cargo workspace compares the Loki path package with unmodified
crates.io `midnight-proofs =0.8.3`. Cargo accepts the two sources with package
renaming; no stock source was vendored. Keep the registry dependency unpatched
so it remains an independent verifier.

Run the commands below from the Loki repository root. The recorded default gate
covers k10, k14 and k17 with stock 0.8.3. G2 also lists k20 and stock 0.8.1,
but no run for those cases is recorded. The Loki path package also resolves
optional dependencies from the sibling `loki-gpu` checkout, even when the
backend feature is off. See the checkout layout in [`README.md`](../README.md)
and [`TODO.md`](../TODO.md).

The recorded fixtures do not cover multiple proofs with mixed committed and
plain instance columns. In that shape, the stock prover and verifier absorb
instances in different orders, so the stock verifier rejects its own proof.
This inherited limitation is documented in [`LOKI-QUINT.md`](../LOKI-QUINT.md)
and tracked as TODO T03.

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

`run.sh` runs k10, k14 and k17 in sequence, sharing each proof pair between cross
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
Both backends run key generation independently. The gate checks VK byte equality,
round-trips full SRS bytes in both directions with checked `RawBytes` reads, and
loads the other backend's VK and verifier parameters before verifying its proof.
Wrong public inputs and empty proofs must be rejected by both verifiers.
The harness also requires complete transcript consumption and rejects an
otherwise valid proof with an appended byte in both cross directions.
MockProver controls mutate each of the four constraint families independently.
For a mutated witness, both MockProver implementations decide whether it
satisfies the constraints. The verifiers must reject a failing mutant and accept
a satisfying one.

All witnesses and randomness are synthetic. This harness uses fixed ChaCha20
seeds for `unsafe_setup` and proof generation. The SRS parameters are insecure
and must never be used in production. The harness saves no proofs or witnesses.

`keygen_s` includes VK and PK generation, `prove_s` includes synthesis and proof
creation, and `verify_s` includes checked VK/verifier-parameter decoding,
transcript parsing and final pairing verification. SRS generation/serialization,
full SRS reads, and extra cross/negative verification are outside these timings.
The harness takes one cold sample per backend, with Loki first. These timings do
not establish a speed difference.

## Determinism policy

Every differential compares proof bytes using fixed RNG seeds and reports
lengths, first differing offset (zero based), byte values, and total differing
bytes. Lookup inequality alone is diagnostic. Stock
`src/plonk/lookup/prover.rs:399-430` iterates a randomly seeded `HashMap` while
placing leftover lookup table entries. A fixed proof RNG and one Rayon worker
do not fix that map's iteration order, so even stock versus stock can differ.
The first differing prefix is reported. A matching prefix alone does not show
which transcript objects are deterministic.

A no-lookup control retains the nonlinear gate, permutation, public input and
challenge-dependent second phase. With the same seed and worker count, it must
produce byte-identical proofs.
Cross-verifier acceptance and VK/SRS identity are mandatory for every case.
`LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10` additionally fails on any
lookup proof byte difference; this is intentionally likely to fail on stock.
Investigate byte differences outside the documented lookup nondeterminism.
Lookup variation gives no evidence about unrelated prover changes.

## Legacy sppark CUDA runs

The harness `cuda` feature enables the legacy `loki/cuda` MSM path; the stock
dependency keeps its default CPU features. Set `LOKI_GPU_MSM_MIN=1` to route
the small k10 fixture to the GPU. The build also needs the CUDA toolkit and
`nvcc`; see the root README for adapter-specific toolkit variables.

```sh
RAYON_NUM_THREADS=1 LOKI_GPU_MSM_MIN=1 cargo run --release --manifest-path loki-harness/Cargo.toml --features cuda --bin gate -- 10
```

A CUDA build alone does not prove GPU dispatch; confirm the backend's dispatch
telemetry separately.

The separate `loki-gpu-backend` feature builds the P1 `loki-gpu` skeleton. Its
current backend advertises no accelerated operations, so a passing harness gate
with that feature checks compile and feature forwarding only. See
[`LOKI-P1-SEAM.md`](../LOKI-P1-SEAM.md).

## Recorded validation (2026-10-24)

This record predates the P1 merge. The later default and seam gate results are
in [`LOKI-P1-SEAM.md`](../LOKI-P1-SEAM.md). The P1 backend feature check is not
a GPU proof run.

The final `./loki-harness/run.sh` completed with exit 0 under the global heavy
lock: 72.78 seconds wall, 508,002,304 bytes peak RSS (warm
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
  loki-harness/Cargo.toml --features cuda --all-targets`: passed as a compile
  check; no GPU proof was run.
- The benchmark command above with `-- 10`: exit 0; both JSON records validated
  for backend name, k, timing fields, and proof length.
- `LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10`: exit 1 as expected,
  reporting a lookup byte difference at offset 192.
- Formatting, `bash -n loki-harness/run.sh`, and `git diff --check`: passed.

The appended-byte regression failed before the transcript fix and passed after
it.
