// -*- mode: Bluespec; -*-
// quint-q4: forward model of the P3 SRS registry contract.
//
// Sources (loki-gpu eb27233): design/PLAN.md:75-92 (P3), design/MSM.md
// "resident bases" and "Rust/CUDA boundary", crates/loki-backend-api/src/lib.rs:
// 549-655 (SrsBasis, SrsCacheKey, SrsLease, SrsRegistry), crates/loki-gpu/src/
// srs.rs (stubs: every operation returns Unsupported). There is no P3
// implementation to replay; this model fixes the contract an implementation
// must refine. Abstract points are ints; bytes are one unit per point.
module srs_p3 {
  const MUT: int
  pure val M_NONE = 0
  pure val M_DIGEST_ONLY = 1     // hit on caller-supplied content_identity, no compare
  pure val M_EVICT_LEASED = 2    // eviction ignores live leases
  pure val M_IGNORE_GEN = 3      // acquire reuses an entry from an older generation
  pure val M_IGNORE_BASIS = 4    // key match ignores basis/domain (content still compared)
  pure val M_TRUST_RELEASE = 5   // release decrements refs for forged/dead lease metadata
  pure val M_RELEASE_INFLIGHT = 6 // release allowed while queued work holds the lease
  pure val M_LEAK_ON_EVICT = 7   // eviction forgets to return bytes
  pure val M_PUBLISH_STALE = 8   // completion publishes after a generation change
  pure val M_NO_VALIDATE = 9     // upload without subgroup/curve validation

  type Basis = Coef | Lag
  type Key = { basis: Basis, domainLog: int, count: int, gen: int, digest: int }
  type Entry = { basis: Basis, domainLog: int, len: int, snapshot: List[int], gen: int, bytes: int, refs: int }
  type Lease = { entry: int, key: Key, gen: int, live: bool, points: List[int] }
  type Pending = { lease: int, n: int, entry: int, gen: int, result: List[int] }
  type Op = { kind: str, ok: bool, result: List[int], expected: List[int], entryBasis: Basis, entryDomain: int,
              leaseBasis: Basis, leaseDomain: int, entryGen: int, leaseGen: int, gen: int }
  pure val NOOP: Op = { kind: "none", ok: false, result: [], expected: [], entryBasis: Coef, entryDomain: 0,
                        leaseBasis: Coef, leaseDomain: 0, entryGen: 0, leaseGen: 0, gen: 0 }

  pure val BUDGET = 7
  pure val INVALID = 0
  // Coefficient points are prefix-stable; each Lagrange domain differs. The
  // one-point Lagrange basis equals the one-point coefficient basis ([G]), so
  // the basis tag, not content, is what separates them there.
  // Literal lists (Apalache needs constant ranges): lag(d)[k] = ((k + 2^d) % 5) + 2.
  pure def coef(len: int): List[int] = [1, 2, 3, 4].slice(0, len)
  pure def lag(d: int): List[int] = if (d == 0) [1] else if (d == 1) [4, 5] else [6, 2, 3, 4]
  pure def digestOf(l: List[int]): int = l.foldl(7, (h, x) => (h * 31 + x) % 1009)
  pure def min(a: int, b: int): int = if (a < b) a else b

  var gen: int
  var entries: int -> Entry          // resident entries of the current generation
  var lru: List[int]                 // entry ids, LRU front
  var leases: int -> Lease
  var pending: int -> Pending        // queued srs_msm work, by ticket
  var nextId: int
  var used: int
  var lastOp: Op

  pure def without(l: List[int], x: int): List[int] = l.foldl([], (acc, y) => if (y == x) acc else acc.append(y))
  def liveLeasesOn(id: int): int = leases.keys().filter(l => leases.get(l).live and leases.get(l).entry == id and leases.get(l).gen == gen).size()
  def busy(id: int): bool = pending.keys().exists(t => pending.get(t).entry == id)

  // The busy() guard is what M_RELEASE_INFLIGHT drops: refcount-only protection.
  def isEvictable(id: int): bool = MUT == M_EVICT_LEASED or (entries.get(id).refs == 0 and (MUT == M_RELEASE_INFLIGHT or not(busy(id))))
  def keyMatches(e: Entry, k: Key): bool =
    (MUT == M_IGNORE_BASIS or (e.basis == k.basis and (k.basis == Coef or e.domainLog == k.domainLog)))
    and (MUT == M_IGNORE_GEN or e.gen == gen)
  def contentMatches(e: Entry, k: Key, pts: List[int]): bool =
    if (MUT == M_DIGEST_ONLY) digestOf(e.snapshot.slice(0, min(k.count, e.len))) == k.digest and k.count <= e.len
    else k.count <= e.len and e.snapshot.slice(0, k.count) == pts
      and (k.basis == Coef or e.len == k.count)   // a larger Lagrange basis is not a smaller domain

  action init = all {
    gen' = 1, entries' = Map(), lru' = [], leases' = Map(), pending' = Map(), nextId' = 0, used' = 0, lastOp' = NOOP,
  }

  // acquire_srs(SrsRequest { key, points }). `lie` makes the caller's digest wrong.
  action acquire(basis: Basis, sz: int, corrupt: bool, lie: bool): bool = {
    val pts0 = if (basis == Coef) coef(sz) else lag(sz)
    val pts = if (corrupt) pts0.replaceAt(pts0.length() - 1, INVALID) else pts0
    val key = { basis: basis, domainLog: if (basis == Lag) sz else 0, count: pts.length(), gen: gen,
                digest: if (lie) digestOf(pts) + 1 else digestOf(pts) }
    // Under the digest-only mutant a lying caller supplies the digest of a cached array.
    val key2 = if (lie and MUT == M_DIGEST_ONLY) { ...key, digest: digestOf(coef(pts.length())) } else key
    val hits = entries.keys().filter(id => keyMatches(entries.get(id), key2) and contentMatches(entries.get(id), key2, pts))
    val lease = { entry: -1, key: key2, gen: gen, live: true, points: pts }
    if (hits.size() > 0) {
      nondet id = hits.oneOf()
      all {
        entries' = entries.set(id, { ...entries.get(id), refs: entries.get(id).refs + 1 }),
        lru' = without(lru, id).append(id),
        leases' = leases.put(nextId, { ...lease, entry: id }), nextId' = nextId + 1,
        lastOp' = { ...NOOP, kind: "hit", ok: true }, gen' = gen, pending' = pending, used' = used,
      }
    } else if ((MUT != M_NO_VALIDATE and pts.foldl(false, (a, x) => a or x == INVALID)) or pts.length() > BUDGET) all {
      lastOp' = { ...NOOP, kind: "reject" },
      gen' = gen, entries' = entries, lru' = lru, leases' = leases, pending' = pending, nextId' = nextId, used' = used,
    } else {
      // Evict unleased, idle entries from the LRU front until the upload fits.
      val plan = lru.foldl({ freed: 0, out: List() }, (st, id) =>
        if (used - st.freed + pts.length() <= BUDGET or not(isEvictable(id))) st
        else { freed: st.freed + entries.get(id).bytes, out: st.out.append(id) })
      val freed = if (MUT == M_LEAK_ON_EVICT) 0 else plan.freed
      if (used - plan.freed + pts.length() > BUDGET) all {
        lastOp' = { ...NOOP, kind: "capacity" },
        gen' = gen, entries' = entries, lru' = lru, leases' = leases, pending' = pending, nextId' = nextId, used' = used,
      } else {
        val kept = entries.keys().filter(id => not(plan.out.foldl(false, (a, x) => a or x == id)))
        val newEntry = { basis: basis, domainLog: key.domainLog, len: pts.length(), snapshot: pts, gen: gen, bytes: pts.length(), refs: 1 }
        all {
          entries' = kept.mapBy(id => entries.get(id)).put(nextId, newEntry),
          lru' = plan.out.foldl(lru, (l, x) => without(l, x)).append(nextId),
          leases' = leases.put(nextId + 1, { ...lease, entry: nextId }), nextId' = nextId + 2,
          used' = used - freed + pts.length(),
          lastOp' = { ...NOOP, kind: "miss", ok: true }, gen' = gen, pending' = pending,
        }
      }
    }
  }

  // Backend validation of (possibly forged) lease metadata.
  def leaseValid(l: int, claimedGen: int): bool =
    leases.keys().contains(l) and leases.get(l).live and leases.get(l).gen == gen and claimedGen == gen
    and entries.keys().contains(leases.get(l).entry)

  // srs_msm start: queue work against the lease (blocking API; completion separate here).
  action msmStart(l: int, n: int, claimedGen: int): bool = {
    val ok = leaseValid(l, claimedGen) or (MUT == M_IGNORE_GEN and leases.keys().contains(l) and entries.keys().contains(leases.get(l).entry))
    if (ok and n >= 1 and n <= leases.get(l).points.length()) {
      val le = leases.get(l)
      val e = entries.get(le.entry)
      all {
        pending' = pending.put(nextId, { lease: l, n: n, entry: le.entry, gen: gen, result: e.snapshot.slice(0, min(n, e.len)) }),
        nextId' = nextId + 1, lastOp' = { ...NOOP, kind: "queued", ok: true },
        gen' = gen, entries' = entries, lru' = lru, leases' = leases, used' = used,
      }
    } else all {
      lastOp' = { ...NOOP, kind: "rejected" },
      gen' = gen, entries' = entries, lru' = lru, leases' = leases, pending' = pending, nextId' = nextId, used' = used,
    }
  }

  action msmComplete(t: int): bool = {
    val p = pending.get(t)
    val le = leases.get(p.lease)
    val stillResident = entries.keys().contains(p.entry) and (MUT == M_IGNORE_GEN or entries.get(p.entry).gen == p.gen)
    val publish = (p.gen == gen and stillResident) or MUT == M_PUBLISH_STALE
    val e = if (entries.keys().contains(p.entry)) entries.get(p.entry) else { basis: Coef, domainLog: -1, len: 0, snapshot: [], gen: -1, bytes: 0, refs: 0 }
    all {
      pending' = pending.keys().filter(x => x != t).mapBy(x => pending.get(x)),
      lastOp' = if (publish) { kind: "published", ok: true, result: p.result, expected: le.points.slice(0, p.n),
                    entryBasis: e.basis, entryDomain: e.domainLog, leaseBasis: le.key.basis, leaseDomain: le.key.domainLog,
                    entryGen: e.gen, leaseGen: le.gen, gen: gen }
                else { ...NOOP, kind: "lost" },
      gen' = gen, entries' = entries, lru' = lru, leases' = leases, nextId' = nextId, used' = used,
    }
  }

  // release_srs(lease). Real leases must be live, current and idle.
  action release(l: int, claimedGen: int): bool = {
    val idle = not(pending.keys().exists(t => pending.get(t).lease == l)) or MUT == M_RELEASE_INFLIGHT
    val known = leases.keys().contains(l)
    val ok = (leaseValid(l, claimedGen) and idle) or (MUT == M_TRUST_RELEASE and known and entries.keys().contains(leases.get(l).entry))
    if (ok) {
      val id = leases.get(l).entry
      all {
        entries' = entries.set(id, { ...entries.get(id), refs: entries.get(id).refs - 1 }),
        leases' = leases.set(l, { ...leases.get(l), live: false }),
        lastOp' = { ...NOOP, kind: "released", ok: true },
        gen' = gen, lru' = lru, pending' = pending, nextId' = nextId, used' = used,
      }
    } else all {
      lastOp' = { ...NOOP, kind: "rejected" },
      gen' = gen, entries' = entries, lru' = lru, leases' = leases, pending' = pending, nextId' = nextId, used' = used,
    }
  }

  // Device context loss: a new generation; resident memory is gone.
  action contextLoss = all {
    gen' = gen + 1,
    entries' = if (MUT == M_IGNORE_GEN) entries else Map(),
    lru' = if (MUT == M_IGNORE_GEN) lru else [],
    used' = if (MUT == M_IGNORE_GEN) used else 0,
    lastOp' = { ...NOOP, kind: "lost-context" },
    leases' = leases, pending' = pending, nextId' = nextId,
  }

  action step = {
    nondet basis = Set(Coef, Lag).oneOf()
    nondet sz = (if (basis == Coef) 1.to(4) else 0.to(2)).oneOf()
    nondet corrupt = Set(false, false, true).oneOf()
    nondet lie = Set(false, true).oneOf()
    nondet l = 0.to(12).oneOf()
    nondet n = 1.to(4).oneOf()
    nondet g = Set(gen, gen, gen - 1).oneOf()
    nondet coin = 0.to(7).oneOf()
    any {
      acquire(basis, sz, corrupt, lie),
      msmStart(l, n, g),
      all { pending.keys().size() > 0, nondet t = pending.keys().oneOf() msmComplete(t) },
      release(l, g),
      all { coin == 0, contextLoss },
    }
  }

  // ---- invariants --------------------------------------------------------
  // Every live current-generation lease points at a resident entry.
  val noEvictLeased = leases.keys().forall(l => (leases.get(l).live and leases.get(l).gen == gen) implies entries.keys().contains(leases.get(l).entry))
  // Queued work only ever references resident entries of its own generation.
  val pendingResident = pending.keys().forall(t => pending.get(t).gen != gen or entries.keys().contains(pending.get(t).entry))
  val refsExact = entries.keys().forall(id => entries.get(id).refs == liveLeasesOn(id))
  val bytesExact = used == entries.keys().fold(0, (a, id) => a + entries.get(id).bytes) and used <= BUDGET
  val residentValid = entries.keys().forall(id => entries.get(id).snapshot.foldl(true, (a, x) => a and x != INVALID))
  val exactContent = lastOp.kind == "published" implies lastOp.result == lastOp.expected
  val noCrossGen = lastOp.kind == "published" implies (lastOp.entryGen == lastOp.gen and lastOp.leaseGen == lastOp.gen)
  val basisMatch = lastOp.kind == "published" implies
    (lastOp.entryBasis == lastOp.leaseBasis and (lastOp.leaseBasis == Coef or lastOp.entryDomain == lastOp.leaseDomain))
  val safety = noEvictLeased and pendingResident and refsExact and bytesExact and residentValid
    and exactContent and noCrossGen and basisMatch

  // ---- witnesses ---------------------------------------------------------
  val wHit = lastOp.kind == "hit"
  val wPublished = lastOp.kind == "published"
  val wPrefixPublished = lastOp.kind == "published" and lastOp.result.length() < 4 and lastOp.leaseBasis == Coef
  val wLagPublished = lastOp.kind == "published" and lastOp.leaseBasis == Lag
  val wCapacity = lastOp.kind == "capacity"
  val wLost = lastOp.kind == "lost"
  val wReject = lastOp.kind == "reject"
  val wForgedRejected = lastOp.kind == "rejected"
  val wEvicted = nextId > 6 and entries.keys().size() >= 2
}

module p3 { import srs_p3(MUT = 0).* from "./srs_p3" }
module p3_digest_only { import srs_p3(MUT = 1).* from "./srs_p3" }
module p3_evict_leased { import srs_p3(MUT = 2).* from "./srs_p3" }
module p3_ignore_gen { import srs_p3(MUT = 3).* from "./srs_p3" }
module p3_ignore_basis { import srs_p3(MUT = 4).* from "./srs_p3" }
module p3_trust_release { import srs_p3(MUT = 5).* from "./srs_p3" }
module p3_release_inflight { import srs_p3(MUT = 6).* from "./srs_p3" }
module p3_leak_on_evict { import srs_p3(MUT = 7).* from "./srs_p3" }
module p3_publish_stale { import srs_p3(MUT = 8).* from "./srs_p3" }
module p3_no_validate { import srs_p3(MUT = 9).* from "./srs_p3" }
