// Scenario tests for transcript_q1 (run: quint test quint/q1/transcript_q1_test.qnt --match '.*Test$').
module transcript_q1_test {
  import transcript_q1.* from "./transcript_q1"
  pure val base = { proofs: 1, committed: 0, plain: 0, values: 0, adv0: 1, adv1: 0, adv2: 0,
    ch0: 0, ch1: 0, advNext: false, lookups: 0, permCols: 0, trash: 0, multiopen: false,
    truncated: false, aliasCommitted: false, mode: "none" }
  // Stock defect: two proofs, one committed + one plain column (Lean stockTrace/verifierTrace).
  pure val mixed = { ...base, proofs: 2, committed: 1, plain: 1, values: 1 }
  pure def commons(l: List[Ev]): List[Ev] = l.select(e => e.op == "common")
  run mixedOrderTest = all {
    assert(commons(proverOps(mixed)) == List(C("vk", -1, 0, 0), C("inst_com", 0, 0, 0),
      C("inst_len", 0, 1, 0), C("inst_val", 0, 1, 0), C("inst_com", 1, 0, 0), C("inst_len", 1, 1, 0), C("inst_val", 1, 1, 0))),
    assert(commons(verifierOps(mixed)) == List(C("vk", -1, 0, 0), C("inst_com", 0, 0, 0), C("inst_com", 1, 0, 0),
      C("inst_len", 0, 1, 0), C("inst_val", 0, 1, 0), C("inst_len", 1, 1, 0), C("inst_val", 1, 1, 0))),
    // the proof stream (writes) is identical: only the hash inputs differ
    assert(proverOps(mixed).select(e => e.op == "write") == verifierOps(mixed).select(e => e.op == "write")),
  }
  run singleMixedAgreesTest = assert(proverOps({ ...mixed, proofs: 1 }) == verifierOps({ ...mixed, proofs: 1 }))
  run allCommittedAgreesTest = assert(proverOps({ ...mixed, plain: 0, committed: 2 }) == verifierOps({ ...mixed, plain: 0, committed: 2 }))
  // Point sets for 1 proof, 4 equality columns (2 chunks), 1 lookup: computed by hand from
  // compute_queries order, then the (cardinality, index) sort.
  run pointSetsTest = {
    val c = { ...base, permCols: 4, lookups: 1 }
    all {
      assert(permSets(c) == 2),
      assert(pointSets(proverQueries(c)) == List(List(CUR), List(CUR, NEXT), List(CUR, PREV), List(CUR, NEXT, LAST))),
      assert(pointSets(verifierQueries(c)) == pointSets(proverQueries(c))),
      assert(unsortedSets(proverQueries(c)) == List(List(CUR), List(CUR, NEXT, LAST), List(CUR, NEXT), List(CUR, PREV))),
    }
  }
  run truncationTagsTest = {
    val ops = proverOps({ ...base, truncated: true }).select(e => e.op == "squeeze")
    assert(mapL1(ops, e => e.deriv).slice(ops.length() - 4, ops.length()) ==
      List("truncPowers", "plain", "truncScalar", "truncPowers"))
  }
  // Aliased committed-instance references: same commitment pointer at the same point.
  run aliasTest = all {
    assert(duplicated(verifierQueries({ ...base, proofs: 2, committed: 1, aliasCommitted: true }))),
    assert(not(duplicated(verifierQueries({ ...base, proofs: 1, committed: 1, aliasCommitted: true })))),
    assert(not(duplicated(proverQueries({ ...base, proofs: 2, committed: 1, aliasCommitted: true })))),
  }
  // Mutations change the schedules as intended (the campaigns show they are caught).
  run mutationShapesTest = {
    val c = { ...base, adv0: 2, lookups: 1, permCols: 1 }
    all {
      assert(proverOps({ ...c, mode: "swapAdvice" }) != verifierOps(c)),
      assert(proverOps({ ...c, mode: "swapProducts" }) != proverOps(c)),
      assert(proverOps({ ...c, mode: "x3BeforeFBoth" }).select(e => e.op != "squeeze") ==
        verifierOps({ ...c, mode: "x3BeforeFBoth" }).select(e => e.op != "squeeze")),
    }
  }
}
