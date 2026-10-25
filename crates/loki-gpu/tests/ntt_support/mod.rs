//! P4 transform oracles shared by `ntt_differential` and `resident_ntt`.
//!
//! Three independent references are used:
//!
//! * [`naive_dft`] / [`naive_idft`]: the O(n^2) definition
//!   `out[j] = sum_i a[i] * omega^(i*j)`, written from the formula only;
//! * [`Stock`]: the unmodified crates.io `midnight-proofs 0.8.3`
//!   `EvaluationDomain`, which is the semantics Loki must preserve;
//! * [`PINNED_ROOT_HEX`] / [`PINNED_ZETA_HEX`]: the 2^32-nd root of unity and
//!   the order-three coset generator, pinned as integers so that a change in
//!   either library's constants cannot silently move the domain.
//!
//! A forward/inverse round trip is never used as evidence that a root or coset
//! is right; it is only a negative control (see `wrong_root_round_trip`).

#![allow(dead_code)]

use ff::{Field, PrimeField, WithSmallOrderMulGroup};
use loki_backend_api::{
    CosetSpec, ElementOrder, InverseNormalization, TransformDirection, TransformSpec,
};
use midnight_proofs::poly::EvaluationDomain;
use rand_core::{impls, Error, RngCore};

pub use midnight_curves::Fq as Fr;

/// BLS12-381 scalar modulus `r`, big-endian hex.
pub const MODULUS_HEX: &str = "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001";
/// Stock `Fq::ROOT_OF_UNITY` (order 2^32), big-endian hex. Same value as
/// `LokiLean.Ntt.rootVal`.
pub const PINNED_ROOT_HEX: &str =
    "16a2a19edfe81f20d09b681922c813b4b63683508c2280b93829971f439f0d2b";

/// Parse a big-endian hex integer below 2^256 into a canonical field element.
pub fn fr_from_hex(hex: &str) -> Fr {
    assert_eq!(hex.len(), 64, "expect 32 bytes of hex");
    let mut repr = <Fr as PrimeField>::Repr::default();
    let bytes = repr.as_mut();
    for i in 0..32 {
        // little-endian repr: byte i is hex pair (31 - i)
        let pair = &hex[2 * (31 - i)..2 * (31 - i) + 2];
        bytes[i] = u8::from_str_radix(pair, 16).unwrap();
    }
    Option::from(Fr::from_repr(repr)).expect("canonical")
}

pub fn fr_to_hex(x: &Fr) -> String {
    let repr = x.to_repr();
    repr.as_ref().iter().rev().map(|b| format!("{b:02x}")).collect()
}

/// `r - 1`, the largest canonical representative.
pub fn max_fr() -> Fr {
    -Fr::ONE
}

// ---------------------------------------------------------------------------
// Deterministic synthetic inputs (no witness data).
// ---------------------------------------------------------------------------

/// SplitMix64: a public, seed-only generator for synthetic test vectors.
pub struct SplitMix(pub u64);

impl RngCore for SplitMix {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        impls::fill_bytes_via_next(self, dest)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pattern {
    Random,
    Zero,
    One,
    /// Every slot `r - 1`.
    Max,
    /// `a[1] = 1`, otherwise zero (the root-pinning impulse).
    Impulse,
    /// Cycles 0, 1, r-1, 2, random; exercises reduction boundaries.
    Mixed,
}

pub const ALL_PATTERNS: [Pattern; 6] = [
    Pattern::Random,
    Pattern::Zero,
    Pattern::One,
    Pattern::Max,
    Pattern::Impulse,
    Pattern::Mixed,
];

pub fn input(pattern: Pattern, len: usize, seed: u64) -> Vec<Fr> {
    let mut rng = SplitMix(seed ^ (len as u64).rotate_left(17));
    (0..len)
        .map(|i| match pattern {
            Pattern::Random => Fr::random(&mut rng),
            Pattern::Zero => Fr::ZERO,
            Pattern::One => Fr::ONE,
            Pattern::Max => max_fr(),
            Pattern::Impulse => {
                if i == 1 % len.max(1) {
                    Fr::ONE
                } else {
                    Fr::ZERO
                }
            }
            Pattern::Mixed => match i % 5 {
                0 => Fr::ZERO,
                1 => Fr::ONE,
                2 => max_fr(),
                3 => Fr::from(2),
                _ => Fr::random(&mut rng),
            },
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Naive definitions.
// ---------------------------------------------------------------------------

/// `out[j] = sum_{i<n} a[i] * omega^(i*j)`, evaluated directly. O(n^2).
pub fn naive_dft(a: &[Fr], omega: Fr) -> Vec<Fr> {
    let n = a.len();
    let mut out = Vec::with_capacity(n);
    let mut wj = Fr::ONE; // omega^j
    for _ in 0..n {
        let mut acc = Fr::ZERO;
        let mut wij = Fr::ONE; // omega^(i*j)
        for ai in a {
            acc += *ai * wij;
            wij *= wj;
        }
        out.push(acc);
        wj *= omega;
    }
    out
}

/// `(1/n) * naive_dft(a, omega^-1)`: exactly one normalization.
pub fn naive_idft(a: &[Fr], omega: Fr) -> Vec<Fr> {
    let n_inv = Fr::from(a.len() as u64).invert().unwrap();
    naive_dft(a, omega.invert().unwrap())
        .into_iter()
        .map(|x| x * n_inv)
        .collect()
}

/// Naive coefficient → extended coset: `p(zeta * omega_e^j)` for `j < N`.
pub fn naive_coset_eval(coeffs: &[Fr], zeta: Fr, omega_e: Fr, big_n: usize) -> Vec<Fr> {
    let mut out = Vec::with_capacity(big_n);
    let mut x = zeta;
    for _ in 0..big_n {
        // Horner
        let mut acc = Fr::ZERO;
        for c in coeffs.iter().rev() {
            acc = acc * x + c;
        }
        out.push(acc);
        x *= omega_e;
    }
    out
}

pub fn pow2(x: Fr, j: u32) -> Fr {
    let mut y = x;
    for _ in 0..j {
        y = y.square();
    }
    y
}

/// Whether `w` has multiplicative order exactly `2^log`.
pub fn has_exact_order(w: Fr, log: u32) -> bool {
    if log == 0 {
        return w == Fr::ONE;
    }
    pow2(w, log) == Fr::ONE && pow2(w, log - 1) != Fr::ONE
}

pub fn bitrev(i: usize, log: u32) -> usize {
    if log == 0 {
        0
    } else {
        i.reverse_bits() >> (usize::BITS - log)
    }
}

pub fn bitrev_permute(a: &[Fr]) -> Vec<Fr> {
    let log = a.len().trailing_zeros();
    (0..a.len()).map(|p| a[bitrev(p, log)]).collect()
}

// ---------------------------------------------------------------------------
// Stock domain oracle.
// ---------------------------------------------------------------------------

/// The unmodified stock domain and the exact constants it uses.
pub struct Stock {
    pub domain: EvaluationDomain<Fr>,
    pub k: u32,
    pub extended_k: u32,
    pub omega: Fr,
    pub extended_omega: Fr,
    pub zeta: Fr,
}

impl Stock {
    pub fn new(j: u32, k: u32) -> Self {
        let domain = EvaluationDomain::<Fr>::new(j, k);
        Stock {
            k: domain.k(),
            extended_k: domain.extended_k(),
            omega: domain.get_omega(),
            extended_omega: domain.get_extended_omega(),
            zeta: <Fr as WithSmallOrderMulGroup<3>>::ZETA,
            domain,
        }
    }

    pub fn n(&self) -> usize {
        1 << self.k
    }

    pub fn big_n(&self) -> usize {
        1 << self.extended_k
    }

    pub fn coeff_to_lagrange(&self, a: &[Fr]) -> Vec<Fr> {
        let p = self.domain.coeff_from_vec(a.to_vec());
        self.domain.coeff_to_lagrange(p).to_vec()
    }

    pub fn lagrange_to_coeff(&self, a: &[Fr]) -> Vec<Fr> {
        let p = self.domain.lagrange_from_vec(a.to_vec());
        self.domain.lagrange_to_coeff(p).to_vec()
    }

    pub fn coeff_to_extended(&self, a: &[Fr]) -> Vec<Fr> {
        let p = self.domain.coeff_from_vec(a.to_vec());
        self.domain.coeff_to_extended(p).to_vec()
    }

    pub fn extended_to_coeff(&self, a: &[Fr]) -> Vec<Fr> {
        // Build the extended polynomial through the public constructor chain:
        // stock has no `extended_from_vec`, so start from `empty_extended`.
        let mut p = self.domain.empty_extended();
        p.copy_from_slice(a);
        self.domain.extended_to_coeff(p)
    }

    pub fn extended_to_lagrange(&self, a: &[Fr]) -> Vec<Fr> {
        let mut p = self.domain.empty_extended();
        p.copy_from_slice(a);
        self.domain.extended_to_lagrange(p).to_vec()
    }

    // --- Specifications built by hand from the stock constants. These are
    // deliberately independent of `loki_gpu::ntt::DomainTransforms`.

    pub fn spec_coeff_to_lagrange(&self) -> TransformSpec<Fr> {
        plain(self.omega, self.k, TransformDirection::Forward)
    }

    pub fn spec_lagrange_to_coeff(&self) -> TransformSpec<Fr> {
        plain(self.omega, self.k, TransformDirection::Inverse)
    }

    pub fn spec_coeff_to_extended(&self) -> TransformSpec<Fr> {
        TransformSpec {
            root: self.extended_omega,
            source_log_size: self.k,
            output_log_size: self.extended_k,
            direction: TransformDirection::Forward,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: InverseNormalization::None,
            coset: Some(CosetSpec {
                generator: self.zeta,
                generator_inverse: self.zeta.square(),
            }),
        }
    }

    pub fn spec_extended_to_coeff(&self) -> TransformSpec<Fr> {
        self.spec_extended_to_coeff_prefix(self.extended_k)
    }

    /// Inverse coset transform keeping the first `2^out_log` coefficients.
    pub fn spec_extended_to_coeff_prefix(&self, out_log: u32) -> TransformSpec<Fr> {
        TransformSpec {
            root: self.extended_omega,
            source_log_size: self.extended_k,
            output_log_size: out_log,
            direction: TransformDirection::Inverse,
            input_order: ElementOrder::Natural,
            output_order: ElementOrder::Natural,
            inverse_normalization: InverseNormalization::DivideByLengthOnce,
            // Same coset as the forward spec: the API multiplies coefficient
            // `i` by `generator_inverse^i` after the inverse transform.
            coset: Some(CosetSpec {
                generator: self.zeta,
                generator_inverse: self.zeta.square(),
            }),
        }
    }
}

pub fn plain(root: Fr, log: u32, direction: TransformDirection) -> TransformSpec<Fr> {
    TransformSpec {
        root,
        source_log_size: log,
        output_log_size: log,
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

/// Stock-style multiplicative root of order `2^log` derived from
/// `ROOT_OF_UNITY` by squaring, as `EvaluationDomain::new` does.
pub fn stock_root(log: u32) -> Fr {
    pow2(Fr::ROOT_OF_UNITY, Fr::S - log)
}

/// First index where two vectors differ, for readable assertion failures.
pub fn first_mismatch(a: &[Fr], b: &[Fr]) -> Option<usize> {
    if a.len() != b.len() {
        return Some(a.len().min(b.len()));
    }
    a.iter().zip(b).position(|(x, y)| x != y)
}

#[track_caller]
pub fn assert_vec_eq(label: &str, got: &[Fr], want: &[Fr]) {
    if let Some(i) = first_mismatch(got, want) {
        panic!(
            "{label}: mismatch at {i} (len got {} want {}): got {:?} want {:?}",
            got.len(),
            want.len(),
            got.get(i).map(fr_to_hex),
            want.get(i).map(fr_to_hex)
        );
    }
}
