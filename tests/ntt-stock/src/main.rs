//! Synthetic public circuit; no production keys or private witnesses are saved.
use blake2b_simd::State;
use midnight_curves::{Bls12, Fq};
use rand_core::OsRng;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tracing_subscriber::{
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    Layer,
};

#[derive(Clone)]
struct ProductCircuit {
    rows: usize,
}

// Implement exactly the same circuit against two distinct crate sources.
macro_rules! circuit {
    ($module:ident, $library:ident) => {
        mod $module {
            use super::*;
            use proofs::{
                circuit::{Layouter, SimpleFloorPlanner, Value},
                plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Instance, Selector},
                poly::Rotation,
            };
            use $library as proofs;
            #[derive(Clone, Debug)]
            pub struct Config {
                advice: [Column<Advice>; 3],
                instance: Column<Instance>,
                q: Selector,
            }
            impl Circuit<Fq> for ProductCircuit {
                type Config = Config;
                type FloorPlanner = SimpleFloorPlanner;
                fn without_witnesses(&self) -> Self {
                    self.clone()
                }
                fn configure(meta: &mut ConstraintSystem<Fq>) -> Config {
                    let advice = [
                        meta.advice_column(),
                        meta.advice_column(),
                        meta.advice_column(),
                    ];
                    let instance = meta.instance_column();
                    meta.enable_equality(instance);
                    for column in advice {
                        meta.enable_equality(column);
                    }
                    let q = meta.selector();
                    meta.create_gate("product and next-row rotation", |meta| {
                        let a = meta.query_advice(advice[0], Rotation::cur());
                        let b = meta.query_advice(advice[1], Rotation::cur());
                        let c = meta.query_advice(advice[2], Rotation::cur());
                        let next_a = meta.query_advice(advice[0], Rotation::next());
                        proofs::plonk::Constraints::with_selector(
                            q,
                            vec![a.clone() * b - c, next_a - a],
                        )
                    });
                    Config {
                        advice,
                        instance,
                        q,
                    }
                }
                fn synthesize(
                    &self,
                    config: Config,
                    mut layouter: impl Layouter<Fq>,
                ) -> Result<(), Error> {
                    let first = layouter.assign_region(
                        || "products",
                        |mut region| {
                            let mut first = None;
                            for row in 0..self.rows {
                                let a = Fq::from(2);
                                let b = Fq::from(row as u64 + 3);
                                let cell = region.assign_advice(
                                    || "a",
                                    config.advice[0],
                                    row,
                                    || Value::known(a),
                                )?;
                                region.assign_advice(
                                    || "b",
                                    config.advice[1],
                                    row,
                                    || Value::known(b),
                                )?;
                                region.assign_advice(
                                    || "c",
                                    config.advice[2],
                                    row,
                                    || Value::known(a * b),
                                )?;
                                if row == 0 {
                                    first = Some(cell.cell());
                                }
                                if row + 1 < self.rows {
                                    config.q.enable(&mut region, row)?;
                                }
                            }
                            Ok(first.unwrap())
                        },
                    )?;
                    layouter.constrain_instance(first, config.instance, 0)
                }
            }
        }
    };
}
circuit!(loki_circuit, loki);
circuit!(stock_circuit, stock);

#[derive(Clone)]
struct PhaseTimer(Arc<Mutex<Vec<(&'static str, Duration)>>>);
impl<S> Layer<S> for PhaseTimer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: Context<'_, S>,
    ) {
        if attrs.metadata().name().starts_with("loki_ntt_") {
            ctx.span(id)
                .unwrap()
                .extensions_mut()
                .insert(Instant::now());
        }
    }
    fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).unwrap();
        let extensions = span.extensions();
        if let Some(start) = extensions.get::<Instant>() {
            self.0.lock().unwrap().push((span.name(), start.elapsed()));
        }
    }
}

fn main() {
    use loki::{
        plonk::{create_proof, keygen_pk, keygen_vk_with_k, VerifyingKey},
        poly::kzg::{params::ParamsKZG, KZGCommitmentScheme},
        transcript::{CircuitTranscript, Transcript},
        utils::SerdeFormat,
    };
    use stock::poly::commitment::Guard;
    // Run both paths in one binary with identical keys and circuit. CPU selection
    // is explicit and real GPU completion counts are required in GPU mode.
    let k = std::env::args()
        .nth(1)
        .map(|s| s.parse().unwrap())
        .unwrap_or(17);
    let repeats: usize = std::env::args()
        .nth(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or(3);
    let circuit = ProductCircuit {
        rows: (1 << k) - 32,
    };
    let phases = PhaseTimer(Arc::new(Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::registry().with(phases.clone());
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let params = ParamsKZG::<Bls12>::unsafe_setup(k, OsRng);
    let vk = keygen_vk_with_k(&params, &circuit, k).unwrap();
    let pk = keygen_pk(vk, &circuit).unwrap();
    println!(
        "CIRCUIT k={k} extended_k={} rows={} rayon_threads={}",
        pk.get_vk().get_domain().extended_k(),
        circuit.rows,
        rayon::current_num_threads()
    );
    let mut params_bytes = vec![];
    params
        .write_custom(&mut params_bytes, SerdeFormat::Processed)
        .unwrap();
    let stock_params = stock::poly::kzg::params::ParamsKZG::<Bls12>::read_custom(
        &mut &params_bytes[..],
        stock::utils::SerdeFormat::Processed,
    )
    .unwrap();
    let mut vk_bytes = vec![];
    pk.get_vk()
        .write(&mut vk_bytes, SerdeFormat::Processed)
        .unwrap();
    let stock_vk =
        stock::plonk::VerifyingKey::<Fq, stock::poly::kzg::KZGCommitmentScheme<Bls12>>::read::<
            _,
            ProductCircuit,
        >(&mut &vk_bytes[..], stock::utils::SerdeFormat::Processed)
        .unwrap();
    // Ensure reading/reconstructing the VK on either side preserves bytes.
    let _vk = VerifyingKey::<Fq, KZGCommitmentScheme<Bls12>>::read::<_, ProductCircuit>(
        &mut &vk_bytes[..],
        SerdeFormat::Processed,
    )
    .unwrap();
    let instances = vec![Fq::from(2)];
    for backend in ["cpu", "gpu"] {
        if backend == "gpu" && !cfg!(feature = "gpu") {
            continue;
        }
        if backend == "cpu" {
            std::env::set_var("LOKI_NTT_CPU", "1");
        } else {
            std::env::remove_var("LOKI_NTT_CPU");
        }
        for run in 0..repeats {
            phases.0.lock().unwrap().clear();
            #[cfg(feature = "gpu")]
            let calls = loki_cuda_ntt::completed_calls();
            let mut transcript: CircuitTranscript<State> = CircuitTranscript::init();
            let start = Instant::now();
            create_proof::<Fq, KZGCommitmentScheme<Bls12>, _, _>(
                &params,
                &pk,
                &[circuit.clone()],
                0,
                &[&[&instances]],
                &mut transcript,
                OsRng,
            )
            .unwrap();
            let prove = start.elapsed();
            let proof = transcript.finalize();
            let mut transcript: stock::transcript::CircuitTranscript<State> =
                stock::transcript::Transcript::init_from_bytes(&proof);
            stock::plonk::prepare::<Fq, stock::poly::kzg::KZGCommitmentScheme<Bls12>, _>(
                &stock_vk,
                &[&[]],
                &[&[&instances]],
                &mut transcript,
            )
            .unwrap()
            .verify(&stock_params.verifier_params())
            .unwrap();
            // Public-input mutation must be rejected by the stock verifier.
            let bad_instances = vec![Fq::from(3)];
            let mut transcript: stock::transcript::CircuitTranscript<State> =
                stock::transcript::Transcript::init_from_bytes(&proof);
            let rejected = stock::plonk::prepare::<
                Fq,
                stock::poly::kzg::KZGCommitmentScheme<Bls12>,
                _,
            >(&stock_vk, &[&[]], &[&[&bad_instances]], &mut transcript)
            .map(|guard| guard.verify(&stock_params.verifier_params()).is_err())
            .unwrap_or(true);
            assert!(rejected, "mutated public input accepted");
            #[cfg(feature = "gpu")]
            {
                let completed = loki_cuda_ntt::completed_calls() - calls;
                if backend == "gpu" {
                    assert!(completed > 0);
                } else {
                    assert_eq!(completed, 0);
                }
                println!("PROOF k={k} backend={backend} run={run} gpu_calls={completed}");
            }
            println!("PROOF k={k} backend={backend} run={run} prove_ms={:.3} bytes={} stock_verified=true mutation_rejected=true",
                prove.as_secs_f64() * 1000.0, proof.len());
            for (name, elapsed) in phases.0.lock().unwrap().iter() {
                println!(
                    "PHASE backend={backend} run={run} name={name} ms={:.3}",
                    elapsed.as_secs_f64() * 1000.0
                );
            }
        }
    }
}
