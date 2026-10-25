# Loki: definition of done

The supervisor loop continues until every gate below is green on `main`. Each row lists the command or artifact used to check it.

| Gate | Meaning | Evidence |
|---|---|---|
| G1 Drop-in | Moriarty and recl9 build and pass tests with `[patch.crates-io] midnight-proofs = loki` | W6 branches, `cargo test` logs |
| G2 Stock-verifier acceptance | Loki proofs accepted by unmodified crates.io 0.8.1 and 0.8.3 verifiers, k10/14/17/20 | `loki-harness/run.sh` (W2) records 0.8.3 k10/14/17. The 0.8.1 and k20 cases remain open under T04 in `TODO.md`. |
| G3 Soundness | Invalid witness mutations, corrupted proofs, wrong instances and swapped VKs are rejected by both verifiers | W7 `soundness` fuzz ≥30 min/target, no findings; mutation test. A witness mutation counts as invalid only when `MockProver` rejects it. W7 records that `proof_roundtrip` and `soundness` inputs stayed between 4 and 6 bytes, so those runs were close to seeded random testing. See `LOKI-FUZZ.md`. |
| G4 Differential | GPU MSM and NTT == CPU == naive on random and edge inputs | W7 `msm_diff`, `ntt_diff`; W4/W5 equality tests via gpu-runner |
| G5 Transcript order | Loki emits the stock Fiat-Shamir event sequence for the tested supported shapes | Quint `transcript.qnt` MBT replay (W8) + Lean `Transcript` theorem. Stock rejects mixed committed/plain instances across multiple proofs; see `LOKI-QUINT.md`. |
| G6 Stateful parts | GPU dispatch/fallback, resident-SRS cache, PK cache, scheduler invariants hold | Quint models + MBT (W8) for implemented paths; scheduler safety is model-only and liveness remains T29 in `TODO.md`. |
| G7 Formal spec | Lean spec accepted by the formal review; obligations listed and mapped to tests | `loki-lean/AUDIT.md`, `SPEC.md` |
| G8 Speed | k20 Sherlock parent measured faster than stock on this host, with stock accepting the proof from each run | W1 profile + bench JSON |
| G9 In use | Sherlock factory loop runs with Loki patched in; predicates unchanged or better | Moriarty `doctorwatson check` with patch |

After G1-G9, the next packages are the resident GPU quotient (L4 full), tree scheduler (L5), and fused k15 terminal (Track B, new keys, owner decision required).
