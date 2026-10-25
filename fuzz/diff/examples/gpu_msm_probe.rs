//! Is the GPU MSM path taken? Prints timing and whether a fallback warning fired.
fn main() {
    loki_diff::msm::init();
    use group::Group;
    use midnight_curves::{Fq, G1Affine, G1Projective};
    let b: Vec<G1Projective> = (1..=65536u64).map(|i| G1Projective::generator() * Fq::from(i)).collect();
    let s: Vec<Fq> = (1..=65536u64).map(Fq::from).collect();
    for n in std::env::args().skip(1).map(|a| a.parse::<usize>().unwrap()) {
        let t = std::time::Instant::now();
        let r = loki::poly::kzg::msm::msm_specific::<G1Affine>(&s[..n], &b[..n]);
        let c = stock::poly::kzg::msm::msm_specific::<G1Affine>(&s[..n], &b[..n]);
        #[cfg(feature = "cuda")]
        let w = loki_diff::msm::gpu_watch::WARNED.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(feature = "cuda"))]
        let w = false;
        println!("n={n} eq={} {:.3}s warned={w}", r == c, t.elapsed().as_secs_f64());
    }
}
