//! P3 concrete point codec for the new backend's SRS registry
//! (`loki-gpu-srs-registry`).
//!
//! `loki-gpu` cannot name `midnight_curves::G1Projective`, and orphan rules
//! forbid implementing its codec trait for that foreign type here, so the
//! codec is this local zero-sized type, bound to exactly one point type, one
//! curve ABI and one wire version. A registry built with it can never
//! reinterpret leases: the type parameter, `curve_abi` and `wire_bytes` are
//! fixed at construction.
//!
//! Wire format (`WIRE_ENCODING_V1`, as in `loki-gpu` `design/MSM.md`): 96
//! bytes per point, affine `x[6] || y[6]` little-endian `u64` limbs in BLST's
//! Montgomery representation (R = 2^384); the identity is `(0, 0)`. This is
//! the same encoding as the legacy `poly::kzg::msm::cuda::convert::affine`.
//! No Rust layout cast is involved.
//!
//! Validation is the W13 whole-request check, shared with the legacy registry
//! (`poly::kzg::msm::srs_validate`): curve equation, 512-point serial
//! normalization sub-batches on at most four native workers, then subgroup
//! membership. Exact hits compare every projective x/y/z limb.

use std::sync::{Arc, OnceLock};

use loki_gpu::srs::{GpuSrsRegistry, SrsPointCodec, SrsPointError};
use midnight_curves::{G1Affine, G1Projective};

use super::api::{BackendError, CurveAbi};
use crate::poly::kzg::msm::srs_validate::{same_coordinates, validate, Rejection};

/// Device wire bytes per BLS12-381 G1 point.
pub const G1_WIRE_BYTES: usize = 96;

/// BLS12-381 G1 codec for `loki_gpu::srs::GpuSrsRegistry`.
#[derive(Clone, Copy, Debug, Default)]
pub struct MidnightG1Codec;

impl MidnightG1Codec {
    /// Curve wire ABI tag of this codec.
    pub fn abi() -> CurveAbi {
        super::arithmetic_abi::<G1Affine>()
            .expect("BLS12-381 G1 has a native ABI")
            .curve
    }

    /// Encode one validated affine point.
    pub fn encode_affine(point: &G1Affine, out: &mut [u8]) {
        let x = blst::blst_fp::from(point.x()).l;
        let y = blst::blst_fp::from(point.y()).l;
        for (i, limb) in x.iter().chain(y.iter()).enumerate() {
            out[8 * i..8 * i + 8].copy_from_slice(&limb.to_le_bytes());
        }
    }
}

impl SrsPointCodec<G1Projective> for MidnightG1Codec {
    fn curve_abi(&self) -> CurveAbi {
        Self::abi()
    }

    fn wire_bytes(&self) -> usize {
        G1_WIRE_BYTES
    }

    fn validate_encode(&self, points: &[G1Projective], wire: &mut [u8]) -> Result<(), SrsPointError> {
        if wire.len() != points.len() * G1_WIRE_BYTES {
            return Err(SrsPointError::Malformed { index: 0 });
        }
        let affine = validate(points).map_err(|r| match r {
            Rejection::OffCurve { index } => SrsPointError::OffCurve { index },
            Rejection::NotInSubgroup { index } => SrsPointError::NotInSubgroup { index },
            Rejection::Worker => SrsPointError::Malformed { index: 0 },
        })?;
        for (point, out) in affine.iter().zip(wire.chunks_exact_mut(G1_WIRE_BYTES)) {
            Self::encode_affine(point, out);
        }
        Ok(())
    }

    fn same_representation(&self, a: &G1Projective, b: &G1Projective) -> bool {
        same_coordinates(a, b)
    }
}

/// Process-wide registry on the shared `loki-gpu` runtime, with the
/// `LOKI_GPU_SRS_BUDGET_BYTES` budget (default 4 GiB). An invalid budget
/// value is an error, never a silently enlarged or disabled cache.
pub fn registry() -> Result<&'static GpuSrsRegistry<G1Projective>, BackendError> {
    static REGISTRY: OnceLock<Result<GpuSrsRegistry<G1Projective>, BackendError>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| GpuSrsRegistry::from_environment(Arc::new(MidnightG1Codec)))
        .as_ref()
        .map_err(|e| *e)
}

#[cfg(test)]
mod tests {
    #![allow(unsafe_code)]
    use super::*;
    use ff::Field;
    use group::{prime::PrimeCurveAffine, Curve, Group};
    use midnight_curves::{CurveExt, Fp};
    use num_bigint::BigUint;
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    fn corrupt() -> G1Projective {
        let raw = blst::blst_p1 {
            x: Fp::ONE.into(),
            y: Fp::ONE.into(),
            z: Fp::ONE.into(),
        };
        // SAFETY: G1Projective is repr(transparent) over blst_p1; reduced limbs.
        unsafe { std::mem::transmute(raw) }
    }

    fn encode(points: &[G1Projective]) -> Result<Vec<u8>, SrsPointError> {
        let mut wire = vec![0u8; points.len() * G1_WIRE_BYTES];
        MidnightG1Codec.validate_encode(points, &mut wire)?;
        Ok(wire)
    }

    #[test]
    fn g1_codec_matches_independent_montgomery_encoding() {
        let p = BigUint::parse_bytes(b"1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab", 16).unwrap();
        let mont = |bytes: &[u8]| {
            let shifted: BigUint = BigUint::from_bytes_le(bytes) << 384usize;
            let mut out = (shifted % &p).to_bytes_le();
            out.resize(48, 0);
            out
        };
        let mut rng = ChaCha20Rng::from_seed([9; 32]);
        let mut points: Vec<G1Projective> = (0..300).map(|_| G1Projective::random(&mut rng)).collect();
        points[0] = G1Projective::identity();
        let wire = encode(&points).unwrap();
        for (point, w) in points.iter().zip(wire.chunks_exact(96)) {
            let a = point.to_affine();
            if bool::from(a.is_identity()) {
                assert_eq!(w, &[0u8; 96][..], "identity is (0, 0)");
                continue;
            }
            assert_eq!(&w[..48], &mont(&a.x().to_bytes_le())[..]);
            assert_eq!(&w[48..], &mont(&a.y().to_bytes_le())[..]);
        }
        assert_eq!(MidnightG1Codec.curve_abi(), MidnightG1Codec::abi());
        assert_eq!(MidnightG1Codec.wire_bytes(), 96);
    }

    #[test]
    fn g1_codec_rejects_corrupt_off_curve_and_torsion_points() {
        let mut rng = ChaCha20Rng::from_seed([10; 32]);
        let g = G1Projective::random(&mut rng);
        for n in [16usize, 5000] {
            let mut pts = vec![g; n];
            pts[n - 1] = corrupt();
            assert_eq!(encode(&pts), Err(SrsPointError::OffCurve { index: n - 1 }));
            // (0, 2) is on y^2 = x^3 + 4 but has order 3, outside prime-order G1.
            let torsion = G1Projective::new_jacobian(Fp::ZERO, Fp::from(2), Fp::ONE).unwrap();
            assert!(bool::from(torsion.is_on_curve()));
            pts[n - 1] = g;
            pts[n / 2] = torsion;
            assert_eq!(encode(&pts), Err(SrsPointError::NotInSubgroup { index: n / 2 }));
        }
        assert!(MidnightG1Codec.same_representation(&g, &g));
        assert!(!MidnightG1Codec.same_representation(&g, &(g + g)));
        let mut short = vec![0u8; 95];
        assert!(MidnightG1Codec.validate_encode(&[g], &mut short).is_err());
    }

    /// k19/k20 coefficient and Lagrange arrays on the device through the
    /// shared runtime; every lease's resident bytes equal the codec encoding
    /// of the exact requested points. Prints cold/warm figures.
    #[test]
    #[ignore = "requires a GPU; run under the GPU lock with --ignored --nocapture"]
    fn gpu_srs_registry_k19_k20_on_device() {
        use loki_gpu::api::{ResidentBackend, SrsBasis, SrsCacheKey, SrsRegistry, SrsRequest};
        use midnight_curves::Fq;
        use std::time::Instant;

        loki_gpu::LokiGpu.initialize().expect("device");
        let rt = loki_gpu::runtime::GpuRuntime::process();
        let generation = ResidentBackend::<Fq, G1Projective>::generation(rt).unwrap();
        let reg = registry().unwrap();
        let r: &dyn SrsRegistry<Fq, G1Projective> = reg;
        let key = |basis, k, n| SrsCacheKey {
            curve_abi: MidnightG1Codec::abi(),
            basis,
            domain_log: k,
            point_count: n,
            generation,
            content_identity: [0; 32],
        };
        let mut rng = ChaCha20Rng::from_seed([113; 32]);
        let base: Vec<G1Projective> = (0..1024).map(|_| G1Projective::random(&mut rng)).collect();
        let array = |n: usize, shift: usize| -> Vec<G1Projective> {
            (0..n).map(|i| base[(i * 7 + shift) % 1024]).collect()
        };
        let coeff20 = array(1 << 20, 0);
        let lag20 = array(1 << 20, 1);
        let lag19 = array(1 << 19, 2);
        let mut log = Vec::new();
        let mut run = |label: &str, basis, k: u32, pts: &[G1Projective]| {
            let t = Instant::now();
            let lease = r.acquire_srs(SrsRequest { key: key(basis, k, pts.len()), points: pts }).unwrap();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            let wire = reg.resident_wire(&lease).unwrap();
            let mut expected = vec![0u8; pts.len() * 96];
            let affine: Vec<G1Affine> = pts.iter().map(|p| p.to_affine()).collect();
            for (a, out) in affine.iter().zip(expected.chunks_exact_mut(96)) {
                MidnightG1Codec::encode_affine(a, out);
            }
            assert_eq!(wire, expected, "{label}: resident bytes are the requested bases");
            log.push(format!("{label}: {ms:.3} ms"));
            lease
        };
        let leases = vec![
            run("cold coeff k20", SrsBasis::Coefficient, 20, &coeff20),
            run("cold lagrange k20", SrsBasis::Lagrange, 20, &lag20),
            run("cold lagrange k19", SrsBasis::Lagrange, 19, &lag19),
            run("warm coeff k19 prefix", SrsBasis::Coefficient, 19, &coeff20[..1 << 19]),
            run("warm coeff k20 n-1", SrsBasis::Coefficient, 20, &coeff20[..(1 << 20) - 1]),
            run("miss lagrange k19 = k20 prefix", SrsBasis::Lagrange, 19, &lag20[..1 << 19]),
            run("warm lagrange k20 12345", SrsBasis::Lagrange, 20, &lag20[..12345]),
        ];
        let mut mutated = coeff20.clone();
        mutated[777] = base[3] + base[4];
        let m = run("miss coeff k20 mutated", SrsBasis::Coefficient, 20, &mutated);
        let mut bad = lag19.clone();
        bad[5] = corrupt();
        let rejected = r.acquire_srs(SrsRequest { key: key(SrsBasis::Lagrange, 19, bad.len()), points: &bad });
        assert_eq!(rejected.err().map(|e| e.kind), Some(loki_gpu::api::ErrorKind::InvalidInput));
        for l in leases {
            r.release_srs(l).unwrap();
        }
        r.release_srs(m).unwrap();
        let s = reg.stats();
        for line in &log {
            eprintln!("P3 gpu_srs {line}");
        }
        eprintln!("P3 gpu_srs stats {s:?}");
        eprintln!("P3 gpu_srs runtime {:?}", rt.snapshot());
        assert_eq!((s.uploads, s.hits, s.rejected_invalid), (5, 3, 1));
        assert_eq!(s.resident_bytes, ((3u64 << 20) + (2 << 19)) * 96);
    }
}
