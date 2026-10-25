# Differential Fuzz Coverage

## Purpose

Define differential fuzz inputs, deterministic coverage checks, and portable corpus replay for Loki components.

## ADDED Requirements

### Requirement: Stock 0.8.3 remains the differential oracle

Every compatibility fuzz target SHALL compare Loki with an unmodified registry `midnight-proofs =0.8.3` build; serialized keys and proofs SHALL cross crate boundaries only as bytes. Existing independent workspace and target declarations are at `fuzz/Cargo.toml:10-24,26-59` and the stock dependency is at `fuzz/diff/Cargo.toml:17-20`.

#### Scenario: Default and opt-in compatibility modes
- **WHEN** the CPU component suite runs with default features and each Loki-only feature mode paired with stock 0.8.3
- **THEN** `fuzz/quick.sh` and `cargo +stable test --release --locked --manifest-path fuzz/diff/Cargo.toml --test quick` SHALL compare both proof-verification directions and require exact bytes only for fixtures whose manifest marks byte identity, while intentional feature mismatches SHALL be asserted as negative controls; the source baseline is `fuzz/diff/Cargo.toml:7-20` and W7's existing exact-byte condition is `LOKI-FUZZ.md:52-59`.

### Requirement: Serialized object deserializers are differentially fuzzed

Fuzz inputs SHALL exercise proving/verifying-key, SRS, verifier-parameter and proof decoding, capped at 4096 bytes and custom SRS reads at k ≤ 6. Existing parser code is at `fuzz/diff/src/parse.rs:101-211`; stock-compatible read APIs are at `src/plonk/mod.rs:111-135,368-415` and `src/poly/kzg/params.rs:153-162`.

#### Scenario: Malformed and valid serialized objects
- **WHEN** the transcript/deserializer corpus is replayed and arbitrary bounded bytes are fuzzed
- **THEN** `cargo +nightly-2026-10-11 fuzz run deserializer_diff fuzz/corpus/deserializer_diff` SHALL require Loki and stock to agree on decode success, rejection and panic classification for all bounded modes, including proving-key read/write; the compatibility surface is `src/plonk/mod.rs:111-135,368-415`, `src/poly/kzg/params.rs:153-162`, and `fuzz/diff/src/parse.rs:101-211`.

### Requirement: Transcript encoding and challenge behavior is differentially fuzzed

Transcript checks SHALL compare read/write framing, challenge bytes and `assert_empty` outcomes without imposing a decoder-level trailing-byte rule. Existing transcript operations are defined at `src/transcript/mod.rs:47-79,96-149`.

#### Scenario: Transcript bytes and challenge vectors
- **WHEN** transcript vectors and bounded mutations are replayed
- **THEN** `cargo +nightly-2026-10-11 fuzz run transcript_diff fuzz/corpus/transcript_diff` SHALL compare Loki and stock challenge bytes and explicit `assert_empty` results against `loki-harness/fixtures/v1/transcripts/<transcript_vector_id>.jsonl`; trailing-byte wrapper policy is distinguished from the stock decoder API at `src/transcript/mod.rs:47-79,96-149`.

### Requirement: Proof fuzz inputs exercise structured semantics

Proof-roundtrip and soundness fuzz inputs SHALL control structured circuit shape, proof count, witness values and mutation selection, so campaign progress does not depend on short inputs serving only as random seeds. The current input stream is defined at `fuzz/diff/src/lib.rs:21-40`; proof entry points are at `fuzz/diff/src/proof.rs:349-355,399-417` and shape generation is at `fuzz/diff/src/circuit.rs:238-261`.

#### Scenario: Structured proof and mutation branches are reached
- **WHEN** `cargo +nightly-2026-10-11 fuzz run proof_roundtrip fuzz/corpus/proof_roundtrip -- -max_total_time=600 -len_control=0` and the equivalent `soundness` run execute
- **THEN** `cargo +stable test --release --locked --manifest-path fuzz/diff/Cargo.toml --test target_cases` SHALL assert deterministic seeds reach every named circuit-shape/proof-count case and each of the six soundness mutation kinds, while the campaigns report coverage beyond the recorded 4–6 byte plateau; the previous limitation is `LOKI-FUZZ.md:105-112`.

### Requirement: MSM, NTT and FFT outputs are checked against independent definitions

Arithmetic targets SHALL compare Loki and stock outputs and use a naive definition on supported small inputs, including boundary and degenerate cases. Existing MSM and NTT logic is in `fuzz/diff/src/msm.rs:1` and `fuzz/diff/src/ntt.rs:1`; the domain API starts at `src/poly/domain.rs:35-63`.

#### Scenario: Arithmetic boundary and seeded corpus replay
- **WHEN** the CPU arithmetic corpora and bounded generated inputs are run
- **THEN** `cargo +nightly-2026-10-11 fuzz run msm_diff fuzz/corpus/msm_diff`, `cargo +nightly-2026-10-11 fuzz run ntt_diff fuzz/corpus/ntt_diff`, and `cargo +nightly-2026-10-11 fuzz run fft_diff fuzz/corpus/fft_diff` SHALL compare Loki and stock MSM for n ≤ 4096, compare MSM to a naive sum for n ≤ 1024, and compare transforms to stock for extended size ≤ 2^18 and a naive DFT for extended size ≤ 256, including direction, roots, cosets and inverse scaling; existing baseline is `fuzz/diff/src/msm.rs:1`, `fuzz/diff/src/ntt.rs:1`, `src/poly/domain.rs:35-63`, and `LOKI-FUZZ.md:31-51`.

### Requirement: Multiopen behavior is differentially fuzzed

Multiopen targets SHALL generate query sets with k ≤ 10, at most 16 queries and at most 1024 polynomial coefficients, and compare Loki and stock proof creation, verification, transcript consumption, per-set q-polynomials and combined f-polynomial against a serial reference. Existing optimized multiopen operations are at `src/poly/kzg/multiopen.rs:13-24,100-149`.

#### Scenario: Multiopen query partitions and edge counts
- **WHEN** generated query sets include empty, repeated, disjoint and boundary-sized point sets
- **THEN** `cargo +nightly-2026-10-11 fuzz run multiopen_diff fuzz/corpus/multiopen_diff` and `fuzz/quick.sh` SHALL preserve stock 0.8.3 accept/reject and cross-verify outcomes and compare q-polynomials and the combined f-polynomial against the serial reference within those limits; verification is based on `src/poly/kzg/multiopen.rs:13-24,100-149`.

### Requirement: Lookup behavior is differentially fuzzed

Lookup targets SHALL cover valid and invalid bounded lookup circuits and compare proof acceptance, rejection and transcript outcomes with stock 0.8.3. Lookup argument construction is defined at `src/plonk/lookup.rs:10-36` and `src/plonk/lookup/prover.rs:44-75`.

#### Scenario: Valid and invalid lookup witnesses
- **WHEN** lookup corpus cases include in-table values, out-of-table values, boundary rows and mutated witnesses
- **THEN** `cargo +nightly-2026-10-11 fuzz run lookup_diff fuzz/corpus/lookup_diff` and `fuzz/quick.sh` SHALL require matching stock/Loki `MockProver` classifications, both verifiers to accept when MockProver accepts, and both verifiers to reject when MockProver rejects; source baseline is `src/plonk/lookup/prover.rs:44-75` and current witness oracle is `fuzz/diff/src/mutation.rs:1`.

### Requirement: Native deterministic regression replay has no GPU dependency

The deterministic component corpus and regression checks SHALL be runnable with CPU-only features and shall not require native GPU libraries or devices. Current GPU and CPU feature boundaries are declared at `fuzz/Cargo.toml:15-20` and the quick CPU matrix at `fuzz/quick.sh:11-17`.

#### Scenario: CPU-only deterministic replay
- **WHEN** the native deterministic regression suite runs on a runner with CUDA hidden
- **THEN** `fuzz/quick.sh` SHALL replay all CPU cases without enabling `cuda` or `cuda-ntt`; the host-only target boundary is recorded at `fuzz/Cargo.toml:15-20`.

### Requirement: Platform runtime gates replay shared corpus cases

WASM and mobile compatibility lanes SHALL replay deterministic named corpus cases and the target-neutral compatibility fixtures with stock 0.8.3 and Loki executing on the target. Current native-only fixture replay is at `fuzz/diff/tests/quick.rs:54-98` and native stock/Loki cross-verification is at `loki-harness/src/lib.rs:87-194`.

#### Scenario: Browser, Node and mobile target replay
- **WHEN** browser/Node and iOS/Android owners run their required single-thread and threaded/emulator lanes
- **THEN** `wasm:fixtures` and `mobile-compat/replay-corpus.sh --target <triple> --commit <sha>` SHALL replay the named case IDs against the shared `loki-harness/fixtures/v1/manifest.jsonl`, with both stock 0.8.3 and Loki built and executed on target by `wasm:stock-compat` or the mobile runner; native-only `fuzz/diff/tests/quick.rs:54-98` is the source baseline, and libFuzzer remains excluded from those target binaries.

### Requirement: Every target has deterministic case and coverage assertions

Each registered target SHALL have stable named cases that assert its required positive, negative and boundary behavior, and the CI inventory SHALL fail if a registered target has no case mapping. Existing named regression cases are at `fuzz/diff/tests/cases.rs:24-192`; proptest and corpus replay dispatch are at `fuzz/diff/tests/quick.rs:39-98`.

#### Scenario: Target inventory is completely covered
- **WHEN** the deterministic suite enumerates registered targets and their required case IDs
- **THEN** `cargo +stable test --release --locked --manifest-path fuzz/diff/Cargo.toml --test target_cases` SHALL pass with every target mapped to at least three named regression cases (one positive, one negative, one boundary) and each case asserting its oracle outcome; `soundness` SHALL additionally map all six mutation kinds, and MSM cases SHALL include sizes 16,383, 16,384 and 16,385 as CPU-only dispatch-threshold inputs; the existing case registry is `fuzz/diff/tests/cases.rs:24-192` and GPU native positive-dispatch verification remains area 5's `scripts/verify-main.sh --tier native` gate.
