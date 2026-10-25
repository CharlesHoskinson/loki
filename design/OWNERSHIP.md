# Design ownership

CODE (coordination): one writer per file; read-only cross-review is allowed. Every owner also owns `design/gluna-N/**`; each scout owns only `design/research/ghaiku-N/**`. All work is Apache-2.0 and the owner brief's clean-room exclusions apply. Agents must not edit `.git`, other worktrees, or Loki production sources. Commits go through gpu-runner-2 after lead review.

| Owner | Exclusive files |
|---|---|
| astra-gpu | design/ARCHITECTURE.md, PLAN.md, OWNERSHIP.md, REVIEW.md, QUOTIENT.md, NTT.md, design/verification/** |
| gluna-1 | crates/loki-backend-api/Cargo.toml, crates/loki-backend-api/src/**, design/LOKI-PATCH-PLAN.md |
| gluna-2 | gpu-spec/LokiLean/Backend.lean, gpu-spec/LokiLean/BackendQuotient.lean, gpu-spec/LokiLean.lean |
| gluna-3 | gpu-spec/LokiLean/BackendMsm.lean, gpu-spec/LokiLean/BackendNtt.lean |
| gluna-4 | design/MSM.md, crates/loki-gpu/src/msm.rs, crates/loki-gpu/cuda/msm.cu |
| gluna-5 | crates/loki-gpu/src/ntt.rs, crates/loki-gpu/cuda/ntt.cu; initial NTT design/Lean credited before transfer |
| gluna-6 | None after 2026-10-25 05:15 UTC transfer; STOP sent |
| gluna-7 | design/RUNTIME.md, crates/loki-gpu/src/runtime.rs, crates/loki-gpu/src/srs.rs |
| gluna-8 | Cargo.toml, Cargo.lock, .gitignore, crates/loki-gpu/Cargo.toml, build.rs, src/lib.rs, src/ffi.rs, src/quotient.rs, cuda/quotient.cu, cuda/include/**, design/BUILD.md, .github/workflows/** |
| gluna-9 | design/TEST-PLAN.md |
| gluna-10 | design/RECURSION.md, design/PROVENANCE.md |

CODE (paths): `gpu-spec/` above means `/home/charles/repos/loki-lean-wt/gpu-spec/`, not a directory in this repository. Source pins are recorded in the design, patch plan, and provenance documents.

CODE (review ring): gluna-1 reviews runtime/SRS; gluna-2 reviews BackendMsm; gluna-3 reviews BackendNtt; gluna-4 reviews build/FFI; gluna-5 reviews MSM; gluna-6 reviews generic Backend; gluna-7 reviews recursion/scheduler; gluna-8 reviews API/patch plan; gluna-9 reviews quotient; gluna-10 reviews tests. Each reviewer writes `design/gluna-N/PEER-REVIEW.md`, sends findings to the owner and lead, and reviews corrections. The lead reviews every draft and reconciles interfaces.

CODE (lead claim receipts): ARCHITECTURE hold 01M5RPN29ZYHP0KZBW1BFY90X6; PLAN hold 01M5RPN2HVFW900M9N9EXCRT7X; OWNERSHIP hold 01M5RPN2QJJ558GE5NK3XDQKCD; REVIEW hold 01M5RPN309EEJ0TSYWBKF2MK8T. All granted through 2026-10-25 08:56 UTC. These are coordination tokens, not credentials.

CODE (recovery): gluna-6 never acknowledged the assignment or produced files before the announced 05:15 UTC deadline. Lead explicitly revoked ownership and sent STOP before transferring files to the owners above. gluna-1 covers the generic Backend review in addition to runtime; gluna-9 still reviews quotient. No claim is made that the unavailable engineer completed a review.

CODE (NTT review recovery): after gluna-5 delivered the initial NTT draft and MSM review, blocking finite-state/algorithm-composition revisions remained unacknowledged through the announced 05:20 UTC deadline. Lead revoked those two paths before transfer: BackendNtt to gluna-3, NTT.md to lead. Lead independently reviews the correction; the original peer findings remain recorded.

CODE (final verification handoff): after accepting gluna-8’s frozen build/FFI work, lead sent STOP and took temporary `.cargo/config.toml`, final runner submissions and BUILD.md receipt edits. The temporary file supplies only toolkit environment variables, is excluded from commits, and is removed after the checks. All other engineer files are frozen; initial ownership/credits above remain historical.

## Opus implementation handoffs

CODE (coordination, 2026-10-25): the table above records initial design-team
credits; all original design engineers/scouts were sent STOP after handoff.
Implementation ownership below supersedes those historical credits for new
work. The coordinator creates worktrees and controls integration; one writer
per logical file remains required even when drafts use separate worktrees.

| Active owner | Scope and handoff constraint |
|---|---|
| loki-p2, corrective `p2-fix` | Runtime/FFI/native-runtime/header and associated tests for REVIEW.md D1–D5. Owns cancellation mappings after the API addition at `cefecbc`. P3/P4 may propose patches but cannot take these shared files until correction review/merge and explicit handoff. P2 owns any correction to `Backend.lean` runtime model. |
| loki-p3 | P3 SRS registry and legacy adapter files assigned by the coordinator; newly approved `gpu-spec/LokiLean/BackendSrs.lean` and its import in `gpu-spec/LokiLean.lean`. Claim both before writing. Prefix/capacity proof and actual registry gates are required before P3 promotion. Runtime byte-form seam waits for P2-fix and a serialized P3/P4 handoff. Loki manifests/dispatch file ownership is coordinated separately; disjoint functions do not authorize simultaneous file writers. |
| loki-p4 | P4-owned NTT kernels/oracles/tests and NTT specification. Proposed runtime launch/FFI/header seam waits for P2-fix and an explicit shared-file handoff, sequenced with P3. Vendored permitted sppark field headers require exact source/version/hash, preserved headers and NOTICE/PROVENANCE updates. |
| astra-gpu | Independent design reviews and coordination; review/build/runtime docs and API edits require a fresh claim. The `Cancelled` API and P2 review are committed at `cefecbc`; those claims were released. No implementation worktree is edited by the lead. |

CODE: P3's point codec must bind concrete type, curve ABI and wire version;
mutable registration cannot reinterpret existing leases. A local adapter type
can implement an external codec trait without implementing it for a foreign
curve type. Any registry design still needs concrete validation and exact
snapshot comparisons. The new SRS Lean module must prove consumed-prefix
refinement, generation rejection and lease/accounting properties with concrete
nonempty examples, not merely restate an assumed cache-hit predicate. Existing
full-sequence `SoundHit` does not already prove capacity/prefix reuse.


SOURCE/CODE (coordinator handoff, 2026-10-25 07:02 UTC): P2-fix `09fd92a`
merged as `eb27233`. Coordinator transferred `runtime.rs`, `ffi.rs`,
`cuda/runtime.cu`, and `cuda/include/loki_gpu_ffi.h` to loki-p4. P4 rebases its
shared-file patch before application; P3 sends its point-wire seam to P4 for
serialized integration or obtains a later explicit transfer. P2-fix remains
the correction credit; its earlier exclusive ownership row is historical
from this handoff onward. P4 also owns the lead's follow-up to propagate
`cudaGetDevice` errors immediately from `ensure_device`.
