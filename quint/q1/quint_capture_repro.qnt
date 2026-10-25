// Quint 0.32.0 name-capture repro: sameTest and sameConstTest FAIL (both backends), distinctTest passes.
// Expected: all three pass. Run: quint test quint/q1/quint_capture_repro.qnt
module quint_capture_repro {
  pure def flat(n: int, f: int => List[int]): List[int] = range(0, n).foldl(List(), (acc, i) => acc.concat(f(i)))
  pure def flatB(nB: int, fB: int => List[int]): List[int] = range(0, nB).foldl(List(), (accB, iB) => accB.concat(fB(iB)))
  pure def z(k: int): int = 0
  run sameTest = assert(flat(3, ph => flat(z(ph), i => List(i))) == List())
  run distinctTest = assert(flat(3, ph => flatB(z(ph), i => List(i))) == List())
  run sameConstTest = assert(flat(3, ph => flat(0, i => List(i))) == List())
}
