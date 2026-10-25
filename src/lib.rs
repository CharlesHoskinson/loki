//! # midnight_proofs

#![cfg_attr(docsrs, feature(doc_cfg))]
// The actual lints we want to disable.
#![allow(clippy::op_ref, clippy::many_single_char_names)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![deny(unsafe_code)]

// P1 accelerator exclusivity: the new backend owns its device runtime and
// cannot be combined with the legacy sppark runtime, whether selected
// directly or through `loki-sppark`.
#[cfg(all(feature = "loki-gpu-backend", any(feature = "cuda", feature = "cuda-ntt")))]
compile_error!(
    "`loki-gpu-backend` is mutually exclusive with the legacy `cuda`/`cuda-ntt` \
     features (including `loki-sppark`)"
);

#[cfg(feature = "loki-backend-api")]
pub mod backend;
pub mod circuit;
pub mod plonk;
pub mod poly;
#[cfg(feature = "loki-profile")]
pub mod profile;
#[cfg(any(feature = "cuda", feature = "cuda-ntt", feature = "loki-backend-api"))]
pub mod telemetry;
pub mod transcript;

pub mod dev;
pub mod utils;
