# Loki identity

This guide sets Loki's name, story, colors and image style. Documentation and diagram authors follow it when writing prose or prompts.

## Name and lineage

**Loki** is the Norse trickster and shapeshifter. In the myths he is the father of Fenrir, the wolf who is bound beneath the moon.

Project Fenrir settles its proofs with Sherlock recursion. Loki makes those proofs. Loki is the prover; Fenrir is the system that depends on it.

Technically, Loki is a fork of `midnight-proofs` 0.8.3 (a halo2/PLONK prover with KZG commitments on BLS12-381). It drops into a build through `[patch.crates-io]`. It is a prover only: the verifier is the unmodified stock 0.8.3 verifier.

Write the name as **Loki**. The package is still called `midnight-proofs`, because the drop-in depends on it. Say "Loki" for the fork and "stock" for unmodified `midnight-proofs` 0.8.3.

## The metaphor: the shapeshifter and the oath

Loki's CPU and CUDA paths preserve the proof construction. Future backends must preserve it too ([platform roadmap](../PLATFORMS.md)). Here, "same proof" describes the compatibility goal. The W2 harness checks cross-verification in both directions and verifying-key byte equality for its listed cases. It does not establish universal byte equality with every independently run stock prover. Stock lookup preparation can choose a different valid permutation between runs, so lookup proof bytes can differ while both proofs verify. Loki defaults to stock HashMap leftover ordering. The opt-in `loki-deterministic-lookup-order` feature sorts those leftovers and can change lookup proof bytes relative to stock.

The stock verifier must accept proofs produced by Loki. The W2 harness is the test evidence for that contract:

- it checks acceptance in both directions and compares verifying-key bytes for the cases it runs;
- the default harness covers k10, k14 and k17.

The Lean theorem `Refinement.loki_proof_eq_stock` proves equality of abstract prover outputs under `Refines` and `SameOrder`, with equal inputs, randomness, thread count and fixed map order. It does not prove that the deployed Rust or CUDA code satisfies those assumptions. The code-binding obligations remain open.

Use this metaphor in titles and short summaries. State the evidence and limits in the body text.

## Tagline

**Same proof. Any shape.**

Short form for badges and headers: "Same proof. Any shape." Do not add a second tagline.

## Palette

Loki keeps Fenrir's night palette and adds emerald and gold. Fenrir and Loki diagrams share the night colors.

| Role | Name | Hex |
|---|---|---|
| Sky, diagram background | Night navy | `#0d1729` |
| Deep shadow, pixel-art sky | Abyss | `#010611` |
| Lines, panel borders | Slate | `#2a3a5a` |
| Secondary text, mountains | Mist | `#8296b0` |
| Light text, moon | Moonlight | `#f4ecda` |
| Pale highlight | Frost | `#b9c3d4` |
| Loki primary | Emerald flame | `#2fbf71` |
| Loki primary, dark core | Deep emerald | `#0f6b45` |
| Loki accent | Gold flame | `#e3b84e` |
| Warm glow, rarely | Ember | `#e85d2c` |
| CPU / GPU neutral highlight | Teal | `#4fc1b5` |
| Planned / future | Violet | `#a58bd9` |

Rules for use:
- Night navy is the base. Emerald is the main accent, gold the second. Use ember only for a small glow, since it is Fenrir's color.
- In diagrams, give measured and shipped parts an emerald or gold outline. Give **Planned** and **TODO** parts a dashed violet outline.
- Text needs contrast on its background in both light and dark mode. Diagrams on GitHub must work on a white page and on a dark page, so give them their own navy background panel.

## Typography

- Documentation uses plain Markdown. Titles are plain text, with no emoji.
- Diagrams use a system sans-serif stack (`system-ui, -apple-system, "Segoe UI", sans-serif`) for labels, and a monospace stack (`ui-monospace, "SF Mono", Menlo, monospace`) for code identifiers, feature flags and file names.
- Do not put text inside generated raster art. Text rendering is unreliable. Put text in Markdown or source SVG.

## Image style

Raster art (hero, mark, cover and section illustrations):
- 16-bit pixel art in the style of `fenrir-hero.png` in Project Fenrir: a Norse night, deep navy sky, a large moon, layered mountains in silhouette.
- Loki's own element is the green and gold flame, a serpent of flame, and a cloaked figure whose outline shifts into other shapes. The wolf (Fenrir) may appear as a silhouette.
- Floating runes glow emerald and gold, not orange.
- No text, letters or numbers in the image.
- Files stay under 1.5 MB.

Technical diagrams are SVG source in Fenrir's `docs/diagrams` style: flat shapes, navy panel, thin slate lines, labels checked by a person.

Files in `docs/brand/`:

| File | Use |
|---|---|
| `loki-hero.png` | wide banner under the README title |
| `loki-mark.png` | square logo, shown at 120 px |
| `loki-mark-small.png` | small logo for tight places (favicon size and listings) |

## Voice

- Short declarative sentences. Plain, precise technical English.
- No hype words ("blazing", "revolutionary", "seamless", "supercharged").
- Every claim can be checked against code or a cited note, for example `LOKI-MULTIOPEN.md`. Quote measured numbers with their source and do not round them up.
- State limits next to results. Kernel speedups alone barely moved the end-to-end time, and the GPU MSM slows down on repeated scalars. Say so.
- Mark planned or unimplemented work as **Planned** or **TODO**.

## Use

- Say that the stock verifier accepts Loki's proofs, and name the gate that shows it (the W2 harness).
- Use "drop-in" for the `[patch.crates-io]` route.
- Credit upstream: `midnight-proofs`, halo2 and sppark.
- Keep the `LOKI-*.md` notes linked from the documentation map.

## Avoid

- Do not claim a speedup without the measurement, host and note behind it.
- Do not call Loki a new proof system. It is the same proof system with a different prover.
- Do not add AI attribution, "Co-Authored-By" lines or generated-by banners to commits or documents.
- Do not use Fenrir's orange as Loki's main color.
- Do not put text in raster art, and do not hand-draw hardware logos.
