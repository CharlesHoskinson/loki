# A3 stock verifier

This isolated package verifies a saved Sherlock A3 parent proof with registry
`midnight-proofs 0.8.3`. It reads the processed public `w.vk`, opaque `w.proof`,
28 canonical `Fq` lanes from `w.instance.json`, and the byte/hash-pinned k20 SRS.
It generates no keys or proofs and does not use CUDA. It checks the full Poseidon
transcript is consumed before the stock KZG guard verifies.
The accepted-artifact gate also requires rejection of every public lane changed
by canonical `+1`, one corrupted proof byte, a truncated proof, and an appended
proof byte. It asserts the VK's 28 public-input metadata and its two PLONK
instance columns: one committed identity plus one ordinary public-input column.

Build and run parser tests through the host runner with `-j3`. For accepted A3
verification, run the binary under the DoctorWatson heavy wrapper. The outer
invocation below enters that wrapper automatically; wait for Astra to reserve
the CPU heavy slot and for W10 to publish an accepted A3 artifact first:

```sh
cargo run --release -j3 --locked --manifest-path probes/a3-stock-verify/Cargo.toml -- \
  /home/charles/repos/loki-wt/w10-gpu-quotient/target/a3-profile/gpu-after-1792902759671818294-2247472/artifacts/parent \
  /home/charles/repos/Moriarty-loki/target/sherlock-srs/bls_midnight_2p20 \
  /home/charles/repos/loki-wt/w10-gpu-quotient/target/w10-integrated-a3-public-manifest.json
```

The verifier pins the W10 manifest digest, run mode, Loki/Moriarty commits,
timing, lane count and the VK/proof/instance/SRS artifact digests. The SRS path
and artifact directory are read-only inputs. The receipt includes the verifier
git head, source-tree and Cargo.lock hashes, argv, host, enabled verifier
features, and the observed Rayon pool size. Missing optional host commands do
not affect verification. It does not write into the W10 run directory.
