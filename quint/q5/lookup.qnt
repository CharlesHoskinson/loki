// -*- mode: Bluespec; -*-
// q5: lookup and permutation argument preparation, stock 0.8.3 vs Loki a256a9a.
//
// Each step computes the stock algorithm and the Loki algorithm side by side on
// the same nondeterministic inputs and records both. Values are abstract ids
// 0..5 whose order is the field `Ord` order (the Rust replay ranks real Fq
// values by `Fq::cmp` and maps rank -> id). Arithmetic for products is in
// GF(17), which has the 4th and 8th roots of unity (omega = 4 or 2) and the
// generator delta = 3, so the halo2 permutation identities hold exactly.
//
// Source correspondence (main a256a9a unless marked stock; verified with `git show a256a9a:<file> | grep -n`):
//   permuteStock          stock src/plonk/lookup/prover.rs:379-461 (permute_expression_pair; sort 396, leftovers 430)
//   permuteLoki           src/plonk/lookup/prover.rs:374-466 (par_sort 391; leftover traversal 424-439:
//                         default `leftover_table_map.iter()` 425-426, `loki-deterministic-lookup-order` sort 429-434;
//                         blinding draws input 442 then table 443)
//   lookupProductStock    stock src/plonk/lookup/prover.rs:155-295 (commit_product; scan/take/blinding 229-240)
//   lookupProductLoki     src/plonk/lookup/prover.rs:155-290 (usable rows 182-184, batch_invert_parallel 198,
//                         prefix_product + push + bf draws 231-235)
//   batchInvertChunked    src/utils/arithmetic.rs:172-180 (batch_invert_parallel; partition of parallelize 120-169;
//                         ff::BatchInvert per chunk)
//   prefixChunked         src/utils/arithmetic.rs:184-221 (prefix_product)
//   permZStock            stock src/plonk/permutation/prover.rs:36-~170 (Argument::commit; blinding 148-153)
//   permZLoki             src/plonk/permutation/prover.rs:35-158 (usable rows 84-86, batch_invert_parallel 107,
//                         prefix_product/push/bf draws 142-145)
//   rationalStock         stock src/poly/mod.rs:287-328 (batch_invert_rational 287-310 + Polynomial::invert 312-328)
//   rationalLoki          src/poly/mod.rs:290-317 (compacts denominators != ONE)
//   drawLog / ctr         every `F::random(&mut *rng)` in the two provers, in program order
//
// MUT selects a mutation of the Loki side (0 = faithful). See q5_mutations below.
module lookup {
  const MUT: int
  // Turn the chunked paths on regardless of size (the Rust code takes them only
  // for len >= 1024; chunk-partition independence is the property modelled).
  pure val P = 17
  pure val VALS = 0.to(5)
  pure val MISSING = 6
  pure val JUNK = 9
  pure val TAG = 1000

  // ---------------- arithmetic in GF(P) ----------------
  pure def md(x: int): int = ((x % P) + P) % P
  pure def mul(a: int, b: int): int = md(a * b)
  pure def add(a: int, b: int): int = md(a + b)
  pure def pw(a: int, e: int): int = range(0, e).foldl(1, (acc, _i) => mul(acc, a))
  pure def inv0(a: int): int = if (md(a) == 0) 0 else pw(a, P - 2)

  // ---------------- list helpers ----------------
  pure def insertSorted(l: List[int], x: int): List[int] =
    l.select(y => y <= x).append(x).concat(l.select(y => y > x))
  pure def sortL(l: List[int]): List[int] = l.foldl([], (acc, x) => insertSorted(acc, x))
  pure def rev(l: List[int]): List[int] = l.foldl([], (acc, x) => [x].concat(acc))
  pure def count(l: List[int], x: int): int = l.foldl(0, (c, y) => if (y == x) c + 1 else c)
  pure def has(l: List[int], x: int): bool = count(l, x) > 0
  pure def isPermOf(a: List[int], b: List[int]): bool =
    a.length() == b.length() and a.foldl(true, (ok, x) => ok and count(a, x) == count(b, x))
  pure def dedup(l: List[int]): List[int] = l.foldl([], (acc, x) => if (has(acc, x)) acc else acc.append(x))
  pure def rep(x: int, n: int): List[int] = range(0, n).foldl([], (acc, _i) => acc.append(x))
  pure def indexOf(l: List[int], x: int): int =
    range(0, l.length()).foldl(-1, (r, i) => if (r < 0 and l[i] == x) i else r)
  pure def draws(ctr: int, k: int): List[int] = range(0, k).foldl([], (acc, j) => acc.append(TAG + ctr + j))
  pure def isSortedL(l: List[int]): bool = range(1, l.length()).foldl(true, (ok, i) => ok and l[i - 1] <= l[i])

  // ---------------- permute_expression_pair ----------------
  type Permuted = { ok: bool, a: List[int], s: List[int], ctr: int }

  // `order` is the HashMap iteration order over the distinct table keys of the usable rows.
  pure def permuteWith(input: List[int], table: List[int], u: int, bf: int, order: List[int], ctr: int,
                       sortDesc: bool, leftRev: bool, tableFirst: bool, overwriteFirst: bool, ignoreMissing: bool): Permuted = {
    val a0 = if (sortDesc) rev(sortL(input.slice(0, u))) else sortL(input.slice(0, u))
    val tbl = table.slice(0, u)
    pure def isFirst(r: int): bool = r == 0 or a0[r] != a0[r - 1]
    val firsts = range(0, u).select(r => isFirst(r))
    val repeated = range(0, u).select(r => not(isFirst(r)))
    // stock: `leftover_table_map.get_mut(input_value)` must exist, else ConstraintSystemFailure
    val ok = firsts.foldl(true, (acc, r) => acc and has(tbl, a0[r]))
    val ord = if (leftRev) rev(order) else order
    val left = ord.foldl([], (acc, key) =>
      acc.concat(rep(key, count(tbl, key) - (if (has(a0, key)) 1 else 0))))
    // `repeated_input_rows.pop()`: the j-th leftover goes to the j-th repeated row from the end
    val s0 = range(0, u).foldl([], (acc, r) =>
      acc.append(
        if (isFirst(r) and not(overwriteFirst and r == 0 and repeated.length() > 0)) a0[r]
        else if (isFirst(r)) left[repeated.length() - 1]
        else left[repeated.length() - 1 - indexOf(repeated, r)]))
    val aBl = if (tableFirst) draws(ctr + bf + 1, bf + 1) else draws(ctr, bf + 1)
    val sBl = if (tableFirst) draws(ctr, bf + 1) else draws(ctr + bf + 1, bf + 1)
    if (not(ok) and not(ignoreMissing)) { ok: false, a: [], s: [], ctr: ctr }
    else { ok: true, a: a0.concat(aBl), s: s0.concat(sBl), ctr: ctr + 2 * (bf + 1) }
  }
  pure def permuteStock(input: List[int], table: List[int], u: int, bf: int, order: List[int], ctr: int): Permuted =
    permuteWith(input, table, u, bf, order, ctr, false, false, false, false, false)
  // det = the opt-in `loki-deterministic-lookup-order` traversal (field Ord sort of the keys)
  pure def permuteLoki(input: List[int], table: List[int], u: int, bf: int, order: List[int], ctr: int, det: bool): Permuted = {
    val ord = if (det) sortL(order) else order
    permuteWith(input, table, u, bf, ord, ctr, MUT == 9, MUT == 1, MUT == 2, MUT == 6, MUT == 10)
  }

  // ---------------- chunked batch inversion (Montgomery trick per chunk) ----------------
  // parallelize's partition: `cut` chunks of size base+1, then chunks of size base.
  pure def chunkStarts(len: int, t: int): List[int] = {
    val base = len / t
    val cut = len % t
    range(0, t).foldl([], (acc, c) => {
      val st = if (c < cut) c * (base + 1) else cut * (base + 1) + (c - cut) * base
      val sz = if (c < cut) base + 1 else base
      if (sz > 0) acc.append(st) else acc
    })
  }
  pure def chunkEnd(starts: List[int], idx: int, len: int): int =
    if (idx + 1 < starts.length()) starts[idx + 1] else len
  // ff::BatchInvert on one slice: skips zeros in the accumulator (zero stays zero).
  pure def montgomery(xs: List[int], skipZero: bool): List[int] = {
    val prefix = xs.foldl({ acc: 1, pre: [] }, (st, x) => {
      acc: if (md(x) == 0 and skipZero) st.acc else mul(st.acc, x), pre: st.pre.append(st.acc) })
    val allInv = inv0(prefix.acc)
    val back = range(0, xs.length()).foldl({ acc: allInv, out: xs }, (st, j) => {
      val i = xs.length() - 1 - j
      val x = xs[i]
      val skip = md(x) == 0 and skipZero
      { acc: if (skip) st.acc else mul(st.acc, x),
        out: st.out.replaceAt(i, if (skip) x else mul(prefix.pre[i], st.acc)) }
    })
    back.out
  }
  pure def batchInvertChunked(xs: List[int], t: int, skipZero: bool): List[int] = {
    val starts = chunkStarts(xs.length(), t)
    range(0, starts.length()).foldl([], (acc, c) =>
      acc.concat(montgomery(xs.slice(starts[c], chunkEnd(starts, c, xs.length())), skipZero)))
  }
  pure def batchInvertSeq(xs: List[int]): List[int] = montgomery(xs, true)

  // ---------------- prefix products ----------------
  type Prefix = { vals: List[int], total: int }
  pure def scanExcl(xs: List[int], init: int, inclusive: bool): Prefix = {
    val r = xs.foldl({ t: init, out: [] }, (st, x) =>
      { t: mul(st.t, x), out: st.out.append(if (inclusive) mul(st.t, x) else st.t) })
    { vals: r.out, total: r.t }
  }
  // prefix_product: chunk-local exclusive scans, scan of chunk totals, then offset scaling.
  pure def prefixChunked(xs: List[int], init: int, t: int): Prefix = {
    val len = xs.length()
    val cs = if (t <= 0) len else (len + t - 1) / t
    val csz = if (cs < 1) 1 else cs
    val nchunks = if (len == 0) 0 else (len + csz - 1) / csz
    val locals = range(0, nchunks).foldl([], (acc, c) => {
      val e = if ((c + 1) * csz < len) (c + 1) * csz else len
      acc.append(scanExcl(xs.slice(c * csz, e), 1, MUT == 7))
    })
    val offs = locals.foldl({ t: init, out: [] }, (st, l) => { t: mul(st.t, l.total), out: st.out.append(st.t) })
    val vals = range(0, nchunks).foldl([], (acc, c) =>
      acc.concat(locals[c].vals.foldl([], (a2, v) => a2.append(if (MUT == 3) v else mul(v, offs.out[c])))))
    { vals: vals, total: offs.t }
  }

  // ---------------- lookup grand product ----------------
  // ci, cs: compressed input/table (length n, junk in blinding rows); a, s: permuted (length n).
  pure def lookupProductStock(ci: List[int], cs: List[int], a: List[int], s: List[int],
                              u: int, bf: int, beta: int, gamma: int, ctr: int): List[int] = {
    val n = u + bf + 1
    val den = range(0, n).foldl([], (acc, i) => acc.append(mul(add(beta, a[i]), add(gamma, s[i]))))
    val inv = batchInvertSeq(den)
    val lp = range(0, n).foldl([], (acc, i) => acc.append(mul(mul(inv[i], add(ci[i], beta)), add(cs[i], gamma))))
    // iter::once(1).chain(lp).scan(..).take(n - bf)
    val z = range(0, n - bf).foldl({ t: 1, out: [] }, (st, i) =>
      { t: if (i < n) mul(st.t, lp[i]) else st.t, out: st.out.append(st.t) }).out
    z.concat(draws(ctr, bf))
  }
  pure def lookupProductLoki(ci: List[int], cs: List[int], a: List[int], s: List[int],
                             u: int, bf: int, beta: int, gamma: int, ctr: int, t: int): List[int] = {
    val den = range(0, u).foldl([], (acc, i) => acc.append(mul(add(beta, a[i]), add(gamma, s[i]))))
    val inv = batchInvertChunked(den, t, MUT != 4)
    val lp = range(0, u).foldl([], (acc, i) => acc.append(mul(mul(inv[i], add(ci[i], beta)), add(cs[i], gamma))))
    val pre = prefixChunked(lp, 1, t)
    if (MUT == 8) pre.vals.concat(draws(ctr, bf + 1)) else pre.vals.append(pre.total).concat(draws(ctr, bf))
  }

  // ---------------- permutation grand product ----------------
  // cols: number of columns; vals[c*n + r]; sig[c*n + r] = target cell (c'*n + r').
  pure def omegaFor(n: int): int = if (n == 4) 4 else 2
  pure val DELTA = 3
  pure def idVal(cell: int, n: int): int = mul(pw(DELTA, cell / n), pw(omegaFor(n), cell % n))
  type PermOut = { sets: List[List[int]], ctr: int }
  pure def permZ(loki: bool, cols: int, chunk: int, n: int, u: int, bf: int, vals: List[int], sig: List[int],
                 beta: int, gamma: int, ctr: int, t: int): PermOut = {
    val nsets = (cols + chunk - 1) / chunk
    val len = if (loki) u else n
    val r = range(0, nsets).foldl({ last: 1, ctr: ctr, sets: [] }, (st, k) => {
      val cset = range(k * chunk, if ((k + 1) * chunk < cols) (k + 1) * chunk else cols)
      val den = range(0, len).foldl([], (acc, i) => acc.append(
        cset.foldl(1, (m, c) => mul(m, add(add(vals[c * n + i], mul(beta, idVal(sig[c * n + i], n))), gamma)))))
      val inv = if (loki) batchInvertChunked(den, t, MUT != 4) else batchInvertSeq(den)
      val m = range(0, len).foldl([], (acc, i) => acc.append(
        cset.foldl(inv[i], (mm, c) => {
          // MUT 11: delta power restarts in every set (deltaomega not carried across chunks)
          val cc = if (loki and MUT == 11) c - k * chunk else c
          mul(mm, add(add(vals[c * n + i], mul(idVal(cc * n + i, n), beta)), gamma)) })))
      if (loki) {
        val pre = prefixChunked(m, st.last, t)
        val z = pre.vals.append(pre.total).concat(draws(st.ctr, bf))
        { last: pre.total, ctr: st.ctr + bf, sets: st.sets.append(z) }
      } else {
        // z[0] = last; z[r] = z[r-1] * m[r-1] for r < n; blinding rows overwritten; last = z[n-bf-1]
        val z0 = range(1, n).foldl([st.last], (zz, row) => zz.append(mul(zz[row - 1], m[row - 1])))
        val z = z0.slice(0, n - bf).concat(draws(st.ctr, bf))
        { last: z[n - bf - 1], ctr: st.ctr + bf, sets: st.sets.append(z) }
      }
    })
    { sets: r.sets, ctr: r.ctr }
  }

  // ---------------- batch_invert_rational ----------------
  // kind 0 = Zero, 1 = Trivial(num), 2 = Rational(num, den)
  type Rat = { kind: int, num: int, den: int }
  pure def ratEval(x: Rat): int =
    if (x.kind == 0) 0 else if (x.kind == 1) md(x.num)
    else if (md(x.den) == 1) md(x.num) else mul(x.num, inv0(x.den))
  pure def rationalStock(xs: List[Rat]): List[int] = {
    val dens = xs.select(x => x.kind == 2).foldl([], (acc, x) => acc.append(x.den))
    val inv = batchInvertSeq(dens)
    xs.foldl({ j: 0, out: [] }, (st, x) =>
      if (x.kind == 2) { j: st.j + 1, out: st.out.append(mul(x.num, inv[st.j])) }
      else { j: st.j, out: st.out.append(if (x.kind == 0) 0 else md(x.num)) }).out
  }
  pure def rationalLoki(xs: List[Rat], t: int): List[int] = {
    pure def needs(x: Rat): bool = x.kind == 2 and md(x.den) != 1
    val dens = xs.select(x => needs(x)).foldl([], (acc, x) => acc.append(x.den))
    val inv = batchInvertChunked(dens, t, true)
    // MUT 5: consumer also pulls an inverse for den == 1 (iterator desynchronised)
    pure def consumes(x: Rat): bool = if (MUT == 5) x.kind == 2 else needs(x)
    xs.foldl({ j: 0, out: [] }, (st, x) =>
      if (consumes(x)) { j: st.j + 1, out: st.out.append(mul(x.num, if (st.j < inv.length()) inv[st.j] else 0)) }
      else { j: st.j, out: st.out.append(if (x.kind == 0) 0 else md(x.num)) }).out
  }

  // ================= state =================
  // `obs` is the part the Rust driver reproduces from the real code (MBT).
  type Obs = {
    phase: str,
    ok: bool,           // permute_expression_pair returned Ok
    aPrime: List[int],  // Loki A' (ids; TAG+j = j-th RNG draw)
    sPrime: List[int],  // Loki S'
    zBlind: List[int],  // Loki lookup z blinding rows (draw tags)
    telescopes: bool,   // Loki lookup z[u] == 1
    lookupEqual: bool,  // Loki A', S', z equal stock under the same HashMap order
    permLast: str,      // "one" | "zero" | "other": final permutation last_z class
    permEqual: bool,
    ratEqual: bool,
  }
  var obs: Obs
  // inputs and internal state
  var u: int
  var bf: int
  var threads: int
  var det: bool
  var input: List[int]
  var table: List[int]
  var order: List[int]
  var ctrStock: int
  var ctrLoki: int
  var pStock: Permuted
  var pLoki: Permuted
  var zStock: List[int]
  var zLoki: List[int]
  var beta: int
  var gamma: int
  var zeroDen: bool
  var permValid: bool
  var permHit: bool
  var permStock: PermOut
  var permLoki: PermOut
  var pn: int
  var permOne: bool
  var permNoZero: bool
  var rats: List[Rat]
  var ratStock: List[int]
  var ratLoki: List[int]

  pure val obs0: Obs = { phase: "init", ok: false, aPrime: [], sPrime: [], zBlind: [], telescopes: false,
                         lookupEqual: true, permLast: "none", permEqual: true, ratEqual: true }

  pure def listOf(m: int -> int, len: int): List[int] = range(0, len).foldl([], (acc, i) => acc.append(m.get(i)))

  action init = {
    nondet u0 = 2.to(5).oneOf()
    nondet bf0 = 1.to(2).oneOf()
    nondet t0 = 1.to(3).oneOf()
    nondet det0 = Bool.oneOf()
    nondet tblM = setOfMaps(0.to(4), VALS).oneOf()
    nondet inM = setOfMaps(0.to(4), 0.to(4)).oneOf()
    nondet missing = Bool.oneOf()
    nondet prio = setOfMaps(VALS, 0.to(5)).oneOf()
    val tbl = listOf(tblM, u0)
    // inputs drawn from table rows (so present), optionally one value absent from the table
    val inp0 = range(0, u0).foldl([], (acc, i) => acc.append(tbl[inM.get(i) % u0]))
    val inp = if (missing) inp0.replaceAt(0, MISSING) else inp0
    val n = u0 + bf0 + 1
    // keys of the HashMap in iteration order: sort distinct keys by (prio, key)
    val tkeys = sortL(dedup(tbl))
    val ord = tkeys
    val ordered = range(0, 6).foldl([], (acc, p) => acc.concat(ord.select(k => prio.get(k) == p)))
    all {
      u' = u0, bf' = bf0, threads' = t0, det' = det0,
      input' = inp.concat(rep(JUNK, n - u0)),
      table' = tbl.concat(rep(JUNK, n - u0)),
      order' = ordered,
      ctrStock' = 0, ctrLoki' = 0,
      pStock' = { ok: false, a: [], s: [], ctr: 0 }, pLoki' = { ok: false, a: [], s: [], ctr: 0 },
      zStock' = [], zLoki' = [], beta' = 0, gamma' = 0, zeroDen' = false,
      permValid' = false, permHit' = false, permStock' = { sets: [], ctr: 0 }, permLoki' = { sets: [], ctr: 0 },
      pn' = 4, permOne' = false, permNoZero' = true,
      rats' = [], ratStock' = [], ratLoki' = [],
      obs' = obs0,
    }
  }

  action permute = all {
    obs.phase == "init",
    val ps = permuteStock(input, table, u, bf, order, ctrStock)
    // Default build compares the default traversal; det compares the opt-in sort with stock
    // under the sorted HashMap order (the only order the sort can equal).
    val pl = permuteLoki(input, table, u, bf, order, ctrLoki, det)
    val ref = if (det) permuteStock(input, table, u, bf, sortL(order), ctrStock) else ps
    all {
      pStock' = ps, pLoki' = pl, ctrStock' = ps.ctr, ctrLoki' = pl.ctr,
      obs' = { ...obs, phase: if (pl.ok) "permuted" else "error", ok: pl.ok, aPrime: pl.a, sPrime: pl.s,
               lookupEqual: pl == ref },
      u' = u, bf' = bf, threads' = threads, det' = det, input' = input, table' = table, order' = order,
      zStock' = zStock, zLoki' = zLoki, beta' = beta, gamma' = gamma, zeroDen' = zeroDen,
      permValid' = permValid, permHit' = permHit, permStock' = permStock, permLoki' = permLoki,
      pn' = pn, permOne' = permOne, permNoZero' = permNoZero, rats' = rats, ratStock' = ratStock, ratLoki' = ratLoki,
    }
  }

  // betaHit/gammaHit >= 0: choose beta = -a'[i] (gamma = -s'[i]) to force a zero denominator.
  // Generic choices avoid every value id, so zero denominators happen exactly on hits.
  action product = all {
    obs.phase == "permuted",
    nondet betaHit = (-1).to(u - 1).oneOf()
    nondet gammaHit = (-1).to(u - 1).oneOf()
    nondet bGen = 1.to(10).oneOf()
    nondet gGen = 1.to(10).oneOf()
    val ref = if (det) permuteStock(input, table, u, bf, sortL(order), 0) else pStock
    val b = if (betaHit >= 0) md(0 - pLoki.a[betaHit]) else bGen
    val g = if (gammaHit >= 0) md(0 - pLoki.s[gammaHit]) else gGen
    val zs = lookupProductStock(input, table, ref.a, ref.s, u, bf, b, g, ctrStock)
    val zl = lookupProductLoki(input, table, pLoki.a, pLoki.s, u, bf, b, g, ctrLoki, threads)
    val zd = 0.to(u - 1).exists(i => mul(add(b, pLoki.a[i]), add(g, pLoki.s[i])) == 0)
    all {
      beta' = b, gamma' = g, zStock' = zs, zLoki' = zl, zeroDen' = zd,
      ctrStock' = ctrStock + bf, ctrLoki' = ctrLoki + bf,
      obs' = { ...obs, phase: "product", zBlind: zl.slice(u + 1, zl.length()),
               telescopes: zl.length() > u and zl[u] == 1, lookupEqual: obs.lookupEqual and zl == zs },
      u' = u, bf' = bf, threads' = threads, det' = det, input' = input, table' = table, order' = order,
      pStock' = pStock, pLoki' = pLoki,
      permValid' = permValid, permHit' = permHit, permStock' = permStock, permLoki' = permLoki,
      pn' = pn, permOne' = permOne, permNoZero' = permNoZero, rats' = rats, ratStock' = ratStock, ratLoki' = ratLoki,
    }
  }

  // Permutation argument on a fresh small circuit: n = 4 (up to 4 columns) or 8 (up to 2).
  action permArg = all {
    obs.phase == "product" or obs.phase == "error",
    nondet n8 = Bool.oneOf()
    nondet colsSel = 1.to(4).oneOf()
    nondet chunk = 1.to(2).oneOf()
    nondet pbf = 1.to(2).oneOf()
    nondet valM = setOfMaps(0.to(31), 0.to(3)).oneOf()
    nondet cyc = setOfMaps(0.to(31), 0.to(1)).oneOf()
    nondet valid = Bool.oneOf()
    nondet hit = Bool.oneOf()
    nondet b0 = 1.to(16).oneOf()
    nondet g0 = 0.to(16).oneOf()
    val n = if (n8) 8 else 4
    val cols = if (n8 and colsSel > 2) 2 else colsSel
    val pu = n - pbf - 1
    val cells = range(0, cols * n).select(c => c % n < pu)
    // sigma: one cycle through the selected usable cells (in index order), identity elsewhere
    val cycle = cells.select(c => cyc.get(c) == 1)
    val sig = range(0, cols * n).foldl([], (acc, c) => {
      val k = indexOf(cycle, c)
      acc.append(if (k < 0) c else cycle[(k + 1) % cycle.length()])
    })
    // values constant on the cycle (valid copy constraints); invalid flips the first cycle cell
    val cv = if (cycle.length() > 0) valM.get(cycle[0]) else 0
    val vals0 = range(0, cols * n).foldl([], (acc, c) => acc.append(if (has(cycle, c)) cv else valM.get(c)))
    val broken = not(valid) and cycle.length() > 1
    val vals = if (broken) vals0.replaceAt(cycle[0], add(cv, 1)) else vals0
    // hit: gamma chosen to zero the denominator at the first usable cell
    val c0 = cells[0]
    val g = if (hit) md(0 - add(vals[c0], mul(b0, idVal(sig[c0], n)))) else g0
    val ps = permZ(false, cols, chunk, n, pu, pbf, vals, sig, b0, g, ctrStock, threads)
    val pl = permZ(true, cols, chunk, n, pu, pbf, vals, sig, b0, g, ctrLoki, threads)
    val lastSet = pl.sets[pl.sets.length() - 1]
    val last = lastSet[pu]
    val noZero = cells.foldl(true, (ok, c) => ok and
      add(add(vals[c], mul(b0, idVal(sig[c], n))), g) != 0 and add(add(vals[c], mul(b0, idVal(c, n))), g) != 0)
    all {
      permValid' = not(broken), permHit' = hit, permStock' = ps, permLoki' = pl, pn' = n,
      permOne' = last == 1, permNoZero' = noZero,
      ctrStock' = ps.ctr, ctrLoki' = pl.ctr,
      obs' = { ...obs, phase: "perm", permEqual: ps == pl,
               permLast: if (hit) "zero" else if (not(broken)) "one" else "other" },
      u' = u, bf' = bf, threads' = threads, det' = det, input' = input, table' = table, order' = order,
      pStock' = pStock, pLoki' = pLoki, zStock' = zStock, zLoki' = zLoki, beta' = beta, gamma' = gamma,
      zeroDen' = zeroDen, rats' = rats, ratStock' = ratStock, ratLoki' = ratLoki,
    }
  }

  action rational = all {
    obs.phase == "perm",
    nondet kinds = setOfMaps(0.to(5), 0.to(2)).oneOf()
    nondet nums = setOfMaps(0.to(5), 0.to(16)).oneOf()
    nondet dens = setOfMaps(0.to(5), 0.to(3)).oneOf()   // 0 = ZERO, 1 = ONE, 2, 3
    nondet len = 0.to(6).oneOf()
    val xs = range(0, len).foldl([], (acc, i) => acc.append({ kind: kinds.get(i), num: nums.get(i), den: dens.get(i) }))
    val rs = rationalStock(xs)
    val rl = rationalLoki(xs, threads)
    all {
      rats' = xs, ratStock' = rs, ratLoki' = rl,
      obs' = { ...obs, phase: "done", ratEqual: rs == rl },
      u' = u, bf' = bf, threads' = threads, det' = det, input' = input, table' = table, order' = order,
      ctrStock' = ctrStock, ctrLoki' = ctrLoki, pStock' = pStock, pLoki' = pLoki, zStock' = zStock, zLoki' = zLoki,
      beta' = beta, gamma' = gamma, zeroDen' = zeroDen, permValid' = permValid, permHit' = permHit,
      permStock' = permStock, permLoki' = permLoki, pn' = pn, permOne' = permOne, permNoZero' = permNoZero,
    }
  }

  action idle = all {
    obs.phase == "done",
    obs' = obs, u' = u, bf' = bf, threads' = threads, det' = det, input' = input, table' = table, order' = order,
    ctrStock' = ctrStock, ctrLoki' = ctrLoki, pStock' = pStock, pLoki' = pLoki, zStock' = zStock, zLoki' = zLoki,
    beta' = beta, gamma' = gamma, zeroDen' = zeroDen, permValid' = permValid, permHit' = permHit,
    permStock' = permStock, permLoki' = permLoki, pn' = pn, permOne' = permOne, permNoZero' = permNoZero,
    rats' = rats, ratStock' = ratStock, ratLoki' = ratLoki,
  }
  action step = any { permute, product, permArg, rational, idle }

  // ================= safety invariants =================
  // I1: A' / S' are permutations of input / table over the usable rows, A' is sorted,
  //     and the halo2 lookup conditions hold: a'_0 = s'_0 and a'_i = s'_i or a'_i = a'_{i-1}.
  val LookupConditions = not(pLoki.ok) or {
    val a = pLoki.a.slice(0, u)
    val s = pLoki.s.slice(0, u)
    isPermOf(a, input.slice(0, u)) and isPermOf(s, table.slice(0, u)) and isSortedL(a) and a[0] == s[0]
      and range(1, u).foldl(true, (ok, i) => ok and (a[i] == s[i] or a[i] == a[i - 1]))
  }
  // I2: Ok/Err agree with stock (Err exactly when an input value is absent from the table).
  val ErrorAgrees = obs.phase == "init" or pLoki.ok == pStock.ok
  // I3: default-build leftover placement equals stock for the same HashMap order
  //     (det build: equals stock under the sorted order); likewise the lookup product.
  val LookupEqualsStock = obs.lookupEqual
  // I4: blinding rows untouched: usable rows hold data, the last bf+1 rows of A'/S' and the
  //     last bf rows of z are fresh draws, input draws strictly before table draws.
  val BlindingRowsUntouched = {
    val okP = not(pLoki.ok) or (
      pLoki.a.slice(u, u + bf + 1) == draws(0, bf + 1) and pLoki.s.slice(u, u + bf + 1) == draws(bf + 1, bf + 1)
      and pLoki.a.slice(0, u).foldl(true, (ok, x) => ok and x < TAG)
      and pLoki.s.slice(0, u).foldl(true, (ok, x) => ok and x < TAG))
    val okZ = zLoki.length() == 0 or (zLoki.length() == u + bf + 1 and zLoki.slice(u + 1, u + bf + 1) == draws(2 * (bf + 1), bf)
      and zLoki.slice(0, u + 1).foldl(true, (ok, x) => ok and x < TAG))
    okP and okZ
  }
  // I5: the lookup product starts at 1 and telescopes to 1 over the usable rows unless a
  //     denominator is zero (then the soundness check `z[u] == 1` correctly fails).
  val LookupTelescopes = zLoki.length() == 0 or (zLoki[0] == 1 and (zeroDen or zLoki[u] == 1))
  // I6: the same random draws happen in the same order as stock.
  val SameDraws = ctrLoki == ctrStock and (not(pLoki.ok) or pLoki.a.slice(u, u + bf + 1) == pStock.a.slice(u, u + bf + 1))
  // I7: permutation products equal stock set by set (incl. blinding draws and set chaining),
  //     and a valid copy-constraint permutation with no zero factor telescopes to 1.
  val PermEqualsStock = permStock == permLoki
  val PermTelescopes = obs.permLast == "none" or not(permValid and permNoZero) or permOne
  // I8: rational compaction equals stock and equals per-element evaluation.
  val RationalEqualsStock = ratStock == ratLoki and ratStock == rats.foldl([], (acc, x) => acc.append(ratEval(x)))
  // I9: the chunked inversion / prefix product equal the sequential ones (closed form, all inputs
  //     of length <= 5 over a fixed set of interesting vectors incl. zeros, ONE, max element P-1).
  pure val VECS = [[], [0], [1], [16], [0, 0], [3, 0, 5], [16, 16, 16, 16], [2, 0, 0, 9, 4], [1, 2, 3, 4, 5], [0, 16, 0, 16, 1]]
  val HelpersEqualSequential = VECS.foldl(true, (ok, v) => ok and 1.to(4).forall(t =>
    batchInvertChunked(v, t, MUT != 4) == batchInvertSeq(v) and prefixChunked(v, 3, t) == scanExcl(v, 3, false)))

  val Safety = LookupConditions and ErrorAgrees and LookupEqualsStock and BlindingRowsUntouched
    and LookupTelescopes and SameDraws and PermEqualsStock and PermTelescopes and RationalEqualsStock
    and HelpersEqualSequential

  // ================= reachability witnesses (expected to be VIOLATED) =================
  val WitnessNoError = obs.phase != "error"
  val WitnessNoZeroDen = not(zeroDen)
  val WitnessOrderInsensitive = not(pStock.ok and permuteStock(input, table, u, bf, rev(order), 0) != pStock)
  val WitnessNoMultiSet = permLoki.sets.length() <= 1
  val WitnessNoPermOne = not(permOne and permValid and obs.permLast == "one")
  val WitnessNoInvalidPerm = permValid or obs.permLast == "none"
  val WitnessNoRationalOne = not(rats.select(x => x.kind == 2 and x.den == 1).length() > 0)
  val WitnessNoRationalZeroDen = not(rats.select(x => x.kind == 2 and x.den == 0).length() > 0)
  val WitnessNotDone = obs.phase != "done"
  val WitnessNoDet = not(det and pLoki.ok)
  val WitnessNoMultiChunk = threads == 1 or u < 3
}

module lookup_ok { import lookup(MUT = 0).* }
// Mutation witnesses: each must violate the invariant named in its comment.
module mut1 { import lookup(MUT = 1).* }  // leftovers traversed in reverse HashMap order   -> LookupEqualsStock
module mut2 { import lookup(MUT = 2).* }  // table blinding drawn before input blinding    -> BlindingRowsUntouched, SameDraws
module mut3 { import lookup(MUT = 3).* }  // prefix_product forgets chunk offsets          -> HelpersEqualSequential, LookupEqualsStock
module mut4 { import lookup(MUT = 4).* }  // batch inversion does not skip zeros           -> HelpersEqualSequential, LookupEqualsStock
module mut5 { import lookup(MUT = 5).* }  // rational consumer desynchronised on den == 1  -> RationalEqualsStock
module mut6 { import lookup(MUT = 6).* }  // first-occurrence row 0 overwritten by leftover-> LookupConditions
module mut7 { import lookup(MUT = 7).* }  // inclusive instead of exclusive prefix         -> LookupTelescopes, PermEqualsStock
module mut8 { import lookup(MUT = 8).* }  // last usable z row overwritten by blinding     -> BlindingRowsUntouched, LookupTelescopes
module mut9 { import lookup(MUT = 9).* }  // input sorted descending                       -> LookupConditions
module mut10 { import lookup(MUT = 10).* } // missing input value not reported             -> ErrorAgrees
module mut11 { import lookup(MUT = 11).* } // permutation delta power restarts per set     -> PermTelescopes, PermEqualsStock
