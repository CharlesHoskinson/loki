# Tasks

## 1. Independent stock workspaces

- [ ] 1.1 Add `./compat-proof-stock/run.sh` and isolated locked workspaces for stock 0.8.1 and stock 0.8.3, consuming `loki-harness/fixtures/v1/manifest.jsonl`; verify provenance, the 0.8.1 `.crate` checksum, exact versions, and lockfile digests with `cargo tree --locked` in each workspace. Invoke k20 directly with a 1800-second hard timeout (timeout is failure, not a passing/skipped row) and record elapsed time/host; document runner arguments.
- [ ] 1.2 Add k20 rows for declared circuit/instance/batch shapes and stock 0.8.1/0.8.3 lanes; verify with `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --k 20 --serial` and archived JSONL rows showing circuit, k, stock version, Loki revision, both verifier results, and VK identity.

## 2. T04 differential and negative evidence

- [ ] 2.1 Run proof verification both ways under stock 0.8.1 at k20 and compare VK, PK, and verifier parameter round trips using existing stock serialization APIs; verify with `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --k 20 --serial` and retain source/lock pins and complete logs.
- [ ] 2.2 Add malformed multi-proof length controls with per-version expected behavior; verify with `./compat-proof-stock/run.sh --negative malformed-multiproof --versions 0.8.1,0.8.3 --serial` and document the 0.8.1 historical classification versus 0.8.2+ `InvalidInstances` behavior.
- [ ] 2.3 Run wrong-input, empty, mutated, malformed-key, satisfying-witness, and failing-witness negative controls for both stock versions; verify with `./compat-proof-stock/run.sh --negative all --versions 0.8.1,0.8.3 --serial` and record version-specific behavior under shared fixture case IDs.

## 3. QA and release records

- [ ] 3.1 Reference T04 manifest IDs from differential fuzzing and Quint/Rust MBT; verify `fuzz/quick.sh` and `python3 quint/check_instances.py` use version-conditional expectations and retain minimized seeds and the T03 counterexample classification.
- [ ] 3.2 Map the source-reported 0.8.1/0.8.3 verifier and MSM differences to Lean version assumptions without claiming that a source diff proves runtime compatibility; verify the Lean release build has no `sorry` and no new custom axioms.
- [ ] 3.3 Run shared stock 0.8.1/0.8.3 fixtures on target-native jobs defined by WASM/mobile owners; verify both Loki and matching stock verifiers execute on target in both directions using `wasm:stock-compat` and `mobile-compat/run.sh --target <rust-triple>`, with target/version results and no GPU requirement.
- [ ] 3.4 Update T04 and compatibility documentation with exact source pins, commands, lockfile digests, version-conditional negative results, and unverified gaps; verify `./compat-proof-stock/run.sh --versions 0.8.1,0.8.3 --k 20 --serial` and `openspec validate v1-compat-proof-stock-matrix --strict`.
