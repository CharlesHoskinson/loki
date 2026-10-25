use midnight_curves::Fq;
use midnight_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, TableColumn},
    poly::Rotation,
    utils::rational::Rational,
};

#[derive(Clone)]
pub struct CpuCircuit {
    pub rows: usize,
}

#[derive(Clone, Debug)]
pub struct CpuConfig {
    advice: [Column<Advice>; 3],
    table: TableColumn,
}

impl Circuit<Fq> for CpuCircuit {
    type Config = CpuConfig;
    type FloorPlanner = SimpleFloorPlanner;
    #[cfg(feature = "circuit-params")]
    type Params = ();

    fn without_witnesses(&self) -> Self {
        self.clone()
    }

    fn configure(meta: &mut ConstraintSystem<Fq>) -> CpuConfig {
        let advice = [(); 3].map(|_| meta.advice_column());
        for column in advice {
            meta.enable_equality(column);
        }
        let table = meta.lookup_table_column();
        meta.lookup("byte range", |meta| {
            vec![(meta.query_advice(advice[0], Rotation::cur()), table)]
        });
        CpuConfig { advice, table }
    }

    fn synthesize(&self, config: CpuConfig, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
        layouter.assign_table(
            || "bytes",
            |mut table| {
                for row in 0..256 {
                    table.assign_cell(
                        || "byte",
                        config.table,
                        row,
                        || Value::known(Fq::from(row as u64)),
                    )?;
                }
                Ok(())
            },
        )?;
        layouter.assign_region(
            || "lookup and copy",
            |mut region| {
                for row in 0..self.rows {
                    // Leave half the distinct table entries unused, exercising
                    // the lookup helper's leftover-table iteration order.
                    let value = Fq::from((row % 128) as u64);
                    let denominator = Fq::from((row % 31 + 1) as u64);
                    let cell = region.assign_advice(
                        || "rational byte",
                        config.advice[0],
                        row,
                        || Value::known(Rational::Rational(value * denominator, denominator)),
                    )?;
                    cell.copy_advice(|| "copy", &mut region, config.advice[1], row)?;
                    cell.copy_advice(|| "copy", &mut region, config.advice[2], row)?;
                }
                Ok(())
            },
        )
    }
}
