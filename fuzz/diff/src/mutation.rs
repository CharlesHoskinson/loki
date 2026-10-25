//! Witness-mutation soundness harness, ported from Moriarty
//! (`crates/mori-backend/src/qa_mutation.rs`) to both Loki and stock.
//!
//! A wrapper circuit runs the inner circuit's own floor planner over an
//! `Assignment` wrapper that records every advice cell and, when a plan is
//! installed, substitutes the planned value for the planned cell. Only the
//! written cell changes (downstream witness values are not recomputed), so it
//! is a true single-cell perturbation. Unlike the original (MockProver only),
//! the same wrapper also drives the real `create_proof`, so a mutated witness
//! can be proven and handed to both verifiers.
//!
//! Differences from the original: no `circuit-params` (`type Params`), the
//! plan state is shared by both crates (it is crate-neutral), and the number
//! of planned cells actually substituted is counted, so a run in which the
//! plan was silently not applied (e.g. synthesis on another thread) is
//! detectable instead of being reported as a soundness result.

use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

use midnight_curves::Fq;

/// One recorded advice assignment.
#[derive(Clone, Debug)]
pub struct AdviceCell {
    pub column: usize,
    pub row: usize,
    pub region: Arc<str>,
    pub value: Fq,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MutationKind {
    PlusOne,
    MinusOne,
    Zero,
    Random,
    Swap { column: usize, row: usize },
}

#[derive(Default)]
pub struct State {
    pub plan: BTreeMap<(usize, usize), Fq>,
    pub record: bool,
    pub recorded: BTreeMap<(usize, usize), AdviceCell>,
    pub applied: usize,
    pub region: String,
}

thread_local! {
    pub static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

/// RAII installation of a mutation plan on this thread.
pub struct StateGuard;

impl StateGuard {
    pub fn install(plan: BTreeMap<(usize, usize), Fq>, record: bool) -> Self {
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                plan,
                record,
                ..State::default()
            })
        });
        StateGuard
    }

    pub fn take_recorded(&self) -> BTreeMap<(usize, usize), AdviceCell> {
        with_state(|st| std::mem::take(&mut st.recorded)).unwrap_or_default()
    }

    /// Planned substitutions performed since installation.
    pub fn applied(&self) -> usize {
        with_state(|st| st.applied).unwrap_or(0)
    }
}

impl Drop for StateGuard {
    fn drop(&mut self) {
        STATE.with(|s| *s.borrow_mut() = None);
    }
}

/// Record / substitute one advice value.
pub fn intercept(column: usize, row: usize, v: Fq) -> Fq {
    with_state(|st| {
        if st.record {
            st.recorded.insert(
                (column, row),
                AdviceCell {
                    column,
                    row,
                    region: Arc::from(st.region.as_str()),
                    value: v,
                },
            );
        }
        match st.plan.get(&(column, row)) {
            Some(new) => {
                st.applied += 1;
                *new
            }
            None => v,
        }
    })
    .unwrap_or(v)
}

/// Prefix of `record_cells` errors caused by a MockProver panic.
pub const MOCK_PANIC: &str = "MockProver panicked: ";

#[derive(Clone, Debug)]
pub struct Accepted {
    pub column: usize,
    pub row: usize,
    pub kind: MutationKind,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub cells_total: usize,
    pub mutations_run: usize,
    pub noop_skipped: usize,
    pub rejected: usize,
    pub accepted: Vec<Accepted>,
    pub accepted_swaps: Vec<Accepted>,
}

/// Wrapper circuit, planner and assignment for one proofs crate.
#[macro_export]
macro_rules! mutation_backend {
    ($module:ident, $proofs:ident) => {
        pub mod $module {
            use std::collections::BTreeMap;

            use ff::Field;
            use midnight_curves::Fq;
            use rand_chacha::ChaCha20Rng;
            use rand_core::{RngCore, SeedableRng};
            use $proofs::{
                circuit::{layouter::SyncDeps, Layouter, Value},
                dev::MockProver,
                plonk::{
                    Advice, Any, Assignment, Challenge, Circuit, Column, ConstraintSystem, Error,
                    Fixed, FloorPlanner, Instance, Selector,
                },
                utils::rational::Rational,
            };

            use $crate::mutation::{
                intercept, with_state, Accepted, MutationKind, Report, StateGuard,
            };

            /// Wrapper circuit that lets the harness observe and override advice cells.
            pub struct Mutated<'a, C> {
                pub inner: &'a C,
            }

            pub struct MutPlanner<P>(std::marker::PhantomData<P>);

            impl<P: FloorPlanner> FloorPlanner for MutPlanner<P> {
                fn synthesize<F: Field, CS: Assignment<F> + SyncDeps, C: Circuit<F>>(
                    cs: &mut CS,
                    circuit: &C,
                    config: C::Config,
                    constants: Vec<Column<Fixed>>,
                ) -> Result<(), Error> {
                    let mut wrapped = MutAssign { inner: cs };
                    P::synthesize(&mut wrapped, circuit, config, constants)
                }
            }

            impl<C: Circuit<Fq>> Circuit<Fq> for Mutated<'_, C> {
                type Config = C::Config;
                type FloorPlanner = MutPlanner<C::FloorPlanner>;

                fn without_witnesses(&self) -> Self {
                    unreachable!("Mutated is only used with a concrete witness")
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Self::Config {
                    C::configure(meta)
                }

                fn synthesize(
                    &self,
                    config: Self::Config,
                    layouter: impl Layouter<Fq>,
                ) -> Result<(), Error> {
                    self.inner.synthesize(config, layouter)
                }
            }

            struct MutAssign<'a, CS> {
                inner: &'a mut CS,
            }

            fn as_fq<F: Field>(v: F) -> Option<Fq> {
                (&v as &dyn std::any::Any).downcast_ref::<Fq>().copied()
            }

            fn from_fq<F: Field>(v: Fq) -> F {
                *(&v as &dyn std::any::Any).downcast_ref::<F>().unwrap()
            }

            impl<F: Field, CS: Assignment<F>> Assignment<F> for MutAssign<'_, CS> {
                fn enter_region<NR, N>(&mut self, name_fn: N)
                where
                    NR: Into<String>,
                    N: FnOnce() -> NR,
                {
                    let name: String = name_fn().into();
                    with_state(|st| st.region = name.clone());
                    self.inner.enter_region(move || name);
                }

                fn annotate_column<A, AR>(&mut self, annotation: A, column: Column<Any>)
                where
                    A: FnOnce() -> AR,
                    AR: Into<String>,
                {
                    self.inner.annotate_column(annotation, column);
                }

                fn exit_region(&mut self) {
                    self.inner.exit_region();
                }

                fn enable_selector<A, AR>(
                    &mut self,
                    annotation: A,
                    selector: &Selector,
                    row: usize,
                ) -> Result<(), Error>
                where
                    A: FnOnce() -> AR,
                    AR: Into<String>,
                {
                    self.inner.enable_selector(annotation, selector, row)
                }

                fn query_instance(
                    &self,
                    column: Column<Instance>,
                    row: usize,
                ) -> Result<Value<F>, Error> {
                    self.inner.query_instance(column, row)
                }

                fn assign_advice<V, VR, A, AR>(
                    &mut self,
                    annotation: A,
                    column: Column<Advice>,
                    row: usize,
                    to: V,
                ) -> Result<(), Error>
                where
                    V: FnOnce() -> Value<VR>,
                    VR: Into<Rational<F>>,
                    A: FnOnce() -> AR,
                    AR: Into<String>,
                {
                    let col = column.index();
                    let value: Value<F> = to().map(|v| v.into()).evaluate();
                    let value = value.map(|v| match as_fq(v) {
                        Some(x) => from_fq::<F>(intercept(col, row, x)),
                        None => v,
                    });
                    self.inner.assign_advice(annotation, column, row, move || value)
                }

                fn assign_fixed<V, VR, A, AR>(
                    &mut self,
                    annotation: A,
                    column: Column<Fixed>,
                    row: usize,
                    to: V,
                ) -> Result<(), Error>
                where
                    V: FnOnce() -> Value<VR>,
                    VR: Into<Rational<F>>,
                    A: FnOnce() -> AR,
                    AR: Into<String>,
                {
                    self.inner.assign_fixed(annotation, column, row, to)
                }

                fn copy(
                    &mut self,
                    left_column: Column<Any>,
                    left_row: usize,
                    right_column: Column<Any>,
                    right_row: usize,
                ) -> Result<(), Error> {
                    self.inner.copy(left_column, left_row, right_column, right_row)
                }

                fn fill_from_row(
                    &mut self,
                    column: Column<Fixed>,
                    row: usize,
                    to: Value<Rational<F>>,
                ) -> Result<(), Error> {
                    self.inner.fill_from_row(column, row, to)
                }

                fn get_challenge(&self, challenge: Challenge) -> Value<F> {
                    self.inner.get_challenge(challenge)
                }

                fn push_namespace<NR, N>(&mut self, name_fn: N)
                where
                    NR: Into<String>,
                    N: FnOnce() -> NR,
                {
                    self.inner.push_namespace(name_fn);
                }

                fn pop_namespace(&mut self, gadget_name: Option<String>) {
                    self.inner.pop_namespace(gadget_name);
                }
            }

            /// MockProver verdict on the circuit with `plan` substituted.
            /// Returns (accepted, substitutions applied).
            pub fn mock<C: Circuit<Fq>>(
                circuit: &C,
                instance: &[Vec<Fq>],
                plan: BTreeMap<(usize, usize), Fq>,
            ) -> (bool, usize) {
                let guard = StateGuard::install(plan, false);
                let wrapped = Mutated { inner: circuit };
                // MockProver picks its own k and can panic on small tables
                // (upstream); callers treat that as "no oracle".
                let ok = $crate::catch(|| match MockProver::<Fq>::run(&wrapped, instance.to_vec()) {
                    Ok(prover) => prover.verify().is_ok(),
                    Err(_) => false,
                });
                match ok {
                    Ok(ok) => (ok, guard.applied()),
                    Err(_) => (false, usize::MAX),
                }
            }

            /// Every advice cell the unmutated witness assigns (MockProver must accept it).
            pub fn record_cells<C: Circuit<Fq>>(
                circuit: &C,
                instance: &[Vec<Fq>],
            ) -> Result<Vec<$crate::mutation::AdviceCell>, String> {
                let guard = StateGuard::install(BTreeMap::new(), true);
                let wrapped = Mutated { inner: circuit };
                $crate::catch(|| -> Result<(), String> {
                    let prover = MockProver::<Fq>::run(&wrapped, instance.to_vec())
                        .map_err(|e| format!("valid witness synthesis error: {e:?}"))?;
                    prover
                        .verify()
                        .map_err(|e| format!("valid witness not accepted: {e:?}"))
                })
                .map_err(|p| format!("{}{p}", $crate::mutation::MOCK_PANIC))??;
                Ok(guard.take_recorded().into_values().collect())
            }

            /// The original `run_suite` over every recorded cell (PlusOne,
            /// MinusOne, Zero, Random) plus `swaps` swaps.
            pub fn run_suite<C: Circuit<Fq>>(
                circuit: &C,
                instance: &[Vec<Fq>],
                seed: u64,
                swaps: usize,
            ) -> Result<Report, String> {
                let cells = record_cells(circuit, instance)?;
                let mut report = Report {
                    cells_total: cells.len(),
                    ..Report::default()
                };
                let mut rng = ChaCha20Rng::seed_from_u64(seed);
                for cell in &cells {
                    let random = Fq::random(&mut rng);
                    for (kind, new) in [
                        (MutationKind::PlusOne, cell.value + Fq::ONE),
                        (MutationKind::MinusOne, cell.value - Fq::ONE),
                        (MutationKind::Zero, Fq::ZERO),
                        (MutationKind::Random, random),
                    ] {
                        if new == cell.value {
                            report.noop_skipped += 1;
                            continue;
                        }
                        let (ok, applied) = mock(
                            circuit,
                            instance,
                            BTreeMap::from([((cell.column, cell.row), new)]),
                        );
                        assert!(applied >= 1 && applied != usize::MAX, "mutation plan not applied");
                        report.mutations_run += 1;
                        if ok {
                            report.accepted.push(Accepted {
                                column: cell.column,
                                row: cell.row,
                                kind,
                            });
                        } else {
                            report.rejected += 1;
                        }
                    }
                }
                let mut done = 0;
                let mut attempts = 0;
                while done < swaps && cells.len() > 1 && attempts < swaps * 50 {
                    attempts += 1;
                    let a = &cells[rng.next_u64() as usize % cells.len()];
                    let b = &cells[rng.next_u64() as usize % cells.len()];
                    if a.value == b.value {
                        continue;
                    }
                    done += 1;
                    let plan = BTreeMap::from([
                        ((a.column, a.row), b.value),
                        ((b.column, b.row), a.value),
                    ]);
                    let (ok, _) = mock(circuit, instance, plan);
                    report.mutations_run += 1;
                    if ok {
                        report.accepted_swaps.push(Accepted {
                            column: a.column,
                            row: a.row,
                            kind: MutationKind::Swap {
                                column: b.column,
                                row: b.row,
                            },
                        });
                    } else {
                        report.rejected += 1;
                    }
                }
                Ok(report)
            }
        }
    };
}
