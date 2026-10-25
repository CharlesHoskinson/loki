# luna-r5 review

## Concerns

- [docs/modules/poly-multiopen.md:15](../modules/poly-multiopen.md): The parallelism description omitted coefficient-level work. `prepare` parallelizes group-level combination and root-product division when there are multiple sets and `work >= 4096` (`src/poly/kzg/multiopen.rs:121,138`). `combine` also parallelizes coefficient chunks when the first polynomial has at least 4096 coefficients (`:83-90`), including a single-set call. The module page now describes both conditions.
- [docs/modules/poly-kzg.md:12,33](../modules/poly-kzg.md): `ParamsKZG::unsafe_setup` was described as "test-only," but it is a public method without a `cfg(test)` gate (`src/poly/kzg/params.rs:53`). Its source documentation says "MUST NOT be used in production" because it draws a toxic secret. The module page now identifies it as public and retains the production warning.
- The fact-change review found stale current-state claims about lookup leftover ordering. Current Loki main `a256a9a15ff18ae12a8131b86cff576787fa5e3c` uses `HashMap` iteration by default and sorts only with `loki-deterministic-lookup-order` (`src/plonk/lookup/prover.rs:425-433`, `Cargo.toml:74`). The W3 documentation baseline `1db6ab8b20d3410ce930808ac7ee9b78ff9ade16` always sorted leftovers. The assigned module pages now distinguish the current default and feature from the older measurement.

## Questions

None remain after the accepted decision to document the public API and preserve its production prohibition.

## Suggestions

- `poly-multiopen.md` now distinguishes group-level parallelism (`work >= 4096` and multiple sets) from coefficient-level `combine` parallelism (polynomial length `>= 4096`, including one set).
- `poly-kzg.md` now describes `unsafe_setup` as public and retains its production warning.

## Novelty evidence

This module accuracy review found no module-specific novelty claim to assess.
