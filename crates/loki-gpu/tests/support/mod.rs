//! Test-only arithmetic fixtures shared by the P2 runtime tests.
//!
//! `Toy` is the Goldilocks prime field p = 2^64 - 2^32 + 1 implemented
//! directly from its definition (u128 reduction). `ToyPoint` is the additive
//! group of that field, used only to instantiate the generic
//! `ResidentBackend<F, G>` trait. Neither type is cryptographic; they exist so
//! the runtime's typed upload/download/registry checks run without pulling a
//! production curve crate into the shared lockfile.

#![allow(dead_code)]

use core::fmt;
use core::iter::{Product, Sum};
use core::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use ff::{Field, PrimeField};
use group::{Curve, Group};
use rand_core::RngCore;
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq, CtOption};

pub const P: u64 = 0xffff_ffff_0000_0001;

#[derive(Clone, Copy, Default, Eq, PartialEq, Hash)]
pub struct Toy(pub u64);

impl fmt::Debug for Toy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Toy({:#x})", self.0)
    }
}

impl Toy {
    pub const fn new(v: u64) -> Self {
        Toy(v % P)
    }
    fn mul_raw(a: u64, b: u64) -> u64 {
        ((a as u128 * b as u128) % P as u128) as u64
    }
    fn pow_u64(self, mut e: u64) -> Self {
        let mut acc = 1u64;
        let mut base = self.0;
        while e > 0 {
            if e & 1 == 1 {
                acc = Self::mul_raw(acc, base);
            }
            base = Self::mul_raw(base, base);
            e >>= 1;
        }
        Toy(acc)
    }
}

impl ConstantTimeEq for Toy {
    fn ct_eq(&self, other: &Self) -> Choice {
        self.0.ct_eq(&other.0)
    }
}
impl ConditionallySelectable for Toy {
    fn conditional_select(a: &Self, b: &Self, choice: Choice) -> Self {
        Toy(u64::conditional_select(&a.0, &b.0, choice))
    }
}

impl Add for Toy {
    type Output = Toy;
    fn add(self, rhs: Toy) -> Toy {
        Toy(((self.0 as u128 + rhs.0 as u128) % P as u128) as u64)
    }
}
impl Sub for Toy {
    type Output = Toy;
    fn sub(self, rhs: Toy) -> Toy {
        self + (-rhs)
    }
}
impl Mul for Toy {
    type Output = Toy;
    fn mul(self, rhs: Toy) -> Toy {
        Toy(Toy::mul_raw(self.0, rhs.0))
    }
}
impl Neg for Toy {
    type Output = Toy;
    fn neg(self) -> Toy {
        if self.0 == 0 {
            self
        } else {
            Toy(P - self.0)
        }
    }
}

macro_rules! ref_ops {
    ($t:ty, $rhs:ty) => {
        impl<'a> Add<&'a $rhs> for $t {
            type Output = $t;
            fn add(self, rhs: &'a $rhs) -> $t {
                self + *rhs
            }
        }
        impl<'a> Sub<&'a $rhs> for $t {
            type Output = $t;
            fn sub(self, rhs: &'a $rhs) -> $t {
                self - *rhs
            }
        }
        impl AddAssign<$rhs> for $t {
            fn add_assign(&mut self, rhs: $rhs) {
                *self = *self + rhs;
            }
        }
        impl SubAssign<$rhs> for $t {
            fn sub_assign(&mut self, rhs: $rhs) {
                *self = *self - rhs;
            }
        }
        impl<'a> AddAssign<&'a $rhs> for $t {
            fn add_assign(&mut self, rhs: &'a $rhs) {
                *self = *self + *rhs;
            }
        }
        impl<'a> SubAssign<&'a $rhs> for $t {
            fn sub_assign(&mut self, rhs: &'a $rhs) {
                *self = *self - *rhs;
            }
        }
    };
}
ref_ops!(Toy, Toy);

impl<'a> Mul<&'a Toy> for Toy {
    type Output = Toy;
    fn mul(self, rhs: &'a Toy) -> Toy {
        self * *rhs
    }
}
impl MulAssign for Toy {
    fn mul_assign(&mut self, rhs: Toy) {
        *self = *self * rhs;
    }
}
impl<'a> MulAssign<&'a Toy> for Toy {
    fn mul_assign(&mut self, rhs: &'a Toy) {
        *self = *self * *rhs;
    }
}
impl Sum for Toy {
    fn sum<I: Iterator<Item = Toy>>(iter: I) -> Toy {
        iter.fold(Toy(0), |a, b| a + b)
    }
}
impl<'a> Sum<&'a Toy> for Toy {
    fn sum<I: Iterator<Item = &'a Toy>>(iter: I) -> Toy {
        iter.fold(Toy(0), |a, b| a + *b)
    }
}
impl Product for Toy {
    fn product<I: Iterator<Item = Toy>>(iter: I) -> Toy {
        iter.fold(Toy(1), |a, b| a * b)
    }
}
impl<'a> Product<&'a Toy> for Toy {
    fn product<I: Iterator<Item = &'a Toy>>(iter: I) -> Toy {
        iter.fold(Toy(1), |a, b| a * *b)
    }
}
impl From<u64> for Toy {
    fn from(v: u64) -> Toy {
        Toy::new(v)
    }
}

impl Field for Toy {
    const ZERO: Self = Toy(0);
    const ONE: Self = Toy(1);
    fn random(mut rng: impl RngCore) -> Self {
        Toy::new(rng.next_u64())
    }
    fn square(&self) -> Self {
        *self * *self
    }
    fn double(&self) -> Self {
        *self + *self
    }
    fn invert(&self) -> CtOption<Self> {
        CtOption::new(self.pow_u64(P - 2), !self.ct_eq(&Toy(0)))
    }
    fn sqrt_ratio(_num: &Self, _div: &Self) -> (Choice, Self) {
        unimplemented!("square roots are not needed by runtime tests")
    }
}

/// Canonical 8-byte little-endian representation. `from_repr` rejects
/// non-canonical values, which the runtime tests use to check that download
/// decoding validates bytes before publishing a vector.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct ToyRepr(pub [u8; 8]);
impl AsRef<[u8]> for ToyRepr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsMut<[u8]> for ToyRepr {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl PrimeField for Toy {
    type Repr = ToyRepr;
    fn from_repr(repr: ToyRepr) -> CtOption<Self> {
        let v = u64::from_le_bytes(repr.0);
        CtOption::new(Toy(v), Choice::from((v < P) as u8))
    }
    fn to_repr(&self) -> ToyRepr {
        ToyRepr(self.0.to_le_bytes())
    }
    fn is_odd(&self) -> Choice {
        Choice::from((self.0 & 1) as u8)
    }
    const MODULUS: &'static str = "0xffffffff00000001";
    const NUM_BITS: u32 = 64;
    const CAPACITY: u32 = 63;
    const TWO_INV: Self = Toy(0x7fff_ffff_8000_0001);
    const MULTIPLICATIVE_GENERATOR: Self = Toy(7);
    const S: u32 = 32;
    const ROOT_OF_UNITY: Self = Toy(0x1856_29dc_da58_878c);
    const ROOT_OF_UNITY_INV: Self = Toy(0x76b6_b635_b6fc_8719);
    const DELTA: Self = Toy(0xaa5b_2509_f86b_b4d4);
}

/// Additive group of `Toy`; scalar multiplication is field multiplication.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct ToyPoint(pub Toy);

impl Add for ToyPoint {
    type Output = ToyPoint;
    fn add(self, rhs: ToyPoint) -> ToyPoint {
        ToyPoint(self.0 + rhs.0)
    }
}
impl Sub for ToyPoint {
    type Output = ToyPoint;
    fn sub(self, rhs: ToyPoint) -> ToyPoint {
        ToyPoint(self.0 - rhs.0)
    }
}
impl Neg for ToyPoint {
    type Output = ToyPoint;
    fn neg(self) -> ToyPoint {
        ToyPoint(-self.0)
    }
}
ref_ops!(ToyPoint, ToyPoint);
impl Mul<Toy> for ToyPoint {
    type Output = ToyPoint;
    fn mul(self, rhs: Toy) -> ToyPoint {
        ToyPoint(self.0 * rhs)
    }
}
impl<'a> Mul<&'a Toy> for ToyPoint {
    type Output = ToyPoint;
    fn mul(self, rhs: &'a Toy) -> ToyPoint {
        ToyPoint(self.0 * *rhs)
    }
}
impl MulAssign<Toy> for ToyPoint {
    fn mul_assign(&mut self, rhs: Toy) {
        self.0 *= rhs;
    }
}
impl<'a> MulAssign<&'a Toy> for ToyPoint {
    fn mul_assign(&mut self, rhs: &'a Toy) {
        self.0 *= *rhs;
    }
}
impl Sum for ToyPoint {
    fn sum<I: Iterator<Item = ToyPoint>>(iter: I) -> ToyPoint {
        iter.fold(ToyPoint(Toy(0)), |a, b| a + b)
    }
}
impl<'a> Sum<&'a ToyPoint> for ToyPoint {
    fn sum<I: Iterator<Item = &'a ToyPoint>>(iter: I) -> ToyPoint {
        iter.fold(ToyPoint(Toy(0)), |a, b| a + *b)
    }
}

impl Group for ToyPoint {
    type Scalar = Toy;
    fn random(rng: impl RngCore) -> Self {
        ToyPoint(Toy::random(rng))
    }
    fn identity() -> Self {
        ToyPoint(Toy(0))
    }
    fn generator() -> Self {
        ToyPoint(Toy(1))
    }
    fn is_identity(&self) -> Choice {
        self.0.ct_eq(&Toy(0))
    }
    fn double(&self) -> Self {
        ToyPoint(self.0.double())
    }
}

impl Curve for ToyPoint {
    type AffineRepr = ToyPoint;
    fn to_affine(&self) -> ToyPoint {
        *self
    }
}

/// Deterministic public synthetic scalars (no witness data).
pub fn scalars(n: usize, seed: u64) -> Vec<Toy> {
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    (0..n)
        .map(|_| {
            // splitmix64
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            Toy::new(z ^ (z >> 31))
        })
        .collect()
}
