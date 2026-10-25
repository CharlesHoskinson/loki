//! Write proof_parse seed inputs (honest VK, verifier params and proofs).
fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/proof_parse");
    std::fs::create_dir_all(&dir).unwrap();
    let fx = loki_diff::parse::fixture();
    let b = &fx.built;
    let put = |name: &str, mode: u8, body: &[u8]| {
        let mut v = vec![mode];
        v.extend_from_slice(body);
        std::fs::write(dir.join(name), v).unwrap();
    };
    put("seed-vk", 0, &b.vk);
    put("seed-vparams", 1, &b.lvp);
    put("seed-proof-loki", 3, &fx.proofs[0]);
    put("seed-proof-stock", 3, &fx.proofs[1]);
    put("seed-edit-loki", 4, &[]);
    put("seed-edit-stock", 4 | 8, &[]);
    put("seed-vk-verify", 5, &b.vk);
    let srs = loki_diff::proof::params_bytes(4);
    put("seed-srs", 2, &srs);
}
