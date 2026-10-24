//! Shared synthetic circuit for profiling and stock-verifier compatibility.
use ff::Field;
use midnight_curves::Fq;
use midnight_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{
        Advice, Circuit, Column, ConstraintSystem, Constraints, Error, SecondPhase, Selector,
        TableColumn,
    },
    poly::Rotation,
};

#[derive(Clone, Debug)]
pub(crate) struct Config {
    a: Column<Advice>,
    b: Column<Advice>,
    c: Column<Advice>,
    table: TableColumn,
    selector: Selector,
}

#[derive(Clone)]
pub(crate) struct ProfileCircuit {
    pub(crate) rows: usize,
}

impl Circuit<Fq> for ProfileCircuit {
    type Config = Config;
    type FloorPlanner = SimpleFloorPlanner;
    #[cfg(feature = "circuit-params")]
    type Params = ();

    fn without_witnesses(&self) -> Self {
        self.clone()
    }

    fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
        let a = meta.advice_column();
        let b = meta.advice_column();
        let c = meta.advice_column_in(SecondPhase);
        meta.enable_equality(a);
        meta.enable_equality(b);
        let table = meta.lookup_table_column();
        let selector = meta.complex_selector();
        meta.lookup("range", |meta| {
            vec![(meta.query_advice(a, Rotation::cur()), table)]
        });
        meta.create_gate("square", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let c = meta.query_advice(c, Rotation::cur());
            Constraints::with_additive_selector(selector, vec![a.clone() * a - c])
        });
        Config {
            a,
            b,
            c,
            table,
            selector,
        }
    }

    fn synthesize(&self, config: Config, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
        layouter.assign_table(
            || "range table",
            |mut table| {
                for i in 0..16 {
                    table.assign_cell(
                        || "entry",
                        config.table,
                        i,
                        || Value::known(Fq::from(i as u64)),
                    )?;
                }
                Ok(())
            },
        )?;
        layouter.assign_region(
            || "squares",
            |mut region| {
                for row in 0..self.rows {
                    config.selector.enable(&mut region, row)?;
                    let value = Fq::from((row % 16) as u64);
                    let a = region.assign_advice(|| "a", config.a, row, || Value::known(value))?;
                    a.copy_advice(|| "copy", &mut region, config.b, row)?;
                    region.assign_advice(
                        || "square",
                        config.c,
                        row,
                        || Value::known(value.square()),
                    )?;
                }
                Ok(())
            },
        )
    }
}
