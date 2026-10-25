//! Stock CPU reference adapter for `loki-backend-api`.
//!
//! Every successful result is produced by the stock midnight-proofs 0.8.3
//! routines that the feature-off build calls directly: `msm_cpu` (stock
//! `msm_specific` body, including zero filtering and the blst/`msm_best`
//! split) and `best_fft`, with the stock coset scaling and exactly one
//! inverse normalization. Resident, quotient and SRS operations are
//! explicitly `Unsupported`: the CPU quotient path remains the existing
//! `Evaluator`, which needs no typed program.

use ff::{Field, PrimeField};
use group::Curve;
use midnight_curves::{fft::best_fft, CurveAffine};

use super::{
    api::{
        Backend, BackendCapabilities, BackendError, CosetSpec, DeviceGeneration, ElementOrder,
        ErrorKind, FailureEffect, InverseNormalization, MsmBackend, MsmLengthPolicy, MsmRequest,
        Operation, QuotientBackend, QuotientProgram, ReservationResult, ReservationToken,
        ResidentBackend, ResidentBuffer, RuntimeSnapshot, SrsLease, SrsRegistry, SrsRequest,
        TransformBackend, TransformDirection, TransformSpec,
    },
    arithmetic_abi,
};
use crate::{poly::kzg::msm::msm_cpu, utils::arithmetic::parallelize};

/// Stateless stock CPU backend. Always available; never fails on a valid
/// request.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuBackend;

const fn invalid(operation: Operation) -> BackendError {
    BackendError {
        operation,
        kind: ErrorKind::InvalidInput,
        effect: FailureEffect::NotStarted,
    }
}

/// Stock MSM over the effective prefix selected by the request's length
/// policy. `ZipTruncate` never fails.
pub(crate) fn msm_request<C: CurveAffine>(
    request: MsmRequest<'_, C::Scalar, C::Curve>,
) -> Result<C::Curve, BackendError> {
    let pairs = request.pair_count()?;
    Ok(msm_cpu::<C>(
        &request.scalars[..pairs],
        &request.bases[..pairs],
    ))
}

impl<F, G> MsmBackend<F, G> for CpuBackend
where
    F: PrimeField,
    G: Curve<Scalar = F>,
    G::AffineRepr: CurveAffine<ScalarExt = F, CurveExt = G>,
{
    fn msm(&self, request: MsmRequest<'_, F, G>) -> Result<G, BackendError> {
        msm_request::<G::AffineRepr>(request)
    }
}

/// `root` has multiplicative order exactly `2^log`.
fn has_exact_order<F: Field>(root: F, log: u32) -> bool {
    if log == 0 {
        return root == F::ONE;
    }
    let mut half = root;
    for _ in 1..log {
        half = half.square();
    }
    half != F::ONE && half.square() == F::ONE
}

/// Checked `2^log` for a transform extent of field `F`.
fn extent<F: PrimeField>(log: u32) -> Option<usize> {
    (log <= F::S).then_some(())?;
    1usize.checked_shl(log)
}

/// Validate a host transform request against `len` input values without
/// touching them. Returns the output length.
pub(crate) fn validate_transform<F: PrimeField>(
    len: usize,
    spec: &TransformSpec<F>,
) -> Result<usize, BackendError> {
    let op = Operation::Transform;
    if spec.input_order != ElementOrder::Natural || spec.output_order != ElementOrder::Natural {
        // The stock domain only uses natural order; layouts are a backend extension.
        return Err(BackendError::unsupported(op));
    }
    let source = extent::<F>(spec.source_log_size).ok_or(invalid(op))?;
    let output = extent::<F>(spec.output_log_size).ok_or(invalid(op))?;
    if len != source {
        return Err(invalid(op));
    }
    let size_ok = match (spec.direction, spec.coset.is_some()) {
        (_, false) => output == source,
        (TransformDirection::Forward, true) => output >= source,
        (TransformDirection::Inverse, true) => output <= source,
    };
    if !size_ok {
        return Err(invalid(op));
    }
    let transform_log = match spec.direction {
        TransformDirection::Forward => {
            if spec.inverse_normalization != InverseNormalization::None {
                return Err(invalid(op));
            }
            spec.output_log_size
        }
        TransformDirection::Inverse => spec.source_log_size,
    };
    if !has_exact_order(spec.root, transform_log) {
        return Err(invalid(op));
    }
    if let Some(CosetSpec {
        generator,
        generator_inverse,
    }) = spec.coset
    {
        if generator.is_zero_vartime() || generator * generator_inverse != F::ONE {
            return Err(invalid(op));
        }
    }
    Ok(output)
}

/// Multiply coefficient `i` by `generator^i`. For a cube root of unity this
/// is exactly stock `EvaluationDomain::distribute_powers_zeta` (with
/// `[generator, generator_inverse] = [g^1, g^2]`); any other generator uses
/// chunked powers, which are equal field values.
fn distribute_powers<F: PrimeField>(values: &mut [F], generator: F, generator_inverse: F) {
    if generator.square() * generator == F::ONE {
        let coset_powers = [generator, generator_inverse];
        parallelize(values, |a, mut index| {
            for a in a {
                let i = index % (coset_powers.len() + 1);
                if i != 0 {
                    *a *= &coset_powers[i - 1];
                }
                index += 1;
            }
        });
    } else {
        parallelize(values, |a, start| {
            let mut power = generator.pow_vartime([start as u64]);
            for a in a {
                *a *= &power;
                power *= &generator;
            }
        });
    }
}

/// Run a validated transform in place, in stock operation order:
/// forward = coset scale, zero pad, `best_fft`;
/// inverse = `best_fft` with the inverted root, one `1/n`, truncate, unscale.
pub(crate) fn transform_in_place<F: PrimeField>(
    values: &mut Vec<F>,
    spec: TransformSpec<F>,
) -> Result<(), BackendError> {
    let output = validate_transform(values.len(), &spec)?;
    match spec.direction {
        TransformDirection::Forward => {
            if let Some(coset) = spec.coset {
                distribute_powers(values, coset.generator, coset.generator_inverse);
            }
            values.resize(output, F::ZERO);
            best_fft(values, spec.root, spec.output_log_size);
        }
        TransformDirection::Inverse => {
            let root_inverse = spec.root.invert().expect("validated root is nonzero");
            best_fft(values, root_inverse, spec.source_log_size);
            if spec.inverse_normalization == InverseNormalization::DivideByLengthOnce {
                let divisor = F::from(2)
                    .pow_vartime([u64::from(spec.source_log_size)])
                    .invert()
                    .expect("two-adic length is invertible");
                parallelize(values, |a, _| {
                    for a in a {
                        *a *= &divisor;
                    }
                });
            }
            if let Some(coset) = spec.coset {
                values.truncate(output);
                distribute_powers(values, coset.generator_inverse, coset.generator);
            }
        }
    }
    Ok(())
}

impl<F: PrimeField> TransformBackend<F> for CpuBackend {
    fn transform(&self, values: &[F], spec: TransformSpec<F>) -> Result<Vec<F>, BackendError> {
        validate_transform(values.len(), &spec)?;
        let mut output = values.to_vec();
        transform_in_place(&mut output, spec)?;
        Ok(output)
    }
}

impl<F, G> ResidentBackend<F, G> for CpuBackend
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn generation(&self) -> Result<DeviceGeneration, BackendError> {
        Err(BackendError::unsupported(Operation::Runtime))
    }

    fn runtime_snapshot(&self) -> Result<RuntimeSnapshot, BackendError> {
        Err(BackendError::unsupported(Operation::Runtime))
    }

    fn try_reserve(&self, _bytes: u64) -> Result<ReservationResult, BackendError> {
        Err(BackendError::unsupported(Operation::Reserve))
    }

    fn release_reservation(&self, _reservation: ReservationToken) -> Result<(), BackendError> {
        Err(BackendError::unsupported(Operation::Release))
    }

    fn upload_scalars(&self, _values: &[F]) -> Result<ResidentBuffer<F>, BackendError> {
        Err(BackendError::unsupported(Operation::Upload))
    }

    fn upload_points(&self, _values: &[G]) -> Result<ResidentBuffer<G>, BackendError> {
        Err(BackendError::unsupported(Operation::Upload))
    }

    fn download_scalars(&self, _source: &ResidentBuffer<F>) -> Result<Vec<F>, BackendError> {
        Err(BackendError::unsupported(Operation::Download))
    }

    fn transform_resident(
        &self,
        _source: &ResidentBuffer<F>,
        _spec: TransformSpec<F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        Err(BackendError::unsupported(Operation::ResidentTransform))
    }

    fn release_scalars(&self, _buffer: ResidentBuffer<F>) -> Result<(), BackendError> {
        Err(BackendError::unsupported(Operation::Release))
    }

    fn release_points(&self, _buffer: ResidentBuffer<G>) -> Result<(), BackendError> {
        Err(BackendError::unsupported(Operation::Release))
    }
}

impl<F: PrimeField> QuotientBackend<F> for CpuBackend {
    fn evaluate_quotient(
        &self,
        _program: QuotientProgram<'_, F>,
    ) -> Result<ResidentBuffer<F>, BackendError> {
        Err(BackendError::unsupported(Operation::Quotient))
    }
}

impl<F, G> SrsRegistry<F, G> for CpuBackend
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, _request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        Err(BackendError::unsupported(Operation::SrsAcquire))
    }

    fn srs_msm(
        &self,
        _lease: &SrsLease<G>,
        _scalars: &[F],
        _length_policy: MsmLengthPolicy,
    ) -> Result<G, BackendError> {
        Err(BackendError::unsupported(Operation::SrsMsm))
    }

    fn release_srs(&self, _lease: SrsLease<G>) -> Result<(), BackendError> {
        Err(BackendError::unsupported(Operation::SrsRelease))
    }
}

impl<F, G> Backend<F, G> for CpuBackend
where
    F: PrimeField,
    G: Curve<Scalar = F>,
    G::AffineRepr: CurveAffine<ScalarExt = F, CurveExt = G>,
{
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            host_msm: true,
            host_transform: true,
            // The CPU adapter serves every Rust curve; only the native ABI it
            // shares with accelerators is listed here.
            supported_arithmetic: arithmetic_abi::<G::AffineRepr>().into_iter().collect(),
            max_transform_log: Some(F::S),
            ..BackendCapabilities::default()
        }
    }
}
