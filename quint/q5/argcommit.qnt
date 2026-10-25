// -*- mode: Bluespec; -*-
// q5: W10 `cuda-argument-commit` commitment strategy vs the main/stock Lagrange commit.
//
// G1 is modelled by discrete logs in GF(17) (commitment = scalar multiple of G),
// so MSMs are dot products. SRS: g_i = tau^i for i < 2^maxK; g_lagrange is
// either the inverse DFT of g over the 2^maxK domain (what unsafe_setup,
// downsize and from_parts(None) produce) or an arbitrary basis supplied by a
// file (read_custom / from_parts(Some), not checked by the crate).
//
// Source correspondence:
//   commitArgW10        W10 5e37ead src/poly/commitment.rs:128-151 (commit_argument)
//   commitLagrange      src/poly/kzg/mod.rs:86-95 (KZG commit_lagrange: MSM over g_lagrange[0..n])
//   commitCoeff         src/poly/kzg/mod.rs:77-84 (KZG commit: MSM over g[0..n])
//   lagrangeToCoeff     src/poly/domain.rs:250 (EvaluationDomain::lagrange_to_coeff)
//   gToLagrange         src/utils/arithmetic.rs g_to_lagrange; kzg/params.rs:40-49 (downsize), 102-118 (from_parts),
//                       154-230 (read_custom: g_lagrange read from the file, unchecked)
//   lookupCommits       W10 src/plonk/lookup/prover.rs:116-140 (commit_values 117-124; writes input 137 then table 140)
//   permCommit          W10 src/plonk/permutation/prover.rs:147-156 (commit_argument 149, write 156)
//   maxK                src/poly/kzg/params.rs:27-31 (ilog2 of g.len())
module argcommit {
  const MUT: int
  pure val P = 17
  pure def md(x: int): int = ((x % P) + P) % P
  pure def mul(a: int, b: int): int = md(a * b)
  pure def pw(a: int, e: int): int = range(0, e).foldl(1, (acc, _i) => mul(acc, a))
  pure def inv(a: int): int = pw(a, P - 2)
  // primitive 2^k-th roots of unity in GF(17): order 2 -> 16, 4 -> 4, 8 -> 2
  pure def omega(k: int): int = if (k == 1) 16 else if (k == 2) 4 else 2
  pure def dot(a: List[int], b: List[int]): int = range(0, a.length()).foldl(0, (s, i) => md(s + mul(a[i], b[i])))

  pure def gOf(tau: int, kk: int): List[int] = range(0, pw(2, kk)).foldl([], (acc, i) => acc.append(pw(tau, i)))
  // g_lagrange_i = (1/N) sum_j omega^{-ij} g_j  (commitment to the i-th Lagrange basis polynomial)
  pure def gToLagrange(g: List[int], kk: int): List[int] = {
    val n = pw(2, kk)
    val wi = inv(omega(kk))
    range(0, n).foldl([], (acc, i) =>
      acc.append(mul(inv(n), range(0, n).foldl(0, (s, j) => md(s + mul(pw(wi, i * j), g[j]))))))
  }
  pure def lagrangeToCoeff(v: List[int], k: int): List[int] = {
    val n = v.length()
    val wi = inv(omega(k))
    range(0, n).foldl([], (acc, j) =>
      acc.append(mul(inv(n), range(0, n).foldl(0, (s, i) => md(s + mul(pw(wi, i * j), v[i]))))))
  }
  type Srs = { maxK: int, g: List[int], gl: List[int] }
  pure def commitCoeff(p: Srs, c: List[int]): int = dot(c, p.g.slice(0, c.length()))
  pure def commitLagrange(p: Srs, v: List[int]): int = dot(v, p.gl.slice(0, v.length()))
  // W10 commit_argument (returns the commitment; the coefficient poly is lagrange_to_coeff in both arms)
  pure def commitArgW10(p: Srs, k: int, v: List[int]): int =
    if (p.maxK == k or MUT == 1) {
      // MUT 2: coefficients committed against the Lagrange basis
      if (MUT == 2) dot(lagrangeToCoeff(v, k), p.gl.slice(0, v.length())) else commitCoeff(p, lagrangeToCoeff(v, k))
    } else commitLagrange(p, v)

  var k: int
  var srs: Srs
  var valid: bool
  var tStock: List[int]   // transcript: commitments written by main (commit_lagrange)
  var tW10: List[int]     // transcript: commitments written by W10
  var coeffUsed: bool
  var fallbackUsed: bool
  var downsized: bool

  action init = {
    nondet k0 = 1.to(2).oneOf()
    nondet over = Bool.oneOf()
    nondet tau = 2.to(16).oneOf()
    nondet good = Bool.oneOf()
    nondet bad = setOfMaps(0.to(7), 0.to(16)).oneOf()
    val mk = if (over) k0 + 1 else k0
    val g = gOf(tau, mk)
    val gl = if (good) gToLagrange(g, mk) else range(0, pw(2, mk)).foldl([], (acc, i) => acc.append(bad.get(i)))
    all {
      k' = k0, srs' = { maxK: mk, g: g, gl: gl }, valid' = gl == gToLagrange(g, mk),
      tStock' = [], tW10' = [], coeffUsed' = false, fallbackUsed' = false, downsized' = false,
    }
  }
  // ParamsKZG::downsize(k): truncate g, regenerate g_lagrange
  action downsize = all {
    srs.maxK > k,
    val g = srs.g.slice(0, pw(2, k))
    all {
      srs' = { maxK: k, g: g, gl: gToLagrange(g, k) }, valid' = true, downsized' = true,
      k' = k, tStock' = tStock, tW10' = tW10, coeffUsed' = coeffUsed, fallbackUsed' = fallbackUsed,
    }
  }
  // One lookup's permuted input + permuted table commitments, written in that order;
  // or one permutation product commitment.
  action commitLookup = {
    nondet am = setOfMaps(0.to(3), 0.to(16)).oneOf()
    nondet sm = setOfMaps(0.to(3), 0.to(16)).oneOf()
    val n = pw(2, k)
    val a = range(0, n).foldl([], (acc, i) => acc.append(am.get(i)))
    val s = range(0, n).foldl([], (acc, i) => acc.append(sm.get(i)))
    val ca = commitArgW10(srs, k, a)
    val cs = commitArgW10(srs, k, s)
    all {
      tStock' = tStock.concat([commitLagrange(srs, a), commitLagrange(srs, s)]),
      tW10' = tW10.concat(if (MUT == 3) [cs, ca] else [ca, cs]),
      coeffUsed' = (coeffUsed or srs.maxK == k), fallbackUsed' = (fallbackUsed or srs.maxK != k),
      k' = k, srs' = srs, valid' = valid, downsized' = downsized,
    }
  }
  action commitPerm = {
    nondet zm = setOfMaps(0.to(3), 0.to(16)).oneOf()
    val z = range(0, pw(2, k)).foldl([], (acc, i) => acc.append(zm.get(i)))
    all {
      tStock' = tStock.append(commitLagrange(srs, z)), tW10' = tW10.append(commitArgW10(srs, k, z)),
      coeffUsed' = (coeffUsed or srs.maxK == k), fallbackUsed' = (fallbackUsed or srs.maxK != k),
      k' = k, srs' = srs, valid' = valid, downsized' = downsized,
    }
  }
  action step = any { downsize, commitLookup, commitPerm }

  // On any SRS whose g_lagrange is the iDFT of g (every in-crate construction), W10's
  // commitments equal main's commit_lagrange, both on the coefficient path (maxK == k)
  // and on the oversized fallback, before and after downsize.
  val W10EqualsStockOnValidSrs = not(valid) or tW10 == tStock
  val TranscriptEqual = not(valid) or tW10 == tStock
  // Closed form: commit(lagrange_to_coeff(v)) == commit_lagrange(v) for a valid exact SRS,
  // for every tau and every v in a fixed set including 0, ONE, the max element and spikes.
  pure val VS = [[0, 0], [1, 1], [16, 16], [1, 0], [0, 16], [3, 7]]
  val ClosedForm = 2.to(16).forall(tau => VS.foldl(true, (ok, v) => {
    val p1 = { maxK: 1, g: gOf(tau, 1), gl: gToLagrange(gOf(tau, 1), 1) }
    ok and commitCoeff(p1, lagrangeToCoeff(v, 1)) == commitLagrange(p1, v)
  }))
  val Safety = W10EqualsStockOnValidSrs and TranscriptEqual and ClosedForm

  // Witnesses (expected violated)
  val WitnessNoCoeffPath = not(coeffUsed and tW10.length() > 0)
  val WitnessNoFallback = not(fallbackUsed and valid)
  val WitnessNoDownsize = not(downsized and tW10.length() > 0)
  // An invalid (file-supplied) g_lagrange makes W10 and main diverge: the property needs SRS validity.
  val WitnessNoBadSrsDivergence = valid or tW10 == tStock
}

module argcommit_ok { import argcommit(MUT = 0).* }
module acmut1 { import argcommit(MUT = 1).* } // coefficient path without the max_k == k guard -> W10EqualsStockOnValidSrs
module acmut2 { import argcommit(MUT = 2).* } // coefficients committed against g_lagrange       -> W10EqualsStockOnValidSrs
module acmut3 { import argcommit(MUT = 3).* } // table commitment written before input          -> TranscriptEqual
