// Deliberately unsafe alternatives. Each campaign MUST find a counterexample.
module mutation_transcript {
  import transcript.* from "./transcript"
  action earlySqueeze = {
    nondet index = state.plan.indices().filter(i => i > state.cursor and
      state.plan.nth(i).kind == "squeeze").oneOf()
    state' = { ...state, log: state.log.append(state.plan.nth(index)), cursor: state.cursor + 1 }
  }
  action mutantStep = any { step, earlySqueeze }
}
module mutation_gpu {
  import gpu_dispatch.* from "./gpu_dispatch"
  action staleHit = {
    nondet p = PROVERS.oneOf()
    val r = requests.get(p)
    all {
      r.phase == Queued, cache.valid, r.len >= THRESHOLD,
      cache.ptr == r.ptr, cache.len == r.len, cache.content != r.content,
      requests' = requests.set(p, { ...r, phase: Gpu, resident: cache.content,
        residentPtr: cache.ptr, residentLen: cache.len, hit: true }),
      cache' = cache, memory' = memory,
    }
  }
  action mutantStep = any { step, staleHit }
}
module mutation_pk_build {
  import pk_cache.* from "./pk_cache"
  action duplicateBuild = {
    nondet job = JOBS.oneOf()
    val key = jobs.get(job).requested
    val e = entries.get(key)
    all {
      jobs.get(job).phase == Waiting, e.phase == Filling, e.owner == job,
      entries' = entries.set(key, { ...e, builds: e.builds + 1 }), jobs' = jobs,
    }
  }
  action mutantStep = any { step, duplicateBuild }
}
module mutation_pk_partial {
  import pk_cache.* from "./pk_cache"
  action prematureUse = {
    nondet job = JOBS.oneOf()
    val j = jobs.get(job)
    all {
      j.phase == Waiting,
      jobs' = jobs.set(job, { ...j, phase: Used, used: entries.get(j.requested).binding }),
      entries' = entries,
    }
  }
  action mutantStep = any { step, prematureUse }
}
module mutation_scheduler {
  import scheduler.* from "./scheduler"
  action overAdmit = {
    nondet id = JOBS.oneOf()
    all {
      jobs.get(id).phase == Pending,
      jobs' = jobs.set(id, { ...jobs.get(id), phase: Running, admissions: 1 }),
    }
  }
  action mutantStep = any { step, overAdmit }
}
