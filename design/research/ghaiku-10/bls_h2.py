# BLS12 G2 cofactor polynomial for BLS12-381 (x = -0xd201000000010000), check parity and divisibility
x = -0xd201000000010000
num = x**8 - 4*x**7 + 5*x**6 - 4*x**4 + 6*x**3 - 4*x**2 - 4*x + 13
print("divisible by 9:", num % 9 == 0)
h2 = num // 9
print("h2 =", hex(h2), " odd:", h2 % 2 == 1)
r = x**4 - x**2 + 1
print("#E'(Fp2) = h2*r odd:", (h2*r) % 2 == 1)
