//! W10 (CPU-only copy) proof emitter. `--features arg` turns on cuda-argument-commit's
//! coefficient-basis commit strategy; without it the copy runs the main commit path.
mod rng {
    include!("../../replay/src/rng.rs");
}
include!("../../common/fixture_macro.rs");
fixture_backend!(fx, loki, crate::rng::Recording);
fixture_backend!(stock_fx, stock, crate::rng::Recording);
include!("../../common/emit.rs");
fn main() {
    let name = env!("CARGO_PKG_NAME").trim_start_matches("loki-q5-");
    let arg = if cfg!(feature = "arg") { "+arg" } else { "" };
    let det = if cfg!(feature = "det") { "+det" } else { "" };
    emit(&format!("{name}{arg}{det}"));
}
