use loki_harness::{determinism, negative_controls, Result};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let sizes = if args.is_empty() {
        vec![10, 14, 17]
    } else {
        args.iter()
            .map(|arg| arg.parse())
            .collect::<std::result::Result<Vec<u32>, _>>()?
    };
    negative_controls()?;
    for k in sizes {
        eprintln!("gate k={k}: SRS, keys, Loki proof, stock proof (sequential)");
        let comparison = determinism(k)?;
        println!("{}", serde_json::to_string(&comparison)?);
        if std::env::var_os("LOKI_STRICT_LOOKUP_BYTES").is_some() && !comparison.determinism.equal {
            return Err("strict lookup proof byte comparison failed".into());
        }
    }
    Ok(())
}
