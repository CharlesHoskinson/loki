// Expand the exact same circuit and prover/verifier code against independent
// midnight-proofs packages. No Loki types reach the stock verifier.
macro_rules! backend {
    ($module:ident, $proofs:ident) => {
        pub mod $module {
            use blake2b_simd::State;
            use ff::Field;
            use midnight_curves::{Bls12, Fq};
            use rand_chacha::ChaCha20Rng;
            use rand_core::SeedableRng;
            use std::{io::Cursor, time::Instant};
            use $proofs::{
                circuit::{Layouter, SimpleFloorPlanner, Value},
                dev::MockProver,
                plonk::{
                    create_proof, keygen_pk, keygen_vk_with_k, prepare, Advice, Challenge, Circuit,
                    Column, ConstraintSystem, Constraints, Error, FirstPhase, Fixed, Instance,
                    SecondPhase, TableColumn, VerifyingKey,
                },
                poly::{
                    commitment::Guard,
                    kzg::{
                        params::{ParamsKZG, ParamsVerifierKZG},
                        KZGCommitmentScheme,
                    },
                    Rotation,
                },
                transcript::{CircuitTranscript, Transcript},
                utils::SerdeFormat,
            };

            use crate::{Fault, Metrics, Result, Sample};

            #[derive(Clone, Debug)]
            pub struct Config {
                x: Column<Advice>,
                copy: Column<Advice>,
                late: Column<Advice>,
                active: Column<Fixed>,
                public: Column<Instance>,
                theta: Challenge,
                table: Option<TableColumn>,
            }

            #[derive(Clone)]
            pub struct Fixture<const LOOKUPS: bool> {
                pub rows: usize,
                pub fault: Fault,
                pub witness: bool,
            }

            impl<const LOOKUPS: bool> Fixture<LOOKUPS> {
                pub fn new(k: u32) -> Self {
                    Self {
                        rows: (1usize << k) - 32,
                        fault: Fault::None,
                        witness: true,
                    }
                }
            }

            impl<const LOOKUPS: bool> Circuit<Fq> for Fixture<LOOKUPS> {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;

                fn without_witnesses(&self) -> Self {
                    Self {
                        witness: false,
                        ..self.clone()
                    }
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let x = meta.advice_column();
                    let copy = meta.advice_column();
                    let late = meta.advice_column_in(SecondPhase);
                    let theta = meta.challenge_usable_after(FirstPhase);
                    let active = meta.fixed_column();
                    let public = meta.instance_column();
                    for column in [x, copy] {
                        meta.enable_equality(column);
                    }
                    meta.enable_equality(public);
                    meta.create_gate("nonlinear challenge-dependent gate", |meta| {
                        let q = meta.query_fixed(active, Rotation::cur());
                        let x = meta.query_advice(x, Rotation::cur());
                        let copied = meta.query_advice(copy, Rotation::cur());
                        let late = meta.query_advice(late, Rotation::cur());
                        let theta = meta.query_challenge(theta);
                        Constraints::without_selector(vec![
                            q * (late - x.clone() * x - theta * copied),
                        ])
                    });
                    let table = LOOKUPS.then(|| {
                        let table = meta.lookup_table_column();
                        meta.lookup("8-bit range", |meta| {
                            let q = meta.query_fixed(active, Rotation::cur());
                            let x = meta.query_advice(x, Rotation::cur());
                            vec![(q * x, table)]
                        });
                        table
                    });
                    Config {
                        x,
                        copy,
                        late,
                        active,
                        public,
                        theta,
                        table,
                    }
                }

                fn synthesize(
                    &self,
                    config: Config,
                    mut layouter: impl Layouter<Fq>,
                ) -> std::result::Result<(), Error> {
                    if let Some(column) = config.table {
                        layouter.assign_table(
                            || "duplicated 0..255",
                            |mut table| {
                                // Leave many distinct table values after matching the
                                // sorted input, exercising stock's HashMap iteration.
                                for row in 0..512 {
                                    table.assign_cell(
                                        || "range value",
                                        column,
                                        row,
                                        || Value::known(Fq::from((row % 256) as u64)),
                                    )?;
                                }
                                Ok(())
                            },
                        )?;
                    }
                    let theta = layouter.get_challenge(config.theta);
                    let first = layouter.assign_region(
                        || "dense arithmetic",
                        |mut region| {
                            let mut first = None;
                            for row in 0..self.rows {
                                region.assign_fixed(
                                    || "active",
                                    config.active,
                                    row,
                                    || Value::known(Fq::ONE),
                                )?;
                                let mut x = Fq::from(((row * 17 + 7) % 256) as u64);
                                if row == 1 && self.fault == Fault::Lookup {
                                    x = Fq::from(256);
                                }
                                let mut copied = x;
                                if row == 1 && self.fault == Fault::Permutation {
                                    copied += Fq::ONE;
                                }
                                let value = |x| {
                                    if self.witness {
                                        Value::known(x)
                                    } else {
                                        Value::unknown()
                                    }
                                };
                                let cell =
                                    region.assign_advice(|| "x", config.x, row, || value(x))?;
                                let copy = region.assign_advice(
                                    || "copy",
                                    config.copy,
                                    row,
                                    || value(copied),
                                )?;
                                region.constrain_equal(cell.cell(), copy.cell())?;
                                let mut late = theta.map(|theta| x.square() + theta * copied);
                                if row == 1 && self.fault == Fault::Gate {
                                    late = late.map(|x| x + Fq::ONE);
                                }
                                if row == 1 && self.fault == Fault::Phase {
                                    // A prover that substitutes zero for the transcript challenge.
                                    late = value(x.square());
                                }
                                region.assign_advice(
                                    || "second phase",
                                    config.late,
                                    row,
                                    || late,
                                )?;
                                if row == 0 {
                                    first = Some(cell.cell());
                                }
                            }
                            Ok(first.expect("fixture has rows"))
                        },
                    )?;
                    layouter.constrain_instance(first, config.public, 0)
                }
            }

            pub fn setup(k: u32) -> Result<Vec<u8>> {
                let params =
                    ParamsKZG::<Bls12>::unsafe_setup(k, ChaCha20Rng::from_seed([0x53; 32]));
                let mut bytes = Vec::new();
                params.write_custom(&mut bytes, SerdeFormat::RawBytes)?;
                Ok(bytes)
            }

            pub fn roundtrip_params(bytes: &[u8]) -> Result<Vec<u8>> {
                let mut reader = Cursor::new(bytes);
                let params = ParamsKZG::<Bls12>::read_custom(&mut reader, SerdeFormat::RawBytes)?;
                crate::ensure(
                    reader.position() as usize == bytes.len(),
                    "trailing SRS bytes",
                )?;
                let mut output = Vec::new();
                params.write_custom(&mut output, SerdeFormat::RawBytes)?;
                Ok(output)
            }

            pub fn prove<const LOOKUPS: bool>(k: u32, bytes: &[u8]) -> Result<Sample> {
                let params =
                    ParamsKZG::<Bls12>::read_custom(&mut &bytes[..], SerdeFormat::RawBytes)?;
                let circuit = Fixture::<LOOKUPS>::new(k);
                let start = Instant::now();
                let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(
                    &params,
                    &circuit.without_witnesses(),
                    k,
                )?;
                let pk = keygen_pk(vk, &circuit.without_witnesses())?;
                let keygen_s = start.elapsed().as_secs_f64();
                let vk = pk.get_vk().to_bytes(SerdeFormat::RawBytes);
                let mut verifier_params = Vec::new();
                params
                    .verifier_params()
                    .write(&mut verifier_params, SerdeFormat::RawBytes)?;
                let public = [Fq::from(7)];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init();
                let start = Instant::now();
                create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
                    &params,
                    &pk,
                    &[circuit],
                    0,
                    &[instances],
                    &mut transcript,
                    ChaCha20Rng::from_seed([0x42; 32]),
                )?;
                let proof = transcript.finalize();
                let prove_s = start.elapsed().as_secs_f64();
                let start = Instant::now();
                verify::<LOOKUPS>(&vk, &verifier_params, &proof, 7)?;
                let verify_s = start.elapsed().as_secs_f64();
                let metrics = Metrics {
                    k,
                    keygen_s,
                    prove_s,
                    verify_s,
                    proof_bytes: proof.len(),
                };
                Ok(Sample {
                    vk,
                    verifier_params,
                    proof,
                    metrics,
                })
            }

            pub fn verify<const LOOKUPS: bool>(
                vk: &[u8],
                params: &[u8],
                proof: &[u8],
                public: u64,
            ) -> Result<()> {
                let mut reader = Cursor::new(vk);
                let key = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<
                    _,
                    Fixture<LOOKUPS>,
                >(&mut reader, SerdeFormat::RawBytes)?;
                crate::ensure(reader.position() as usize == vk.len(), "trailing VK bytes")?;
                crate::ensure(
                    key.to_bytes(SerdeFormat::RawBytes) == vk,
                    "VK roundtrip differs",
                )?;
                let mut reader = Cursor::new(params);
                let params = ParamsVerifierKZG::<Bls12>::read(&mut reader, SerdeFormat::RawBytes)?;
                crate::ensure(
                    reader.position() as usize == reader.get_ref().len(),
                    "trailing verifier params",
                )?;
                let public = [Fq::from(public)];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
                let guard = prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
                    &key,
                    &[&[]],
                    &[instances],
                    &mut transcript,
                )?;
                transcript.assert_empty()?;
                guard.verify(&params).map_err(|error| {
                    std::io::Error::other(format!("pairing verification: {error:?}"))
                })?;
                Ok(())
            }

            pub fn negative_controls() -> Result<()> {
                for fault in [
                    Fault::None,
                    Fault::Gate,
                    Fault::Permutation,
                    Fault::Lookup,
                    Fault::Phase,
                ] {
                    let mut circuit = Fixture::<true>::new(10);
                    circuit.fault = fault;
                    let prover = MockProver::run(&circuit, vec![vec![Fq::from(7)]])?;
                    crate::ensure(
                        prover.verify().is_ok() == (fault == Fault::None),
                        &format!("{fault:?} constraint control"),
                    )?;
                }
                Ok(())
            }
        }
    };
}

backend!(fork, loki);
backend!(reference, stock);
