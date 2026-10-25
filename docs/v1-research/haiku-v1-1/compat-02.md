# compat-02: outside practice for Rust public-API CI and drop-in [patch.crates-io]

Scope: research only. Sources are official docs or the tool repos. Nothing here has been run against Loki yet, unless a line says so.

## 1. How a `[patch.crates-io]` drop-in behaves (Cargo reference)
Source: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html
- `[patch]` is read only from the top-level workspace `Cargo.toml`. "Cargo ignores `[patch]` sections in dependencies, so consumers of a library must repeat any patches they need." Consequence for Loki: a downstream app must add the patch itself. A library that depends on Loki does not carry the patch with it.
- Name matching: the patch key must be the crate name used in the graph (`midnight-proofs`). To patch another version of the same crate, use the `package` key.
- Version matching: a patch is used only when its version satisfies the dependency requirement. Loki declares `version = "0.8.3"`, the same as stock, so a consumer `midnight-proofs = "=0.8.3"`, `"0.8"`, or `"^0.8"` is satisfied.
- "If the patched version already exists in the source, the source's original crate is replaced by the patch." This is the drop-in case, and it is how Loki works today.
- Troubleshooting: if the local copy is not used, `Cargo.lock` may need `cargo update midnight-proofs --precise 0.8.3`.
- Caveat for the gate: a same-version patch is silent. Nothing in Cargo fails if the patch's API drifts. The gate has to check that itself.

## 2. Feature additivity (Cargo reference)
Source: https://doc.rust-lang.org/cargo/reference/features.html
- "features should be _additive_. That is, enabling a feature should not disable functionality... A feature should not introduce a SemVer-incompatible change."
- Cargo unifies features across the graph. Any Loki feature, such as `loki-harden` or `loki-multiopen`, can be enabled by a crate the consumer does not control. Therefore every Loki-only feature must be additive, and mutually exclusive ones need a `compile_error!`.
- Loki already does this for `loki-gpu-backend` vs `cuda`/`cuda-ntt` (`src/lib.rs:12-16` in the Loki tree, `compile_error!`). That matches the Cargo guidance.
- Caveat: several Loki features change default-off arithmetic paths, not just add items (for example `loki-deterministic-lookup-order` changes proof bytes, `src/plonk/lookup/prover.rs:425-431`). The Cargo rule about "additive" is about functionality, but a feature that changes wire output is a compat question, not only a Cargo one. It has to be documented as opt-in and excluded from the default-stock gate.

## 3. Public-API diff tools
### cargo-semver-checks
Source: https://github.com/obi1kenobi/cargo-semver-checks/blob/main/README.md
- Lints a crate's rustdoc JSON. It runs against a baseline: `--baseline-rev <REV>` (git), `--baseline-version <X.Y.Z>` (crates.io), `--baseline-root`, `--baseline-rustdoc`.
- Feature selection: `--all-features`, `--default-features`, `--only-explicit-features`, `--features <NAME>`, plus `--baseline-features` and `--current-features`.
- Limits stated by the project: custom registries are unsupported; it misses breaking type/generic/lifetime changes; it misses changes that appear only under a feature subset. Rustdoc JSON is unstable, and nightly is best-effort.
- Background: https://rust-lang.github.io/rust-project-goals/2026/cargo-semver-checks.html says it has 245 lints and is being considered for merging into cargo.
- Loki-specific risk: the current crate version is `0.8.3`, the same as the baseline. The version-bump rules in a semver check may not apply cleanly to a same-version comparison. Unverified. The gate may need `--baseline-root` pointing at the stock source and `--current-root` at Loki, then reading the output, not the exit code alone.

### cargo-public-api
Source: https://github.com/cargo-public-api/cargo-public-api (search result; the README was not fetched in full)
- Lists and diffs the public API of a crate between releases or commits. The CI pattern is a snapshot file (`tests/public-api.txt`) regenerated with `UPDATE_SNAPSHOTS=yes cargo test`, then a plain `cargo test` that fails on any API change. Needs rustdoc JSON, so a nightly toolchain.
- Local toolchains: `nightly-2026-10-11-x86_64-unknown-linux-gnu` is installed alongside stable 1.99.1. Not yet tested.
- Gotcha from a downstream issue (orieg/discipline#462): `cargo public-api diff` with no argument compares against the latest published release, and it exits 0 unless `--deny` is passed. Pass an explicit base and `--deny=removed --deny=changed`. This is reported in a third-party issue, not in the tool's docs, so it is unverified.
- Snapshot-based is the better fit here: the baseline is the same version, so a version-keyed tool is awkward.

## 4. Proposed gate commands (not run; to be validated)
1. Public-API snapshot (stock vs Loki, feature-specific). Stock side: an unmodified copy of `midnight-proofs-0.8.3` from the registry, pinned by SHA-256 `19b2e4b1acb581ad741b0cb7c0e9dbdf51142f4e9518886f25fcff40a6702dcc` (the `.crate` file under `~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/`, which matches `Cargo.lock` `checksum`). Run `cargo +nightly-2026-10-11 public-api` on each side with the default feature set, then diff. Fail on any removed or changed line.
2. Default-features check: both sides with no `--features`. Loki's default set must equal stock's.
3. Consumer fixtures, each a small workspace with `[patch.crates-io] midnight-proofs = { path = "<loki>" }`:
   - `midnight-proofs = "=0.8.3"`;
   - `midnight-proofs = "^0.8"`;
   - a no-patch fixture that pins the stock registry source, so the test shows the swap is real (`cargo tree -i midnight-proofs` should show the path source only in the patched fixture).
4. Negative control: a fixture that removes one stock `pub fn` in a scratch copy must make gate 1 fail.
5. MSRV: declared `rust-version = "1.76.0"` on both sides. Verifying it needs a `cargo +1.76` toolchain, which is not installed locally (installed: 1.95, 1.95.0, 1.98.1, 1.99.0 and 1.99.1). Do not raise MSRV to satisfy a dependency (astra's challenge agrees).

## 5. Open points
- The lock-level stock identity is `checksum = 19b2e4b1...702dcc` (verified locally against the cache file). Good as a pin.
- None of the gate commands has been run. cargo-semver-checks and cargo-public-api are not installed (`which` returned nothing for both).
