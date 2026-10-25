#![allow(non_snake_case)]
//! Model-based testing (quint-connect): replay `quint/q5/lookup.qnt` traces against
//! the snapshotted stock 0.8.3 and Loki a256a9a code. The driver rebuilds each
//! action's inputs from the trace's nondet picks exactly as the model does, runs
//! the real code, and reproduces the model's `obs` record from the real outputs.
//! Value ids are ranks of a pool of field elements under `Fq::cmp`, so the
//! model's integer sort and the code's `par_sort` agree; the j-th RNG draw is
//! identified by regenerating the draw sequence from the same seed.
use ff::Field;
use loki_q5_replay::{shim::*, *};
use midnight_curves::Fq;
use quint_connect::{switch, Config, Driver, Result, State, Step};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use serde::Deserialize;
use std::collections::BTreeMap;

const TAG: i64 = 1000;
const MISSING: i64 = 6;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Obs {
    phase: String,
    ok: bool,
    a_prime: Vec<i64>,
    s_prime: Vec<i64>,
    z_blind: Vec<i64>,
    telescopes: bool,
    lookup_equal: bool,
    perm_last: String,
    perm_equal: bool,
    rat_equal: bool,
}

pub struct Q5Driver {
    obs: Obs,
    pool: Vec<Fq>, // pool[id], ascending in Fq::cmp
    seed: u64,
    rng: ChaCha20Rng,
    reference: Vec<Fq>, // reference[j] = j-th draw
    u: usize,
    bf: usize,
    threads: usize,
    det: bool,
    input: Vec<Fq>,
    table: Vec<Fq>,
    order: Vec<usize>,
    stock_p: Option<ExpressionPair<Fq>>,
    loki_p: Option<ExpressionPair<Fq>>,
    traces: u64,
    steps: u64,
}

impl Default for Q5Driver {
    fn default() -> Self {
        let mut r = ChaCha20Rng::seed_from_u64(0x9001);
        let mut pool = vec![Fq::ZERO, Fq::ONE, -Fq::ONE, Fq::from(2), Fq::from(7), Fq::random(&mut r), Fq::random(&mut r)];
        pool.sort();
        Self {
            obs: obs0(),
            pool,
            seed: 0,
            rng: ChaCha20Rng::seed_from_u64(0),
            reference: vec![],
            u: 0,
            bf: 0,
            threads: 1,
            det: false,
            input: vec![],
            table: vec![],
            order: vec![],
            stock_p: None,
            loki_p: None,
            traces: 0,
            steps: 0,
        }
    }
}

fn obs0() -> Obs {
    Obs {
        phase: "init".into(),
        ok: false,
        a_prime: vec![],
        s_prime: vec![],
        z_blind: vec![],
        telescopes: false,
        lookup_equal: true,
        perm_last: "none".into(),
        perm_equal: true,
        rat_equal: true,
    }
}

impl State<Q5Driver> for Obs {
    fn from_driver(d: &Q5Driver) -> Result<Self> {
        Ok(d.obs.clone())
    }
}

fn tpool(t: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new().num_threads(t).build().unwrap()
}

impl Q5Driver {
    fn id(&self, x: &Fq) -> i64 {
        if let Some(i) = self.pool.iter().position(|p| p == x) {
            return i as i64;
        }
        let j = self.reference.iter().position(|p| p == x).expect("value is neither data nor a draw");
        TAG + j as i64
    }
    fn ids(&self, xs: &[Fq]) -> Vec<i64> {
        xs.iter().map(|x| self.id(x)).collect()
    }
    fn small(&self, x: i64) -> Fq {
        // GF(17) model scalars used only where equality, not arithmetic, crosses the boundary
        if x == 16 { -Fq::ONE } else { Fq::from(x as u64) }
    }
    fn pk(&self) -> ProvingKey<Fq, Pcs> {
        let n = self.u + self.bf + 1;
        // the shim domain only needs n; pick k with 2^k = n when possible (u+bf+1 may not be a power of two)
        let mut p = pk::<Fq>(3, self.bf, 4);
        p.vk.domain.n = n as u64;
        p
    }

    fn init(&mut self, u0: i64, bf0: i64, t0: i64, det0: bool, tbl_m: BTreeMap<i64, i64>, in_m: BTreeMap<i64, i64>, missing: bool, prio: BTreeMap<i64, i64>) {
        let (u, bf) = (u0 as usize, bf0 as usize);
        let tbl: Vec<i64> = (0..u).map(|i| tbl_m[&(i as i64)]).collect();
        let mut inp: Vec<i64> = (0..u).map(|i| tbl[(in_m[&(i as i64)] as usize) % u]).collect();
        if missing {
            inp[0] = MISSING;
        }
        let mut keys: Vec<i64> = tbl.clone();
        keys.sort();
        keys.dedup();
        let mut ordered = vec![];
        for p in 0..6 {
            ordered.extend(keys.iter().copied().filter(|k| prio[k] == p));
        }
        // ShimMap order: position j holds the rank (within the sorted present keys) of ordered[j]
        self.order = ordered.iter().map(|k| keys.iter().position(|x| x == k).unwrap()).collect();
        self.traces += 1;
        self.seed = 0x5_0000 + self.traces;
        self.rng = ChaCha20Rng::seed_from_u64(self.seed);
        let mut r = ChaCha20Rng::seed_from_u64(self.seed);
        self.reference = (0..64).map(|_| Fq::random(&mut r)).collect();
        let n = u + bf + 1;
        let mut junk = ChaCha20Rng::seed_from_u64(!self.seed);
        self.input = (0..n).map(|i| if i < u { self.pool[inp[i] as usize] } else { Fq::random(&mut junk) }).collect();
        self.table = (0..n).map(|i| if i < u { self.pool[tbl[i] as usize] } else { Fq::random(&mut junk) }).collect();
        self.u = u;
        self.bf = bf;
        self.threads = t0 as usize;
        self.det = det0;
        self.stock_p = None;
        self.loki_p = None;
        self.obs = obs0();
    }

    fn permute(&mut self) {
        let pk = self.pk();
        let dom = pk.vk.domain.clone();
        let (ip, tp) = (poly(self.input.clone()), poly(self.table.clone()));
        let order = Order::Perm(self.order.clone());
        let mut rs = self.rng.clone();
        let stock = with_order(order.clone(), || stock_permute::permute_expression_pair::<Fq, Pcs>(&pk, &dom, &mut rs, &ip, &tp));
        let pool = tpool(self.threads);
        let mut rl = self.rng.clone();
        let loki = pool.install(|| {
            with_order(order.clone(), || {
                if self.det {
                    loki_permute_det::permute_expression_pair::<Fq, Pcs>(&pk, &dom, &mut rl, &ip, &tp)
                } else {
                    loki_permute_default::permute_expression_pair::<Fq, Pcs>(&pk, &dom, &mut rl, &ip, &tp)
                }
            })
        });
        let reference = if self.det {
            let mut r = self.rng.clone();
            with_order(Order::Sorted, || stock_permute::permute_expression_pair::<Fq, Pcs>(&pk, &dom, &mut r, &ip, &tp))
        } else {
            stock.clone()
        };
        assert_eq!(rl.get_word_pos(), rs.get_word_pos(), "RNG consumption differs from stock");
        self.rng = rl;
        let equal = loki == reference;
        match &loki {
            Ok((a, s)) => {
                self.obs = Obs { phase: "permuted".into(), ok: true, a_prime: self.ids(a), s_prime: self.ids(s), lookup_equal: equal, ..self.obs.clone() };
            }
            Err(_) => {
                self.obs = Obs { phase: "error".into(), ok: false, a_prime: vec![], s_prime: vec![], lookup_equal: equal, ..self.obs.clone() };
            }
        }
        self.stock_p = reference.ok();
        self.loki_p = loki.ok();
    }

    fn product(&mut self, beta_hit: i64, gamma_hit: i64) {
        let pk = self.pk();
        let (sa, ss) = self.stock_p.clone().unwrap();
        let (la, ls) = self.loki_p.clone().unwrap();
        let mut gen = ChaCha20Rng::seed_from_u64(self.seed ^ 0xbeef);
        let beta = if beta_hit >= 0 { -la[beta_hit as usize] } else { Fq::random(&mut gen) };
        let gamma = if gamma_hit >= 0 { -ls[gamma_hit as usize] } else { Fq::random(&mut gen) };
        let mut rs = self.rng.clone();
        let zs = stock_product::Permuted {
            compressed_input_expression: poly(self.input.clone()),
            permuted_input_expression: sa,
            compressed_table_expression: poly(self.table.clone()),
            permuted_table_expression: ss,
        }
        .product(&pk, beta, gamma, &mut rs);
        let mut rl = self.rng.clone();
        let zl = tpool(self.threads).install(|| {
            loki_product::Permuted {
                compressed_input_expression: poly(self.input.clone()),
                permuted_input_expression: la,
                compressed_table_expression: poly(self.table.clone()),
                permuted_table_expression: ls,
            }
            .product(&pk, beta, gamma, &mut rl)
        });
        assert_eq!(rl.get_word_pos(), rs.get_word_pos());
        self.rng = rl;
        let u = self.u;
        self.obs = Obs {
            phase: "product".into(),
            z_blind: self.ids(&zl[u + 1..]),
            telescopes: zl[u] == Fq::ONE,
            lookup_equal: self.obs.lookup_equal && zl == zs,
            ..self.obs.clone()
        };
    }

    #[allow(clippy::too_many_arguments)]
    fn perm_arg(&mut self, n8: bool, cols_sel: i64, chunk: i64, pbf: i64, val_m: BTreeMap<i64, i64>, cyc: BTreeMap<i64, i64>, valid: bool, hit: bool) {
        let n: usize = if n8 { 8 } else { 4 };
        let k = if n8 { 3 } else { 2 };
        let cols = if n8 && cols_sel > 2 { 2 } else { cols_sel as usize };
        let pbf = pbf as usize;
        let pu = n - pbf - 1;
        let pk = pk::<Fq>(k, pbf, chunk as usize + 2);
        let omega = pk.vk.domain.omega;
        let cells: Vec<usize> = (0..cols * n).filter(|c| c % n < pu).collect();
        let cycle: Vec<usize> = cells.iter().copied().filter(|c| cyc[&(*c as i64)] == 1).collect();
        let sig: Vec<usize> = (0..cols * n)
            .map(|c| match cycle.iter().position(|x| *x == c) {
                None => c,
                Some(i) => cycle[(i + 1) % cycle.len()],
            })
            .collect();
        let cv = cycle.first().map(|c| val_m[&(*c as i64)]).unwrap_or(0);
        let mut vals: Vec<i64> = (0..cols * n).map(|c| if cycle.contains(&c) { cv } else { val_m[&(c as i64)] }).collect();
        let broken = !valid && cycle.len() > 1;
        if broken {
            vals[cycle[0]] = (cv + 1) % 17;
        }
        let mut deltas = vec![Fq::ONE; cols];
        for j in 1..cols {
            deltas[j] = deltas[j - 1] * Fq::DELTA;
        }
        let idv = |cell: usize| deltas[cell / n] * omega.pow_vartime([(cell % n) as u64]);
        let mut gen = ChaCha20Rng::seed_from_u64(self.seed ^ 0xfeed);
        let beta = Fq::random(&mut gen);
        let c0 = cells[0];
        let gamma = if hit { -(self.pool[vals[c0] as usize] + beta * idv(sig[c0])) } else { Fq::random(&mut gen) };
        let types: Vec<Any> = (0..cols).map(|i| match i % 3 { 0 => Any::Advice(()), 1 => Any::Fixed, _ => Any::Instance }).collect();
        let mut counts = [0usize; 3];
        let mut adv = vec![];
        let mut fix = vec![];
        let mut ins = vec![];
        let mut columns = vec![];
        let mut perms = vec![];
        for (j, ty) in types.iter().enumerate() {
            let v: Vec<Fq> = (0..n).map(|r| if r < pu { self.pool[vals[j * n + r] as usize] } else { Fq::random(&mut gen) }).collect();
            let s: Vec<Fq> = (0..n).map(|r| idv(sig[j * n + r])).collect();
            let slot = match ty { Any::Advice(_) => 0, Any::Fixed => 1, Any::Instance => 2 };
            match slot { 0 => adv.push(poly(v)), 1 => fix.push(poly(v)), _ => ins.push(poly(v)) }
            columns.push(PermColumn { ty: *ty, idx: counts[slot] });
            counts[slot] += 1;
            perms.push(poly(s));
        }
        let pkey = PermProvingKey { permutations: perms };
        let mut rs = self.rng.clone();
        let zs = stock_perm::PermArgument { columns: columns.clone() }.commit_shim(&pk, &pkey, &adv, &fix, &ins, beta, gamma, &mut rs);
        let mut rl = self.rng.clone();
        let zl = tpool(self.threads).install(|| loki_perm::PermArgument { columns: columns.clone() }.commit_shim(&pk, &pkey, &adv, &fix, &ins, beta, gamma, &mut rl));
        assert_eq!(rl.get_word_pos(), rs.get_word_pos());
        self.rng = rl;
        let last = zl.last().unwrap()[pu];
        let class = if last == Fq::ONE { "one" } else if last == Fq::ZERO { "zero" } else { "other" };
        self.obs = Obs { phase: "perm".into(), perm_equal: zs == zl, perm_last: class.into(), ..self.obs.clone() };
    }

    fn rational(&mut self, kinds: BTreeMap<i64, i64>, nums: BTreeMap<i64, i64>, dens: BTreeMap<i64, i64>, len: i64) {
        let xs: Vec<Rational<Fq>> = (0..len)
            .map(|i| match kinds[&i] {
                0 => Rational::Zero,
                1 => Rational::Trivial(self.small(nums[&i])),
                _ => Rational::Rational(self.small(nums[&i]), match dens[&i] { 0 => Fq::ZERO, 1 => Fq::ONE, 2 => Fq::from(2), _ => -Fq::ONE }),
            })
            .collect();
        let s = stock_rational::batch_invert_rational(vec![poly(xs.clone())]);
        let l = tpool(self.threads).install(|| loki_rational::batch_invert_rational(vec![poly(xs.clone())]));
        let eval: Vec<Fq> = xs.iter().map(|x| x.evaluate()).collect();
        self.obs = Obs { phase: "done".into(), rat_equal: s == l && l[0].values == eval, ..self.obs.clone() };
    }
}

impl Driver for Q5Driver {
    type State = Obs;
    fn config() -> Config {
        Config { state: &["lookup_ok::lookup::obs"], ..Config::default() }
    }
    fn step(&mut self, step: &Step) -> Result {
        self.steps += 1;
        switch!(step {
            init(u0: i64, bf0: i64, t0: i64, det0: bool, tblM: BTreeMap<i64, i64>, inM: BTreeMap<i64, i64>, missing: bool, prio: BTreeMap<i64, i64>) => {
                self.init(u0, bf0, t0, det0, tblM, inM, missing, prio);
            },
            permute => { self.permute(); },
            idle => {},
            product(betaHit: i64, gammaHit: i64) => { self.product(betaHit, gammaHit); },
            permArg(n8: bool, colsSel: i64, chunk: i64, pbf: i64, valM: BTreeMap<i64, i64>, cyc: BTreeMap<i64, i64>, valid: bool, hit: bool) => {
                self.perm_arg(n8, colsSel, chunk, pbf, valM, cyc, valid, hit);
            },
            rational(kinds: BTreeMap<i64, i64>, nums: BTreeMap<i64, i64>, dens: BTreeMap<i64, i64>, len: i64) => {
                self.rational(kinds, nums, dens, len);
            }
        })
    }
}

#[quint_connect::quint_run(spec = "../lookup.qnt", main = "lookup_ok", max_samples = 2000, max_steps = 5, seed = "0x51515")]
fn q5_lookup_mbt() -> Q5Driver {
    Q5Driver::default()
}

// Non-vacuity: the same driver replayed against mutated models must be rejected,
// i.e. the real code's observable state disagrees with each mutation.
macro_rules! mutant {
    ($ty:ident, $path:literal, $test:ident, $main:literal) => {
        #[derive(Default)]
        pub struct $ty(Q5Driver);
        impl State<$ty> for Obs {
            fn from_driver(d: &$ty) -> Result<Self> {
                Ok(d.0.obs.clone())
            }
        }
        impl Driver for $ty {
            type State = Obs;
            fn config() -> Config {
                Config { state: &[$path], ..Config::default() }
            }
            fn step(&mut self, step: &Step) -> Result {
                self.0.step(step)
            }
        }
        #[quint_connect::quint_run(spec = "../lookup.qnt", main = $main, max_samples = 300, max_steps = 5, seed = "0x51515")]
        #[should_panic]
        fn $test() -> $ty {
            $ty::default()
        }
    };
}
mutant!(Mut1, "mut1::lookup::obs", mbt_rejects_mut1_reversed_leftovers, "mut1");
mutant!(Mut2, "mut2::lookup::obs", mbt_rejects_mut2_table_draws_first, "mut2");
mutant!(Mut9, "mut9::lookup::obs", mbt_rejects_mut9_descending_sort, "mut9");
