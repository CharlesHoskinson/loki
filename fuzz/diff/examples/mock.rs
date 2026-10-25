use loki_diff::{circuit::{Case, set_shape}, proof::loki_fixture::Fixture, Input};
fn main() {
    let i: u64 = std::env::args().nth(1).unwrap().parse().unwrap();
    let data = i.wrapping_mul(0x9e37_79b9_7f4a_7c15).to_le_bytes().repeat(32);
    let case = Case::generate(&mut Input::new(&data));
    let inst = case.layout.instances(&case.values[0]);
    set_shape(case.layout.shape);
    let c = Fixture { layout: case.layout.clone(), values: Some(case.values[0].clone()) };
    let p = loki::dev::MockProver::run(&c, inst).unwrap();
    match p.verify() { Ok(()) => println!("ok"), Err(e) => for f in e.iter().take(4) { println!("{f:?}") } }
}
