module transcript_test {
  import transcript.* from "./transcript"
  run stockOrderTest = {
    val plan = stock(shapeFor(3, 2, false))
    all { plan.nth(0).label == "vk",
      plan.select(e => e.kind == "squeeze").length() == 11,
      plan.nth(plan.length() - 1).label == "multiopen_pi" }
  }
  run reorderedCommitmentRejectedTest = {
    val plan = stock(shapeFor(1, 2, false))
    val first = plan.indices().filter(i => plan.nth(i).label == "advice0").fold(plan.length(), (a, i) => if (i < a) i else a)
    val mutated = plan.replaceAt(first, plan.nth(first + 1)).replaceAt(first + 1, plan.nth(first))
    all { not(prefix(mutated, plan)) }
  }
  run earlyThetaRejectedTest = {
    val plan = stock(shapeFor(1, 1, false))
    all { not(prefix(List(plan.nth(0), event("squeeze", "theta", -1, 0)), plan)) }
  }
  run unreadyCommitmentBlockedTest =
    initFor(shapeFor(0, 1, false)).then(emit).then(emit).then(emit)
      .then(emit.fail())
  run committedInstancesTest = {
    val plan = stock(shapeFor(2, 2, true))
    all { plan.nth(1).kind == "common_point", plan.nth(2).proof == 1,
      plan.select(e => e.label == "instance_length").length() == 0 }
  }
}
