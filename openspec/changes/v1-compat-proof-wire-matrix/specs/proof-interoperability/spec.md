# Proof Interoperability

## Purpose

Define replayable proof, key, transcript, instance, and batch compatibility with unmodified stock `midnight-proofs` 0.8.3.

## ADDED Requirements

### Requirement: Proofs SHALL verify across Loki and stock 0.8.3

For each supported fixture, Loki SHALL verify stock 0.8.3 proofs and stock 0.8.3 SHALL verify Loki proofs.

#### Scenario: Bidirectional proof verification

- **WHEN** the serial W2 gate runs `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential -- --test-threads=1` against pinned stock 0.8.3
- **THEN** every supported fixture passes in both verifier directions; verification is the named differential gate, building on `loki-harness/src/lib.rs:87-115` and `loki-harness/src/backend.rs:198-299`

### Requirement: VK, PK, and verifier parameter encodings SHALL interoperate

Loki and stock SHALL read each other's serialized SRS, VK, PK, and verifier parameter bytes through existing public APIs. Independently generated VK bytes for identical circuit/configuration inputs SHALL be equal. No new wire encoding SHALL be introduced.

#### Scenario: Cross-backend key and parameter round trips

- **WHEN** the W2 serialization gate reads stock-written bytes in Loki and Loki-written bytes in stock using the declared `SerdeFormat`
- **THEN** both backends load corresponding SRS/VK/PK/verifier parameters, VK bytes match, and checked malformed/truncated cases fail; verification is the named serialization gate, building on `loki-harness/src/backend.rs:198-299`, `loki-harness/src/lib.rs:98-103`, and stock `ProvingKey::{bytes_length,write,read}` at `midnight-proofs-0.8.3/src/plonk/mod.rs:359-431`

### Requirement: Serialization behavior SHALL preserve each stock SerdeFormat contract

The matrix SHALL compare bytes and round trips for every supported `SerdeFormat`, including `RawBytesUnchecked`. Checked `Processed` and `RawBytes` decoders SHALL reject malformed or truncated encodings; unchecked malformed-input behavior SHALL preserve stock API behavior.

#### Scenario: Format-specific checked and unchecked decoding

- **WHEN** the serialization corpus runs its `Processed`, `RawBytes`, and `RawBytesUnchecked` cases through both backends
- **THEN** emitted bytes and round trips match across implementations, checked invalid encodings reject, and unchecked results match stock behavior; verification is the named SerdeFormat corpus, building on `loki-harness/src/backend.rs:202-215`, `loki-harness/src/backend.rs:274-289`, and stock `ProvingKey::read` at `midnight-proofs-0.8.3/src/plonk/mod.rs:405-427`

### Requirement: Transcript events and challenges SHALL match stock 0.8.3

For equal parameters, circuit, instances, prover randomness, lookup-placement order, and worker schedule, Loki SHALL emit the same transcript events in the same order and derive equal challenges as stock 0.8.3.

#### Scenario: Stock transcript vector equality

- **WHEN** `cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential transcript_vectors_match_stock -- --test-threads=1` compares stock-generated vectors under matched lookup-placement order and worker schedule
- **THEN** Loki events and challenges match the stock vectors; verification is the named Rust vector test, building on `src/transcript/mod.rs:47-76` and `loki-harness/src/backend.rs:237-249`

### Requirement: Randomized lookup order SHALL be judged by acceptance

For randomized lookup-placement order, the gate SHALL compare proof acceptance across stock and Loki without claiming equal transcript challenge or proof bytes.

#### Scenario: Lookup proofs with differing order

- **WHEN** the fixed-seed lookup determinism gate processes a fixture marked with randomized lookup-placement order
- **THEN** the result records byte differences and still requires cross-verification and VK identity; verification is `RAYON_NUM_THREADS=1 ./loki-harness/run.sh 10` plus the lookup determinism report, building on `loki-harness/README.md:31-60` and `loki-harness/src/lib.rs:148-153`

### Requirement: Appended proof bytes SHALL preserve the stock API contract

The gate SHALL record stock API behavior for appended proof bytes and SHALL preserve it. An additional full-consumption wrapper check SHALL be identified as wrapper behavior.

#### Scenario: Appended-byte classification

- **WHEN** `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential -- --test-threads=1` runs the appended-byte differential control
- **THEN** each stock/Loki API pair has matching acceptance behavior and any stricter harness result is labeled wrapper policy; verification is the named differential test, building on `loki-harness/src/lib.rs:135-146` and `loki-harness/src/backend.rs:292-303`

### Requirement: W2 SHALL cover the declared circuit and k matrix

The release W2 matrix SHALL cover both existing `Fixture<false>` and `Fixture<true>` forms at k=10, 14, and 17. These are the only declared circuit forms: the dense arithmetic/challenge circuit without and with the 8-bit lookup.

#### Scenario: Circuit and size result rows

- **WHEN** `RAYON_NUM_THREADS=1 ./loki-harness/run.sh 10 14 17` runs against the pinned 0.8.3 oracle
- **THEN** each `Fixture<false>` and `Fixture<true>` case has a machine-readable row; verification is the named W2 gate, building on `loki-harness/README.md:1-10`, `loki-harness/src/backend.rs:45-111`, and `loki-harness/tests/differential.rs:3-27`

### Requirement: W2 SHALL classify plain, committed, and batched instances

For the existing two-instance-column batch fixture, the W2 matrix SHALL include one, two, and three proofs; zero, one, and two committed columns; and plain, committed, single-mixed, and multi-proof-mixed shapes.

#### Scenario: Instance and batch shape rows

- **WHEN** `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential -- --test-threads=1` runs the committed/plain batch cases using the pinned 0.8.3 oracle
- **THEN** every listed shape has a result row recording instance mode, committed-column count, proof count, both verifier outcomes, and rejection classification; verification is the named W2 batch gate, building on `loki-harness/src/backend.rs:73-111` and `LOKI-QUINT.md:151-166`

### Requirement: T03 mixed multi-proof behavior SHALL match stock

The matrix SHALL retain the known mixed committed/plain multi-proof ordering defect as an expected stock outcome. Loki SHALL match stock acceptance or rejection unless a separately approved wire-compatible protocol decision changes that classification. A stock-rejected case SHALL NOT be documented as supported.

#### Scenario: T03 counterexample and real batch replay

- **WHEN** `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential -- --test-threads=1`, `CARGO_TARGET_DIR=target/quint-mbt cargo test --locked --manifest-path quint/mbt/Cargo.toml diagnose_stock_mixed_instance_order -- --nocapture`, and `python3 quint/check_instances.py` run the T03 cases
- **THEN** the result records stock and Loki classifications, transcript divergence index, and stable T03 case IDs; verification is the Rust matrix, real Rust MBT and Quint audit, building on `LOKI-QUINT.md:120-166`, `quint/mbt/src/instance_order.rs:15-94`, and `TODO.md:15-17`

### Requirement: Negative controls SHALL classify invalid proofs and keys

Both 0.8.3 verifiers SHALL reject wrong public inputs, empty, mutated, and truncated proofs, malformed checked VK/PK encodings, and unsatisfied witnesses. Both MockProver implementations SHALL agree whether each test witness satisfies the circuit.

#### Scenario: Differential rejection controls

- **WHEN** `RAYON_NUM_THREADS=1 CARGO_BUILD_JOBS=4 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --test differential reject_broken_constraints -- --test-threads=1` and the 0.8.3 key/proof mutation corpus run
- **THEN** proof/key and MockProver outcomes match stock 0.8.3; verification is the differential negative-control test and corpus replay, building on `loki-harness/src/lib.rs:116-146`, `loki-harness/src/backend.rs:306-324`, and `loki-harness/tests/differential.rs:25-32`

### Requirement: Determinism SHALL separate no-lookup equality from lookup randomness

With fixed seeds and one Rayon worker, no-lookup proofs SHALL be byte-identical. Lookup byte differences SHALL be reported but SHALL NOT alone fail interoperability; lookup cases SHALL still pass cross-verification and VK identity checks.

#### Scenario: Fixed-seed determinism gate

- **WHEN** `RAYON_NUM_THREADS=1 ./loki-harness/run.sh 10 14 17` runs the deterministic controls
- **THEN** no-lookup bytes match and lookup cases report lengths and first difference while preserving verification/VK checks; `LOKI_STRICT_LOOKUP_BYTES=1 ./loki-harness/run.sh 10` SHALL fail on any lookup byte difference; verification is the named determinism gate, building on `loki-harness/src/lib.rs:148-153` and `loki-harness/src/lib.rs:171-191`

### Requirement: W2 SHALL publish one canonical fixture manifest

Compat-proof SHALL publish `loki-harness/fixtures/v1/manifest.jsonl`. Each row SHALL identify schema version, fixture ID, stock version/source pin, Loki revision, circuit ID, k, instance mode, batch shape, `SerdeFormat`, transcript mode, lookup-order mode, worker schedule, feature set, seed ID, expected outcome, transcript vector ID, artifact paths, and SHA-256 digests.

#### Scenario: Shared manifest and transcript replay

- **WHEN** `python3 loki-harness/fixtures/v1/validate_manifest.py` reads `loki-harness/fixtures/v1/manifest.jsonl`
- **THEN** every case has all required fields, including the lookup-order mode and worker schedule, and verifies its referenced artifact digests; verification is the manifest schema test, extending the result structures at `loki-harness/src/lib.rs:22-78` and building on proof/key generation at `loki-harness/src/backend.rs:198-299`

### Requirement: Canonical fixture artifacts SHALL cover proof and key inputs

The fixture bundle SHALL include serialized SRS/verifier parameters, VK, PK, proof, and public inputs, plus named wrong-input, malformed/truncated, mutated, and appended variants. Transcript vectors SHALL record ordered event identities/bytes and challenge labels/bytes without witness secrets.

#### Scenario: Fixture bundle includes required artifacts

- **WHEN** `python3 loki-harness/fixtures/v1/validate_manifest.py` validates each fixture bundle and transcript vector
- **THEN** each artifact path and transcript event is present, digest-checked, and contains no witness secret; verification is the fixture schema/corpus test, building on `loki-harness/src/backend.rs:198-299` and `src/transcript/mod.rs:47-76`

### Requirement: QA consumers SHALL replay canonical fixture IDs

Differential fuzzing, Quint/Rust MBT, Lean, WASM/mobile, and performance QA SHALL consume canonical manifest and transcript vector IDs rather than define competing fixture formats.

#### Scenario: Shared manifest and transcript replay

- **WHEN** `fuzz/quick.sh`, `python3 quint/check_instances.py`, the Lean release check, `wasm:stock-compat`, `mobile-compat/run.sh --platform ios --runtime simulator`, `mobile-compat/run.sh --platform android --runtime emulator`, and the performance replay consume `loki-harness/fixtures/v1/manifest.jsonl`
- **THEN** consumers resolve identical case and transcript vector IDs and verify artifact digests; verification is each consumer replay gate, extending result structures at `loki-harness/src/lib.rs:22-78` and building on proof/key generation at `loki-harness/src/backend.rs:198-299` and MBT evidence at `LOKI-QUINT.md:168-175`

### Requirement: CPU compatibility SHALL run on WASM and mobile targets

The canonical fixture bundle and CPU W2 gate SHALL require no GPU and SHALL run with both unmodified stock 0.8.3 and Loki verifiers on target in both proof directions. Required targets are WASM browser and Node in single-threaded and wasm-threads modes, `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and `aarch64-linux-android`. Native stock cross-verification SHALL be additional evidence. The 0.8.1 and k20 T04 lane is specified in `v1-compat-proof-stock-matrix`.

#### Scenario: Target-native stock and Loki replay

- **WHEN** `wasm:stock-compat`, `mobile-compat/run.sh --platform ios --runtime simulator`, `mobile-compat/run.sh --platform ios --runtime device`, `mobile-compat/run.sh --platform android --runtime emulator`, and `mobile-compat/run.sh --platform android --runtime device` execute the shared fixtures on each required target
- **THEN** stock and Loki both run on target, each case records target/runtime/backend/version, and no GPU feature is required; verification is the target CPU W2 jobs, building on `loki-harness/Cargo.toml:9-20` and `loki-harness/src/backend.rs:62-111`

### Requirement: Release compatibility evidence SHALL be replayable and documented

Each W2 result set SHALL record Loki and stock revisions, lockfiles, compiler/target, features, commands, seeds, and machine-readable outcomes. Release documentation SHALL publish the supported matrix, T03 limitation, version pins, determinism policy, negative controls, and runnable commands. Unrun gates SHALL be labeled unverified.

#### Scenario: Release reviewer replays W2

- **WHEN** a release reviewer runs the documented commands against the archived manifest and pins
- **THEN** the commands reproduce recorded classifications and missing target runs are reported as unverified; verification is the W2 replay command and documentation review, building on `loki-harness/README.md:1-10`, `loki-harness/README.md:31-60`, and `TODO.md:15-21`
