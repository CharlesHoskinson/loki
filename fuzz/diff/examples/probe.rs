//! Run one check on N deterministic inputs (or on files), printing timings.
//! `cargo run --release --example probe -- <target> <count|file...>`
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run: fn(&[u8]) = match args[0].as_str() {
        "msm_diff" => loki_diff::msm::run,
        "ntt_diff" => loki_diff::ntt::run,
        "proof_roundtrip" => loki_diff::proof::roundtrip,
        "soundness" => loki_diff::proof::soundness,
        "proof_parse" => loki_diff::parse::run,
        t => panic!("unknown target {t}"),
    };
    let inputs: Vec<(String, Vec<u8>)> = match args[1].parse::<u64>() {
        Ok(n) => (0..n)
            .map(|i| (format!("seed{i}"), i.wrapping_mul(0x9e37_79b9_7f4a_7c15).to_le_bytes().repeat(32)))
            .collect(),
        Err(_) => args[1..].iter().map(|f| (f.clone(), std::fs::read(f).unwrap())).collect(),
    };
    for (name, data) in inputs {
        if std::env::var_os("DUMP").is_some() { dump(&data); }
        let t = Instant::now();
        run(&data);
        eprintln!("{name}: ok in {:.3}s", t.elapsed().as_secs_f64());
    }
}

#[allow(dead_code)]
fn dump(data: &[u8]) {
    let mut input = loki_diff::Input::new(data);
    let case = loki_diff::circuit::Case::generate(&mut input);
    eprintln!("links={:?} rows={} inst={:?}", case.layout.links, case.layout.rows.len(),
        case.values.iter().map(|v| case.layout.instances(v)).collect::<Vec<_>>());
}
