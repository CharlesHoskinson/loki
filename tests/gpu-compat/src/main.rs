use midnight_proofs as proofs;
include!("../circuit.rs");

fn main() {
    let dir = std::env::args().nth(1).expect("fixture directory argument");
    verify(std::path::Path::new(&dir));
}
