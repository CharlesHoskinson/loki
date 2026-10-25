# Spec Delta

## Purpose

Defines how GPU operations are admitted and composed across recursive proving while preserving the stock proof order and the independent verifier contract.

## ADDED Requirements

### Requirement: GPU proof admission is bounded and dependency-safe
The proof scheduler SHALL admit only ready proof-DAG work with a current healthy device generation and an immediate reservation for its accounted resources; queued jobs MUST NOT hold large SRS leases or witness buffers.

P7 release status remains provisional until the reviewed reservation design is frozen and admission tokens are atomically connected to allocation ownership (or an accepted scoped proof budget); P2 snapshot reservations alone do not establish that guarantee.

#### Scenario: Admit work within the single-heavy policy
- **WHEN** one ready heavy proof has reserved its required device budget
- **THEN** it may run while other heavy jobs wait without holding resident leases or large buffers, and all allocation high-water remains within the configured hard budget. Verification: after Task 4.1 creates the planned test, `cd loki-gpu && cargo test -p loki-gpu --test scheduler_contract`; source artifact: `loki-gpu/design/PLAN.md:130-142` (planned scheduler integration; Loki implementation is on unmerged branch `loki/p7-chain`) and `loki-gpu/crates/loki-gpu/src/runtime.rs:439-448`.

#### Scenario: Reject stale or over-budget admission
- **WHEN** a reservation is stale, generation-mismatched, or exceeds available accounted bytes
- **THEN** the scheduler rejects or queues the job before device work and publishes no partial proof artifact. Verification: after Task 4.1 creates the planned test, `cd loki-gpu && cargo test -p loki-gpu --test scheduler_contract`, plus the Quint MBT replay owned by area 7; source artifact: `loki-gpu/design/PLAN.md:130-142` (planned; Loki implementation on unmerged branch `loki/p7-chain`) and `loki-gpu/design/RUNTIME.md:160-177`.

#### Scenario: Freeze scheduler reservation ownership before release
- **WHEN** the P7 scheduler is proposed for release qualification
- **THEN** its review records how an admitted reservation is atomically redeemed by allocation/launch or bounded by an accepted scoped proof budget, and all identified open refinements are either resolved or explicitly excluded from the shipped contract. Verification: Task 4.1 review gate plus the generated `scheduler_contract` runtime test and area-7 Rust MBT replay; source artifact: `loki-gpu/design/REVIEW.md:120-124` and `loki-gpu/design/PLAN.md:130-142`.

### Requirement: Cancellation and completion preserve proof order
The scheduler SHALL preserve each proof's stock transcript and event order, and cancellation MUST terminate that proof without retrying it on CPU.

#### Scenario: Complete independent work out of order
- **WHEN** independent ready jobs finish in a different order from submission
- **THEN** each proof's transcript events and output slots remain in stock order and no cross-proof buffer is aliased. Verification: after Task 4.1 creates the planned test, `cd loki-gpu && cargo test -p loki-gpu --test scheduler_contract` and area-7 Quint MBT replay; source artifact: `src/plonk/prover.rs:397-810` and `loki-gpu/design/PLAN.md:130-142` (planned; Loki integration on unmerged branch `loki/p7-chain`).

#### Scenario: Cancel an in-flight proof
- **WHEN** cancellation occurs before or after a device operation is enqueued
- **THEN** the proof is terminal, publishes no output, releases only resources whose completion is known, and is never transparently CPU-retried. Verification: after Task 4.1 creates the planned test, `cd loki-gpu && cargo test -p loki-gpu --test scheduler_contract`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:25-35,424-437` and `loki-gpu/design/RUNTIME.md:257-261`.

### Requirement: Recursive proofs interoperate with stock 0.8.3
The GPU-enabled Loki path SHALL preserve the stock API/default path and proof semantics, and its generated proofs, verifying keys, SRS bytes, and supported key serialization MUST interoperate with an unmodified `midnight-proofs` 0.8.3 verifier in both directions.

#### Scenario: Cross-verify a fixed-order GPU fixture
- **WHEN** a pinned deterministic W2 fixture runs with the GPU backend required and fallback forbidden
- **THEN** stock 0.8.3 accepts the Loki proof, Loki accepts the stock proof, VK/PK/verifier-parameter/SRS bytes match, and the test records positive dispatch counts with zero fallback. Verification: after Task 4.3 adds the GPU-required W2 cases, `RAYON_NUM_THREADS=1 cargo test --release --locked --manifest-path loki-harness/Cargo.toml --features loki-gpu-backend -- --test-threads=1`; source artifact: `loki-harness/README.md:3-19,21-58`, `loki-gpu/design/TEST-PLAN.md:33-45`, and `docs/GPU.md:29-34`.

#### Scenario: Preserve stock default behavior
- **WHEN** Loki is built and tested with its stock default feature set
- **THEN** the backend is absent, no CUDA dependency is activated, and fixed controlled-order proof bytes match the stock 0.8.3 fixture; lookup byte equality is claimed only for controlled map order. Verification: `cargo test --test backend_default_stock` and `cargo tree -e features`; source artifact: `Cargo.toml:45-74`, `src/backend/dispatch.rs:259-270,272-286`, and `loki-gpu/design/TEST-PLAN.md:15-17`.

### Requirement: Complete-chain GPU qualification reports attributable results
The release qualification SHALL measure the same pinned leaf, parent, and decider cohort on CPU and GPU with repeated matched runs, disclose every fallback and resource high-water, and MUST NOT claim a speedup from kernel-only or isolated-phase timings.

#### Scenario: Report the full recursion chain
- **WHEN** CPU and GPU candidate runs complete for a pinned Sherlock cohort
- **THEN** the report includes leaf, parent, and decider acceptance, independent stock verification, top-level chain time, nonoverlapping phases, features, hardware/toolchain/source pins, sample count/spread, dispatch/fallback counts, and peak host/device memory. Verification: run the command and report format owned by the perf benchmark change, then attach the generated report to the GPU release receipt; source artifact: `loki-gpu/design/TEST-PLAN.md:107-115` and `docs/GPU.md:230-239,576-580`.

### Requirement: Lean coverage distinguishes scheduler contracts from runtime assumptions
The release Lean coverage ledger SHALL name the P7 scheduler admission, cancellation, and transcript-order contracts and distinguish proved abstract properties from Rust, CUDA, and operating-system assumptions; it MUST NOT claim that a model alone proves native scheduler execution.

#### Scenario: Audit the P7 release coverage row
- **WHEN** the P7 scheduler is included in the release source manifest
- **THEN** the Lean coverage ledger links its abstract state/order contracts to Rust MBT and native fault evidence, lists every remaining external assumption, and the release project has no sorry/admit/sorryAx or new custom axioms. Verification: `lake build LokiLean.Backend LokiLean.Refinement`, `lake exe axiom-audit --allow propext,Classical.choice,Quot.sound`, and `lake env lean scripts/AxiomSweep.lean`; source artifact: `loki-gpu/design/TEST-PLAN.md:93-105` and `loki-gpu/design/PLAN.md:130-142` (planned P7; Loki integration is on unmerged branch `loki/p7-chain`).

