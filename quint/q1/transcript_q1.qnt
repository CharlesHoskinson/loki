// quint-q1: prover/verifier Fiat-Shamir transcript order for Loki a256a9a.
//
// Two INDEPENDENT generators are derived from two different Rust loop nests:
//   proverOps   <- src/plonk/prover.rs (compute_trace, compute_instances,
//                  parse_advices, finalise_proof, write_evals_to_transcript,
//                  compute_queries), lookup/permutation/trash/vanishing provers,
//                  src/poly/kzg/mod.rs multi_open (stock path) or
//                  src/poly/kzg/multiopen.rs prepare (`loki-multiopen` path).
//   verifierOps <- src/plonk/verifier.rs (parse_trace, verify_algebraic_constraints)
//                  and sub-verifiers, src/poly/kzg/mod.rs multi_prepare.
// Multiopen point sets are COMPUTED from each side's query list with the
// construct_intermediate_sets algorithm (src/poly/kzg/utils.rs:37) followed by the
// (cardinality, index) sort, never hard-coded.
// Challenges carry a derivation tag so `truncated-challenges` consistency is checked.
// The state machine runs the prover (with out-of-order computation inside a phase)
// and the verifier, which takes `common` items from its OWN inputs and `read` items
// from the prover's proof stream. Inline comments cite Loki a256a9a file:line.
module transcript_q1 {
  // ---------- configuration ----------
  type Cfg = {
    proofs: int,
    committed: int,        // nb_committed_instances (first columns)
    plain: int,            // non-committed instance columns
    values: int,           // public values per instance column
    adv0: int, adv1: int, adv2: int,   // advice columns per phase
    ch0: int, ch1: int,    // challenges usable after phase 0 / phase 1
    advNext: bool,         // advice column 0 also queried at Rotation::next
    lookups: int,
    permCols: int,         // equality-enabled columns
    trash: int,
    multiopen: bool,       // feature loki-multiopen (prover side only)
    truncated: bool,       // feature truncated-challenges (both sides)
    aliasCommitted: bool,  // verifier caller passes ONE committed-instance slice for all proofs
    mode: str              // "none" or a mutation name (prover/verifier code mutation)
  }
  // Fixture constants (set_minimum_degree(5) in quint/q1/mbt): chunk = degree-2, h pieces = degree-1.
  pure val DEGREE = 5
  pure def permSets(c: Cfg): int = if (c.permCols == 0) 0 else (c.permCols + DEGREE - 3) / (DEGREE - 2)
  pure val QUOTIENT = DEGREE - 1
  pure def numAdvice(c: Cfg): int = c.adv0 + c.adv1 + c.adv2
  pure def phaseOf(c: Cfg, col: int): int =
    if (col < c.adv0) 0 else if (col < c.adv0 + c.adv1) 1 else 2
  pure def maxPhase(c: Cfg): int = if (c.adv2 > 0) 2 else if (c.adv1 > 0) 1 else 0
  pure def chIn(c: Cfg, phase: int): int = if (phase == 0) c.ch0 else if (phase == 1) c.ch1 else 0
  // Rotations as ints. LAST = -(blinding_factors+1), symbolic and distinct from -1,0,1.
  pure val CUR = 0
  pure val NEXT = 1
  pure val PREV = -1
  pure val LAST = -100
  // Queries in creation order of the fixture's configure(); counts are cross-checked in Rust.
  pure def adviceQueries(c: Cfg): List[{ col: int, rot: int }] =
    range(0, numAdvice(c)).foldl(List(), (acc, col) => acc.append({ col: col, rot: CUR }))
      .concat(if (c.advNext) List({ col: 0, rot: NEXT }) else List())
  pure def instanceQueries(c: Cfg): List[int] = range(0, c.committed + c.plain)
  // fixed: zero-gate column, one table column per lookup, trash selector column, equality columns.
  pure def fixedQueries(c: Cfg): int = 1 + c.lookups + c.trash + c.permCols

  // ---------- events ----------
  type Ev = { op: str, label: str, proof: int, col: int, idx: int, set: List[int], deriv: str }
  pure def ev(op: str, label: str, proof: int, col: int, idx: int): Ev =
    { op: op, label: label, proof: proof, col: col, idx: idx, set: List(), deriv: "" }
  pure def sq(label: str, idx: int, deriv: str): Ev =
    { op: "squeeze", label: label, proof: -1, col: -1, idx: idx, set: List(), deriv: deriv }
  pure def W(label: str, proof: int, col: int, idx: int): Ev = ev("write", label, proof, col, idx)
  pure def C(label: str, proof: int, col: int, idx: int): Ev = ev("common", label, proof, col, idx)
  pure def flat1(n1: int, f1: int => List[Ev]): List[Ev] =
    range(0, n1).foldl(List(), (acc1, i1) => acc1.concat(f1(i1)))
  pure def flat2(n2: int, f2: int => List[Ev]): List[Ev] =
    range(0, n2).foldl(List(), (acc2, i2) => acc2.concat(f2(i2)))
  pure def flat3(n3: int, f3: int => List[Ev]): List[Ev] =
    range(0, n3).foldl(List(), (acc3, i3) => acc3.concat(f3(i3)))
  pure def mapL1(l1: List[a1], g1: a1 => b1): List[b1] = l1.foldl(List(), (acc1, e1) => acc1.append(g1(e1)))
  // Quint 0.32.0 captures names when a higher-order pure def is nested inside its own
  // lambda argument (repro: quint/q1/quint_capture_repro.qnt); every nesting level therefore has its own copy.
  pure def has(l: List[a], x: a): bool = l.foldl(false, (b, e) => b or e == x)

  // ---------- multiopen point sets: utils.rs construct_intermediate_sets + (len,i) sort ----------
  type Poly = { label: str, proof: int, col: int }
  type Query = { poly: Poly, rot: int }
  pure def indexOf(l: List[int], x: int): int =
    l.indices().fold(-1, (r, i) => if (r < 0 and l[i] == x) i else r)
  pure def pointOrder(qs: List[Query]): List[int] =
    qs.foldl(List(), (acc, q) => if (has(acc, q.rot)) acc else acc.append(q.rot))
  pure def commitments(qs: List[Query]): List[Poly] =
    qs.foldl(List(), (acc, q) => if (has(acc, q.poly)) acc else acc.append(q.poly))
  pure def duplicated(qs: List[Query]): bool =
    qs.indices().exists(i => qs.indices().exists(j => i < j and qs[i] == qs[j]))
  pure def unsortedSets(qs: List[Query]): List[List[int]] = {
    val points = pointOrder(qs)
    val idxSets = commitments(qs).foldl(List(), (acc, p) => {
      val s = qs.select(q => q.poly == p).foldl(Set(), (s, q) => s.union(Set(indexOf(points, q.rot))))
      if (has(acc, s)) acc else acc.append(s)
    })
    // BTreeSet iteration: ascending point index, mapped back to rotations.
    mapL1(idxSets, s => range(0, points.length()).foldl(List(), (l, i) => if (s.contains(i)) l.append(points[i]) else l))
  }
  pure def sortByLenThenIndex(sets: List[List[int]]): List[List[int]] =
    range(1, 8).foldl(List(), (acc, len) => acc.concat(sets.select(s => s.length() == len)))
  pure def pointSets(qs: List[Query]): List[List[int]] = sortByLenThenIndex(unsortedSets(qs))

  // ---------- prover side ----------
  pure def P(label: str, proof: int, col: int): Poly = { label: label, proof: proof, col: col }
  // prover.rs compute_queries (:794); lookup/permutation/trash/vanishing `open`.
  pure def proverQueries(c: Cfg): List[Query] = {
    val n = permSets(c)
    range(0, c.proofs).foldl(List(), (acc, p) => acc
      .concat(mapL1(adviceQueries(c), q => { poly: P("advice", p, q.col), rot: q.rot }))
      .concat(mapL1(instanceQueries(c).select(col => col < c.committed), col => { poly: P("instance", p, col), rot: CUR }))
      .concat(range(0, n).foldl(List(), (l, s) => l.append({ poly: P("pz", p, s), rot: CUR }).append({ poly: P("pz", p, s), rot: NEXT })))
      .concat(range(0, n).foldl(List(), (l, i) => if (i == 0) l else l.append({ poly: P("pz", p, n - 1 - i), rot: LAST })))
      .concat(range(0, c.lookups).foldl(List(), (l, k) => l
        .append({ poly: P("lprod", p, k), rot: CUR }).append({ poly: P("lin", p, k), rot: CUR })
        .append({ poly: P("ltab", p, k), rot: CUR }).append({ poly: P("lin", p, k), rot: PREV })
        .append({ poly: P("lprod", p, k), rot: NEXT })))
      .concat(range(0, c.trash).foldl(List(), (l, t) => l.append({ poly: P("trash", p, t), rot: CUR }))))
    .concat(range(0, fixedQueries(c)).foldl(List(), (l, j) => l.append({ poly: P("fixed", -1, j), rot: CUR })))
    .concat(range(0, c.permCols).foldl(List(), (l, j) => l.append({ poly: P("sigma", -1, j), rot: CUR })))
    .append({ poly: P("h", -1, 0), rot: CUR }).append({ poly: P("random", -1, 0), rot: CUR })
  }
  // Instance absorption, prover.rs compute_instances (:449-500): proof-major.
  pure def proverInstances(c: Cfg): List[Ev] =
    flat1(c.proofs, p => flat2(c.committed + c.plain, col =>
      if (col < c.committed) List(C("inst_com", p, col, 0))
      else List(C("inst_len", p, col, 0)).concat(flat3(c.values, i => List(C("inst_val", p, col, i))))))
  // Verifier-style order (global committed, then plain): used only by mutation "verifierInstanceOrder".
  pure def verifierInstances(c: Cfg): List[Ev] =
    flat1(c.proofs, p => flat2(c.committed, col => List(C("inst_com", p, col, 0))))
      .concat(flat1(c.proofs, p => flat2(c.plain, j => {
        val col = c.committed + j
        List(C("inst_len", p, col, 0)).concat(flat3(c.values, i => List(C("inst_val", p, col, i))))
      })))
  // parse_advices (:558-660) / verifier.rs:85-100: per phase, per proof, ascending column; then challenges.
  pure def advicePhases(c: Cfg, swapFirstTwo: bool, dropPhaseChallenge: bool): List[Ev] =
    flat1(maxPhase(c) + 1, ph => {
      val cols = range(0, numAdvice(c)).select(col => phaseOf(c, col) == ph)
      val commits = flat2(c.proofs, p => {
        val l = mapL1(cols, col => W("advice", p, col, 0))
        if (swapFirstTwo and p == 0 and l.length() >= 2)
          List(l[1], l[0]).concat(l.slice(2, l.length())) else l
      })
      val base = if (ph == 0) 0 else if (ph == 1) c.ch0 else c.ch0 + c.ch1
      val chs = flat2(chIn(c, ph), i => List(sq("phase_challenge", base + i, "plain")))
      commits.concat(if (dropPhaseChallenge) chs.slice(0, 0) else chs)
    })
  pure def lookupPermuted(c: Cfg, lookupMajor: bool): List[Ev] =
    if (lookupMajor)
      flat1(c.lookups, k => flat2(c.proofs, p => List(W("lin", p, k, 0), W("ltab", p, k, 0))))
    else flat1(c.proofs, p => flat2(c.lookups, k => List(W("lin", p, k, 0), W("ltab", p, k, 0))))
  pure def permProducts(c: Cfg): List[Ev] = flat1(c.proofs, p => flat2(permSets(c), s => List(W("pz", p, s, 0))))
  pure def lookupProducts(c: Cfg): List[Ev] = flat1(c.proofs, p => flat2(c.lookups, k => List(W("lprod", p, k, 0))))
  pure def evaluations(c: Cfg): List[Ev] = {
    val n = permSets(c)
    flat1(c.proofs, p => mapL1(instanceQueries(c).select(col => col < c.committed), col => W("inst_eval", p, col, 0)))
      .concat(flat1(c.proofs, p => range(0, adviceQueries(c).length()).foldl(List(), (l, i) => l.append(W("adv_eval", p, i, 0)))))
      .concat(flat1(fixedQueries(c), j => List(W("fixed_eval", -1, j, 0))))
      .append(W("random_eval", -1, 0, 0))
      .concat(flat1(c.permCols, j => List(W("sigma_eval", -1, j, 0))))
      .concat(flat1(c.proofs, p => flat2(n, s =>
        List(W("pz_eval", p, s, 0), W("pz_next_eval", p, s, 0))
          .concat(if (s < n - 1) List(W("pz_last_eval", p, s, 0)) else List()))))
      .concat(flat1(c.proofs, p => flat2(c.lookups, k => List(W("lprod_eval", p, k, 0), W("lprod_next_eval", p, k, 0),
        W("lin_eval", p, k, 0), W("lin_inv_eval", p, k, 0), W("ltab_eval", p, k, 0)))))
      .concat(flat1(c.proofs, p => flat2(c.trash, t => List(W("trash_eval", p, t, 0)))))
  }
  pure def tr(c: Cfg, d: str): str = if (c.truncated) d else "plain"
  pure def qEvals(sets: List[List[int]]): List[Ev] =
    range(0, sets.length()).foldl(List(), (l, i) => l.append({ ...W("q_eval", -1, -1, i), set: sets[i] }))
  // kzg/mod.rs multi_open (:96-232). Stock path :117-173, loki-multiopen path multiopen.rs:102-148.
  // Both paths: x1 -> powers or truncated_powers (stock :129-136, loki :114-117); x2 -> powers (stock :172,
  // loki :147); x3 -> truncate (:181-182); x4 -> truncated_powers (:197-201).
  pure def proverMultiopen(c: Cfg): List[Ev] = {
    val qs = proverQueries(c)
    val sets = if (c.multiopen and c.mode == "lokiUnsortedSets") unsortedSets(qs) else pointSets(qs)
    val x2d = if (c.multiopen and c.mode == "lokiTruncX2") tr(c, "truncPowers") else "plain"
    val x1 = sq("x1", 0, tr(c, "truncPowers"))
    val x2 = sq("x2", 0, x2d)
    val f = W("f", -1, 0, 0)
    val x3 = sq("x3", 0, tr(c, "truncScalar"))
    val x4 = sq("x4", 0, tr(c, "truncPowers"))
    val pi = W("pi", -1, 0, 0)
    if (c.mode == "x3BeforeF" or c.mode == "x3BeforeFBoth")
      List(x1, x2, x3, f).concat(qEvals(sets)).concat(List(x4, pi))
    else if (c.mode == "x4AfterPi")
      List(x1, x2, f, x3).concat(qEvals(sets)).concat(List(pi, x4))
    else List(x1, x2, f, x3).concat(qEvals(sets)).concat(List(x4, pi))
  }
  pure def proverOps(c: Cfg): List[Ev] = {
    val m = c.mode
    List(C("vk", -1, 0, 0))
      .concat(if (m == "verifierInstanceOrder") verifierInstances(c) else proverInstances(c))
      .concat(advicePhases(c, m == "swapAdvice", m == "dropPhaseChallenge"))
      .append(sq("theta", 0, "plain"))
      .concat(lookupPermuted(c, m == "lookupMajor"))
      .concat(if (m == "dropBeta") List() else List(sq("beta", 0, "plain")))
      .append(sq("gamma", 0, "plain"))
      .concat(if (m == "swapProducts") lookupProducts(c).concat(permProducts(c))
              else permProducts(c).concat(lookupProducts(c)))
      .append(sq("trash", 0, "plain"))
      .concat(flat1(c.proofs, p => flat2(c.trash, t => List(W("trash", p, t, 0)))))
      .append(W("random", -1, 0, 0))
      .append(sq("y", 0, "plain"))
      .concat(flat1(QUOTIENT, i => List(W("h", -1, i, 0))))
      .append(sq("x", 0, "plain"))
      .concat(evaluations(c))
      .concat(proverMultiopen(c))
  }

  // ---------- verifier side (stock 0.8.3, byte-identical in Loki) ----------
  // verifier.rs:340-405 query chain and sub-verifier `queries`; identity of committed
  // instance commitments is the caller's reference (ptr::eq, query.rs:114-122).
  pure def verifierQueries(c: Cfg): List[Query] = {
    val n = permSets(c)
    range(0, c.proofs).foldl(List(), (acc, p) => acc
      .concat(mapL1(adviceQueries(c), q => { poly: P("advice", p, q.col), rot: q.rot }))
      .concat(mapL1(instanceQueries(c).select(col => col < c.committed), col => { poly: P("instance", if (c.aliasCommitted) 0 else p, col), rot: CUR }))
      .concat(range(0, n).foldl(List(), (l, s) => l.append({ poly: P("pz", p, s), rot: CUR }).append({ poly: P("pz", p, s), rot: NEXT })))
      .concat(range(0, n).foldl(List(), (l, i) => if (i == 0) l else l.append({ poly: P("pz", p, n - 1 - i), rot: LAST })))
      .concat(range(0, c.lookups).foldl(List(), (l, k) => l
        .append({ poly: P("lprod", p, k), rot: CUR }).append({ poly: P("lin", p, k), rot: CUR })
        .append({ poly: P("ltab", p, k), rot: CUR }).append({ poly: P("lin", p, k), rot: PREV })
        .append({ poly: P("lprod", p, k), rot: NEXT })))
      .concat(range(0, c.trash).foldl(List(), (l, t) => l.append({ poly: P("trash", p, t), rot: CUR }))))
    .concat(range(0, fixedQueries(c)).foldl(List(), (l, j) => l.append({ poly: P("fixed", -1, j), rot: CUR })))
    .concat(range(0, c.permCols).foldl(List(), (l, j) => l.append({ poly: P("sigma", -1, j), rot: CUR })))
    .append({ poly: P("h", -1, 0), rot: CUR }).append({ poly: P("random", -1, 0), rot: CUR })
  }
  pure def R(label: str, proof: int, col: int, idx: int): Ev = ev("write", label, proof, col, idx)
  pure def verifierOps(c: Cfg): List[Ev] = {
    val n = permSets(c)
    val sets = pointSets(verifierQueries(c))
    val mo = if (c.mode == "x3BeforeFBoth")
      List(sq("x1", 0, tr(c, "truncPowers")), sq("x2", 0, "plain"), sq("x3", 0, tr(c, "truncScalar")), R("f", -1, 0, 0))
      else List(sq("x1", 0, tr(c, "truncPowers")), sq("x2", 0, "plain"), R("f", -1, 0, 0), sq("x3", 0, tr(c, "truncScalar")))
    // parse_trace :59-76 : VK, then committed of every proof, then plain of every proof.
    List(C("vk", -1, 0, 0))
      .concat(flat1(c.proofs, p => flat2(c.committed, col => List(C("inst_com", p, col, 0)))))
      .concat(flat1(c.proofs, p => flat2(c.plain, j => {
        val col = c.committed + j
        List(C("inst_len", p, col, 0)).concat(flat3(c.values, i => List(C("inst_val", p, col, i))))
      })))
      // :85-100
      .concat(flat1(maxPhase(c) + 1, ph => {
        val base = if (ph == 0) 0 else if (ph == 1) c.ch0 else c.ch0 + c.ch1
        flat2(c.proofs, p => mapL1(range(0, numAdvice(c)).select(col => phaseOf(c, col) == ph), col => R("advice", p, col, 0)))
          .concat(flat2(chIn(c, ph), i => List(sq("phase_challenge", base + i, "plain"))))
      }))
      .append(sq("theta", 0, "plain"))                                   // :106
      .concat(flat1(c.proofs, p => flat2(c.lookups, k => List(R("lin", p, k, 0), R("ltab", p, k, 0))))) // :108-117, lookup/verifier.rs:41-42
      .append(sq("beta", 0, "plain")).append(sq("gamma", 0, "plain"))  // :120-123
      .concat(flat1(c.proofs, p => flat2(n, s => List(R("pz", p, s, 0)))))  // :125-131, permutation/verifier.rs:44
      .concat(flat1(c.proofs, p => flat2(c.lookups, k => List(R("lprod", p, k, 0))))) // :133-141
      .append(sq("trash", 0, "plain"))                                   // :143
      .concat(flat1(c.proofs, p => flat2(c.trash, t => List(R("trash", p, t, 0))))) // :145-153
      .append(R("random", -1, 0, 0))                                     // :155 vanishing/verifier.rs:42
      .append(sq("y", 0, "plain"))                                       // :158
      .concat(flat1(QUOTIENT, i => List(R("h", -1, i, 0))))              // :234 read_commitments_after_y
      .append(sq("x", 0, "plain"))                                       // :238
      // :240-292 instance evals (committed only), advice, fixed, random, sigma
      .concat(flat1(c.proofs, p => mapL1(instanceQueries(c).select(col => col < c.committed), col => R("inst_eval", p, col, 0))))
      .concat(flat1(c.proofs, p => range(0, adviceQueries(c).length()).foldl(List(), (l, i) => l.append(R("adv_eval", p, i, 0)))))
      .concat(flat1(fixedQueries(c), j => List(R("fixed_eval", -1, j, 0))))
      .append(R("random_eval", -1, 0, 0))
      .concat(flat1(c.permCols, j => List(R("sigma_eval", -1, j, 0))))
      // permutation/verifier.rs:72-95, lookup/verifier.rs:69-80, trash/verifier.rs:35-42
      .concat(flat1(c.proofs, p => flat2(n, s =>
        List(R("pz_eval", p, s, 0), R("pz_next_eval", p, s, 0))
          .concat(if (s < n - 1) List(R("pz_last_eval", p, s, 0)) else List()))))
      .concat(flat1(c.proofs, p => flat2(c.lookups, k => List(R("lprod_eval", p, k, 0), R("lprod_next_eval", p, k, 0),
        R("lin_eval", p, k, 0), R("lin_inv_eval", p, k, 0), R("ltab_eval", p, k, 0)))))
      .concat(flat1(c.proofs, p => flat2(c.trash, t => List(R("trash_eval", p, t, 0)))))
      // kzg/mod.rs multi_prepare :235-345
      .concat(mo).concat(qEvals(sets))
      .append(sq("x4", 0, tr(c, "truncPowers"))).append(R("pi", -1, 0, 0))
  }

  // ---------- Fiat-Shamir dependency table (soundness-relevant order, independent of both sides) ----------
  // Items whose value is computed FROM a challenge must be absorbed after it.
  pure def dependsOn(label: str): str =
    if (label == "lin" or label == "ltab") "theta"
    else if (label == "pz" or label == "lprod") "gamma"
    else if (label == "trash") "trash"
    else if (label == "h") "y"
    else if (label.in(Set("inst_eval", "adv_eval", "fixed_eval", "random_eval", "sigma_eval", "pz_eval",
      "pz_next_eval", "pz_last_eval", "lprod_eval", "lprod_next_eval", "lin_eval", "lin_inv_eval",
      "ltab_eval", "trash_eval"))) "x"
    else if (label == "f") "x2"
    else if (label == "q_eval") "x3"
    else if (label == "pi") "x4"
    else ""
  // Challenge c must be squeezed after every absorbed item with a label in prereq(c).
  pure def prereq(ch: str): Set[str] =
    if (ch == "theta") Set("vk", "inst_com", "inst_len", "inst_val", "advice")
    else if (ch == "beta") Set("lin", "ltab")
    else if (ch == "trash") Set("pz", "lprod")
    else if (ch == "y") Set("trash", "random")
    else if (ch == "x") Set("h")
    else if (ch == "x1") Set("inst_eval", "adv_eval", "fixed_eval", "random_eval", "sigma_eval", "pz_eval",
      "pz_next_eval", "pz_last_eval", "lprod_eval", "lprod_next_eval", "lin_eval", "lin_inv_eval", "ltab_eval", "trash_eval")
    else if (ch == "x3") Set("f")
    else if (ch == "x4") Set("q_eval")
    else Set()
  pure def requiredChallenges: List[str] =
    List("theta", "beta", "gamma", "trash", "y", "x", "x1", "x2", "x3", "x4")

  // ---------- state machine ----------
  type St = {
    cfg: Cfg,
    pPlan: List[Ev], vPlan: List[Ev],
    pc: int, ready: Set[int],
    pAbsorbed: List[Ev],            // prover hash input so far
    pChallenges: List[{ ev: Ev, n: int }],  // n = length of pAbsorbed at the squeeze
    stream: List[Ev],               // proof bytes (prover writes, in order)
    vc: int, sc: int,               // verifier plan cursor, stream cursor
    vAbsorbed: List[Ev],
    vChallenges: List[{ ev: Ev, n: int }],
    misread: List[{ expected: Ev, got: Ev }],
    fsBad: bool,
    vDone: bool, vError: bool
  }
  var st: St

  pure def isPoint(e: Ev): bool = e.op == "write" and not(e.label.in(Set(
    "inst_eval", "adv_eval", "fixed_eval", "random_eval", "sigma_eval", "pz_eval", "pz_next_eval",
    "pz_last_eval", "lprod_eval", "lprod_next_eval", "lin_eval", "lin_inv_eval", "ltab_eval", "trash_eval", "q_eval")))
  pure def kindOf(e: Ev): str = if (e.op == "squeeze") "squeeze" else if (isPoint(e) or e.label == "inst_com") "point" else "scalar"
  pure def cfgOk(c: Cfg): bool = all {
    c.adv0 >= 1, c.adv1 == 0 or c.adv0 > 0, c.adv2 == 0 or c.adv1 > 0,
    c.ch0 == 0 or c.adv1 > 0, c.ch1 == 0 or c.adv2 > 0
  }
  pure def initState(c: Cfg): St = {
    cfg: c, pPlan: proverOps(c), vPlan: verifierOps(c), pc: 0, ready: Set(),
    pAbsorbed: List(), pChallenges: List(), stream: List(),
    vc: 0, sc: 0, vAbsorbed: List(), vChallenges: List(), misread: List(), fsBad: false,
    vDone: false, vError: duplicated(verifierQueries(c))
  }
  action initWith(c: Cfg): bool = st' = initState(c)
  action initMode(mode: str): bool = {
    nondet proofs = 1.to(3).oneOf()
    nondet committed = 0.to(2).oneOf()
    nondet plain = 0.to(2).oneOf()
    nondet values = 0.to(2).oneOf()
    nondet adv0 = 1.to(2).oneOf()
    nondet adv1 = 0.to(1).oneOf()
    nondet adv2 = 0.to(1).oneOf()
    nondet ch0 = 0.to(1).oneOf()
    nondet ch1 = 0.to(1).oneOf()
    nondet advNext = Bool.oneOf()
    nondet lookups = 0.to(2).oneOf()
    nondet permCols = 0.to(5).oneOf()
    nondet trash = 0.to(1).oneOf()
    nondet multiopen = Bool.oneOf()
    nondet truncated = Bool.oneOf()
    val c = { proofs: proofs, committed: committed, plain: plain, values: values,
      adv0: adv0, adv1: adv1, adv2: if (adv1 > 0) adv2 else 0, ch0: if (adv1 > 0) ch0 else 0,
      ch1: if (adv1 > 0 and adv2 > 0) ch1 else 0, advNext: advNext,
      lookups: lookups, permCols: permCols, trash: trash, multiopen: multiopen,
      truncated: truncated, aliasCommitted: false, mode: mode }
    all { cfgOk(c), initWith(c) }
  }
  action init = initMode("none")
  // Aliasing audit: the verifier caller reuses one committed-instance slice.
  action initAlias = {
    nondet proofs = 1.to(3).oneOf()
    nondet committed = 0.to(2).oneOf()
    initWith({ proofs: proofs, committed: committed, plain: 0, values: 0, adv0: 1, adv1: 0, adv2: 0,
      ch0: 0, ch1: 0, advNext: false, lookups: 0, permCols: 0, trash: 0, multiopen: false,
      truncated: false, aliasCommitted: true, mode: "none" })
  }

  // Prover: computation of a commitment/evaluation may finish out of order within the
  // current Fiat-Shamir phase (parallel MSMs, rayon par_iter over q sets), but only the
  // ordered collect publishes. Mutation "emitOnCompletion" publishes in completion order.
  pure def phaseOpen(s: St, i: int): bool =
    i >= s.pc and s.pPlan.slice(s.pc, i).foldl(true, (ok, e) => ok and e.op != "squeeze")
  action proverCompute = {
    val cand = s_indices.filter(i => phaseOpen(st, i) and st.pPlan[i].op == "write" and not(st.ready.contains(i)))
    all { cand.size() > 0, nondet i = cand.oneOf() st' = { ...st, ready: st.ready.union(Set(i)) } }
  }
  val s_indices = st.pPlan.indices()
  // Checked when item i is published: (a) its challenge dependency was already squeezed;
  // (b) for a squeeze, no prerequisite item remains unpublished (positions > i).
  pure def fsViolation(s: St, i: int): bool = {
    val e = s.pPlan[i]
    val d = dependsOn(e.label)
    if (e.op == "squeeze")
      s.pPlan.indices().exists(j => j > i and s.pPlan[j].op != "squeeze" and s.pPlan[j].label.in(prereq(e.label)))
    else d != "" and not(s.pChallenges.foldl(false, (b, x) => b or x.ev.label == d))
  }
  pure def publish(s: St, i: int): St = {
    val e = s.pPlan[i]
    val absorbed = if (e.op == "squeeze") s.pAbsorbed else s.pAbsorbed.append(e)
    val chs = if (e.op == "squeeze") s.pChallenges.append({ ev: e, n: s.pAbsorbed.length() }) else s.pChallenges
    val stream = if (e.op == "write") s.stream.append(e) else s.stream
    { ...s, pAbsorbed: absorbed, pChallenges: chs, stream: stream, fsBad: s.fsBad or fsViolation(s, i) }
  }
  action proverEmit = {
    val i = st.pc
    all {
      i < st.pPlan.length(),
      st.pPlan[i].op != "write" or st.ready.contains(i),
      st' = { ...publish(st, i), pc: i + 1 }
    }
  }
  // Mutation: the first ready item in the open phase is published even if earlier ones are pending.
  action proverEmitOnCompletion = {
    val cand = s_indices.filter(i => i > st.pc and phaseOpen(st, i) and st.pPlan[i].op == "write" and st.ready.contains(i))
    all {
      st.cfg.mode == "emitOnCompletion", cand.size() > 0,
      nondet i = cand.oneOf()
      val e = st.pPlan[i]
      val plan2 = st.pPlan.replaceAt(i, st.pPlan[st.pc]).replaceAt(st.pc, e)
      val ready2 = st.ready.exclude(Set(i, st.pc)).union(if (st.ready.contains(st.pc)) Set(i) else Set())
      st' = { ...publish({ ...st, pPlan: plan2 }, st.pc), pc: st.pc + 1, ready: ready2 }
    }
  }
  // Verifier: common items from its own inputs, reads from the proof stream, squeezes bind vAbsorbed.
  action verifierStep = {
    val i = st.vc
    val e = if (i < st.vPlan.length()) st.vPlan[i] else ev("", "", -1, -1, -1)
    all {
      not(st.vError), not(st.vDone),
      if (i >= st.vPlan.length()) st' = { ...st, vDone: true, vError: st.sc != st.stream.length() or st.pc < st.pPlan.length() }
      else if (e.op == "common") st' = { ...st, vc: i + 1, vAbsorbed: st.vAbsorbed.append(e) }
      else if (e.op == "squeeze") st' = { ...st, vc: i + 1, vChallenges: st.vChallenges.append({ ev: e, n: st.vAbsorbed.length() }) }
      else if (st.sc < st.stream.length())
        st' = { ...st, vc: i + 1, sc: st.sc + 1, vAbsorbed: st.vAbsorbed.append(st.stream[st.sc]),
          misread: if (st.stream[st.sc] == e) st.misread else st.misread.append({ expected: e, got: st.stream[st.sc] }) }
      // a read past the end of the proof fails (the prover has finished publishing)
      else all { st.pc >= st.pPlan.length(), st' = { ...st, vError: true } }
    }
  }
  action stutter = all { st.pc >= st.pPlan.length(), st.vDone or st.vError, st' = st }
  action step = any { proverCompute, proverEmit, proverEmitOnCompletion, verifierStep, stutter }

  // ---------- properties ----------
  pure def mixedBatch(c: Cfg): bool = c.proofs > 1 and c.committed > 0 and c.plain > 0
  // I1: the k-th challenge is computed from identical absorbed sequences with identical derivations.
  pure def chAgree(s: St, k: int): bool = {
    val a = s.pChallenges[k]
    val b = s.vChallenges[k]
    a.ev == b.ev and a.n == b.n and s.pAbsorbed.slice(0, a.n) == s.vAbsorbed.slice(0, b.n)
  }
  val challengesAgree = st.vChallenges.indices().forall(k =>
    k < st.pChallenges.length() implies chAgree(st, k))
    and ((st.vDone and st.pc >= st.pPlan.length()) implies st.pChallenges.length() == st.vChallenges.length())
  // I2: every proof item is interpreted by the verifier as the item the prover meant.
  val readsAgree = st.misread.length() == 0
  // I3: Fiat-Shamir dependency order of the prover's (and hence the proof's) schedule.
  val fsDependencies = not(st.fsBad)
  // I4: every required challenge is squeezed exactly once, in schedule order, on both completed sides.
  pure def chLabels(l: List[{ ev: Ev, n: int }]): List[str] =
    mapL1(l, x => x.ev.label).select(x => x != "phase_challenge")
  val challengeSchedule = (st.pc >= st.pPlan.length()) implies chLabels(st.pChallenges) == requiredChallenges
  // I5: completion — the verifier consumes the whole proof without error (assert_empty).
  val completesCleanly = st.vDone implies not(st.vError)
  val safety = challengesAgree and readsAgree and fsDependencies and challengeSchedule and completesCleanly
  // Stock defect, exact classification over completed runs (aliasing excluded, see initAlias).
  val defectExact = (st.vDone or st.vError) implies
    ((challengesAgree and readsAgree and not(st.vError)) iff not(mixedBatch(st.cfg)))
  val safetyOutsideDefect = mixedBatch(st.cfg) or safety
  // I6: the two independently derived schedules coincide exactly outside the defect.
  val plansEqual = mixedBatch(st.cfg) or st.cfg.aliasCommitted or st.pPlan == st.vPlan
  // Aliased committed-instance references make the stock verifier reject (DuplicatedQuery).
  val aliasClassified = st.cfg.aliasCommitted implies
    (st.vError iff (st.cfg.proofs > 1 and st.cfg.committed > 0))

  // ---------- reachability witnesses ----------
  // r*: positive predicates for `quint run --witnesses` (counts traces reaching them).
  // w*: kit-style negated goals for `--invariant` (a violation = reachable).
  val rComplete = st.vDone and not(st.vError)
  val rDefect = mixedBatch(st.cfg) and not(challengesAgree)
  val rManySets = st.vDone and st.vPlan.select(e => e.label == "q_eval").length() >= 4
  val rPermChain = st.vDone and st.vPlan.select(e => e.label == "pz_last_eval").length() > 0
  val rPhaseChallenge = st.vDone and st.cfg.ch1 > 0
  val rTruncMultiopen = st.vDone and st.cfg.truncated and st.cfg.multiopen and st.cfg.lookups == 2
  val rBatched = st.ready.filter(i => i > st.pc).size() > 1
  val rTrash = st.vDone and st.cfg.trash > 0 and st.cfg.proofs == 3
  val rVerifierAhead = st.vChallenges.length() > st.pChallenges.length()
  val wComplete = not(rComplete)
  val wDefect = not(rDefect)
  val wManySets = not(rManySets)
  val wBatched = not(rBatched)
  // Mutation harness: each init forces one code mutation; `safetyOutsideDefect` (or `defectExact`
  // for verifierInstanceOrder) must be violated. These runs are expected to FAIL (quint/q1/check_q1.py).
  action init_swapAdvice = initMode("swapAdvice")
  action init_dropPhaseChallenge = initMode("dropPhaseChallenge")
  action init_dropBeta = initMode("dropBeta")
  action init_swapProducts = initMode("swapProducts")
  action init_lookupMajor = initMode("lookupMajor")
  action init_x3BeforeF = initMode("x3BeforeF")
  action init_x3BeforeFBoth = initMode("x3BeforeFBoth")
  action init_x4AfterPi = initMode("x4AfterPi")
  action init_lokiTruncX2 = initMode("lokiTruncX2")
  action init_lokiUnsortedSets = initMode("lokiUnsortedSets")
  action init_emitOnCompletion = initMode("emitOnCompletion")
  action init_verifierInstanceOrder = initMode("verifierInstanceOrder")
}
