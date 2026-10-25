//! Main a256a9a (default, or `--features det`) proof emitter.
use loki_q5_replay::fixtures::{loki_fx as fx, stock_fx};
include!("../../../common/emit.rs");
fn main() {
    emit(if cfg!(feature = "det") { "main-det" } else { "main" });
}
