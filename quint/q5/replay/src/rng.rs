// RNG wrapper that records every draw, so stock and Loki can be shown to make
// the same random draws in the same order (not just the same final state).
use rand_chacha::ChaCha20Rng;
use rand_core::{CryptoRng, RngCore, SeedableRng};

pub struct Recording {
    inner: ChaCha20Rng,
    pub log: Vec<(u8, Vec<u8>)>,
}
impl Recording {
    pub fn new(seed: u64) -> Self {
        Self { inner: ChaCha20Rng::seed_from_u64(seed), log: vec![] }
    }
    pub fn word_pos(&self) -> u128 {
        self.inner.get_word_pos()
    }
}
impl RngCore for Recording {
    fn next_u32(&mut self) -> u32 {
        let v = self.inner.next_u32();
        self.log.push((0, v.to_le_bytes().to_vec()));
        v
    }
    fn next_u64(&mut self) -> u64 {
        let v = self.inner.next_u64();
        self.log.push((1, v.to_le_bytes().to_vec()));
        v
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.inner.fill_bytes(dest);
        self.log.push((2, dest.to_vec()));
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
impl CryptoRng for Recording {}
