# bench: Sherlock recursion-chain benchmark harness (P7)

This is the implementation side of `wiki-llm/loki/benchmark/SUITE.md` (v2) for
the CHAIN workload: A102 + A80 leaves, then the A3 parent (k20), then the DEC
terminal decider. Each stage artifact is checked by a separate, unmodified
stock verifier.

## Layout

| Path | Role |
|---|---|
| `stage.sh` | `git archive` read-only snapshots: Moriarty `f2ea193777897869c2e45c08601cc412e00c3897` to `deps/moriarty`, Loki `LOKI_REV` to `deps/loki`. Writes `deps/STAGED` with the tree hashes and the Loki src patch digest against `0c4ab9d`. |
| `stock/`, `loki/`, `verifier/` | Separate workspaces, each with its own lockfile, so features never unify across arms. `stock` uses registry `midnight-proofs =0.8.3`. `loki` patches it to `deps/loki`. `verifier` is stock and has no Moriarty prover dependencies. |
| `adapter/driver.rs` | The `bench-driver` binary (S0/L0/G0, chosen by features). |
| `adapter/verifier.rs` | The `stock-verifier` worker. Uses `batch_verify` of a single proof: full transcript consumption plus the final pairing. |
| `build.sh` | Release builds into `target-circuits-<arm>` with `--locked --offline` and `CARGO_BUILD_JOBS=4`. Archives metadata, the feature tree and binary digests under `campaign/builds/<stamp>/`. |
| `control/` | `bench-launch`, the block controller (std + libc). See its module documentation. |
| `campaign/make-manifest.sh`, `campaign/run-smoke.sh` | Manifest generation and serial smoke blocks. |

## Status semantics

`bench-launch` exits 0 only if collection is `COMPLETE` and qualification is
`PASS`. Qualification is `PASS` when every scheduled arm is `ACCEPTED` and the
manifest's `vk_policy` holds. `require-equal` fails on any cross-arm VK
difference. `record` only reports the difference: VK equality must come from
deterministic construction or a controlled shared key artifact, and lookup
order is never sorted to force it. An arm is `FAILED` in these cases:

- nonzero exit;
- missing samples;
- G0 dispatch qualification failed;
- surviving descendants.

An arm is `INVALID` on any host swap I/O. `TIMEOUT`, `CANCELLED` and
`DEFERRED` are reported as such. On cancellation the controller exits 130.

### G0 dispatch qualification

A G0 sample counts only if every stage (A2 leaves, A3 parent, DEC) shows zero
accelerator errors and zero CPU fallbacks, and has at least one accelerated MSM
and at least one accelerated NTT.

### Timing scope

`chain_ms` is the SUITE CHAIN endpoint (`qualification_inclusive_chain_ms`),
including the extra stock-verifier round trips. `production_chain_ms` is the
sum of the stage operational windows (production APIs and their own checks).
`stock_receipt_ms_sum` is the total of the extra stock-verifier time.

### Process cleanup scope

Timeout and cancellation kill the arm's process group. Descendants that left
the group are re-parented to the controller (`PR_SET_CHILD_SUBREAPER`), then
killed and reaped, and the arm is marked `FAILED`.

If the controller itself is SIGKILLed, only the direct arm process
(PDEATHSIG, armed by the controller) and the driver's verifier worker
(PDEATHSIG, armed by the driver) are guaranteed to die. Both check the
expected parent after arming. Arbitrary grandchildren are not covered. `campaign/sigkill-receipt.sh <bench-driver>` is the process receipt for this guarantee: it uses the real driver and a fake worker that never reports ready, so nothing is proved. Post-exit cleanup is bounded at 15 s. Survivors, or stdout that never reaches EOF, fail the arm, and the locks are released anyway.

## Provenance and licence

The stage bodies in `adapter/driver.rs` and the checks in `adapter/verifier.rs`
are transcribed and adapted from the pinned Moriarty tree at
`f2ea193777897869c2e45c08601cc412e00c3897`, licensed Apache-2.0 (workspace
`Cargo.toml` `license`, root `LICENSE`; no root `NOTICE`). Sources:

- `crates/mori-backend/tests/sherlock_linked_children.rs` (leaves)
- `crates/mori-recursion/tests/sherlock_parent.rs` (parent)
- `crates/mori-recursion/tests/sherlock_terminal.rs` (decider)
- `crates/mori-recursion/examples/verify_chain.rs` (stock verification checks)
- `crates/mori-core/src/sherlock/reference.rs` and
  `crates/mori-recursion/tests/fixtures/accept-template.json`, embedded with
  `include_bytes!` from the staged snapshot

Modifications:

- the test assertions were turned into error returns;
- non-overlapping timing windows were added around production calls;
- artifacts are published atomically (`.partial`, then rename);
- the decider's communication randomness stays in memory and is never written.

Copyright remains with the original authors.
