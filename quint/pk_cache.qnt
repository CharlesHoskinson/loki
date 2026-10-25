// Successful single-flight fills, with full key identity. See report for W6 disk scope.
module pk_cache {
  type Key = { circuit: int, k: int, params: int }
  type EntryPhase = Empty | Filling | Ready
  type JobPhase = Idle | Waiting | Used
  type Entry = { phase: EntryPhase, owner: int, builds: int, binding: Key }
  type Job = { phase: JobPhase, requested: Key, used: Key }
  pure val JOBS = Set(0, 1, 2)
  pure val KEYS = Set(0, 1).map(c => Set(0, 1).map(k =>
    Set(0, 1).map(p => { circuit: c, k: k, params: p }))).flatten().flatten()
  pure val ZERO: Key = { circuit: 0, k: 0, params: 0 }
  var entries: Key -> Entry
  var jobs: int -> Job
  action init = all {
    entries' = KEYS.mapBy(key => { phase: Empty, owner: -1, builds: 0, binding: key }),
    jobs' = JOBS.mapBy(_ => { phase: Idle, requested: ZERO, used: ZERO }),
  }
  // Lookup and fill claim share one lock/CAS linearization point.
  action request(job: int, key: Key): bool = {
    val e = entries.get(key)
    all {
      jobs.get(job).phase == Idle,
      jobs' = jobs.set(job, { phase: Waiting, requested: key, used: ZERO }),
      entries' = if (e.phase == Empty)
        entries.set(key, { ...e, phase: Filling, owner: job }) else entries,
    }
  }
  // Keygen runs outside publication. Other callers see Filling, never a partial PK.
  action build(job: int): bool = {
    val key = jobs.get(job).requested
    val e = entries.get(key)
    all {
      jobs.get(job).phase == Waiting, e.phase == Filling,
      e.owner == job, e.builds == 0,
      entries' = entries.set(key, { ...e, builds: e.builds + 1, binding: key }), jobs' = jobs,
    }
  }
  action publish(job: int): bool = {
    val key = jobs.get(job).requested
    val e = entries.get(key)
    all {
      e.phase == Filling, e.owner == job, e.builds == 1,
      entries' = entries.set(key, { ...e, phase: Ready }), jobs' = jobs,
    }
  }
  action useKey(job: int): bool = {
    val j = jobs.get(job)
    val e = entries.get(j.requested)
    all {
      j.phase == Waiting, e.phase == Ready,
      jobs' = jobs.set(job, { ...j, phase: Used, used: e.binding }), entries' = entries,
    }
  }
  action recycle(job: int): bool = all {
    jobs.get(job).phase == Used,
    jobs' = jobs.set(job, { ...jobs.get(job), phase: Idle }), entries' = entries,
  }
  action step = {
    nondet job = JOBS.oneOf()
    nondet key = KEYS.oneOf()
    any { request(job, key), build(job), publish(job), useKey(job), recycle(job) }
  }
  val bindingCorrect = JOBS.forall(job => {
    val j = jobs.get(job)
    j.phase == Used implies j.used == j.requested
  })
  val keygenAtMostOnce = KEYS.forall(key => entries.get(key).builds <= 1)
  val noPartialUse = JOBS.forall(job => {
    val j = jobs.get(job)
    j.phase == Used implies entries.get(j.requested).phase == Ready and
      entries.get(j.requested).builds == 1
  })
  val safety = bindingCorrect and keygenAtMostOnce and noPartialUse
  val used = JOBS.exists(job => jobs.get(job).phase == Used)
  val filled = KEYS.exists(key => entries.get(key).phase == Ready)
  val concurrentWaiters = JOBS.exists(a => JOBS.exists(b => a != b and
    jobs.get(a).phase == Waiting and jobs.get(b).phase == Waiting and
    jobs.get(a).requested == jobs.get(b).requested))
  val differentKeys = JOBS.exists(a => JOBS.exists(b => a != b and
    jobs.get(a).phase == Used and jobs.get(b).phase == Used and
    jobs.get(a).requested != jobs.get(b).requested))
  val sharedUse = JOBS.exists(a => JOBS.exists(b => a != b and
    jobs.get(a).phase == Used and jobs.get(b).phase == Used and
    jobs.get(a).requested == jobs.get(b).requested))
}
