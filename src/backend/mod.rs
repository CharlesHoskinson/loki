//! Optional typed backend seam (P1), compiled only with `loki-backend-api`.
//!
//! With the feature off, Loki runs the stock midnight-proofs 0.8.3 call sites
//! unchanged and this module does not exist. With it on, MSM and host
//! transform call sites build typed `loki-backend-api` requests and pass them
//! to [`dispatch`], which owns selection and fallback:
//!
//! - [`cpu::CpuBackend`] is the stock CPU reference adapter. It runs the
//!   same stock routines (`msm_cpu`, `best_fft`, the stock coset scaling and a
//!   single inverse normalization) and is always the terminal fallback.
//! - `loki-gpu-backend` additionally offers the `loki-gpu` crate as an
//!   accelerator. It is used only when it advertises the operation and the
//!   exact arithmetic ABI; any error preserves inputs and falls back to CPU.
//! - Legacy sppark (`cuda`, `cuda-ntt`, `loki-sppark`) keeps its existing
//!   call-site arms ahead of the dispatcher; it cannot be combined with
//!   `loki-gpu-backend` (compile-time guard in `lib.rs`).
//!
//! Transcript writes, randomness and request ordering stay at the call sites;
//! no backend sees the transcript or the RNG.

pub mod cpu;
pub mod dispatch;

use std::any::TypeId;

pub use loki_backend_api as api;

use api::{
    ArithmeticAbi, CurveAbi, FieldAbi, CURVE_BLS12_381_G1, FIELD_BLS12_381_SCALAR,
    WIRE_ENCODING_V1,
};

/// Stable field wire ABI of a concrete Rust scalar type, if any backend ABI
/// is defined for it. Decided by exact type identity, never by trait bounds.
pub fn field_abi<F: 'static>() -> Option<FieldAbi> {
    (TypeId::of::<F>() == TypeId::of::<midnight_curves::Fq>()).then_some(FieldAbi {
        field_id: FIELD_BLS12_381_SCALAR,
        encoding_version: WIRE_ENCODING_V1,
    })
}

/// Stable scalar/curve wire ABI of a concrete affine curve type, if defined.
/// Only BLS12-381 G1 with its scalar field has a native ABI; every other curve
/// is served by the CPU adapter.
pub fn arithmetic_abi<C: 'static>() -> Option<ArithmeticAbi> {
    (TypeId::of::<C>() == TypeId::of::<midnight_curves::G1Affine>()).then_some(ArithmeticAbi {
        scalar_field: FieldAbi {
            field_id: FIELD_BLS12_381_SCALAR,
            encoding_version: WIRE_ENCODING_V1,
        },
        curve: CurveAbi {
            curve_id: CURVE_BLS12_381_G1,
            encoding_version: WIRE_ENCODING_V1,
        },
    })
}
