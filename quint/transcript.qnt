// Stock 0.8.3 order: prover.rs compute_trace/finalise_proof, then KZG multi_open.
// Payloads are symbolic; the Rust adapter compares real bytes in memory.
module transcript {
  type Event = { kind: str, label: str, proof: int, index: int }
  type Shape = { proofs: int, advice0: int, advice1: int, advice2: int, phaseChallenges: int, phaseChallenges1: int, phaseChallenges2: int,
    lookups: int, permutations: int, trash: int, quotient: int,
    instanceValues: int, committed: bool, evaluations: int, pointSets: int,
    adviceQueries: int, fixedQueries: int, instanceQueries: int, permutationColumns: int }
  type State = { shape: Shape, plan: List[Event], log: List[Event], ready: Set[int], cursor: int }
  var state: State

  pure def event(kind: str, label: str, proof: int, index: int): Event =
    { kind: kind, label: label, proof: proof, index: index }
  pure def repeatEvents(kind: str, label: str, proof: int, n: int): List[Event] =
    range(0, n).foldl(List(), (acc, i) => acc.append(event(kind, label, proof, i)))
  pure def perProof(s: Shape, label: str, n: int): List[Event] =
    range(0, s.proofs).foldl(List(), (acc, p) =>
      acc.concat(repeatEvents("point", label, p, n)))
  pure def challenge(label: str): List[Event] = List(event("squeeze", label, -1, 0))
  pure def scalarPerProof(s: Shape, label: str, n: int): List[Event] =
    range(0, s.proofs).foldl(List(), (acc, p) =>
      acc.concat(repeatEvents("scalar", label, p, n)))
  pure def evalEvents(s: Shape): List[Event] =
    scalarPerProof(s, "instance_eval", if (s.committed) s.instanceQueries else 0)
      .concat(scalarPerProof(s, "advice_eval", s.adviceQueries))
      .concat(repeatEvents("scalar", "fixed_eval", -1, s.fixedQueries))
      .append(event("scalar", "random_eval", -1, 0))
      .concat(repeatEvents("scalar", "permutation_common_eval", -1, s.permutationColumns))
      .concat(scalarPerProof(s, "permutation_eval",
        if (s.permutations == 0) 0 else 3 * s.permutations - 1))
      .concat(scalarPerProof(s, "lookup_eval", 5 * s.lookups))
      .concat(scalarPerProof(s, "trash_eval", s.trash))
  pure def stock(s: Shape): List[Event] = {
    val instances = range(0, s.proofs).foldl(List(), (acc, p) => acc.concat(
      if (s.committed) List(event("common_point", "instance", p, 0))
      else List(event("common_scalar", "instance_length", p, 0))
        .concat(repeatEvents("common_scalar", "instance", p, s.instanceValues))))
    List(event("common_scalar", "vk", -1, 0)).concat(instances)
      .concat(perProof(s, "advice0", s.advice0))
      .concat(repeatEvents("squeeze", "phase0", -1, s.phaseChallenges))
      .concat(perProof(s, "advice1", s.advice1))
      .concat(repeatEvents("squeeze", "phase1", -1, s.phaseChallenges1))
      .concat(perProof(s, "advice2", s.advice2))
      .concat(repeatEvents("squeeze", "phase2", -1, s.phaseChallenges2))
      .concat(challenge("theta"))
      .concat(perProof(s, "lookup_permuted", 2 * s.lookups))
      .concat(challenge("beta")).concat(challenge("gamma"))
      .concat(perProof(s, "permutation_product", s.permutations))
      .concat(perProof(s, "lookup_product", s.lookups))
      .concat(challenge("trash"))
      .concat(perProof(s, "trash", s.trash))
      .append(event("point", "vanishing_random", -1, 0))
      .concat(challenge("y"))
      .concat(repeatEvents("point", "quotient", -1, s.quotient))
      .concat(challenge("x"))
      .concat(evalEvents(s))
      .concat(challenge("multiopen_x1")).concat(challenge("multiopen_x2"))
      .append(event("point", "multiopen_f", -1, 0))
      .concat(challenge("multiopen_x3"))
      .concat(repeatEvents("scalar", "multiopen_q", -1, s.pointSets))
      .concat(challenge("multiopen_x4"))
      .append(event("point", "multiopen_pi", -1, 0))
  }
  // Payload types and counts for four concrete circuit families in the Rust MBT.
  pure def shapeFor(profile: int, proofs: int, committed: bool): Shape = {
    val rich = profile > 0
    { proofs: proofs, advice0: if (rich) 2 else 1,
      advice1: if (profile >= 2) 1 else 0, advice2: 0,
      phaseChallenges1: 0, phaseChallenges2: 0,
      phaseChallenges: if (profile >= 2) 1 else 0,
      lookups: if (rich) 1 else 0, permutations: if (rich) 2 else 0,
      trash: if (profile == 3) 1 else 0, quotient: if (rich) 3 else 2,
      instanceValues: 1, committed: committed,
      evaluations: proofs * (if (rich) 12 else 1) + (if (rich) 6 else 2)
        + (if (profile >= 2) proofs else 0) + (if (profile == 3) proofs else 0)
        + (if (committed) proofs else 0),
      pointSets: if (rich) 4 else 1,
      adviceQueries: (if (rich) 2 else 1) + (if (profile >= 2) 1 else 0),
      fixedQueries: if (rich) 2 else 1, instanceQueries: 1,
      permutationColumns: if (rich) 3 else 0 }
  }
  pure def prefix(log: List[Event], plan: List[Event]): bool =
    log.length() <= plan.length() and log == plan.slice(0, log.length())
  pure def canEmit(s: State): bool = s.cursor < s.plan.length() and
    (s.plan.nth(s.cursor).kind != "point" or s.ready.contains(s.cursor))
  pure def applyEmit(s: State): State =
    { ...s, log: s.log.append(s.plan.nth(s.cursor)), cursor: s.cursor + 1 }
  action initFor(shape: Shape): bool = state' =
    { shape: shape, plan: stock(shape), log: List(), ready: Set(), cursor: 0 }
  action init = {
    nondet profile = 0.to(3).oneOf()
    nondet proofs = 1.to(2).oneOf()
    nondet committed = Set(false, true).oneOf()
    initFor(shapeFor(profile, proofs, committed))
  }
  // Computation may complete out of order within the current Fiat-Shamir phase.
  action compute = {
    val plan = state.plan
    nondet index = plan.indices().filter(i => i >= state.cursor and
      plan.nth(i).kind == "point" and not(state.ready.contains(i)) and
      plan.slice(state.cursor, i).foldl(true, (ok, e) => ok and e.kind != "squeeze")).oneOf()
    state' = { ...state, ready: state.ready.union(Set(index)) }
  }
  action emit = all { canEmit(state), state' = applyEmit(state) }
  action finished = all { state.cursor == state.plan.length(), state' = state }
  action step = any { compute, emit, finished }
  val orderedPrefix = prefix(state.log, state.plan)
  val absorbedSequence = state.log.select(e => e.kind != "squeeze") ==
    state.plan.slice(0, state.cursor).select(e => e.kind != "squeeze")
  val challengesBound = state.log.indices().forall(i =>
    state.log.nth(i).kind == "squeeze" implies
      state.log.slice(0, i) == state.plan.slice(0, i))
  val safety = orderedPrefix and absorbedSequence and challengesBound
  val complete = state.cursor == state.plan.length()
  val batched = state.ready.filter(i => i >= state.cursor).size() > 1
  val thetaReached = state.log.foldl(false, (yes, e) => yes or e.label == "theta")
  val multiopenReached = state.log.foldl(false, (yes, e) => yes or e.label == "multiopen_x4")
  val phased = state.log.foldl(false, (yes, e) => yes or e.label == "phase0")
}
