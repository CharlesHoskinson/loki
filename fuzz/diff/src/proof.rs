//! `proof_roundtrip` and `soundness`: random circuits proven by Loki and by
//! stock and cross-verified. Proofs, VKs and SRS cross crates only as bytes.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock},
};

use ff::Field;
use midnight_curves::{Bls12, Fq};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;

use crate::{
    circuit::{set_shape, Case, Layout, Values},
    mutation::MOCK_PANIC,
    Input,
};

crate::fixture_backend!(loki_fixture, loki);
crate::fixture_backend!(stock_fixture, stock);
crate::mutation_backend!(loki_mutation, loki);
crate::mutation_backend!(stock_mutation, stock);

/// Shared SRS bytes per k (stock writer, test-only fixed seed).
pub fn params_bytes(k: u32) -> Arc<Vec<u8>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Arc<Vec<u8>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(b) = cache.lock().unwrap().get(&k) {
        return b.clone();
    }
    let params = stock::poly::kzg::params::ParamsKZG::<Bls12>::unsafe_setup(
        k,
        ChaCha20Rng::from_seed([0x53 ^ k as u8; 32]),
    );
    let mut bytes = Vec::new();
    params
        .write_custom(&mut bytes, stock::utils::SerdeFormat::RawBytes)
        .unwrap();
    let bytes = Arc::new(bytes);
    cache.lock().unwrap().insert(k, bytes.clone());
    bytes
}

/// Verifier decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Pairing check passed and the transcript was fully consumed.
    Accept,
    /// Pairing check passed but bytes were left over.
    AcceptTrailing,
    Reject(String),
}

impl Verdict {
    pub fn accepted(&self) -> bool {
        !matches!(self, Verdict::Reject(_))
    }
}

macro_rules! proof_backend {
    ($module:ident, $proofs:ident, $fixture:ident, $mutation:ident) => {
        pub mod $module {
            use super::*;
            use blake2b_simd::State;
            use $fixture::Fixture;
            use $proofs::{
                plonk::{create_proof, keygen_pk, keygen_vk_with_k, prepare, ProvingKey, VerifyingKey},
                poly::{
                    commitment::Guard,
                    kzg::{
                        params::{ParamsKZG, ParamsVerifierKZG},
                        KZGCommitmentScheme,
                    },
                },
                transcript::{CircuitTranscript, Transcript},
                utils::SerdeFormat,
            };

            pub type Pcs = KZGCommitmentScheme<Bls12>;

            pub fn params(k: u32) -> Arc<ParamsKZG<Bls12>> {
                static CACHE: OnceLock<Mutex<HashMap<u32, Arc<ParamsKZG<Bls12>>>>> =
                    OnceLock::new();
                let cache = CACHE.get_or_init(Default::default);
                if let Some(p) = cache.lock().unwrap().get(&k) {
                    return p.clone();
                }
                let bytes = params_bytes(k);
                let p = Arc::new(
                    ParamsKZG::<Bls12>::read_custom(&mut &bytes[..], SerdeFormat::RawBytes)
                        .unwrap(),
                );
                cache.lock().unwrap().insert(k, p.clone());
                p
            }

            pub fn verifier_params_bytes(k: u32) -> Vec<u8> {
                let mut out = Vec::new();
                params(k)
                    .verifier_params()
                    .write(&mut out, SerdeFormat::RawBytes)
                    .unwrap();
                out
            }

            pub fn keygen(layout: &Arc<Layout>) -> Result<(ProvingKey<Fq, Pcs>, Vec<u8>), String> {
                set_shape(layout.shape);
                let circuit = Fixture {
                    layout: layout.clone(),
                    values: None,
                };
                let params = params(layout.k);
                let vk = keygen_vk_with_k::<_, Pcs, _>(&params, &circuit, layout.k)
                    .map_err(|e| format!("keygen_vk: {e:?}"))?;
                let pk = keygen_pk(vk, &circuit).map_err(|e| format!("keygen_pk: {e:?}"))?;
                let vk = pk.get_vk().to_bytes(SerdeFormat::RawBytes);
                Ok((pk, vk))
            }

            /// Prove all `values` in one proof. `plan` mutates proof 0's witness.
            /// Returns the proof and the number of planned substitutions applied.
            pub fn prove(
                layout: &Arc<Layout>,
                pk: &ProvingKey<Fq, Pcs>,
                values: &[Arc<Values>],
                instances: &[Vec<Vec<Fq>>],
                plan: Option<BTreeMap<(usize, usize), Fq>>,
                seed: u64,
            ) -> Result<(Vec<u8>, usize), String> {
                set_shape(layout.shape);
                let params = params(layout.k);
                let circuits: Vec<Fixture> = values
                    .iter()
                    .map(|v| Fixture {
                        layout: layout.clone(),
                        values: Some(v.clone()),
                    })
                    .collect();
                let inst: Vec<Vec<&[Fq]>> = instances
                    .iter()
                    .map(|p| p.iter().map(|c| &c[..]).collect())
                    .collect();
                let inst: Vec<&[&[Fq]]> = inst.iter().map(|p| &p[..]).collect();
                let mut transcript = CircuitTranscript::<State>::init();
                let rng = ChaCha20Rng::seed_from_u64(seed);
                let nb_committed = layout.shape.nb_committed;
                let (result, applied) = match plan {
                    None => (
                        create_proof::<Fq, Pcs, _, _>(
                            &params,
                            pk,
                            &circuits,
                            nb_committed,
                            &inst,
                            &mut transcript,
                            rng,
                        ),
                        0,
                    ),
                    Some(plan) => {
                        // Only proof 0 is mutated: wrap every circuit, but the
                        // plan is consumed by the first synthesis pass only.
                        let guard = $crate::mutation::StateGuard::install(plan, false);
                        let wrapped: Vec<_> = circuits
                            .iter()
                            .map(|c| $mutation::Mutated { inner: c })
                            .collect();
                        let r = create_proof::<Fq, Pcs, _, _>(
                            &params,
                            pk,
                            &wrapped[..1],
                            nb_committed,
                            &inst[..1],
                            &mut transcript,
                            rng,
                        );
                        (r, guard.applied())
                    }
                };
                result.map_err(|e| format!("create_proof: {e:?}"))?;
                Ok((transcript.finalize(), applied))
            }

            pub fn verify(
                layout: &Arc<Layout>,
                vk: &[u8],
                vparams: &[u8],
                instances: &[Vec<Vec<Fq>>],
                proof: &[u8],
            ) -> Verdict {
                set_shape(layout.shape);
                let reject = |e: String| Verdict::Reject(e);
                let mut reader = vk;
                let key = match VerifyingKey::<Fq, Pcs>::read::<_, Fixture>(
                    &mut reader,
                    SerdeFormat::RawBytes,
                ) {
                    Ok(k) => k,
                    Err(e) => return reject(format!("vk read: {e}")),
                };
                let vp = match ParamsVerifierKZG::<Bls12>::read(&mut &vparams[..], SerdeFormat::RawBytes) {
                    Ok(p) => p,
                    Err(e) => return reject(format!("verifier params read: {e}")),
                };
                let nc = layout.shape.nb_committed;
                let params = params(layout.k);
                let committed: Vec<Vec<_>> = instances
                    .iter()
                    .map(|p| {
                        p[..nc]
                            .iter()
                            .map(|col| {
                                $proofs::plonk::commit_to_instances::<Fq, Pcs>(
                                    &params,
                                    key.get_domain(),
                                    col,
                                )
                            })
                            .collect()
                    })
                    .collect();
                let committed: Vec<&[_]> = committed.iter().map(|c| &c[..]).collect();
                let plain: Vec<Vec<&[Fq]>> = instances
                    .iter()
                    .map(|p| p[nc..].iter().map(|c| &c[..]).collect())
                    .collect();
                let plain: Vec<&[&[Fq]]> = plain.iter().map(|p| &p[..]).collect();
                let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
                let guard = match prepare::<Fq, Pcs, _>(&key, &committed, &plain, &mut transcript) {
                    Ok(g) => g,
                    Err(e) => return reject(format!("prepare: {e:?}")),
                };
                let empty = transcript.assert_empty().is_ok();
                match guard.verify(&vp) {
                    Ok(()) if empty => Verdict::Accept,
                    Ok(()) => Verdict::AcceptTrailing,
                    Err(e) => reject(format!("pairing: {e:?}")),
                }
            }

            /// MockProver on proof `i` with an optional mutation plan.
            pub fn mock(
                layout: &Arc<Layout>,
                values: &Arc<Values>,
                instances: &[Vec<Fq>],
                plan: BTreeMap<(usize, usize), Fq>,
            ) -> (bool, usize) {
                set_shape(layout.shape);
                let circuit = Fixture {
                    layout: layout.clone(),
                    values: Some(values.clone()),
                };
                $mutation::mock(&circuit, instances, plan)
            }

            pub fn record(
                layout: &Arc<Layout>,
                values: &Arc<Values>,
                instances: &[Vec<Fq>],
            ) -> Result<Vec<$crate::mutation::AdviceCell>, String> {
                set_shape(layout.shape);
                let circuit = Fixture {
                    layout: layout.clone(),
                    values: Some(values.clone()),
                };
                $mutation::record_cells(&circuit, instances)
            }
        }
    };
}

proof_backend!(lk, loki, loki_fixture, loki_mutation);
proof_backend!(st, stock, stock_fixture, stock_mutation);

/// Cross directions are expected to fail only when the truncated-challenges
/// feature differs between the crates.
pub const CROSS_MISMATCH: bool = cfg!(feature = "truncated-loki-only");

pub struct Built {
    pub case: Case,
    pub instances: Vec<Vec<Vec<Fq>>>,
    pub loki_pk: loki::plonk::ProvingKey<Fq, lk::Pcs>,
    pub stock_pk: stock::plonk::ProvingKey<Fq, st::Pcs>,
    pub vk: Vec<u8>,
    pub lvp: Vec<u8>,
    pub svp: Vec<u8>,
}

/// Keygen in both crates; `None` when both reject the layout identically.
pub fn build(case: Case) -> Option<Built> {
    let l = lk::keygen(&case.layout);
    let s = st::keygen(&case.layout);
    let (loki_pk, lvk, stock_pk, svk) = match (l, s) {
        (Ok((lp, lv)), Ok((sp, sv))) => (lp, lv, sp, sv),
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "keygen errors differ");
            if std::env::var_os("LOKI_FUZZ_VERBOSE").is_some() {
                eprintln!("skip: {a}");
            }
            return None;
        }
        (l, s) => panic!("keygen disagreement: loki={:?} stock={:?}", l.err(), s.err()),
    };
    assert!(lvk == svk, "VK bytes differ between Loki and stock keygen");
    let lvp = lk::verifier_params_bytes(case.layout.k);
    let svp = st::verifier_params_bytes(case.layout.k);
    assert!(lvp == svp, "verifier params bytes differ");
    let instances = case.values.iter().map(|v| case.layout.instances(v)).collect();
    Some(Built {
        case,
        instances,
        loki_pk,
        stock_pk,
        vk: lvk,
        lvp,
        svp,
    })
}

impl Built {
    pub fn prove_both(&self, seed: u64) -> (Result<Vec<u8>, String>, Result<Vec<u8>, String>) {
        let l = lk::prove(&self.case.layout, &self.loki_pk, &self.case.values, &self.instances, None, seed);
        let s = st::prove(&self.case.layout, &self.stock_pk, &self.case.values, &self.instances, None, seed);
        (l.map(|p| p.0), s.map(|p| p.0))
    }

    pub fn verify_loki(&self, instances: &[Vec<Vec<Fq>>], proof: &[u8]) -> Verdict {
        lk::verify(&self.case.layout, &self.vk, &self.lvp, instances, proof)
    }

    pub fn verify_stock(&self, instances: &[Vec<Vec<Fq>>], proof: &[u8]) -> Verdict {
        st::verify(&self.case.layout, &self.vk, &self.svp, instances, proof)
    }
}

/// `proof_roundtrip` entry point.
/// With the GPU MSM feature, route every MSM to the GPU and fail on fallback.
fn gpu_setup() {
    #[cfg(feature = "cuda")]
    crate::msm::init();
}

fn gpu_check() {
    #[cfg(feature = "cuda")]
    crate::msm::gpu_watch::assert_no_fallback();
}

pub fn roundtrip(data: &[u8]) {
    gpu_setup();
    let mut input = Input::new(data);
    let case = Case::generate(&mut input);
    let seed = input.u64();
    roundtrip_case(case, seed);
    gpu_check();
}

pub fn roundtrip_case(case: Case, seed: u64) {
    let ctx = format!("{:?} k={} proofs={}", case.layout.shape, case.layout.k, case.values.len());
    let mismatch = case.order_mismatch();
    let lookups = case.layout.shape.lookup;
    let Some(b) = build(case) else { return };
    let (lp, sp) = b.prove_both(seed);
    let (lp, sp) = match (lp, sp) {
        (Ok(l), Ok(s)) => (l, s),
        (Err(l), Err(s)) => {
            assert_eq!(l, s, "prover errors differ ({ctx})");
            return;
        }
        (l, s) => panic!("prover disagreement ({ctx}): loki={:?} stock={:?}", l.err(), s.err()),
    };
    if !lookups && !CROSS_MISMATCH {
        // Without lookups stock is deterministic for a fixed rng and thread count.
        assert!(lp == sp, "no-lookup proof bytes differ ({ctx})");
    }
    let ll = b.verify_loki(&b.instances, &lp);
    let ls = b.verify_stock(&b.instances, &lp);
    let ss = b.verify_stock(&b.instances, &sp);
    let sl = b.verify_loki(&b.instances, &sp);
    if mismatch {
        // Upstream: prover absorbs instances per proof (committed, plain);
        // the verifier absorbs all committed first. Loki must match stock.
        for v in [&ll, &ls, &ss, &sl] {
            assert!(!v.accepted(), "order-mismatch case accepted?! ({ctx}) {v:?}");
        }
        return;
    }
    assert_eq!(ss, Verdict::Accept, "stock rejects its own proof ({ctx})");
    assert_eq!(ll, Verdict::Accept, "Loki rejects its own proof ({ctx})");
    if CROSS_MISMATCH {
        assert!(!ls.accepted(), "stock (untruncated) accepted a truncated Loki proof ({ctx})");
        assert!(!sl.accepted(), "Loki (truncated) accepted an untruncated stock proof ({ctx})");
    } else {
        assert_eq!(ls, Verdict::Accept, "STOCK VERIFIER REJECTS LOKI PROOF ({ctx})");
        assert_eq!(sl, Verdict::Accept, "Loki verifier rejects stock proof ({ctx})");
    }
}

/// One mutation of a valid case for the `soundness` target.
#[derive(Clone, Debug)]
pub enum Mutation {
    Witness { column: usize, row: usize, value: Fq },
    ProofByte { pos: usize, xor: u8 },
    Truncate { len: usize },
    Append { byte: u8 },
    PublicInput { proof: usize, column: usize, row: usize, delta: Fq },
    FixedCoeff { row: usize, delta: Fq },
}

/// `soundness` (negative_cases) entry point.
pub fn soundness(data: &[u8]) {
    gpu_setup();
    let mut input = Input::new(data);
    let case = Case::generate(&mut input);
    let seed = input.u64();
    soundness_case(case, seed, &mut input);
    gpu_check();
}

fn accepted_any(vs: &[&Verdict]) -> bool {
    vs.iter().any(|v| v.accepted())
}

pub fn soundness_case(case: Case, seed: u64, input: &mut Input) {
    if case.order_mismatch() {
        return; // never accepted, covered by roundtrip
    }
    let ctx = format!("{:?} k={} proofs={}", case.layout.shape, case.layout.k, case.values.len());
    let Some(b) = build(case) else { return };
    let layout = b.case.layout.clone();
    let choice = input.u8() % 6;
    match choice {
        0 => {
            // Invalid witness: one advice cell of proof 0 changed.
            let lrec = lk::record(&layout, &b.case.values[0], &b.instances[0]);
            let srec = st::record(&layout, &b.case.values[0], &b.instances[0]);
            let (cells, scells) = match (lrec, srec) {
                (Ok(l), Ok(s)) => (l, s),
                (Err(l), Err(s)) if l.starts_with(MOCK_PANIC) && s.starts_with(MOCK_PANIC) => return,
                (l, s) => panic!("valid witness: MockProver loki={:?} stock={:?} ({ctx})", l.err(), s.err()),
            };
            assert_eq!(cells.len(), scells.len(), "recorded cells differ");
            if cells.is_empty() {
                return;
            }
            let cell = &cells[input.range(0, cells.len() - 1)];
            let new = match input.u8() % 4 {
                0 => cell.value + Fq::ONE,
                1 => cell.value - Fq::ONE,
                2 => Fq::ZERO,
                _ => input.rng_scalar(),
            };
            if new == cell.value {
                return;
            }
            let plan = BTreeMap::from([((cell.column, cell.row), new)]);
            let (lmock, la) = lk::mock(&layout, &b.case.values[0], &b.instances[0], plan.clone());
            let (smock, sa) = st::mock(&layout, &b.case.values[0], &b.instances[0], plan.clone());
            assert!(la >= 1 && sa >= 1 && la != usize::MAX && sa != usize::MAX, "plan not applied in MockProver");
            assert_eq!(lmock, smock, "MockProver verdicts differ ({ctx}) cell={cell:?}");
            // Real proofs of the mutated witness (proof 0 only).
            let inst0 = &b.instances[..1];
            let lp = lk::prove(&layout, &b.loki_pk, &b.case.values[..1], inst0, Some(plan.clone()), seed);
            let sp = st::prove(&layout, &b.stock_pk, &b.case.values[..1], inst0, Some(plan), seed);
            let mut verdicts = Vec::new();
            for p in [&lp, &sp] {
                match p {
                    Ok((proof, applied)) => {
                        assert!(*applied >= 1, "plan not applied in create_proof ({ctx})");
                        verdicts.push(b.verify_loki(inst0, proof));
                        verdicts.push(b.verify_stock(inst0, proof));
                    }
                    Err(_) => verdicts.push(Verdict::Reject("prover error".into())),
                }
            }
            if lmock && !CROSS_MISMATCH {
                // MockProver accepts the mutant (unconstrained cell): the
                // proof is honest for that witness and must verify.
                for v in &verdicts {
                    assert_eq!(*v, Verdict::Accept, "completeness: mutant accepted by MockProver but proof rejected ({ctx}) cell={cell:?}");
                }
            } else if !lmock {
                for v in &verdicts {
                    assert!(!v.accepted(), "BUG: invalid witness proof ACCEPTED ({ctx}) cell={cell:?} {verdicts:?}");
                }
            }
            assert_eq!(lp.is_ok(), sp.is_ok(), "prover Ok/Err differs on mutated witness ({ctx})");
        }
        _ => {
            let (lp, sp) = b.prove_both(seed);
            let (Ok(lp), Ok(sp)) = (lp, sp) else {
                panic!("valid witness failed to prove ({ctx})")
            };
            let mut inst = b.instances.clone();
            let mut proofs = [lp, sp];
            let mut vk = b.vk.clone();
            let what;
            match choice {
                1 => {
                    let pos = input.range(0, proofs[0].len() - 1);
                    let xor = input.u8().max(1);
                    for p in &mut proofs {
                        let pos = pos.min(p.len() - 1);
                        p[pos] ^= xor;
                    }
                    what = format!("proof byte {pos} ^= {xor:#x}");
                }
                2 => {
                    let len = input.range(0, proofs[0].len() - 1);
                    for p in &mut proofs {
                        p.truncate(len.min(p.len() - 1));
                    }
                    what = format!("truncate to {len}");
                }
                3 => {
                    // Change a public input (committed or plain).
                    let p = input.range(0, inst.len() - 1);
                    let cols: Vec<usize> = (0..inst[p].len()).filter(|c| !inst[p][*c].is_empty()).collect();
                    if cols.is_empty() {
                        // Add a value to an empty instance column instead.
                        inst[p][0].push(Fq::ONE);
                        what = "extra public input".into();
                    } else {
                        let c = cols[input.range(0, cols.len() - 1)];
                        let r = input.range(0, inst[p][c].len() - 1);
                        let delta = match input.u8() % 3 { 0 => Fq::ONE, 1 => -Fq::ONE, _ => input.scalar() };
                        let delta = if delta.is_zero_vartime() { Fq::ONE } else { delta };
                        inst[p][c][r] += delta;
                        what = format!("public input p{p} c{c} r{r}");
                    }
                }
                4 => {
                    // Mismatched VK: one fixed coefficient changed.
                    let mut l = (*layout).clone();
                    let r = input.range(0, l.rows.len() - 1);
                    let delta = if input.bool() { Fq::ONE } else { input.rng_scalar() };
                    match input.u8() % 3 {
                        0 => l.rows[r].qc += delta,
                        1 => l.rows[r].qm += delta,
                        _ => l.rows[r].qo += delta,
                    }
                    let (_, other) = lk::keygen(&Arc::new(l)).expect("keygen of altered layout");
                    vk = other;
                    what = format!("vk with fixed row {r} changed");
                }
                _ => {
                    let byte = input.u8();
                    for p in &mut proofs {
                        p.push(byte);
                    }
                    what = "appended byte".into();
                }
            }
            let mut verdicts = Vec::new();
            for p in &proofs {
                verdicts.push(lk::verify(&layout, &vk, &b.lvp, &inst, p));
                verdicts.push(st::verify(&layout, &vk, &b.svp, &inst, p));
            }
            let refs: Vec<&Verdict> = verdicts.iter().collect();
            if CROSS_MISMATCH {
                // Cross directions reject by design; only forgery checks apply.
                if choice != 5 {
                    assert!(!accepted_any(&refs), "BUG: {what} ACCEPTED ({ctx}) {verdicts:?}");
                }
                return;
            }
            if choice == 5 {
                // Trailing bytes: pairing passes; only the emptiness check fails.
                for v in &verdicts {
                    assert_eq!(*v, Verdict::AcceptTrailing, "appended byte ({ctx})");
                }
            } else {
                assert!(!accepted_any(&refs), "BUG: {what} ACCEPTED ({ctx}) {verdicts:?}");
            }
            // Loki and stock verifiers agree on each proof.
            assert_eq!(verdicts[0].accepted(), verdicts[1].accepted(), "verifier disagreement ({ctx}) {what}");
            assert_eq!(verdicts[2].accepted(), verdicts[3].accepted(), "verifier disagreement ({ctx}) {what}");
        }
    }
}
