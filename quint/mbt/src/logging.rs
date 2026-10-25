//! Transcript delegation records operations in memory only. No secret values are printed.
use blake2b_simd::State;
use midnight_curves::G1Projective;
use midnight_proofs::transcript::{CircuitTranscript, Hashable, Sampleable, Transcript};
use std::io;

pub type Plain = CircuitTranscript<State>;

#[derive(Clone, PartialEq, Eq)]
pub struct Raw {
    pub kind: &'static str,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct Logging {
    inner: Plain,
    pub ops: Vec<Raw>,
}

fn kind<T>() -> &'static str {
    if std::any::type_name::<T>() == std::any::type_name::<G1Projective>() {
        "point"
    } else {
        "scalar"
    }
}

impl Transcript for Logging {
    type Hash = State;
    fn init() -> Self {
        Self {
            inner: Plain::init(),
            ops: vec![],
        }
    }
    fn init_from_bytes(bytes: &[u8]) -> Self {
        Self {
            inner: Plain::init_from_bytes(bytes),
            ops: vec![],
        }
    }
    fn squeeze_challenge<T: Sampleable<State>>(&mut self) -> T {
        self.ops.push(Raw {
            kind: "squeeze",
            bytes: vec![],
        });
        self.inner.squeeze_challenge()
    }
    fn common<T: Hashable<State>>(&mut self, input: &T) -> io::Result<()> {
        self.inner.common(input)?;
        let k = if kind::<T>() == "point" {
            "common_point"
        } else {
            "common_scalar"
        };
        self.ops.push(Raw {
            kind: k,
            bytes: input.to_bytes(),
        });
        Ok(())
    }
    fn read<T: Hashable<State>>(&mut self) -> io::Result<T> {
        let input: T = self.inner.read()?;
        self.ops.push(Raw {
            kind: kind::<T>(),
            bytes: input.to_bytes(),
        });
        Ok(input)
    }
    fn write<T: Hashable<State>>(&mut self, input: &T) -> io::Result<()> {
        self.inner.write(input)?;
        self.ops.push(Raw {
            kind: kind::<T>(),
            bytes: input.to_bytes(),
        });
        Ok(())
    }
    fn finalize(self) -> Vec<u8> {
        self.inner.finalize()
    }
    fn assert_empty(&mut self) -> io::Result<()> {
        self.inner.assert_empty()
    }
}
