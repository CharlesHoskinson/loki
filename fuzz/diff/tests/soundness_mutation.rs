//! Port of Moriarty's witness-mutation soundness test
//! (`crates/mori-backend/tests/soundness_mutation.rs`) to Loki and stock.
//!
//! The Moriarty fixtures are Sherlock machine circuits built on
//! midnight-zk-stdlib; they are not dependencies of this crate, so the port
//! keeps the oracle's positive control (`Leaky`) and runs the full mutation
//! suite on generated fuzz circuits instead, requiring (1) identical
//! MockProver verdicts for every mutation in Loki and stock, and (2) for a
//! sample of mutations, that real proofs of the mutated witness are rejected
//! by both verifiers exactly when MockProver rejects the mutant.

use std::{collections::BTreeSet, sync::Arc};

use loki_diff::{
    circuit::{set_shape, Case},
    mutation::Report,
    proof, Input,
};

const SEED: u64 = 0x5EED_50DA_7E57;

macro_rules! leaky {
    ($module:ident, $proofs:ident, $mutation:path) => {
        mod $module {
            use midnight_curves::Fq;
            use $mutation as mutation;
            use $proofs::{
                circuit::{Layouter, SimpleFloorPlanner, Value},
                plonk::{Advice, Circuit, Column, ConstraintSystem, Constraints, Error, Expression, Selector},
                poly::Rotation,
            };

            #[derive(Clone)]
            pub struct Cfg {
                a: Column<Advice>,
                b: Column<Advice>,
                free: Column<Advice>,
                q: Selector,
            }

            /// Proves `a * b = 6` and assigns one cell, `free`, that no
            /// constraint touches. The oracle must flag exactly that cell.
            pub struct Leaky;

            impl Circuit<Fq> for Leaky {
                type Config = Cfg;
                type FloorPlanner = SimpleFloorPlanner;

                fn without_witnesses(&self) -> Self {
                    Leaky
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Cfg {
                    let a = meta.advice_column();
                    let b = meta.advice_column();
                    let free = meta.advice_column();
                    let q = meta.selector();
                    meta.create_gate("a*b=6", |meta| {
                        let a = meta.query_advice(a, Rotation::cur());
                        let b = meta.query_advice(b, Rotation::cur());
                        Constraints::with_selector(q, vec![a * b - Expression::Constant(Fq::from(6u64))])
                    });
                    Cfg { a, b, free, q }
                }

                fn synthesize(&self, cfg: Cfg, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
                    layouter.assign_region(
                        || "leaky",
                        |mut region| {
                            cfg.q.enable(&mut region, 0)?;
                            region.assign_advice(|| "a", cfg.a, 0, || Value::known(Fq::from(2u64)))?;
                            region.assign_advice(|| "b", cfg.b, 0, || Value::known(Fq::from(3u64)))?;
                            region.assign_advice(|| "free", cfg.free, 0, || Value::known(Fq::from(99u64)))?;
                            Ok(())
                        },
                    )
                }
            }

            pub fn suite() -> loki_diff::mutation::Report {
                mutation::run_suite(&Leaky, &[], 1, 0).expect("valid witness")
            }
        }
    };
}

leaky!(loki_leaky, loki, loki_diff::proof::loki_mutation);
leaky!(stock_leaky, stock, loki_diff::proof::stock_mutation);

fn check_control(report: &Report) {
    assert_eq!(report.cells_total, 3);
    assert!(
        report.accepted.iter().all(|a| a.column == 2 && a.row == 0),
        "unexpected accepted mutations: {:?}",
        report.accepted
    );
    assert_eq!(report.accepted.len(), 4, "{report:?}");
    assert!(report.rejected >= 8, "{report:?}");
}

#[test]
fn oracle_flags_an_unconstrained_cell_and_only_it() {
    check_control(&loki_leaky::suite());
    check_control(&stock_leaky::suite());
}

fn accepted_set(r: &Report) -> BTreeSet<(usize, usize, String)> {
    r.accepted
        .iter()
        .map(|a| (a.column, a.row, format!("{:?}", a.kind)))
        .collect()
}

/// Full single-cell suite on generated circuits: Loki and stock MockProver
/// accept exactly the same mutants.
#[test]
fn fuzz_circuit_mutation_verdicts_match() {
    let mut total = 0;
    let mut accepted = 0;
    for i in 0..12u64 {
        let bytes: Vec<u8> = (SEED + i).to_le_bytes().repeat(16);
        let case = Case::generate(&mut Input::new(&bytes));
        let layout = case.layout.clone();
        let instances = layout.instances(&case.values[0]);
        set_shape(layout.shape);
        let lc = proof::loki_fixture::Fixture {
            layout: layout.clone(),
            values: Some(case.values[0].clone()),
        };
        let sc = proof::stock_fixture::Fixture {
            layout: layout.clone(),
            values: Some(case.values[0].clone()),
        };
        let lr = proof::loki_mutation::run_suite(&lc, &instances, SEED, 8).expect("loki valid witness");
        let sr = proof::stock_mutation::run_suite(&sc, &instances, SEED, 8).expect("stock valid witness");
        assert_eq!(accepted_set(&lr), accepted_set(&sr), "case {i}: accepted mutants differ");
        assert_eq!(lr.rejected, sr.rejected, "case {i}");
        assert!(lr.mutations_run > 0);
        total += lr.mutations_run;
        accepted += lr.accepted.len();
    }
    eprintln!("mutation suite: {total} mutations, {accepted} accepted (unconstrained cells)");
}

/// Real proofs of mutated witnesses: both verifiers reject exactly the
/// mutants MockProver rejects (the `soundness` fuzz target's witness branch).
#[test]
fn mutated_witness_proofs_are_rejected() {
    for i in 0..10u64 {
        let bytes: Vec<u8> = (SEED ^ (i << 8)).to_le_bytes().repeat(8);
        let mut input = Input::new(&bytes);
        let case = Case::generate(&mut input);
        if case.layout.k > 6 {
            continue;
        }
        // Force the witness-mutation branch.
        let mut tail = vec![0u8];
        tail.extend((0..64).map(|j| (i as u8).wrapping_mul(31).wrapping_add(j)));
        proof::soundness_case(Case { layout: Arc::clone(&case.layout), values: case.values.clone() }, i, &mut Input::new(&tail));
    }
}
