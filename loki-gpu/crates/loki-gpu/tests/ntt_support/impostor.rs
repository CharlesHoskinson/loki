//! `Impostor`: BLS12-381 Fr arithmetic (delegated to `midnight_curves::Fq`)
//! with a *different* bijective representation: bytes 1 and 2 of the
//! canonical little-endian encoding are swapped. It agrees with the canonical
//! encoding on the four wire samples (0, 1, 2^64 and r - 1 all have zero
//! bytes 1 and 2) and on modulus and width, yet differs for e.g. 256. The
//! NTT wire gate must refuse it by type (review finding N2).

use std::iter::{Product, Sum};
use std::ops::{Add, AddAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use ff::{Field, PrimeField};
use rand_core::RngCore;
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq, CtOption};

type Fq = midnight_curves::Fq;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Impostor(pub Fq);

fn swap12(mut b: [u8; 32]) -> [u8; 32] {
    b.swap(1, 2);
    b
}

impl ConstantTimeEq for Impostor {
    fn ct_eq(&self, other: &Self) -> Choice {
        self.0.ct_eq(&other.0)
    }
}
impl ConditionallySelectable for Impostor {
    fn conditional_select(a: &Self, b: &Self, c: Choice) -> Self {
        Impostor(Fq::conditional_select(&a.0, &b.0, c))
    }
}
impl Neg for Impostor {
    type Output = Self;
    fn neg(self) -> Self {
        Impostor(-self.0)
    }
}

macro_rules! binop {
    ($tr:ident, $f:ident, $atr:ident, $af:ident) => {
        impl $tr for Impostor {
            type Output = Self;
            fn $f(self, r: Self) -> Self {
                Impostor($tr::$f(self.0, r.0))
            }
        }
        impl<'a> $tr<&'a Impostor> for Impostor {
            type Output = Self;
            fn $f(self, r: &'a Self) -> Self {
                Impostor($tr::$f(self.0, r.0))
            }
        }
        impl $atr for Impostor {
            fn $af(&mut self, r: Self) {
                $atr::$af(&mut self.0, r.0)
            }
        }
        impl<'a> $atr<&'a Impostor> for Impostor {
            fn $af(&mut self, r: &'a Self) {
                $atr::$af(&mut self.0, r.0)
            }
        }
    };
}
binop!(Add, add, AddAssign, add_assign);
binop!(Sub, sub, SubAssign, sub_assign);
binop!(Mul, mul, MulAssign, mul_assign);

impl Sum for Impostor {
    fn sum<I: Iterator<Item = Self>>(it: I) -> Self {
        it.fold(Self::ZERO, |a, b| a + b)
    }
}
impl<'a> Sum<&'a Impostor> for Impostor {
    fn sum<I: Iterator<Item = &'a Self>>(it: I) -> Self {
        it.fold(Self::ZERO, |a, b| a + b)
    }
}
impl Product for Impostor {
    fn product<I: Iterator<Item = Self>>(it: I) -> Self {
        it.fold(Self::ONE, |a, b| a * b)
    }
}
impl<'a> Product<&'a Impostor> for Impostor {
    fn product<I: Iterator<Item = &'a Self>>(it: I) -> Self {
        it.fold(Self::ONE, |a, b| a * b)
    }
}
impl From<u64> for Impostor {
    fn from(v: u64) -> Self {
        Impostor(Fq::from(v))
    }
}

impl Field for Impostor {
    const ZERO: Self = Impostor(Fq::ZERO);
    const ONE: Self = Impostor(Fq::ONE);
    fn random(rng: impl RngCore) -> Self {
        Impostor(Fq::random(rng))
    }
    fn square(&self) -> Self {
        Impostor(self.0.square())
    }
    fn double(&self) -> Self {
        Impostor(self.0.double())
    }
    fn invert(&self) -> CtOption<Self> {
        self.0.invert().map(Impostor)
    }
    fn sqrt_ratio(num: &Self, div: &Self) -> (Choice, Self) {
        let (c, r) = Fq::sqrt_ratio(&num.0, &div.0);
        (c, Impostor(r))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImpostorRepr(pub [u8; 32]);
impl AsRef<[u8]> for ImpostorRepr {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl AsMut<[u8]> for ImpostorRepr {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl PrimeField for Impostor {
    type Repr = ImpostorRepr;
    fn from_repr(r: Self::Repr) -> CtOption<Self> {
        let mut canonical = <Fq as PrimeField>::Repr::default();
        canonical.as_mut().copy_from_slice(&swap12(r.0));
        Fq::from_repr(canonical).map(Impostor)
    }
    fn to_repr(&self) -> Self::Repr {
        let mut b = [0u8; 32];
        b.copy_from_slice(self.0.to_repr().as_ref());
        ImpostorRepr(swap12(b))
    }
    fn is_odd(&self) -> Choice {
        self.0.is_odd()
    }
    const MODULUS: &'static str = <Fq as PrimeField>::MODULUS;
    const NUM_BITS: u32 = <Fq as PrimeField>::NUM_BITS;
    const CAPACITY: u32 = <Fq as PrimeField>::CAPACITY;
    const TWO_INV: Self = Impostor(<Fq as PrimeField>::TWO_INV);
    const MULTIPLICATIVE_GENERATOR: Self = Impostor(<Fq as PrimeField>::MULTIPLICATIVE_GENERATOR);
    const S: u32 = <Fq as PrimeField>::S;
    const ROOT_OF_UNITY: Self = Impostor(<Fq as PrimeField>::ROOT_OF_UNITY);
    const ROOT_OF_UNITY_INV: Self = Impostor(<Fq as PrimeField>::ROOT_OF_UNITY_INV);
    const DELTA: Self = Impostor(<Fq as PrimeField>::DELTA);
}
