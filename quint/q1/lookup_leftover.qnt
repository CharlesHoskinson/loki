// quint-q1: lookup permute_expression_pair leftover traversal (P1).
// Source: src/plonk/lookup/prover.rs:374-465 (Loki a256a9a); stock 0.8.3 lookup/prover.rs:379-460.
//   A' = stable sort of the usable input rows (:390-391, par_sort == sort for a total order)
//   first occurrence of each value in A' takes that table value (:402-418)
//   repeated rows (ascending) are filled by popping from the END (:436-437)
//   leftover keys are visited in HashMap order (default, :425-426) or ascending field order
//   (`loki-deterministic-lookup-order`, :429-434).
// The leftover order has no transcript effect (two commitments are written either way); it
// changes only which repeated rows get which leftover value. Properties are value-level.
module lookup_leftover {
  // All loops are bounded by constants so Apalache can check the model exhaustively.
  const ROWS: int
  type Case = { input: List[int], table: List[int], ranks: int -> int, det: bool, mode: str }
  type Res = { aPrime: List[int], sPrime: List[int], error: bool }
  var lk: { case: Case, res: Res, canonical: Res }

  pure val VALUES = 0.to(3)
  pure def count(l: List[int], v: int): int = l.foldl(0, (n, x) => if (x == v) n + 1 else n)
  pure def sorted(l: List[int]): List[int] =
    range(0, 4).foldl(List(), (acc, v) => range(0, ROWS).foldl(acc, (a, i) => if (i < count(l, v)) a.append(v) else a))
  pure def keysOf(l: List[int]): List[int] = range(0, 4).select(v => count(l, v) > 0)
  // Leftover traversal order: stock = arbitrary (HashMap) order modelled by free ranks; det = ascending.
  pure def order(c: Case): List[int] =
    if (c.det and c.mode != "detIgnored") keysOf(c.table)
    else range(0, 4).foldl(List(), (acc, r) => acc.concat(keysOf(c.table).select(k => c.ranks.get(k) == r)))
  pure def permute(c: Case): Res = {
    val a = sorted(c.input)
    val first = range(0, ROWS).select(r => r == 0 or a[r] != a[r - 1])
    val repeated = range(0, ROWS).select(r => not(r == 0 or a[r] != a[r - 1]))
    val counts0 = VALUES.mapBy(v => count(c.table, v))
    // first occurrences consume one table entry each; missing value => ConstraintSystemFailure
    val missing = first.foldl(false, (b, r) => b or count(c.table, a[r]) == 0)
    val counts = if (c.mode == "noFirstConsume") counts0
      else first.foldl(counts0, (m, r) => m.set(a[r], m.get(a[r]) - 1))
    val s0 = range(0, ROWS).foldl(List(), (l, r) => l.append(if (first.foldl(false, (b, x) => b or x == r)) a[r] else 0))
    val ks = order(c)
    val keys2 = if (c.mode == "skipLastKey" and ks.length() > 0) ks.slice(0, ks.length() - 1) else ks
    // pop from the end of `repeated` (mode popFront pops from the front: also valid; mode fillFirstRows
    // targets first-occurrence rows instead and must be caught)
    val targets = if (c.mode == "fillFirstRows") repeated.concat(first) else repeated
    val fill = keys2.foldl({ s: s0, stack: targets, err: false }, (st, k) =>
      range(0, ROWS).foldl(st, (st2, i) =>
        if (i >= counts.get(k)) st2
        else if (st2.stack.length() == 0) { ...st2, err: true }
        else {
          val n = st2.stack.length()
          val row = if (c.mode == "popFront") st2.stack[0] else st2.stack[n - 1]
          { s: st2.s.replaceAt(row, k), stack: if (c.mode == "popFront") st2.stack.slice(1, n) else st2.stack.slice(0, n - 1), err: st2.err }
        }))
    // assert!(repeated_input_rows.is_empty()) :438
    { aPrime: a, sPrime: fill.s, error: missing or fill.err or (fill.stack.length() > 0 and c.mode != "fillFirstRows") }
  }
  action initMode(mode: str, det: bool): bool = {
    nondet t = 0.to(ROWS - 1).setOfMaps(VALUES).oneOf()
    nondet q = 0.to(ROWS - 1).setOfMaps(0.to(ROWS - 1)).oneOf()
    // ranks 0..3 express every order of the (at most 4) keys; ties fall back to value order.
    nondet ranks = VALUES.setOfMaps(0.to(3)).oneOf()
    val table = range(0, ROWS).foldl(List(), (l, i) => l.append(t.get(i)))
    val pick = range(0, ROWS).foldl(List(), (l, i) => l.append(q.get(i)))
    // inputs drawn from table rows, so the lookup is satisfiable (a failing lookup errors in both orders)
    val input = range(0, ROWS).foldl(List(), (l, i) => l.append(table[pick[i]]))
    val c = { input: input, table: table, ranks: ranks, det: det, mode: mode }
    lk' = { case: c, res: permute(c), canonical: permute({ ...c, det: true, mode: "none" }) }
  }
  action init = { nondet det = Bool.oneOf() initMode("none", det) }
  action init_popFront = initMode("popFront", false)
  action init_skipLastKey = initMode("skipLastKey", false)
  action init_fillFirstRows = initMode("fillFirstRows", false)
  action init_noFirstConsume = initMode("noFirstConsume", false)
  action init_detIgnored = initMode("detIgnored", true)
  action step = lk' = lk

  val r = lk.res
  // L1: S' is a permutation of the usable table rows.
  val tablePermutation = VALUES.forall(v => count(r.sPrime, v) == count(lk.case.table, v))
  // L2: lookup argument shape (debug block :444-456): S'[i] == A'[i] or A'[i] == A'[i-1]; and row 0 matches.
  val adjacency = range(0, ROWS).foldl(true, (ok, i) =>
    ok and (r.aPrime[i] == r.sPrime[i] or (i > 0 and r.aPrime[i] == r.aPrime[i - 1])))
  val noPanic = not(r.error)
  val leftoverSafety = noPanic and tablePermutation and adjacency
  // L3: `loki-deterministic-lookup-order` output is a function of (input, table) only.
  val deterministic = lk.case.det implies r == lk.canonical
  // Witnesses (positive): stock order really changes S' (byte nondeterminism), and many leftovers exist.
  val rOrderMatters = not(lk.case.det) and r.sPrime != lk.canonical.sPrime
  val rTwoLeftoverKeys = keysOf(lk.case.table).select(k => count(lk.case.table, k) - (if (count(lk.case.input, k) > 0) 1 else 0) > 0).length() >= 2
}

module lookup_leftover_r3 { import lookup_leftover(ROWS = 3).* }
module lookup_leftover_r4 { import lookup_leftover(ROWS = 4).* }
module lookup_leftover_r5 { import lookup_leftover(ROWS = 5).* }
module lookup_leftover_r6 { import lookup_leftover(ROWS = 6).* }
