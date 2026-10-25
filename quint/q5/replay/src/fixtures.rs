include!("../../common/fixture_macro.rs");
fixture_backend!(stock_fx, stock, crate::rng::Recording);
fixture_backend!(loki_fx, loki, crate::rng::Recording);
