//! CODE (proposed contract): CPU- and accelerator-neutral contracts for Loki
//! proof backends. This API definition and the compileable skeleton do not
//! constitute evidence that the native operations are implemented or verified.
//!
//! The API describes values and state transitions only. It contains no CUDA
//! types, pointers, stream implementation, or backend selection policy. Loki
//! owns backend selection, CPU fallback, and all transcript-visible ordering.
//!
//! Implementations must preserve the reference value for successful calls.
//! On error, caller-visible inputs remain unchanged and no tentative output is
//! published. An implementation may poison its device generation or invalidate
//! a cache entry after a hardware failure. Loki may fall back only when it
//! still owns a host checkpoint of every input needed by the CPU operation.

use core::marker::PhantomData;
pub use ff::PrimeField;
pub use group::Curve;

/// Identifies an operation for diagnostics and error classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operation {
    Msm,
    Transform,
    Upload,
    Download,
    ResidentTransform,
    Quotient,
    SrsAcquire,
    SrsMsm,
    SrsRelease,
    Runtime,
    Reserve,
    Release,
}

/// Broad failure category. Error text is intentionally not part of the API.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ErrorKind {
    Unsupported,
    InvalidInput,
    Capacity,
    Allocation,
    Transfer,
    Kernel,
    Synchronization,
    ContextLost,
    Poisoned,
    Internal,
    /// The caller cancelled before publication. `NotStarted` means no work
    /// began; `InputsPreserved` means submitted work retired successfully.
    /// Failure to establish retirement is reported as the actual device or
    /// synchronization error with `SessionPoisoned`, not as cancellation.
    /// This category does not request automatic CPU retry of cancelled work.
    Cancelled,
}

/// Caller-visible effects of a failed operation.
///
/// Every error preserves caller-visible inputs and publishes no output.
/// `SessionPoisoned` invalidates the generation. CPU fallback still requires
/// Loki to retain all original host inputs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FailureEffect {
    /// The operation did not start and made no caller-visible change.
    NotStarted,
    /// Inputs are unchanged and no output was published.
    InputsPreserved,
    /// Inputs are unchanged and no output was published; this generation must
    /// not accept further work.
    SessionPoisoned,
}

/// Backend failure with machine-readable operation, cause, and state effect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BackendError {
    pub operation: Operation,
    pub kind: ErrorKind,
    pub effect: FailureEffect,
}

impl BackendError {
    /// Construct an unsupported-operation result. This always has no side effect.
    pub const fn unsupported(operation: Operation) -> Self {
        Self {
            operation,
            kind: ErrorKind::Unsupported,
            effect: FailureEffect::NotStarted,
        }
    }

    /// Whether the failed operation left caller-visible inputs unchanged and
    /// published no result. Loki must separately check that it retained host
    /// inputs before retrying on CPU.
    pub const fn preserves_observable_state(self) -> bool {
        matches!(
            self.effect,
            FailureEffect::NotStarted
                | FailureEffect::InputsPreserved
                | FailureEffect::SessionPoisoned
        )
    }
}

/// Capabilities advertised by a backend instance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    pub host_msm: bool,
    pub host_transform: bool,
    pub resident_buffers: bool,
    pub resident_transform: bool,
    pub quotient_interpreter: bool,
    pub srs_registry: bool,
    /// Exact scalar-field and curve wire ABIs supported by this backend.
    /// Generic traits do not imply that every `PrimeField`/`Curve` is accepted.
    pub supported_arithmetic: Vec<ArithmeticAbi>,
    /// Maximum transform log-size, when `host_transform` or
    /// `resident_transform` is supported.
    pub max_transform_log: Option<u32>,
    /// Device-memory budget exposed to the scheduler; `None` means unknown.
    pub device_memory_budget: Option<u64>,
}

impl BackendCapabilities {
    pub fn supports_arithmetic(&self, arithmetic: ArithmeticAbi) -> bool {
        self.supported_arithmetic.contains(&arithmetic)
    }
}

/// Stable field identifiers defined by this API. New values must not be
/// reassigned after publication.
pub const FIELD_BLS12_381_SCALAR: u32 = 1;

/// Stable curve identifiers defined by this API. New values must not be
/// reassigned after publication.
pub const CURVE_BLS12_381_G1: u32 = 1;

/// Version of the scalar/point wire encodings used at the backend boundary.
pub const WIRE_ENCODING_V1: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FieldAbi {
    pub field_id: u32,
    pub encoding_version: u32,
}

/// Curve and point wire-ABI tag attached to an arithmetic capability or SRS.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CurveAbi {
    pub curve_id: u32,
    pub encoding_version: u32,
}

/// A supported scalar-field / group / wire-encoding combination.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArithmeticAbi {
    pub scalar_field: FieldAbi,
    pub curve: CurveAbi,
}

/// Whether an MSM request requires equal lengths or stock zip truncation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MsmLengthPolicy {
    /// Match `msm_specific`: process `min(scalars.len(), bases.len())` pairs.
    ZipTruncate,
    /// Reject unequal lengths before starting the operation.
    RequireEqual,
}

/// Borrowed MSM inputs and the length rule used by the stock call site.
pub struct MsmRequest<'a, F, G> {
    pub scalars: &'a [F],
    pub bases: &'a [G],
    pub length_policy: MsmLengthPolicy,
}

impl<'a, F, G> MsmRequest<'a, F, G> {
    pub const fn new(scalars: &'a [F], bases: &'a [G], length_policy: MsmLengthPolicy) -> Self {
        Self {
            scalars,
            bases,
            length_policy,
        }
    }

    /// Number of pairs an implementation may consume after validating policy.
    pub fn pair_count(&self) -> Result<usize, BackendError> {
        match self.length_policy {
            MsmLengthPolicy::ZipTruncate => Ok(self.scalars.len().min(self.bases.len())),
            MsmLengthPolicy::RequireEqual if self.scalars.len() == self.bases.len() => {
                Ok(self.scalars.len())
            }
            MsmLengthPolicy::RequireEqual => Err(BackendError {
                operation: Operation::Msm,
                kind: ErrorKind::InvalidInput,
                effect: FailureEffect::NotStarted,
            }),
        }
    }
}

/// Several scalar columns committed against one shared ordered base array.
/// Results from `msm_batch` correspond to `scalar_batches` slot for slot.
pub struct MsmBatchRequest<'a, F, G> {
    pub scalar_batches: &'a [&'a [F]],
    pub bases: &'a [G],
    pub length_policy: MsmLengthPolicy,
}

/// Transform direction and output representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransformDirection {
    Forward,
    Inverse,
}

/// Element ordering at the API boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElementOrder {
    Natural,
    BitReversed,
}

/// Inverse normalization contract. Exactly one factor of `1/n` is required
/// for Loki's inverse transforms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InverseNormalization {
    None,
    DivideByLengthOnce,
}

/// The public coset generator and its inverse used by Midnight's extended
/// domain. Forward conversion scales coefficient `i` by `generator^i`, pads
/// from `source_log_size` to `output_log_size`, then applies the natural-order
/// NTT. Inverse conversion performs the inverse NTT with the specified single
/// `1/n` normalization, then scales coefficient `i` by `generator_inverse^i`.
/// The backend may batch these operations, but their result must match this
/// sequence exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CosetSpec<F> {
    pub generator: F,
    pub generator_inverse: F,
}

/// Explicit mathematical transform contract, independent of backend layout.
/// `root` is always the canonical forward root for the full transform extent;
/// callers never pass an already-inverted root. For a forward transform, the
/// extent is `2^output_log_size` (after any zero padding). For an inverse
/// transform, it is `2^source_log_size`; the backend inverts this root and
/// applies `inverse_normalization` exactly once. The root must have exact order
/// equal to the transform extent. Coset extension uses the output extent and
/// inverse coset recovery uses the source extent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformSpec<F> {
    pub root: F,
    /// Log-size of the supplied vector/domain.
    pub source_log_size: u32,
    /// Log-size of the result domain. Forward coset extension may increase it;
    /// inverse coset recovery may decrease it. Plain NTTs use equal values.
    pub output_log_size: u32,
    pub direction: TransformDirection,
    pub input_order: ElementOrder,
    pub output_order: ElementOrder,
    pub inverse_normalization: InverseNormalization,
    pub coset: Option<CosetSpec<F>>,
}

/// Stable context identity. `generation` changes after a reset or context loss.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceGeneration {
    pub device_ordinal: u32,
    pub context_epoch: u64,
    pub generation: u64,
}

/// Current backend resource state for non-blocking scheduler admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeSnapshot {
    pub generation: DeviceGeneration,
    pub context_healthy: bool,
    pub budget_bytes: u64,
    pub live_bytes: u64,
    pub reserved_bytes: u64,
}

/// Opaque reservation identity. It is valid only for its device generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReservationId(pub u64);

/// Memory budget reservation held until all associated work is complete.
///
/// This token is intentionally not `Clone`; the backend validates its ID and
/// generation on release. Forgetting to release conservatively retains budget.
pub struct ReservationToken {
    id: ReservationId,
    generation: DeviceGeneration,
    bytes: u64,
}

impl ReservationToken {
    /// Construct untrusted reservation metadata for a backend implementation.
    /// Release must validate the token against its live reservation registry.
    pub const fn from_untrusted_metadata(
        id: ReservationId,
        generation: DeviceGeneration,
        bytes: u64,
    ) -> Self {
        Self {
            id,
            generation,
            bytes,
        }
    }

    pub const fn id(&self) -> ReservationId {
        self.id
    }

    pub const fn generation(&self) -> DeviceGeneration {
        self.generation
    }

    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// Result of an immediate scheduler admission request.
pub enum ReservationResult {
    Reserved(ReservationToken),
    Rejected {
        requested_bytes: u64,
        available_bytes: u64,
    },
}

/// Opaque allocation identity. This is metadata, never a device pointer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AllocationId(pub u64);

/// A typed, context-tagged reference to immutable resident data.
///
/// The handle contains no raw pointer and does not free memory on drop. The
/// backend validates its generation before use and owns the allocation policy.
pub struct ResidentBuffer<T> {
    generation: DeviceGeneration,
    allocation: AllocationId,
    len: usize,
    bytes: u64,
    _element: PhantomData<fn() -> T>,
}

impl<T> ResidentBuffer<T> {
    /// Construct untrusted allocation metadata for a backend implementation.
    /// This does not create or validate an allocation. Every backend operation
    /// must check generation, allocation identity, element type, length and
    /// byte size against its own allocation registry before using the handle.
    pub const fn from_untrusted_metadata(
        generation: DeviceGeneration,
        allocation: AllocationId,
        len: usize,
        bytes: u64,
    ) -> Self {
        Self {
            generation,
            allocation,
            len,
            bytes,
            _element: PhantomData,
        }
    }

    pub const fn generation(&self) -> DeviceGeneration {
        self.generation
    }

    pub const fn allocation(&self) -> AllocationId {
        self.allocation
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl<T> core::fmt::Debug for ResidentBuffer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResidentBuffer")
            .field("generation", &self.generation)
            .field("allocation", &self.allocation)
            .field("len", &self.len)
            .field("bytes", &self.bytes)
            .finish()
    }
}

/// Host-slice MSM interface. Successful results equal the selected stock MSM
/// semantics, including zip truncation when requested.
pub trait MsmBackend<F, G>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn msm(&self, request: MsmRequest<'_, F, G>) -> Result<G, BackendError>;

    /// Compute several MSMs over shared bases. Implementations publish the
    /// ordered result vector only after every requested MSM succeeds. The
    /// default is a sequential reference implementation; accelerators may
    /// batch uploads and launches while preserving result-slot order.
    fn msm_batch(&self, request: MsmBatchRequest<'_, F, G>) -> Result<Vec<G>, BackendError> {
        let mut results = Vec::with_capacity(request.scalar_batches.len());
        for scalars in request.scalar_batches {
            results.push(self.msm(MsmRequest::new(
                scalars,
                request.bases,
                request.length_policy,
            ))?);
        }
        Ok(results)
    }
}

/// Host-slice transform interface. Implementations return a fresh vector, so
/// an error cannot expose a partially transformed host input.
pub trait TransformBackend<F: PrimeField> {
    fn transform(&self, values: &[F], spec: TransformSpec<F>) -> Result<Vec<F>, BackendError>;
}

/// Resident storage and transform interface.
pub trait ResidentBackend<F, G>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn generation(&self) -> Result<DeviceGeneration, BackendError>;

    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError>;

    /// Non-blocking scheduler admission. It must not wait for another proof or
    /// evict memory protected by a live SRS lease.
    fn try_reserve(&self, bytes: u64) -> Result<ReservationResult, BackendError>;

    fn release_reservation(&self, reservation: ReservationToken) -> Result<(), BackendError>;

    /// Returns only after all transfer work has retired. Implementations must
    /// copy the caller slice into backend-owned staging before scheduling DMA.
    fn upload_scalars(&self, values: &[F]) -> Result<ResidentBuffer<F>, BackendError>;

    fn upload_points(&self, values: &[G]) -> Result<ResidentBuffer<G>, BackendError>;

    /// Returns a complete fresh vector. It never exposes a partially written
    /// caller destination if device transfer fails.
    fn download_scalars(&self, source: &ResidentBuffer<F>) -> Result<Vec<F>, BackendError>;

    /// Produces a fresh handle. The input buffer remains logically unchanged
    /// if an error is returned.
    fn transform_resident(
        &self,
        source: &ResidentBuffer<F>,
        spec: TransformSpec<F>,
    ) -> Result<ResidentBuffer<F>, BackendError>;

    fn release_scalars(&self, buffer: ResidentBuffer<F>) -> Result<(), BackendError>;

    fn release_points(&self, buffer: ResidentBuffer<G>) -> Result<(), BackendError>;
}

/// Operand of a quotient bytecode instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operand {
    Register(u32),
    Constant(u32),
    Column { column: u32, rotation: i64 },
}

/// Arithmetic instruction. Program order is semantic and must not be changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotientOpcode {
    Add,
    Sub,
    Mul,
    Square,
    Double,
    Negate,
}

/// One row-wise arithmetic instruction. `Add`, `Sub`, and `Mul` require
/// `right = Some(_)`; `Square`, `Double`, and `Negate` require `right = None`.
/// For every opcode, operands are read before `destination` is written, so an
/// instruction may overwrite a register it reads, provided that register was
/// initialized by an earlier instruction. Horner accumulation is represented
/// by ordinary `Mul` followed by `Add` instructions to keep its read/write
/// order explicit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotientInstruction {
    pub opcode: QuotientOpcode,
    pub destination: u32,
    pub left: Operand,
    pub right: Option<Operand>,
}

/// Bytecode and resident extended-domain columns for one deterministic
/// quotient numerator evaluation.
///
/// The program is valid only when `domain_log <= extended_domain_log < usize::BITS`,
/// `rows == 2^extended_domain_log`, `rotation_scale ==
/// 2^(extended_domain_log - domain_log)`, and each column has exactly `rows`
/// values. Implementations must use checked size arithmetic and reject invalid
/// shapes before reading a column or launching work. Column rotation at row
/// `r` uses the Euclidean index
/// `(r + rotation * rotation_scale) mod rows`; implementations must avoid
/// signed overflow while computing that index.
///
/// There are `register_count` row-wise registers numbered from zero. Every
/// instruction destination and every register operand must be less than this
/// count. A register operand must have been initialized by an earlier
/// instruction, including when it aliases the current instruction's
/// destination; constants and columns must also be in bounds. The output
/// register must be initialized on every row. Reject malformed programs before
/// starting execution, with no result published.
pub struct QuotientProgram<'a, F> {
    pub rows: usize,
    pub domain_log: u32,
    pub extended_domain_log: u32,
    pub rotation_scale: usize,
    pub register_count: u32,
    pub instructions: &'a [QuotientInstruction],
    pub constants: &'a [F],
    pub columns: &'a [ResidentBuffer<F>],
    pub output_register: u32,
}

/// Resident quotient numerator bytecode execution. The returned buffer
/// contains the ordered numerator evaluation on the extended domain, before
/// division by the vanishing polynomial and before the inverse coset transform
/// and quotient truncation. Those remain explicit caller-side operations.
/// Output remains unpublished until the complete ordered program succeeds.
pub trait QuotientBackend<F: PrimeField> {
    fn evaluate_quotient(
        &self,
        program: QuotientProgram<'_, F>,
    ) -> Result<ResidentBuffer<F>, BackendError>;
}

/// Basis represented by an SRS array.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SrsBasis {
    Coefficient,
    Lagrange,
}

/// Cache key supplied alongside the actual points. The backend must validate
/// content identity against the points; a caller-supplied digest alone is not
/// sufficient to establish a hit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SrsCacheKey {
    pub curve_abi: CurveAbi,
    pub basis: SrsBasis,
    pub domain_log: u32,
    pub point_count: usize,
    pub generation: DeviceGeneration,
    pub content_identity: [u8; 32],
}

/// Identity of one live SRS lease within a backend generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SrsLeaseId(pub u64);

pub struct SrsRequest<'a, G> {
    pub key: SrsCacheKey,
    pub points: &'a [G],
}

/// A checked lease on a resident SRS array. Backends reject stale generations
/// and validate the lease ID, key, and buffer against their active registry on
/// every use. A live lease prevents eviction until `release_srs` is called
/// after dependent work completes.
pub struct SrsLease<G> {
    lease_id: SrsLeaseId,
    key: SrsCacheKey,
    generation: DeviceGeneration,
    bases: ResidentBuffer<G>,
}

impl<G> SrsLease<G> {
    /// Construct explicitly untrusted lease metadata for a backend
    /// implementation. This does not establish a live lease; every operation
    /// and release must verify it against the active registry.
    pub const fn from_untrusted_metadata(
        lease_id: SrsLeaseId,
        key: SrsCacheKey,
        generation: DeviceGeneration,
        bases: ResidentBuffer<G>,
    ) -> Self {
        Self {
            lease_id,
            key,
            generation,
            bases,
        }
    }

    pub const fn lease_id(&self) -> SrsLeaseId {
        self.lease_id
    }

    pub const fn key(&self) -> SrsCacheKey {
        self.key
    }

    pub const fn generation(&self) -> DeviceGeneration {
        self.generation
    }

    pub const fn bases(&self) -> &ResidentBuffer<G> {
        &self.bases
    }
}

/// SRS residency registry. Implementations validate points before insertion,
/// check exact content on a cache hit, and account resident bytes.
pub trait SrsRegistry<F, G>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError>;

    fn srs_msm(
        &self,
        lease: &SrsLease<G>,
        scalars: &[F],
        length_policy: MsmLengthPolicy,
    ) -> Result<G, BackendError>;

    /// SRS-backed batch variant with all-or-error output publication and
    /// request-slot ordering. The default delegates to `srs_msm` sequentially.
    fn srs_msm_batch(
        &self,
        lease: &SrsLease<G>,
        scalar_batches: &[&[F]],
        length_policy: MsmLengthPolicy,
    ) -> Result<Vec<G>, BackendError> {
        let mut results = Vec::with_capacity(scalar_batches.len());
        for scalars in scalar_batches {
            results.push(self.srs_msm(lease, scalars, length_policy)?);
        }
        Ok(results)
    }

    fn release_srs(&self, lease: SrsLease<G>) -> Result<(), BackendError>;
}

/// Complete backend surface for a field/group instantiation.
pub trait Backend<F, G>:
    MsmBackend<F, G>
    + TransformBackend<F>
    + ResidentBackend<F, G>
    + QuotientBackend<F>
    + SrsRegistry<F, G>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn capabilities(&self) -> BackendCapabilities;
}
