# luna-r10 review: consistency, identity and cross-links

## Concerns

- `/home/charles/repos/loki-gpu-wt/docs/README.md:22-27` and `/home/charles/repos/loki-gpu-wt/docs/docs/OVERVIEW.md:5,157-164` reported P2 as in progress. The README said its runtime was on `p2-runtime` and absent from the docs branch. The source repository was pinned at `d71b7ca`, which merged the transactional runtime and versioned native ABI. `crates/loki-gpu/src/runtime.rs` implements scalar resident upload, download and release, and `crates/loki-gpu/src/lib.rs` reports runtime capabilities. At first review, the Loki docs worktree was at `df9584f` on branch `docs`; the coordinator later fast-forwarded it to `a256a9a15ff18ae12a8131b86cff576787fa5e3c`, which merged the P1 adapter seam. P2 runs in the separate backend repository. The backend feature is wired into Loki, while arithmetic operations remain unsupported and P4 is in progress. The assigned docs now pin both repositories and distinguish their status.

- `docs/brand/IDENTITY.md:21-23` stated stock acceptance in both directions, verifying-key byte equality and Lean theorem equality without their evidence limits. `README.md:147` identifies the W2 cross-verification gate. `README.md:154` describes `Refinement.loki_proof_eq_stock` as conditional on a backend refining stock operations in the same order, and says runtime evidence does not complete the component proofs. `README.md:158` records that stock lookup preparation can produce different proof bytes across runs. The identity text could be read as a claim of unconditional byte equality or a completed proof of every implementation. C06, C08 and C28 now correct that scope.

- `docs/brand/IDENTITY.md:17` used the inline path `docs/PLATFORMS.md`. From that file, the path resolves under `docs/brand/docs/`; the roadmap is the sibling file `docs/PLATFORMS.md`.

## Questions

The review originally asked which loki-gpu revision the pages should describe. They now pin loki-gpu `main` at `d71b7ca` and Loki `main` at `a256a9a15ff18ae12a8131b86cff576787fa5e3c`. C28 defines "same proof" as the stock-verifier compatibility goal and records the lookup-order limitation.

## Suggestions

- Update `/home/charles/repos/loki-gpu-wt/docs/README.md` and `docs/OVERVIEW.md` to pin status to Loki `a256a9a15ff18ae12a8131b86cff576787fa5e3c` and loki-gpu `d71b7cafd4308206cc7f283b07a4e67baae89305`. State that P1 and P2 are merged, P4 is in progress and arithmetic operations remain unsupported. Done.
- In `docs/brand/IDENTITY.md`, link tested acceptance to W2, state the Lean theorem's refinement assumptions and distinguish verifier compatibility from byte equality with an independently run stock prover. Done under C06, C08 and C28.
- Link the platform reference with `[platform roadmap](../PLATFORMS.md)`. Done.
- In `docs/PLATFORMS.md`, link the source references in lines 27-40 and 45-47 to the Loki harness and loki-gpu design pages when canonical repository URLs are available. The current inline-code paths do not navigate to their sources.

## Novelty evidence

This review covered consistency, identity and cross-links. It did not assess novelty or prior art.
