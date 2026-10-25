//! q5 differential replay: snapshotted stock 0.8.3 vs Loki a256a9a (and W10
//! 5e37ead) lookup/permutation preparation code, plus real end-to-end proofs.
pub mod shim;

pub mod stock_arith {
    include!(concat!(env!("OUT_DIR"), "/stock_arith.rs"));
}
pub mod loki_arith {
    include!(concat!(env!("OUT_DIR"), "/loki_arith.rs"));
}
pub mod stock_permute {
    include!(concat!(env!("OUT_DIR"), "/stock_permute.rs"));
}
pub mod loki_permute_default {
    include!(concat!(env!("OUT_DIR"), "/loki_permute_default.rs"));
}
pub mod loki_permute_det {
    include!(concat!(env!("OUT_DIR"), "/loki_permute_det.rs"));
}
pub mod w10_permute {
    include!(concat!(env!("OUT_DIR"), "/w10_permute.rs"));
}
pub mod stock_product {
    include!(concat!(env!("OUT_DIR"), "/stock_product.rs"));
}
pub mod loki_product {
    include!(concat!(env!("OUT_DIR"), "/loki_product.rs"));
}
pub mod stock_perm {
    include!(concat!(env!("OUT_DIR"), "/stock_perm.rs"));
}
pub mod loki_perm {
    include!(concat!(env!("OUT_DIR"), "/loki_perm.rs"));
}
pub mod stock_rational {
    include!(concat!(env!("OUT_DIR"), "/stock_rational.rs"));
}
pub mod loki_rational {
    include!(concat!(env!("OUT_DIR"), "/loki_rational.rs"));
}
pub mod w10_commit {
    include!(concat!(env!("OUT_DIR"), "/w10_commit_argument.rs"));
}
pub mod fixtures;
pub mod rng;
