//! Differential replay: snapshotted stock 0.8.3 vs Loki a256a9a production code.
//! Run with `cargo test --release` (debug_assertions off, as in production).
use ff::{BatchInvert, Field};
use loki_q5_replay::{shim::*, *};
use midnight_curves::Fq;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};

fn pools(rng: &mut ChaCha20Rng) -> Vec<Fq> {
    let mut v = vec![Fq::ZERO, Fq::ONE, -Fq::ONE, Fq::from(2), -Fq::from(2), Fq::from(7)];
    v.extend((0..4).map(|_| Fq::random(&mut *rng)));
    v
}
fn pick(rng: &mut ChaCha20Rng, pool: &[Fq]) -> Fq {
    pool[(rng.next_u32() as usize) % pool.len()]
}
fn pool(threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap()
}
const THREADS: [usize; 5] = [1, 2, 3, 4, 7];

/// Value patterns that hit zero handling and chunk boundaries.
fn pattern(rng: &mut ChaCha20Rng, len: usize, kind: usize, threads: usize) -> Vec<Fq> {
    let p = pools(rng);
    let base = (len / threads).max(1);
    let chunk = len.div_ceil(threads).max(1024);
    (0..len)
        .map(|i| match kind {
            0 => Fq::random(&mut *rng),
            1 => Fq::ZERO,
            2 => Fq::ONE,
            3 => -Fq::ONE,
            4 => pick(rng, &p),
            5 => if i % base == 0 || i % base == base - 1 || i % chunk == 0 || (i + 1) % chunk == 0 { Fq::ZERO } else { Fq::random(&mut *rng) },
            6 => if i % 2 == 0 { Fq::ZERO } else { -Fq::ONE },
            _ => if rng.next_u32() % 8 == 0 { Fq::ZERO } else { Fq::random(&mut *rng) },
        })
        .collect()
}

#[test]
fn batch_invert_and_prefix_product_equal_stock() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x5151);
    let lens = [0usize, 1, 2, 3, 17, 1023, 1024, 1025, 2047, 2048, 2049, 4093, 8191];
    let mut samples = 0;
    for &t in &THREADS {
        let tp = pool(t);
        for &len in &lens {
            for kind in 0..8 {
                for _rep in 0..if len < 64 { 48 } else { 4 } {
                    let v = pattern(&mut rng, len, kind, t);
                    // Stock: one sequential ff batch inversion over the whole slice.
                    let mut want = v.clone();
                    want.iter_mut().batch_invert();
                    let mut got = v.clone();
                    tp.install(|| loki_arith::batch_invert_parallel(&mut got));
                    assert_eq!(got, want, "batch_invert len={len} kind={kind} threads={t}");
                    // Stock lookup/permutation z: z0 = init, z_{i+1} = z_i * v_i.
                    let init = if kind % 2 == 0 { Fq::ONE } else { let pp = pools(&mut rng); pick(&mut rng, &pp) };
                    let mut want = Vec::with_capacity(len);
                    let mut acc = init;
                    for x in &v {
                        want.push(acc);
                        acc *= x;
                    }
                    let mut got = v.clone();
                    let total = tp.install(|| loki_arith::prefix_product(&mut got, init));
                    assert_eq!((got, total), (want, acc), "prefix len={len} kind={kind} threads={t}");
                    samples += 1;
                }
            }
        }
    }
    println!("helper samples={samples}");
    assert!(samples >= 10_000);
}

#[test]
fn batch_invert_rational_equals_stock() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x5252);
    let mut samples = 0;
    for &t in &THREADS {
        let tp = pool(t);
        for len in [0usize, 1, 5, 1023, 1025, 3000] {
            for _ in 0..if len < 64 { 700 } else { 6 } {
                let p = pools(&mut rng);
                let cols = 1 + (rng.next_u32() % 3) as usize;
                let assigned: Vec<_> = (0..cols)
                    .map(|_| {
                        poly((0..len)
                            .map(|_| match rng.next_u32() % 6 {
                                0 => Rational::Zero,
                                1 => Rational::Trivial(pick(&mut rng, &p)),
                                2 => Rational::Rational(pick(&mut rng, &p), Fq::ONE),
                                3 => Rational::Rational(pick(&mut rng, &p), Fq::ZERO),
                                4 => Rational::Rational(pick(&mut rng, &p), -Fq::ONE),
                                _ => Rational::Rational(pick(&mut rng, &p), pick(&mut rng, &p)),
                            })
                            .collect())
                    })
                    .collect();
                let want = stock_rational::batch_invert_rational(assigned.clone());
                let got = tp.install(|| loki_rational::batch_invert_rational(assigned.clone()));
                assert_eq!(got, want);
                // And both equal per-element evaluation.
                for (g, a) in got.iter().zip(&assigned) {
                    assert_eq!(g.values, a.iter().map(|r| r.evaluate()).collect::<Vec<_>>());
                }
                samples += 1;
            }
        }
    }
    println!("rational samples={samples}");
    assert!(samples >= 10_000);
}

/// Check the halo2 lookup-argument conditions on (A', S') for usable rows.
pub fn check_lookup(input: &[Fq], table: &[Fq], ap: &[Fq], sp: &[Fq], u: usize) {
    let mut a = input[..u].to_vec();
    let mut s = table[..u].to_vec();
    let (mut ap_u, mut sp_u) = (ap[..u].to_vec(), sp[..u].to_vec());
    a.sort();
    s.sort();
    // A' is sorted (so like values adjacent) and is a permutation of the input.
    assert_eq!(ap_u, a, "A' must be the sorted input");
    sp_u.sort();
    assert_eq!(sp_u, s, "S' must be a permutation of the table");
    ap_u = ap[..u].to_vec();
    for i in 0..u {
        assert!(ap_u[i] == sp[i] || (i > 0 && ap_u[i] == ap_u[i - 1]), "lookup condition at row {i}");
    }
    assert_eq!(ap[0], sp[0], "row 0 must satisfy a'_0 = s'_0");
}

type PermFn = fn(&ProvingKey<Fq, Pcs>, &EvaluationDomain<Fq>, &mut ChaCha20Rng, &Polynomial<Fq, LagrangeCoeff>, &Polynomial<Fq, LagrangeCoeff>) -> Result<ExpressionPair<Fq>, Error>;

fn permute_case(rng: &mut ChaCha20Rng, k: u32, bf: usize, distinct: usize, missing: bool) -> (ProvingKey<Fq, Pcs>, Vec<Fq>, Vec<Fq>) {
    let pk = pk::<Fq>(k, bf, 4);
    let n = 1usize << k;
    let u = n - bf - 1;
    let p = pools(rng);
    let vals: Vec<Fq> = (0..distinct).map(|i| if i < p.len() { p[i] } else { Fq::random(&mut *rng) }).collect();
    let mut table: Vec<Fq> = (0..n).map(|_| vals[(rng.next_u32() as usize) % distinct]).collect();
    // Ensure each value used by the input is somewhere in the table's usable rows.
    let used = 1 + (rng.next_u32() as usize) % distinct;
    for i in 0..used.min(u) {
        table[i] = vals[i];
    }
    let mut input: Vec<Fq> = (0..n).map(|_| vals[(rng.next_u32() as usize) % used.min(u)]).collect();
    if missing {
        input[(rng.next_u32() as usize) % u] = Fq::from(1_000_003u64); // not in any pool/table
    }
    // Junk in blinding rows of the compressed expressions must be ignored.
    for i in u..n {
        input[i] = Fq::random(&mut *rng);
        table[i] = Fq::random(&mut *rng);
    }
    (pk, input, table)
}

#[test]
fn permute_expression_pair_equals_stock_for_every_hash_order() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x5353);
    let stock: PermFn = stock_permute::permute_expression_pair::<Fq, Pcs>;
    let loki: PermFn = loki_permute_default::permute_expression_pair::<Fq, Pcs>;
    let det: PermFn = loki_permute_det::permute_expression_pair::<Fq, Pcs>;
    let w10: PermFn = w10_permute::permute_expression_pair::<Fq, Pcs>;
    let (mut samples, mut errors, mut order_sensitive) = (0, 0, 0);
    for &t in &[1usize, 3, 4] {
        let tp = pool(t);
        for case in 0..3400 {
            let k = if case % 50 == 0 { 11 } else { 3 + (case % 4) as u32 };
            let bf = 1 + (rng.next_u32() % 5) as usize;
            if (1usize << k) <= bf + 2 { continue; }
            let distinct = 1 + (rng.next_u32() % 9) as usize;
            let missing = case % 17 == 0;
            let (pk, input, table) = permute_case(&mut rng, k, bf, distinct, missing);
            let dom = pk.vk.domain.clone();
            let (ip, tpoly) = (poly(input.clone()), poly(table.clone()));
            let seed = rng.next_u64();
            let order = match case % 4 {
                0 => Order::Sorted,
                1 => Order::FixedHash,
                _ => Order::Seed(rng.next_u64()),
            };
            let run = |f: PermFn, order: Order| {
                let mut r = ChaCha20Rng::seed_from_u64(seed);
                let out = tp.install(|| with_order(order, || f(&pk, &dom, &mut r, &ip, &tpoly)));
                (out, r.get_word_pos())
            };
            let (s_out, s_pos) = run(stock, order.clone());
            let (l_out, l_pos) = run(loki, order.clone());
            assert_eq!(l_out, s_out, "default leftover placement != stock (case {case})");
            assert_eq!(l_pos, s_pos, "RNG consumption differs (case {case})");
            let (d_out, d_pos) = run(det, Order::Seed(seed));
            let (s_sorted, _) = run(stock, Order::Sorted);
            assert_eq!(d_out, s_sorted, "deterministic order != stock under sorted HashMap order");
            assert_eq!(d_pos, s_pos);
            let (w_out, _) = run(w10, Order::Seed(seed ^ 1));
            assert_eq!(w_out, d_out, "W10 head leftover order != main deterministic order");
            match s_out {
                Err(e) => {
                    assert!(missing, "unexpected error");
                    assert_eq!(e, Error::ConstraintSystemFailure);
                    errors += 1;
                }
                Ok((ap, sp)) => {
                    assert!(!missing || input[..1usize << k].len() > 0);
                    let n = 1usize << k;
                    let u = n - bf - 1;
                    check_lookup(&input, &table, &ap, &sp, u);
                    // Blinding rows: exactly bf+1 draws for A', then bf+1 for S', in that order.
                    let mut r = ChaCha20Rng::seed_from_u64(seed);
                    let a_bl: Vec<Fq> = (0..=bf).map(|_| Fq::random(&mut r)).collect();
                    let s_bl: Vec<Fq> = (0..=bf).map(|_| Fq::random(&mut r)).collect();
                    assert_eq!(&ap[u..], &a_bl[..]);
                    assert_eq!(&sp[u..], &s_bl[..]);
                    assert_eq!(r.get_word_pos(), s_pos);
                    if s_sorted != run(stock, Order::Seed(seed ^ 7)).0 {
                        order_sensitive += 1;
                    }
                }
            }
            samples += 1;
        }
    }
    println!("permute samples={samples} errors={errors} order_sensitive={order_sensitive}");
    assert!(samples >= 10_000 && errors > 0 && order_sensitive > 0);
}

#[test]
fn lookup_product_equals_stock_and_telescopes() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x5454);
    let (mut samples, mut telescoped, mut zero_den) = (0, 0, 0);
    for &t in &THREADS {
        let tp = pool(t);
        for case in 0..2100 {
            let k = match case % 70 { 0 => 12, 1 => 11, _ => 3 + (case % 5) as u32 };
            let bf = 1 + (rng.next_u32() % 5) as usize;
            if (1usize << k) <= bf + 2 { continue; }
            let distinct = 1 + (rng.next_u32() % 9) as usize;
            let (pk, input, table) = permute_case(&mut rng, k, bf, distinct, false);
            let dom = pk.vk.domain.clone();
            let mut r0 = ChaCha20Rng::seed_from_u64(rng.next_u64());
            let (ap, sp) = with_order(Order::Seed(rng.next_u64()), || {
                stock_permute::permute_expression_pair::<Fq, Pcs>(&pk, &dom, &mut r0, &poly(input.clone()), &poly(table.clone()))
            }).unwrap();
            let n = 1usize << k;
            let u = n - bf - 1;
            let (mut beta, mut gamma) = (Fq::random(&mut rng), Fq::random(&mut rng));
            match case % 6 {
                // Adversarial: a zero denominator (beta = -a'_i) or zero numerator.
                1 => beta = -ap[(rng.next_u32() as usize) % u],
                2 => gamma = -sp[(rng.next_u32() as usize) % u],
                3 => beta = -input[(rng.next_u32() as usize) % u],
                4 => { beta = Fq::ZERO; gamma = -Fq::ONE; }
                _ => {}
            }
            let seed = rng.next_u64();
            let mk = || (poly(input.clone()), ap.clone(), poly(table.clone()), sp.clone());
            let (ci, pi, ct, pt) = mk();
            let mut rs = ChaCha20Rng::seed_from_u64(seed);
            let zs = tp.install(|| stock_product::Permuted { compressed_input_expression: ci, permuted_input_expression: pi, compressed_table_expression: ct, permuted_table_expression: pt }.product(&pk, beta, gamma, &mut rs));
            let (ci, pi, ct, pt) = mk();
            let mut rl = ChaCha20Rng::seed_from_u64(seed);
            let zl = tp.install(|| loki_product::Permuted { compressed_input_expression: ci, permuted_input_expression: pi, compressed_table_expression: ct, permuted_table_expression: pt }.product(&pk, beta, gamma, &mut rl));
            assert_eq!(zl, zs, "lookup z differs (case {case}, threads {t})");
            assert_eq!(rl.get_word_pos(), rs.get_word_pos(), "RNG draws differ");
            assert_eq!(zl.len(), n);
            let has_zero_den = (0..u).any(|i| (beta + ap[i]) * (gamma + sp[i]) == Fq::ZERO);
            assert_eq!(zl[0], Fq::ONE);
            if !has_zero_den {
                assert_eq!(zl[u], Fq::ONE, "product must telescope to 1 over usable rows");
                telescoped += 1;
            } else {
                zero_den += 1;
            }
            // Blinding rows are exactly the next bf draws.
            let mut r = ChaCha20Rng::seed_from_u64(seed);
            let bl: Vec<Fq> = (0..bf).map(|_| Fq::random(&mut r)).collect();
            assert_eq!(&zl[u + 1..], &bl[..]);
            samples += 1;
        }
    }
    println!("product samples={samples} telescoped={telescoped} zero_den={zero_den}");
    assert!(samples >= 10_000 && zero_den > 0);
}

#[test]
fn permutation_product_equals_stock_and_telescopes() {
    let mut rng = ChaCha20Rng::seed_from_u64(0x5555);
    let (mut samples, mut telescoped) = (0, 0);
    for &t in &THREADS {
        let tp = pool(t);
        for case in 0..2100 {
            let k = match case % 70 { 0 => 12, 1 => 11, _ => 3 + (case % 5) as u32 };
            let bf = 1 + (rng.next_u32() % 4) as usize;
            let n = 1usize << k;
            if n <= bf + 2 { continue; }
            let u = n - bf - 1;
            let cs_degree = 3 + (rng.next_u32() % 4) as usize;
            let pk = pk::<Fq>(k, bf, cs_degree);
            let omega = pk.vk.domain.omega;
            let ncols = 1 + (rng.next_u32() % 7) as usize;
            let types: Vec<Any> = (0..ncols).map(|i| match (i + case) % 3 { 0 => Any::Advice(()), 1 => Any::Fixed, _ => Any::Instance }).collect();
            let mut counts = [0usize; 3];
            let cols: Vec<PermColumn> = types.iter().map(|ty| {
                let slot = match ty { Any::Advice(_) => 0, Any::Fixed => 1, Any::Instance => 2 };
                counts[slot] += 1;
                PermColumn { ty: *ty, idx: counts[slot] - 1 }
            }).collect();
            // Valid copy constraints: a random permutation sigma of usable cells,
            // with values constant on sigma's cycles; identity on blinding rows.
            let cells = ncols * u;
            let mut sigma: Vec<usize> = (0..cells).collect();
            let valid = case % 5 != 0;
            let swaps = (rng.next_u32() as usize) % (cells + 1);
            for _ in 0..swaps {
                let (a, b) = ((rng.next_u32() as usize) % cells, (rng.next_u32() as usize) % cells);
                sigma.swap(a, b);
            }
            let p = pools(&mut rng);
            let mut val = vec![Fq::ZERO; cells];
            let mut seen = vec![false; cells];
            for s in 0..cells {
                if seen[s] { continue; }
                let v = pick(&mut rng, &p);
                let mut c = s;
                while !seen[c] { seen[c] = true; val[c] = v; c = sigma[c]; }
            }
            if !valid { val[(rng.next_u32() as usize) % cells] = Fq::random(&mut rng); }
            let mut deltas = vec![Fq::ONE; ncols];
            for j in 1..ncols { deltas[j] = deltas[j - 1] * Fq::DELTA; }
            let id = |col: usize, row: usize| deltas[col] * omega.pow_vartime([row as u64]);
            let mut adv = vec![poly(vec![]); counts[0]];
            let mut fix = vec![poly(vec![]); counts[1]];
            let mut ins = vec![poly(vec![]); counts[2]];
            let mut perms = vec![];
            for (j, c) in cols.iter().enumerate() {
                let mut v: Vec<Fq> = (0..n).map(|_| Fq::random(&mut rng)).collect();
                let mut s: Vec<Fq> = (0..n).map(|r| id(j, r)).collect();
                for r in 0..u {
                    v[r] = val[j * u + r];
                    let tgt = sigma[j * u + r];
                    s[r] = id(tgt / u, tgt % u);
                }
                match c.ty { Any::Advice(_) => adv[c.idx] = poly(v), Any::Fixed => fix[c.idx] = poly(v), Any::Instance => ins[c.idx] = poly(v) }
                perms.push(poly(s));
            }
            let (mut beta, mut gamma) = (Fq::random(&mut rng), Fq::random(&mut rng));
            if case % 7 == 3 {
                // zero denominator at some cell: value + beta*sigma + gamma = 0
                let c = (rng.next_u32() as usize) % cells;
                gamma = -(val[c] + beta * perms[c / u][c % u]);
            }
            if case % 11 == 4 { beta = Fq::ZERO; }
            let seed = rng.next_u64();
            let pkey = PermProvingKey { permutations: perms };
            let mut rs = ChaCha20Rng::seed_from_u64(seed);
            let zs = tp.install(|| stock_perm::PermArgument { columns: cols.clone() }.commit_shim(&pk, &pkey, &adv, &fix, &ins, beta, gamma, &mut rs));
            let mut rl = ChaCha20Rng::seed_from_u64(seed);
            let zl = tp.install(|| loki_perm::PermArgument { columns: cols.clone() }.commit_shim(&pk, &pkey, &adv, &fix, &ins, beta, gamma, &mut rl));
            assert_eq!(zl, zs, "permutation z differs (case {case} threads {t})");
            assert_eq!(rl.get_word_pos(), rs.get_word_pos());
            // Sets chain: each set starts at the previous set's last usable value.
            assert_eq!(zl[0][0], Fq::ONE);
            for w in zl.windows(2) { assert_eq!(w[1][0], w[0][u]); }
            let mut r = ChaCha20Rng::seed_from_u64(seed);
            for z in &zl {
                let bl: Vec<Fq> = (0..bf).map(|_| Fq::random(&mut r)).collect();
                assert_eq!(&z[u + 1..], &bl[..], "blinding rows");
            }
            let den_zero = (0..cells).any(|c| val[c] + beta * pkey.permutations[c / u][c % u] + gamma == Fq::ZERO);
            if valid && !den_zero && beta != Fq::ZERO {
                assert_eq!(zl.last().unwrap()[u], Fq::ONE, "permutation product must telescope to 1");
                telescoped += 1;
            }
            samples += 1;
        }
    }
    println!("permutation samples={samples} telescoped={telescoped}");
    assert!(samples >= 10_000 && telescoped > 1000);
}
