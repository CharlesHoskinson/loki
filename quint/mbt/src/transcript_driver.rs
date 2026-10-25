use crate::{
    logging::Raw,
    proof::{prove, Shape},
};
use quint_connect::{switch, Config, Driver, Result, State, Step};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Event {
    kind: String,
    label: String,
    proof: i64,
    index: usize,
}

// Decode observed transcript operations, using independent CS metadata for boundaries.
// Actual absorbed values are additionally compared with stock verifier reads in prove().
fn decode(raw: &[Raw], s: &Shape) -> Vec<Event> {
    let mut ops = raw.iter();
    let mut events = vec![];
    let mut take = |kind: &str, label: &str, proof: i64, n: usize| {
        for index in 0..n {
            let observed = ops.next().expect("missing real transcript operation");
            assert_eq!(observed.kind, kind, "wrong operation kind for {label}");
            events.push(Event {
                kind: kind.into(),
                label: label.into(),
                proof,
                index,
            });
        }
    };
    take("common_scalar", "vk", -1, 1);
    for p in 0..s.proofs {
        if s.committed {
            take("common_point", "instance", p as i64, 1);
        } else {
            take("common_scalar", "instance_length", p as i64, 1);
            take("common_scalar", "instance", p as i64, s.instance_values);
        }
    }
    for (label, count, challenge, challenges) in [
        ("advice0", s.advice0, "phase0", s.phase_challenges),
        ("advice1", s.advice1, "phase1", s.phase_challenges1),
        ("advice2", s.advice2, "phase2", s.phase_challenges2),
    ] {
        for p in 0..s.proofs {
            take("point", label, p as i64, count);
        }
        take("squeeze", challenge, -1, challenges);
    }
    take("squeeze", "theta", -1, 1);
    for p in 0..s.proofs {
        take("point", "lookup_permuted", p as i64, 2 * s.lookups);
    }
    take("squeeze", "beta", -1, 1);
    take("squeeze", "gamma", -1, 1);
    for p in 0..s.proofs {
        take("point", "permutation_product", p as i64, s.permutations);
    }
    for p in 0..s.proofs {
        take("point", "lookup_product", p as i64, s.lookups);
    }
    take("squeeze", "trash", -1, 1);
    for p in 0..s.proofs {
        take("point", "trash", p as i64, s.trash);
    }
    take("point", "vanishing_random", -1, 1);
    take("squeeze", "y", -1, 1);
    take("point", "quotient", -1, s.quotient);
    take("squeeze", "x", -1, 1);
    for p in 0..s.proofs {
        take(
            "scalar",
            "instance_eval",
            p as i64,
            if s.committed { s.instance_queries } else { 0 },
        );
    }
    for p in 0..s.proofs {
        take("scalar", "advice_eval", p as i64, s.advice_queries);
    }
    take("scalar", "fixed_eval", -1, s.fixed_queries);
    take("scalar", "random_eval", -1, 1);
    take(
        "scalar",
        "permutation_common_eval",
        -1,
        s.permutation_columns,
    );
    for p in 0..s.proofs {
        take(
            "scalar",
            "permutation_eval",
            p as i64,
            if s.permutations == 0 {
                0
            } else {
                3 * s.permutations - 1
            },
        );
    }
    for p in 0..s.proofs {
        take("scalar", "lookup_eval", p as i64, 5 * s.lookups);
    }
    for p in 0..s.proofs {
        take("scalar", "trash_eval", p as i64, s.trash);
    }
    take("squeeze", "multiopen_x1", -1, 1);
    take("squeeze", "multiopen_x2", -1, 1);
    take("point", "multiopen_f", -1, 1);
    take("squeeze", "multiopen_x3", -1, 1);
    take("scalar", "multiopen_q", -1, s.point_sets);
    take("squeeze", "multiopen_x4", -1, 1);
    take("point", "multiopen_pi", -1, 1);
    assert!(
        ops.next().is_none(),
        "unmodeled trailing transcript operation"
    );
    events
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    shape: Shape,
    plan: Vec<Event>,
    log: Vec<Event>,
    ready: BTreeSet<usize>,
    cursor: usize,
}

#[derive(Default)]
pub struct TranscriptDriver {
    snapshot: Option<Snapshot>,
    serial: u64,
}
impl State<TranscriptDriver> for Snapshot {
    fn from_driver(d: &TranscriptDriver) -> Result<Self> {
        Ok(d.snapshot.clone().unwrap())
    }
}
impl Driver for TranscriptDriver {
    type State = Snapshot;
    fn config() -> Config {
        Config {
            state: &["state"],
            ..Config::default()
        }
    }
    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            init(profile: usize, proofs: usize, committed: bool) => {
                let (shape, raw, equal) = prove(profile, proofs, committed, 1000 + self.serial);
                self.serial += 1;
                let plan = decode(&raw, &shape);
                println!("trace {}: profile={profile} proofs={proofs} committed={committed} operations={} stock_bytes_equal={equal}",
                    self.serial, plan.len());
                self.snapshot = Some(Snapshot { shape, plan, log: vec![], ready: BTreeSet::new(), cursor: 0 });
            },
            compute(index: usize) => { self.snapshot.as_mut().unwrap().ready.insert(index); },
            emit => {
                let s = self.snapshot.as_mut().unwrap();
                s.log.push(s.plan[s.cursor].clone());
                s.cursor += 1;
            },
            finished => {
                let s = self.snapshot.as_ref().unwrap();
                assert_eq!(s.log.len(), s.plan.len(), "incomplete transcript replay");
            }
        })
    }
}

#[quint_connect::quint_run(
    spec = "../transcript.qnt",
    max_samples = 24,
    max_steps = 180,
    seed = "20261024"
)]
fn random_real_transcripts() -> TranscriptDriver {
    TranscriptDriver::default()
}
