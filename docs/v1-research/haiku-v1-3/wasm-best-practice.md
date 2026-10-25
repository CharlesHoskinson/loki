# TASK wasm-best-practice: wasm32-unknown-unknown constraints for Loki CPU build

Sources opened (URL, what was taken from it):
- rayon README, https://github.com/rayon-rs/rayon (section "Usage with WebAssembly"): without the adapter, rayon treats wasm as a no-threads platform and runs sequentially. Requires rustc 1.85 or newer. Real threads need wasm-bindgen-rayon.
- rayon-core 1.13.0 source (local copy `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rayon-core-1.13.0/src/lib.rs:20-30`): on wasm32-unknown-unknown the global pool falls back to sequential; `join` runs its closures in order.
- wasm-bindgen-rayon README, https://github.com/RReverser/wasm-bindgen-rayon: needs cross-origin isolation (COOP/COEP, https://web.dev/coop-coep/) for SharedArrayBuffer; needs nightly Rust with `+atomics,+bulk-memory` and `build-std`; tested with nightly-2025-11-15; needs `--target web`; the page calls `initThreadPool(navigator.hardwareConcurrency)` after `init()`. Documents that Rust std for wasm32 is built without threads by default.
- getrandom 0.2.17 README, https://github.com/rust-random/getrandom (tag v0.2.17): wasm32-unknown-unknown is not supported by default; enable the `js` feature for `crypto.getRandomValues` via wasm-bindgen. The README warns against enabling it in libraries because it bloats Cargo.lock and breaks non-web wasm builds. (The current master README describes the 0.3+ name `wasm_js`.)
- Rust std `Instant`, https://doc.rust-lang.org/std/time/struct.Instant.html, and std source `library/std/src/sys/time/mod.rs` and `unsupported.rs` on rust-lang/rust master (fetched raw): targets not matched by an OS arm fall to `unsupported`, where `Instant::now()` panics with "time not implemented on this platform". wasm32-unknown-unknown is in that fallback. Source is master, not necessarily 1.99.1; re-check against that release.
- blst 0.3.17 build.rs (local copy): on wasm32 it sets no_std, `no-threads`, and `-ffreestanding` (`blst-0.3.17/build.rs:47-53, 223-226`).

Recommendations for Loki (ranked):
1. Keep the CPU wasm build single-threaded by default. rayon's fallback is correct, so the build works without the adapter. Threads are an opt-in feature (for example `wasm-threads`) that depends on nightly, atomics, COOP/COEP, and a JS thread-pool init. Do not make nightly a default requirement.
2. Gate the GPU crates off on wasm (`cfg(not(target_arch = "wasm32"))`). Measured failure: `loki-gpu-backend` breaks the wasm32 check (see inventory.md R1).
3. Gate `loki-profile` off on wasm, or give it a backend that does not call `Instant::now()` (it panics on wasm32-unknown-unknown per the std source above). Writing JSONL to disk also has no wasm equivalent.
4. getrandom: Loki's `Cargo.toml:227-229` enables getrandom 0.2 with `js` for wasm32-unknown-unknown in library code. The getrandom README advises against this in libraries. Better: keep `js` in the wasm-bindgen consumer (or a wasm-only binding crate), or document it as the one required setting for browser/Node consumers and check that non-web wasm consumers do not break. Decision for owners; not resolved here.
5. blst: no action needed. Portable C on wasm32, no threads, matches `loki-cpu-budget` (`Cargo.toml:61`). Measure the gap versus native (O9).
6. Memory: wasm32 linear memory is limited to 4 GiB (docs/PLATFORMS.md:325). Memory64 is shipped in Chrome 133+ and Firefox 134+; Safari is not supported as of its latest listed release in Can I Use (https://caniuse.com/wf-wasm-memory64, searched 2026-10-25). Do not plan v1 on memory64; treat it as post-v1 unless browser coverage changes.

Unverified in this task: no wasm binary was run; no node or browser runner was used; the wasm-threads build was not attempted (needs nightly + build-std).

Sources:
- https://github.com/rayon-rs/rayon
- https://github.com/RReverser/wasm-bindgen-rayon
- https://web.dev/coop-coep/
- https://github.com/rust-random/getrandom
- https://doc.rust-lang.org/std/time/struct.Instant.html
- https://caniuse.com/wf-wasm-memory64
