// Maps single-flight model operations to the pinned W6 process/disk cache.
mod mbt {
    use super::*;
    use midnight_circuits::{instructions::*, types::AssignedNative};
    use midnight_proofs::{
        circuit::{Layouter, Value},
        plonk::Error,
        utils::SerdeFormat,
    };
    use midnight_zk_stdlib::{MidnightVK, ZkStdLib};
    use quint_connect::{Driver, Result, State, Step};
    use rand::{rngs::StdRng, SeedableRng};
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct Tiny(u32);
    impl Relation for Tiny {
        type Instance = F;
        type Witness = F;
        type Error = Error;
        fn format_instance(i: &F) -> std::result::Result<Vec<F>, Error> {
            Ok(vec![*i])
        }
        fn circuit(
            &self,
            s: &ZkStdLib,
            l: &mut impl Layouter<F>,
            _: Value<F>,
            w: Value<F>,
        ) -> std::result::Result<(), Error> {
            let mut last = None;
            for _ in 0..self.0 {
                last = Some(s.assign(l, w)?);
            }
            let a: AssignedNative<F> = last.unwrap();
            s.constrain_as_public_input(l, &a)
        }
        fn write_relation<W: std::io::Write>(&self, w: &mut W) -> io::Result<()> {
            w.write_all(&self.0.to_le_bytes())
        }
        fn read_relation<Rd: std::io::Read>(r: &mut Rd) -> io::Result<Self> {
            let mut b = [0; 4];
            r.read_exact(&mut b)?;
            Ok(Self(u32::from_le_bytes(b)))
        }
    }
    #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    struct Key {
        circuit: u32,
        k: u32,
        params: u64,
    }
    const ZERO: Key = Key {
        circuit: 0,
        k: 0,
        params: 0,
    };
    #[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
    #[serde(tag = "tag")]
    enum EntryPhase {
        Empty,
        Filling,
        Ready,
    }
    #[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
    #[serde(tag = "tag")]
    enum JobPhase {
        Idle,
        Waiting,
        Used,
    }
    #[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
    struct Entry {
        phase: EntryPhase,
        owner: i64,
        builds: usize,
        binding: Key,
    }
    #[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
    struct Job {
        phase: JobPhase,
        requested: Key,
        used: Key,
    }
    #[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
    struct Snapshot {
        entries: BTreeMap<Key, Entry>,
        jobs: BTreeMap<usize, Job>,
    }
    struct Material {
        params: ParamsKZG<Bls12>,
        rel: Tiny,
        vk: MidnightVK,
        key: CacheKey,
        pk: Option<Arc<CachedPK<Tiny>>>,
        count: AtomicUsize,
        proved: bool,
    }
    #[derive(Default)]
    struct PkDriver {
        s: Snapshot,
        cache: KeyCache,
        material: BTreeMap<Key, Material>,
        dir: Option<tempfile::TempDir>,
    }
    impl State<PkDriver> for Snapshot {
        fn from_driver(d: &PkDriver) -> Result<Self> {
            Ok(d.s.clone())
        }
    }
    impl PkDriver {
        fn material(&mut self, key: Key) {
            self.material.entry(key).or_insert_with(|| {
                let params =
                    ParamsKZG::unsafe_setup(8 + key.k, StdRng::seed_from_u64(900 + key.params));
                let rel = Tiny(key.circuit + 1);
                let vk = midnight_zk_stdlib::setup_vk(&params, &rel);
                let actual = CacheKey::new(&params, &rel, &vk).unwrap();
                Material {
                    params,
                    rel,
                    vk,
                    key: actual,
                    pk: None,
                    count: AtomicUsize::new(0),
                    proved: false,
                }
            });
        }
    }
    impl Driver for PkDriver {
        type State = Snapshot;
        fn step(&mut self, step: &Step) -> Result {
            quint_connect::switch!(step {
                init => {
                    self.material.clear(); self.cache = KeyCache::default();
                    self.dir = Some(tempfile::tempdir().unwrap());
                    self.s = Snapshot::default();
                    for circuit in 0..2 { for k in 0..2 { for params in 0..2 {
                        let key = Key { circuit, k, params };
                        self.s.entries.insert(key, Entry { phase: EntryPhase::Empty, owner: -1, builds: 0, binding: key });
                    } } }
                    for job in 0..3 { self.s.jobs.insert(job, Job { phase: JobPhase::Idle, requested: ZERO, used: ZERO }); }
                },
                request(job: usize, key: Key) => {
                    self.material(key);
                    self.s.jobs.insert(job, Job { phase: JobPhase::Waiting, requested: key, used: ZERO });
                    let e = self.s.entries.get_mut(&key).unwrap();
                    if e.phase == EntryPhase::Empty { e.phase = EntryPhase::Filling; e.owner = job as i64; }
                },
                build(job: usize) => {
                    let key = self.s.jobs[&job].requested;
                    let m = &self.material[&key];
                    let dir = self.dir.as_ref().unwrap().path();
                    // Three simultaneous cold requests exercise the actual process lock
                    // and one successful keygen callback, rather than counting model calls.
                    let barrier = std::sync::Barrier::new(3);
                    let keys = std::thread::scope(|scope| {
                        let joins: Vec<_> = (0..3).map(|_| scope.spawn(|| {
                            barrier.wait();
                            self.cache.get(dir, &m.key, &m.rel, &m.vk, || {
                                m.count.fetch_add(1, Ordering::SeqCst);
                                midnight_zk_stdlib::setup_pk(&Framed(m.rel.clone()), &m.vk)
                            }).unwrap()
                        })).collect();
                        joins.into_iter().map(|j| j.join().unwrap()).collect::<Vec<_>>()
                    });
                    assert!(keys.iter().all(|pk| Arc::ptr_eq(pk, &keys[0])));
                    self.s.entries.get_mut(&key).unwrap().builds = m.count.load(Ordering::SeqCst);
                    self.material.get_mut(&key).unwrap().pk = Some(keys[0].clone());
                },
                publish(job: usize) => {
                    let key = self.s.jobs[&job].requested;
                    self.s.entries.get_mut(&key).unwrap().phase = EntryPhase::Ready;
                },
                useKey(job: usize) => {
                    let key = self.s.jobs[&job].requested;
                    let m = self.material.get_mut(&key).unwrap();
                    let pk = m.pk.as_ref().unwrap();
                    let mut actual = vec![]; let mut expected = vec![];
                    pk.0.pk().get_vk().write(&mut actual, SerdeFormat::Processed).unwrap();
                    m.vk.vk().write(&mut expected, SerdeFormat::Processed).unwrap();
                    assert!(actual == expected, "cache supplied a PK for another VK");
                    if !m.proved {
                        let proof = prove::<Tiny, blake2b_simd::State>(&m.params, pk, &m.rel,
                            &F::from(7), F::from(7), StdRng::seed_from_u64(100)).unwrap();
                        midnight_zk_stdlib::verify::<Tiny, blake2b_simd::State>(&m.params.verifier_params(),
                            &m.vk, &F::from(7), None, &proof).unwrap();
                        m.proved = true;
                    }
                    self.s.jobs.get_mut(&job).unwrap().phase = JobPhase::Used;
                    self.s.jobs.get_mut(&job).unwrap().used = key;
                },
                recycle(job: usize) => { self.s.jobs.get_mut(&job).unwrap().phase = JobPhase::Idle; }
            })
        }
    }
    #[test]
    fn public_cache_api_reuses_key() {
        let dir = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("LOKI_KEY_CACHE_DIR");
        std::env::set_var("LOKI_KEY_CACHE_DIR", dir.path());
        let params = ParamsKZG::unsafe_setup(8, StdRng::seed_from_u64(909));
        let rel = Tiny(1);
        let vk = midnight_zk_stdlib::setup_vk(&params, &rel);
        let first = setup_pk(&params, &rel, &vk).unwrap();
        let second = setup_pk(&params, &rel, &vk).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        match previous {
            Some(value) => std::env::set_var("LOKI_KEY_CACHE_DIR", value),
            None => std::env::remove_var("LOKI_KEY_CACHE_DIR"),
        }
    }
    #[quint_connect::quint_run(
        spec = "../pk_cache.qnt",
        max_samples = 4,
        max_steps = 80,
        seed = "20261024"
    )]
    fn replay_w6_cache() -> PkDriver {
        PkDriver::default()
    }
}
