# A3 independent stock-verifier probe

## Purpose and boundary

Verify one saved W10 A3 parent proof using registry `midnight-proofs 0.8.3`, without keygen, proving, GPU calls, or edits to Moriarty/Loki production sources. This is an isolated verifier package proposal, not an implementation or result. W10 confirmed it does not already own an independent A3 stock gate.

On the successful W10 A3 run, read only:

```text
<run>/artifacts/parent/w.vk
<run>/artifacts/parent/w.proof
<run>/artifacts/parent/w.instance.json
/home/charles/repos/Moriarty-loki/target/sherlock-srs/bls_midnight_2p20
```

W10 says `w.vk` is processed `MidnightVK` serialization, `w.instance.json` is a JSON array of canonical 32-byte little-endian field hex, and the parent instance has 28 lanes. Its current candidate run is still in progress, and `parent/w.proof` does not exist yet. Wait for W10's accepted-run notification and use the exact run path it supplies; do not substitute an older artifact based only on its filename.

## Isolated dependency graph

Make this a standalone Cargo package with its own lockfile and `[workspace]` boundary. Pin direct dependencies to `midnight-proofs = "=0.8.3"`, `midnight-curves = "=0.3.1"`, and the exact `midnight-zk-stdlib` / `midnight-circuits` versions used by the pinned Moriarty source, to read `MidnightVK` and obtain the A3 transcript hash implementation. Enable the `committed-instances` feature if required by stock `prepare`; A3 has zero committed public inputs, but its verifier API argument still needs the empty committed-instance columns in that feature configuration. Resolve and lock all dependencies from crates.io; no Loki path dependency or `[patch.crates-io]` is permitted.

Before the first run, inspect the lock and `cargo tree --locked --manifest-path probes/a3-stock-verify/Cargo.toml -p midnight-proofs --depth 0`; record that it resolves to registry 0.8.3. The existing `/home/charles/repos/loki-wt/w10-gpu-quotient/tests/stock-profile-verifier` is a useful isolation pattern, but cannot verify A3 unchanged: it rebuilds a synthetic `ProfileCircuit` VK and `unsafe_setup` SRS, and assumes a Blake2b transcript. A3 needs its saved processed VK, ceremony SRS, and Poseidon transcript.

## Read-only verification flow

1. Accept the artifact directory, SRS path, and W10 public manifest path as CLI arguments. Pin the manifest's SHA-256 plus `gpu-after` mode, Loki/Moriarty commits, 237.808-second accepted-run metadata and 28-lane shape. Confirm the manifest entries match the actual VK/proof/instance/SRS bytes. Read `w.vk`, `w.proof`, `w.instance.json`, and the SRS; print their SHA-256 digests and lengths as public artifact provenance. Do not copy or log any proving witness or secret material.
2. Check the SRS file is exactly 201,326,980 bytes and SHA-256 `1cc62978558fdc1e445cd70cfd9a86ec3c2e2151b6d74811232d37faf9133ff1`. Parse using `ParamsKZG::<Bls12>::read_custom(..., SerdeFormat::RawBytesUnchecked)`, require complete cursor consumption and `max_k() == 20`, then use `params.verifier_params()`. The unchecked raw format is acceptable only after this public byte-length/hash pin.
3. Parse `MidnightVK::read(..., SerdeFormat::Processed)` and require EOF after the key. Independently parse its public header and assert k=20, 28 declared public inputs, and one instance column. Parse exactly 28 JSON field strings into canonical `Fq` values; malformed, noncanonical, extra, or missing fields are errors. The proof is opaque bytes and is never decoded or rewritten by the harness.
4. Use stock `midnight_proofs::plonk::prepare` with `vk.vk()`, noncommitted instances `&[&[&instance]]`, and, when the `committed-instances` API is enabled, the one-proof empty committed input `&[&[]]`. Initialize `CircuitTranscript<PoseidonState<Fq>>` from the saved proof, matching `mori-recursion/tests/sherlock_parent.rs` (`PoseidonState`, not the k14 profile fixture's Blake2b). Call `transcript.assert_empty()` and then `Guard::verify(&params.verifier_params())`.
5. Using the same VK/SRS and no generated proof, mutate each of the 28 instance lanes by canonical field `+1`, flip one middle proof byte, truncate one byte, and append one zero byte. Require every stock verification attempt to reject. Print only a concise receipt with the positive acceptance, all negative controls rejected, stock crate source/version, SRS/VK/proof/instance hashes, k=20, one-column/28-lane shape, proof byte count, and full transcript consumption. Do not keygen, prove, serialize the VK/proof, or write into the source run directory.

The direct `prepare` + `Guard::verify` route verifies the saved VK and public lanes with unmodified stock verifier code; it does not require reimplementing `SherlockLinkRelation` or trusting the Loki-linked `midnight_zk_stdlib::verify` call that generated the artifact. If API visibility or feature resolution prevents this direct route, stop and report the blocker before switching to a different verifier path.

## Execution gate and provenance

This reads the full public k20 SRS, so run the eventual CPU verifier under DoctorWatson's normal heavy wrapper. It needs no GPU lock request beyond whatever the host runner imposes for the CPU command; do not add `cuda` features. Build jobs remain `CARGO_BUILD_JOBS=3`. Before the accepted-artifact run, verify the standalone Cargo.lock registry source and inspect the compiled dependency `.d` file to confirm the binary used registry `midnight-proofs 0.8.3`; retain the lock and dep-info hashes with the receipt. `cargo tree` is not available through the host runner allowlist, so the lock plus actual dep-info are the source-resolution evidence.

Use the W10 run's exact `w.vk`, `w.proof`, and `w.instance.json`; this package pins the accepted public manifest at SHA-256 `f4e2c035e37f996ee0b13563cc2c8dc7e16e607b044f7f95228b12cb6e4531bb`. Name circuit `mori-recursion::SherlockLinkRelation`, Moriarty source commit `6ee918696ab907244c703bbd94bfea9acde7ddd1`, proof-producing Loki commit `053577f6083b329f2bbef1f3711f92cadc92a24f`, W10 feature set, prover Rayon=4/BLST=1, verifier Rayon=4/BLST no-threads, lab host, verifier commit, and raw DoctorWatson/runner log. No proof-generation or key-generation timing is included.

The required run includes all negative controls. This means 32 total stock verifier attempts (one accepted proof and 31 rejected controls), all under the same DoctorWatson heavy reservation. The verifier confirms its actual Rayon pool has four threads before reading the SRS. Its receipt identifies argv, host, verifier git head and source hash, Cargo.lock hash, and verifier features. Optional host probes must be handled without making the CPU verifier fail when a tool such as `nvidia-smi` is absent.
