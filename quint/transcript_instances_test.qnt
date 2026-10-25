module transcript_instances_test {
  import transcript_instances.* from "./transcript"
  run allPlainMatchTest = initCase({ proofs: 3, committedColumns: 0, plainColumns: 2, values: 2 })
    .expect(proverEqualsVerifier)
  run allCommittedMatchTest = initCase({ proofs: 3, committedColumns: 2, plainColumns: 0, values: 1 })
    .expect(proverEqualsVerifier)
  run singleMixedMatchTest = initCase({ proofs: 1, committedColumns: 1, plainColumns: 1, values: 1 })
    .expect(proverEqualsVerifier)
  run mixedBatchMismatchTest = initCase({ proofs: 2, committedColumns: 1, plainColumns: 1, values: 1 })
    .expect(not(proverEqualsVerifier) and mismatchExactlyMixedBatch and
      stockInstanceOrder(layout).nth(1).kind == "length" and
      verifierInstanceOrder(layout).nth(1).kind == "commitment")
  run emptyPlainColumnStillMismatchesTest = initCase({ proofs: 2, committedColumns: 1, plainColumns: 1, values: 0 })
    .expect(not(proverEqualsVerifier) and mismatchExactlyMixedBatch)
  run exhaustiveBoundaryTest = all {
    1.to(3).forall(proofs => 0.to(2).forall(committedColumns =>
      0.to(2).forall(plainColumns => 0.to(2).forall(values => {
        val l = { proofs: proofs, committedColumns: committedColumns,
          plainColumns: plainColumns, values: values }
        ordersAgree(l) iff not(mixedBatch(l))
      }))))
  }
}
