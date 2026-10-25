// Emit one JSON line per (k, coverage, srs, seed) proof from the `fx` package,
// with the stock 0.8.3 verifier's verdict on that proof. Shared by every variant.
pub fn emit(variant: &str) {
    use fx::{Coverage as C, Srs as S};
    let ks: Vec<u32> = std::env::var("Q5_KS").ok().map(|s| s.split(',').map(|x| x.parse().unwrap()).collect()).unwrap_or(vec![7, 11]);
    for &k in &ks {
        for cov in [C::Single, C::Multi, C::AllEqual, C::MaxElem] {
            for srs in [S::Exact, S::Oversized, S::Downsized, S::Serialized, S::BadLagrange] {
                for seed in [1u64, 2] {
                    let out = fx::prove(k, cov, srs, seed);
                    let ssrs: stock_fx::Srs = serde_json::from_value(serde_json::to_value(srs).unwrap()).unwrap();
                    let stock_accepts = stock_fx::verify(k, ssrs, &out.vk, &out.proof);
                    let line = serde_json::json!({
                        "variant": variant, "k": k, "coverage": cov, "srs": srs, "seed": seed,
                        "proof": blake2b_simd::blake2b(&out.proof).to_hex().to_string(),
                        "proof_len": out.proof.len(),
                        "vk": blake2b_simd::blake2b(&out.vk).to_hex().to_string(),
                        "draws": out.draws, "draw_digest": out.draw_digest, "word_pos": out.word_pos.to_string(),
                        "stock_accepts": stock_accepts,
                    });
                    println!("{line}");
                }
            }
        }
    }
}
