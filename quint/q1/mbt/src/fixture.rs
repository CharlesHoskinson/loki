//! The same configurable circuit, logging transcript, prover and verifier, expanded
//! against two independent midnight-proofs packages. No Loki type reaches `stock`.
macro_rules! side {
    ($module:ident, $proofs:ident) => {
        pub mod $module {
            use crate::{current_cfg, Cfg, Op, Shape, DEGREE, K};
            use blake2b_simd::State;
            use ff::Field;
            use midnight_curves::{Bls12, Fq, G1Projective};
            use rand_chacha::ChaCha20Rng;
            use rand_core::SeedableRng;
            use std::io::{self, Cursor};
            use $proofs::{
                circuit::{Layouter, SimpleFloorPlanner, Value},
                plonk::{
                    commit_to_instances, create_proof, keygen_pk, keygen_vk_with_k, prepare,
                    Advice, Challenge, Circuit, Column, ConstraintSystem, Constraints, Error,
                    FirstPhase, Fixed, Instance, SecondPhase, Selector, TableColumn, ThirdPhase,
                    VerifyingKey,
                },
                poly::{
                    commitment::Guard,
                    kzg::{params::ParamsKZG, KZGCommitmentScheme},
                    Rotation,
                },
                transcript::{CircuitTranscript, Hashable, Sampleable, Transcript},
                utils::SerdeFormat,
            };

            type Plain = CircuitTranscript<State>;

            /// Delegating transcript that records every operation in memory.
            #[derive(Clone)]
            pub struct Logging {
                inner: Plain,
                absorbed: Vec<u8>,
                pub ops: Vec<Op>,
                /// Test-only fault: absorb write number `fault` a second time (hash only).
                pub fault: Option<usize>,
                writes: usize,
            }
            fn is_point<T>() -> bool {
                std::any::type_name::<T>() == std::any::type_name::<G1Projective>()
            }
            impl Logging {
                fn absorb(&mut self, kind: &'static str, bytes: Vec<u8>) {
                    self.absorbed.extend_from_slice(&bytes);
                    self.ops.push(Op { kind, bytes });
                }
            }
            impl Transcript for Logging {
                type Hash = State;
                fn init() -> Self {
                    Self { inner: Plain::init(), absorbed: vec![], ops: vec![], fault: None, writes: 0 }
                }
                fn init_from_bytes(bytes: &[u8]) -> Self {
                    Self { inner: Plain::init_from_bytes(bytes), absorbed: vec![], ops: vec![], fault: None, writes: 0 }
                }
                fn squeeze_challenge<T: Sampleable<State>>(&mut self) -> T {
                    let digest = blake2b_simd::blake2b(&self.absorbed).as_bytes().to_vec();
                    self.ops.push(Op { kind: "squeeze", bytes: digest });
                    self.inner.squeeze_challenge()
                }
                fn common<T: Hashable<State>>(&mut self, input: &T) -> io::Result<()> {
                    self.inner.common(input)?;
                    let kind = if is_point::<T>() { "common_point" } else { "common_scalar" };
                    self.absorb(kind, input.to_bytes());
                    Ok(())
                }
                fn read<T: Hashable<State>>(&mut self) -> io::Result<T> {
                    let input: T = self.inner.read()?;
                    self.absorb(if is_point::<T>() { "point" } else { "scalar" }, input.to_bytes());
                    Ok(input)
                }
                fn write<T: Hashable<State>>(&mut self, input: &T) -> io::Result<()> {
                    self.inner.write(input)?;
                    if self.fault == Some(self.writes) {
                        self.inner.common(input)?;
                    }
                    self.writes += 1;
                    self.absorb(if is_point::<T>() { "point" } else { "scalar" }, input.to_bytes());
                    Ok(())
                }
                fn finalize(self) -> Vec<u8> {
                    self.inner.finalize()
                }
                fn assert_empty(&mut self) -> io::Result<()> {
                    self.inner.assert_empty()
                }
            }

            #[derive(Clone)]
            pub struct Fx {
                pub a0: u64,
            }
            #[derive(Clone)]
            pub struct FxConfig {
                adv: Vec<Column<Advice>>,
                eq: Vec<Column<Fixed>>,
                tables: Vec<TableColumn>,
                trash: Option<Selector>,
            }

            // Query creation order matches `adviceQueries`/`instanceQueries` in the model.
            impl Circuit<Fq> for Fx {
                type Config = FxConfig;
                type FloorPlanner = SimpleFloorPlanner;
                fn without_witnesses(&self) -> Self {
                    self.clone()
                }
                fn configure(meta: &mut ConstraintSystem<Fq>) -> FxConfig {
                    let c = current_cfg();
                    meta.set_minimum_degree(DEGREE);
                    let mut adv: Vec<Column<Advice>> = (0..c.adv0).map(|_| meta.advice_column()).collect();
                    adv.extend((0..c.adv1).map(|_| meta.advice_column_in(SecondPhase)));
                    adv.extend((0..c.adv2).map(|_| meta.advice_column_in(ThirdPhase)));
                    let mut chs: Vec<Challenge> =
                        (0..c.ch0).map(|_| meta.challenge_usable_after(FirstPhase)).collect();
                    chs.extend((0..c.ch1).map(|_| meta.challenge_usable_after(SecondPhase)));
                    let inst: Vec<Column<Instance>> =
                        (0..c.committed + c.plain).map(|_| meta.instance_column()).collect();
                    let fz = meta.fixed_column();
                    let (a, i, ch, next) = (adv.clone(), inst.clone(), chs.clone(), c.adv_next);
                    meta.create_gate("zero-gated queries", move |meta| {
                        let z = meta.query_fixed(fz, Rotation::cur());
                        let mut polys = vec![];
                        for col in &a {
                            polys.push(z.clone() * meta.query_advice(*col, Rotation::cur()));
                        }
                        for col in &i {
                            polys.push(z.clone() * meta.query_instance(*col, Rotation::cur()));
                        }
                        if next {
                            polys.push(z.clone() * meta.query_advice(a[0], Rotation::next()));
                        }
                        for x in &ch {
                            polys.push(z.clone() * meta.query_challenge(*x));
                        }
                        Constraints::without_selector(polys)
                    });
                    let tables: Vec<TableColumn> = (0..c.lookups)
                        .map(|_| {
                            let t = meta.lookup_table_column();
                            let a0 = adv[0];
                            meta.lookup("range", move |meta| {
                                vec![(meta.query_advice(a0, Rotation::cur()), t)]
                            });
                            t
                        })
                        .collect();
                    let trash = (c.trash > 0).then(|| {
                        let s = meta.complex_selector();
                        let a0 = adv[0];
                        meta.create_gate("trash", move |meta| {
                            let a = meta.query_advice(a0, Rotation::cur());
                            Constraints::with_additive_selector(s, vec![a.clone() - a])
                        });
                        s
                    });
                    let eq: Vec<Column<Fixed>> = (0..c.perm_cols)
                        .map(|_| {
                            let e = meta.fixed_column();
                            meta.enable_equality(e);
                            e
                        })
                        .collect();
                    FxConfig { adv, eq, tables, trash }
                }
                fn synthesize(&self, cfg: FxConfig, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
                    layouter.assign_region(
                        || "cells",
                        |mut region| {
                            if let Some(s) = cfg.trash {
                                s.enable(&mut region, 0)?;
                            }
                            for col in &cfg.adv {
                                region.assign_advice(|| "a", *col, 0, || Value::known(Fq::from(self.a0)))?;
                            }
                            for e in &cfg.eq {
                                region.assign_fixed(|| "e", *e, 0, || Value::known(Fq::ONE))?;
                            }
                            Ok(())
                        },
                    )?;
                    for t in &cfg.tables {
                        layouter.assign_table(
                            || "table",
                            |mut table| {
                                for i in 0..8 {
                                    table.assign_cell(|| "v", *t, i, || Value::known(Fq::from(i as u64)))?;
                                }
                                Ok(())
                            },
                        )?;
                    }
                    Ok(())
                }
            }

            pub fn setup() -> Vec<u8> {
                let params = ParamsKZG::<Bls12>::unsafe_setup(K, ChaCha20Rng::from_seed([0x51; 32]));
                let mut bytes = Vec::new();
                params.write_custom(&mut bytes, SerdeFormat::RawBytes).unwrap();
                bytes
            }
            fn params(bytes: &[u8]) -> ParamsKZG<Bls12> {
                ParamsKZG::<Bls12>::read_custom(&mut &bytes[..], SerdeFormat::RawBytes).unwrap()
            }

            pub struct Proved {
                pub vk: Vec<u8>,
                pub proof: Vec<u8>,
                pub ops: Vec<Op>,
                pub shape: Shape,
            }

            pub fn prove(srs: &[u8], c: &Cfg, publics: &[Vec<Vec<Fq>>], seed: u64) -> Proved {
                prove_with_fault(srs, c, publics, seed, None)
            }

            pub fn prove_with_fault(srs: &[u8], c: &Cfg, publics: &[Vec<Vec<Fq>>], seed: u64, fault: Option<usize>) -> Proved {
                let params = params(srs);
                let circuits: Vec<Fx> = (0..c.proofs).map(|p| Fx { a0: (p as u64 + 3) % 8 }).collect();
                let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(&params, &circuits[0], K).unwrap();
                let pk = keygen_pk(vk, &circuits[0]).unwrap();
                let cs = pk.get_vk().cs();
                let shape = Shape {
                    degree: cs.degree(),
                    quotient: pk.get_vk().get_domain().get_quotient_poly_degree(),
                    advice_queries: cs.advice_queries().len(),
                    instance_queries: cs.instance_queries().len(),
                    fixed_queries: cs.fixed_queries().len(),
                    lookups: cs.lookups().len(),
                    trashcans: cs.trashcans().len(),
                    permutation_columns: cs.permutation().get_columns().len(),
                    challenges: cs.num_challenges(),
                };
                let cols: Vec<Vec<&[Fq]>> =
                    publics.iter().map(|p| p.iter().map(Vec::as_slice).collect()).collect();
                let instances: Vec<&[&[Fq]]> = cols.iter().map(Vec::as_slice).collect();
                let mut t = Logging::init();
                t.fault = fault;
                create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
                    &params,
                    &pk,
                    &circuits,
                    c.committed,
                    &instances,
                    &mut t,
                    ChaCha20Rng::seed_from_u64(seed),
                )
                .unwrap();
                let ops = t.ops.clone();
                Proved { vk: pk.get_vk().to_bytes(SerdeFormat::RawBytes), proof: t.finalize(), ops, shape }
            }

            pub fn commitment_bytes(srs: &[u8], vk: &[u8], values: &[Fq]) -> Vec<u8> {
                let params = params(srs);
                let key = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<_, Fx>(
                    &mut Cursor::new(vk),
                    SerdeFormat::RawBytes,
                )
                .unwrap();
                let com = commit_to_instances::<Fq, KZGCommitmentScheme<Bls12>>(&params, key.get_domain(), values);
                <G1Projective as Hashable<State>>::to_bytes(&com)
            }

            pub struct Verified {
                pub ops: Vec<Op>,
                pub prepared: bool,
                pub accepted: bool,
            }

            /// `alias`: pass ONE committed-instance slice for every proof (identical values).
            pub fn verify(srs: &[u8], vk: &[u8], proof: &[u8], c: &Cfg, publics: &[Vec<Vec<Fq>>], alias: bool) -> Verified {
                let params = params(srs);
                let key = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<_, Fx>(
                    &mut Cursor::new(vk),
                    SerdeFormat::RawBytes,
                )
                .unwrap();
                assert_eq!(key.to_bytes(SerdeFormat::RawBytes), vk, "VK roundtrip");
                let comms: Vec<Vec<G1Projective>> = publics
                    .iter()
                    .map(|p| {
                        p[..c.committed]
                            .iter()
                            .map(|v| commit_to_instances::<Fq, KZGCommitmentScheme<Bls12>>(&params, key.get_domain(), v))
                            .collect()
                    })
                    .collect();
                let comm_refs: Vec<&[G1Projective]> = if alias {
                    vec![comms[0].as_slice(); c.proofs]
                } else {
                    comms.iter().map(Vec::as_slice).collect()
                };
                let normal: Vec<Vec<&[Fq]>> =
                    publics.iter().map(|p| p[c.committed..].iter().map(Vec::as_slice).collect()).collect();
                let normal: Vec<&[&[Fq]]> = normal.iter().map(Vec::as_slice).collect();
                let mut t = Logging::init_from_bytes(proof);
                let guard = prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(&key, &comm_refs, &normal, &mut t);
                let prepared = guard.is_ok();
                let accepted = match guard {
                    Ok(g) => t.assert_empty().is_ok() && g.verify(&params.verifier_params()).is_ok(),
                    Err(_) => false,
                };
                Verified { ops: t.ops, prepared, accepted }
            }
        }
    };
}

side!(loki_side, loki);
side!(stock_side, stock);
