use midnight_proofs::utils::SerdeFormat;
use midnight_zk_stdlib::MidnightVK;
use std::io::{self, Read, Write};
pub type F = midnight_curves::Fq;
pub fn write_bytes<W: Write>(w: &mut W, b: &[u8]) -> io::Result<()> {
    w.write_all(&(b.len() as u32).to_le_bytes())?;
    w.write_all(b)
}
pub fn read_bytes<R: Read>(r: &mut R, max: usize) -> io::Result<Vec<u8>> {
    let mut n = [0; 4];
    r.read_exact(&mut n)?;
    let n = u32::from_le_bytes(n) as usize;
    if n > max {
        return Err(io::Error::other("length exceeds limit"));
    }
    let mut bytes = vec![0; n];
    r.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn vk_bytes(vk: &MidnightVK) -> Vec<u8> {
    let mut bytes = vec![];
    vk.write(&mut bytes, SerdeFormat::Processed).unwrap();
    bytes
}
