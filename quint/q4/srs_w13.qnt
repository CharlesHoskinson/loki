// -*- mode: Bluespec; -*-
// quint-q4: Loki W13 GPU SRS caching, CPU-visible logic, at a256a9a.
//
// Mirrors src/poly/kzg/msm/cuda/backend.rs (feature `cuda-srs-registry` on:
// REGISTRY = true; off: the legacy two-slot cache) and the native resident
// allocation of cuda/msm.cu + pinned sppark 0.1.16 (msm/pippenger.cuh:351-384,
// util/all_gpus.cpp:11-54). The source-correspondence table is in REPORT.md.
//
// Abstraction. A model point {e, r} is a group element id `e` with a
// projective representation `r` (a Jacobian scaling). e = OFF_CURVE is not on
// the curve, e = TORSION is on the curve but outside prime-order G1. The Rust
// replay expands every model point to BLOCK identical real points, so a slice
// of b blocks has b * BLOCK real bases, and a request of n real scalars
// consumes ceil(n / BLOCK) blocks. This keeps the 4096-point native-worker
// threshold and the exact byte formula at their real values.
//
// MUT selects a deliberately wrong variant (0 = the real code). Every safety
// invariant is broken by at least one mutant (see srs_w13_mutants.qnt).
module srs_w13 {
  const REGISTRY: bool
  const MUT: int

  // ---- mutant identifiers ------------------------------------------------
  pure val M_NONE = 0
  pure val M_ADDR_ONLY = 1        // hit on address + capacity, no content compare
  pure val M_FIRST_BLOCK = 2      // compare only the first consumed block
  pure val M_NO_LEN = 3           // registry: skip `n <= capacity`
  pure val M_ORDINAL0_SM = 4      // astra finding: charge CUDA ordinal 0's SMs
  pure val M_CHARGE_N = 5         // charge the requested n, not the capacity
  pure val M_NO_STALE_EVICT = 6   // keep stale capacities at the same address
  pure val M_BUDGET_USED = 7      // evict while used > budget (forgets new bytes)
  pure val M_RELEASE_DROP = 8     // drop the entry when its release fails
  pure val M_SHORT_JOIN = 9       // pre-fix short-circuit worker join
  pure val M_FLOOR_CHUNK = 10     // chunk_len = len / 4 instead of div_ceil
  pure val M_VALIDATE_PREFIX = 11 // validate only the consumed prefix on a miss
  pure val M_SKIP_FAILED = 12     // ignore the sticky failed flag
  pure val M_LEGACY_LEN = 13      // legacy: hit when capacity >= n (not ==); benign control
  pure val M_EVICT_NO_RELEASE = 14 // drop evicted entries without a native release
  pure val M_RELEASE_KEEP = 15    // release natively but keep the entry
  pure val M_LEGACY_THREE = 16    // legacy: evict only at three entries
  pure val M_REFRESH_SNAPSHOT = 17 // a hit refreshes the snapshot from live memory

  // ---- real constants ----------------------------------------------------
  pure val BLOCK = 1024           // replay granularity (driver constant)
  pure val MAXB = 5               // blocks per slice: 1024 .. 5120 bases
  pure val PAR_MIN = 4096         // backend.rs:112
  pure val WORKERS = 4            // backend.rs:115
  pure val ADDRS = Set(0, 1)
  pure val OFF_CURVE = 0
  pure val TORSION = 7
  pure val DEFAULT_BUDGET = 4294967296 // backend.rs:189

  type Point = { e: int, r: int }
  pure def valid(p: Point): bool = p.e != OFF_CURVE and p.e != TORSION
  pure val POINTS: Set[Point] = tuples(0.to(7), 0.to(1)).map(t => { e: t._1, r: t._2 })

  type Basis = Coef | Lag | Other
  // Coefficient bases are prefix-stable across domains (tau^i G); a Lagrange
  // basis is different for every domain size.
  pure def coef(len: int): List[Point] = range(0, len).foldl([], (l, k) => l.append({ e: k + 1, r: 0 }))
  pure def lag(len: int): List[Point] =
    range(0, len).foldl([], (l, k) => l.append({ e: ((k + len) % 6) + 1, r: 0 }))

  // sppark keeps a device only when major >= 7 and cooperativeLaunch
  // (all_gpus.cpp:21-24). queryOk models cudaDeviceGetAttribute success.
  type Device = { sm: int, eligible: bool, queryOk: bool }
  pure val DEVICE_CONFIGS: Set[List[Device]] = Set(
    [{ sm: 170, eligible: true, queryOk: true }],                                       // supplied host
    [{ sm: 28, eligible: false, queryOk: true }, { sm: 170, eligible: true, queryOk: true }], // ordinal 0 ineligible, fewer SMs
    [{ sm: 200, eligible: false, queryOk: true }, { sm: 84, eligible: true, queryOk: true }], // ineligible with more SMs
    [{ sm: 84, eligible: true, queryOk: true }, { sm: 170, eligible: true, queryOk: true }],  // two eligible, index 0 smaller
    [{ sm: 170, eligible: true, queryOk: true }, { sm: 1, eligible: false, queryOk: false }]  // unrelated device query fails
  )

  type Entry = { addr: int, len: int, blocks: List[Point], handle: int, bytes: int }
  // Native ground truth: what sppark actually allocated and uploaded.
  type Native = { live: bool, bytes: int, elems: List[int], len: int, releaseFailed: bool, tracked: bool }
  type Fault = NoFault | WorkerErr | WorkerPanic | SpawnErr | PrepareCtorErr | PrepareSyncErr | ReleaseErr | InvokeErr
  type Outcome = Idle | Empty | Disabled | GpuOk | GpuErr
  type LastOp = {
    outcome: Outcome, addr: int, n: int, hit: bool, staleRejected: bool,
    result: List[int], expected: List[int], workers: int, unwound: bool,
    evictions: int, handle: int, error: str
  }
  type ParResult = { spawned: int, joined: int, err: int, unwound: bool }
  pure val NO_PAR: ParResult = { spawned: 0, joined: 0, err: -1, unwound: false }
  type WOut = WOk | WErr | WPanic

  pure val IDLE: LastOp = {
    outcome: Idle, addr: -1, n: 0, hit: false, staleRejected: false, result: [], expected: [],
    workers: 0, unwound: false, evictions: 0, handle: -1, error: ""
  }

  // ---- state -------------------------------------------------------------
  var memory: int -> List[Point]       // live host bases at each stable address
  var label: int -> Basis              // ghost: what was last written there
  var entries: List[Entry]             // Cache.entries, LRU front .. MRU back
  var native: int -> Native            // sppark resident handles
  var nextHandle: int
  var failed: bool                     // Cache.failed (sticky)
  var budget: int                      // LOKI_GPU_SRS_BUDGET_BYTES
  var devices: List[Device]
  var lastOp: LastOp
  var lastPar: ParResult               // standalone parallel_chunks probe
  var nativeAfterFail: int             // native calls issued while disabled

  // ---- pure helpers ------------------------------------------------------
  pure def min(a: int, b: int): int = if (a < b) a else b
  pure def max(a: int, b: int): int = if (a > b) a else b
  pure def ceilDiv(a: int, b: int): int = (a + b - 1) / b
  pure def ilog2(x: int): int = range(0, 48).foldl(0, (acc, i) => if (2^i <= x) i else acc)
  pure def clamp(x: int, lo: int, hi: int): int = if (x < lo) lo else if (x > hi) hi else x
  pure def elems(l: List[Point]): List[int] = l.foldl([], (acc, p) => acc.append(p.e))
  pure def allValid(l: List[Point]): bool = l.foldl(true, (acc, p) => acc and valid(p))
  pure def removeAt(l: List[Entry], i: int): List[Entry] =
    range(0, l.length()).foldl([], (acc, j) => if (j == i) acc else acc.append(l.nth(j)))
  pure def firstIdx(l: List[Entry], p: (Entry) => bool): int =
    range(0, l.length()).foldl(-1, (acc, i) => if (acc < 0 and p(l.nth(i))) i else acc)
  pure def used(l: List[Entry]): int = l.foldl(0, (acc, e) => acc + e.bytes)

  // backend.rs:197-217 == pippenger.cuh:351-375 (npoints rounded to WARP_SZ).
  pure def residentBytes(cap: int, sm: int): int = {
    val rounded = ceilDiv(cap, 32) * 32
    val wbits = if (rounded > 192) clamp(ilog2(rounded + rounded / 2) - 8, 10, 18) else 10
    val rows = (254 / wbits + 1) * 2^(wbits - 1)
    rounded * 96 + (rows + sm * 8) * 192 + rows * 4
  }
  // backend.rs:220-243: maximum over every CUDA ordinal (mutant: ordinal 0).
  pure def chargedSm(devs: List[Device], mut: int): int =
    if (mut == M_ORDINAL0_SM) devs.nth(0).sm else devs.foldl(0, (acc, d) => max(acc, d.sm))
  pure def queriesOk(devs: List[Device]): bool = devs.foldl(true, (acc, d) => acc and d.queryOk)
  // select_gpu(0): first eligible device (all_gpus.cpp:39-54).
  pure def anyEligible(devs: List[Device]): bool = devs.foldl(false, (acc, d) => acc or d.eligible)
  pure def selectedSm(devs: List[Device]): int =
    devs.foldl(-1, (acc, d) => if (acc < 0 and d.eligible) d.sm else acc)

  // backend.rs:108-148. chunk_len = div_ceil(len, 4); every spawned job is joined.
  pure def chunkLen(len: int, mut: int): int = if (mut == M_FLOOR_CHUNK) max(1, len / WORKERS) else ceilDiv(len, WORKERS)
  pure def workerCount(len: int, mut: int): int = if (len < PAR_MIN) 0 else ceilDiv(len, chunkLen(len, mut))
  // outs: outcome of each chunk worker; spawnFailAt: index of the failing spawn (-1 none).
  // err: -1 ok, -2 spawn error, else the first failing chunk (join order).
  pure def joinWorkers(outs: List[WOut], spawnFailAt: int, mut: int): ParResult = {
    val spawned = if (spawnFailAt >= 0 and spawnFailAt < outs.length()) spawnFailAt else outs.length()
    val firstBad = range(0, spawned).foldl(-1, (acc, i) => if (acc < 0 and outs.nth(i) != WOk) i else acc)
    val err = if (spawned < outs.length()) -2 else firstBad
    if (mut == M_SHORT_JOIN) {
      // Pre-fix: collect::<Result<_,_>>() stops at the first failure; scope
      // cleanup then joins the rest implicitly and re-raises a later panic.
      val stop = if (firstBad < 0) spawned else firstBad + 1
      val laterPanic = range(stop, spawned).foldl(false, (acc, i) => acc or outs.nth(i) == WPanic)
      { spawned: spawned, joined: stop, err: err, unwound: laterPanic }
    } else {
      { spawned: spawned, joined: spawned, err: err, unwound: false }
    }
  }
  // A parallel phase of `len` points under fault f: the first worker fails.
  pure def phase(len: int, f: Fault, mut: int): ParResult = {
    val k = workerCount(len, mut)
    if (k == 0) NO_PAR
    else {
      val outs = range(0, k).foldl([], (acc, i) => acc.append(
        if (i == 0 and f == WorkerErr) WErr else if (i == 0 and f == WorkerPanic) WPanic
        else if (i == k - 1 and f == WorkerPanic) WPanic else WOk))
      joinWorkers(outs, if (f == SpawnErr) k - 1 else -1, mut)
    }
  }
  pure def parFault(f: Fault): bool = f == WorkerErr or f == WorkerPanic or f == SpawnErr

  // Hit predicate, backend.rs:274-282 (registry) and 286-294 (legacy).
  pure def prefixEq(src: List[Point], mem: List[Point], cb: int, mut: int): bool = {
    val k = if (mut == M_FIRST_BLOCK) min(cb, 1) else cb
    range(0, k).foldl(true, (acc, i) => acc and i < src.length() and i < mem.length() and src.nth(i) == mem.nth(i))
  }
  pure def isHit(e: Entry, addr: int, n: int, mem: List[Point], mut: int): bool = {
    val cb = ceilDiv(n, BLOCK)
    if (REGISTRY)
      e.addr == addr and (n <= e.len or mut == M_NO_LEN) and
      (mut == M_ADDR_ONLY or prefixEq(e.blocks, mem, min(cb, ceilDiv(e.len, BLOCK)), mut))
    else
      e.addr == addr and (e.len == n or (mut == M_LEGACY_LEN and n <= e.len)) and
      (mut == M_ADDR_ONLY or prefixEq(e.blocks, mem, cb, mut))
  }

  // Eviction loop, backend.rs:331-347: stale same-address capacities first,
  // then LRU front while used > budget - bytes. Only the first release can fail.
  type Evict = { entries: List[Entry], released: List[int], err: bool, failedHandle: int, done: bool, oob: bool }
  pure def evict(es: List[Entry], addr: int, bud: int, bytes: int, relFail: bool, mut: int): Evict =
    range(0, 8).foldl({ entries: es, released: [], err: false, failedHandle: -1, done: false, oob: false }, (st, _i) =>
      if (st.err or st.done) st else {
        val same = if (mut == M_NO_STALE_EVICT) -1 else firstIdx(st.entries, e => e.addr == addr)
        val over = if (mut == M_BUDGET_USED) used(st.entries) > bud else used(st.entries) > bud - bytes
        val idx = if (same >= 0) same else if (over) 0 else -1
        if (idx < 0) { ...st, done: true }
        else if (idx >= st.entries.length()) { ...st, oob: true, done: true }
        else if (relFail and st.released.length() == 0) {
          val h = st.entries.nth(idx).handle
          if (mut == M_RELEASE_DROP) { ...st, entries: removeAt(st.entries, idx), err: true, failedHandle: h }
          else { ...st, err: true, failedHandle: h }
        } else if (mut == M_EVICT_NO_RELEASE) { ...st, entries: removeAt(st.entries, idx) }
        else if (mut == M_RELEASE_KEEP) { ...st, released: st.released.append(st.entries.nth(idx).handle), done: true }
        else { ...st, entries: removeAt(st.entries, idx), released: st.released.append(st.entries.nth(idx).handle) }
      })

  pure def releaseAll(nat: int -> Native, hs: List[int]): int -> Native =
    hs.foldl(nat, (m, h) => m.set(h, { ...m.get(h), live: false, tracked: false }))

  // ---- actions -----------------------------------------------------------
  action init = {
    nondet devs = DEVICE_CONFIGS.oneOf()
    val one = residentBytes(MAXB * BLOCK, chargedSm(devs, M_NONE))
    nondet bud = Set(DEFAULT_BUDGET, 2 * one + 1, 3 * one, residentBytes(2 * BLOCK, chargedSm(devs, M_NONE))).oneOf()
    all {
      memory' = Map(0 -> coef(4), 1 -> lag(4)),
      label' = Map(0 -> Coef, 1 -> Lag),
      entries' = [], native' = Map(), nextHandle' = 0, failed' = false,
      budget' = bud, devices' = devs, lastOp' = IDLE, lastPar' = NO_PAR, nativeAfterFail' = 0,
    }
  }

  // GPU replay host: one RTX 5090 (170 SMs), budgets as in init.
  action initGpu = {
    val devs = [{ sm: 170, eligible: true, queryOk: true }]
    val one = residentBytes(MAXB * BLOCK, 170)
    nondet bud = Set(DEFAULT_BUDGET, 2 * one + 1, residentBytes(2 * BLOCK, 170)).oneOf()
    all {
      memory' = Map(0 -> coef(4), 1 -> lag(4)),
      label' = Map(0 -> Coef, 1 -> Lag),
      entries' = [], native' = Map(), nextHandle' = 0, failed' = false,
      budget' = bud, devices' = devs, lastOp' = IDLE, lastPar' = NO_PAR, nativeAfterFail' = 0,
    }
  }

  action unchangedCache = all {
    entries' = entries, native' = native, nextHandle' = nextHandle, failed' = failed,
    budget' = budget, devices' = devices, nativeAfterFail' = nativeAfterFail,
  }

  // Free + reallocate (or overwrite) a stable address with a whole array.
  action writeArray(addr: int, basis: Basis, len: int): bool = all {
    memory' = memory.set(addr, if (basis == Coef) coef(len) else lag(len)),
    label' = label.set(addr, basis),
    unchangedCache, lastOp' = IDLE, lastPar' = lastPar,
  }

  // In-place mutation of one block at the same address (new value, new
  // representation of the same value, off-curve or torsion point).
  action mutate(addr: int, i: int, p: Point): bool = all {
    i < memory.get(addr).length(),
    memory' = memory.set(addr, memory.get(addr).replaceAt(i, p)),
    label' = label.set(addr, Other),
    unchangedCache, lastOp' = IDLE, lastPar' = lastPar,
  }

  // Standalone probe of parallel_chunks' join discipline (backend.rs:108-148).
  action parJoin(len: int, outs: List[WOut], spawnFailAt: int): bool = all {
    outs.length() == workerCount(len, MUT),
    lastPar' = joinWorkers(outs, spawnFailAt, MUT),
    memory' = memory, label' = label, unchangedCache, lastOp' = IDLE,
  }

  // backend::msm(scalars, bases) with |scalars| = nb * BLOCK - trim.
  action msmBody(addr: int, nb: int, trim: int, f: Fault): bool = {
    val mem = memory.get(addr)
    val memLen = mem.length() * BLOCK
    val n = min(max(nb * BLOCK - trim, 0), memLen)       // backend.rs:394
    val cb = ceilDiv(n, BLOCK)
    val expected = elems(mem.slice(0, cb))
    val op0 = { ...IDLE, addr: addr, n: n, expected: expected }
    val disabled = failed and MUT != M_SKIP_FAILED
    val hitIdx = firstIdx(entries, e => isHit(e, addr, n, mem, MUT))
    val stale = entries.foldl(false, (acc, e) => acc or (e.addr == addr and n <= e.len and not(prefixEq(e.blocks, mem, cb, M_NONE))))
    // Supplied capacity: registry passes the whole slice, legacy only bases[..n].
    val upLen = if (REGISTRY) memLen else n
    val upBlocks = mem.slice(0, ceilDiv(upLen, BLOCK))
    val validated = if (REGISTRY and MUT == M_VALIDATE_PREFIX) mem.slice(0, cb) else upBlocks
    val cmpPar = if (hitIdx >= 0 or stale) phase(if (REGISTRY) n else 0, f, MUT) else NO_PAR
    val valPar = phase(if (REGISTRY) upLen else 0, f, MUT)
    val bytes = residentBytes(if (MUT == M_CHARGE_N) n else upLen, chargedSm(devices, MUT))
    val afterFail = if (failed and MUT == M_SKIP_FAILED) nativeAfterFail + 1 else nativeAfterFail
    if (n == 0) all {
      lastOp' = { ...op0, outcome: Empty }, unchangedCache,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else if (disabled) all {
      lastOp' = { ...op0, outcome: Disabled }, unchangedCache,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else if (REGISTRY and cmpPar.err != -1) all {
      // same_prefix worker failure (backend.rs:277 `?`), state unchanged, then disabled.
      lastOp' = { ...op0, outcome: GpuErr, workers: cmpPar.spawned, unwound: cmpPar.unwound, staleRejected: stale, error: "worker" },
      failed' = true, entries' = entries, native' = native, nextHandle' = nextHandle,
      budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else if (hitIdx >= 0) {
      val e = entries.nth(hitIdx)
      val e2 = if (MUT == M_REFRESH_SNAPSHOT) { ...e, blocks: mem.slice(0, min(e.blocks.length(), mem.length())) } else e
      val es = removeAt(entries, hitIdx).append(e2)
      val res = native.get(e.handle).elems.slice(0, min(cb, native.get(e.handle).elems.length()))
      all {
        lastOp' = { ...op0, outcome: if (f == InvokeErr) GpuErr else GpuOk, hit: true,
                    result: if (f == InvokeErr) [] else res, workers: cmpPar.spawned,
                    handle: e.handle, error: if (f == InvokeErr) "invoke" else "" },
        entries' = es, failed' = (failed or f == InvokeErr),
        native' = native, nextHandle' = nextHandle, budget' = budget, devices' = devices,
        nativeAfterFail' = afterFail, memory' = memory, label' = label, lastPar' = lastPar,
      }
    } else if (REGISTRY and valPar.err != -1) all {
      lastOp' = { ...op0, outcome: GpuErr, workers: max(cmpPar.spawned, valPar.spawned), unwound: valPar.unwound or cmpPar.unwound, staleRejected: stale, error: "worker" },
      failed' = true, entries' = entries, native' = native, nextHandle' = nextHandle,
      budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else if (not(allValid(validated))) all {
      lastOp' = { ...op0, outcome: GpuErr, workers: max(cmpPar.spawned, valPar.spawned), staleRejected: stale, error: "invalid" },
      failed' = true, entries' = entries, native' = native, nextHandle' = nextHandle,
      budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else if (REGISTRY and (not(queriesOk(devices)) or bytes > budget)) all {
      lastOp' = { ...op0, outcome: GpuErr, workers: max(cmpPar.spawned, valPar.spawned), staleRejected: stale,
                  error: if (not(queriesOk(devices))) "query" else "budget" },
      failed' = true, entries' = entries, native' = native, nextHandle' = nextHandle,
      budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
      memory' = memory, label' = label, lastPar' = lastPar,
    } else {
      // Registry eviction loop, or legacy pop_front when two entries exist.
      val ev = if (REGISTRY) evict(entries, addr, budget, bytes, f == ReleaseErr, MUT)
               else if (entries.length() == (if (MUT == M_LEGACY_THREE) 3 else 2)) {
                 val h = entries.nth(0).handle
                 // backend.rs:351-353: pop_front, then release; on error the entry is gone.
                 if (f == ReleaseErr) { entries: removeAt(entries, 0), released: [], err: true, failedHandle: h, done: true, oob: false }
                 else { entries: removeAt(entries, 0), released: [h], err: false, failedHandle: -1, done: true, oob: false }
               } else { entries: entries, released: [], err: false, failedHandle: -1, done: true, oob: false }
      val nat1 = releaseAll(native, ev.released)
      val nat2 = if (ev.err) nat1.set(ev.failedHandle, { ...nat1.get(ev.failedHandle), releaseFailed: true,
                   tracked: ev.entries.foldl(false, (acc, x) => acc or x.handle == ev.failedHandle) }) else nat1
      val workers = max(cmpPar.spawned, valPar.spawned)
      val h = nextHandle
      val actual = { live: true, bytes: residentBytes(upLen, selectedSm(devices)), elems: elems(upBlocks), len: upLen, releaseFailed: false, tracked: true }
      val entry = { addr: addr, len: upLen, blocks: upBlocks, handle: h, bytes: if (REGISTRY) bytes else 0 }
      if (ev.err or ev.oob) all {
        lastOp' = { ...op0, outcome: GpuErr, workers: workers, staleRejected: stale, evictions: ev.released.length(), error: "release" },
        entries' = ev.entries, native' = nat2, failed' = true, nextHandle' = nextHandle,
        budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
        memory' = memory, label' = label, lastPar' = lastPar,
      } else if (f == PrepareCtorErr or not(anyEligible(devices))) all {
        lastOp' = { ...op0, outcome: GpuErr, workers: workers, staleRejected: stale, evictions: ev.released.length(), error: "prepare" },
        entries' = ev.entries, native' = nat2, failed' = true, nextHandle' = nextHandle,
        budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
        memory' = memory, label' = label, lastPar' = lastPar,
      } else if (f == PrepareSyncErr) all {
        // msm.cu:31-35: constructor allocated, sync failed; Rust drops the handle.
        lastOp' = { ...op0, outcome: GpuErr, workers: workers, staleRejected: stale, evictions: ev.released.length(), error: "prepare" },
        entries' = ev.entries, native' = nat2.put(h, { ...actual, tracked: false }), failed' = true, nextHandle' = h + 1,
        budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
        memory' = memory, label' = label, lastPar' = lastPar,
      } else all {
        lastOp' = { ...op0, outcome: if (f == InvokeErr) GpuErr else GpuOk, hit: false, staleRejected: stale,
                    result: if (f == InvokeErr) [] else actual.elems.slice(0, cb), workers: workers,
                    evictions: ev.released.length(), handle: h, error: if (f == InvokeErr) "invoke" else "" },
        entries' = ev.entries.append(entry), native' = nat2.put(h, actual), nextHandle' = h + 1,
        failed' = (f == InvokeErr or failed),
        budget' = budget, devices' = devices, nativeAfterFail' = afterFail,
        memory' = memory, label' = label, lastPar' = lastPar,
      }
    }
  }

  action msmCall(addr: int, nb: int, trim: int): bool = msmBody(addr, nb, trim, NoFault)
  action msmFault(addr: int, nb: int, trim: int, f: Fault, coin: int): bool = all { coin == 0, msmBody(addr, nb, trim, f) }
  action parProbe(plen: int, outs: List[WOut], sf: int, coin: int): bool = all { coin == 1, parJoin(plen, outs, sf) }

  pure val ALL_FAULTS = Set(WorkerErr, WorkerPanic, SpawnErr, PrepareCtorErr, PrepareSyncErr, ReleaseErr, InvokeErr)
  // Faults the CPU-surrogate replay can inject into the real code.
  pure val MBT_FAULTS = Set(PrepareCtorErr, PrepareSyncErr, ReleaseErr, InvokeErr)

  action stepWith(faults: Set[Fault], spawnFaults: bool, par: bool): bool = {
    nondet addr = ADDRS.oneOf()
    nondet nb = 0.to(MAXB).oneOf()
    nondet trim = 0.to(1).oneOf()
    nondet basis = Set(Coef, Lag).oneOf()
    nondet len = 1.to(MAXB).oneOf()
    nondet i = 0.to(MAXB - 1).oneOf()
    nondet p = POINTS.oneOf()
    nondet f = faults.oneOf()
    nondet coin = (if (par) 0.to(5) else Set(0, 2, 3, 4, 5)).oneOf()
    nondet plen = Set(4095, 4096, 4097, 4099, 5120, 5121, 16385).oneOf()
    nondet o0 = Set(WOk, WErr, WPanic).oneOf()
    nondet o1 = Set(WOk, WErr, WPanic).oneOf()
    nondet o2 = Set(WOk, WErr, WPanic).oneOf()
    nondet o3 = Set(WOk, WErr, WPanic).oneOf()
    nondet o4 = Set(WOk, WErr, WPanic).oneOf()
    nondet sf = (if (spawnFaults) Set(-1, 0, 1, 2, 3) else Set(-1)).oneOf()
    val outs = [o0, o1, o2, o3, o4].slice(0, min(5, workerCount(plen, MUT)))
    any {
      writeArray(addr, basis, len),
      mutate(addr, i, p),
      msmCall(addr, nb, trim),
      msmFault(addr, nb, trim, f, coin),
      parProbe(plen, outs, sf, coin),
    }
  }
  action step = stepWith(ALL_FAULTS, true, true)
  action stepMbt = stepWith(MBT_FAULTS, false, true)
  // parallel_chunks exists only with the registry feature.
  action stepMbtLegacy = stepWith(MBT_FAULTS, false, false)
  // GPU replay: no injected faults (no fake devices, no fake errors).
  action stepGpu = {
    nondet addr = ADDRS.oneOf()
    nondet nb = 0.to(MAXB).oneOf()
    nondet trim = 0.to(1).oneOf()
    nondet basis = Set(Coef, Lag).oneOf()
    nondet len = 1.to(MAXB).oneOf()
    nondet i = 0.to(MAXB - 1).oneOf()
    nondet p = POINTS.filter(q => valid(q)).oneOf()
    any { writeArray(addr, basis, len), mutate(addr, i, p), msmCall(addr, nb, trim) }
  }

  // ---- safety invariants -------------------------------------------------
  // Every GPU result uses exactly the requested bases' group elements.
  val hitExact = lastOp.outcome == GpuOk implies lastOp.result == lastOp.expected
  // Every resident handle referenced by the cache holds validated points.
  val residentValid = entries.foldl(true, (acc, e) => acc and native.get(e.handle).elems.foldl(true, (a, x) => a and x != OFF_CURVE and x != TORSION))
  // No cache entry refers to a released handle.
  val noDangling = entries.foldl(true, (acc, e) => acc and native.keys().contains(e.handle) and native.get(e.handle).live)
  // Snapshot and native upload agree (the snapshot is what was uploaded).
  val snapshotMatchesUpload = entries.foldl(true, (acc, e) => acc and elems(e.blocks) == native.get(e.handle).elems and e.len == native.get(e.handle).len)
  val onePerAddress = not(REGISTRY) or ADDRS.forall(a => entries.foldl(0, (c, e) => if (e.addr == a) c + 1 else c) <= 1)
  val legacyTwoSlots = REGISTRY or entries.length() <= 2
  // Persistent bytes never exceed the budget while the backend is healthy.
  val budgetRespected = not(REGISTRY) or failed or used(entries) <= budget
  // The charge never underestimates what sppark allocated on its selected device.
  val chargeConservative = not(REGISTRY) or entries.foldl(true, (acc, e) => acc and e.bytes >= native.get(e.handle).bytes)
  // Registry: a handle whose release failed stays reachable (quarantined).
  val releaseQuarantined = native.keys().forall(h => native.get(h).releaseFailed implies entries.foldl(false, (acc, e) => acc or e.handle == h))
  // While healthy, live native allocations are exactly the cached handles.
  val noUntrackedWhileHealthy = failed or native.keys().forall(h => native.get(h).live iff entries.foldl(false, (acc, e) => acc or e.handle == h))
  // Stronger, not part of safety: also after a failure (REPORT.md F4).
  val noUntrackedLive = native.keys().forall(h => native.get(h).live implies entries.foldl(false, (acc, e) => acc or e.handle == h))
  val noNativeAfterFail = nativeAfterFail == 0
  val workersBounded = lastOp.workers <= WORKERS and lastPar.spawned <= WORKERS
  val workersDrained = not(lastOp.unwound) and not(lastPar.unwound) and lastPar.joined == lastPar.spawned

  val safetyCore = hitExact and residentValid and noDangling and snapshotMatchesUpload and onePerAddress
    and legacyTwoSlots and budgetRespected and chargeConservative and noUntrackedWhileHealthy
    and noNativeAfterFail and workersBounded and workersDrained
  // releaseQuarantined holds for the registry only; legacy violates it (REPORT.md F2).
  val safety = safetyCore and (not(REGISTRY) or releaseQuarantined)

  // Exact accounting: holds on homogeneous hosts, over-approximates on mixed ones.
  val chargeExact = not(REGISTRY) or entries.foldl(true, (acc, e) => acc and e.bytes == native.get(e.handle).bytes)
  // W13's claim: exact when every visible device is the selected one's model.
  val homogeneous = devices.foldl(true, (acc, d) => acc and d.sm == devices.nth(0).sm and d.eligible)
  val chargeExactHomogeneous = homogeneous implies chargeExact

  // ---- reachability witnesses (positive goals; must be reached) ----------
  val wHit = lastOp.outcome == GpuOk and lastOp.hit
  val wPrefixHit = lastOp.outcome == GpuOk and lastOp.hit and entries.foldl(false, (acc, e) => acc or (e.handle == lastOp.handle and lastOp.n < e.len))
  val wNminus1Hit = lastOp.outcome == GpuOk and lastOp.hit and lastOp.n % BLOCK == BLOCK - 1
  val wStaleRejected = lastOp.outcome == GpuOk and not(lastOp.hit) and lastOp.staleRejected
  val wEvict = lastOp.evictions > 0
  val wTwoResident = entries.length() >= 2
  val wBudgetReject = lastOp.error == "budget"
  val wInvalidReject = lastOp.error == "invalid"
  val wQueryReject = lastOp.error == "query"
  val wReleaseFail = lastOp.error == "release"
  val wWorkers4 = lastOp.workers == 4
  val wParPanic = lastPar.err >= 0 and lastPar.spawned == 4
  val wDisabled = lastOp.outcome == Disabled
  val wMixedOvercharge = not(chargeExact)
  val wCoefLagResident = entries.length() >= 2 and entries.nth(0).blocks.nth(0) != entries.nth(1).blocks.nth(0)
}

module w13_registry {
  import srs_w13(REGISTRY = true, MUT = 0).* from "./srs_w13"
}
module w13_legacy {
  import srs_w13(REGISTRY = false, MUT = 0).* from "./srs_w13"
}
