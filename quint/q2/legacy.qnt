// quint-q2: legacy sppark GPU paths (W4/W5 + W9 loki-harden fallback + W13
// SRS registry) as they run *ahead of* the P1 dispatcher, plus the build
// feature lattice and the compile-time exclusivity guard (a256a9a).
//
// Source correspondence:
//   build lattice       Cargo.toml:45-75 (features), src/lib.rs:11-18 (compile_error!)
//   msmCall             msm_specific             src/poly/kzg/msm.rs:165-179 (legacy arm 169-172)
//     legacy arm        cuda::try_msm / dispatch src/poly/kzg/msm/cuda.rs:23-56 (min threshold 40)
//     sticky flag       backend::msm             src/poly/kzg/msm/cuda/backend.rs:392-415 (failed = true 407)
//                       Cache::run `if self.failed` backend.rs:265-268
//     error classes     invalid base  backend.rs:165 (registry) / 308 (legacy)
//                       budget        backend.rs:326-330 (registry only)
//                       sm/spawn      backend.rs:220-, 108- (registry only)
//                       FFI msm error backend.rs:387 ; decode backend.rs:388, convert.rs:26-
//     P1 continuation   dispatch::msm_specific   src/backend/dispatch.rs:260-270 (NoAccelerator)
//   nttCall             EvaluationDomain::fft/ifft  src/poly/domain.rs:412-422, 455-472
//     glue              gpu_fft                   domain.rs:425-453 (2^10..2^28 429-430, LOKI_NTT_CPU 431)
//     adapter           loki_cuda_ntt::fft        cuda-ntt/src/lib.rs:29-59
//                       LOKI_NTT_FORCE_ERROR read cuda-ntt/src/lib.rs:33-35 (every call)
//     P1 continuation   dispatch::fft/ifft -> run_transform  dispatch.rs:272-326
module q2_legacy {
  type Build = { cuda: bool, cudaNtt: bool, registry: bool, backendApi: bool, gpuBackend: bool }
  // Legacy MSM outcome classes. LSpawn (registry worker spawn / SM query
  // failure) is modelled but not injectable in the Rust replay.
  type MsmErr = LNone | LDevice | LDecode | LInvalidBase | LBudget | LSpawn
  type NttErr = NNone | NDevice
  type P1Ctr = { msmCpu: int, msmFb: int, trCpu: int, trFb: int }

  pure val MIN = 4            // LOKI_GPU_MSM_MIN in the replay (production default 2^14)
  pure val NTT_MIN_LOG = 10   // domain.rs:429 a.len() < 1 << 10
  pure val WRONG = -99
  pure def oracle(code: int, x: int): int = 10 * x + code

  pure def closed(b: Build): bool =
    (not(b.registry) or b.cuda) and (not(b.gpuBackend) or b.backendApi)
  pure def isDefault(b: Build): bool =
    not(b.cuda or b.cudaNtt or b.registry or b.backendApi or b.gpuBackend)
  pure val ALL_BUILDS: Set[Build] =
    tuples(Bool, Bool, Bool, Bool, Bool).map(t =>
      { cuda: t._1, cudaNtt: t._2, registry: t._3, backendApi: t._4, gpuBackend: t._5 })
      .filter(b => closed(b))

  // ------------------------------------------------------------- mutation / fix switches
  pure val M_NO_COMPILE_GUARD = false   // drop src/lib.rs compile_error!
  pure val M_DEFAULT_LEAK = false       // legacy arm compiled regardless of `cuda`
  pure val M_NO_STICKY_CHECK = false    // drop `if self.failed` in Cache::run
  pure val M_GPU_WRONG_ON_DECODE = false // publish undecodable GPU output
  pure val M_DOUBLE_NORM = false        // CPU applies 1/n after a scaled sppark inverse
  pure val M_SHARED_POISON = false      // fix: one device-wide poison flag gates MSM and NTT
  pure val M_STICKY_DEVICE_ONLY = false // fix: only device/decode errors set `failed`
  pure val M_COUNT_LEGACY = false       // fix: legacy failures bump P1 *_fallback
  pure val M_NO_ENV_INJECTION = false   // fix: LOKI_NTT_FORCE_ERROR compiled only under cfg(test)
  pure val M_CLEAR_ON_NTT_SUCCESS = false // mutation: a successful NTT re-enables GPU MSM

  pure def compiles(b: Build): bool =
    M_NO_COMPILE_GUARD or not(b.gpuBackend and (b.cuda or b.cudaNtt))

  // ------------------------------------------------------------- state
  var build: Build
  var envForceNtt: bool     // LOKI_NTT_FORCE_ERROR present
  var envNttCpu: bool       // LOKI_NTT_CPU present
  var advertised: bool      // LokiGpu::capabilities advertises host ops (false today)
  var msmFailed: bool       // backend.rs Cache.failed
  var gpuMsmCalls: int      // loki_gpu_msm FFI entries
  var gpuNttCalls: int      // loki_ntt FFI entries
  var p1Accel: int          // P1 accelerator entries
  var p1: P1Ctr             // P1 dispatcher counters (when backendApi)
  var lastResult: int
  var lastOracle: int
  // ghosts
  var deviceFault: bool
  var nttAfterFault: bool
  var msmWhileFailed: bool
  var everFailed: bool
  var stickyNonDevice: bool
  var legacyFails: int
  var forcedErrorTookEffect: bool
  var nttRetried: bool
  var nttErrored: bool

  action init = {
    nondet b = ALL_BUILDS.filter(b => compiles(b)).oneOf()
    nondet f = Bool.oneOf()
    nondet c = Bool.oneOf()
    nondet a = Bool.oneOf()
    initWith(b, f, c, a)
  }
  action initWith(b: Build, f: bool, c: bool, a: bool): bool = all {
    build' = b, envForceNtt' = f, envNttCpu' = c, advertised' = a,
    msmFailed' = false, gpuMsmCalls' = 0, gpuNttCalls' = 0, p1Accel' = 0,
    p1' = { msmCpu: 0, msmFb: 0, trCpu: 0, trFb: 0 },
    lastResult' = 0, lastOracle' = 0,
    deviceFault' = false, nttAfterFault' = false, msmWhileFailed' = false,
    everFailed' = false, stickyNonDevice' = false, legacyFails' = 0,
    forcedErrorTookEffect' = false, nttRetried' = false, nttErrored' = false,
  }
  // MBT build: loki-sppark (backend-api + cuda + cuda-ntt), without/with registry.
  action initSppark = initWith({ cuda: true, cudaNtt: true, registry: false, backendApi: true, gpuBackend: false }, false, false, false)
  action initSpparkRegistry = initWith({ cuda: true, cudaNtt: true, registry: true, backendApi: true, gpuBackend: false }, false, false, false)

  val sharedBlocked = M_SHARED_POISON and deviceFault

  // ------------------------------------------------------------- MSM
  action msmCall(len: int, err: MsmErr, x: int): bool = {
    val legacyArm = build.cuda or M_DEFAULT_LEAK
    val regOnly = err == LBudget or err == LSpawn
    val tryGpu = legacyArm and len >= MIN
    val gated = msmFailed and not(M_NO_STICKY_CHECK)
    val blocked = gated or sharedBlocked
    val ffi = tryGpu and not(blocked) and (err == LNone or err == LDevice or err == LDecode)
    val gpuErr = tryGpu and not(blocked) and err != LNone
    val gpuOk = tryGpu and not(blocked) and err == LNone
    val devClass = err == LDevice or err == LDecode
    val setsSticky = gpuErr and (not(M_STICKY_DEVICE_ONLY) or devClass)
    val o = oracle(1, x)
    // Any legacy None (below threshold, disabled, or error) continues to P1.
    val toP1 = not(gpuOk) and build.backendApi
    val p1AccelNow = toP1 and build.gpuBackend and advertised and len > 0
    val legacyFailed = tryGpu and (blocked or gpuErr)
    all {
      not(regOnly) or build.registry,
      gpuMsmCalls' = if (ffi) gpuMsmCalls + 1 else gpuMsmCalls,
      msmWhileFailed' = (msmWhileFailed or (ffi and msmFailed)),
      msmFailed' = (msmFailed or setsSticky),
      everFailed' = (everFailed or setsSticky),
      deviceFault' = (deviceFault or (gpuErr and devClass)),
      stickyNonDevice' = (stickyNonDevice or (setsSticky and not(devClass))),
      legacyFails' = if (legacyFailed and build.backendApi) legacyFails + 1 else legacyFails,
      p1Accel' = if (p1AccelNow) p1Accel + 1 else p1Accel,
      p1' = if (toP1 and not(p1AccelNow))
              { ...p1, msmCpu: p1.msmCpu + 1,
                msmFb: if (M_COUNT_LEGACY and legacyFailed) p1.msmFb + 1 else p1.msmFb }
            else p1,
      lastResult' = if (M_GPU_WRONG_ON_DECODE and gpuErr and err == LDecode) WRONG else o,
      lastOracle' = o,
      gpuNttCalls' = gpuNttCalls, nttAfterFault' = nttAfterFault,
      forcedErrorTookEffect' = forcedErrorTookEffect, nttRetried' = nttRetried, nttErrored' = nttErrored,
      build' = build, envForceNtt' = envForceNtt, envNttCpu' = envNttCpu, advertised' = advertised,
    }
  }

  // ------------------------------------------------------------- NTT
  action nttCall(logn: int, inverse: bool, err: NttErr, x: int): bool = {
    val glue = build.cudaNtt
    val eligible = glue and logn >= NTT_MIN_LOG and logn <= 28 and not(envNttCpu)
    val forced = eligible and envForceNtt and not(M_NO_ENV_INJECTION)
    val blocked = sharedBlocked and not(forced)
    val ffi = eligible and not(forced) and not(blocked)
    val gpuOk = ffi and err == NNone
    val o = oracle(if (inverse) 3 else 2, x)
    val toP1 = not(gpuOk) and build.backendApi
    val p1AccelNow = toP1 and build.gpuBackend and advertised
    all {
      gpuNttCalls' = if (ffi) gpuNttCalls + 1 else gpuNttCalls,
      nttAfterFault' = (nttAfterFault or (ffi and deviceFault)),
      nttRetried' = (nttRetried or (ffi and nttErrored)),
      nttErrored' = (nttErrored or (ffi and err == NDevice)),
      deviceFault' = (deviceFault or (ffi and err == NDevice)),
      forcedErrorTookEffect' = (forcedErrorTookEffect or forced),
      p1Accel' = if (p1AccelNow) p1Accel + 1 else p1Accel,
      p1' = if (toP1 and not(p1AccelNow)) { ...p1, trCpu: p1.trCpu + 1 } else p1,
      lastResult' = if (M_DOUBLE_NORM and gpuOk and inverse) WRONG else o,
      lastOracle' = o,
      msmFailed' = if (M_CLEAR_ON_NTT_SUCCESS and gpuOk) false else msmFailed,
      gpuMsmCalls' = gpuMsmCalls, msmWhileFailed' = msmWhileFailed,
      everFailed' = everFailed, stickyNonDevice' = stickyNonDevice, legacyFails' = legacyFails,
      build' = build, envForceNtt' = envForceNtt, envNttCpu' = envNttCpu, advertised' = advertised,
    }
  }

  // The process environment can change between calls (both variables are
  // read on every call, never cached).
  action setEnv(force: bool, cpu: bool): bool = all {
    envForceNtt' = force, envNttCpu' = cpu,
    build' = build, advertised' = advertised, msmFailed' = msmFailed,
    gpuMsmCalls' = gpuMsmCalls, gpuNttCalls' = gpuNttCalls, p1Accel' = p1Accel, p1' = p1,
    lastResult' = lastResult, lastOracle' = lastOracle, deviceFault' = deviceFault,
    nttAfterFault' = nttAfterFault, msmWhileFailed' = msmWhileFailed, everFailed' = everFailed,
    stickyNonDevice' = stickyNonDevice, legacyFails' = legacyFails,
    forcedErrorTookEffect' = forcedErrorTookEffect, nttRetried' = nttRetried, nttErrored' = nttErrored,
  }

  action step = {
    nondet len = Set(0, 3, 5).oneOf()
    nondet err = Set(LNone, LDevice, LDecode, LInvalidBase, LBudget, LSpawn).oneOf()
    nondet logn = Set(4, 10, 12).oneOf()
    nondet inverse = Bool.oneOf()
    nondet nerr = Set(NNone, NDevice).oneOf()
    nondet x = 1.to(3).oneOf()
    nondet force = Bool.oneOf()
    nondet cpu = Bool.oneOf()
    any { msmCall(len, err, x), nttCall(logn, inverse, nerr, x), setEnv(force, cpu) }
  }

  // Replay step: LSpawn is not injectable; NTT sizes are kept small (2^10).
  action mbtStep = {
    nondet len = Set(0, 3, 5).oneOf()
    nondet err = (if (build.registry) Set(LNone, LDevice, LDecode, LInvalidBase, LBudget)
                  else Set(LNone, LDevice, LDecode, LInvalidBase)).oneOf()
    nondet logn = Set(4, 10).oneOf()
    nondet inverse = Bool.oneOf()
    nondet nerr = Set(NNone, NDevice).oneOf()
    nondet x = 1.to(3).oneOf()
    nondet force = Bool.oneOf()
    nondet cpu = Bool.oneOf()
    any { msmCall(len, err, x), nttCall(logn, inverse, nerr, x), setEnv(force, cpu) }
  }

  // ------------------------------------------------------------- invariants
  // Expected to hold:
  val legacyResultEqualsOracle = lastResult == lastOracle
  val noGpuMsmWhileSticky = not(msmWhileFailed)
  val stickyIsPermanent = everFailed implies msmFailed
  val defaultBuildNeverTouchesAccelerator =
    isDefault(build) implies (gpuMsmCalls == 0 and gpuNttCalls == 0 and p1Accel == 0)
  val acceleratorsExclusive = not(build.gpuBackend and (build.cuda or build.cudaNtt))
  // Contract statements the code violates (findings):
  // L1: after a device fault reported by the MSM (or NTT) adapter, the device is still used.
  val noDeviceUseAfterFault = not(nttAfterFault)
  // L2: only device faults should permanently disable GPU MSM.
  val stickyOnlyForDeviceFaults = not(stickyNonDevice)
  // L3: P1 counters see legacy GPU failures (GPU-required gates assert *_fallback == 0).
  val legacyFailuresVisibleInCounters = build.backendApi implies p1.msmFb >= legacyFails
  // L4: a test-only injection variable has no effect in production builds.
  val envInjectionInert = not(forcedErrorTookEffect)
  // Witness (expected violated): NTT is retried on the GPU after an NTT error.
  val nttNeverRetried = not(nttRetried)
}
