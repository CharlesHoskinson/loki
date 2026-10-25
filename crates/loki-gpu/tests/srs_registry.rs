//! P3 SRS registry contract.
//!
//! Part 1 runs without a toolkit or device: the registry sits on a
//! `GpuRuntime` over the fault-injecting fake driver, which executes copies
//! lazily at synchronization and records lifetime violations. Every resident
//! hit is checked by downloading the device bytes and comparing them with the
//! codec encoding of the exact requested points.
//!
//! Part 2 (`--features cuda`) repeats the core cases on the native runtime.
//! A missing device fails the gate unless `LOKI_GPU_ALLOW_NO_DEVICE=1`.
//!
//! `ToyPoint` is not a curve. The test codec models curve/subgroup checks with
//! explicit reserved values so validation failures are deterministic; the real
//! BLS12-381 checks are exercised by Loki's codec tests.

mod support;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use loki_backend_api::{
    AllocationId, BackendError, CurveAbi, DeviceGeneration, ErrorKind, FailureEffect,
    MsmLengthPolicy, ResidentBackend, ResidentBuffer, SrsBasis, SrsCacheKey, SrsLease,
    SrsLeaseId, SrsRegistry, SrsRequest,
};
use loki_gpu::runtime::fake::{DriverCall, FakeConfig, FakeDriver, Fault};
use loki_gpu::runtime::{DriverErrorClass, GpuRuntime, RuntimeConfig};
use loki_gpu::srs::{parse_budget, GpuSrsRegistry, SrsPointCodec, SrsPointError, SrsStats};
use support::{Toy, ToyPoint, P};

const TOY_ABI: CurveAbi = CurveAbi {
    curve_id: 0x7e57,
    encoding_version: 1,
};
const WIRE: usize = 8;

/// Reserved low 16 bits marking a point "off curve".
const OFF_CURVE_TAG: u64 = 0xbad0;
/// Reserved low 16 bits marking a point "outside the subgroup".
const TORSION_TAG: u64 = 0x7051;

#[derive(Default)]
struct ToyCodec {
    validated_points: AtomicU64,
}

impl SrsPointCodec<ToyPoint> for ToyCodec {
    fn curve_abi(&self) -> CurveAbi {
        TOY_ABI
    }
    fn wire_bytes(&self) -> usize {
        WIRE
    }
    fn validate_encode(&self, points: &[ToyPoint], wire: &mut [u8]) -> Result<(), SrsPointError> {
        self.validated_points
            .fetch_add(points.len() as u64, Ordering::Relaxed);
        for (index, (p, out)) in points.iter().zip(wire.chunks_exact_mut(WIRE)).enumerate() {
            let v = p.0 .0;
            if v >= P {
                return Err(SrsPointError::Malformed { index });
            }
            match v & 0xffff {
                OFF_CURVE_TAG => return Err(SrsPointError::OffCurve { index }),
                TORSION_TAG => return Err(SrsPointError::NotInSubgroup { index }),
                _ => {}
            }
            out.copy_from_slice(&encode_one(p));
        }
        Ok(())
    }
    fn same_representation(&self, a: &ToyPoint, b: &ToyPoint) -> bool {
        a.0 .0 == b.0 .0
    }
}

fn encode_one(p: &ToyPoint) -> [u8; WIRE] {
    // Independent of the codec: a fixed affine transform so a raw-value
    // upload could not pass the residency check by accident.
    (p.0 .0 ^ 0x5a5a_5a5a_5a5a_5a5a).to_le_bytes()
}

fn encode(points: &[ToyPoint]) -> Vec<u8> {
    points.iter().flat_map(encode_one).collect()
}

/// Deterministic valid points; `seed` separates arrays.
fn points(n: usize, seed: u64) -> Vec<ToyPoint> {
    (0..n as u64)
        .map(|i| {
            let mut v = (i.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ seed.wrapping_mul(0xd1b5_4a32_d192_ed03)) % P;
            if matches!(v & 0xffff, OFF_CURVE_TAG | TORSION_TAG) {
                v ^= 1;
            }
            ToyPoint(Toy::new(v))
        })
        .collect()
}

struct Env {
    fake: Arc<FakeDriver>,
    rt: &'static GpuRuntime,
    codec: Arc<ToyCodec>,
    reg: GpuSrsRegistry<ToyPoint>,
}

fn env_with(budget: u64, device_cap: u64) -> Env {
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    let rt = Box::leak(Box::new(GpuRuntime::with_driver(
        fake.clone(),
        RuntimeConfig {
            device_budget_cap: Some(device_cap),
            device_headroom_bytes: 0,
            staging_slots: 2,
            staging_slot_bytes: 1 << 20,
            ..RuntimeConfig::default()
        },
    )));
    rt.initialize().expect("fake device initializes");
    let codec = Arc::new(ToyCodec::default());
    let reg = GpuSrsRegistry::with_runtime(rt, codec.clone(), budget);
    Env {
        fake,
        rt,
        codec,
        reg,
    }
}

fn env(budget: u64) -> Env {
    env_with(budget, 1 << 30)
}

type Reg = dyn SrsRegistry<Toy, ToyPoint>;

impl Env {
    fn r(&self) -> &Reg {
        &self.reg
    }
    fn gen(&self) -> DeviceGeneration {
        ResidentBackend::<Toy, ToyPoint>::generation(self.rt).unwrap()
    }
    fn key(&self, basis: SrsBasis, k: u32, n: usize) -> SrsCacheKey {
        SrsCacheKey {
            curve_abi: TOY_ABI,
            basis,
            domain_log: k,
            point_count: n,
            generation: self.gen(),
            content_identity: [0; 32],
        }
    }
    fn acquire(&self, basis: SrsBasis, k: u32, pts: &[ToyPoint]) -> Result<SrsLease<ToyPoint>, BackendError> {
        let key = self.key(basis, k, pts.len());
        self.r().acquire_srs(SrsRequest { key, points: pts })
    }
    fn coeff(&self, k: u32, pts: &[ToyPoint]) -> SrsLease<ToyPoint> {
        self.acquire(SrsBasis::Coefficient, k, pts).expect("acquire")
    }
    fn stats(&self) -> SrsStats {
        self.reg.stats()
    }
    /// Every hit/miss proves exact residency of the requested points.
    fn assert_resident(&self, lease: &SrsLease<ToyPoint>, pts: &[ToyPoint]) {
        let wire = self.reg.resident_wire(lease).expect("resident download");
        assert_eq!(wire, encode(pts), "device bytes equal the requested bases");
    }
    fn release(&self, lease: SrsLease<ToyPoint>) {
        self.r().release_srs(lease).expect("release");
    }
    fn clean(&self) {
        let v = self.fake.violations();
        assert!(v.is_empty(), "fake driver recorded violations: {v:?}");
    }
    fn allocs(&self) -> usize {
        self.fake.calls(DriverCall::DeviceAlloc)
    }
}

fn assert_err<T>(r: Result<T, BackendError>, kind: ErrorKind, effect: FailureEffect) {
    match r {
        Ok(_) => panic!("expected {kind:?}/{effect:?}, got success"),
        Err(e) => {
            assert_eq!((e.kind, e.effect), (kind, effect), "{e:?}");
            assert!(e.preserves_observable_state());
        }
    }
}

fn forge(lease: &SrsLease<ToyPoint>, id: Option<u64>, key: Option<SrsCacheKey>, alloc: Option<u64>) -> SrsLease<ToyPoint> {
    let b = lease.bases();
    SrsLease::from_untrusted_metadata(
        SrsLeaseId(id.unwrap_or(lease.lease_id().0)),
        key.unwrap_or(lease.key()),
        lease.generation(),
        ResidentBuffer::from_untrusted_metadata(
            b.generation(),
            AllocationId(alloc.unwrap_or(b.allocation().0)),
            b.len(),
            b.bytes(),
        ),
    )
}

// --------------------------------------------------------------------------
// Rewarm, same-address mutation, distinct-address equality, prefixes
// --------------------------------------------------------------------------

#[test]
fn rewarm_then_same_address_mutation_misses() {
    let e = env(1 << 30);
    let mut a = points(1024, 1);
    let l = e.coeff(10, &a);
    e.assert_resident(&l, &a);
    e.release(l);
    // Rewarm A immediately before mutating it at the same address.
    let l = e.coeff(10, &a);
    assert_eq!((e.stats().hits, e.stats().misses, e.stats().uploads), (1, 1, 1));
    e.assert_resident(&l, &a);
    e.release(l);
    let addr = a.as_ptr();
    a[517] = ToyPoint(Toy::new(42));
    assert_eq!(a.as_ptr(), addr, "same host address");
    let l = e.coeff(10, &a);
    let s = e.stats();
    assert_eq!((s.hits, s.misses, s.uploads), (1, 2, 2), "mutation is a miss");
    e.assert_resident(&l, &a);
    e.release(l);
    // Mutating the very first or last point is also caught.
    for idx in [0, 1023] {
        let before = e.stats().uploads;
        a[idx] = ToyPoint(Toy::new(7 + idx as u64));
        let l = e.coeff(10, &a);
        assert_eq!(e.stats().uploads, before + 1);
        e.assert_resident(&l, &a);
        e.release(l);
    }
    e.clean();
}

#[test]
fn equal_content_at_distinct_addresses_hits() {
    let e = env(1 << 30);
    let a = points(5000, 2);
    let b = a.clone();
    assert_ne!(a.as_ptr(), b.as_ptr());
    let la = e.coeff(13, &a);
    let allocs = e.allocs();
    let lb = e.coeff(13, &b);
    assert_eq!(e.allocs(), allocs, "no second upload");
    let s = e.stats();
    assert_eq!((s.hits, s.misses, s.uploads, s.resident_entries), (1, 1, 1, 1));
    assert_eq!(lb.bases().allocation(), la.bases().allocation());
    e.assert_resident(&lb, &b);
    // A caller digest is never trusted: same digest, different content misses.
    let mut c = a.clone();
    c[4999] = ToyPoint(Toy::new(3));
    let lc = e.coeff(13, &c);
    assert_eq!(e.stats().uploads, 2);
    e.assert_resident(&lc, &c);
    for l in [la, lb, lc] {
        e.release(l);
    }
    e.clean();
}

#[test]
fn n_and_n_minus_one_prefixes() {
    let e = env(1 << 30);
    let n = 4096 + 17; // exercises the parallel comparison path
    let mut a = points(n, 3);
    let full = e.coeff(13, &a);
    let prefix = e.coeff(13, &a[..n - 1]);
    let s = e.stats();
    assert_eq!((s.hits, s.misses, s.uploads), (1, 1, 1), "n-1 prefix reuses the n upload");
    assert_eq!(prefix.bases().allocation(), full.bases().allocation());
    assert_eq!(prefix.key().point_count, n - 1);
    e.assert_resident(&prefix, &a[..n - 1]);
    e.release(full);
    e.release(prefix);
    // An unused suffix may change: the n-1 request still hits.
    a[n - 1] = ToyPoint(Toy::new(99));
    let l = e.coeff(13, &a[..n - 1]);
    assert_eq!(e.stats().uploads, 1);
    e.assert_resident(&l, &a[..n - 1]);
    e.release(l);
    // ... but the full request consumes it and must miss.
    let l = e.coeff(13, &a);
    assert_eq!(e.stats().uploads, 2);
    e.assert_resident(&l, &a);
    e.release(l);
    // A change inside the n-1 prefix misses for both lengths.
    a[n - 2] = ToyPoint(Toy::new(100));
    let l = e.coeff(13, &a[..n - 1]);
    assert_eq!(e.stats().uploads, 3);
    e.assert_resident(&l, &a[..n - 1]);
    e.release(l);
    // A longer request never hits a shorter entry.
    let short = points(64, 9);
    let long = {
        let mut v = short.clone();
        v.extend(points(1, 10));
        v
    };
    let l1 = e.coeff(7, &short);
    let l2 = e.coeff(7, &long);
    assert_ne!(l1.bases().allocation(), l2.bases().allocation());
    e.assert_resident(&l2, &long);
    e.release(l1);
    e.release(l2);
    e.clean();
}

#[test]
fn k19_k20_coefficient_versus_lagrange() {
    let e = env(1 << 30);
    let coeff20 = points(1 << 20, 20);
    let lag20 = points(1 << 20, 21);
    let lag19 = points(1 << 19, 22);
    // Interleaved: k20 coefficient, k20 Lagrange, k19 Lagrange.
    let c20 = e.acquire(SrsBasis::Coefficient, 20, &coeff20).unwrap();
    let l20 = e.acquire(SrsBasis::Lagrange, 20, &lag20).unwrap();
    let l19 = e.acquire(SrsBasis::Lagrange, 19, &lag19).unwrap();
    assert_eq!(e.stats().uploads, 3);
    // k19 coefficient = prefix of the k20 coefficient basis: hit.
    let c19 = e.acquire(SrsBasis::Coefficient, 19, &coeff20[..1 << 19]).unwrap();
    assert_eq!(c19.bases().allocation(), c20.bases().allocation());
    assert_eq!(e.stats().uploads, 3);
    e.assert_resident(&c19, &coeff20[..1 << 19]);
    // A k19 Lagrange request whose points happen to equal the k20 Lagrange
    // prefix is a different domain basis: miss, separate allocation.
    let fake19 = e.acquire(SrsBasis::Lagrange, 19, &lag20[..1 << 19]).unwrap();
    assert_eq!(e.stats().uploads, 4);
    assert_ne!(fake19.bases().allocation(), l20.bases().allocation());
    // Same content, other basis: miss.
    let wrong = e.acquire(SrsBasis::Lagrange, 20, &coeff20).unwrap();
    assert_eq!(e.stats().uploads, 5);
    assert_ne!(wrong.bases().allocation(), c20.bases().allocation());
    // Mixed-order re-requests all hit their own entries.
    for (basis, k, pts, alloc) in [
        (SrsBasis::Lagrange, 19, &lag19[..], l19.bases().allocation()),
        (SrsBasis::Coefficient, 20, &coeff20[..(1 << 20) - 1], c20.bases().allocation()),
        (SrsBasis::Lagrange, 20, &lag20[..12345], l20.bases().allocation()),
        (SrsBasis::Coefficient, 19, &coeff20[..1 << 19], c20.bases().allocation()),
    ] {
        let l = e.acquire(basis, k, pts).unwrap();
        assert_eq!(l.bases().allocation(), alloc, "{basis:?} k{k}");
        e.assert_resident(&l, pts);
        e.release(l);
    }
    assert_eq!(e.stats().uploads, 5);
    // A larger coefficient request cannot claim a smaller domain.
    assert_err(
        e.acquire(SrsBasis::Coefficient, 19, &coeff20),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    let s = e.stats();
    assert_eq!(s.evictions, 0);
    assert_eq!(s.resident_bytes, ((3 << 20) + (2 << 19)) as u64 * WIRE as u64);
    for l in [c20, l20, l19, c19, fake19, wrong] {
        e.release(l);
    }
    e.clean();
}

// --------------------------------------------------------------------------
// Validation
// --------------------------------------------------------------------------

#[test]
fn corrupted_off_curve_and_non_subgroup_inputs_are_rejected_before_upload() {
    let e = env(1 << 30);
    let good = points(256, 4);
    let cases: [(usize, ToyPoint); 4] = [
        (0, ToyPoint(Toy(P + 5))),                      // corrupted / non-canonical
        (100, ToyPoint(Toy::new(0x1234_0000 | OFF_CURVE_TAG))), // off curve
        (255, ToyPoint(Toy::new(0x9999_0000 | TORSION_TAG))),   // outside subgroup
        (200, ToyPoint(Toy(u64::MAX))),
    ];
    for (idx, bad) in cases {
        let mut pts = good.clone();
        pts[idx] = bad;
        let allocs = e.allocs();
        assert_err(
            e.acquire(SrsBasis::Coefficient, 8, &pts),
            ErrorKind::InvalidInput,
            FailureEffect::NotStarted,
        );
        assert_eq!(e.allocs(), allocs, "no device allocation for a rejected input");
        // The valid prefix before the bad point is still a fresh, valid miss.
        if idx > 0 {
            let l = e.coeff(8, &pts[..idx]);
            e.assert_resident(&l, &pts[..idx]);
            e.release(l);
        }
    }
    let s = e.stats();
    assert_eq!(s.rejected_invalid, 4);
    assert_eq!(s.resident_bytes, s.uploaded_bytes, "only valid uploads became resident");
    // A cached valid entry never vouches for a mutated, invalid request.
    let l = e.coeff(8, &good);
    e.release(l);
    let mut pts = good.clone();
    pts[3] = ToyPoint(Toy::new(OFF_CURVE_TAG));
    assert_err(e.acquire(SrsBasis::Coefficient, 8, &pts), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    // Whole-request validation: an invalid point anywhere rejects the entry.
    assert!(e.codec.validated_points.load(Ordering::Relaxed) > 0);
    e.clean();
}

#[test]
fn malformed_requests_are_rejected_before_any_work() {
    let e = env(1 << 30);
    let pts = points(16, 5);
    let validated = e.codec.validated_points.load(Ordering::Relaxed);
    let mut key = e.key(SrsBasis::Coefficient, 4, 16);
    key.point_count = 15;
    assert_err(e.r().acquire_srs(SrsRequest { key, points: &pts }), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_err(e.acquire(SrsBasis::Coefficient, 3, &pts), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_err(e.acquire(SrsBasis::Coefficient, 4, &[]), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    let mut key = e.key(SrsBasis::Coefficient, 4, 16);
    key.curve_abi.curve_id += 1;
    assert_err(e.r().acquire_srs(SrsRequest { key, points: &pts }), ErrorKind::Unsupported, FailureEffect::NotStarted);
    let mut key = e.key(SrsBasis::Coefficient, 4, 16);
    key.generation.generation += 1;
    assert_err(e.r().acquire_srs(SrsRequest { key, points: &pts }), ErrorKind::ContextLost, FailureEffect::NotStarted);
    assert_eq!(e.codec.validated_points.load(Ordering::Relaxed), validated);
    assert_eq!(e.stats().uploads, 0);
    // Uninitialized runtime: Unsupported.
    let fake = Arc::new(FakeDriver::new(FakeConfig::default()));
    let rt = Box::leak(Box::new(GpuRuntime::with_driver(fake, RuntimeConfig::default())));
    let reg = GpuSrsRegistry::with_runtime(rt, Arc::new(ToyCodec::default()), 1 << 20);
    let r: &Reg = &reg;
    assert_err(
        r.acquire_srs(SrsRequest { key: e.key(SrsBasis::Coefficient, 4, 16), points: &pts }),
        ErrorKind::Unsupported,
        FailureEffect::NotStarted,
    );
    assert_eq!(parse_budget(None).unwrap(), 4 << 30);
    assert_eq!(parse_budget(Some("8388608")).unwrap(), 8 << 20);
    assert!(parse_budget(Some("bad")).is_err());
    assert!(parse_budget(Some("-1")).is_err());
    assert!(parse_budget(Some("")).is_err());
    assert!(parse_budget(Some(" 1")).is_err());
}

// --------------------------------------------------------------------------
// Leases, eviction and budget
// --------------------------------------------------------------------------

#[test]
fn leased_entries_are_never_evicted() {
    let n = 1024;
    let bytes = (n * WIRE) as u64;
    let e = env(bytes); // room for exactly one entry
    let a = points(n, 6);
    let b = points(n, 7);
    let la = e.coeff(10, &a);
    let allocs = e.allocs();
    assert_err(e.acquire(SrsBasis::Coefficient, 10, &b), ErrorKind::Capacity, FailureEffect::NotStarted);
    assert_eq!(e.allocs(), allocs, "refused before upload");
    assert_eq!(e.stats().evictions, 0);
    e.assert_resident(&la, &a); // still resident and intact
    let la2 = e.coeff(10, &a); // second lease on the same entry
    e.release(la);
    assert_err(e.acquire(SrsBasis::Coefficient, 10, &b), ErrorKind::Capacity, FailureEffect::NotStarted);
    let la2_meta = forge(&la2, None, None, None);
    e.release(la2);
    // Now unleased: B evicts A.
    let lb = e.coeff(10, &b);
    let s = e.stats();
    assert_eq!((s.evictions, s.evicted_bytes, s.resident_entries), (1, bytes, 1));
    assert_eq!(e.fake.calls(DriverCall::DeviceFree), 1);
    e.assert_resident(&lb, &b);
    // A released/evicted lease is no longer usable.
    assert_err(e.reg.resident_wire(&la2_meta), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_err(e.r().srs_msm(&la2_meta, &[Toy::new(1)], MsmLengthPolicy::ZipTruncate), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_err(e.r().release_srs(la2_meta), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    // A forgotten (never released) lease pins its entry permanently.
    std::mem::forget(lb);
    assert_err(e.acquire(SrsBasis::Coefficient, 10, &a), ErrorKind::Capacity, FailureEffect::NotStarted);
    assert_eq!(e.stats().active_leases, 1);
    e.clean();
}

#[test]
fn eviction_is_lru_not_fifo() {
    let n = 512;
    let bytes = (n * WIRE) as u64;
    let e = env(2 * bytes);
    let a = points(n, 11);
    let b = points(n, 12);
    let c = points(n, 13);
    for pts in [&a, &b, &a] {
        let l = e.coeff(9, pts);
        e.release(l);
    }
    // A was used most recently: C evicts B.
    let l = e.coeff(9, &c);
    e.release(l);
    let before = e.stats().uploads;
    let l = e.coeff(9, &a);
    assert_eq!(e.stats().uploads, before, "A survived");
    e.assert_resident(&l, &a);
    e.release(l);
    let l = e.coeff(9, &b);
    assert_eq!(e.stats().uploads, before + 1, "B was evicted");
    e.release(l);
    let s = e.stats();
    assert_eq!(s.evictions, 2);
    assert!(s.high_water_bytes <= 2 * bytes, "budget never exceeded: {s:?}");
    e.clean();
}

#[test]
fn oversized_entries_are_refused_before_upload() {
    let e = env(4096);
    let pts = points(4096 / WIRE + 1, 14);
    assert_err(e.acquire(SrsBasis::Coefficient, 10, &pts), ErrorKind::Capacity, FailureEffect::NotStarted);
    assert_eq!(e.allocs(), 0);
    assert_eq!(e.codec.validated_points.load(Ordering::Relaxed), 0, "refused before validation work");
    let l = e.coeff(9, &pts[..4096 / WIRE]); // exactly the budget fits
    e.release(l);
    assert_eq!(e.reg.persistent_bytes_for(usize::MAX), None);
    // Zero budget disables residency explicitly.
    let z = env(0);
    assert_err(z.acquire(SrsBasis::Coefficient, 4, &points(4, 1)), ErrorKind::Capacity, FailureEffect::NotStarted);
    // The shared runtime budget can refuse independently of the SRS budget.
    let small = env_with(1 << 30, 1000);
    assert_err(small.acquire(SrsBasis::Coefficient, 9, &points(512, 2)), ErrorKind::Capacity, FailureEffect::NotStarted);
    let s = small.stats();
    assert_eq!((s.resident_entries, s.pending_bytes, s.uploads), (0, 0, 0));
    assert_eq!(e.stats().rejected_capacity, 1);
    e.clean();
}

// --------------------------------------------------------------------------
// Context loss, failure atomicity, metadata forgery
// --------------------------------------------------------------------------

#[test]
fn leases_across_context_loss() {
    let e = env(1 << 30);
    let a = points(2048, 15);
    let old_gen = e.gen();
    let la = e.coeff(11, &a);
    e.assert_resident(&la, &a);
    let stale_key = la.key();
    // Fatal device error, then the only path to a new generation.
    e.fake.inject(DriverCall::DeviceAlloc, 1, Fault::Fail(DriverErrorClass::ContextLost));
    assert_err(
        ResidentBackend::<Toy, ToyPoint>::upload_scalars(e.rt, &[Toy::new(1)]),
        ErrorKind::ContextLost,
        FailureEffect::SessionPoisoned,
    );
    // While poisoned: no acquire, the lease is refused.
    assert_err(e.acquire(SrsBasis::Coefficient, 11, &a), ErrorKind::Poisoned, FailureEffect::SessionPoisoned);
    assert_err(e.reg.resident_wire(&la), ErrorKind::Poisoned, FailureEffect::SessionPoisoned);
    let new_gen = e.rt.reinitialize().unwrap();
    assert_ne!(new_gen, old_gen);
    assert_err(e.r().srs_msm(&la, &[Toy::new(1)], MsmLengthPolicy::ZipTruncate), ErrorKind::ContextLost, FailureEffect::NotStarted);
    assert_err(e.r().acquire_srs(SrsRequest { key: stale_key, points: &a }), ErrorKind::ContextLost, FailureEffect::NotStarted);
    // Same content in the new generation is a fresh miss: no cross-generation reuse.
    let lb = e.coeff(11, &a);
    assert_eq!(lb.generation(), new_gen);
    assert_ne!(lb.bases().allocation(), la.bases().allocation());
    let s = e.stats();
    assert_eq!((s.uploads, s.stale_purged, s.resident_entries), (2, 1, 1));
    e.assert_resident(&lb, &a);
    // Stale release reports ContextLost and drops the record.
    assert_err(e.r().release_srs(la), ErrorKind::ContextLost, FailureEffect::NotStarted);
    assert_eq!(e.stats().active_leases, 1);
    e.release(lb);
    assert_eq!(e.fake.calls(DriverCall::DeviceFree), 0, "nothing freed through the lost context");
    e.clean();
}

#[test]
fn failed_upload_publishes_nothing() {
    let e = env(1 << 30);
    let a = points(300_000, 16); // > 1 staging chunk
    e.fake.inject(DriverCall::CopyH2D, 2, Fault::Fail(DriverErrorClass::Transient));
    assert_err(e.acquire(SrsBasis::Coefficient, 19, &a), ErrorKind::Transfer, FailureEffect::InputsPreserved);
    let s = e.stats();
    assert_eq!((s.resident_entries, s.pending_bytes, s.uploads, s.active_leases), (0, 0, 0, 0));
    let snap = ResidentBackend::<Toy, ToyPoint>::runtime_snapshot(e.rt).unwrap();
    assert_eq!((snap.live_bytes, snap.reserved_bytes), (0, 0));
    // Retry succeeds and is exact.
    let l = e.coeff(19, &a);
    e.assert_resident(&l, &a);
    e.release(l);
    e.clean();
}

#[test]
fn failed_eviction_release_is_quarantined() {
    let n = 256;
    let bytes = (n * WIRE) as u64;
    let e = env(bytes);
    let l = e.coeff(8, &points(n, 17));
    e.release(l);
    e.fake.inject(DriverCall::DeviceFree, 1, Fault::Fail(DriverErrorClass::Poisoned));
    assert_err(e.acquire(SrsBasis::Coefficient, 8, &points(n, 18)), ErrorKind::Internal, FailureEffect::SessionPoisoned);
    let s = e.stats();
    assert_eq!((s.resident_entries, s.pending_bytes, s.retiring_bytes), (0, 0, bytes));
    // Quarantined bytes are dropped only with their generation.
    e.rt.reinitialize().unwrap();
    let l = e.coeff(8, &points(n, 18));
    assert_eq!(e.stats().retiring_bytes, 0);
    e.release(l);
    e.clean();
}

/// Codec that runs a one-shot hook during validation (outside every lock),
/// used to replace the device context between key check and upload.
struct HookCodec {
    inner: ToyCodec,
    hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl SrsPointCodec<ToyPoint> for HookCodec {
    fn curve_abi(&self) -> CurveAbi {
        self.inner.curve_abi()
    }
    fn wire_bytes(&self) -> usize {
        self.inner.wire_bytes()
    }
    fn validate_encode(&self, points: &[ToyPoint], wire: &mut [u8]) -> Result<(), SrsPointError> {
        if let Some(hook) = self.hook.lock().unwrap().take() {
            hook();
        }
        self.inner.validate_encode(points, wire)
    }
    fn same_representation(&self, a: &ToyPoint, b: &ToyPoint) -> bool {
        self.inner.same_representation(a, b)
    }
}

/// lean-w1 finding: an upload that lands in a newer generation is released,
/// and if that release fails its bytes are quarantined (charged) under the
/// stray buffer's generation, exactly like a failed victim release.
#[test]
fn stray_generation_upload_release_failure_is_quarantined() {
    let n = 256;
    let bytes = (n * WIRE) as u64;
    let e = env(1 << 30);
    let rt = e.rt;
    let fake = e.fake.clone();
    let old_gen = e.gen();
    let codec = Arc::new(HookCodec {
        inner: ToyCodec::default(),
        hook: std::sync::Mutex::new(Some(Box::new(move || {
            fake.inject(DriverCall::DeviceAlloc, 1, Fault::Fail(DriverErrorClass::ContextLost));
            assert!(ResidentBackend::<Toy, ToyPoint>::upload_scalars(rt, &[Toy::new(1)]).is_err());
            rt.reinitialize().unwrap();
            // The stray buffer's release will fail.
            fake.inject(DriverCall::DeviceFree, 1, Fault::Fail(DriverErrorClass::Transient));
        }))),
    });
    let reg = GpuSrsRegistry::with_runtime(rt, codec, 1 << 30);
    let r: &Reg = &reg;
    let pts = points(n, 30);
    let key = e.key(SrsBasis::Coefficient, 8, n);
    assert_eq!(key.generation, old_gen);
    assert_err(
        r.acquire_srs(SrsRequest { key, points: &pts }),
        ErrorKind::ContextLost,
        FailureEffect::InputsPreserved,
    );
    let new_gen = e.gen();
    assert_ne!(new_gen, old_gen);
    let s = reg.stats();
    assert_eq!((s.resident_entries, s.pending_bytes, s.uploads), (0, 0, 0));
    assert_eq!(s.retiring_bytes, bytes, "failed stray release stays charged");
    // The failed free poisoned the session: no acquire until a new
    // generation, and the quarantine survives the refused attempt.
    assert_err(
        r.acquire_srs(SrsRequest { key: e.key(SrsBasis::Coefficient, 8, n), points: &pts }),
        ErrorKind::Poisoned,
        FailureEffect::SessionPoisoned,
    );
    assert_eq!(reg.stats().retiring_bytes, bytes);
    rt.reinitialize().unwrap();
    let again = r.acquire_srs(SrsRequest { key: e.key(SrsBasis::Coefficient, 8, n), points: &pts }).unwrap();
    assert_eq!(reg.stats().retiring_bytes, 0);
    r.release_srs(again).unwrap();
}

#[test]
fn forged_lease_metadata_and_public_paths_are_rejected() {
    let e = env(1 << 30);
    let a = points(128, 19);
    let l = e.coeff(7, &a);
    let mut other_key = l.key();
    other_key.point_count = 127;
    for forged in [
        forge(&l, Some(l.lease_id().0 + 1000), None, None),
        forge(&l, None, Some(other_key), None),
        forge(&l, None, None, Some(l.bases().allocation().0 + 1)),
    ] {
        assert_err(e.reg.resident_wire(&forged), ErrorKind::InvalidInput, FailureEffect::NotStarted);
        assert_err(e.r().release_srs(forged), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    }
    // Public resident-buffer paths cannot read or free a registry allocation.
    let b = l.bases();
    let alias: ResidentBuffer<ToyPoint> =
        ResidentBuffer::from_untrusted_metadata(b.generation(), b.allocation(), b.len(), b.bytes());
    assert_err(
        ResidentBackend::<Toy, ToyPoint>::release_points(e.rt, alias),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    let as_scalars: ResidentBuffer<Toy> =
        ResidentBuffer::from_untrusted_metadata(b.generation(), b.allocation(), b.len(), b.bytes());
    assert_err(
        ResidentBackend::<Toy, ToyPoint>::download_scalars(e.rt, &as_scalars),
        ErrorKind::InvalidInput,
        FailureEffect::NotStarted,
    );
    e.assert_resident(&l, &a);
    e.release(l);
    e.clean();
}

#[test]
fn srs_msm_validates_then_reports_unsupported() {
    let e = env(1 << 30);
    let a = points(64, 20);
    let l = e.coeff(6, &a);
    let s = vec![Toy::new(3); 64];
    assert_err(e.r().srs_msm(&l, &s[..63], MsmLengthPolicy::RequireEqual), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    assert_err(e.r().srs_msm(&l, &s, MsmLengthPolicy::RequireEqual), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(e.r().srs_msm(&l, &s[..10], MsmLengthPolicy::ZipTruncate), ErrorKind::Unsupported, FailureEffect::NotStarted);
    assert_err(
        e.r().srs_msm_batch(&l, &[&s[..], &s[..1]], MsmLengthPolicy::ZipTruncate),
        ErrorKind::Unsupported,
        FailureEffect::NotStarted,
    );
    e.release(l);
    // The public backend shell advertises nothing and reports Unsupported.
    let caps = <loki_gpu::LokiGpu as loki_backend_api::Backend<Toy, ToyPoint>>::capabilities(&loki_gpu::LokiGpu);
    assert!(!caps.srs_registry);
}

#[test]
fn concurrent_acquires_share_and_account_exactly() {
    let e = Arc::new(env(1 << 30));
    let sets: Vec<Vec<ToyPoint>> = (0..4).map(|i| points(6000, 30 + i)).collect();
    let sets = Arc::new(sets);
    let threads: Vec<_> = (0..8)
        .map(|t| {
            let e = e.clone();
            let sets = sets.clone();
            std::thread::spawn(move || {
                for round in 0..6 {
                    let pts = &sets[(t + round) % 4];
                    let n = pts.len() - (round % 3);
                    let l = e.coeff(13, &pts[..n]);
                    e.assert_resident(&l, &pts[..n]);
                    e.release(l);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().expect("no panic, no lock-order violation");
    }
    let s = e.stats();
    assert_eq!(s.active_leases, 0);
    assert_eq!(s.pending_bytes, 0);
    assert_eq!(s.hits + s.misses, 48);
    assert!(s.uploads >= 4, "{s:?}");
    // Concurrent misses may each upload (sound, merely redundant); the byte
    // accounting must still be exact.
    assert_eq!(s.resident_bytes, s.uploaded_bytes - s.evicted_bytes);
    assert_eq!(s.evictions, 0);
    assert!(s.resident_entries >= 4);
    e.clean();
}

/// Cold/warm figures on synthetic inputs (k19/k20 shapes). Printed for the
/// P3 record; asserts the expected miss/byte pattern.
#[test]
fn synthetic_cold_warm_figures() {
    let e = env(1 << 30);
    let coeff = points(1 << 20, 40);
    let lag = points(1 << 19, 41);
    let t0 = std::time::Instant::now();
    let l = e.acquire(SrsBasis::Coefficient, 20, &coeff).unwrap();
    let cold = t0.elapsed();
    e.release(l);
    let t1 = std::time::Instant::now();
    let l = e.acquire(SrsBasis::Coefficient, 20, &coeff[..(1 << 20) - 1]).unwrap();
    let warm = t1.elapsed();
    e.release(l);
    for _ in 0..3 {
        let l = e.acquire(SrsBasis::Lagrange, 19, &lag).unwrap();
        e.release(l);
        let l = e.acquire(SrsBasis::Coefficient, 19, &coeff[..1 << 19]).unwrap();
        e.release(l);
    }
    let s = e.stats();
    eprintln!(
        "P3 synthetic (fake driver, debug): cold k20 {:?}, warm k20-1 {:?}; {s:?}",
        cold, warm
    );
    assert_eq!((s.misses, s.uploads, s.hits), (2, 2, 6));
    assert_eq!(s.uploaded_bytes, ((1u64 << 20) + (1 << 19)) * WIRE as u64);
    assert_eq!(s.snapshot_host_bytes, ((1u64 << 20) + (1 << 19)) * std::mem::size_of::<ToyPoint>() as u64);
    e.clean();
}

// --------------------------------------------------------------------------
// Part 2: native device
// --------------------------------------------------------------------------

#[cfg(feature = "cuda")]
#[test]
fn native_registry_on_device() {
    // A dedicated native runtime (not the process static) so teardown is
    // observable: compute-sanitizer leak-check covers registry release and
    // runtime destruction.
    let rt_ptr = Box::into_raw(Box::new(
        GpuRuntime::native(RuntimeConfig::default()).expect("process-unique native claim"),
    ));
    // SAFETY: reclaimed only at the end of this test, after every borrower
    // (the registry and all leases) has been dropped.
    let rt: &'static GpuRuntime = unsafe { &*rt_ptr };
    match rt.initialize() {
        Ok(_) => {}
        Err(e) if std::env::var_os("LOKI_GPU_ALLOW_NO_DEVICE").is_some() => {
            eprintln!("skipping native SRS tests: {e:?}");
            return;
        }
        Err(e) => panic!("native SRS gate needs a device ({e:?}); set LOKI_GPU_ALLOW_NO_DEVICE=1 to skip"),
    }
    let n = 1 << 16;
    let bytes = (n * WIRE) as u64;
    let codec = Arc::new(ToyCodec::default());
    let reg = GpuSrsRegistry::with_runtime(rt, codec, 2 * bytes);
    let r: &Reg = &reg;
    let gen = ResidentBackend::<Toy, ToyPoint>::generation(rt).unwrap();
    let key = |basis, k, n| SrsCacheKey {
        curve_abi: TOY_ABI,
        basis,
        domain_log: k,
        point_count: n,
        generation: gen,
        content_identity: [0; 32],
    };
    let acquire = |basis, k: u32, pts: &[ToyPoint]| r.acquire_srs(SrsRequest { key: key(basis, k, pts.len()), points: pts });
    let check = |l: &SrsLease<ToyPoint>, pts: &[ToyPoint]| assert_eq!(reg.resident_wire(l).unwrap(), encode(pts));
    let live0 = ResidentBackend::<Toy, ToyPoint>::runtime_snapshot(rt).unwrap().live_bytes;

    let mut a = points(n, 50);
    let l = acquire(SrsBasis::Coefficient, 16, &a).unwrap();
    check(&l, &a);
    r.release_srs(l).unwrap();
    let l = acquire(SrsBasis::Coefficient, 16, &a[..n - 1]).unwrap(); // rewarm + prefix
    check(&l, &a[..n - 1]);
    r.release_srs(l).unwrap();
    a[n / 2] = ToyPoint(Toy::new(5)); // same-address mutation
    let l = acquire(SrsBasis::Coefficient, 16, &a).unwrap();
    check(&l, &a);
    let held = acquire(SrsBasis::Lagrange, 16, &points(n, 51)).unwrap();
    // Budget is two entries, both now leased or resident: a third must evict
    // the unleased one only.
    r.release_srs(l).unwrap();
    let third = points(n, 52);
    let l3 = acquire(SrsBasis::Coefficient, 16, &third).unwrap();
    check(&l3, &third);
    assert_err(acquire(SrsBasis::Coefficient, 16, &points(n, 53)), ErrorKind::Capacity, FailureEffect::NotStarted);
    let mut bad = points(n, 54);
    bad[n - 1] = ToyPoint(Toy::new(OFF_CURVE_TAG));
    assert_err(acquire(SrsBasis::Coefficient, 16, &bad), ErrorKind::InvalidInput, FailureEffect::NotStarted);
    r.release_srs(l3).unwrap();
    r.release_srs(held).unwrap();
    let s = reg.stats();
    eprintln!("P3 native: {s:?}");
    assert_eq!((s.hits, s.misses, s.uploads), (1, 6, 4));
    assert!(s.high_water_bytes <= 2 * bytes);
    drop(reg);
    let live1 = ResidentBackend::<Toy, ToyPoint>::runtime_snapshot(rt).unwrap().live_bytes;
    assert_eq!(live1, live0, "dropping the registry released its device memory");
    // SAFETY: `reg` and every lease are gone; nothing else borrows `rt`.
    drop(unsafe { Box::from_raw(rt_ptr) });
}
