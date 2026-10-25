// quint-q2: P1 accelerator dispatch and CPU fallback (src/backend/dispatch.rs @ a256a9a).
//
// The model mirrors the real Dispatcher, including its defects, so that the
// Rust driver in quint/q2/mbt can replay traces against the real code. The
// *contract* properties (from loki-backend-api @ loki-gpu cefecbc and the
// dispatch.rs doc comments) are invariants; the ones the real code violates
// are reported as findings in quint/q2/REPORT.md with their counterexamples.
//
// Abstraction: values are integers. oracle(op, input) stands for the stock CPU
// result (msm_cpu / best_fft over the request's effective prefix). A
// contract-respecting accelerator Ok returns the oracle; a wrong-length
// transform output is the distinguished value WRONG.
//
// Source correspondence (file:line at a256a9a):
//   admit              Dispatcher::msm                 dispatch.rs:130-159 (gate 143-146)
//                      Dispatcher::transform           dispatch.rs:165-183 (validate 175)
//                      try_accelerated_transform gate  dispatch.rs:187-207 (gate 200-205)
//                      cpu::validate_transform         cpu.rs:83-
//   enter              backend.msm / backend.transform dispatch.rs:148, 209
//   accelOk            Ok arms                         dispatch.rs:149-152, 210-213
//   accelWrongLen      Ok(_) wrong length arm          dispatch.rs:214-219
//   accelErr           Err arms -> failed()            dispatch.rs:153, 220-223; failed 115-127
//   poison             Dispatcher::poison              dispatch.rs:107-111
//   cpu fallback       bump msm_cpu + cpu::msm_request dispatch.rs:157-158; cpu.rs:42-51
//                      bump transform_cpu + in-place   dispatch.rs:181-182
//   coset (OCoset)     accelerated_coset               dispatch.rs:333-350
//                      EvaluationDomain::coeff_to_extended stock body after None
//                                                      domain.rs:277-302 -> fft 412-422
//                                                      -> dispatch::fft 306 / run_transform 272-288
module q2_dispatch {
  // ------------------------------------------------------------- types
  type Op = OMsm | OTransform | OCoset
  type Kind = Unsupported | InvalidInput | Capacity | Allocation | Transfer | Kernel
    | Synchronization | ContextLost | Poisoned | Internal | Cancelled
  type Effect = NotStarted | InputsPreserved | SessionPoisoned
  // Admitted: gate passed, backend not yet entered (the check-to-call window).
  // InAccel: inside the backend call. Inner: coset fell back to the stock body
  // and its inner fft is now a fresh P1 transform request.
  type Phase = Idle | Admitted | InAccel | Done
  type Src = SNone | SCpu | SAccel | SReject | SCancel

  type Req = {
    phase: Phase, op: Op, pairs: int, abiOk: bool, capOk: bool, sizeOk: bool,
    valid: bool, input: int, inner: bool,
    result: int, source: Src,
    entered: bool,          // some backend call was entered for this logical op
    lastAccelFailed: bool,  // the most recent backend call of this op failed
    cancelled: bool,        // a backend call of this op returned Cancelled
  }
  type Counters = {
    msmCpu: int, msmAcc: int, msmFb: int,
    trCpu: int, trAcc: int, trFb: int, poisoned: int,
  }

  pure val THREADS = Set(0, 1)
  pure val WRONG = -99
  pure val KINDS = Set(Unsupported, InvalidInput, Capacity, Allocation, Transfer, Kernel,
    Synchronization, ContextLost, Poisoned, Internal, Cancelled)
  pure val EFFECTS = Set(NotStarted, InputsPreserved, SessionPoisoned)

  pure def opCode(op: Op): int = match op { | OMsm => 1 | OTransform => 2 | OCoset => 3 }
  pure def oracle(op: Op, input: int): int = 10 * input + opCode(op)
  // Empty MSM prefix: the stock identity (msm_cpu after zero filtering).
  pure val IDENTITY = 0
  pure def cpuValue(r: Req): int =
    if (r.op == OMsm and r.pairs == 0) IDENTITY else oracle(r.op, r.input)

  pure val emptyReq: Req = {
    phase: Idle, op: OMsm, pairs: 0, abiOk: false, capOk: false, sizeOk: false,
    valid: true, input: 0, inner: false, result: 0, source: SNone,
    entered: false, lastAccelFailed: false, cancelled: false,
  }
  pure val zeroCounters: Counters = {
    msmCpu: 0, msmAcc: 0, msmFb: 0, trCpu: 0, trAcc: 0, trFb: 0, poisoned: 0,
  }

  // ------------------------------------------------------------- state
  var reqs: int -> Req
  var disabled: bool
  var ctr: Counters
  // ghosts (never read by the modelled code)
  var msmDone: int           // completed Dispatcher::msm calls
  var trDone: int            // completed P1 transform requests (transform + inner fft)
  var entryWhileDisabled: bool
  var accPublishWhileDisabled: bool
  var cancelledRetried: bool
  var poisonCauseSeen: bool  // wrong length / ContextLost / Poisoned / SessionPoisoned

  // ------------------------------------------------------------- mutation switches
  // Each is false in the faithful model; mutations.qnt-style witnesses flip one.
  pure val M_NO_EMPTY_GUARD = false
  pure val M_NO_DISABLED_CHECK = false
  pure val M_NO_ABI_CHECK = false
  pure val M_PUBLISH_WRONG_LEN = false
  pure val M_NO_POISON_WRONG_LEN = false
  pure val M_NO_CPU_BUMP_ON_FALLBACK = false
  pure val M_NO_VALIDATE = false
  pure val M_POISON_COUNT_EVERY = false
  pure val M_NO_TR_CPU_BUMP = false
  // Fix switches (not the real code): show the violated contracts are satisfiable.
  // M_FIX_CANCEL: Cancelled is propagated to the caller, never recomputed.
  // M_FIX_RECHECK: `disabled` is re-checked at entry and before publication.
  pure val M_FIX_CANCEL = false
  pure val M_FIX_RECHECK = false
  // M_FIX_COSET_ACCOUNTING: the coset hook's failed offer is not a *_fallback
  // (its stock body is accounted by the inner P1 transform).
  pure val M_FIX_COSET_ACCOUNTING = false

  // ------------------------------------------------------------- gates
  // dispatch.rs:143-146 (msm) and 200-205 (transform). Transform requests
  // reaching the gate are already validated (dispatch.rs:175, 341).
  def admittedNow(r: Req): bool =
    (M_NO_DISABLED_CHECK or not(disabled))
    and r.capOk and (M_NO_ABI_CHECK or r.abiOk)
    and (if (r.op == OMsm) (M_NO_EMPTY_GUARD or r.pairs > 0) else r.sizeOk)

  def isMsm(r: Req): bool = r.op == OMsm

  // CPU completion of the current dispatcher request. For a coset whose outer
  // offer was not taken or failed, the stock body runs fft -> P1 transform.
  action finishCpu(t: int, r: Req, c: Counters): bool = all {
    reqs' = reqs.set(t, { ...r, phase: Done, result: cpuValue(r), source: SCpu }),
    ctr' = if (isMsm(r)) { ...c, msmCpu: c.msmCpu + 1 }
      else if (M_NO_TR_CPU_BUMP) c else { ...c, trCpu: c.trCpu + 1 },
    msmDone' = if (isMsm(r)) msmDone + 1 else msmDone,
    trDone' = if (isMsm(r)) trDone else trDone + 1,
  }

  // Coset outer offer not taken / failed: the stock body runs its inner fft as
  // a new P1 transform, which is gated again against the *current* state.
  action toInnerOrCpu(t: int, r: Req, c: Counters, d: bool): bool = {
    val inner = { ...r, inner: true }
    val again = (M_NO_DISABLED_CHECK or not(d)) and r.capOk and r.sizeOk and r.abiOk
    if (again) all {
      reqs' = reqs.set(t, { ...inner, phase: Admitted }),
      ctr' = c, msmDone' = msmDone, trDone' = trDone,
    } else finishCpu(t, inner, c)
  }

  // ------------------------------------------------------------- actions
  action init = all {
    reqs' = THREADS.mapBy(_ => emptyReq),
    disabled' = false,
    ctr' = zeroCounters,
    msmDone' = 0, trDone' = 0,
    entryWhileDisabled' = false, accPublishWhileDisabled' = false,
    cancelledRetried' = false, poisonCauseSeen' = false,
  }

  // A caller issues a request; the gate is evaluated now (split model).
  action admit(t: int, op: Op, pairs: int, abiOk: bool, capOk: bool, sizeOk: bool,
               valid: bool, input: int): bool = {
    val r = { ...emptyReq, op: op, pairs: if (op == OMsm) pairs else 0,
      abiOk: abiOk, capOk: capOk, sizeOk: sizeOk,
      valid: if (op == OMsm) true else valid, input: input }
    all {
      reqs.get(t).phase == Idle,
      if (not(r.valid) and not(M_NO_VALIDATE)) all {
        // validate_transform error: values unchanged, no backend, no counters.
        // OCoset with an invalid spec is unreachable (domain specs are valid),
        // and accelerated_coset would just return None (dispatch.rs:341).
        reqs' = reqs.set(t, { ...r, phase: Done, source: SReject, result: 0 }),
        ctr' = ctr, msmDone' = msmDone, trDone' = trDone,
      } else if (admittedNow(r)) all {
        reqs' = reqs.set(t, { ...r, phase: Admitted }),
        ctr' = ctr, msmDone' = msmDone, trDone' = trDone,
      } else if (op == OCoset) {
        // accelerated_coset: try_accelerated_transform returned None silently.
        toInnerOrCpu(t, r, ctr, disabled)
      } else finishCpu(t, r, ctr),
      disabled' = disabled,
      entryWhileDisabled' = entryWhileDisabled, accPublishWhileDisabled' = accPublishWhileDisabled,
      cancelledRetried' = cancelledRetried, poisonCauseSeen' = poisonCauseSeen,
    }
  }

  // The admitted request enters the backend (dispatch.rs:148 / 209). Nothing
  // re-checks `disabled` between the gate and this call.
  action enter(t: int): bool = {
    val r = reqs.get(t)
    all {
      r.phase == Admitted,
      if (M_FIX_RECHECK and disabled) all {
        // fix: gate re-checked at entry; compute on CPU instead.
        finishCpu(t, r, ctr), entryWhileDisabled' = entryWhileDisabled,
      } else all {
        reqs' = reqs.set(t, { ...r, phase: InAccel, entered: true, lastAccelFailed: false }),
        entryWhileDisabled' = (entryWhileDisabled or disabled),
        ctr' = ctr, msmDone' = msmDone, trDone' = trDone,
      },
      disabled' = disabled,
      accPublishWhileDisabled' = accPublishWhileDisabled,
      cancelledRetried' = cancelledRetried, poisonCauseSeen' = poisonCauseSeen,
    }
  }

  action accelOk(t: int): bool = {
    val r = reqs.get(t)
    all {
      r.phase == InAccel,
      if (M_FIX_RECHECK and disabled) all {
        // fix: a result completing after a concurrent poison is discarded.
        finishCpu(t, { ...r, lastAccelFailed: true }, ctr),
        accPublishWhileDisabled' = accPublishWhileDisabled,
      } else all {
        reqs' = reqs.set(t, { ...r, phase: Done, result: oracle(r.op, r.input), source: SAccel }),
        ctr' = if (isMsm(r)) { ...ctr, msmAcc: ctr.msmAcc + 1 } else { ...ctr, trAcc: ctr.trAcc + 1 },
        msmDone' = if (isMsm(r)) msmDone + 1 else msmDone,
        // accelerated_coset's outer success is not a P1 `transform` call; the
        // inner fft (inner = true) and plain transforms are.
        trDone' = if (isMsm(r) or (r.op == OCoset and not(r.inner))) trDone else trDone + 1,
        accPublishWhileDisabled' = (accPublishWhileDisabled or disabled),
      },
      disabled' = disabled, entryWhileDisabled' = entryWhileDisabled,
      cancelledRetried' = cancelledRetried, poisonCauseSeen' = poisonCauseSeen,
    }
  }

  // Transform only: Ok(output) with output.len() != output_len.
  action accelWrongLen(t: int): bool = {
    val r = reqs.get(t)
    val outerCoset = r.op == OCoset and not(r.inner)
    val c1 = if (M_FIX_COSET_ACCOUNTING and outerCoset) ctr else { ...ctr, trFb: ctr.trFb + 1 }
    val poisonNow = not(M_NO_POISON_WRONG_LEN)
    val d1 = disabled or poisonNow
    val c2 = if (poisonNow and (M_POISON_COUNT_EVERY or not(disabled))) { ...c1, poisoned: c1.poisoned + 1 } else c1
    val failedReq = { ...r, lastAccelFailed: true }
    all {
      r.phase == InAccel, r.op != OMsm,
      disabled' = d1,
      poisonCauseSeen' = true,
      if (M_PUBLISH_WRONG_LEN) all {
        reqs' = reqs.set(t, { ...failedReq, phase: Done, result: WRONG, source: SAccel }),
        ctr' = c2, msmDone' = msmDone, trDone' = trDone + 1,
      } else if (r.op == OCoset and not(r.inner)) toInnerOrCpu(t, failedReq, c2, d1)
      else finishCpu(t, failedReq, c2),
      entryWhileDisabled' = entryWhileDisabled, accPublishWhileDisabled' = accPublishWhileDisabled,
      cancelledRetried' = cancelledRetried,
    }
  }

  // Backend error -> Dispatcher::failed: bump *_fallback, poison on
  // SessionPoisoned / ContextLost / Poisoned, then recompute on CPU.
  action accelErr(t: int, kind: Kind, effect: Effect): bool = {
    val r = reqs.get(t)
    val poisonNow = effect == SessionPoisoned or kind == ContextLost or kind == Poisoned
    val outerCoset = r.op == OCoset and not(r.inner)
    val c1 = if (isMsm(r)) { ...ctr, msmFb: ctr.msmFb + 1 }
      else if (M_FIX_COSET_ACCOUNTING and outerCoset) ctr else { ...ctr, trFb: ctr.trFb + 1 }
    val c2 = if (poisonNow and (M_POISON_COUNT_EVERY or not(disabled))) { ...c1, poisoned: c1.poisoned + 1 } else c1
    val d1 = disabled or poisonNow
    val failedReq = { ...r, lastAccelFailed: true, cancelled: r.cancelled or kind == Cancelled }
    // API: Cancelled is reported only with NotStarted or InputsPreserved.
    val wellFormed = kind != Cancelled or effect != SessionPoisoned
    // Mutation-only knob: M_NO_CPU_BUMP_ON_FALLBACK drops the cpu bump.
    val c3 = if (M_NO_CPU_BUMP_ON_FALLBACK and isMsm(r)) { ...c2, msmCpu: c2.msmCpu - 1 } else c2
    all {
      r.phase == InAccel, wellFormed,
      disabled' = d1,
      poisonCauseSeen' = (poisonCauseSeen or poisonNow),
      // The real code always recomputes; a Cancelled request retried on CPU
      // violates the cefecbc contract.
      if (M_FIX_CANCEL and kind == Cancelled) all {
        // fix: no fallback accounting, no CPU retry; caller sees Cancelled.
        reqs' = reqs.set(t, { ...failedReq, phase: Done, source: SCancel, result: 0 }),
        ctr' = ctr, msmDone' = msmDone, trDone' = trDone,
        cancelledRetried' = cancelledRetried,
      } else all {
        cancelledRetried' = (cancelledRetried or kind == Cancelled),
        if (r.op == OCoset and not(r.inner)) toInnerOrCpu(t, failedReq, c3, d1)
        else finishCpu(t, failedReq, c3),
      },
      entryWhileDisabled' = entryWhileDisabled, accPublishWhileDisabled' = accPublishWhileDisabled,
    }
  }

  action recycle(t: int): bool = all {
    reqs.get(t).phase == Done,
    reqs' = reqs.set(t, emptyReq),
    disabled' = disabled, ctr' = ctr, msmDone' = msmDone, trDone' = trDone,
    entryWhileDisabled' = entryWhileDisabled, accPublishWhileDisabled' = accPublishWhileDisabled,
    cancelledRetried' = cancelledRetried, poisonCauseSeen' = poisonCauseSeen,
  }

  // Full model: split gate/entry, all ops including the domain coset path.
  action step = {
    nondet t = THREADS.oneOf()
    nondet op = Set(OMsm, OTransform, OCoset).oneOf()
    nondet pairs = Set(0, 1, 2).oneOf()
    nondet abiOk = Bool.oneOf()
    nondet capOk = Bool.oneOf()
    nondet sizeOk = Bool.oneOf()
    nondet valid = Bool.oneOf()
    nondet input = 1.to(3).oneOf()
    nondet kind = KINDS.oneOf()
    nondet effect = EFFECTS.oneOf()
    any {
      admit(t, op, pairs, abiOk, capOk, sizeOk, valid, input),
      enter(t), accelOk(t), accelWrongLen(t), accelErr(t, kind, effect), recycle(t),
    }
  }

  // MBT step: only Dispatcher::msm / Dispatcher::transform (public API).
  // `admit` here is immediately followed by `enter` in the driver; the
  // driver collapses Admitted and InAccel into one observable "busy" phase.
  action mbtStep = {
    nondet t = THREADS.oneOf()
    nondet op = Set(OMsm, OTransform).oneOf()
    nondet pairs = Set(0, 1, 2).oneOf()
    nondet abiOk = Bool.oneOf()
    nondet capOk = Bool.oneOf()
    nondet sizeOk = Bool.oneOf()
    nondet valid = Bool.oneOf()
    nondet input = 1.to(3).oneOf()
    nondet kind = KINDS.oneOf()
    nondet effect = EFFECTS.oneOf()
    any {
      admit(t, op, pairs, abiOk, capOk, sizeOk, valid, input),
      enter(t), accelOk(t), accelWrongLen(t), accelErr(t, kind, effect), recycle(t),
    }
  }

  // ------------------------------------------------------------- invariants
  // P1: every published result equals the stock CPU oracle.
  val resultEqualsOracle = THREADS.forall(t => {
    val r = reqs.get(t)
    (r.phase == Done and r.source != SReject and r.source != SCancel) implies r.result == cpuValue(r)
  })
  // P2: the output of a failing backend call is never published: if the last
  // backend call of an op failed, its result came from CPU.
  val noPublishAfterError = THREADS.forall(t => {
    val r = reqs.get(t)
    (r.phase == Done and r.lastAccelFailed) implies r.source != SAccel
  })
  // P3a: no backend entry while the dispatcher is sticky-disabled.
  val noAccelEntryAfterDisable = not(entryWhileDisabled)
  // P3b: no accelerator result is published (msm_accelerated/transform_accelerated
  // bumped) once the dispatcher is sticky-disabled.
  val noAccelPublishAfterDisable = not(accPublishWhileDisabled)
  // P3c: a poisoning cause always leaves the accelerator disabled.
  val poisonIsSticky = poisonCauseSeen implies disabled
  // P4: cancelled backend work is never retried on CPU (cefecbc contract).
  val noCancelledRetry = not(cancelledRetried)
  // P5: counters.
  val msmCountersConsistent = ctr.msmAcc + ctr.msmCpu == msmDone and ctr.msmFb <= ctr.msmCpu
  val poisonCounterExact = ctr.poisoned == (if (disabled) 1 else 0)
  // dispatch.rs:42-43 (DispatchCounters doc): every transform fallback was recomputed on CPU.
  val transformFallbackRecomputedOnCpu = ctr.trFb <= ctr.trCpu
  // Each P1 transform call bumps exactly one of cpu / accelerated, ignoring
  // the coset hook's own bumps.
  val transformPerCallAccounting = ctr.trCpu + ctr.trAcc >= trDone
  // P6: empty MSM never reaches a backend and returns the identity.
  val emptyMsmIsIdentityOnCpu = THREADS.forall(t => {
    val r = reqs.get(t)
    (r.op == OMsm and r.pairs == 0 and r.phase != Idle) implies
      (not(r.entered) and r.phase != Admitted and (r.phase == Done implies r.result == IDENTITY))
  })
  // P7: invalid requests are rejected before any backend and touch no counter.
  val invalidRejectedBeforeBackend = THREADS.forall(t => {
    val r = reqs.get(t)
    (r.phase != Idle and not(r.valid)) implies (r.source == SReject and not(r.entered))
  })
  // P8: a backend is entered only with matching capability and exact ABI.
  val backendOnlyWithCapabilityAndAbi = THREADS.forall(t => {
    val r = reqs.get(t)
    r.entered implies (r.capOk and r.abiOk and (r.op == OMsm or r.sizeOk))
  })

  // Properties the real code satisfies (expected to hold).
  val holds = resultEqualsOracle and noPublishAfterError and poisonIsSticky
    and msmCountersConsistent and poisonCounterExact and transformPerCallAccounting
    and emptyMsmIsIdentityOnCpu and invalidRejectedBeforeBackend
    and backendOnlyWithCapabilityAndAbi
  // Sequential-only (expected to hold in mbtStep with one in-flight request,
  // violated under interleaving).
  val concurrencySensitive = noAccelEntryAfterDisable and noAccelPublishAfterDisable
}

// Single in-flight request: one thread.
module q2_dispatch_seq {
  import q2_dispatch.* from "./dispatch"
  action seqStep = {
    nondet op = Set(OMsm, OTransform, OCoset).oneOf()
    nondet pairs = Set(0, 1, 2).oneOf()
    nondet abiOk = Bool.oneOf()
    nondet capOk = Bool.oneOf()
    nondet sizeOk = Bool.oneOf()
    nondet valid = Bool.oneOf()
    nondet input = 1.to(3).oneOf()
    nondet kind = KINDS.oneOf()
    nondet effect = EFFECTS.oneOf()
    any {
      admit(0, op, pairs, abiOk, capOk, sizeOk, valid, input),
      enter(0), accelOk(0), accelWrongLen(0), accelErr(0, kind, effect), recycle(0),
    }
  }
}
