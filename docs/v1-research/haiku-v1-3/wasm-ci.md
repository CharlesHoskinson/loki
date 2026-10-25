# TASK wasm-ci: CI, stock-compat harness, fuzz/docs status for wasm32

Evidence: `~/repos/loki-wt/v1/docs/v1-research/haiku-v1-3/inventory.md` section 5.

- No `.github/` in `~/repos/loki-wt/v1`, so no repository CI config for any target. Gates are shell scripts: `scripts/verify-main.sh`, `scripts/verify-w9.sh`, `scripts/w9-cpu-matrix.sh`. `grep -i wasm scripts/` returns nothing.
- Fuzz targets (`fuzz/fuzz_targets/`): msm_diff, ntt_diff, proof_parse, proof_roundtrip, soundness. None build for wasm32; no wasm mention in `fuzz/` except lockfile.
- Stock-compat harness `loki-harness/` (`loki-harness/src/lib.rs:174` reads RAYON_NUM_THREADS) and `tests-stock/`: native only. No wasm runner (no wasm-bindgen-test, no wasm-pack, no node runner) anywhere in the repo.
- Docs: `docs/PLATFORMS.md:320-358` WASM CPU section, checklist unchecked (`:351-358`); `TODO.md:295-304, 323, 359-368` (T73-T75, T80, T89, browser-bytes gate) all open; `README.md:68, 218, 260`.
- Gaps: (1) no wasm CI job; (2) no wasm test runner choice; (3) stock-compat gate never run on wasm (O11 in PLATFORMS.md:434 proposes native verifier on wasm bytes); (4) fuzz never on wasm; (5) MSRV 1.76 on wasm32 unverified (`Cargo.toml:14`).
