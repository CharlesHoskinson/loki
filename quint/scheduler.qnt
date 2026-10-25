// LOKI-DESIGN L5 proposal. Units are reservations, not measured GiB.
module scheduler {
  type Phase = Pending | Running | Checkpointed | Done
  type Job = { phase: Phase, heavy: bool, remaining: int, saved: int,
    checkpoints: int, admissions: int, finishes: int }
  pure val JOBS = Set(0, 1, 2, 3)
  pure val BUDGET = 10
  pure val MAX_CHECKPOINTS = 1
  var jobs: int -> Job
  pure def cost(j: Job): int = if (j.heavy) 7 else 3
  pure def used(js: int -> Job): int = JOBS.fold(0, (total, id) =>
    total + (if (js.get(id).phase == Running) cost(js.get(id)) else 0))
  pure def canAdmit(js: int -> Job, id: int): bool = {
    val j = js.get(id)
    (j.phase == Pending or j.phase == Checkpointed) and
      used(js) + cost(j) <= BUDGET and
      JOBS.forall(other => js.get(other).phase != Running or js.get(other).heavy != j.heavy)
  }
  action init = jobs' = JOBS.mapBy(id => { phase: Pending, heavy: id < 2,
    remaining: 3, saved: 3, checkpoints: 0, admissions: 0, finishes: 0 })
  action admit(id: int): bool = all {
    canAdmit(jobs, id), jobs.get(id).phase == Pending,
    jobs' = jobs.set(id, { ...jobs.get(id), phase: Running, admissions: 1 }),
  }
  action checkpoint(id: int): bool = {
    val j = jobs.get(id)
    all {
      j.phase == Running, j.checkpoints < MAX_CHECKPOINTS,
      jobs' = jobs.set(id, { ...j, phase: Checkpointed,
        saved: j.remaining, checkpoints: j.checkpoints + 1 }),
    }
  }
  action resume(id: int): bool = all {
    canAdmit(jobs, id), jobs.get(id).phase == Checkpointed,
    jobs' = jobs.set(id, { ...jobs.get(id), phase: Running,
      remaining: jobs.get(id).saved }),
  }
  action advance(id: int): bool = {
    val j = jobs.get(id)
    all {
      j.phase == Running, j.remaining > 1,
      jobs' = jobs.set(id, { ...j, remaining: j.remaining - 1 }),
    }
  }
  action finish(id: int): bool = {
    val j = jobs.get(id)
    all {
      j.phase == Running, j.remaining == 1,
      jobs' = jobs.set(id, { ...j, phase: Done, remaining: 0, finishes: j.finishes + 1 }),
    }
  }
  action service(id: int): bool = any { admit(id), resume(id), advance(id), finish(id) }
  action finished = all { JOBS.forall(id => jobs.get(id).phase == Done), jobs' = jobs }
  action step = {
    nondet id = JOBS.oneOf()
    any { service(id), checkpoint(id), finished }
  }
  val vramBudget = used(jobs) <= BUDGET
  val slotLimit = Set(false, true).forall(heavy =>
    JOBS.filter(id => jobs.get(id).phase == Running and jobs.get(id).heavy == heavy).size() <= 1)
  val noLostOrDuplicated = JOBS.forall(id => {
    val j = jobs.get(id)
    and { j.admissions <= 1, j.finishes <= 1,
      (j.phase == Pending iff j.admissions == 0),
      (j.phase == Done iff j.finishes == 1),
      j.phase != Done implies j.remaining >= 1 }
  })
  val checkpointIntegrity = JOBS.forall(id => {
    val j = jobs.get(id)
    j.phase == Checkpointed implies j.remaining == j.saved and j.admissions == 1
  })
  val safety = vramBudget and slotLimit and noLostOrDuplicated and checkpointIntegrity
  val allDone = JOBS.forall(id => jobs.get(id).phase == Done)
  val overlap = JOBS.filter(id => jobs.get(id).phase == Running).size() == 2
  val checkpointed = JOBS.exists(id => jobs.get(id).phase == Checkpointed)
  val resumed = JOBS.exists(id => jobs.get(id).phase == Running and jobs.get(id).checkpoints > 0)
  val completed = JOBS.exists(id => jobs.get(id).phase == Done)
  // Finite jobs, finite work, bounded preemption. Weak fair service is explicit;
  // arbitrary infinite preemption or crashes would make this false.
  temporal admittedEventuallyFinish =
    JOBS.forall(id => service(id).weakFair(Set(jobs))) implies
      JOBS.forall(id => always(jobs.get(id).admissions == 1 implies eventually(jobs.get(id).phase == Done)))
}
