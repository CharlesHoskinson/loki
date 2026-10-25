//! quint-q1 ITF replay: Loki prover, stock 0.8.3 prover, stock 0.8.3 verifier.
//! Transcript payloads stay in memory; only shapes and equality flags are printed.
use serde_json::Value;
use std::sync::Mutex;

pub mod fixture;

/// Fixture minimum degree; must equal `DEGREE` in quint/q1/transcript_q1.qnt.
pub const DEGREE: usize = 5;
pub const K: u32 = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cfg {
    pub proofs: usize,
    pub committed: usize,
    pub plain: usize,
    pub values: usize,
    pub adv0: usize,
    pub adv1: usize,
    pub adv2: usize,
    pub ch0: usize,
    pub ch1: usize,
    pub adv_next: bool,
    pub lookups: usize,
    pub perm_cols: usize,
    pub trash: usize,
    pub multiopen: bool,
    pub truncated: bool,
    pub alias: bool,
    pub mode: String,
}

static CFG: Mutex<Option<Cfg>> = Mutex::new(None);
pub fn set_cfg(c: &Cfg) {
    *CFG.lock().unwrap() = Some(c.clone());
}
pub fn current_cfg() -> Cfg {
    CFG.lock()
        .unwrap()
        .clone()
        .expect("fixture configuration not set")
}

/// One observed transcript operation. For squeezes, `bytes` is a BLAKE2b digest of all
/// bytes absorbed so far (prefix identity), so challenge k agrees iff digests agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op {
    pub kind: &'static str,
    pub bytes: Vec<u8>,
}

/// Model event (subset of `Ev` in the Quint model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ev {
    pub op: String,
    pub label: String,
    pub proof: i64,
    pub col: i64,
    pub idx: i64,
}

const SCALAR_WRITES: &[&str] = &[
    "inst_eval",
    "adv_eval",
    "fixed_eval",
    "random_eval",
    "sigma_eval",
    "pz_eval",
    "pz_next_eval",
    "pz_last_eval",
    "lprod_eval",
    "lprod_next_eval",
    "lin_eval",
    "lin_inv_eval",
    "ltab_eval",
    "trash_eval",
    "q_eval",
];

impl Ev {
    /// Mirrors `kindOf` in the model, in the vocabulary of the logging transcript.
    pub fn kind(&self) -> &'static str {
        match self.op.as_str() {
            "squeeze" => "squeeze",
            "common" if self.label == "inst_com" => "common_point",
            "common" => "common_scalar",
            _ if SCALAR_WRITES.contains(&self.label.as_str()) => "scalar",
            _ => "point",
        }
    }
}

// ---------- ITF decoding ----------
pub fn int(v: &Value) -> i64 {
    match v {
        Value::Object(o) if o.contains_key("#bigint") => {
            o["#bigint"].as_str().unwrap().parse().unwrap()
        }
        Value::Number(n) => n.as_i64().unwrap(),
        _ => panic!("not an ITF int: {v}"),
    }
}
pub fn list(v: &Value) -> &Vec<Value> {
    v.as_array().expect("ITF list")
}
pub fn ev(v: &Value) -> Ev {
    Ev {
        op: v["op"].as_str().unwrap().into(),
        label: v["label"].as_str().unwrap().into(),
        proof: int(&v["proof"]),
        col: int(&v["col"]),
        idx: int(&v["idx"]),
    }
}
pub fn cfg(v: &Value) -> Cfg {
    let u = |k: &str| int(&v[k]) as usize;
    Cfg {
        proofs: u("proofs"),
        committed: u("committed"),
        plain: u("plain"),
        values: u("values"),
        adv0: u("adv0"),
        adv1: u("adv1"),
        adv2: u("adv2"),
        ch0: u("ch0"),
        ch1: u("ch1"),
        adv_next: v["advNext"].as_bool().unwrap(),
        lookups: u("lookups"),
        perm_cols: u("permCols"),
        trash: u("trash"),
        multiopen: v["multiopen"].as_bool().unwrap(),
        truncated: v["truncated"].as_bool().unwrap(),
        alias: v["aliasCommitted"].as_bool().unwrap(),
        mode: v["mode"].as_str().unwrap().into(),
    }
}

/// Model formulas from transcript_q1.qnt, cross-checked against the real ConstraintSystem.
pub fn model_perm_sets(c: &Cfg) -> usize {
    c.perm_cols.div_ceil(DEGREE - 2)
}
pub fn model_fixed_queries(c: &Cfg) -> usize {
    1 + c.lookups + c.trash + c.perm_cols
}
pub fn model_advice_queries(c: &Cfg) -> usize {
    c.adv0 + c.adv1 + c.adv2 + usize::from(c.adv_next)
}

/// Shape facts read from a real constraint system (loki and stock must agree).
#[derive(Debug, PartialEq, Eq)]
pub struct Shape {
    pub degree: usize,
    pub quotient: usize,
    pub advice_queries: usize,
    pub instance_queries: usize,
    pub fixed_queries: usize,
    pub lookups: usize,
    pub trashcans: usize,
    pub permutation_columns: usize,
    pub challenges: usize,
}

/// Public values: proof p, column c, row i. With `alias`, identical for every proof.
pub fn public_values(c: &Cfg) -> Vec<Vec<Vec<midnight_curves::Fq>>> {
    (0..c.proofs)
        .map(|p| {
            let p = if c.alias { 0 } else { p };
            (0..c.committed + c.plain)
                .map(|col| {
                    (0..c.values)
                        .map(|i| midnight_curves::Fq::from((100 * p + 10 * col + i + 1) as u64))
                        .collect()
                })
                .collect()
        })
        .collect()
}
