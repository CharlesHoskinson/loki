// P1 seam fixtures. The same circuit and prover/verifier code expand against
// Loki (`midnight_proofs`) and the unmodified crates.io 0.8.3 package
// (`stock`), so no Loki type reaches the stock verifier.
//
// Shapes control the lookup leftover map. Stock iterates leftover table keys
// in `HashMap` order, which is randomly seeded per map, so byte equality is
// only promised when at most one leftover key has a nonzero count.
#![allow(dead_code)]

/// No lookup argument: byte equality with stock is required.
pub const NO_LOOKUP: u8 = 0;
/// Lookup whose leftover map has exactly one key with a nonzero count
/// (the padding value). Leftover order is irrelevant: byte equality required.
pub const SINGLE_LEFTOVER: u8 = 1;
/// Lookup with many distinct leftovers: placement follows map order, so byte
/// equality is diagnostic only; verifier acceptance is required.
pub const MANY_LEFTOVERS: u8 = 2;

pub const K: u32 = 9;
pub const PUBLIC: u64 = 7;

macro_rules! fixture {
    ($module:ident, $proofs:ident) => {
        pub mod $module {
            use blake2b_simd::State;
            use ff::Field;
            use midnight_curves::{Bls12, Fq};
            use rand_chacha::ChaCha20Rng;
            use rand_core::SeedableRng;
            use std::io::Cursor;
            use $proofs::{
                circuit::{Layouter, SimpleFloorPlanner, Value},
                plonk::{
                    create_proof, keygen_pk, keygen_vk_with_k, prepare, Advice, Circuit, Column,
                    ConstraintSystem, Constraints, Error, Fixed, Instance, TableColumn,
                    VerifyingKey,
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

            #[derive(Clone, Debug)]
            pub struct Config {
                x: Column<Advice>,
                y: Column<Advice>,
                active: Column<Fixed>,
                public: Column<Instance>,
                table: Option<TableColumn>,
            }

            #[derive(Clone)]
            pub struct Fixture<const SHAPE: u8> {
                pub rows: usize,
            }

            impl<const SHAPE: u8> Fixture<SHAPE> {
                pub fn new() -> Self {
                    Self {
                        rows: (1usize << super::K) / 2,
                    }
                }
            }

            impl<const SHAPE: u8> Circuit<Fq> for Fixture<SHAPE> {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;
                #[cfg(feature = "circuit-params")]
                type Params = ();

                fn without_witnesses(&self) -> Self {
                    self.clone()
                }

                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let x = meta.advice_column();
                    let y = meta.advice_column();
                    let active = meta.fixed_column();
                    let public = meta.instance_column();
                    meta.enable_equality(x);
                    meta.enable_equality(y);
                    meta.enable_equality(public);
                    meta.create_gate("y = x^2 + x", |meta| {
                        let q = meta.query_fixed(active, Rotation::cur());
                        let x = meta.query_advice(x, Rotation::cur());
                        let y = meta.query_advice(y, Rotation::cur());
                        Constraints::without_selector(vec![q * (y - x.clone() * x.clone() - x)])
                    });
                    let table = (SHAPE != super::NO_LOOKUP).then(|| {
                        let table = meta.lookup_table_column();
                        meta.lookup("range", |meta| {
                            let q = meta.query_fixed(active, Rotation::cur());
                            let x = meta.query_advice(x, Rotation::cur());
                            vec![(q * x, table)]
                        });
                        table
                    });
                    Config {
                        x,
                        y,
                        active,
                        public,
                        table,
                    }
                }

                fn synthesize(
                    &self,
                    config: Config,
                    mut layouter: impl Layouter<Fq>,
                ) -> Result<(), Error> {
                    if let Some(column) = config.table {
                        // SINGLE_LEFTOVER: table 0..64, every value used by the
                        // input, so only the padding value 0 is left over.
                        // MANY_LEFTOVERS: table 0..256, inputs only use 0..64.
                        let size = if SHAPE == super::SINGLE_LEFTOVER { 64 } else { 256 };
                        layouter.assign_table(
                            || "range table",
                            |mut table| {
                                for row in 0..size {
                                    table.assign_cell(
                                        || "value",
                                        column,
                                        row,
                                        || Value::known(Fq::from(row as u64)),
                                    )?;
                                }
                                Ok(())
                            },
                        )?;
                    }
                    let first = layouter.assign_region(
                        || "rows",
                        |mut region| {
                            let mut first = None;
                            for row in 0..self.rows {
                                region.assign_fixed(
                                    || "active",
                                    config.active,
                                    row,
                                    || Value::known(Fq::ONE),
                                )?;
                                // gcd(17, 64) = 1, so 64 consecutive rows cover 0..64.
                                let x = Fq::from(((row * 17 + 7) % 64) as u64);
                                let cell = region.assign_advice(
                                    || "x",
                                    config.x,
                                    row,
                                    || Value::known(x),
                                )?;
                                let y = x.square() + x;
                                region.assign_advice(|| "y", config.y, row, || Value::known(y))?;
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

            pub fn setup() -> Vec<u8> {
                let params =
                    ParamsKZG::<Bls12>::unsafe_setup(super::K, ChaCha20Rng::from_seed([0x51; 32]));
                let mut bytes = Vec::new();
                params.write_custom(&mut bytes, SerdeFormat::RawBytes).unwrap();
                bytes
            }

            pub fn roundtrip_params(bytes: &[u8]) -> Vec<u8> {
                let mut reader = Cursor::new(bytes);
                let params =
                    ParamsKZG::<Bls12>::read_custom(&mut reader, SerdeFormat::RawBytes).unwrap();
                assert_eq!(reader.position() as usize, bytes.len(), "trailing SRS bytes");
                let mut output = Vec::new();
                params.write_custom(&mut output, SerdeFormat::RawBytes).unwrap();
                output
            }

            pub struct Sample {
                pub vk: Vec<u8>,
                pub verifier_params: Vec<u8>,
                pub proof: Vec<u8>,
            }

            /// Keygen and prove under a fixed proof RNG seed. Callers pin the
            /// Rayon pool size (stock vanishing randomness depends on it).
            pub fn prove<const SHAPE: u8>(params: &[u8], seed: u8) -> Sample {
                let params =
                    ParamsKZG::<Bls12>::read_custom(&mut &params[..], SerdeFormat::RawBytes)
                        .unwrap();
                let circuit = Fixture::<SHAPE>::new();
                let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(
                    &params,
                    &circuit.without_witnesses(),
                    super::K,
                )
                .unwrap();
                let pk = keygen_pk(vk, &circuit.without_witnesses()).unwrap();
                let vk = pk.get_vk().to_bytes(SerdeFormat::RawBytes);
                let mut verifier_params = Vec::new();
                params
                    .verifier_params()
                    .write(&mut verifier_params, SerdeFormat::RawBytes)
                    .unwrap();
                let public = [Fq::from(super::PUBLIC)];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init();
                create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
                    &params,
                    &pk,
                    &[circuit],
                    0,
                    &[instances],
                    &mut transcript,
                    ChaCha20Rng::from_seed([seed; 32]),
                )
                .unwrap();
                Sample {
                    vk,
                    verifier_params,
                    proof: transcript.finalize(),
                }
            }

            /// Full verification, including transcript exhaustion.
            pub fn verify<const SHAPE: u8>(
                vk: &[u8],
                params: &[u8],
                proof: &[u8],
                public: u64,
            ) -> bool {
                let mut reader = Cursor::new(vk);
                let key = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<
                    _,
                    Fixture<SHAPE>,
                >(
                    &mut reader,
                    SerdeFormat::RawBytes,
                    #[cfg(feature = "circuit-params")]
                    (),
                )
                .unwrap();
                assert_eq!(reader.position() as usize, vk.len(), "trailing VK bytes");
                let params =
                    ParamsVerifierKZG::<Bls12>::read(&mut &params[..], SerdeFormat::RawBytes)
                        .unwrap();
                let public = [Fq::from(public)];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
                let Ok(guard) = prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(
                    &key,
                    &[&[]],
                    &[instances],
                    &mut transcript,
                ) else {
                    return false;
                };
                if transcript.assert_empty().is_err() {
                    return false;
                }
                guard.verify(&params).is_ok()
            }
        }
    };
}

fixture!(loki, midnight_proofs);
fixture!(reference, stock);

/// Run `f` on a dedicated single-worker Rayon pool.
pub fn single_thread<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(f)
}

pub fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}

/// Shared cross-verification gate. Returns (loki proof, stock proof).
/// Requires: identical SRS bytes through both readers, identical VK and
/// verifier-param bytes, acceptance in both directions, rejection of a wrong
/// public input, an empty proof, a truncated proof and a trailing byte.
pub fn cross_verify<const SHAPE: u8>(seed: u8) -> (Vec<u8>, Vec<u8>) {
    single_thread(|| {
        let params = loki::setup();
        let stock_params = reference::roundtrip_params(&params);
        assert_eq!(stock_params, params, "stock SRS bytes differ");
        assert_eq!(loki::roundtrip_params(&stock_params), params, "Loki SRS bytes differ");

        let candidate = loki::prove::<SHAPE>(&params, seed);
        let baseline = reference::prove::<SHAPE>(&params, seed);
        assert_eq!(candidate.vk, baseline.vk, "VK bytes differ");
        assert_eq!(
            candidate.verifier_params, baseline.verifier_params,
            "verifier params differ"
        );
        let (vk, vp) = (&baseline.vk, &baseline.verifier_params);

        assert!(
            reference::verify::<SHAPE>(vk, vp, &candidate.proof, PUBLIC),
            "unmodified stock verifier rejected Loki proof"
        );
        assert!(
            loki::verify::<SHAPE>(vk, vp, &baseline.proof, PUBLIC),
            "Loki verifier rejected stock proof"
        );
        for proof in [&candidate.proof, &baseline.proof] {
            assert!(!reference::verify::<SHAPE>(vk, vp, proof, PUBLIC + 1), "wrong public accepted");
            assert!(!reference::verify::<SHAPE>(vk, vp, &[], PUBLIC), "empty proof accepted");
            assert!(
                !reference::verify::<SHAPE>(vk, vp, &proof[..proof.len() - 1], PUBLIC),
                "truncated proof accepted"
            );
            let mut extended = proof.clone();
            extended.push(0);
            assert!(!reference::verify::<SHAPE>(vk, vp, &extended, PUBLIC), "trailing byte accepted");
            let mut flipped = proof.clone();
            flipped[proof.len() / 2] ^= 1;
            assert!(!reference::verify::<SHAPE>(vk, vp, &flipped, PUBLIC), "bit flip accepted");
        }
        (candidate.proof, baseline.proof)
    })
}
