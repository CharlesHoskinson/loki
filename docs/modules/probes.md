# `probes/`: independent verification of real artifacts

## `probes/a3-stock-verify`

Verify one saved Sherlock A3 parent proof with registry `midnight-proofs 0.8.3`. The package ([`probes/a3-stock-verify`](../../probes/a3-stock-verify)) has `src/lib.rs`, `src/main.rs`, its own `Cargo.lock` and workspace, no Loki dependency and no CUDA. It is a Cargo package outside the root build.

The package (`README.md`, `DESIGN.md`):
- Reads the processed public `w.vk`, the opaque `w.proof`, 28 canonical `Fq` lanes from `w.instance.json`, and the k20 SRS pinned by byte length and SHA-256.
- Verifies with the stock `prepare` and `Guard::verify` over a Poseidon transcript, after checking the transcript is fully consumed.
- Rejects every public lane changed by canonical `+1`, one corrupted proof byte, a truncated proof and an appended byte.
- Asserts the VK's 28 public inputs and its two instance columns (one committed identity, one ordinary).
- Pins the W10 manifest digest, run mode, commits, timing and artifact digests, and prints a receipt with the verifier git head, source and `Cargo.lock` hashes, argv, host and the observed Rayon pool size.

The probe is the stock side. It writes nothing into the run directory.

## Invariants and pitfalls
- The run command hard-codes host-specific paths (artifact directory, SRS) and needs the host heavy-job wrapper. It cannot be reproduced on another machine without those artifacts, which are not in the repository.
- Proof and witness artifacts are not in the repository. Only public provenance is printed.
- The SRS is read in the unchecked raw format. That is acceptable only after the length and hash pin.

## Tests

Parser tests run through the host runner. The accepted-artifact gate is the run itself (commit `33e3c65` "Add independent A3 stock verifier", merge `1db6ab8` "X2").

## Notes

[`probes/a3-stock-verify/README.md`](../../probes/a3-stock-verify/README.md), [`DESIGN.md`](../../probes/a3-stock-verify/DESIGN.md). The A3 timing claim (294.3 s to 237.8 s, -19.2%) is from the W10 run that this probe verifies.
