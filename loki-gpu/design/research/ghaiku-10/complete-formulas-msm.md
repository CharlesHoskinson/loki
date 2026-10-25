# ghaiku-10: complete addition formulas + Pippenger literature (accessed 2026-10-24)

Clean room: formulas are described and cited, not copied. Open-icicle was not consulted.

## 1. Complete formulas (Renes–Costello–Batina, ePrint 2015/1060; the addition law is Bosma–Lenstra 1995)
- Projective (X:Y:Z) with identity (0:1:0), and b3 = 3b precomputed.
- The formulas are complete when #E(Fq) is odd (no rational 2-torsion). P=Q, P=-Q and identity cases need no branches.
- Costs for a=0:

  | Operation | Cost |
  |---|---|
  | ADD (Alg 7) | 12M + 2m3b + 19a |
  | Mixed ADD (Alg 8) | 11M + 2m3b + 13a |
  | DBL (Alg 9) | 6M + 2S + 1mb + 9a |

- Mixed ADD assumes Z2=1, so the identity must never be the second argument. Whether it is safe as the first argument is OPEN; test it.
- RCB reports 1.34–1.44x slower ECDH than incomplete formulas (OpenSSL, NIST curves).

## 2. Group orders
- BLS12-381 G1: r is prime and h1 = (x-1)^2/3, both checked locally. #E(Fp) is odd, so Alg 7 is complete on all of E(Fp). MEASURED.
- G2: h2 is derived, divisible by 9 and odd. Verify h2 and b'=4(1+u) against RFC 9380.
- BN254: a=0, b=3, cofactor 1. Confirm against EIP-197.

## 3. Interface implications
- One canonical projective encoding with identity (0:1:0); do not treat Z=0 alone as the identity.
- Two entry points:
  - add_complete;
  - mixed add, which takes only a non-identity affine second point.
- No branches in bucket accumulation or reduction.
- Store b3 (and 3b') in the curve descriptor.
- Validate that inputs are on the curve in debug and test builds.

## 4. MSM literature
- cuZK (ePrint 2022/1321, TCHES 2023):
  - bucket sums are computed as an ELL→CSR transpose followed by SpMV;
  - skew is handled by CSR-Balanced (sort rows by length, schedule threads dynamically), which fits our repeated-scalar skew (2.4 s vs 25 ms at k18);
  - reported speedups (their hardware): MSM 2.08x average (up to 2.63x), end to end 2.65x (up to 4.47x).
- Read at abstract or metadata level only:
  - GZKP (ASPLOS 2023): claims load balancing for sparse scalar distributions;
  - DistMSM: multi-GPU; the 6.39x figure is unverified;
  - SRE (arXiv 2610.27452): claims 25% fewer operations; treat it as an idea to test.
- Not read: Bernstein's Pippenger draft (2002) and Pippenger 1980.

## 5. Clean-room obligations
- Our own computer-algebra derivation of the a=0 formulas from Bosma–Lenstra.
- An exhaustive small-field test (p ≥ 5 with #E odd, all pairs including O, P=Q and P=-Q, scaled representatives) against an affine reference.
- Differential tests on BN254 and BLS G1/G2 with repeated, boolean and zero scalars, and the identity in each allowed argument position.
- PROVENANCE entries for RCB, Bosma–Lenstra, Pippenger, cuZK (idea only) and SRE (idea only).

Scratch scripts: bls_check.py, bls_h2.py, rcb.txt and cuzk.txt in this directory.
(Saved by coordinator; the scout's Write was refused by its harness.)
