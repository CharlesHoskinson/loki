use super::*;
use midnight_curves::Fq;

// This guard must fail while the feature still executes the stock CPU FFT.
// The full matrix below will also require successful CUDA dispatch, so a CPU
// fallback cannot turn GPU differentials into vacuous CPU-vs-CPU comparisons.
#[test]
fn gpu_ntt_dispatch() {
    let domain = EvaluationDomain::<Fq>::new(2, 10);
    let calls = gpu_calls();
    let result = domain.coeff_to_lagrange(domain.coeff_from_vec(vec![Fq::ONE; 1 << 10]));
    assert_eq!(result[0], Fq::from(1 << 10));
    assert!(result[1..].iter().all(|x| *x == Fq::ZERO));
    assert!(
        gpu_calls() > calls,
        "eligible FFT did not execute on the GPU"
    );
}

fn gpu_calls() -> u64 {
    loki_cuda_ntt::completed_calls()
}

use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use std::time::Instant;

fn input(log_n: u32, pattern: usize) -> Vec<Fq> {
    let n = 1 << log_n;
    let mut rng = ChaCha20Rng::seed_from_u64(0x4c4f4b49 + u64::from(log_n));
    match pattern {
        0 => (0..n).map(|_| Fq::random(&mut rng)).collect(),
        1 => vec![Fq::ZERO; n],
        2 => vec![Fq::ONE; n],
        3 => vec![-Fq::ONE; n],
        4 => {
            let mut values = vec![Fq::ZERO; n];
            values[n - 1] = -Fq::ONE;
            values
        }
        _ => {
            let edge = [
                Fq::ZERO,
                Fq::ONE,
                -Fq::ONE,
                Fq::from(1u64 << 63),
                Fq::from_raw([u64::MAX; 4]),
            ];
            (0..n).map(|i| edge[i % edge.len()]).collect()
        }
    }
}

// Independent stock FFT oracle, including explicit coset powers and 1/n.
fn cpu_fft(values: &mut [Fq], root: Fq, log_n: u32, inverse: bool) {
    best_fft(values, root, log_n);
    if inverse {
        let divisor = Fq::from(values.len() as u64).invert().unwrap();
        for value in values {
            *value *= divisor;
        }
    }
}

fn cpu_coset(values: &mut [Fq], inverse: bool) {
    let generator = if inverse {
        Fq::ZETA.invert().unwrap()
    } else {
        Fq::ZETA
    };
    // A running power deliberately avoids the production modulo-three indexing.
    let mut power = Fq::ONE;
    for value in values {
        *value *= power;
        power *= generator;
    }
}

fn check(
    log: u32,
    factor: u32,
    pattern: usize,
    op: &str,
    expected: impl FnOnce() -> Vec<Fq>,
    actual: impl FnOnce() -> Vec<Fq>,
) {
    let now = Instant::now();
    let expected = expected();
    let cpu_ms = now.elapsed().as_secs_f64() * 1000.0;
    let calls = gpu_calls();
    let now = Instant::now();
    let actual = actual();
    let gpu_ms = now.elapsed().as_secs_f64() * 1000.0;
    let base_eligible = u64::from((1u32 << log) / factor >= (1 << 10));
    let expected_calls = match op {
        "coeff_to_lagrange" | "lagrange_to_coeff" => base_eligible,
        "extended_to_lagrange" => 1 + base_eligible,
        _ => 1,
    };
    assert_eq!(
        gpu_calls() - calls,
        expected_calls,
        "{op}: incomplete GPU dispatch"
    );

    assert_eq!(actual.len(), expected.len());
    if let Some((i, (got, want))) = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!("log={log} factor={factor} pattern={pattern} {op}[{i}]: {got:?} != {want:?}");
    }
    println!("NTT log={log} factor={factor} pattern={pattern} op={op} cpu_ms={cpu_ms:.3} gpu_ms={gpu_ms:.3}");
}

fn equality(log: u32) {
    for factor_log in 0..=2 {
        let domain = EvaluationDomain::<Fq>::new((1 << factor_log) + 1, log - factor_log);
        assert_eq!(domain.extended_k, log);
        for pattern in 0..6 {
            let extended = input(log, pattern);
            let base = &extended[..domain.n as usize];
            check(
                log,
                1 << factor_log,
                pattern,
                "coeff_to_lagrange",
                || {
                    let mut v = base.to_vec();
                    cpu_fft(&mut v, domain.omega, domain.k, false);
                    v
                },
                || {
                    domain
                        .coeff_to_lagrange(domain.coeff_from_vec(base.to_vec()))
                        .values
                },
            );
            check(
                log,
                1 << factor_log,
                pattern,
                "lagrange_to_coeff",
                || {
                    let mut v = base.to_vec();
                    cpu_fft(&mut v, domain.omega_inv, domain.k, true);
                    v
                },
                || {
                    domain
                        .lagrange_to_coeff(domain.lagrange_from_vec(base.to_vec()))
                        .values
                },
            );
            check(
                log,
                1 << factor_log,
                pattern,
                "coeff_to_extended",
                || {
                    let mut v = base.to_vec();
                    cpu_coset(&mut v, false);
                    v.resize(domain.extended_len(), Fq::ZERO);
                    cpu_fft(&mut v, domain.extended_omega, log, false);
                    v
                },
                || {
                    domain
                        .coeff_to_extended(domain.coeff_from_vec(base.to_vec()))
                        .values
                },
            );
            let polynomial = || Polynomial {
                values: extended.clone(),
                _marker: PhantomData,
            };
            check(
                log,
                1 << factor_log,
                pattern,
                "extended_to_coeff",
                || {
                    let mut v = extended.clone();
                    cpu_fft(&mut v, domain.extended_omega_inv, log, true);
                    cpu_coset(&mut v, true);
                    v
                },
                || domain.extended_to_coeff(polynomial()),
            );
            check(
                log,
                1 << factor_log,
                pattern,
                "extended_to_lagrange",
                || {
                    let mut v = extended.clone();
                    cpu_fft(&mut v, domain.extended_omega_inv, log, true);
                    v.truncate(domain.n as usize);
                    cpu_coset(&mut v, true);
                    cpu_fft(&mut v, domain.omega, domain.k, false);
                    v
                },
                || domain.extended_to_lagrange(polynomial()).values,
            );
        }
    }
}

macro_rules! matrix {
    ($($name:ident: $log:literal),* $(,)?) => {$(
        #[test]
        #[ignore = "requires CUDA; full CPU/GPU equality matrix"]
        fn $name() { equality($log); }
    )*};
}

matrix! {
    gpu_ntt_equality_10: 10, gpu_ntt_equality_11: 11,
    gpu_ntt_equality_12: 12, gpu_ntt_equality_13: 13,
    gpu_ntt_equality_14: 14, gpu_ntt_equality_15: 15,
    gpu_ntt_equality_16: 16, gpu_ntt_equality_17: 17,
    gpu_ntt_equality_18: 18, gpu_ntt_equality_19: 19,
    gpu_ntt_equality_20: 20, gpu_ntt_equality_21: 21,
    gpu_ntt_equality_22: 22, gpu_ntt_equality_23: 23,
}

// Benchmark the real public entry points on both backends, with identical
// inputs and host allocations. Run alone with --test-threads=1: backend
// selection changes a process environment variable for this benchmark only.
#[test]
#[ignore = "requires CUDA; per-call wall-time benchmark"]
fn gpu_ntt_benchmark() {
    for log in 10..=23 {
        for factor_log in [0, 2] {
            let domain = EvaluationDomain::<Fq>::new((1 << factor_log) + 1, log - factor_log);
            let extended = input(log, 0);
            let run = |op| match op {
                0 => {
                    domain
                        .coeff_to_lagrange(
                            domain.coeff_from_vec(extended[..domain.n as usize].to_vec()),
                        )
                        .values
                }
                1 => {
                    domain
                        .lagrange_to_coeff(
                            domain.lagrange_from_vec(extended[..domain.n as usize].to_vec()),
                        )
                        .values
                }
                2 => {
                    domain
                        .coeff_to_extended(
                            domain.coeff_from_vec(extended[..domain.n as usize].to_vec()),
                        )
                        .values
                }
                3 => domain.extended_to_coeff(Polynomial {
                    values: extended.clone(),
                    _marker: PhantomData,
                }),
                _ => {
                    domain
                        .extended_to_lagrange(Polynomial {
                            values: extended.clone(),
                            _marker: PhantomData,
                        })
                        .values
                }
            };
            for (op, name) in [
                "coeff_to_lagrange",
                "lagrange_to_coeff",
                "coeff_to_extended",
                "extended_to_coeff",
                "extended_to_lagrange",
            ]
            .iter()
            .enumerate()
            {
                let mut timings = [vec![], vec![]];
                // First pair warms context/parameters; rotate CPU/GPU order to
                // reduce systematic ordering bias. Cloning and transfers count.
                for repetition in 0..4 {
                    for backend in if repetition % 2 == 0 { [0, 1] } else { [1, 0] } {
                        if backend == 0 {
                            std::env::set_var("LOKI_NTT_CPU", "1");
                        } else {
                            std::env::remove_var("LOKI_NTT_CPU");
                        }
                        let calls = gpu_calls();
                        let start = Instant::now();
                        let result = run(op);
                        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                        if backend == 0 {
                            assert_eq!(gpu_calls(), calls);
                        } else if op >= 2 || domain.k >= 10 {
                            assert!(gpu_calls() > calls);
                        }
                        std::hint::black_box(result);
                        if repetition != 0 {
                            timings[backend].push(elapsed);
                        }
                    }
                }
                for samples in &mut timings {
                    samples.sort_by(f64::total_cmp);
                }
                println!(
                    "BENCH log={log} factor={} op={name} cpu_ms={:.3} gpu_ms={:.3} samples=3",
                    1 << factor_log,
                    timings[0][1],
                    timings[1][1]
                );
            }
        }
    }
    std::env::remove_var("LOKI_NTT_CPU");
}

#[test]
fn gpu_ntt_reject_wrong_root() {
    let mut values = vec![Fq::ONE; 1 << 10];
    let original = values.clone();
    assert!(loki_cuda_ntt::fft(&mut values, Fq::ONE, false).is_err());
    assert_eq!(values, original);
    assert!(loki_cuda_ntt::fft(&mut values[..1000], Fq::ONE, false).is_err());
    assert_eq!(values, original);
}
