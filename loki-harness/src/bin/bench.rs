use loki_harness::{cross_verify, Result};
use serde::Serialize;

#[derive(Serialize)]
struct Record<'a> {
    backend: &'static str,
    #[serde(flatten)]
    metrics: &'a loki_harness::Metrics,
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let sizes = if args.is_empty() {
        vec![10, 14, 17]
    } else {
        args.iter()
            .map(|arg| arg.parse())
            .collect::<std::result::Result<Vec<u32>, _>>()?
    };
    for k in sizes {
        let comparison = cross_verify(k)?;
        for (backend, metrics) in [("loki", &comparison.loki), ("stock", &comparison.stock)] {
            println!("{}", serde_json::to_string(&Record { backend, metrics })?);
        }
    }
    Ok(())
}
