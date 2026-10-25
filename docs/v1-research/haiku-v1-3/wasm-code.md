# TASK wasm-code: current WASM/target support, build cfgs, dependency blockers

Full evidence: `~/repos/loki-wt/v1/docs/v1-research/haiku-v1-3/inventory.md` (sections 1-3 have file:line references).

Probe results (rustc 1.99.1, `cargo check --target wasm32-unknown-unknown --lib`, CARGO_BUILD_JOBS=2):
- PASS: default features; `--no-default-features`; `--features loki-cpu-budget,loki-profile`; `--features loki-backend-api`.
- FAIL: `--features loki-gpu-backend`. loki-gpu build script runs nvcc (`~/repos/loki-wt/v1/loki-gpu/crates/loki-gpu/build.rs:1-40`), enabled by `Cargo.toml:69-73` with no target gate.

Blockers and hazards:
1. `Cargo.toml:64-73` (`loki-sppark`, `loki-gpu-backend`) enable `cuda` without a target gate. Blocks wasm and mobile GPU-off builds.
2. `src/profile.rs:24,104-205` uses `std::time::Instant` and file I/O (`:169`); `src/lib.rs:25-26` gates it behind `loki-profile`. Runtime panic risk on wasm32-unknown-unknown (not executed).
3. getrandom: `Cargo.toml:227-229` sets getrandom 0.2 `js` for wasm32-unknown-unknown. Lockfile wasm tree resolves only 0.2.17 (Cargo.lock:552). Correct today.
4. rayon: `src/utils/multicore.rs:1`, 49 call sites in src/. rayon-core 1.13.0 falls back to sequential on wasm32-unknown-unknown (`rayon-core-1.13.0/src/lib.rs:20-30`). No `wasm-bindgen-rayon` dependency yet.
5. blst: `Cargo.toml:154-155`, resolves 0.3.17. Its build.rs sets no_std + no-threads + `-ffreestanding` on wasm32 (`blst-0.3.17/build.rs:47-53, 223-226`). Compiles on wasm.
6. `OsRng` used in non-test `gen_params`: `src/poly/kzg/mod.rs:70`.

Not verified: wasm dev/test graph (stock dev-dep `Cargo.toml:202-208`), wasm-threads build, any runtime execution.
