# Loki phase profile

Measured 2026-10-24 on the W1 worktree, with `midnight-proofs` 0.8.3.
Implementation: `82ff1b2` (`Add optional prover phase profiling and arithmetic counts`).
This is a filled-domain synthetic CPU workload that exercises every requested
prover phase. It is not a Sherlock recursive parent or a k20 result.

## Interface

Enable `loki-profile` and call `midnight_proofs::profile::take_report()` after
`create_proof`. The returned `Report { proofs: Vec<ProofReport> }` implements
serde `Serialize` and `Deserialize`. Taking a report drains completed attempts
atomically; active proofs remain private until completion. Concurrent callers
have independent records. Reports accumulate until drained.

Each proof contains `k`, batch circuit count, success, `total_wall_ns`, a phase
map (`calls`, `wall_ns`), and MSM/FFT counters (`calls`, `elements`, size-to-count
histogram). MSM lengths are measured before zero filtering; FFT counts include
forward and inverse transforms. Keygen and verification are excluded.

`LOKI_PROFILE=/absolute/path.jsonl` also appends one JSON object per completed
proof attempt. Writes are synchronized within the process; separate processes
should use separate files. An output error is available as `jsonl_error` in the
in-memory report and does not change proof success. The output contains no
witness, SRS, key or transcript values. Runtime profiling uses std and the serde
already required by the crate; no dependency was added to the library.

With the feature disabled, the module, guards, counters and environment access
are all removed by `#[cfg(feature = "loki-profile")]`.

## Timing definitions

All phase times are **exclusive wall time**, including waits for parallel work.
The sum equals `total_wall_ns`; publication, mutex waiting at publication, and
JSONL I/O occur after that measurement. `other` records work outside the named
phases, including orchestration.

- `witness_synthesis.phase_N` includes witness allocation, floor-planner synthesis,
  rational inversion and blinding for phase N. `advice_commitments` times the
  advice MSMs and excludes their transcript writes.
- Lookup preparation includes permuted-value construction and commitments.
  Lookup/permutation products include construction, transforms and commitments.
  `trash` is the stock trash compression/commitment argument, not a grand product.
- `compute_nu_poly` measures column extension outside the nested evaluator;
  `quotient_evaluation` measures `Evaluator::evaluate_numerator`, including the
  grand-product coset conversions it invokes. Add these for inclusive nu time.
- `quotient_construct` covers division by the vanishing polynomial, inverse FFT,
  splitting and blinding; `quotient_commitment` covers the quotient-piece MSMs.
- `evaluations` covers polynomial evaluations and their transcript writes.
  `multiopen` covers query assembly and remaining finalization outside the nested
  `kzg_multiopen`, which times the KZG multi-opening implementation itself.
- `other` includes instance preparation, advice coefficient conversion, initial
  vanishing blinding/commitment, transcript orchestration and cleanup.

Counters run at arithmetic entry on the calling thread, before worker dispatch.
They count all current built-in prover FFT/MSM calls. If future backends move
those entry calls into worker tasks, they must carry the proof context or keep
these hooks at the dispatch boundary.

## k16 measurement

Host: Intel Core Ultra 9 285K, 20 cores, Linux; rustc 1.99.1; release build,
`CARGO_BUILD_JOBS=4`, `RAYON_NUM_THREADS=4`. BLST retains its stock threading.
One measured proof, shared host, no speedup comparison or variance estimate.

Circuit: 65,520 occupied rows in a 65,536-row domain; three advice columns in two
phases; equality copies between two columns; a 16-value range lookup; and an
additively selected square constraint exercising trash. The extended FFT length
is 262,144. The synthetic fixture uses deterministic test randomness and unsafe
test-only SRS setup; it is not a production SRS generator.

Build and direct reproduction (run the proof only while holding the global
DoctorWatson heavy slot):

```bash
CARGO_BUILD_JOBS=4 cargo build --offline --release --features loki-profile --example loki-profile
RAYON_NUM_THREADS=4 LOKI_PROFILE="$PWD/target/profile-k16.jsonl" \
  target/release/examples/loki-profile 16 target/profile-k16-proof.bin
```

The measured invocation used the actual shared-lock wrapper, with its records
inside this worktree:

```bash
PYTHONDONTWRITEBYTECODE=1 \
PYTHONPATH=/home/charles/repos/Moriarty-factory/models/doctorwatson \
DW_REPO="$PWD/target/profile-measure" RAYON_NUM_THREADS=4 \
LOKI_PROFILE="$PWD/target/profile-k16.jsonl" \
python3 -m factory heavy --id W1-profile-k16 -- \
  "$PWD/target/release/examples/loki-profile" 16 "$PWD/target/profile-k16-proof.bin"
```

DoctorWatson run `20261024T234849Z-W1-profile-k16`: exit 0; whole fixture wall
**3.806 s**, peak process-group RSS **220,127,232 bytes** (~209.93 MiB), including
SRS setup, keygen and local verification. The following times cover only
`create_proof`. The fixture asserted that their sum was within 10% of an external
`Instant` around the call, including report publication and JSONL output.

| Phase | Calls | Wall ms | Proof share |
|---|---:|---:|---:|
| `witness_synthesis.phase_0` | 1 | 23.984 | 2.13% |
| `witness_synthesis.phase_1` | 1 | 13.396 | 1.19% |
| `advice_commitments` | 2 | 87.938 | 7.80% |
| `lookup_prepare` | 1 | 54.371 | 4.82% |
| `permutation_products` | 1 | 45.427 | 4.03% |
| `lookup_products` | 1 | 78.025 | 6.92% |
| `trash` | 1 | 8.915 | 0.79% |
| `compute_nu_poly` | 1 | 74.121 | 6.58% |
| `quotient_evaluation` | 1 | 209.484 | 18.58% |
| `quotient_construct` | 1 | 36.943 | 3.28% |
| `quotient_commitment` | 1 | 190.182 | 16.87% |
| `evaluations` | 1 | 16.247 | 1.44% |
| `multiopen` | 1 | 0.033 | 0.00% |
| `kzg_multiopen` | 1 | 211.465 | 18.76% |
| `other` | 1 | 76.753 | 6.81% |
| **Total** | | **1127.287** | **100%** |

The unrounded sum is **1,127,286,910 ns**, exactly the recorded total.

| Arithmetic | Calls | Total input elements | Input-length histogram |
|---|---:|---:|---|
| MSM | 14 | 917,503 | 13 × 65,536; 1 × 65,535 |
| FFT / IFFT | 17 | 2,883,584 | 8 × 65,536; 9 × 262,144 |

Raw report: `target/profile-k16.jsonl`. Whole-run log:
`target/profile-k16-run.log`; DoctorWatson trace:
`target/profile-measure/qa/sherlock/traces/20261024T234849Z-W1-profile-k16.log`.
These local build artifacts are not committed.

## Verification

The feature-required example test proves a k6 circuit, verifies it, asserts every
required phase has positive calls/time, checks histogram sums and destructive
report taking, and compares phase sum to both the internal total and external
wall time. Its second test launches separate child processes to check JSONL
append, agreement with serde output, and successful proofs despite output errors.
The library test covers concurrent active/completed isolation, nested exclusive
timers, recursive proof context restoration, serialization, and early-return and
unwind cleanup.

```bash
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --features loki-profile --example loki-profile
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --no-default-features --features loki-profile --example loki-profile
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --features loki-profile --lib profile::tests
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --all-features --lib profile::tests
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --test plonk_api
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo test --offline --release --features loki-profile --test plonk_api
CARGO_BUILD_JOBS=4 cargo check --offline --release --no-default-features
```

The separate `tests/stock-profile-verifier` package depends on the **unmodified
registry** `midnight-proofs = "=0.8.3"` and `midnight-curves = "=0.3.1"`, with no
local patch. It reconstructs the same deterministic SRS and VK from the shared
circuit, verifies exported Loki proof bytes and checks full transcript consumption:

```bash
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 cargo run --offline --release --features loki-profile --example loki-profile -- 6 target/profile-k6-proof.bin
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=4 CARGO_TARGET_DIR="$PWD/target" \
  cargo run --offline --release --manifest-path tests/stock-profile-verifier/Cargo.toml \
  -- 6 target/profile-k6-proof.bin
```

The k6 and k16 exported proofs are both 1,280 bytes and were accepted by that
stock verifier. For the measured k16 artifact, run the same package with
`-- 16 target/profile-k16-proof.bin` under the global heavy slot. The completed
stock k16 check is recorded in `target/profile-stock-k16.log` (exit 0; whole
fixture 2.627 s, including SRS reconstruction and keygen).

Results: feature/default and feature/no-default example suites each passed 2/2;
profile unit tests passed 1/1 with ordinary features and 1/1 with all features;
the PLONK API test passed 1/1 both with and without profiling; and the
feature-off, no-default library check passed. Setting `LOKI_PROFILE` in the feature-off API run created no file.
Lookup iteration retains stock nondeterminism, so byte equality across separately
generated proofs is not claimed. The instrumentation changes no arithmetic,
randomness consumption, verifier code or transcript/proof encoding.

The standalone stock helper also passed release checks with
`--no-default-features` and with `--features circuit-params`; its feature flags
are forwarded to the registry dependency.
