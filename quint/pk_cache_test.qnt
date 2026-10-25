module pk_cache_test {
  import pk_cache.* from "./pk_cache"
  run sharedFillTest = init.then(request(0, ZERO)).then(request(1, ZERO))
    .expect(concurrentWaiters).then(build(0)).then(publish(0))
    .then(useKey(0)).then(useKey(1)).expect(sharedUse and safety)
  run noEarlyUseTest = init.then(request(0, ZERO)).then(useKey(0).fail())
  run noWaiterBuildTest = init.then(request(0, ZERO)).then(request(1, ZERO))
    .then(build(1).fail())
  run noDuplicateBuildTest = init.then(request(0, ZERO)).then(build(0))
    .then(build(0).fail())
  run changedCircuitTest = init.then(request(0, ZERO)).then(build(0)).then(publish(0))
    .then(useKey(0)).then(request(1, { ...ZERO, circuit: 1 })).then(build(1))
    .then(publish(1)).then(useKey(1)).expect(differentKeys and safety)
  run changedParamsTest = init.then(request(0, ZERO)).then(build(0)).then(publish(0))
    .then(request(1, { ...ZERO, params: 1 })).then(useKey(1).fail())
  run changedDegreeTest = init.then(request(0, ZERO)).then(build(0)).then(publish(0))
    .then(request(1, { ...ZERO, k: 1 })).then(useKey(1).fail())
}
