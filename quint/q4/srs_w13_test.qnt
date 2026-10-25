// -*- mode: Bluespec; -*-
module srs_w13_test {
  import srs_w13(REGISTRY = true, MUT = 0).* from "./srs_w13"

  // Literal totals asserted by backend.rs:500-502 (pinned sppark constructor, 170 SMs).
  run bytesVectorsTest = all {
    assert(residentBytes(1025, 170) == 2971648),
    assert(residentBytes(2^19, 170) == 55409664),
    assert(residentBytes(2^20, 170) == 109755392),
    // REPORT.md: two k19 + two k20 arrays at 170 SMs.
    assert(2 * residentBytes(2^19, 170) + 2 * residentBytes(2^20, 170) == 330330112),
  }

  // backend.rs:115: div_ceil(len, 4) never yields more than four chunks, and
  // a floor-based chunk length would (mutant M_FLOOR_CHUNK).
  run workerBoundTest = all {
    assert(range(4096, 4096 + 3000).foldl(true, (a, n) => a and workerCount(n, M_NONE) <= 4)),
    assert(range(2^20 - 64, 2^20 + 64).foldl(true, (a, n) => a and workerCount(n, M_NONE) == 4)),
    assert(workerCount(4097, M_FLOOR_CHUNK) == 5),
    assert(workerCount(4095, M_NONE) == 0),
  }

  // Join discipline: every spawned worker is joined; the first failure in
  // join order is reported; a spawn error wins (backend.rs:126-146).
  run joinTest = all {
    assert(joinWorkers([WErr, WPanic, WOk, WPanic], -1, M_NONE) == { spawned: 4, joined: 4, err: 0, unwound: false }),
    assert(joinWorkers([WOk, WOk, WOk, WOk], 2, M_NONE) == { spawned: 2, joined: 2, err: -2, unwound: false }),
    assert(joinWorkers([WErr, WPanic, WOk, WPanic], -1, M_SHORT_JOIN).unwound),
  }

  // Coefficient bases are prefix-stable; Lagrange bases of different sizes are not.
  run basisTest = all {
    assert(coef(5).slice(0, 3) == coef(3)),
    assert(lag(5).slice(0, 3) != lag(3)),
    assert(lag(4).nth(0) != coef(4).nth(0)),
  }

  // Rewarm, then mutate at the same address: the next call must miss and re-upload.
  run rewarmMutateMissTest =
    init.then(all { devices' = [{ sm: 170, eligible: true, queryOk: true }], budget' = DEFAULT_BUDGET,
                    memory' = memory, label' = label, entries' = entries, native' = native,
                    nextHandle' = nextHandle, failed' = failed, lastOp' = lastOp, lastPar' = lastPar,
                    nativeAfterFail' = nativeAfterFail })
      .then(msmCall(0, 4, 0))
      .then(msmCall(0, 4, 1))
      .then(all { assert(lastOp.hit and lastOp.outcome == GpuOk), mutate(0, 3, { e: 6, r: 0 }) })
      .then(msmCall(0, 4, 1))
      .then(all { assert(not(lastOp.hit) and lastOp.staleRejected and lastOp.result == lastOp.expected
                         and entries.length() == 1 and entries.nth(0).handle == 1),
                  msmCall(0, 3, 0) })
      .then(all { assert(lastOp.hit and lastOp.handle == 1), unchangedCache, memory' = memory,
                  label' = label, lastOp' = lastOp, lastPar' = lastPar })
}
