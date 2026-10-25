//! Loki-owned backend selection and checked CPU fallback.
//!
//! The dispatcher validates each request before any backend entry, offers it
//! to an accelerator only when that accelerator advertises the operation and
//! the exact arithmetic ABI, and otherwise (or on any error) runs the stock
//! CPU adapter. Inputs are borrowed host slices that Loki retains, so every
//! accelerator error permits CPU recomputation. An error that poisons a
//! session, or a contract violation such as a wrong-length result, disables
//! the accelerator for the rest of the process (sticky). Backends never call
//! back into the dispatcher, so fallback cannot recurse.
//!
//! Counters are observational: GPU-required gates assert nonzero
//! `*_accelerated` and zero `*_fallback`; the P1 CPU gates assert routing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ff::{PrimeField, WithSmallOrderMulGroup};
use midnight_curves::CurveAffine;

use super::{
    api::{
        BackendCapabilities, BackendError, ElementOrder, ErrorKind, FailureEffect,
        InverseNormalization, MsmBackend, MsmLengthPolicy, MsmRequest, TransformBackend,
        TransformDirection, TransformSpec,
    },
    arithmetic_abi, cpu, field_abi,
};

/// Snapshot of dispatcher activity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DispatchCounters {
    /// MSMs computed by the stock CPU adapter (including after fallback).
    pub msm_cpu: u64,
    /// MSMs whose accelerator result was published.
    pub msm_accelerated: u64,
    /// MSMs offered to an accelerator that failed; CPU recomputed them.
    pub msm_fallback: u64,
    /// Host transforms computed by the stock CPU adapter.
    pub transform_cpu: u64,
    /// Host transforms whose accelerator result was published.
    pub transform_accelerated: u64,
    /// Transforms offered to an accelerator that failed or violated the
    /// output contract; CPU recomputed them.
    pub transform_fallback: u64,
    /// Number of times the accelerator was sticky-disabled.
    pub accelerator_poisoned: u64,
}

/// Selection/fallback state. Production call sites use [`global`]; tests may
/// construct independent dispatchers.
#[derive(Debug)]
pub struct Dispatcher {
    disabled: AtomicBool,
    msm_cpu: AtomicU64,
    msm_accelerated: AtomicU64,
    msm_fallback: AtomicU64,
    transform_cpu: AtomicU64,
    transform_accelerated: AtomicU64,
    transform_fallback: AtomicU64,
    accelerator_poisoned: AtomicU64,
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}

fn bump(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

impl Dispatcher {
    /// A dispatcher with the accelerator enabled and zeroed counters.
    pub const fn new() -> Self {
        Self {
            disabled: AtomicBool::new(false),
            msm_cpu: AtomicU64::new(0),
            msm_accelerated: AtomicU64::new(0),
            msm_fallback: AtomicU64::new(0),
            transform_cpu: AtomicU64::new(0),
            transform_accelerated: AtomicU64::new(0),
            transform_fallback: AtomicU64::new(0),
            accelerator_poisoned: AtomicU64::new(0),
        }
    }

    /// Current counter values.
    pub fn counters(&self) -> DispatchCounters {
        let get = |c: &AtomicU64| c.load(Ordering::Relaxed);
        DispatchCounters {
            msm_cpu: get(&self.msm_cpu),
            msm_accelerated: get(&self.msm_accelerated),
            msm_fallback: get(&self.msm_fallback),
            transform_cpu: get(&self.transform_cpu),
            transform_accelerated: get(&self.transform_accelerated),
            transform_fallback: get(&self.transform_fallback),
            accelerator_poisoned: get(&self.accelerator_poisoned),
        }
    }

    /// Whether the accelerator has been sticky-disabled.
    pub fn accelerator_disabled(&self) -> bool {
        self.disabled.load(Ordering::Acquire)
    }

    fn poison(&self) {
        if !self.disabled.swap(true, Ordering::AcqRel) {
            bump(&self.accelerator_poisoned);
        }
    }

    /// Account for a failed accelerator call. Inputs are retained host
    /// slices, so CPU recomputation is always permitted.
    fn failed(&self, error: BackendError, fallback: &AtomicU64) {
        assert!(
            error.preserves_observable_state(),
            "backend error without observable-state preservation"
        );
        bump(fallback);
        if error.effect == FailureEffect::SessionPoisoned
            || matches!(error.kind, ErrorKind::ContextLost | ErrorKind::Poisoned)
        {
            self.poison();
        }
    }

    /// Stock `msm_specific` semantics: `min(coeffs.len(), bases.len())`
    /// pairs, zero scalars filtered, identity for an empty prefix.
    pub fn msm<C, B>(
        &self,
        accelerator: Option<(&B, &BackendCapabilities)>,
        coeffs: &[C::Scalar],
        bases: &[C::Curve],
    ) -> C::Curve
    where
        C: CurveAffine,
        B: MsmBackend<C::Scalar, C::Curve> + ?Sized,
    {
        let request = || MsmRequest::new(coeffs, bases, MsmLengthPolicy::ZipTruncate);
        let pairs = coeffs.len().min(bases.len());
        if let Some((backend, caps)) = accelerator {
            let admitted = pairs > 0
                && !self.accelerator_disabled()
                && caps.host_msm
                && arithmetic_abi::<C>().is_some_and(|abi| caps.supports_arithmetic(abi));
            if admitted {
                match backend.msm(request()) {
                    Ok(result) => {
                        bump(&self.msm_accelerated);
                        return result;
                    }
                    Err(error) => self.failed(error, &self.msm_fallback),
                }
            }
        }
        bump(&self.msm_cpu);
        cpu::msm_request::<C>(request()).expect("ZipTruncate never rejects")
    }

    /// Validate `spec` against `values`, then compute it in place. On a CPU
    /// validation error `values` is unchanged and nothing is offered to the
    /// accelerator. An accelerator result replaces `values` only after it is
    /// complete and has the contracted length.
    pub fn transform<F, B>(
        &self,
        accelerator: Option<(&B, &BackendCapabilities)>,
        values: &mut Vec<F>,
        spec: TransformSpec<F>,
    ) -> Result<(), BackendError>
    where
        F: PrimeField,
        B: TransformBackend<F> + ?Sized,
    {
        let output_len = cpu::validate_transform(values.len(), &spec)?;
        if let Some(output) =
            self.try_accelerated_transform(accelerator, values, spec, output_len)?
        {
            *values = output;
            return Ok(());
        }
        bump(&self.transform_cpu);
        cpu::transform_in_place(values, spec)
    }

    /// A whole coset conversion (`coeff_to_extended`, `extended_to_coeff`)
    /// offered as one request. `Ok(None)`: not admitted; the caller runs its
    /// stock body (whose inner NTTs are offered separately). Once offered,
    /// the conversion is settled here: the accelerator result, or after a
    /// failure one CPU computation of the same spec, so the inner NTTs are
    /// never re-offered and the fallback is counted once. `Err` only for a
    /// rejected spec or a cancellation (never retried on CPU).
    pub fn coset_transform<F, B>(
        &self,
        accelerator: Option<(&B, &BackendCapabilities)>,
        values: &[F],
        spec: TransformSpec<F>,
    ) -> Result<Option<Vec<F>>, BackendError>
    where
        F: PrimeField,
        B: TransformBackend<F> + ?Sized,
    {
        let output_len = cpu::validate_transform(values.len(), &spec)?;
        if !self.admits_transform(accelerator.map(|(_, caps)| caps), &spec) {
            return Ok(None);
        }
        if let Some(output) = self.try_accelerated_transform(accelerator, values, spec, output_len)? {
            return Ok(Some(output));
        }
        let mut output = values.to_vec();
        bump(&self.transform_cpu);
        cpu::transform_in_place(&mut output, spec)?;
        Ok(Some(output))
    }

    /// Whether a validated transform may be offered: accelerator enabled,
    /// host transforms and the extent advertised, and the exact arithmetic
    /// ABI the capabilities were queried for (BLS12-381 scalar field with its
    /// G1 curve), not merely a matching scalar field.
    fn admits_transform<F: PrimeField>(
        &self,
        caps: Option<&BackendCapabilities>,
        spec: &TransformSpec<F>,
    ) -> bool {
        let Some(caps) = caps else { return false };
        let largest = spec.source_log_size.max(spec.output_log_size);
        !self.accelerator_disabled()
            && caps.host_transform
            && caps.max_transform_log.is_some_and(|max| largest <= max)
            && transform_abi::<F>().is_some_and(|abi| caps.supports_arithmetic(abi))
    }

    /// Offer a validated transform to the accelerator only. `Ok(None)` means
    /// the caller must compute it on CPU (not admitted, or failed and
    /// accounted). A cancellation is returned as `Err` with the host values
    /// untouched, no CPU retry and no counter change; it sticky-disables the
    /// accelerator only if it also poisoned the session.
    fn try_accelerated_transform<F, B>(
        &self,
        accelerator: Option<(&B, &BackendCapabilities)>,
        values: &[F],
        spec: TransformSpec<F>,
        output_len: usize,
    ) -> Result<Option<Vec<F>>, BackendError>
    where
        F: PrimeField,
        B: TransformBackend<F> + ?Sized,
    {
        let Some((backend, caps)) = accelerator else { return Ok(None) };
        if !self.admits_transform(Some(caps), &spec) {
            return Ok(None);
        }
        match backend.transform(values, spec) {
            Ok(output) if output.len() == output_len => {
                // A result that arrives after another call sticky-disabled the
                // accelerator is not trusted (check-then-use window).
                if self.accelerator_disabled() {
                    bump(&self.transform_fallback);
                    return Ok(None);
                }
                bump(&self.transform_accelerated);
                Ok(Some(output))
            }
            Ok(_) => {
                // Contract violation: discard the tentative output.
                bump(&self.transform_fallback);
                self.poison();
                Ok(None)
            }
            Err(error) if error.kind == ErrorKind::Cancelled => {
                if error.effect == FailureEffect::SessionPoisoned {
                    self.poison();
                }
                Err(error)
            }
            Err(error) => {
                self.failed(error, &self.transform_fallback);
                Ok(None)
            }
        }
    }
}

/// The arithmetic ABI a scalar-field transform is admitted under: the scalar
/// field's own ABI, paired with the curve the capabilities were queried for
/// (BLS12-381 G1; see `run_transform`). `None` for any other field.
fn transform_abi<F: 'static>() -> Option<super::api::ArithmeticAbi> {
    let field = field_abi::<F>()?;
    arithmetic_abi::<midnight_curves::G1Affine>().filter(|abi| abi.scalar_field == field)
}

static GLOBAL: Dispatcher = Dispatcher::new();

/// The process-wide dispatcher used by Loki's call sites.
pub fn global() -> &'static Dispatcher {
    &GLOBAL
}

// ------------------------------------------------------------ selection

/// Uninhabited placeholder when no accelerator is compiled in.
#[cfg(not(feature = "loki-gpu-backend"))]
enum NoAccelerator {}

#[cfg(not(feature = "loki-gpu-backend"))]
impl<F, G> MsmBackend<F, G> for NoAccelerator
where
    F: PrimeField,
    G: group::Curve<Scalar = F>,
{
    fn msm(&self, _request: MsmRequest<'_, F, G>) -> Result<G, BackendError> {
        match *self {}
    }
}

#[cfg(not(feature = "loki-gpu-backend"))]
impl<F: PrimeField> TransformBackend<F> for NoAccelerator {
    fn transform(&self, _values: &[F], _spec: TransformSpec<F>) -> Result<Vec<F>, BackendError> {
        match *self {}
    }
}

/// `msm_specific` feature-on body.
pub(crate) fn msm_specific<C: CurveAffine>(coeffs: &[C::Scalar], bases: &[C::Curve]) -> C::Curve {
    #[cfg(feature = "loki-gpu-backend")]
    {
        use super::api::Backend;
        let gpu = loki_gpu::LokiGpu;
        let caps = <loki_gpu::LokiGpu as Backend<C::Scalar, C::Curve>>::capabilities(&gpu);
        GLOBAL.msm::<C, _>(Some((&gpu, &caps)), coeffs, bases)
    }
    // P3: legacy sppark through the typed adapter (exclusive with
    // `loki-gpu-backend`, see `src/lib.rs`).
    #[cfg(feature = "loki-sppark")]
    {
        let caps = super::sppark::SpparkBackend::capabilities_for(coeffs.len().min(bases.len()));
        GLOBAL.msm::<C, _>(Some((&super::sppark::SpparkBackend, &caps)), coeffs, bases)
    }
    #[cfg(not(any(feature = "loki-gpu-backend", feature = "loki-sppark")))]
    GLOBAL.msm::<C, NoAccelerator>(None, coeffs, bases)
}

fn run_transform<F: PrimeField>(values: &mut Vec<F>, spec: TransformSpec<F>) {
    #[cfg(feature = "loki-gpu-backend")]
    let result = {
        use super::api::Backend;
        let gpu = loki_gpu::LokiGpu;
        // Capabilities are per (field, curve); the native ABI exists only for
        // the BLS12-381 scalar field, so ask with its G1 pairing.
        let caps = field_abi::<F>().map(|_| {
            <loki_gpu::LokiGpu as Backend<midnight_curves::Fq, midnight_curves::G1Projective>>::capabilities(&gpu)
        });
        GLOBAL.transform(caps.as_ref().map(|caps| (&gpu, caps)), values, spec)
    };
    #[cfg(not(feature = "loki-gpu-backend"))]
    let result = GLOBAL.transform::<F, NoAccelerator>(None, values, spec);
    // Stock-signature shim: requests are valid by construction and no caller
    // can cancel (`LokiGpu` host transforms use a private, never-cancelled
    // token) until P7 adds error plumbing to the stock call sites.
    result.expect("stock domain transform request is valid and not cancellable");
}

fn plain_spec<F>(root: F, log_n: u32, direction: TransformDirection) -> TransformSpec<F> {
    TransformSpec {
        root,
        source_log_size: log_n,
        output_log_size: log_n,
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

/// `EvaluationDomain::fft` feature-on body (natural-order forward NTT).
pub(crate) fn fft<F: WithSmallOrderMulGroup<3>>(a: &mut Vec<F>, omega: F, log_n: u32) {
    run_transform(a, plain_spec(omega, log_n, TransformDirection::Forward));
}

/// `EvaluationDomain::ifft` feature-on body. Requests carry the canonical
/// forward root, recovered as the unique inverse of the stock `omega_inv`;
/// the adapter re-inverts it and applies the single `1/n` that equals the
/// stock `divisor`.
pub(crate) fn ifft<F: WithSmallOrderMulGroup<3>>(
    a: &mut Vec<F>,
    omega_inv: F,
    log_n: u32,
    divisor: F,
) {
    debug_assert_eq!(
        divisor * F::from(2).pow_vartime([u64::from(log_n)]),
        F::ONE,
        "stock ifft divisor is 1/n"
    );
    let omega = omega_inv.invert().expect("root of unity is nonzero");
    run_transform(a, plain_spec(omega, log_n, TransformDirection::Inverse));
}

/// Accelerator-only hook for whole coset conversions
/// (`coeff_to_extended`, `extended_to_coeff`). Returns `None` whenever no
/// accelerator takes the request; the caller then runs its stock body, whose
/// NTTs route through [`fft`]/[`ifft`] above.
pub(crate) fn accelerated_coset<F: WithSmallOrderMulGroup<3>>(
    values: &[F],
    spec: TransformSpec<F>,
) -> Option<Vec<F>> {
    #[cfg(feature = "loki-gpu-backend")]
    {
        use super::api::Backend;
        field_abi::<F>()?;
        let gpu = loki_gpu::LokiGpu;
        let caps = <loki_gpu::LokiGpu as Backend<midnight_curves::Fq, midnight_curves::G1Projective>>::capabilities(&gpu);
        // See `run_transform`: stock-signature shim, valid and not cancellable.
        GLOBAL
            .coset_transform(Some((&gpu, &caps)), values, spec)
            .expect("stock coset request is valid and not cancellable")
    }
    #[cfg(not(feature = "loki-gpu-backend"))]
    {
        let _ = (values, spec);
        None
    }
}
