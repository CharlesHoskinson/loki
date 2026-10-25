# Spec Delta

## Purpose

Defines a supportable CPU-only release contract for Loki on iOS and Android, including compatibility evidence and resource limits measured on phones.

## ADDED Requirements

### Requirement: Mobile CPU targets compile with stock defaults
Loki SHALL compile for iOS device, iOS simulator, and Android AArch64 using the stock default feature set without requiring an accelerator SDK.

#### Scenario: Mobile target build matrix passes at the declared MSRV
- **WHEN** CI runs `cargo +1.76.0 check --locked --manifest-path compat/msrv/Cargo.toml --target aarch64-apple-ios`, the same command for `aarch64-apple-ios-sim` and `aarch64-linux-android`, with the Android NDK configured; iOS jobs run on Apple Silicon macOS, Android cross-compilation runs on a named Linux host, and the Android ARM64 emulator lane runs on ARM64 Linux or a physical device
- **THEN** all three builds complete without enabling CUDA, Metal, Vulkan, or another accelerator feature, and the feature tree plus host architecture is retained; verify in the mobile CI matrix, building on `Cargo.toml:13-16,45-74` and `docs/PLATFORMS.md:9-13`

### Requirement: Mobile runtime gate uses stock 0.8.3
The iOS simulator and Android emulator CPU artifacts SHALL pass an independent stock `midnight-proofs` 0.8.3 compatibility gate in both proof directions, with both implementations using their default `committed-instances` feature and with PK, VK, SRS, verifier-parameter serialization checks, and negative controls.

#### Scenario: Simulator and emulator proof exchange passes in both directions
- **WHEN** `mobile-compat/run.sh --platform ios --runtime simulator` executes on an Apple Silicon macOS host and `mobile-compat/run.sh --platform android --runtime emulator` executes on an ARM64 Linux host against compat-proof-owned fixtures
- **THEN** Loki proves and stock verifies, stock proves and Loki verifies, PK/VK/SRS/verifier-parameter serialization round-trips match, wrong public inputs and malformed proofs are rejected, and logs identify target, host architecture, OS/runtime, fixture revision, stock version, and stock feature set; verify with the mobile compatibility runner, building on `loki-harness/README.md:3-10,49-55`, `loki-harness/Cargo.toml:18-25`, `Cargo.toml:51-52`, and `src/plonk/mod.rs:354-370,372-465 (ProvingKey impls)`

#### Scenario: Physical iOS and Android phones pass compatibility checks
- **WHEN** `mobile-compat/run.sh --platform ios --runtime device` and `mobile-compat/run.sh --platform android --runtime device` execute against the named low- and mid-range handset matrix using the same compat-proof fixtures
- **THEN** both stock/Loki proof directions and serialization/negative controls pass on at least one low-range and one mid-range physical phone per operating system, with default `committed-instances` enabled and OS/device/stock-feature/fixture provenance retained; verify from attached device-runner reports, building on `TODO.md:271-288`, `loki-harness/Cargo.toml:18-25`, and `Cargo.toml:51-52`

### Requirement: Phone resource budgets are numeric and evidence-based
The mobile release SHALL publish per-device peak-RSS, installed-library-size, per-proof-time, and sustained-throughput limits measured on named low- and mid-range phone configurations; an absent threshold or measurement SHALL fail the release gate.

#### Scenario: Baseline freezes numeric limits before qualification
- **WHEN** the owner runs `mobile-compat/measure.sh --mode freeze --device-matrix mobile-compat/devices.toml --proofs 10 --budget mobile-compat/budgets.toml` on the named low- and mid-range physical devices
- **THEN** the command records numeric per-device limits and directions before any release qualification run: maximum peak RSS bytes (`getrusage` maximum resident set, normalized), maximum stripped Loki-linked `loki-mobile-compat-smoke` artifact bytes (artifact type recorded), maximum per-proof wall time in seconds, and minimum sustained throughput as `7 / sum(proof_time_seconds[samples 4 through 10 inclusive, 1-based]) * 60` proofs/minute, together with OS thermal state before and after; freeze evidence alone does not pass release, verified in the versioned budget file and baseline report, building on `TODO.md:271-288` and `docs/PLATFORMS.md:282-284,429-431`

#### Scenario: Frozen resource budgets and soak pass
- **WHEN** a later candidate runs `mobile-compat/measure.sh --mode qualify --device-matrix mobile-compat/devices.toml --proofs 10 --budget mobile-compat/budgets.toml`
- **THEN** each named device has numeric frozen limits and recorded observations for all four metrics, all maximum and minimum comparisons pass in their declared directions, all ten consecutive CPU proofs complete, and missing values or any over-budget result fail; verify by the release device-runner report, building on `TODO.md:287-291` and `docs/PLATFORMS.md:284,429-431`

### Requirement: Mobile CPU release remains independent of GPU features
The mobile CPU support SHALL preserve stock default-feature behavior and SHALL NOT require Metal, Vulkan, CUDA, or a Loki-only accelerator feature to build or pass its compatibility gate.

#### Scenario: Default feature tree has no accelerator requirement
- **WHEN** CI runs `cargo tree --locked --target aarch64-apple-ios -e features` and the equivalent Android command, followed by the target build and compatibility jobs with no explicit Loki accelerator features
- **THEN** the default feature graph and all mobile gates pass without accelerator SDK or runtime dependencies; verify from captured feature-tree and job artifacts, building on `Cargo.toml:45-74` and `src/lib.rs:14-18`

### Requirement: Mobile support limits and binding boundary are documented
The platform guide SHALL document the tested phone/OS matrix, build and runtime commands, measured limits, known scaling ceiling, interruption limitations, and whether app-facing FFI is supported; v1 SHALL make no stable C ABI or UniFFI promise unless separately specified.

#### Scenario: Published platform guide matches release evidence
- **WHEN** maintainers run `cargo doc --no-deps --locked` and `scripts/check-mobile-docs.sh --report mobile-compat/report.json`
- **THEN** rustdoc builds and the checked platform guide names the same supported devices, budgets, SDK/NDK versions, verification commands, unsupported proof sizes, process-interruption behavior, and explicitly deferred FFI/UniFFI boundary as the report; verify against the report, building on `docs/PLATFORMS.md:264-316,422-435`, `src/lib.rs:9,20-30`, and `TODO.md:271-291`

#### Scenario: Host verification supplements target execution
- **WHEN** maintainers run the compat-proof-owned native host verification command on proof artifacts emitted by each phone
- **THEN** host results are attached as supplemental evidence and cannot mark either target runtime gate passed without its simulator/emulator and physical-device result; verify with the release evidence manifest, building on `loki-harness/README.md:3-10,49-55` and `TODO.md:271-288`

### Requirement: Interrupted CPU proofs produce no accepted proof artifact
The mobile runner SHALL treat OS process termination during CPU proving as an incomplete operation and SHALL NOT publish or accept a partial proof artifact; mobile support SHALL NOT promise GPU-style `ContextLost` recovery or resume.

#### Scenario: Memory termination during proving is not accepted as proof output
- **WHEN** `mobile-compat/run.sh --inject-terminate-during-prove` terminates its proof worker before the proof call completes on iOS simulator and Android emulator
- **THEN** the runner records process termination, emits no proof artifact as successful output, and stock 0.8.3 rejects any deliberately retained partial bytes; verify with the fault-injection runner on both targets, building on `src/plonk/prover.rs:397-448`, `loki-harness/README.md:49-55`, and `TODO.md:287-291`

### Requirement: Client-side circuit and witness-data limits are explicit
The mobile release SHALL identify measured client-side candidate circuits and state, for each candidate, which witness data stays on-device and which data may leave the device; unsupported or unmeasured candidates SHALL be marked as not qualified.

#### Scenario: Client-side candidate report is backed by device measurements
- **WHEN** maintainers run `mobile-compat/measure.sh --mode qualify --device-matrix mobile-compat/devices.toml --proofs 10 --budget mobile-compat/budgets.toml --clients mobile-compat/client-candidates.toml`
- **THEN** each candidate row names the circuit/fixture ID, proof shape, device pass/fail, measured RSS and time, local-versus-delegated decision, witness-data egress boundary, and evidence owner; missing measurements or data-boundary decisions fail the release gate, verified in `mobile-compat/report.json`, building on `TODO.md:275-276` and `docs/PLATFORMS.md:287-302`
