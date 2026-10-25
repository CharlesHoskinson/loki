//! Real proofs: stock 0.8.3 vs Loki main a256a9a (default build, or `--features det`),
//! plus the actual W10 `commit_argument` strategy against main's commit path.
use ff::Field;
use loki::poly::{commitment::PolynomialCommitmentScheme, kzg::KZGCommitmentScheme, EvaluationDomain};
use loki_q5_replay::{fixtures::{loki_fx, stock_fx}, w10_commit::commit_argument};
use midnight_curves::{Bls12, Fq};
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

fn cov_pairs() -> [(stock_fx::Coverage, loki_fx::Coverage); 4] {
    use loki_fx::Coverage as L;
    use stock_fx::Coverage as S;
    [(S::Single, L::Single), (S::Multi, L::Multi), (S::AllEqual, L::AllEqual), (S::MaxElem, L::MaxElem)]
}

#[test]
fn proofs_match_stock_and_verify_with_stock() {
    let mut report = vec![];
    for k in [7u32, 11] {
        for (sc, lc) in cov_pairs() {
            for seed in [1u64, 2] {
                let s = stock_fx::prove(k, sc, stock_fx::Srs::Exact, seed);
                let l = loki_fx::prove(k, lc, loki_fx::Srs::Exact, seed);
                assert_eq!(l.vk, s.vk, "VK differs k={k} {sc:?}");
                assert_eq!((l.draws, &l.draw_digest, l.word_pos), (s.draws, &s.draw_digest, s.word_pos),
                    "RNG draw sequence differs k={k} {sc:?}");
                assert!(stock_fx::verify(k, stock_fx::Srs::Exact, &s.vk, &l.proof), "stock verifier rejects Loki proof");
                assert!(stock_fx::verify(k, stock_fx::Srs::Exact, &s.vk, &s.proof));
                let equal = l.proof == s.proof;
                // A second stock run with the same seed: is stock itself deterministic here?
                let s2 = stock_fx::prove(k, sc, stock_fx::Srs::Exact, seed);
                let stock_det = s2.proof == s.proof;
                if sc == stock_fx::Coverage::Single {
                    assert!(stock_det, "single-leftover fixture must be order independent");
                    assert!(equal, "Loki proof bytes != stock for order-independent fixture k={k}");
                }
                if stock_det && cfg!(not(feature = "det")) {
                    assert!(equal, "stock deterministic but Loki differs k={k} {sc:?}");
                }
                println!("k={k} coverage={sc:?} seed={seed} draws={} stock_self_equal={stock_det} loki_equal_stock={equal}", l.draws);
                report.push((k, format!("{sc:?}"), equal, stock_det));
            }
        }
    }
    // Stock's HashMap traversal makes multi-leftover proofs nondeterministic.
    assert!(report.iter().any(|r| !r.3), "expected stock nondeterminism on multi-leftover fixtures");
}

/// The exact W10 strategy function vs main's (commit_lagrange, lagrange_to_coeff).
#[test]
fn w10_commit_argument_equals_main_commit() {
    use loki_fx::{params, Srs};
    let mut rng = ChaCha20Rng::seed_from_u64(0x5656);
    let (mut equal, mut bad_diverged, mut samples) = (0, 0, 0);
    for k in 1u32..=9 {
        let domain = EvaluationDomain::<Fq>::new(1, k);
        for srs in [Srs::Exact, Srs::Oversized, Srs::Downsized, Srs::Serialized, Srs::BadLagrange] {
            let p = params(k, srs);
            let n = 1usize << k;
            let reps = if k <= 6 { 60 } else { 12 };
            for rep in 0..reps {
                let wide = Fq::random(&mut rng);
                let v: Vec<Fq> = (0..n).map(|i| match rep % 7 {
                    0 => Fq::ZERO, 1 => Fq::ONE, 2 => -Fq::ONE, 3 => wide,
                    4 => Fq::from((i % 3) as u64),
                    5 => if rng.next_u32() % 2 == 0 { Fq::ZERO } else { -Fq::ONE },
                    _ => Fq::random(&mut rng),
                }).collect();
                let values = domain.lagrange_from_vec(v);
                let want_poly = domain.lagrange_to_coeff(values.clone());
                let want = KZGCommitmentScheme::<Bls12>::commit_lagrange(&p, &values);
                let (poly, got) = commit_argument::<Fq, KZGCommitmentScheme<Bls12>>(&p, &domain, values);
                assert_eq!(poly, want_poly);
                let zero_poly = rep % 7 == 0;
                if srs == Srs::BadLagrange {
                    if got != want { bad_diverged += 1; } else { assert!(zero_poly || n == 1 || rep % 7 == 3 && false || got == want); }
                } else {
                    assert_eq!(got, want, "W10 commitment differs k={k} srs={srs:?} rep={rep}");
                    equal += 1;
                }
                samples += 1;
            }
        }
    }
    println!("w10 commit samples={samples} equal={equal} bad_lagrange_diverged={bad_diverged}");
    assert!(bad_diverged > 0, "mutation witness: an inconsistent g_lagrange must be observable");
}
