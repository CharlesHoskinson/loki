# Proposal

## Why

The opt-in `loki-gpu` runtime is integrated, but its P3–P5 arithmetic interfaces still return `Unsupported`, and P7 whole-chain scheduling is not complete. Completing those pieces behind explicit features can accelerate the actual Sherlock recursion workload while preserving the CPU implementation as the compatibility reference.

## What Changes

- Finish validated SRS residency (P3), exact resident NTT/coset operations (P4), and ordered quotient compilation/evaluation (P5).
- Integrate those operations with a one-heavy-job scheduler and complete-chain compatibility/measurement gate (P7).
- Require transactional outputs, exact CPU equality, complete retained host checkpoints for CPU fallback, bounded memory reservations, and generation-scoped failure handling.
- Keep P6 balanced MSM, P8 lookup preparation, and P9 concurrency/precompute conditional on proof obligations and measured benefit; they do not block the core v1 path.

## Capabilities

### New Capabilities

- `gpu-arithmetic`: exact, opt-in GPU SRS, NTT/coset, and quotient operations with a CPU reference and safe fallback.
- `gpu-chain`: resource-bounded proof-chain scheduling and qualification for complete recursive proving flows.

### Modified Capabilities

None.

## Impact

The change covers Loki's typed dispatcher and opt-in feature integrations, `loki-gpu` SRS/NTT/quotient/runtime modules, W2 compatibility fixtures, and GPU-specific fuzz, Quint, and Lean contracts. It depends on P1/backend API, the compat-proof stock 0.8.3 fixtures, and W10 coefficient-commit handoff; P4 precedes P5 and P3/P4/P5 precede P7. It does not change stock defaults, transcript/RNG ownership, or proof encodings. WASM and mobile stay CPU-only and must compile without CUDA. GPU behavior remains opt-in and requires GPU/platform documentation and rustdoc updates.
