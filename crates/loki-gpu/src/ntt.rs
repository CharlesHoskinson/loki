//! Resident NTT and exact coset transforms (P4).
//!
//! CODE (P4): every transform is one runtime launch
//! ([`GpuRuntime::launch_transform_fresh`]) that reads an immutable source
//! allocation and publishes a fresh output only after its completion event
//! has retired. This module owns the mathematical contract and the checks
//! that run before any device work:
//!
//! * **shape** ([`validate_transform`]): exact power-of-two source length,
//!   equal logs for plain transforms, padding only for forward coset and
//!   truncation only for inverse coset, natural order at coset boundaries,
//!   no normalization on forward transforms, `log <= MAX_TRANSFORM_LOG`;
//! * **root and coset**: the supplied forward root must have exact order
//!   `2^extent`; `generator * generator_inverse == 1`. The backend never picks
//!   a root or coset of its own;
//! * **wire field** ([`wire_field_supported`]): the native kernel is
//!   BLS12-381 Fr only. The field is identified by modulus, bit length,
//!   repr width and the little-endian canonical encoding of `1`, `2^64` and
//!   `-1`; any other `PrimeField` is `Unsupported` before device work.
//!
//! Scalars stay in canonical `to_repr` form in resident buffers (the P2
//! storage form). Each launch converts canonical → Montgomery on load and
//! back to fully reduced canonical bytes on finish, so no new storage form is
//! introduced. Inverse normalization is a single multiplication by `N⁻¹`
//! inside the finish step, and only when `DivideByLengthOnce` is requested.
//!
//! Coset semantics follow `CosetSpec` / stock `distribute_powers_zeta`: the
//! forward transform multiplies source coefficient `i` (global index) by
//! `generator^i` before zero padding; the inverse multiplies output
//! coefficient `i` by `generator_inverse^i` after the normalized inverse. For
//! Loki's ζ with ζ³ = 1 this is exactly `ζ^(i mod 3)` / `ζ^(-i mod 3)`.

use ff::PrimeField;
use loki_backend_api::{
    ArithmeticAbi, BackendError, CurveAbi, ElementOrder, ErrorKind, FailureEffect, FieldAbi,
    InverseNormalization, Operation, ResidentBuffer, TransformBackend, TransformDirection,
    TransformSpec, CosetSpec, CURVE_BLS12_381_G1, FIELD_BLS12_381_SCALAR, WIRE_ENCODING_V1,
};

use crate::runtime::{
    CancelToken, GpuRuntime, TransformLaunch, TRANSFORM_ELEM_BYTES, TRANSFORM_MAX_POW_TABLE,
};
use crate::LokiGpu;

/// Largest supported transform log-size. `2^27` scalars are 4 GiB per
/// buffer; larger transforms return `Unsupported` so Loki keeps the stock CPU
/// path. (BLS12-381 Fr has 2-adicity 32.)
pub const MAX_TRANSFORM_LOG: u32 = 27;

/// The scalar wire ABI accepted by the native transform.
pub const TRANSFORM_ARITHMETIC: ArithmeticAbi = ArithmeticAbi {
    scalar_field: FieldAbi {
        field_id: FIELD_BLS12_381_SCALAR,
        encoding_version: WIRE_ENCODING_V1,
    },
    curve: CurveAbi {
        curve_id: CURVE_BLS12_381_G1,
        encoding_version: WIRE_ENCODING_V1,
    },
};

/// BLS12-381 scalar modulus `r`, big-endian hex without prefix.
const MODULUS_HEX: &str = "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";

/// `r - 1`, little-endian bytes.
const R_MINUS_ONE_LE: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0x02, 0xa4, 0xbd, 0x53,
    0x05, 0xd8, 0xa1, 0x09, 0x08, 0xd8, 0x39, 0x33, 0x48, 0x7d, 0x9d, 0x29, 0x53, 0xa7, 0xed, 0x73,
];

/// Whether `F` is BLS12-381 Fr with canonical little-endian `to_repr`
/// encoding, i.e. whether its resident bytes are what the kernel expects.
pub fn wire_field_supported<F: PrimeField>() -> bool {
    if F::NUM_BITS != 255 || F::Repr::default().as_ref().len() != TRANSFORM_ELEM_BYTES {
        return false;
    }
    let modulus = F::MODULUS.trim_start_matches("0x").to_ascii_lowercase();
    if modulus != MODULUS_HEX {
        return false;
    }
    let le = |x: F| -> [u8; 32] {
        let mut out = [0u8; 32];
        out.copy_from_slice(x.to_repr().as_ref());
        out
    };
    let mut one = [0u8; 32];
    one[0] = 1;
    let mut two64 = [0u8; 32];
    two64[8] = 1;
    le(F::ONE) == one
        && le(F::from(u64::MAX) + F::ONE) == two64
        && le(-F::ONE) == R_MINUS_ONE_LE
        && le(F::ZERO) == [0u8; 32]
}

/// Validated extents of one transform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransformShape {
    /// Elements read from the source.
    pub source_len: usize,
    /// Elements in the published result.
    pub output_len: usize,
    /// `log2` of the transform extent (the root's exact order).
    pub extent_log: u32,
}

const fn invalid(op: Operation) -> BackendError {
    BackendError {
        operation: op,
        kind: ErrorKind::InvalidInput,
        effect: FailureEffect::NotStarted,
    }
}

fn pow2<F: PrimeField>(x: F, j: u32) -> F {
    let mut y = x;
    for _ in 0..j {
        y = y.square();
    }
    y
}

/// Check a transform request before any device work. Every rejection is
/// `NotStarted`. `Unsupported` means Loki may use the stock CPU path;
/// `InvalidInput` means the request itself is malformed.
pub fn validate_transform<F: PrimeField>(
    source_len: usize,
    spec: &TransformSpec<F>,
    op: Operation,
) -> Result<TransformShape, BackendError> {
    let bad = invalid(op);
    let (src_log, out_log) = (spec.source_log_size, spec.output_log_size);
    if src_log >= usize::BITS || out_log >= usize::BITS {
        return Err(bad);
    }
    let natural = spec.input_order == ElementOrder::Natural
        && spec.output_order == ElementOrder::Natural;
    let extent_log = match (spec.direction, spec.coset.is_some()) {
        (_, false) if src_log != out_log => return Err(bad),
        (_, false) => src_log,
        (TransformDirection::Forward, true) if src_log > out_log => return Err(bad),
        (TransformDirection::Forward, true) => out_log,
        (TransformDirection::Inverse, true) if out_log > src_log => return Err(bad),
        (TransformDirection::Inverse, true) => src_log,
    };
    if spec.coset.is_some() && !natural {
        return Err(bad);
    }
    if spec.direction == TransformDirection::Forward
        && spec.inverse_normalization != InverseNormalization::None
    {
        return Err(bad);
    }
    if source_len != 1usize << src_log {
        return Err(bad);
    }
    if let Some(CosetSpec {
        generator,
        generator_inverse,
    }) = spec.coset
    {
        if bool::from(generator.is_zero()) || generator * generator_inverse != F::ONE {
            return Err(bad);
        }
    }
    // Exact order 2^extent: w^(2^extent) = 1 and, for extent >= 1,
    // w^(2^(extent-1)) != 1 (so -1 for a 2-power order).
    let root_ok = if extent_log == 0 {
        spec.root == F::ONE
    } else {
        let half = pow2(spec.root, extent_log - 1);
        half != F::ONE && half.square() == F::ONE
    };
    if !root_ok {
        return Err(bad);
    }
    if extent_log > MAX_TRANSFORM_LOG {
        return Err(BackendError::unsupported(op));
    }
    Ok(TransformShape {
        source_len,
        output_len: 1usize << out_log,
        extent_log,
    })
}

fn repr32<F: PrimeField>(x: F) -> [u8; TRANSFORM_ELEM_BYTES] {
    let mut out = [0u8; TRANSFORM_ELEM_BYTES];
    out.copy_from_slice(x.to_repr().as_ref());
    out
}

/// Build the native launch descriptor for a validated request.
pub(crate) fn build_launch<F: PrimeField>(
    shape: &TransformShape,
    spec: &TransformSpec<F>,
    op: Operation,
) -> Result<TransformLaunch, BackendError> {
    let inverse = spec.direction == TransformDirection::Inverse;
    // The spec always carries the forward root; invert it exactly once here.
    let w = if inverse {
        Option::<F>::from(spec.root.invert()).ok_or(invalid(op))?
    } else {
        spec.root
    };
    let scale = match spec.inverse_normalization {
        InverseNormalization::None => F::ONE,
        InverseNormalization::DivideByLengthOnce => {
            let n = F::from(1u64 << shape.extent_log);
            Option::<F>::from(n.invert()).ok_or(invalid(op))?
        }
    };
    let mut root_powers = [[0u8; TRANSFORM_ELEM_BYTES]; TRANSFORM_MAX_POW_TABLE];
    let mut coset_powers = [[0u8; TRANSFORM_ELEM_BYTES]; TRANSFORM_MAX_POW_TABLE];
    let mut x = w;
    for slot in root_powers.iter_mut().take(shape.extent_log as usize) {
        *slot = repr32(x);
        x = x.square();
    }
    let (pre_coset, post_coset) = match spec.coset {
        None => (false, false),
        Some(c) => {
            let g = if inverse { c.generator_inverse } else { c.generator };
            let mut y = g;
            for slot in coset_powers.iter_mut().take(shape.extent_log as usize) {
                *slot = repr32(y);
                y = y.square();
            }
            (!inverse, inverse)
        }
    };
    Ok(TransformLaunch {
        ntt_log: shape.extent_log,
        source_len: shape.source_len as u64,
        output_len: shape.output_len as u64,
        input_bitrev: spec.input_order == ElementOrder::BitReversed,
        output_bitrev: spec.output_order == ElementOrder::BitReversed,
        pre_coset,
        post_coset,
        root_powers,
        coset_powers,
        scale: repr32(scale),
    })
}

fn prepare<F: PrimeField>(
    source_len: usize,
    spec: &TransformSpec<F>,
    op: Operation,
) -> Result<TransformLaunch, BackendError> {
    let shape = validate_transform(source_len, spec, op)?;
    if !wire_field_supported::<F>() {
        return Err(BackendError::unsupported(op));
    }
    build_launch(&shape, spec, op)
}

/// Resident transform: validate, then one runtime launch from `source` into a
/// fresh handle. `source` is never written; on error nothing is published.
pub fn transform_resident<F: PrimeField>(
    runtime: &GpuRuntime,
    source: &ResidentBuffer<F>,
    spec: TransformSpec<F>,
) -> Result<ResidentBuffer<F>, BackendError> {
    let op = Operation::ResidentTransform;
    let launch = prepare(source.len(), &spec, op)?;
    runtime.launch_transform_fresh(source, &launch)
}

/// Host transform: upload → one launch → download → release. The caller's
/// slice is copied into backend staging and never written; a fresh vector is
/// returned only after every step succeeded. Device data is released on
/// every path (best effort after an error, which never affects the result).
pub fn transform_host<F: PrimeField>(
    runtime: &GpuRuntime,
    values: &[F],
    spec: TransformSpec<F>,
) -> Result<Vec<F>, BackendError> {
    let op = Operation::Transform;
    let launch = prepare(values.len(), &spec, op)?;
    if !runtime.driver_supports_transform() {
        return Err(BackendError::unsupported(op));
    }
    let cancel = CancelToken::new();
    let source = runtime.upload_scalars_cancellable(values, &cancel)?;
    let result = runtime.launch_transform_fresh(&source, &launch);
    let _ = runtime.release_handle(source);
    let output = result?;
    let host = runtime.download_scalars_cancellable(&output, &cancel);
    let _ = runtime.release_handle(output);
    host
}

/// Apply `specs` in order, entirely on the device. Each step reads the
/// previous step's published buffer; intermediates are released after the
/// next step completes (or fails). `source` is never written or released.
/// Readback is the caller's explicit choice at the chain boundary.
pub fn transform_chain<F: PrimeField>(
    runtime: &GpuRuntime,
    source: &ResidentBuffer<F>,
    specs: &[TransformSpec<F>],
) -> Result<ResidentBuffer<F>, BackendError> {
    let op = Operation::ResidentTransform;
    if specs.is_empty() {
        return Err(invalid(op));
    }
    // Validate the whole chain before touching the device.
    let mut len = source.len();
    let mut launches = Vec::with_capacity(specs.len());
    for spec in specs {
        let launch = prepare(len, spec, op)?;
        len = launch.output_len as usize;
        launches.push(launch);
    }
    let mut current: Option<ResidentBuffer<F>> = None;
    for launch in &launches {
        let next = runtime.launch_transform_fresh(current.as_ref().unwrap_or(source), launch);
        if let Some(prev) = current.take() {
            let _ = runtime.release_handle(prev);
        }
        current = Some(next?);
    }
    Ok(current.expect("non-empty chain"))
}

impl<F: PrimeField> TransformBackend<F> for LokiGpu {
    fn transform(&self, values: &[F], spec: TransformSpec<F>) -> Result<Vec<F>, BackendError> {
        transform_host(GpuRuntime::process(), values, spec)
    }
}

/// The exact constants of one stock evaluation domain, and the transform
/// specifications of its five public conversions. Loki builds this from its
/// `EvaluationDomain` (`get_omega`, `get_extended_omega`, `F::ZETA`); the
/// backend never derives a root on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DomainTransforms<F> {
    pub k: u32,
    pub extended_k: u32,
    /// Primitive `2^k`-th root (`get_omega`).
    pub omega: F,
    /// Primitive `2^extended_k`-th root (`get_extended_omega`).
    pub extended_omega: F,
    /// Coset generator (`F::ZETA`, order three).
    pub zeta: F,
}

impl<F: PrimeField> DomainTransforms<F> {
    fn plain(&self, direction: TransformDirection) -> TransformSpec<F> {
        TransformSpec {
            root: self.omega,
            source_log_size: self.k,
            output_log_size: self.k,
            direction,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: match direction {
                TransformDirection::Forward => InverseNormalization::None,
                TransformDirection::Inverse => InverseNormalization::DivideByLengthOnce,
            },
            coset: None,
        }
    }

    fn coset(&self) -> Option<CosetSpec<F>> {
        Option::<F>::from(self.zeta.invert()).map(|inv| CosetSpec {
            generator: self.zeta,
            generator_inverse: inv,
        })
    }

    /// `coeff_to_lagrange`: forward NTT with `omega`.
    pub fn coeff_to_lagrange(&self) -> TransformSpec<F> {
        self.plain(TransformDirection::Forward)
    }

    /// `lagrange_to_coeff`: inverse NTT with `omega⁻¹` and one `1/n`.
    pub fn lagrange_to_coeff(&self) -> TransformSpec<F> {
        self.plain(TransformDirection::Inverse)
    }

    /// `coeff_to_extended`: ζ^i scaling, pad to `2^extended_k`, forward NTT.
    /// `None` if ζ is not invertible (then no valid coset exists).
    pub fn coeff_to_extended(&self) -> Option<TransformSpec<F>> {
        Some(TransformSpec {
            root: self.extended_omega,
            source_log_size: self.k,
            output_log_size: self.extended_k,
            direction: TransformDirection::Forward,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: InverseNormalization::None,
            coset: Some(self.coset()?),
        })
    }

    /// Inverse coset transform keeping the first `2^output_log` coefficients.
    pub fn extended_to_coeff_prefix(&self, output_log: u32) -> Option<TransformSpec<F>> {
        Some(TransformSpec {
            root: self.extended_omega,
            source_log_size: self.extended_k,
            output_log_size: output_log,
            direction: TransformDirection::Inverse,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: InverseNormalization::DivideByLengthOnce,
            coset: Some(self.coset()?),
        })
    }

    /// `extended_to_coeff`: inverse NTT with one `1/N`, then ζ^-i; all `N`
    /// coefficients.
    pub fn extended_to_coeff(&self) -> Option<TransformSpec<F>> {
        self.extended_to_coeff_prefix(self.extended_k)
    }

    /// `extended_to_lagrange`: the inverse coset transform truncated to `n`
    /// coefficients, then the small forward NTT. Run with
    /// [`transform_chain`] to keep the intermediate on the device.
    pub fn extended_to_lagrange(&self) -> Option<[TransformSpec<F>; 2]> {
        Some([self.extended_to_coeff_prefix(self.k)?, self.coeff_to_lagrange()])
    }
}
