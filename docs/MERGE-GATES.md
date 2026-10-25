# Merge gates: MERGE-CPU and RECEIPT-NATIVE

Why: on 2026-10-25 P3, P4, P5 and P7 were source-ACKed by ~10:30Z but none reached main for 7+ hours, because READY-MERGE required native GPU receipts and an orphaned W12 test held heavy.lock and gpu.lock. No tier of the gate could run without the locks.

## Tiers
| Tier | Command | Needs | Gates a merge? |
|---|---|---|---|
| MERGE-CPU | `scripts/verify-main.sh --tier cpu` | no GPU, no gpu.lock; `--heavy` takes heavy.lock only for large builds | yes |
| RECEIPT-NATIVE | `scripts/verify-main.sh --tier native` (sanitizer, device tests, GPU proofs; package scripts in deps.yaml `native_receipt`) | heavy.lock then gpu.lock | no: runs post-merge against the merged sha |

READY-MERGE now means: source ACK from the reviewer, MERGE-CPU green (fake-driver gates included), and the native receipt script named in deps.yaml. A native failure afterwards opens a bug on that package and blocks the next release/push, not the next merge. Merge commits carry the tag "MERGE-CPU green"; the native receipt is recorded against the merged sha in the package REVIEW.md.

Agents hold at most 30 minutes for a heavy slot; after that they post READY-MERGE (cpu tier) rather than ask whether to wait.

## Merge train
1. `scripts/loki-dag next` / `order` reads `deps.yaml` and prints what can merge and in what order. `check` shows branches whose base moved (a rebase invalidates sha-bound native receipts, so request them after the train, not before).
2. loki-gpu work: merge in `~/repos/loki-gpu` first, then `scripts/train.sh --gpu-repo ~/repos/loki-gpu --gpu-ref main ...` does the `git subtree pull --prefix=loki-gpu` inside the throwaway tree.
3. `scripts/train.sh $(scripts/loki-dag order)` (dry run): throwaway worktree from main, rebase each branch on the running tip, MERGE-CPU after each, `--no-ff` merge. Stops at the first conflict or red gate. Result is `refs/train/<stamp>/tip`; main is untouched. Delete refs with `git for-each-ref refs/train | ... update-ref -d`.
4. Coordinator decision: `LOKI_TRAIN_APPLY=1 scripts/train.sh --apply ...` fast-forwards local main (never pushes). Without the env var `--apply` refuses.
5. After apply, queue RECEIPT-NATIVE jobs for the merged sha in `loki-dag order`.

Ports that must be done by the branch owner when the train stops on a conflict: P3/P5/P7 tests to `with_fake` after P4's unsafe `GpuRuntime::with_driver`.

Self-test: `bash scripts/tests/train-selftest.sh` (seconds, throwaway repo, no cargo).
