# Self-check of BLS12-381 constants: r = x^4 - x^2 + 1, h1 = (x-1)^2/3, x = -0xd201000000010000
x = -0xd201000000010000
r = x**4 - x**2 + 1
h1_num = (x-1)**2
assert h1_num % 3 == 0
h1 = h1_num // 3
print("r   =", hex(r))
print("h1  =", hex(h1))
print("r odd:", r % 2 == 1, " h1 odd:", h1 % 2 == 1)
def is_prime(n, k=40):
    import random
    if n < 2: return False
    for p in [2,3,5,7,11,13,17,19,23,29,31,37]:
        if n % p == 0: return n == p
    d, s = n-1, 0
    while d % 2 == 0: d//=2; s+=1
    for _ in range(k):
        a = random.randrange(2, n-2); y = pow(a, d, n)
        if y in (1, n-1): continue
        for _ in range(s-1):
            y = pow(y, 2, n)
            if y == n-1: break
        else: return False
    return True
print("r prime (MR, 40 rounds):", is_prime(r))
print("#E(Fp) = h1*r odd:", (h1*r) % 2 == 1)
# BN254 (alt_bn128) prime group order check
rbn = 21888242871839275222246405745257275088548364400416034343698204186575808495617
print("BN254 r odd:", rbn % 2 == 1, " prime:", is_prime(rbn))
