//! `ntt_diff`: Loki `EvaluationDomain` conversions (CPU, or sppark with
//! `--features cuda-ntt`) == stock 0.8.3 elementwise == naive O(n^2) DFT.
//!
//! Lean `Ntt`: natural-order NTT with the domain omega (`bestFft_isNaturalNtt`),
//! coset by ZETA with the global index mod 3 (`coset_via_natural`), the inverse
//! scaled by 1/n exactly once (`icoset_via_inverse`). Elementwise comparison of
//! `coeff_to_extended` / `extended_to_coeff` against stock and naive evaluation
//! pins the root and the coset; round trips alone would not.

use ff::{Field, WithSmallOrderMulGroup};
use midnight_curves::Fq;

use crate::Input;

/// Naive DFT up to this length.
const NAIVE_MAX: usize = 256;

fn eval(coeffs: &[Fq], x: Fq) -> Fq {
    coeffs.iter().rev().fold(Fq::ZERO, |acc, c| acc * x + c)
}

fn naive_dft(coeffs: &[Fq], omega: Fq, scale: Fq, n: usize) -> Vec<Fq> {
    let mut point = scale;
    (0..n)
        .map(|_| {
            let v = eval(coeffs, point);
            point *= omega;
            v
        })
        .collect()
}

fn values(input: &mut Input, n: usize) -> Vec<Fq> {
    match input.u8() % 8 {
        0 => vec![Fq::ZERO; n],
        1 => {
            // Impulse at a fuzzed index.
            let mut v = vec![Fq::ZERO; n];
            v[input.range(0, n - 1)] = input.scalar();
            v
        }
        2 => vec![input.scalar(); n],
        3 => vec![-Fq::ONE; n],
        _ => (0..n)
            .map(|_| if input.exhausted() { input.rng_scalar() } else { input.scalar() })
            .collect(),
    }
}

/// Whether the active Loki build could route a transform of `len` to the GPU.
pub fn gpu_eligible(len: usize) -> bool {
    cfg!(feature = "cuda-ntt") && len >= 1 << 10
}

/// Completed GPU NTT calls so far (0 without `cuda-ntt`).
pub fn gpu_calls() -> u64 {
    #[cfg(feature = "cuda-ntt")]
    return loki_cuda_ntt::completed_calls();
    #[cfg(not(feature = "cuda-ntt"))]
    0
}

macro_rules! with_cpu {
    ($cpu:expr, $body:expr) => {{
        if $cpu {
            std::env::set_var("LOKI_NTT_CPU", "1");
        }
        let out = $body;
        if $cpu {
            std::env::remove_var("LOKI_NTT_CPU");
        }
        out
    }};
}

pub fn run(data: &[u8]) {
    let mut input = Input::new(data);
    let k = match input.u8() % 8 {
        0..=3 => input.range(1, 8),
        // GPU threshold: n = 2^9 (CPU) / 2^10 (first GPU size), extended 2^10.
        4 | 5 => input.range(9, 11),
        _ => input.range(1, 16),
    } as u32;
    let j = match input.u8() % 4 {
        0 => 1,
        1 => 2,
        2 => 3,
        _ => input.range(4, 9) as u32,
    };
    if j > 1 && (k + 4) > 18 {
        // Bound extended domains (2^18) for throughput.
        return;
    }
    let ld = loki::poly::EvaluationDomain::<Fq>::new(j, k);
    if ld.extended_k() > 18 {
        return;
    }
    let n = 1usize << k;
    let ext = ld.extended_len();
    // With cuda-ntt, also run Loki with the GPU explicitly disabled.
    let cpu_too = cfg!(feature = "cuda-ntt") && input.bool();
    let a = values(&mut input, n);
    let e = values(&mut input, ext);
    check(k, j, cpu_too, &a, &e);
}

/// Compare every conversion for one domain `(j, k)`, coefficients `a`
/// (length 2^k) and extended values `e` (length extended_len).
pub fn check(k: u32, j: u32, cpu_too: bool, a: &[Fq], e: &[Fq]) {
    let ld = loki::poly::EvaluationDomain::<Fq>::new(j, k);
    let sd = stock::poly::EvaluationDomain::<Fq>::new(j, k);
    let n = 1usize << k;
    let ext = ld.extended_len();
    assert_eq!((a.len(), e.len()), (n, ext));
    assert_eq!(ld.extended_k(), sd.extended_k());
    assert_eq!(ld.get_omega(), sd.get_omega());
    assert_eq!(ld.get_omega_inv(), sd.get_omega_inv());
    assert_eq!(ld.get_extended_omega(), sd.get_extended_omega());
    let ctx = format!("k={k} j={j} ext_k={} gpu_n={} gpu_ext={}", ld.extended_k(), gpu_eligible(n), gpu_eligible(ext));

    for cpu in [false, cpu_too] {
        let tag = if cpu { "loki-cpu" } else { "loki" };
        let calls = gpu_calls();
        // coeff -> lagrange (forward NTT, omega).
        let l = with_cpu!(cpu, ld.coeff_to_lagrange(ld.coeff_from_vec(a.to_vec())));
        let s = sd.coeff_to_lagrange(sd.coeff_from_vec(a.to_vec()));
        assert!(l[..] == s[..], "{tag} coeff_to_lagrange != stock ({ctx})");
        // lagrange -> coeff (inverse, 1/n once).
        let l = with_cpu!(cpu, ld.lagrange_to_coeff(ld.lagrange_from_vec(a.to_vec())));
        let s = sd.lagrange_to_coeff(sd.lagrange_from_vec(a.to_vec()));
        assert!(l[..] == s[..], "{tag} lagrange_to_coeff != stock ({ctx})");
        // coeff -> extended coset (ZETA, extended omega, zero padding).
        let lx = with_cpu!(cpu, ld.coeff_to_extended(ld.coeff_from_vec(a.to_vec())));
        let sx = sd.coeff_to_extended(sd.coeff_from_vec(a.to_vec()));
        assert!(lx[..] == sx[..], "{tag} coeff_to_extended != stock ({ctx})");
        // extended -> coeff (inverse with coset correction).
        let l = with_cpu!(cpu, ld.extended_to_coeff(lx.clone()));
        let s = sd.extended_to_coeff(sx);
        assert!(l == s, "{tag} extended_to_coeff != stock ({ctx})");
        // Round trip: the padded coefficients come back exactly.
        assert!(l[..n] == *a && l[n..].iter().all(|x| x.is_zero_vartime()), "{tag} extended round trip ({ctx})");
        // Arbitrary extended values -> lagrange and -> coeff, quotient division.
        let l = with_cpu!(cpu, ld.extended_to_lagrange(ld_ext(&ld, e)));
        let s = sd.extended_to_lagrange(sd_ext(&sd, e));
        assert!(l[..] == s[..], "{tag} extended_to_lagrange != stock ({ctx})");
        let l = with_cpu!(cpu, ld.extended_to_coeff(ld_ext(&ld, e)));
        let s = sd.extended_to_coeff(sd_ext(&sd, e));
        assert!(l == s, "{tag} extended_to_coeff(arbitrary) != stock ({ctx})");
        let l = ld.divide_by_vanishing_poly(ld_ext(&ld, e));
        let s = sd.divide_by_vanishing_poly(sd_ext(&sd, e));
        assert!(l[..] == s[..], "divide_by_vanishing_poly != stock ({ctx})");
        // The GPU really ran for eligible lengths, and never when disabled.
        let used = gpu_calls() - calls;
        if cpu || !gpu_eligible(ext) {
            assert_eq!(used, 0, "{tag}: unexpected GPU NTT calls ({ctx})");
        } else {
            assert!(used >= 4, "{tag}: GPU NTT not used ({ctx}, calls={used})");
        }
    }

    // Round trip lagrange -> coeff -> lagrange.
    let back = ld.coeff_to_lagrange(ld.lagrange_to_coeff(ld.lagrange_from_vec(a.to_vec())));
    assert!(back[..] == *a, "lagrange round trip ({ctx})");

    // Naive mathematical definition (pins omega, ZETA and 1/n).
    if ext <= NAIVE_MAX {
        let omega = ld.get_omega();
        let lag = ld.coeff_to_lagrange(ld.coeff_from_vec(a.to_vec()));
        assert!(lag[..] == naive_dft(a, omega, Fq::ONE, n)[..], "coeff_to_lagrange != naive DFT ({ctx})");
        let coeff = ld.lagrange_to_coeff(ld.lagrange_from_vec(a.to_vec()));
        let n_inv = Fq::from(n as u64).invert().unwrap();
        let inv: Vec<Fq> = naive_dft(a, ld.get_omega_inv(), Fq::ONE, n)
            .into_iter()
            .map(|x| x * n_inv)
            .collect();
        assert!(coeff[..] == inv[..], "lagrange_to_coeff != naive inverse DFT ({ctx})");
        let x = ld.coeff_to_extended(ld.coeff_from_vec(a.to_vec()));
        let want = naive_dft(a, ld.get_extended_omega(), <Fq as WithSmallOrderMulGroup<3>>::ZETA, ext);
        assert!(x[..] == want[..], "coeff_to_extended != naive coset DFT at ZETA ({ctx})");
    }
}

fn ld_ext(
    d: &loki::poly::EvaluationDomain<Fq>,
    v: &[Fq],
) -> loki::poly::Polynomial<Fq, loki::poly::ExtendedLagrangeCoeff> {
    let mut p = d.empty_extended();
    p.iter_mut().zip(v).for_each(|(a, b)| *a = *b);
    p
}

fn sd_ext(
    d: &stock::poly::EvaluationDomain<Fq>,
    v: &[Fq],
) -> stock::poly::Polynomial<Fq, stock::poly::ExtendedLagrangeCoeff> {
    let mut p = d.empty_extended();
    p.iter_mut().zip(v).for_each(|(a, b)| *a = *b);
    p
}
