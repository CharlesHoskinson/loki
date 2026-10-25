//! Differential checks of Loki (the parent `midnight-proofs` path package)
//! against unmodified crates.io `midnight-proofs` 0.8.3 (`stock`) and naive
//! mathematical definitions.
//!
//! Every check is a function of a byte string, so the libFuzzer targets in
//! `fuzz/fuzz_targets` and the stable proptest gate in `tests/quick.rs` run
//! exactly the same code. A check panics when an invariant is violated.

pub mod circuit;
pub mod msm;
pub mod mutation;
pub mod ntt;
pub mod parse;
pub mod proof;

use ff::{Field, PrimeField};
use midnight_curves::Fq;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

/// Byte source: consumes fuzz input first, then a ChaCha stream seeded by the
/// input, so short inputs still describe large cases deterministically.
pub struct Input<'a> {
    data: &'a [u8],
    pos: usize,
    rng: ChaCha20Rng,
}

impl<'a> Input<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        let mut seed = [0x4c; 32];
        for (i, b) in data.iter().take(32).enumerate() {
            seed[i] ^= *b;
        }
        seed[31] ^= data.len() as u8;
        Self {
            data,
            pos: 0,
            rng: ChaCha20Rng::from_seed(seed),
        }
    }

    pub fn exhausted(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn u8(&mut self) -> u8 {
        if self.pos < self.data.len() {
            self.pos += 1;
            self.data[self.pos - 1]
        } else {
            self.rng.next_u32() as u8
        }
    }

    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    pub fn u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        for x in &mut b {
            *x = self.u8();
        }
        u64::from_le_bytes(b)
    }

    pub fn bool(&mut self) -> bool {
        self.u8() & 1 == 1
    }

    /// Uniform-ish in `lo..=hi`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        assert!(lo <= hi);
        let span = (hi - lo + 1) as u64;
        lo + (self.u64() % span) as usize
    }

    pub fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut b = [0u8; N];
        for x in &mut b {
            *x = self.u8();
        }
        b
    }

    /// A random field element from the stream (not from fuzz bytes).
    pub fn rng_scalar(&mut self) -> Fq {
        Fq::random(&mut self.rng)
    }

    pub fn rng(&mut self) -> &mut ChaCha20Rng {
        &mut self.rng
    }

    /// A scalar biased toward edge cases: 0, 1, -1, small, top-window values
    /// (2^254..r), r-1-small and uniform.
    pub fn scalar(&mut self) -> Fq {
        match self.u8() % 12 {
            0 => Fq::ZERO,
            1 => Fq::ONE,
            2 => -Fq::ONE,
            3 => Fq::from(self.u8() as u64),
            4 => Fq::from(self.u64()),
            5 => -Fq::from(self.u16() as u64 + 1),
            6 => two_pow(254) + Fq::from(self.u64()),
            7 => two_pow(254) + two_pow(253) + Fq::from(self.u64()),
            8 => two_pow(self.range(0, 254) as u32),
            9 => {
                // A canonical encoding taken from the fuzz bytes when < r.
                let mut repr = <Fq as PrimeField>::Repr::default();
                let b: [u8; 32] = self.bytes();
                repr.as_mut().copy_from_slice(&b);
                Option::from(Fq::from_repr(repr)).unwrap_or_else(|| self.rng_scalar())
            }
            _ => self.rng_scalar(),
        }
    }
}

pub fn two_pow(e: u32) -> Fq {
    Fq::from(2).pow_vartime([e as u64])
}

/// Raw limbs of a Montgomery residue are fully reduced (< modulus).
pub fn limbs_lt(raw: &[u8], modulus_le: &[u64]) -> bool {
    let limbs: Vec<u64> = raw
        .chunks(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(limbs.len(), modulus_le.len());
    for i in (0..limbs.len()).rev() {
        if limbs[i] != modulus_le[i] {
            return limbs[i] < modulus_le[i];
        }
    }
    false
}

/// BLS12-381 base field modulus p, little-endian limbs.
pub const P_LIMBS: [u64; 6] = [
    0xb9fe_ffff_ffff_aaab,
    0x1eab_fffe_b153_ffff,
    0x6730_d2a0_f6b0_f624,
    0x6477_4b84_f385_12bf,
    0x4b1b_a7b6_434b_acd7,
    0x1a01_11ea_397f_e69a,
];

/// BLS12-381 scalar field modulus r, little-endian limbs.
pub const R_LIMBS: [u64; 4] = [
    0xffff_ffff_0000_0001,
    0x53bd_a402_fffe_5bfe,
    0x3339_d808_09a1_d805,
    0x73ed_a753_299d_7d48,
];

thread_local! {
    static CATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Silence panic messages inside [`catch`] only; other panics still reach the
/// previous hook (libFuzzer's abort hook in fuzz builds).
pub fn quiet_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !CATCHING.with(|c| c.get()) {
                previous(info);
            }
        }));
    });
}

/// Outcome of a closure that may panic.
pub fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    quiet_panics();
    let was = CATCHING.with(|c| c.replace(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CATCHING.with(|c| c.set(was));
    r.map_err(|p| {
        p.downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string panic".into())
    })
}
