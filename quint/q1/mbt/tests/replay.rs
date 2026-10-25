//! ITF replay of quint/q1/transcript_q1.qnt against real Rust transcripts.
//! Trace directory: $Q1_ITF_DIR (default ../../../.artifacts/q1/itf). Traces whose feature
//! flags differ from this build are skipped, and the run fails if fewer than
//! $Q1_MIN_TRACES (default 8) replayed.
use loki_quint_q1::{fixture::*, *};
use midnight_curves::Fq;
use serde_json::Value;
use std::path::PathBuf;

fn fq_bytes(x: Fq) -> Vec<u8> {
    use ff::PrimeField;
    x.to_repr().as_ref().to_vec()
}

fn kinds(ops: &[Op]) -> Vec<&'static str> {
    ops.iter().map(|o| o.kind).collect()
}
fn squeezes(ops: &[Op]) -> Vec<&[u8]> {
    ops.iter().filter(|o| o.kind == "squeeze").map(|o| o.bytes.as_slice()).collect()
}

/// Expected bytes of a labelled common item, or None when the label is not value-checked.
fn expected_common(e: &Ev, c: &Cfg, publics: &[Vec<Vec<Fq>>], srs: &[u8], vk: &[u8]) -> Option<Vec<u8>> {
    let (p, col) = (e.proof as usize, e.col as usize);
    match e.label.as_str() {
        "inst_com" => Some(loki_side::commitment_bytes(srs, vk, &publics[p][col])),
        "inst_len" => Some(fq_bytes(Fq::from(publics[p][col].len() as u64))),
        "inst_val" => Some(fq_bytes(publics[p][col][e.idx as usize])),
        _ => {
            let _ = c;
            None
        }
    }
}

/// Check that each labelled common op carries exactly the bytes its label names.
fn check_commons(side: &str, plan: &[Ev], ops: &[Op], c: &Cfg, publics: &[Vec<Vec<Fq>>], srs: &[u8], vk: &[u8]) {
    for (i, (e, o)) in plan.iter().zip(ops).enumerate() {
        if e.op == "common" {
            if let Some(bytes) = expected_common(e, c, publics, srs, vk) {
                assert_eq!(o.bytes, bytes, "{side}: op {i} is not {} p{} c{} i{}", e.label, e.proof, e.col, e.idx);
            }
        }
    }
}

/// Model challenge agreement (chAgree in the model): same event, same prefix length,
/// identical absorbed prefixes.
fn model_agree(st: &Value, k: usize) -> bool {
    let (a, b) = (&list(&st["pChallenges"])[k], &list(&st["vChallenges"])[k]);
    let (na, nb) = (int(&a["n"]) as usize, int(&b["n"]) as usize);
    a["ev"] == b["ev"] && na == nb && list(&st["pAbsorbed"])[..na] == list(&st["vAbsorbed"])[..nb]
}
fn model_all_agree(st: &Value) -> bool {
    let (p, v) = (list(&st["pChallenges"]).len(), list(&st["vChallenges"]).len());
    p == v && (0..p).all(|k| model_agree(st, k))
}

#[test]
fn replay_itf_traces() {
    let dir = std::env::var("Q1_ITF_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../.artifacts/q1/itf")
    });
    let min: usize = std::env::var("Q1_MIN_TRACES").ok().and_then(|s| s.parse().ok()).unwrap_or(8);
    let multiopen = cfg!(feature = "loki-multiopen");
    let truncated = cfg!(feature = "truncated");
    let det = cfg!(feature = "det-lookup");
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("ITF dir").map(|e| e.unwrap().path()).collect();
    files.sort();
    let srs = loki_side::setup();
    let (mut replayed, mut mixed, mut alias_cases, mut byte_equal, mut lookup_cases) = (0, 0, 0, 0, 0);
    let (mut loki_repro, mut stock_repro) = (0, 0);
    for f in files {
        let t: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let states = list(&t["states"]);
        let last = &states.last().unwrap()["st"];
        let c = cfg(&last["cfg"]);
        if c.mode != "none" || c.multiopen != multiopen || c.truncated != truncated {
            continue;
        }
        set_cfg(&c);
        let p_plan: Vec<Ev> = list(&last["pPlan"]).iter().map(ev).collect();
        let v_plan: Vec<Ev> = list(&last["vPlan"]).iter().map(ev).collect();
        let publics = public_values(&c);
        let seed = 9000 + replayed as u64;

        let loki = loki_side::prove(&srs, &c, &publics, seed);
        let stock = stock_side::prove(&srs, &c, &publics, seed);

        // Constraint-system shape vs model formulas, and loki vs stock keygen.
        let s = &loki.shape;
        assert_eq!(s, &stock.shape, "{f:?}: loki/stock CS shape");
        assert_eq!(loki.vk, stock.vk, "{f:?}: loki/stock VK bytes");
        assert_eq!(s.degree, DEGREE, "{f:?}: degree");
        assert_eq!(s.quotient, DEGREE - 1, "{f:?}: quotient pieces");
        assert_eq!(s.advice_queries, model_advice_queries(&c), "{f:?}: advice queries");
        assert_eq!(s.instance_queries, c.committed + c.plain, "{f:?}: instance queries");
        assert_eq!(s.fixed_queries, model_fixed_queries(&c), "{f:?}: fixed queries");
        assert_eq!((s.lookups, s.trashcans, s.permutation_columns, s.challenges),
            (c.lookups, c.trash, c.perm_cols, c.ch0 + c.ch1), "{f:?}: argument counts");

        // (1) prover op sequence == model prover plan; stock prover has the same sequence.
        let model_p: Vec<_> = p_plan.iter().map(Ev::kind).collect();
        assert_eq!(kinds(&loki.ops), model_p, "{f:?}: Loki prover op kinds vs model");
        assert_eq!(kinds(&stock.ops), model_p, "{f:?}: stock prover op kinds vs model");
        check_commons("loki prover", &p_plan, &loki.ops, &c, &publics, &srs, &loki.vk);
        check_commons("stock prover", &p_plan, &stock.ops, &c, &publics, &srs, &loki.vk);
        if c.lookups == 0 && !det {
            assert_eq!(loki.proof, stock.proof, "{f:?}: no-lookup proof bytes loki vs stock");
        }
        byte_equal += usize::from(loki.proof == stock.proof);
        if c.lookups > 0 {
            // P1: default = stock HashMap leftover order (not reproducible in-process);
            // loki-deterministic-lookup-order = field order (reproducible).
            let again = loki_side::prove(&srs, &c, &publics, seed);
            let stock_again = stock_side::prove(&srs, &c, &publics, seed);
            if det {
                assert_eq!(again.proof, loki.proof, "{f:?}: det-lookup proof not reproducible");
            }
            loki_repro += usize::from(again.proof == loki.proof);
            stock_repro += usize::from(stock_again.proof == stock.proof);
        }
        lookup_cases += usize::from(c.lookups > 0);

        // (2) stock verifier on the Loki proof: op sequence == model verifier plan.
        let v = stock_side::verify(&srs, &loki.vk, &loki.proof, &c, &publics, c.alias);
        let model_ok = last["vDone"].as_bool().unwrap() && !last["vError"].as_bool().unwrap()
            && list(&last["misread"]).is_empty()
            && model_all_agree(last);
        if c.alias && !last["vDone"].as_bool().unwrap() {
            // Model: verifier fails at multiopen set construction (DuplicatedQuery).
            assert!(!v.prepared && !v.accepted, "{f:?}: aliased commitments accepted");
            let fresh = stock_side::verify(&srs, &loki.vk, &loki.proof, &c, &publics, false);
            assert!(fresh.accepted, "{f:?}: same proof with distinct references rejected");
            alias_cases += 1;
            replayed += 1;
            continue;
        }
        let model_v: Vec<_> = v_plan.iter().map(Ev::kind).collect();
        assert_eq!(kinds(&v.ops), model_v, "{f:?}: stock verifier op kinds vs model");
        check_commons("stock verifier", &v_plan, &v.ops, &c, &publics, &srs, &loki.vk);
        // Proof stream: verifier reads == prover writes, byte for byte.
        let w: Vec<_> = loki.ops.iter().filter(|o| !o.kind.starts_with("common") && o.kind != "squeeze").collect();
        let r: Vec<_> = v.ops.iter().filter(|o| !o.kind.starts_with("common") && o.kind != "squeeze").collect();
        assert_eq!(w, r, "{f:?}: proof stream");

        // (3) challenge k agrees in Rust iff it agrees in the model.
        let (pc, vc) = (list(&last["pChallenges"]), list(&last["vChallenges"]));
        let (ps, vs) = (squeezes(&loki.ops), squeezes(&v.ops));
        assert_eq!((ps.len(), vs.len()), (pc.len(), vc.len()), "{f:?}: challenge counts");
        for k in 0..pc.len() {
            assert_eq!(ps[k] == vs[k], model_agree(last, k), "{f:?}: challenge {k} agreement");
        }
        // (4) acceptance == model verdict; the same for the stock prover's proof.
        assert_eq!(v.accepted, model_ok, "{f:?}: stock acceptance vs model");
        let sv = stock_side::verify(&srs, &stock.vk, &stock.proof, &c, &publics, false);
        assert_eq!(sv.accepted, model_ok, "{f:?}: stock-on-stock acceptance vs model");
        let lv = loki_side::verify(&srs, &loki.vk, &loki.proof, &c, &publics, false);
        assert_eq!(kinds(&lv.ops), model_v, "{f:?}: Loki verifier op kinds");
        assert_eq!(lv.accepted, model_ok, "{f:?}: Loki verifier acceptance");

        // (5) step correspondence: every model state's published stream and challenge
        // agreement flags are a prefix of the real ones.
        for st in states {
            let st = &st["st"];
            let stream = list(&st["stream"]);
            for (i, e) in stream.iter().enumerate() {
                assert_eq!(ev(e).kind(), w[i].kind, "{f:?}: state stream item {i}");
            }
            let (spc, svc) = (list(&st["pChallenges"]), list(&st["vChallenges"]));
            for k in 0..spc.len().min(svc.len()) {
                assert_eq!(model_agree(st, k), ps[k] == vs[k], "{f:?}: state challenge {k}");
            }
        }
        let mixed_case = c.proofs > 1 && c.committed > 0 && c.plain > 0;
        assert_eq!(model_ok, !mixed_case, "{f:?}: defect classification");
        if mixed_case {
            mixed += 1;
            let first = (0..pc.len()).find(|&k| !model_agree(last, k));
            let real = (0..ps.len()).find(|&k| ps[k] != vs[k]);
            assert_eq!(first, real, "{f:?}: first diverging challenge");
        }
        replayed += 1;
    }
    println!(
        "q1 replay multiopen={multiopen} truncated={truncated} det={det}: replayed={replayed} mixed_defect={mixed} alias={alias_cases} lookup_cases={lookup_cases} loki_eq_stock_bytes={byte_equal} lookup_reproducible_loki={loki_repro} lookup_reproducible_stock={stock_repro}"
    );
    assert!(replayed >= min, "only {replayed} traces matched this build");
}

/// Negative control for the harness itself: a prover transcript with one extra absorption
/// (test-only fault in the logging wrapper) must be rejected by stock, contradicting the
/// unmutated model's verdict, for every fault position tried.
#[test]
fn injected_transcript_fault_is_detected() {
    let dir = std::env::var("Q1_ITF_DIR").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../.artifacts/q1/itf")
    });
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("ITF dir").map(|e| e.unwrap().path()).collect();
    files.sort();
    let srs = loki_side::setup();
    let mut detected = 0;
    for f in files {
        let t: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let last = &list(&t["states"]).last().unwrap()["st"];
        let c = cfg(&last["cfg"]);
        let model_ok = last["vDone"].as_bool().unwrap() && !last["vError"].as_bool().unwrap() && model_all_agree(last);
        if c.mode != "none" || c.multiopen != cfg!(feature = "loki-multiopen")
            || c.truncated != cfg!(feature = "truncated") || !model_ok {
            continue;
        }
        set_cfg(&c);
        let publics = public_values(&c);
        let writes = list(&last["pPlan"]).iter().filter(|e| e["op"] == "write").count();
        // pi (the last write) is followed by no squeeze, so absorbing it again is unobservable.
        for fault in [0, writes / 2, writes - 2] {
            let bad = loki_side::prove_with_fault(&srs, &c, &publics, 77, Some(fault));
            let v = stock_side::verify(&srs, &bad.vk, &bad.proof, &c, &publics, false);
            assert!(!v.accepted, "{f:?}: fault at write {fault} undetected");
            detected += 1;
        }
        if detected >= 12 {
            break;
        }
    }
    println!("q1 injected faults detected: {detected}");
    assert!(detected >= 12);
}
