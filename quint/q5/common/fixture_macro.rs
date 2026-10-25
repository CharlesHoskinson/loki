// Shared end-to-end fixture, expanded once per midnight-proofs package.
// `$proofs` is the package path; `$rec` is a path to the `Recording` RNG type.
macro_rules! fixture_backend {
    ($module:ident, $proofs:ident, $rec:path) => {
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
                    ConstraintSystem, Constraints, Error, Fixed, Instance, TableColumn, VerifyingKey,
                },
                poly::{
                    commitment::Guard,
                    kzg::{params::ParamsKZG, KZGCommitmentScheme},
                    Rotation,
                },
                transcript::{CircuitTranscript, Transcript},
                utils::{rational::Rational, SerdeFormat},
            };

            /// Lookup input coverage. `Single`: inputs use every distinct table
            /// value, so the only leftovers are the table's default padding value
            /// (placement is HashMap-order independent). `Multi`: several distinct
            /// leftover values (placement depends on HashMap order). `AllEqual`:
            /// every input is the same value. `MaxElem`: inputs are 0 and -1 only.
            #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            pub enum Coverage { Single, Multi, AllEqual, MaxElem }

            #[derive(Clone, Debug)]
            pub struct Config {
                q: Column<Fixed>,
                a: Column<Advice>,
                b: Column<Advice>,
                c: Column<Advice>,
                d: Column<Advice>,
                e: Column<Advice>,
                r: Column<Advice>,
                public: Column<Instance>,
                t1: TableColumn,
                t2: TableColumn,
            }

            #[derive(Clone)]
            pub struct Fixture {
                pub k: u32,
                pub coverage: Coverage,
                pub witness: bool,
            }

            pub const TABLE_ROWS: usize = 16;
            /// Table value at row: 0..15 cyclic, with 15 replaced by the max field element -1.
            pub fn table_value(row: usize) -> Fq {
                let v = row % 16;
                if v == 15 { -Fq::ONE } else { Fq::from(v as u64) }
            }

            impl Fixture {
                fn input(&self, row: usize, salt: usize) -> Fq {
                    if row == 0 {
                        return Fq::ZERO; // public input is a(0) = 0
                    }
                    match self.coverage {
                        Coverage::Single => table_value(row * salt + salt),
                        Coverage::Multi => table_value((row * salt) % 5),
                        Coverage::AllEqual => table_value(7),
                        Coverage::MaxElem => if (row + salt) % 3 == 0 { Fq::ZERO } else { -Fq::ONE },
                    }
                }
                fn rows(&self) -> usize {
                    (1usize << self.k) / 2
                }
            }

            impl Circuit<Fq> for Fixture {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;
                fn without_witnesses(&self) -> Self {
                    Self { witness: false, ..self.clone() }
                }
                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let q = meta.fixed_column();
                    let [a, b, c, d, e, r] = [(); 6].map(|_| meta.advice_column());
                    let public = meta.instance_column();
                    for col in [a, b, c, d, e, r] {
                        meta.enable_equality(col);
                    }
                    meta.enable_equality(public);
                    let t1 = meta.lookup_table_column();
                    let t2 = meta.lookup_table_column();
                    meta.create_gate("sum", |meta| {
                        let q = meta.query_fixed(q, Rotation::cur());
                        let a = meta.query_advice(a, Rotation::cur());
                        let b = meta.query_advice(b, Rotation::cur());
                        let e = meta.query_advice(e, Rotation::cur());
                        Constraints::without_selector(vec![q * (a + b - e)])
                    });
                    meta.lookup("a in t1", |meta| {
                        let q = meta.query_fixed(q, Rotation::cur());
                        let a = meta.query_advice(a, Rotation::cur());
                        vec![(q * a, t1)]
                    });
                    meta.lookup("(b, c) in (t1, t2)", |meta| {
                        let q = meta.query_fixed(q, Rotation::cur());
                        let b = meta.query_advice(b, Rotation::cur());
                        let c = meta.query_advice(c, Rotation::cur());
                        vec![(q.clone() * b, t1), (q * c, t2)]
                    });
                    meta.lookup("d in t1", |meta| {
                        let q = meta.query_fixed(q, Rotation::cur());
                        let d = meta.query_advice(d, Rotation::cur());
                        vec![(q * d, t1)]
                    });
                    Config { q, a, b, c, d, e, r, public, t1, t2 }
                }
                fn synthesize(&self, cfg: Config, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
                    layouter.assign_table(
                        || "table",
                        |mut table| {
                            for row in 0..TABLE_ROWS {
                                let v = table_value(row);
                                table.assign_cell(|| "t1", cfg.t1, row, || Value::known(v))?;
                                table.assign_cell(|| "t2", cfg.t2, row, || Value::known(v.double()))?;
                            }
                            Ok(())
                        },
                    )?;
                    let first = layouter.assign_region(
                        || "main",
                        |mut region| {
                            let w = |x: Fq| if self.witness { Value::known(x) } else { Value::unknown() };
                            let mut first = None;
                            let mut den = Fq::from(3);
                            for row in 0..self.rows() {
                                region.assign_fixed(|| "q", cfg.q, row, || Value::known(Fq::ONE))?;
                                let a = self.input(row, 1);
                                let b = self.input(row, 7);
                                let d = self.input(row, 3);
                                let ca = region.assign_advice(|| "a", cfg.a, row, || w(a))?;
                                region.assign_advice(|| "b", cfg.b, row, || w(b))?;
                                region.assign_advice(|| "c", cfg.c, row, || w(b.double()))?;
                                let cd = region.assign_advice(|| "d", cfg.d, row, || w(d))?;
                                region.assign_advice(|| "e", cfg.e, row, || w(a + b))?;
                                // Rational advice: denominators ONE, ZERO, random, plus Trivial/Zero.
                                den = den.square() + Fq::from(row as u64);
                                let rv = match row % 6 {
                                    0 => Rational::Rational(Fq::from(row as u64 + 5), Fq::ONE),
                                    1 => Rational::Rational(Fq::from(row as u64 + 5), Fq::ZERO),
                                    2 => Rational::Rational(Fq::from(row as u64 + 5), den),
                                    3 => Rational::Trivial(Fq::from(row as u64)),
                                    4 => Rational::Zero,
                                    _ => Rational::Rational(-Fq::ONE, -Fq::ONE),
                                };
                                let cr = region.assign_advice(
                                    || "r",
                                    cfg.r,
                                    row,
                                    || if self.witness { Value::known(rv) } else { Value::unknown() },
                                )?;
                                if row % 4 == 1 && row < self.rows() / 2 {
                                    // Copy constraint across columns (permutation argument content).
                                    let copy = region.assign_advice(|| "a copy", cfg.d, row + self.rows(), || w(a))?;
                                    region.constrain_equal(ca.cell(), copy.cell())?;
                                }
                                let _ = (cd, cr);
                                if row == 0 {
                                    first = Some(ca.cell());
                                }
                            }
                            Ok(first.unwrap())
                        },
                    )?;
                    layouter.constrain_instance(first, cfg.public, 0)
                }
            }

            /// SRS handling for the W10 strategy check.
            #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            pub enum Srs {
                /// `unsafe_setup(k)`: `max_k == k` (W10 coefficient path).
                Exact,
                /// `unsafe_setup(k + 1)`, not downsized (W10 Lagrange fallback path).
                Oversized,
                /// `unsafe_setup(k + 1)` then `downsize(k)` (realistic deployment path).
                Downsized,
                /// Exact, serialized and read back through `read_custom(RawBytes)`.
                Serialized,
                /// Exact `g` with a `g_lagrange` from a different toxic value (invalid SRS).
                BadLagrange,
            }

            pub fn params(k: u32, srs: Srs) -> ParamsKZG<Bls12> {
                let rng = || ChaCha20Rng::from_seed([0x53; 32]);
                match srs {
                    Srs::Exact => ParamsKZG::<Bls12>::unsafe_setup(k, rng()),
                    Srs::Oversized => ParamsKZG::<Bls12>::unsafe_setup(k + 1, rng()),
                    Srs::Downsized => {
                        let mut p = ParamsKZG::<Bls12>::unsafe_setup(k + 1, rng());
                        p.downsize(k);
                        p
                    }
                    Srs::Serialized => {
                        let p = ParamsKZG::<Bls12>::unsafe_setup(k, rng());
                        let mut bytes = vec![];
                        p.write_custom(&mut bytes, SerdeFormat::RawBytes).unwrap();
                        ParamsKZG::<Bls12>::read_custom(&mut &bytes[..], SerdeFormat::RawBytes).unwrap()
                    }
                    Srs::BadLagrange => {
                        let good = ParamsKZG::<Bls12>::unsafe_setup(k, rng());
                        let other = ParamsKZG::<Bls12>::unsafe_setup(k, ChaCha20Rng::from_seed([0x54; 32]));
                        let mut bytes = vec![];
                        good.write_custom(&mut bytes, SerdeFormat::RawBytes).unwrap();
                        let mut obytes = vec![];
                        other.write_custom(&mut obytes, SerdeFormat::RawBytes).unwrap();
                        // RawBytes layout: k (4) | g (n points) | g_lagrange (n points) | g2 | s_g2.
                        // Derive the G1 point size from two serialized lengths.
                        let bigger = ParamsKZG::<Bls12>::unsafe_setup(k + 1, rng());
                        let mut bb = vec![];
                        bigger.write_custom(&mut bb, SerdeFormat::RawBytes).unwrap();
                        let n = 1usize << k;
                        let g1 = (bb.len() - bytes.len()) / (2 * n);
                        assert_eq!(bytes.len(), obytes.len());
                        bytes[4 + n * g1..4 + 2 * n * g1].copy_from_slice(&obytes[4 + n * g1..4 + 2 * n * g1]);
                        ParamsKZG::<Bls12>::read_custom(&mut &bytes[..], SerdeFormat::RawBytes).unwrap()
                    }
                }
            }

            #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
            pub struct Out {
                pub proof: Vec<u8>,
                pub vk: Vec<u8>,
                pub draws: usize,
                pub draw_digest: String,
                pub word_pos: u128,
            }

            pub fn prove(k: u32, coverage: Coverage, srs: Srs, seed: u64) -> Out {
                let params = params(k, srs);
                let circuit = Fixture { k, coverage, witness: true };
                let vk = keygen_vk_with_k::<_, KZGCommitmentScheme<Bls12>, _>(&params, &circuit.without_witnesses(), k).unwrap();
                let pk = keygen_pk(vk, &circuit.without_witnesses()).unwrap();
                let vk = pk.get_vk().to_bytes(SerdeFormat::RawBytes);
                let public = [Fq::ZERO];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init();
                let mut rng = <$rec>::new(seed);
                create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
                    &params, &pk, &[circuit], 0, &[instances], &mut transcript, &mut rng,
                ).unwrap();
                let mut h = blake2b_simd::State::new();
                for (kind, bytes) in &rng.log {
                    h.update(&[*kind]);
                    h.update(&(bytes.len() as u64).to_le_bytes());
                    h.update(bytes);
                }
                Out {
                    proof: transcript.finalize(),
                    vk,
                    draws: rng.log.len(),
                    draw_digest: h.finalize().to_hex().to_string(),
                    word_pos: rng.word_pos(),
                }
            }

            /// Verify with this package's verifier (use the `stock` expansion for the stock verifier).
            pub fn verify(k: u32, srs: Srs, vk: &[u8], proof: &[u8]) -> bool {
                let params = params(k, srs);
                let Ok(key) = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<_, Fixture>(
                    &mut Cursor::new(vk), SerdeFormat::RawBytes,
                ) else { return false };
                let public = [Fq::ZERO];
                let instances: &[&[Fq]] = &[&public];
                let mut transcript = CircuitTranscript::<State>::init_from_bytes(proof);
                let Ok(guard) = prepare::<Fq, KZGCommitmentScheme<Bls12>, _>(&key, &[&[]], &[instances], &mut transcript) else { return false };
                if transcript.assert_empty().is_err() { return false; }
                guard.verify(&params.verifier_params()).is_ok()
            }
        }
    };
}
