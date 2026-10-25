module scheduler_test {
  import scheduler.* from "./scheduler"
  run overlapTest = init.then(admit(0)).then(admit(2)).expect(overlap and safety)
  run twoHeavyBlockedTest = init.then(admit(0)).then(admit(1).fail())
  run twoSmallBlockedTest = init.then(admit(2)).then(admit(3).fail())
  run checkpointResumeTest = init.then(admit(0)).then(advance(0)).then(checkpoint(0))
    .expect(checkpointed and used(jobs) == 0).then(resume(0))
    .expect(resumed and jobs.get(0).remaining == 2).then(advance(0)).then(finish(0)).expect(safety)
  run duplicateAdmitBlockedTest = init.then(admit(0)).then(admit(0).fail())
  run duplicateFinishBlockedTest = init.then(admit(0)).then(advance(0)).then(advance(0))
    .then(finish(0)).then(finish(0).fail())
  run allCompleteTest = init.then(4.reps(id => admit(id).then(advance(id))
    .then(advance(id)).then(finish(id)))).expect(allDone and safety)
}
