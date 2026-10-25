use std::{fs::File, path::Path};

use blake2b_simd::State;
use ff::Field;
use midnight_curves::{Bls12, Fq};
use proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{
        self, Advice, Circuit, Column, ConstraintSystem, Constraints, Error, Instance, Selector,
        VerifyingKey,
    },
    poly::{
        commitment::Guard,
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
        Rotation,
    },
    transcript::{CircuitTranscript, Transcript},
    utils::SerdeFormat,
};
use rand_core::OsRng;

type Scheme = KZGCommitmentScheme<Bls12>;

#[derive(Clone, Default)]
struct SquareCircuit;

#[derive(Clone)]
struct Config {
    advice: Column<Advice>,
    _instance: Column<Instance>,
    selector: Selector,
}

impl Circuit<Fq> for SquareCircuit {
    type Config = Config;
    type FloorPlanner = SimpleFloorPlanner;
    #[cfg(feature = "circuit-params")]
    type Params = ();

    fn without_witnesses(&self) -> Self {
        Self
    }
    fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
        let advice = meta.advice_column();
        let instance = meta.instance_column();
        let selector = meta.selector();
        meta.create_gate("public square", |meta| {
            let a = meta.query_advice(advice, Rotation::cur());
            let b = meta.query_instance(instance, Rotation::cur());
            let q = meta.query_selector(selector);
            Constraints::without_selector(vec![q * (a.clone() * a - b)])
        });
        Config {
            advice,
            _instance: instance,
            selector,
        }
    }
    fn synthesize(&self, config: Config, mut layouter: impl Layouter<Fq>) -> Result<(), Error> {
        layouter.assign_region(
            || "square",
            |mut region| {
                config.selector.enable(&mut region, 0)?;
                region.assign_advice(|| "a", config.advice, 0, || Value::known(Fq::from(7)))?;
                Ok(())
            },
        )
    }
}

#[allow(dead_code)]
pub fn produce(dir: &Path) {
    let params = ParamsKZG::<Bls12>::unsafe_setup(8, OsRng);
    let vk = plonk::keygen_vk_with_k::<Fq, Scheme, _>(&params, &SquareCircuit, 8).unwrap();
    let pk = plonk::keygen_pk(vk, &SquareCircuit).unwrap();
    let values = [Fq::from(49)];
    let instances: &[&[Fq]] = &[&values];
    let mut transcript = CircuitTranscript::<State>::init();
    plonk::create_proof::<Fq, Scheme, _, _>(
        &params,
        &pk,
        &[SquareCircuit],
        #[cfg(feature = "committed-instances")]
        0,
        &[instances],
        &mut transcript,
        OsRng,
    )
    .unwrap();
    std::fs::create_dir_all(dir).unwrap();
    params
        .write_custom(
            &mut File::create(dir.join("params.bin")).unwrap(),
            SerdeFormat::Processed,
        )
        .unwrap();
    pk.get_vk()
        .write(
            &mut File::create(dir.join("vk.bin")).unwrap(),
            SerdeFormat::Processed,
        )
        .unwrap();
    std::fs::write(dir.join("proof.bin"), transcript.finalize()).unwrap();
    verify(dir);
}

pub fn verify(dir: &Path) {
    let params = ParamsKZG::<Bls12>::read_custom(
        &mut File::open(dir.join("params.bin")).unwrap(),
        SerdeFormat::Processed,
    )
    .unwrap();
    let vk = VerifyingKey::<Fq, Scheme>::read::<_, SquareCircuit>(
        &mut File::open(dir.join("vk.bin")).unwrap(),
        SerdeFormat::Processed,
        #[cfg(feature = "circuit-params")]
        (),
    )
    .unwrap();
    let proof = std::fs::read(dir.join("proof.bin")).unwrap();
    for (public, expected) in [(Fq::from(49), true), (Fq::from(49) + Fq::ONE, false)] {
        let values = [public];
        let mut transcript = CircuitTranscript::<State>::init_from_bytes(&proof);
        let accepted = plonk::prepare::<Fq, Scheme, _>(
            &vk,
            #[cfg(feature = "committed-instances")]
            &[&[]],
            &[&[&values]],
            &mut transcript,
        )
        .map(|guard| guard.verify(&params.verifier_params()).is_ok())
        .unwrap_or(false);
        assert_eq!(accepted, expected, "public-input acceptance");
    }
    eprintln!(
        "Accepted CUDA proof ({} bytes); rejected mutated public input",
        proof.len()
    );
}
