# Coordinator screenshot check of architecture diagrams

These observations concern the initial rendering against main `a256a9a`, before the fixes recorded in [REVIEW.md](REVIEW.md). Final screenshot outcomes are recorded there.

- **pipeline.svg**: minor FAIL. The "y" box needs the sub-label "CPU + GPU NTT". The legend should note that the seam is on main but loki-gpu returns Unsupported. "beta, gamma" and "Quotient commitments" sit tight against their box edges.
- **layers.svg**: FAIL, stale. src/backend/, loki-backend-api and loki-gpu (skeleton) are on main as optional dependencies. Move them out of Planned, and update the header, desc and legend.
- **correctness.svg**: PASS. Cosmetic: the first line of "The oath" is smaller than the rest.
- **recursion.svg**: minor FAIL. The box at (30,466) should say that the seam is on main and the loki-gpu implementation is planned. The arity-4 footnote is unverified; check ~/repos/fenrir-loki.
- **features.svg**: FAIL.
  - Restyle loki-backend-api, loki-sppark, loki-gpu-backend and loki-deterministic-lookup-order as on main, and drop the "branch" tags.
  - Show loki-gpu as an optional dependency on main.
  - The excludes edges are enforced by compile_error! at src/lib.rs:14-17.
  - The "excludes" label (x 567-667, y 246-268) hides the cuda→loki-cuda-runtime edge. Move it.
  - loki-argument-commit is unverified.
- **ARCHITECTURE.md**: sections 1, 2, 6, 7, the feature table, and lines 7, 61, 166, 266-278 and 311 still say main is 1db6ab8 and that P1 is branch-only. Main is now a256a9a, with P1 merged.
- Screenshots: scratchpad/loki-docs/shots/haiku-arch/*.png.
