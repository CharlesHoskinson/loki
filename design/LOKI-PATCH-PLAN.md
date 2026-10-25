# Loki integration patch plan

**CODE (proposed):** This is the planned `loki-backend-api` integration seam;
no Loki source is modified by this repository. Unless a paragraph is explicitly
labeled SOURCE, MEASURED, ESTIMATE, or OPEN QUESTION, its normative design
prose is a proposed CODE contract, not evidence that the integration has been
implemented. The integration must retain stock 0.8.3 feature-off behavior; an
existing lookup-order drift in `877114d` is listed below as a prerequisite to
that claim.

## Cargo feature boundary

Add an optional path dependency on `loki-backend-api` in Loki's normalized and
original manifests. Keep the default feature set empty with respect to the
backend API and all CUDA crates. Add opt-in features:

| Feature | Dependencies | Role |
|---|---|---|
| `loki-backend-api` | `dep:loki-backend-api` | Enables the backend dispatcher and CPU adapter while retaining the existing stock CPU routines as its oracle. |
| `loki-sppark` | `loki-backend-api`, existing `cuda`, `cuda-ntt` | Selects the current sppark MSM/NTT adapters as the legacy accelerator. |
| `loki-gpu-backend` | `loki-backend-api`, `dep:loki-gpu`, `loki-gpu/cuda` | Selects the new Apache-2.0 crate and explicitly forwards its CUDA build feature. |
| `loki-argument-commit` | `loki-backend-api` | Enables W10's coefficient-basis strategy without selecting a CUDA implementation. |
| `cuda-argument-commit` | existing `cuda`, `loki-argument-commit` | Compatibility alias preserving W10's legacy CUDA feature behavior. |
| `cuda-srs-registry` | existing `cuda` | W13 legacy registry feature; it remains on the sppark runtime. |
| `loki-gpu-srs-registry` | `loki-gpu-backend` | Selects the new backend's validated registry on its shared device runtime. |
| `cuda-resident` | `loki-gpu-backend` | Enables new-backend resident transform support. |
| `cuda-quotient` | `cuda-resident`, `loki-gpu-srs-registry`, `loki-argument-commit` | Enables the new-backend quotient pipeline and its W10 coefficient strategy. |
| `cuda-msm-balanced` | `loki-gpu-srs-registry` | Enables the new-backend balanced MSM implementation. |
| `cuda-lookup-prepare` | `cuda-quotient` | Enables lookup preparation over the new-backend quotient runtime. |
| `loki-deterministic-lookup-order` | none | Opt-in deterministic leftover-lookup placement; does not alter stock default behavior or select a backend. |

The exact normalized-manifest feature entries are `loki-backend-api =
["dep:loki-backend-api"]`, `loki-sppark = ["loki-backend-api", "cuda",
"cuda-ntt"]`, `loki-gpu-backend = ["loki-backend-api", "dep:loki-gpu",
"loki-gpu/cuda"]`, `loki-argument-commit = ["loki-backend-api"]`,
`cuda-argument-commit = ["cuda", "loki-argument-commit"]`,
`cuda-srs-registry = ["cuda"]`,
`loki-gpu-srs-registry = ["loki-gpu-backend"]`,
`cuda-resident = ["loki-gpu-backend"]`,
`cuda-quotient = ["cuda-resident", "loki-gpu-srs-registry",
"loki-argument-commit"]`, `cuda-msm-balanced = ["loki-gpu-srs-registry"]`,
`cuda-lookup-prepare = ["cuda-quotient"]`, and
`loki-deterministic-lookup-order = []`. Mirror these entries in
`Cargo.toml.orig`. These are Loki integration features; the native `loki-gpu`
crate currently has only its `cuda` build feature. When W10 is rebased,
preserve the legacy `cuda-argument-commit` alias exactly and route the
coefficient strategy through the CUDA-neutral `loki-argument-commit` feature.

In `loki-harness/Cargo.toml`, forward the W2 gate exactly as
`loki-gpu-backend = ["loki/loki-gpu-backend"]`; keep its existing `cuda = []`
placeholder CPU-only until a separately reviewed legacy migration. The W2 GPU
commands must select `--features loki-gpu-backend`, assert nonzero new-backend
dispatch/completion counters, and fail on fallback. Merely compiling the
forwarding feature is not a GPU test pass.

The new backend rejects any simultaneous legacy `cuda` or `cuda-ntt` feature,
whether reached directly, through `loki-sppark`, or through the compatibility
`cuda-argument-commit` alias. Loki integration must enforce this exclusivity
explicitly, for example with a compile-time guard equivalent to
`all(feature = "loki-gpu-backend", any(feature = "cuda", feature =
"cuda-ntt"))`. Thus `loki-sppark` and `loki-gpu-backend` are also incompatible.
The neutral `loki-argument-commit` feature may be enabled with the new backend
and must not enable legacy CUDA. The `loki-backend-api` feature is the shared
base and selects the CPU reference adapter when neither accelerator is
enabled. With no feature, Loki must run stock 0.8.3 call sites unchanged. A
later promotion of an opt-in mode to default requires a full stock-verifier
gate on a real recursion chain and a separate owner decision.

### Pre-existing default drift to resolve

The current Loki `877114d` tree is not a pristine stock source snapshot.
Specifically, `src/plonk/lookup/prover.rs:427-431` sorts leftover table keys
before filling rows; registry stock 0.8.3
`src/plonk/lookup/prover.rs:429-434` traverses `leftover_table_map.iter()` in
its native `HashMap` order. This can change lookup column order and proof
bytes. Before claiming the hard default gate, restore the stock traversal in
the no-feature branch and gate deterministic sorting behind
`loki-deterministic-lookup-order` only. The backend-only features must not
enable that ordering feature. Compare the full feature-off source diff against
the resolved registry package and run a byte-for-byte proof fixture. API
backend parity alone does not close this existing drift.

## Exact seams in Loki main at `877114d`

Line numbers below are the checked-out `loki-proofs` main snapshot from the
input brief; implementation must rebase and re-check before editing.

1. **MSM dispatch:** `src/poly/kzg/msm.rs:165-174` defines
   `msm_specific`; it profiles the zipped length, tries `cuda::try_msm`, then calls
   `msm_cpu`. Replace only the feature-enabled dispatch arm with the selected
   `Backend<F,G>::msm` call. Keep the no-feature body, zero filtering, and
   `min(scalars.len(), bases.len())` behavior exact. The call under
   `src/poly/kzg/msm/cuda.rs:17-57` is the legacy sppark adapter; route it
   through `loki-sppark` without changing its converter or safe type check.

2. **Base and extended transforms:** `src/poly/domain.rs:249-340,392-447`,
   `EvaluationDomain::{fft,ifft,gpu_fft}`. The legacy GPU arm is gated by
   `cuda-ntt` and falls back to `best_fft` on error. The new selected backend
   must return a fresh output before replacing `a`; inverse normalization is
   applied exactly once. Coset transforms are the domain operations at
   `src/poly/domain.rs:249-340`: `lagrange_to_coeff`, `coeff_to_lagrange`,
   `coeff_to_extended`, `extended_to_coeff`, and `extended_to_lagrange`.
   Extended-domain scaling uses the domain's three-cycle coset constants.

3. **Argument commitments (W10 follow-up, not present in the base revision):**
   These call sites are from W10 branch `loki/w10-gpu-quotient` at
   `053577f6083b329f2bbef1f3711f92cadc92a24f`, not `877114d`:
   `src/poly/commitment.rs:122-151`,
   `src/plonk/lookup/prover.rs:116-134`, and
   `src/plonk/permutation/prover.rs:147-153`. This integration is conditional
   on W10 merging first. If it has not merged, do not patch the base call
   sites from these line numbers; rebase the feature after merge and re-check
   the same files. Select the coefficient-basis strategy under the CUDA-neutral
   `loki-argument-commit` feature. Legacy `cuda-argument-commit` remains an
   alias that enables existing `cuda`; the new backend cannot be combined with
   `cuda` or `cuda-ntt`. The strategy may select coefficient-basis commitment
   only when
   `params.max_k() == domain.k()`, preserving the exact transform and SRS
   basis precondition. Keep transcript writes after the existing computation
   point and do not draw randomness in the backend.

4. **Quotient evaluator:** `src/plonk/evaluation.rs:305-577`,
   `Evaluator::evaluate_numerator`; caller `src/plonk/prover.rs:667-730`,
   `compute_nu_poly`. Compile the existing ordered graph into
   `QuotientProgram`; preserve circuit-instance order, gates, permutation sets,
   lookups, trash terms, Horner accumulation, signed Euclidean rotations, and
   existing zero-preserving vanishing inverses. On failure, recompute from
   retained host inputs. Do not let a resident-only failure fabricate a CPU
   fallback if no host checkpoint remains.

5. **SRS residency:** `src/poly/kzg/msm/cuda/backend.rs:89-212` and
   `src/poly/kzg/msm/cuda/tests.rs`. The W13 registry expands the legacy
   two-entry cache into byte-budgeted validated residency. The API lease key
   includes basis, point count, curve/wire ABI, device generation, and verified
   content identity. Do not accept address-only or caller-digest-only hits.
   Keep the current `cuda-runtime::lock` outer-before-cache-lock order for the
   legacy sppark path.

6. **Proof-resource admission:** the backend-neutral
   `ResidentBackend::{runtime_snapshot,try_reserve,release_reservation}` API
   exposes generation health and live/reserved/budget bytes to the recursion
   scheduler. Admission is immediate and nonblocking. The scheduler owns job
   identity, DAG dependencies, proof-key lineage, and admission policy; the
   backend reports resources and validates reservation generations. A
   reservation remains held until every dependent backend event has retired.

7. **Proof ordering:** `src/plonk/prover.rs:397-810` and evaluation helpers
   above. The backend computes values; Loki owns all event construction and
   transcript writes. The public v1 API returns only after its operation has
   completed and its output is safe to publish. Backend streams may execute
   work asynchronously internally, but host transfers use backend-owned
   staging and events retire before the host API returns. Loki writes transcript
   events in the same request/commitment slot order as stock.

## Dispatch and fallback semantics

`loki-backend-api` defines generic contracts; it does not select a backend or
perform fallback. The Loki dispatcher owns:

- CPU as the default implementation of each operation in API-enabled builds;
- selection of sppark or loki-gpu only under its explicit feature;
- fallback to CPU only for errors that guarantee unchanged caller-visible
  inputs and unpublished output, and only while Loki retains all host inputs;
- sticky disable of a failed backend generation after context loss or a fatal
  device error;
- transcript/event ordering, irrespective of completion order;
- validation of request sizes and supported field/curve/wire ABI before GPU
  entry. The dispatcher maps concrete Rust field/group types to a stable
  `ArithmeticAbi` and checks `BackendCapabilities::supports_arithmetic`; it
  must not infer GPU support from a generic trait bound or type name.

Every API error preserves caller-visible inputs and publishes no output.
Loki may recompute on CPU only when it has retained every original host input;
residency alone does not make data recoverable after context loss. Cache
eviction and generation poison may follow an error because they affect future
scheduling, not values already visible to the caller. The public v1 API avoids
borrowed asynchronous transfers: upload copies into backend-owned staging and
waits for transfer retirement, while download returns a completed fresh vector.

## Correctness contract map

| API operation | Lean obligation / theorem | Rust reference seam |
|---|---|---|
| `MsmBackend::msm` | `Msm.MeetsMsmSpecific`, `GpuSystem.msmDispatch_meets`, `dispatchSeqW9_eq`; stock zip semantics | `poly/kzg/msm.rs:165-190` |
| resident SRS reuse | `GpuSystem.residentCache_sound`, `stockHit_sound`, `KernelOkOnValid`, `msmSpecific_take_min` | `poly/kzg/msm/cuda/backend.rs:89-212` |
| host/resident transform | `Ntt.IsNaturalNtt`, `coset_via_natural`, `icoset_via_inverse`, `ifftW9_isInverse`; failure preserves original input | `poly/domain.rs:249-340,392-447` |
| quotient bytecode | `Plonk.numeratorAt`, `Fusion.fused_numerator_eq`, `Fusion.fused_order_matters`; Euclidean rotations | `plonk/evaluation.rs:305-577` |
| composed backend | `Refinement.Refines`, `Refinement.SameOrder`, `loki_proof_eq_stock` | `plonk/prover.rs:397-810` |

The Lean theorem is about mathematical values and observable event order, not
CUDA layout or timing. Each concrete implementation must connect its bytecode,
layout, and wire conversion to these contracts with differential tests and the
independent unmodified 0.8.3 verifier.

## Integration gates

1. Feature-off Loki build and proof-byte fixture match the stock 0.8.3 baseline.
2. CPU adapter tests for random, zero, repeated, boolean and unequal-length MSM
   inputs; all transform/coset shapes; quotient per-point equality; and SRS
   mutation/prefix/eviction cases.
3. Error injection proves untouched host inputs, no published partial output,
   and CPU fallback only with retained host checkpoints.
4. W2 k10/k14/k17 determinism and cross-verification; an independent registry
   stock verifier must accept the proof and reject mutation, truncation, and
   trailing bytes.
5. A3 k20 and the full leaf → parent → decider chain at k19–k20, with actual
   backend-success counters, zero unaccounted fallback, exact source/feature
   provenance, and repeated matched latency samples.

The default feature-off build is a separate invariant from these gates: no
backend path may enter it until its behavior and proof encoding match stock.
