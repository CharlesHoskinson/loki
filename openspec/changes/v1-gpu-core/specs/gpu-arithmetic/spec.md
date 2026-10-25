# Spec Delta

## Purpose

Defines the exact arithmetic and SRS-residency behavior that the opt-in GPU backend must provide before Loki may rely on it for proof generation.

## ADDED Requirements

### Requirement: SRS cache hits are content-exact and lease-safe
The GPU SRS registry SHALL bind each resident entry to validated point contents, requested prefix, basis, arithmetic wire version, device generation, and accounted bytes; it MUST NOT evict an entry while a lease is active.

#### Scenario: Reuse an unchanged validated prefix
- **WHEN** a caller acquires the same immutable SRS prefix in the same healthy device generation
- **THEN** the registry reuses only an entry whose complete requested point prefix and metadata match and whose persistent and scratch accounting fit their budgets. Verification: after Task 2.1 creates the planned target, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test srs_registry`; source artifact: `loki-gpu/crates/loki-gpu/src/srs.rs:1-20,27-70` and `loki-gpu/design/PLAN.md:75-87`.

#### Scenario: Reject changed contents and stale generations
- **WHEN** a caller mutates a source point at the same address, supplies an invalid point, or reuses a lease after device-generation change
- **THEN** the registry rejects the hit or use before dispatch, retains no stale success, and leaves caller-visible inputs unchanged. Verification: after Tasks 2.1–2.2 create the planned targets, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test srs_registry` and `cargo fuzz run residency_state`; source artifact: `loki-gpu/crates/loki-gpu/src/srs.rs:3-5,18-20` and `loki-gpu/design/RUNTIME.md:50-59,81-92`.

### Requirement: NTT and coset operations equal the stock CPU operation
The GPU transform path SHALL produce the same field elements in the same order as the stock CPU operation for every advertised direction, domain, coset conversion, and supported size.

#### Scenario: Evaluate a supported transform
- **WHEN** an advertised forward, inverse, resident, or coset transform receives a valid request
- **THEN** every output element equals the CPU result, the domain root and natural order match, and inverse normalization is applied exactly once. Verification: after Task 1.2 creates the planned targets, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test ntt_differential` and `cargo test -p loki-gpu --features cuda --test resident_ntt`; source artifact: `loki-gpu/crates/loki-gpu/src/ntt.rs:1-36`, `src/backend/dispatch.rs:161-224`, and `loki-gpu/design/PLAN.md:89-100`.

#### Scenario: Reject a malformed transform request
- **WHEN** a transform has an invalid length, root, order, domain, or stale resident handle
- **THEN** it returns a typed error before publishing an output and the original host vector remains unchanged. Verification: after Task 1.2 creates the planned targets, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test ntt_differential` and `cargo test -p loki-gpu --features cuda --test resident_ntt`; source artifact: `src/backend/dispatch.rs:161-224` and `loki-gpu/crates/loki-gpu/src/ntt.rs:1-36`.

### Requirement: Quotient evaluation preserves the stock ordered numerator
The quotient compiler and interpreter SHALL preserve the stock evaluator's expression order, rotations, constants, challenge snapshot, and output lengths, and MUST return exactly the stock numerator values before the existing vanishing-polynomial division.

#### Scenario: Evaluate a compiled proof numerator
- **WHEN** a valid supported PLONK expression graph is compiled and evaluated
- **THEN** each numerator element and chunk boundary equals the CPU evaluator result before the existing prover tail performs vanishing-polynomial division, without changing transcript state. Verification: after Task 3.2 creates the planned target, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test quotient_differential`; in the gpu-spec worktree run `lake build LokiLean.BackendQuotient LokiLean.Refinement`, `lake exe axiom-audit --allow propext,Classical.choice,Quot.sound`, and `lake env lean scripts/AxiomSweep.lean`, with no `sorry`, `admit`, `sorryAx`, or new custom axioms in release Lean sources. Source artifact: `loki-gpu/crates/loki-gpu/src/quotient.rs:1-20`, `src/plonk/evaluation.rs:305-577`, `src/plonk/prover.rs:667-730`, `loki-gpu/design/PLAN.md:102-108`, and `loki-lean-wt/gpu-spec/LokiLean/BackendQuotient.lean:176-182,387`.

#### Scenario: Fail closed on invalid bytecode
- **WHEN** bytecode contains an unknown opcode, out-of-range register or column, invalid rotation, or missing challenge input
- **THEN** validation rejects the program before kernel dispatch and no partial numerator is published. Verification: after Task 3.1 creates the planned target, `cd loki-gpu && cargo test -p loki-gpu --features cuda --test quotient_validation`; run the Lean build and axiom audit in Task 3.2. Source artifact: `loki-gpu/crates/loki-gpu/src/quotient.rs:1-20` and `loki-gpu/design/QUOTIENT.md:20-46`.

### Requirement: GPU arithmetic failures are transactional
Every GPU operation SHALL either publish a fully validated result after successful completion or return an error with caller-visible inputs and outputs unchanged.

#### Scenario: Recover from an accelerator error with host inputs retained
- **WHEN** allocation, transfer, launch, synchronization, or device-context failure occurs while the complete original host inputs remain available
- **THEN** the GPU publishes no partial output, Loki may recompute using the CPU path, and a poisoned generation is disabled before subsequent dispatch. Verification: `cargo test -p loki-gpu --test runtime_contract` and Loki `cargo test --test backend_cpu_equivalence`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:25-47,424-437` and `src/backend/dispatch.rs:107-125,128-159`.

#### Scenario: Abort when data is device-only or the request is cancelled
- **WHEN** context loss makes device-only data unrecoverable or a proof job is cancelled
- **THEN** the current proof aborts without CPU retry and without transcript or artifact publication. Verification: `cargo test -p loki-gpu --test runtime_contract` and `cargo test -p loki-gpu --test scheduler_contract`; source artifact: `loki-gpu/crates/loki-gpu/src/runtime.rs:424-437` and `loki-gpu/design/RUNTIME.md:81-92`.

### Requirement: P3 Lean coverage separates shipped MSM dispatch from P6 research
The release Lean source set SHALL prove or explicitly discharge the non-segmented MSM dispatch obligations used by P3, and MUST exclude the unfinished segmented-partition theorem/source from the release closure while P6 remains deferred; the global no-hole/no-new-custom-axiom rule still applies.

#### Scenario: Audit MSM obligations at the release source manifest
- **WHEN** P3 SRS/dispatch is included and P6 segmented MSM remains out of scope
- **THEN** release coverage identifies the shipped non-segmented theorem and its source correspondence, while no `sorry`, `admit`, `sorryAx`, or new custom axiom from the deferred segmented research source enters the release Lean project. Verification: `lake build LokiLean.BackendMsm LokiLean.BackendSrs`, `lake exe axiom-audit --allow propext,Classical.choice,Quot.sound`, and `lake env lean scripts/AxiomSweep.lean`; source artifact: `loki-lean-wt/gpu-spec/LokiLean/BackendMsm.lean:270-282` and `loki-gpu/design/PLAN.md:116-128`.
