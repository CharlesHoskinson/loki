//! sppark wire types. Limbs are least-significant first, in Montgomery form.
//! Fp uses R=2^384; Fq (sppark Fr) uses R=2^256. No Rust layout casts.
use midnight_curves::{CurveAffine, Fq, G1Affine, G1Projective};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Affine {
    pub(super) x: [u64; 6],
    pub(super) y: [u64; 6],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Scalar(pub(super) [u64; 4]);

pub(super) fn affine(point: &G1Affine) -> Affine {
    Affine {
        x: blst::blst_fp::from(point.x()).l,
        y: blst::blst_fp::from(point.y()).l,
    }
}

pub(super) fn scalar(value: &Fq) -> Scalar {
    Scalar(blst::blst_fr::from(*value).l)
}

pub(super) fn decode(point: &Affine) -> Result<G1Projective, String> {
    if *point == Affine::default() {
        return Ok(G1Projective::default());
    }
    // A Montgomery residue must still be a reduced field element. Fp::from
    // accepts raw BLST limbs without a reduction or canonicality check.
    const P: [u64; 6] = [
        0xb9feffffffffaaab,
        0x1eabfffeb153ffff,
        0x6730d2a0f6b0f624,
        0x64774b84f38512bf,
        0x4b1ba7b6434bacd7,
        0x1a0111ea397fe69a,
    ];
    if point.x.iter().rev().cmp(P.iter().rev()).is_ge()
        || point.y.iter().rev().cmp(P.iter().rev()).is_ge()
    {
        return Err("non-canonical GPU coordinate".into());
    }
    let affine = Option::<G1Affine>::from(G1Affine::from_xy(
        blst::blst_fp { l: point.x }.into(),
        blst::blst_fp { l: point.y }.into(),
    ))
    .ok_or_else(|| "GPU result is outside G1".to_owned())?;
    if !bool::from(affine.is_on_curve()) {
        return Err("GPU result is off curve".into());
    }
    Ok(affine.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ff::Field;
    use group::{prime::PrimeCurveAffine, Curve, Group};
    use midnight_curves::{Fq, G1Affine, G1Projective};
    use num_bigint::BigUint;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    fn montgomery(bytes: &[u8], modulus: &BigUint, bits: usize) -> Vec<u8> {
        let value = (BigUint::from_bytes_le(bytes) << bits) % modulus;
        let mut out = value.to_bytes_le();
        out.resize(bits / 8, 0);
        out
    }

    fn bytes(limbs: &[u64]) -> Vec<u8> {
        limbs.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn cuda_conversion_layout_and_montgomery_constants() {
        assert_eq!(std::mem::size_of::<Affine>(), 96);
        assert_eq!(std::mem::align_of::<Affine>(), 8);
        assert_eq!(std::mem::size_of::<Scalar>(), 32);
        assert_eq!(
            scalar(&Fq::ONE).0,
            [
                0x00000001fffffffe,
                0x5884b7fa00034802,
                0x998c4fefecbc4ff5,
                0x1824b159acc5056f,
            ]
        );
        assert_eq!(affine(&G1Affine::identity()), Affine::default());
        assert_eq!(
            decode(&Affine::default()).unwrap(),
            G1Projective::identity()
        );
    }

    #[test]
    fn cuda_conversion_matches_independent_big_integer_arithmetic() {
        let p = BigUint::parse_bytes(b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab", 16).unwrap();
        let r = BigUint::parse_bytes(
            b"73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001",
            16,
        )
        .unwrap();
        let mut rng = ChaCha20Rng::from_seed([42; 32]);
        for i in 0..1024 {
            let s = match i {
                0 => Fq::ZERO,
                1 => Fq::ONE,
                2 => -Fq::ONE,
                _ => Fq::random(&mut rng),
            };
            assert_eq!(bytes(&scalar(&s).0), montgomery(&s.to_bytes_le(), &r, 256));
            let point = if i == 0 {
                G1Projective::identity()
            } else {
                G1Projective::random(&mut rng)
            };
            let a = point.to_affine();
            let encoded = affine(&a);
            assert_eq!(bytes(&encoded.x), montgomery(&a.x().to_bytes_le(), &p, 384));
            assert_eq!(bytes(&encoded.y), montgomery(&a.y().to_bytes_le(), &p, 384));
            assert_eq!(decode(&encoded).unwrap(), point);
        }
    }

    #[test]
    fn cuda_conversion_rejects_invalid_output() {
        let mut bad = affine(&G1Affine::generator());
        bad.x = [u64::MAX; 6];
        assert!(decode(&bad).is_err());
        bad = Affine::default();
        bad.y[0] = 1;
        assert!(decode(&bad).is_err());
    }
}
