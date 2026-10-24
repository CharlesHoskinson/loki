# Loki: definition of done

The supervisor loop keeps launching work packages until every gate below is green on `main`. Each gate names
the command or artifact that proves it.

| Gate | Meaning | Evidence |
|---|---|---|
| G1 Drop-in | Moriarty and recl9 build and pass tests with `[patch.crates-io] midnight-proofs = loki` | W6 branches, `cargo test` logs |
| G2 Stock-verifier acceptance | Loki proofs accepted by unmodified crates.io 0.8.1 and 0.8.3 verifiers, k10/14/17/20 | `loki-harness/run.sh` (W2) |
| G3 Soundness | mutated witness / corrupted proof / wrong instance / swapped VK rejected by both verifiers | W7 `soundness` fuzz ≥30 min/target, no findings; mutation test |
| G4 Differential | GPU MSM and NTT == CPU == naive on random and edge inputs | W7 `msm_diff`, `ntt_diff`; W4/W5 equality tests via gpu-runner |
| G5 Transcript order | Loki emits the stock Fiat-Shamir event sequence | Quint `transcript.qnt` MBT replay (W8) + Lean `Transcript` theorem |
| G6 Stateful parts | GPU dispatch/fallback, resident-SRS cache, PK cache, scheduler invariants hold | Quint models + MBT (W8) |
| G7 Formal spec | Lean spec accepted by the Opus auditor; obligations listed and mapped to tests | `loki-lean/AUDIT.md`, `SPEC.md` |
| G8 Speed | k20 Sherlock parent measured faster than stock on this host, same proofs accepted | W1 profile + bench JSON |
| G9 In use | Sherlock factory loop runs with Loki patched in; predicates unchanged or better | Moriarty `doctorwatson check` with patch |

Next packages, after G1-G9: resident GPU quotient (L4 full), tree scheduler (L5), fused k15 terminal (Track B, new keys,
owner decision required).
