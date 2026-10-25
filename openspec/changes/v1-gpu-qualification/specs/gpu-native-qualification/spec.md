# Spec Delta

## Purpose

Defines reproducible CPU merge and native GPU receipt gates so implementation review can proceed without claiming unrun device evidence or blocking merges on a shared GPU lock.

## ADDED Requirements

### Requirement: CPU merge qualification does not require a native GPU
The merge gate SHALL run source, API, fake-driver, default-feature, and CPU compatibility checks without requiring a GPU device, GPU lock, or native receipt.

#### Scenario: Qualify a GPU change for merge
- **WHEN** a GPU feature change is proposed for merge
- **THEN** `scripts/verify-main.sh --tier cpu` and fake-driver checks pass without acquiring `gpu.lock`, and the merge record identifies the checked source revision. Verification: `scripts/verify-main.sh --tier cpu`; source artifact: `docs/MERGE-GATES.md:5-13` and `loki-gpu/design/PLAN.md:170-174`.

### Requirement: Native receipts are post-merge and revision-bound
Each shipped GPU operation SHALL produce a native receipt against the exact merged source revision; a native receipt failure MUST block release or push promotion, but MUST NOT retroactively block the CPU-qualified merge.

#### Scenario: Record native evidence for a merged revision
- **WHEN** the post-merge native tier runs on a supported device
- **THEN** the receipt records the merged source revision, hardware/toolkit matrix, sanitizer output, operation dispatch counts, CPU/GPU differential result, stock 0.8.3 acceptance, and resource high-water. Verification: `scripts/verify-main.sh --tier native` and the package receipt check in `deps.yaml`; source artifact: `docs/MERGE-GATES.md:8-20` and `loki-gpu/design/TEST-PLAN.md:19-45`.

#### Scenario: Native qualification fails
- **WHEN** a sanitizer, device, differential, stock verifier, memory, or dispatch gate fails for a merged revision
- **THEN** the receipt is marked failed and release/push promotion is blocked until a corrected revision passes the same gate. Verification: `scripts/verify-main.sh --tier native`; source artifact: `docs/MERGE-GATES.md:9-20`.

### Requirement: GPU-required tests detect hidden CPU fallback
GPU-required tests SHALL assert a successful dispatch for each expected operation and MUST fail if any operation silently falls back to CPU.

#### Scenario: Run the W2 GPU fixture
- **WHEN** the W2 harness executes a GPU-required fixture on a qualified device
- **THEN** it records nonzero expected backend completion counts and zero unexpected fallback, in addition to bidirectional stock 0.8.3 verification. Verification: after Task 4.3 adds GPU-required W2 cases, `RAYON_NUM_THREADS=1 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --features loki-gpu-backend -- --test-threads=1`; source artifact: `loki-gpu/design/TEST-PLAN.md:33-45,68-79` and `src/backend/dispatch.rs:90-99,128-159`.

### Requirement: Native qualification covers memory and asynchronous safety
The native receipt suite SHALL exercise allocation, transfer, launch, synchronization, context-loss, cancellation, and stale-generation failures, and MUST confirm all resources stay within the declared device, pinned-host, checkpoint, and SRS budgets.

#### Scenario: Inject device and memory failures
- **WHEN** fault injection triggers at any asynchronous operation boundary or an allocation exceeds budget
- **THEN** sanitizer and runtime tests show no invalid access, use-after-free, leak of unpublished handles, or partial result, and the operation follows the documented recoverable or terminal failure policy. Verification: `scripts/verify-main.sh --tier native` runs `cd loki-gpu && cargo test -p loki-gpu --test runtime_contract`, the scheduler test created by core Task 4.1, and Compute Sanitizer memcheck/racecheck on native test binaries; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:25-47,64-100,398-437` and `loki-gpu/design/TEST-PLAN.md:23-30`.

