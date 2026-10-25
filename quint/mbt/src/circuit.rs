use midnight_curves::Fq;
use midnight_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{
        Advice, Challenge, Circuit, Column, ConstraintSystem, Constraints, Error, FirstPhase,
        SecondPhase, Selector, TableColumn,
    },
    poly::Rotation,
};

#[derive(Clone)]
pub struct RandomCircuit {
    pub profile: usize,
    pub value: Fq,
}

#[derive(Clone)]
pub struct Config {
    a: Column<Advice>,
    b: Option<Column<Advice>>,
    c: Option<Column<Advice>>,
    challenge: Option<Challenge>,
    selector: Selector,
    table: Option<TableColumn>,
}

impl Circuit<Fq> for RandomCircuit {
    type Config = Config;
    type FloorPlanner = SimpleFloorPlanner;
    type Params = usize;

    fn without_witnesses(&self) -> Self {
        self.clone()
    }
    fn params(&self) -> usize {
        self.profile
    }
    fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
        Self::configure_with_params(meta, 0)
    }

    fn configure_with_params(meta: &mut ConstraintSystem<Fq>, profile: usize) -> Config {
        let a = meta.advice_column();
        let instance = meta.instance_column();
        let selector = if profile == 3 {
            meta.complex_selector()
        } else {
            meta.selector()
        };
        let b = (profile > 0).then(|| meta.advice_column());
        let c = (profile >= 2).then(|| meta.advice_column_in(SecondPhase));
        let challenge = (profile >= 2).then(|| meta.challenge_usable_after(FirstPhase));
        meta.create_gate("public input", |meta| {
            let a = meta.query_advice(a, Rotation::cur());
            let i = meta.query_instance(instance, Rotation::cur());
            Constraints::with_selector(selector, vec![a - i])
        });
        if let Some(b) = b {
            meta.enable_equality(a);
            meta.enable_equality(b);
            meta.enable_equality(instance);
            meta.create_gate("copy", |meta| {
                let a = meta.query_advice(a, Rotation::cur());
                let b = meta.query_advice(b, Rotation::cur());
                if profile == 3 {
                    Constraints::with_additive_selector(selector, vec![b - a])
                } else {
                    Constraints::with_selector(selector, vec![b - a])
                }
            });
        }
        if let (Some(c), Some(challenge)) = (c, challenge) {
            meta.create_gate("phase challenge", |meta| {
                let a = meta.query_advice(a, Rotation::cur());
                let c = meta.query_advice(c, Rotation::cur());
                let r = meta.query_challenge(challenge);
                Constraints::with_selector(selector, vec![c - r * a])
            });
        }
        let table = (profile > 0).then(|| {
            let table = meta.lookup_table_column();
            meta.lookup("range", |meta| {
                vec![(meta.query_advice(a, Rotation::cur()), table)]
            });
            table
        });
        Config {
            a,
            b,
            c,
            challenge,
            selector,
            table,
        }
    }

    fn synthesize(&self, config: Config, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
        let r = config.challenge.map(|c| layouter.get_challenge(c));
        layouter.assign_region(
            || "random public circuit",
            |mut region| {
                config.selector.enable(&mut region, 0)?;
                region.assign_advice(|| "a", config.a, 0, || Value::known(self.value))?;
                if let Some(b) = config.b {
                    region.assign_advice(|| "b", b, 0, || Value::known(self.value))?;
                }
                if let Some(c) = config.c {
                    region.assign_advice(|| "c", c, 0, || r.unwrap().map(|r| r * self.value))?;
                }
                Ok(())
            },
        )?;
        if let Some(table) = config.table {
            layouter.assign_table(
                || "range",
                |mut region| {
                    for i in 0..8 {
                        region.assign_cell(
                            || "entry",
                            table,
                            i,
                            || Value::known(Fq::from(i as u64)),
                        )?;
                    }
                    Ok(())
                },
            )?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MixedCircuit {
    pub values: [Fq; 2],
}
#[derive(Clone)]
pub struct MixedConfig {
    advice: [Column<Advice>; 2],
    selector: Selector,
}
impl Circuit<Fq> for MixedCircuit {
    type Config = MixedConfig;
    type FloorPlanner = SimpleFloorPlanner;
    type Params = ();
    fn without_witnesses(&self) -> Self {
        self.clone()
    }
    fn configure(meta: &mut ConstraintSystem<Fq>) -> MixedConfig {
        let advice = [meta.advice_column(), meta.advice_column()];
        let instances = [meta.instance_column(), meta.instance_column()];
        let selector = meta.selector();
        meta.create_gate("two public columns", |meta| {
            let constraints: Vec<_> = advice
                .iter()
                .zip(instances)
                .map(|(a, i)| {
                    meta.query_advice(*a, Rotation::cur()) - meta.query_instance(i, Rotation::cur())
                })
                .collect();
            Constraints::with_selector(selector, constraints)
        });
        MixedConfig { advice, selector }
    }
    fn synthesize(
        &self,
        config: MixedConfig,
        mut layouter: impl Layouter<Fq>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "two public columns",
            |mut region| {
                config.selector.enable(&mut region, 0)?;
                for (column, value) in config.advice.iter().zip(self.values) {
                    region.assign_advice(|| "public value", *column, 0, || Value::known(value))?;
                }
                Ok(())
            },
        )
    }
}
