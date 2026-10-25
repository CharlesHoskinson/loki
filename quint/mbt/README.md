# Loki model replay

This separate test crate leaves the production crate and its features unchanged.
It uses the public Informal Systems `quint-connect` repository, pinned to
`4f018f54fc7dd4cef341d10111427bab59d3b307` (0.1.2), and Quint 0.32.0.

From the Loki worktree root:

```sh
CARGO_BUILD_JOBS=4 RAYON_NUM_THREADS=2 cargo test --locked \
  --manifest-path quint/mbt/Cargo.toml \
  --features w4-source,w6-source -- --nocapture --test-threads=1
```

Without either optional source feature, only the real transcript tests run.
Use a fixed Rayon count for proof-byte comparisons: stock vanishing randomness
depends on the number of workers.

`build.rs` reads source from two immutable local Git commits into Cargo's
`OUT_DIR`. It fails if those commits are unavailable; it never edits either
source repository. W4 is read from this repository's object database at
`fdef4c4d70b6a5ea040a8bc94638a57ecc1eb3b2`. W6 is read from
`/home/charles/repos/fenrir-loki` at
`abe97019b099026698d10c97f57d490a9554018f`; override that repository path with
`LOKI_RECL9_REPO`. The test source fingerprint for W6 is synthetic, so the tests
exercise cache identity without generating production cache entries.

- The transcript driver generates 24 model traces, creates randomized k5 proofs,
  and checks every model state against the logged operations. Both wrapped and
  plain proofs must pass the unchanged stock verifier. Each wrapped absorption is
  also compared byte for byte with the stock verifier's corresponding absorption.
  Fixed-RNG proof equality is asserted for the no-lookup circuit family. Stock
  lookup nondeterminism is reported in `LOKI-QUINT.md`.
- A separate two-instance-column diagnostic checks nine real stock cases and
  expects rejection exactly for multiple proofs with a mixed committed/plain
  layout. It demonstrates the existing prover/verifier absorption-order bug;
  it does not fix that bug. The symbolic audit and its counterexample are in
  `transcript.qnt`'s `transcript_instances` module and `check_instances.py`.
- `w4-source` compiles the actual W4 Rust dispatch, conversion and resident-cache
  code against **CPU-backed FFI functions**. It does not link CUDA, run a GPU
  kernel, or claim a GPU arithmetic differential. The fixture's `cuda` feature
  only enables the original Rust source's conditional compilation. The adapter
  uses the actual native `msm_specific` CPU path after backend failure and on
  below-threshold requests. Model completion publishes the result captured from
  synchronous dispatch. Error transitions additionally probe failure handling;
  they do not simulate an asynchronous CUDA execution engine. A test-only reset
  releases CPU surrogate handles between traces. A test mutex serializes resets
  and fault injection. An additional test uses eight concurrent host callers.
- `w6-source` compiles the actual W6 PK cache and its original five tests. Four
  additional model traces exercise the cache with a barrier synchronizing three
  cold callers, count real keygen callbacks, compare returned PK/VK bindings, and
  verify real proofs. The public setup API is tested separately. The model's
  build/publication split stages exposure of the result of W6's synchronous
  `get`; it does not claim W6 exposes these as separate public API calls. These
  small fixtures use registry `midnight-circuits` 7.2.4, rather than the full
  hardened recl9 application.

SRS setup and transcript payloads stay in memory. W6 tests temporarily serialize
only generated public proving material and remove their cache directories.
No application witnesses, private setup randomness or transcript payloads are
written to logs. Diagnostic output contains model shapes and equality flags.

There is no scheduler implementation to replay, and this crate does not exercise
W5's NTT adapter. See `LOKI-QUINT.md` for its fail-stop/fallback mismatch.
