//! Random small PLONK circuits described by fuzz bytes, expanded against both
//! `loki` and `stock` by one macro so the two backends see identical code.
//!
//! Each row has a standard gate `qm*a*b + ql*a + qr*b + qc - qo*c = 0`;
//! optional features: a selector-gated cube gate `s*(a^3 - b)` (raises the
//! quotient degree), an 8-bit-style table lookup `ql*a in T`, a second-phase
//! column `d = a + theta*c` under a challenge, an all-zero equality column,
//! copy constraints between cells, and up to two instance columns (the first
//! `nb_committed` of which are committed instances).

use std::{cell::RefCell, sync::Arc};

use ff::Field;
use midnight_curves::Fq;

use crate::Input;

/// Constraint-system shape (what `configure` sees).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub phases: bool,
    pub lookup: bool,
    pub cube: bool,
    pub zero_col: bool,
    pub nb_instance: usize,
    pub nb_committed: usize,
}

/// Advice columns: 0 = a, 1 = b, 2 = c (first phase), 3 = d (second phase).
pub const A: usize = 0;
pub const B: usize = 1;
pub const C: usize = 2;

/// Fixed assignment and wiring, shared by every proof of one VK.
#[derive(Clone, Debug)]
pub struct Layout {
    pub k: u32,
    pub shape: Shape,
    pub rows: Vec<RowFixed>,
    pub table: Vec<u64>,
    /// `(instance column, instance row) <- (advice column, row)`; per column
    /// the instance rows are 0..len.
    pub links: Vec<Vec<Option<(usize, usize)>>>,
}

#[derive(Clone, Debug)]
pub struct RowFixed {
    pub qm: Fq,
    pub ql: Fq,
    pub qr: Fq,
    pub qc: Fq,
    pub qo: Fq,
    pub qd: Fq,
    pub cube: bool,
    pub lookup: bool,
    /// Copy sources for a and b: `(column, row)` with row < this row, or
    /// `(A, this row)` for b.
    pub a_src: Option<(usize, usize)>,
    pub b_src: Option<(usize, usize)>,
}

/// Per-proof free inputs.
#[derive(Clone, Debug)]
pub struct Values {
    pub a: Vec<Fq>,
    pub b: Vec<Fq>,
    /// Free c values for rows where `qo == 0`.
    pub c: Vec<Fq>,
    /// Values of unlinked instance cells.
    pub free_instance: Vec<Vec<Fq>>,
}

thread_local! {
    static SHAPE: RefCell<Option<Shape>> = const { RefCell::new(None) };
}

/// Install the shape `configure` will use on this thread.
pub fn set_shape(shape: Shape) {
    SHAPE.with(|s| *s.borrow_mut() = Some(shape));
}

pub fn shape() -> Shape {
    SHAPE.with(|s| s.borrow().expect("circuit shape not installed on this thread"))
}

fn coeff(input: &mut Input) -> Fq {
    match input.u8() % 6 {
        0 => Fq::ZERO,
        1 => Fq::ONE,
        2 => -Fq::ONE,
        3 => Fq::from(input.u8() as u64),
        _ => input.scalar(),
    }
}

impl Layout {
    pub fn usable_rows(k: u32) -> usize {
        // Generous blinding margin for the queried rotations used here.
        (1usize << k).saturating_sub(12)
    }

    pub fn generate(input: &mut Input, k: u32, shape: Shape) -> Self {
        let max = Self::usable_rows(k).max(1);
        let nrows = input.range(1, max.min(64));
        let table_len = input.range(1, max.min(40));
        let mut rows: Vec<RowFixed> = Vec::with_capacity(nrows);
        for r in 0..nrows {
            let cube = shape.cube && input.u8() % 4 == 0;
            let lookup = shape.lookup && input.u8() % 3 == 0;
            let pick = |input: &mut Input| -> Option<(usize, usize)> {
                (r > 0 && input.u8() % 3 == 0).then(|| (input.range(A, C), input.range(0, r - 1)))
            };
            let a_src = if lookup { None } else { pick(input) };
            let b_src = if cube {
                None
            } else if input.u8() % 8 == 0 {
                Some((A, r))
            } else {
                pick(input)
            };
            // qo == 0 makes the whole row gate vacuous (free a/b/c cells);
            // otherwise c is solved from the other terms.
            let qo = if input.u8() % 8 == 0 { Fq::ZERO } else { coeff(input) };
            let mut q = [coeff(input), coeff(input), coeff(input), coeff(input)];
            if qo.is_zero_vartime() {
                q = [Fq::ZERO; 4];
            }
            rows.push(RowFixed {
                qm: q[0],
                ql: q[1],
                qr: q[2],
                qc: q[3],
                qo,
                qd: if shape.phases { coeff(input) } else { Fq::ZERO },
                cube,
                lookup,
                a_src,
                b_src,
            });
        }
        let links = (0..shape.nb_instance)
            .map(|_| {
                (0..input.range(0, 3))
                    .map(|_| {
                        (input.u8() % 4 != 0)
                            .then(|| (input.range(A, C), input.range(0, nrows - 1)))
                    })
                    .collect()
            })
            .collect();
        Layout {
            k,
            shape,
            rows,
            table: (0..table_len as u64).collect(),
            links,
        }
    }

    pub fn values(&self, input: &mut Input) -> Values {
        let n = self.rows.len();
        let free = |input: &mut Input| match input.u8() % 4 {
            0 => Fq::from(input.u8() as u64),
            1 => Fq::ZERO,
            _ => input.scalar(),
        };
        Values {
            a: (0..n)
                .map(|r| {
                    if self.rows[r].lookup {
                        Fq::from(self.table[input.range(0, self.table.len() - 1)])
                    } else {
                        free(input)
                    }
                })
                .collect(),
            b: (0..n).map(|_| free(input)).collect(),
            c: (0..n).map(|_| free(input)).collect(),
            free_instance: self
                .links
                .iter()
                .map(|col| col.iter().map(|_| free(input)).collect())
                .collect(),
        }
    }

    /// The full first-phase witness `[a, b, c]` per row (copies applied).
    pub fn witness(&self, v: &Values) -> Vec<[Fq; 3]> {
        let mut out: Vec<[Fq; 3]> = Vec::with_capacity(self.rows.len());
        for (r, row) in self.rows.iter().enumerate() {
            let a = match row.a_src {
                Some((col, src)) => out[src][col],
                None => v.a[r],
            };
            let b = if row.cube {
                a * a * a
            } else {
                match row.b_src {
                    Some((A, src)) if src == r => a,
                    Some((col, src)) => out[src][col],
                    None => v.b[r],
                }
            };
            let lhs = row.qm * a * b + row.ql * a + row.qr * b + row.qc;
            let c = if row.qo.is_zero_vartime() {
                v.c[r]
            } else {
                lhs * row.qo.invert().unwrap()
            };
            out.push([a, b, c]);
        }
        out
    }

    /// Instance columns (all values, committed columns first).
    pub fn instances(&self, v: &Values) -> Vec<Vec<Fq>> {
        let w = self.witness(v);
        self.links
            .iter()
            .zip(&v.free_instance)
            .map(|(col, free)| {
                col.iter()
                    .zip(free)
                    .map(|(link, f)| link.map_or(*f, |(c, r)| w[r][c]))
                    .collect()
            })
            .collect()
    }
}

/// A complete case: one layout, one or more proofs.
#[derive(Clone, Debug)]
pub struct Case {
    pub layout: Arc<Layout>,
    pub values: Vec<Arc<Values>>,
}

impl Case {
    pub fn generate(input: &mut Input) -> Self {
        let k = match input.u8() % 16 {
            0..=6 => 4,
            7..=10 => 5,
            11 | 12 => 6,
            13 => 7,
            // GPU NTT threshold region: n = 2^9 / 2^10, extended 2^10..2^12.
            14 => 9,
            _ => 10,
        };
        let nb_instance = input.range(1, 2);
        let shape = Shape {
            phases: input.bool(),
            lookup: input.bool(),
            cube: input.bool(),
            zero_col: input.u8() % 4 == 0,
            nb_instance,
            nb_committed: input.range(0, nb_instance),
        };
        let nproofs = if input.u8() % 4 == 0 { 2 } else { 1 };
        let layout = Arc::new(Layout::generate(input, k, shape));
        let values = (0..nproofs).map(|_| Arc::new(layout.values(input))).collect();
        Case { layout, values }
    }

    /// A case with an explicit shape; every instance column gets at least
    /// one linked value.
    pub fn with_shape(input: &mut Input, k: u32, shape: Shape, nproofs: usize) -> Self {
        let mut layout = Layout::generate(input, k, shape);
        for col in &mut layout.links {
            if col.is_empty() {
                col.push(Some((C, 0)));
            }
        }
        let values = (0..nproofs).map(|_| Arc::new(layout.values(input))).collect();
        Case {
            layout: Arc::new(layout),
            values,
        }
    }

    /// The upstream instance-absorption order differs between prover and
    /// verifier exactly when several proofs mix committed and plain columns.
    pub fn order_mismatch(&self) -> bool {
        let s = self.layout.shape;
        self.values.len() >= 2 && s.nb_committed >= 1 && s.nb_instance > s.nb_committed
    }
}

#[macro_export]
macro_rules! fixture_backend {
    ($module:ident, $proofs:ident) => {
        pub mod $module {
            use std::sync::Arc;

            use ff::Field;
            use midnight_curves::Fq;
            use $proofs::{
                circuit::{AssignedCell, Cell, Layouter, SimpleFloorPlanner, Value},
                plonk::{
                    Advice, Challenge, Circuit, Column, ConstraintSystem, Constraints, Error,
                    Fixed, FirstPhase, Instance, SecondPhase, Selector, TableColumn,
                },
                poly::Rotation,
            };

            use $crate::circuit::{shape, Layout, Values, A, B, C};

            #[derive(Clone, Debug)]
            pub struct Config {
                pub adv: [Column<Advice>; 3],
                pub d: Option<Column<Advice>>,
                pub zero: Option<Column<Advice>>,
                pub q: [Column<Fixed>; 6],
                pub ql_lookup: Column<Fixed>,
                pub s_cube: Selector,
                pub theta: Option<Challenge>,
                pub table: Option<TableColumn>,
                pub inst: Vec<Column<Instance>>,
            }

            #[derive(Clone)]
            pub struct Fixture {
                pub layout: Arc<Layout>,
                pub values: Option<Arc<Values>>,
            }

            impl Circuit<Fq> for Fixture {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;

                fn without_witnesses(&self) -> Self {
                    Fixture {
                        layout: self.layout.clone(),
                        values: None,
                    }
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let s = shape();
                    let adv = [meta.advice_column(), meta.advice_column(), meta.advice_column()];
                    // Instance columns come first in the committed-instance order.
                    let inst: Vec<_> = (0..s.nb_instance).map(|_| meta.instance_column()).collect();
                    let q = [
                        meta.fixed_column(),
                        meta.fixed_column(),
                        meta.fixed_column(),
                        meta.fixed_column(),
                        meta.fixed_column(),
                        meta.fixed_column(),
                    ];
                    let ql_lookup = meta.fixed_column();
                    let s_cube = meta.selector();
                    for c in adv {
                        meta.enable_equality(c);
                    }
                    for c in &inst {
                        meta.enable_equality(*c);
                    }
                    let zero = s.zero_col.then(|| {
                        let z = meta.advice_column();
                        meta.enable_equality(z);
                        z
                    });
                    meta.create_gate("plonk", |meta| {
                        let a = meta.query_advice(adv[A], Rotation::cur());
                        let b = meta.query_advice(adv[B], Rotation::cur());
                        let c = meta.query_advice(adv[C], Rotation::cur());
                        let [qm, ql, qr, qc, qo, _] =
                            q.map(|q| meta.query_fixed(q, Rotation::cur()));
                        Constraints::without_selector(vec![
                            qm * a.clone() * b.clone() + ql * a + qr * b + qc - qo * c,
                        ])
                    });
                    if s.cube {
                        meta.create_gate("cube", |meta| {
                            let a = meta.query_advice(adv[A], Rotation::cur());
                            let b = meta.query_advice(adv[B], Rotation::cur());
                            Constraints::with_selector(
                                s_cube,
                                vec![a.clone() * a.clone() * a - b],
                            )
                        });
                    }
                    let (d, theta) = if s.phases {
                        let d = meta.advice_column_in(SecondPhase);
                        let theta = meta.challenge_usable_after(FirstPhase);
                        meta.create_gate("second phase", |meta| {
                            let a = meta.query_advice(adv[A], Rotation::cur());
                            let c = meta.query_advice(adv[C], Rotation::cur());
                            let d = meta.query_advice(d, Rotation::cur());
                            let qd = meta.query_fixed(q[5], Rotation::cur());
                            let t = meta.query_challenge(theta);
                            Constraints::without_selector(vec![qd * (d - a - t * c)])
                        });
                        (Some(d), Some(theta))
                    } else {
                        (None, None)
                    };
                    let table = s.lookup.then(|| {
                        let table = meta.lookup_table_column();
                        meta.lookup("table", |meta| {
                            let q = meta.query_fixed(ql_lookup, Rotation::cur());
                            let a = meta.query_advice(adv[A], Rotation::cur());
                            vec![(q * a, table)]
                        });
                        table
                    });
                    Config {
                        adv,
                        d,
                        zero,
                        q,
                        ql_lookup,
                        s_cube,
                        theta,
                        table,
                        inst,
                    }
                }

                fn synthesize(
                    &self,
                    config: Config,
                    mut layouter: impl Layouter<Fq>,
                ) -> Result<(), Error> {
                    let layout = &self.layout;
                    if let Some(table) = config.table {
                        layouter.assign_table(
                            || "table",
                            |mut t| {
                                for (i, v) in layout.table.iter().enumerate() {
                                    t.assign_cell(|| "t", table, i, || Value::known(Fq::from(*v)))?;
                                }
                                Ok(())
                            },
                        )?;
                    }
                    let witness = self.values.as_ref().map(|v| layout.witness(v));
                    let theta = config
                        .theta
                        .map(|t| layouter.get_challenge(t))
                        .unwrap_or(Value::unknown());
                    let cells: Vec<[Cell; 3]> = layouter.assign_region(
                        || "rows",
                        |mut region| {
                            let mut cells: Vec<[Cell; 3]> = Vec::new();
                            for (r, row) in layout.rows.iter().enumerate() {
                                let coeffs = [row.qm, row.ql, row.qr, row.qc, row.qo, row.qd];
                                for (col, v) in config.q.iter().zip(coeffs) {
                                    region.assign_fixed(|| "q", *col, r, || Value::known(v))?;
                                }
                                region.assign_fixed(
                                    || "ql",
                                    config.ql_lookup,
                                    r,
                                    || Value::known(if row.lookup { Fq::ONE } else { Fq::ZERO }),
                                )?;
                                if row.cube {
                                    config.s_cube.enable(&mut region, r)?;
                                }
                                let mut row_cells = Vec::with_capacity(3);
                                for col in [A, B, C] {
                                    let value = match &witness {
                                        Some(w) => Value::known(w[r][col]),
                                        None => Value::unknown(),
                                    };
                                    let cell: AssignedCell<Fq, Fq> = region.assign_advice(
                                        || "abc",
                                        config.adv[col],
                                        r,
                                        || value,
                                    )?;
                                    row_cells.push(cell.cell());
                                }
                                if let Some(d) = config.d {
                                    let value = match &witness {
                                        Some(w) => {
                                            let (a, c) = (w[r][A], w[r][C]);
                                            theta.map(|t| a + t * c)
                                        }
                                        None => Value::unknown(),
                                    };
                                    region.assign_advice(|| "d", d, r, || value)?;
                                }
                                if let Some((col, src)) = row.a_src {
                                    region.constrain_equal(cells[src][col], row_cells[A])?;
                                }
                                match row.b_src {
                                    Some((A, src)) if src == r => {
                                        region.constrain_equal(row_cells[A], row_cells[B])?
                                    }
                                    Some((col, src)) => {
                                        region.constrain_equal(cells[src][col], row_cells[B])?
                                    }
                                    None => {}
                                }
                                cells.push([row_cells[0], row_cells[1], row_cells[2]]);
                            }
                            if let Some(z) = config.zero {
                                // Two equal zero cells wired together; the rest stays unassigned.
                                let x = region.assign_advice(|| "z0", z, 0, || Value::known(Fq::ZERO))?;
                                let y = region.assign_advice(|| "z1", z, 1, || Value::known(Fq::ZERO))?;
                                region.constrain_equal(x.cell(), y.cell())?;
                            }
                            Ok(cells)
                        },
                    )?;
                    for (i, col) in layout.links.iter().enumerate() {
                        for (row, link) in col.iter().enumerate() {
                            if let Some((c, r)) = link {
                                layouter.constrain_instance(cells[*r][*c], config.inst[i], row)?;
                            }
                        }
                    }
                    let _ = Fq::ZERO;
                    Ok(())
                }
            }
        }
    };
}
