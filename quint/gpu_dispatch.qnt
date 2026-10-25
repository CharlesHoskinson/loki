// Contract model: GPU arithmetic equality is a backend obligation, not a symbolic proof.
module gpu_dispatch {
  type Phase = Idle | Queued | Gpu | Fallback | Done
  type Op = Msm | Ntt
  type Cache = { valid: bool, ptr: int, len: int, content: int }
  type Request = { phase: Phase, op: Op, ptr: int, len: int, content: int,
    resident: int, residentPtr: int, residentLen: int, result: int, failed: bool, hit: bool, rejectedStale: bool }
  pure val PROVERS = Set(0, 1)
  pure val THRESHOLD = 4
  var requests: int -> Request
  var cache: Cache
  var memory: int -> int

  pure def cpu(op: Op, len: int, content: int): int =
    if (op == Msm) len * content else len + content
  pure def cacheMatches(c: Cache, r: Request): bool =
    c.valid and c.ptr == r.ptr and c.len == r.len and c.content == r.content
  pure val emptyRequest: Request = { phase: Idle, op: Msm, ptr: 0, len: 0,
    content: 0, resident: -1, residentPtr: -1, residentLen: 0, result: -1, failed: false, hit: false, rejectedStale: false }
  action init = all {
    requests' = PROVERS.mapBy(_ => emptyRequest),
    cache' = { valid: false, ptr: 0, len: 0, content: 0 },
    memory' = Set(0, 1).mapBy(_ => 1),
  }
  action submit(p: int, op: Op, ptr: int, len: int): bool = all {
    requests.get(p).phase == Idle,
    requests' = requests.set(p, { ...emptyRequest, phase: Queued, op: op,
      ptr: ptr, len: len, content: memory.get(ptr) }),
    cache' = cache, memory' = memory,
  }
  action dispatch(p: int): bool = {
    val r = requests.get(p)
    all {
      r.phase == Queued,
      requests' = requests.set(p, { ...r,
        phase: if (r.len < THRESHOLD) Fallback else Gpu,
        resident: if (r.len < THRESHOLD) -1 else
          (if (cacheMatches(cache, r)) cache.content else r.content),
        residentPtr: if (cacheMatches(cache, r)) cache.ptr else r.ptr,
        residentLen: if (cacheMatches(cache, r)) cache.len else r.len,
        hit: r.len >= THRESHOLD and cacheMatches(cache, r),
        rejectedStale: r.len >= THRESHOLD and cache.valid and cache.ptr == r.ptr and
          cache.len == r.len and cache.content != r.content }),
      // A miss uploads a new immutable handle. In-flight handles survive eviction.
      cache' = if (r.len < THRESHOLD) cache else
        { valid: true, ptr: r.ptr, len: r.len, content: r.content },
      memory' = memory,
    }
  }
  action gpuError(p: int): bool = all {
    requests.get(p).phase == Gpu,
    requests' = requests.set(p, { ...requests.get(p), phase: Fallback, failed: true }),
    cache' = cache, memory' = memory,
  }
  action gpuSuccess(p: int): bool = {
    val r = requests.get(p)
    all {
      r.phase == Gpu,
      requests' = requests.set(p, { ...r, phase: Done,
        result: cpu(r.op, r.residentLen, r.resident) }),
      cache' = cache, memory' = memory,
    }
  }
  action cpuFinish(p: int): bool = {
    val r = requests.get(p)
    all {
      r.phase == Fallback,
      requests' = requests.set(p, { ...r, phase: Done,
        result: cpu(r.op, r.len, r.content) }),
      cache' = cache, memory' = memory,
    }
  }
  // Also covers initialization/upload failure before a usable GPU handle exists.
  action uploadError(p: int): bool = all {
    requests.get(p).phase == Queued, requests.get(p).len >= THRESHOLD,
    requests' = requests.set(p, { ...requests.get(p), phase: Fallback, failed: true }),
    cache' = cache, memory' = memory,
  }
  action recycle(p: int): bool = all {
    requests.get(p).phase == Done,
    requests' = requests.set(p, emptyRequest), cache' = cache, memory' = memory,
  }
  action reusePointer(ptr: int, content: int): bool = all {
    memory.get(ptr) != content,
    memory' = memory.set(ptr, content), cache' = cache, requests' = requests,
  }
  action step = {
    nondet p = PROVERS.oneOf()
    nondet op = Set(Msm, Ntt).oneOf()
    nondet ptr = Set(0, 1).oneOf()
    nondet len = Set(0, 3, 4, 5).oneOf()
    nondet content = 1.to(3).oneOf()
    any { submit(p, op, ptr, len), dispatch(p), gpuError(p), gpuSuccess(p),
      cpuFinish(p), uploadError(p), recycle(p), reusePointer(ptr, content) }
  }
  // Specialization for portable replay of the real W4 host cache with CPU FFI.
  action msmStep = {
    nondet p = PROVERS.oneOf()
    nondet op = Set(Msm).oneOf()
    nondet ptr = Set(0, 1).oneOf()
    nondet len = Set(0, 3, 4, 5).oneOf()
    nondet content = 1.to(3).oneOf()
    any { submit(p, op, ptr, len), dispatch(p), gpuError(p), gpuSuccess(p),
      cpuFinish(p), uploadError(p), recycle(p), reusePointer(ptr, content) }
  }
  val resultEqualsCpu = PROVERS.forall(p => {
    val r = requests.get(p)
    r.phase == Done implies r.result == cpu(r.op, r.len, r.content)
  })
  val staleNeverUsed = PROVERS.forall(p => {
    val r = requests.get(p)
    (r.phase == Gpu or (r.phase == Done and r.resident >= 0 and not(r.failed)))
      implies r.resident == r.content and r.residentPtr == r.ptr and r.residentLen == r.len
  })
  val fallbackTotal = PROVERS.forall(p => {
    val r = requests.get(p)
    r.phase == Fallback implies r.len >= 0 and r.content >= 1
  })
  val safety = resultEqualsCpu and staleNeverUsed and fallbackTotal
  val completed = PROVERS.exists(p => requests.get(p).phase == Done)
  val fellBack = PROVERS.exists(p => requests.get(p).phase == Done and requests.get(p).failed)
  val cacheHit = PROVERS.exists(p => requests.get(p).hit)
  val concurrent = PROVERS.forall(p => requests.get(p).phase == Gpu)
  val staleRejected = PROVERS.exists(p => requests.get(p).rejectedStale and
    requests.get(p).phase == Done)
}
