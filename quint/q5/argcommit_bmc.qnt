// -*- mode: Bluespec; -*-
// Fixed-size variant of argcommit.qnt for Apalache (quint verify): domain k = 1 (n = 2),
// SRS exact (maxK = 1) or oversized (maxK = 2), all taus and all value vectors symbolic.
// Same semantics as argcommit.qnt commitArgW10 / commitLagrange (see its correspondence table).
module argcommit_bmc {
  const MUT: int
  pure val P = 17
  pure def md(x: int): int = ((x % P) + P) % P
  pure def mul(a: int, b: int): int = md(a * b)
  pure val INV2 = 9    // 2 * 9 = 18 = 1
  pure val INV4 = 13   // 4 * 13 = 52 = 1
  pure val W4INV = 13  // omega_4 = 4, 4 * 13 = 1
  pure def w4i(e: int): int = if (e % 4 == 0) 1 else if (e % 4 == 1) W4INV else if (e % 4 == 2) 16 else 4
  var tau: int
  var over: bool
  var v0: int
  var v1: int
  var cStock: int
  var cW10: int
  def g(j: int): int = if (j == 0) 1 else if (j == 1) tau else if (j == 2) mul(tau, tau) else mul(mul(tau, tau), tau)
  // g_lagrange_i for the SRS's own domain (n = 2 if exact, 4 if oversized)
  def gl(i: int): int =
    if (not(over)) mul(INV2, md(g(0) + (if (i == 0) g(1) else md(0 - g(1)))))
    else mul(INV4, md(mul(w4i(0), g(0)) + mul(w4i(i), g(1)) + mul(w4i(2 * i), g(2)) + mul(w4i(3 * i), g(3))))
  action init = {
    nondet t = 2.to(16).oneOf()
    nondet o = Bool.oneOf()
    nondet a = 0.to(16).oneOf()
    nondet b = 0.to(16).oneOf()
    all { tau' = t, over' = o, v0' = a, v1' = b, cStock' = -1, cW10' = -1 }
  }
  action commit = all {
    cStock == -1,
    val lag = md(mul(v0, gl(0)) + mul(v1, gl(1)))
    val c0 = mul(INV2, md(v0 + v1))
    val c1 = mul(INV2, md(v0 - v1))
    val coeff = md(mul(c0, g(0)) + mul(c1, g(1)))
    all {
      cStock' = lag,
      cW10' = if (not(over) or MUT == 1) coeff else lag,
      tau' = tau, over' = over, v0' = v0, v1' = v1,
    }
  }
  action stutter = all { cStock != -1, tau' = tau, over' = over, v0' = v0, v1' = v1, cStock' = cStock, cW10' = cW10 }
  action step = any { commit, stutter }
  val Equal = cStock == cW10
}
module bmc_ok { import argcommit_bmc(MUT = 0).* }
module bmc_mut1 { import argcommit_bmc(MUT = 1).* }
