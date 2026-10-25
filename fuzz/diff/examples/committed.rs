//! Characterise committed-instance acceptance by column contents.
use std::sync::Arc;
use loki_diff::{circuit::Case, proof::{build, Verdict}, Input};
use midnight_curves::Fq;

fn main() {
    let data = 2u64.wrapping_mul(0x9e37_79b9_7f4a_7c15).to_le_bytes().repeat(32);
    let base = Case::generate(&mut Input::new(&data));
    for (name, vals) in [("empty", vec![]), ("zero", vec![Fq::from(0)]), ("five", vec![Fq::from(5)]), ("zero,five", vec![Fq::from(0), Fq::from(5)])] {
        for nc in [0usize, 1] {
            let mut layout = (*base.layout).clone();
            layout.shape.nb_committed = nc;
            layout.links = vec![vals.iter().map(|_| None).collect()];
            let mut v = (*base.values[0]).clone();
            v.free_instance = vec![vals.clone()];
            let case = Case { layout: Arc::new(layout), values: vec![Arc::new(v)] };
            let b = build(case).unwrap();
            let (l, s) = b.prove_both(1);
            let (l, s) = (l.unwrap(), s.unwrap());
            let r = [b.verify_stock(&b.instances, &s), b.verify_loki(&b.instances, &l), b.verify_stock(&b.instances, &l)];
            println!("{name:10} committed={nc}: stock/stock={:?} loki/loki={:?} loki->stock={:?}", r[0] == Verdict::Accept, r[1] == Verdict::Accept, r[2] == Verdict::Accept);
        }
    }
}
