//! `proof_parse`: arbitrary bytes into the VK, verifier-parameter, SRS and
//! proof decoders of Loki and stock. Both must agree on accept/reject and on
//! whether they panic; a strictly accepted proof must be one of the honest
//! proofs (anything else is a forgery or a malleability finding).

use std::sync::{Arc, OnceLock};

use midnight_curves::{Bls12, Fq};

use crate::{
    catch,
    circuit::{set_shape, Case, Layout, Shape},
    proof::{build, lk, st, Built, Verdict, CROSS_MISMATCH},
    Input,
};

pub struct Fixture {
    pub built: Built,
    pub proofs: Vec<Vec<u8>>,
}

/// Fixed small case: k = 5, lookup, second phase, one committed + one plain
/// instance column, a single proof.
pub fn fixture() -> &'static Fixture {
    static F: OnceLock<Fixture> = OnceLock::new();
    F.get_or_init(|| {
        let shape = Shape {
            phases: true,
            lookup: true,
            cube: true,
            zero_col: true,
            nb_instance: 2,
            nb_committed: 1,
        };
        let mut input = Input::new(b"proof_parse fixture");
        let mut layout = Layout::generate(&mut input, 5, shape);
        for col in &mut layout.links {
            if col.is_empty() {
                col.push(Some((crate::circuit::C, 0)));
            }
        }
        let values = layout.values(&mut input);
        let case = Case {
            layout: Arc::new(layout),
            values: vec![Arc::new(values)],
        };
        let built = build(case).expect("fixture keygen");
        let (Ok(l), Ok(s)) = built.prove_both(7) else {
            panic!("fixture proof failed")
        };
        assert_eq!(built.verify_loki(&built.instances, &l), Verdict::Accept);
        assert_eq!(built.verify_stock(&built.instances, &s), Verdict::Accept);
        if !CROSS_MISMATCH {
            assert_eq!(built.verify_stock(&built.instances, &l), Verdict::Accept);
            assert_eq!(built.verify_loki(&built.instances, &s), Verdict::Accept);
        }
        Fixture {
            built,
            proofs: vec![l, s],
        }
    })
}

fn format(b: u8) -> (loki::utils::SerdeFormat, stock::utils::SerdeFormat) {
    if b & 1 == 0 {
        (loki::utils::SerdeFormat::RawBytes, stock::utils::SerdeFormat::RawBytes)
    } else {
        (loki::utils::SerdeFormat::Processed, stock::utils::SerdeFormat::Processed)
    }
}

fn agree<T: std::fmt::Debug, U: std::fmt::Debug>(
    what: &str,
    l: Result<Result<T, String>, String>,
    s: Result<Result<U, String>, String>,
) -> Option<(T, U)> {
    match (l, s) {
        (Err(lp), Err(_)) => {
            // Both panic: upstream behaviour, recorded but not a Loki bug.
            record_shared_panic(what, &lp);
            None
        }
        (Err(lp), Ok(_)) => panic!("{what}: Loki panicked ({lp}), stock did not"),
        (Ok(_), Err(sp)) => panic!("{what}: stock panicked ({sp}), Loki did not"),
        (Ok(Ok(a)), Ok(Ok(b))) => Some((a, b)),
        (Ok(Err(_)), Ok(Err(_))) => None,
        (Ok(a), Ok(b)) => panic!("{what}: accept/reject differs: loki={a:?} stock={b:?}"),
    }
}

/// Count shared (stock-and-Loki) panics; print the first of each kind.
fn record_shared_panic(what: &str, msg: &str) {
    use std::{collections::HashSet, sync::Mutex};
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let key = format!("{what}: {}", msg.split(':').next().unwrap_or(msg));
    if SEEN.get_or_init(Default::default).lock().unwrap().insert(key.clone()) {
        eprintln!("SHARED-PANIC {key} :: {msg}");
    }
}

pub fn run(data: &[u8]) {
    crate::quiet_panics();
    let fx = fixture();
    let b = &fx.built;
    let layout = Arc::clone(&b.case.layout);
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    match mode % 6 {
        0 => {
            // Verifying key decoding.
            let (lf, sf) = format(mode >> 3);
            set_shape(layout.shape);
            let l = catch(|| {
                loki::plonk::VerifyingKey::<Fq, lk::Pcs>::read::<_, crate::proof::loki_fixture::Fixture>(
                    &mut &rest[..],
                    lf,
                )
                .map(|k| k.to_bytes(lf))
                .map_err(|e| e.to_string())
            });
            let s = catch(|| {
                stock::plonk::VerifyingKey::<Fq, st::Pcs>::read::<_, crate::proof::stock_fixture::Fixture>(
                    &mut &rest[..],
                    sf,
                )
                .map(|k| k.to_bytes(sf))
                .map_err(|e| e.to_string())
            });
            if let Some((a, c)) = agree("vk read", l, s) {
                assert!(a == c, "decoded VKs re-encode differently");
            }
        }
        1 => {
            let (lf, sf) = format(mode >> 3);
            let l = catch(|| {
                loki::poly::kzg::params::ParamsVerifierKZG::<Bls12>::read(&mut &rest[..], lf)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
            let s = catch(|| {
                stock::poly::kzg::params::ParamsVerifierKZG::<Bls12>::read(&mut &rest[..], sf)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
            agree("verifier params read", l, s);
        }
        2 => {
            // SRS decoding; k is capped so a garbage header cannot request 2^31 points.
            if rest.len() < 4 || u32::from_le_bytes(rest[..4].try_into().unwrap()) > 6 {
                return;
            }
            let (lf, sf) = format(mode >> 3);
            let l = catch(|| {
                loki::poly::kzg::params::ParamsKZG::<Bls12>::read_custom(&mut &rest[..], lf)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
            let s = catch(|| {
                stock::poly::kzg::params::ParamsKZG::<Bls12>::read_custom(&mut &rest[..], sf)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
            agree("srs read", l, s);
        }
        3 | 4 => {
            // Proof bytes: raw (3) or edits of an honest proof (4).
            let proof: Vec<u8> = if mode % 6 == 3 {
                rest.to_vec()
            } else {
                let mut p = fx.proofs[(mode as usize >> 3) & 1].clone();
                for e in rest.chunks(3) {
                    if e.len() == 3 {
                        let pos = u16::from_le_bytes([e[0], e[1]]) as usize % (p.len() + 1);
                        match e[2] {
                            0 => {
                                p.truncate(pos);
                            }
                            1 => p.insert(pos, e[0]),
                            x => {
                                if pos < p.len() {
                                    p[pos] = x;
                                }
                            }
                        }
                    }
                }
                p
            };
            // Stock's lookup permutation is randomly ordered per process, so a
            // raw input may be another valid proof; only edits of this
            // process's honest proofs are checked for forgery.
            let honest: &[Vec<u8>] = if mode % 6 == 4 { &fx.proofs } else { &[] };
            check_proof(b, honest, &proof);
        }
        _ => {
            // Proof against a garbage VK built from the fuzz bytes.
            let vk = rest.to_vec();
            let proof = &fx.proofs[(mode as usize >> 3) & 1];
            let l = catch(|| Ok::<_, String>(lk::verify(&layout, &vk, &b.lvp, &b.instances, proof)));
            let s = catch(|| Ok::<_, String>(st::verify(&layout, &vk, &b.svp, &b.instances, proof)));
            if let Some((l, s)) = agree("verify with fuzzed vk", l, s) {
                if !CROSS_MISMATCH {
                    assert_eq!(l.accepted(), s.accepted(), "verdicts differ with fuzzed VK: {l:?} {s:?}");
                }
                if l == Verdict::Accept || s == Verdict::Accept {
                    assert!(vk == b.vk, "proof accepted under a VK different from the honest one");
                }
            }
        }
    }
}

fn check_proof(b: &Built, honest: &[Vec<u8>], proof: &[u8]) {
    let layout = &b.case.layout;
    let l = catch(|| Ok::<_, String>(lk::verify(layout, &b.vk, &b.lvp, &b.instances, proof)));
    let s = catch(|| Ok::<_, String>(st::verify(layout, &b.vk, &b.svp, &b.instances, proof)));
    if let Some((l, s)) = agree("verify proof", l, s) {
        if !CROSS_MISMATCH {
            assert_eq!(l.accepted(), s.accepted(), "verdicts differ: loki={l:?} stock={s:?}");
            assert_eq!(l == Verdict::Accept, s == Verdict::Accept, "strict verdicts differ");
        }
        if (l.accepted() || s.accepted()) && !honest.is_empty() {
            assert!(
                honest.iter().any(|h| proof.starts_with(h)),
                "BUG?: accepted a proof that is not an honest proof (+ trailing bytes): len={}",
                proof.len()
            );
        }
    }
}
