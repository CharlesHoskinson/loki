//! P3: validated, lease-counted SRS residency registry.
//!
//! The registry keeps immutable, validated host snapshots of SRS point arrays
//! and their device-resident wire encoding on the shared process runtime
//! ([`GpuRuntime`]). A cache hit is established only by comparing every point
//! of the requested prefix with the snapshot taken at upload; the host address
//! is a search filter and the caller's `content_identity` digest is never
//! trusted. Generic `group::Curve` types carry no wire encoding, so points
//! enter through a concrete [`SrsPointCodec`] supplied by the integrating
//! crate (Loki provides one for BLS12-381 G1).
//!
//! Policy:
//!
//! - Key compatibility binds curve wire ABI, basis and device generation. A
//!   coefficient request may use a prefix of a larger coefficient entry
//!   (`domain_log ≤ entry`); a Lagrange request requires the exact domain,
//!   because a prefix of a larger Lagrange basis is not the smaller domain's
//!   basis.
//! - The persistent budget (`LOKI_GPU_SRS_BUDGET_BYTES`, default 4 GiB) bounds
//!   the sum of resident, in-flight and retiring wire bytes. Per-call MSM
//!   scratch is charged to the shared runtime budget, not here. The runtime
//!   budget independently charges every upload and may refuse it.
//! - Eviction is LRU among entries with zero leases and zero in-progress
//!   compare pins. An entry larger than the budget, or a miss that cannot fit
//!   without evicting a leased entry, is refused before any upload.
//! - Upload candidates are validated by the codec (curve and subgroup
//!   membership) before any device allocation; a rejected or failed upload
//!   publishes no entry.
//! - A generation change (context loss / reinitialization) invalidates every
//!   entry and lease from the prior generation; their device memory is owned by
//!   the runtime's quarantine/reset path and is never freed from here.
//! - Lock order: the registry state lock sits at runtime level 2 and is never
//!   held across a runtime call (the ordered mutex panics on such nesting).
//!
//! `srs_msm` validates the lease and length policy but returns `Unsupported`:
//! the native resident MSM kernel is package P6. `LokiGpu` therefore keeps
//! `srs_registry` unadvertised.
//!
//! Provenance: the exact projective-snapshot prefix comparison, the
//! whole-capacity validation before upload, the four-worker scoped-thread
//! partitioning that never re-enters Rayon, and the "join every worker" rule
//! are ported from Loki W13 `src/poly/kzg/msm/cuda/backend.rs` (Loki
//! `b6af6f9`, SHA-256 `d84fbb23…a6d8a`, Apache-2.0 OR MIT), as recorded in
//! `design/PROVENANCE.md`. The lease/generation/byte-budget machinery is new.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use loki_backend_api::{
    BackendError, Curve, CurveAbi, DeviceGeneration, ErrorKind, FailureEffect, MsmLengthPolicy,
    Operation, PrimeField, ResidentBuffer, SrsBasis, SrsCacheKey, SrsLease, SrsLeaseId,
    SrsRegistry, SrsRequest,
};

use crate::runtime::{
    GpuRuntime, OrderedGuard, OrderedMutex, RuntimeSnapshot, DEFAULT_SRS_BUDGET_BYTES,
    LEVEL_REGISTRY,
};
use crate::LokiGpu;

/// Environment key for the persistent SRS allocation budget.
pub const SRS_BUDGET_ENV: &str = "LOKI_GPU_SRS_BUDGET_BYTES";

/// Below this many points exact comparison runs on the calling thread.
const PARALLEL_COMPARE_MIN: usize = 4096;
/// Native scoped comparison workers (W13 bound; never Rayon).
const COMPARE_WORKERS: usize = 4;

/// Why a codec rejected an upload candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SrsPointError {
    /// The point does not satisfy the curve equation (or is not a valid
    /// encoding of a curve point).
    OffCurve { index: usize },
    /// The point is on the curve but outside the prime-order subgroup.
    NotInSubgroup { index: usize },
    /// The codec could not encode the point (for example a non-canonical
    /// coordinate).
    Malformed { index: usize },
}

/// Concrete point adapter supplied by the integrating crate.
///
/// Implementations must be deterministic and must not panic on malformed
/// points. `validate_encode` is the only path by which points reach device
/// memory, so it must reject every off-curve or non-subgroup point.
pub trait SrsPointCodec<G>: Send + Sync {
    /// Curve / wire ABI of the encoding this codec produces.
    fn curve_abi(&self) -> CurveAbi;

    /// Device wire bytes per point (96 for BLS12-381 G1 affine `x || y`).
    fn wire_bytes(&self) -> usize;

    /// Validate every point (curve equation and prime-order subgroup) and
    /// write its wire encoding into `wire[i * wire_bytes ..]`. `wire.len()`
    /// is exactly `points.len() * wire_bytes()`. On error the contents of
    /// `wire` are unspecified and are discarded.
    fn validate_encode(&self, points: &[G], wire: &mut [u8]) -> Result<(), SrsPointError>;

    /// Exact representation equality (every stored coordinate limb). This is
    /// the cache-hit predicate; it must imply group-element equality.
    fn same_representation(&self, a: &G, b: &G) -> bool;
}

/// Observational registry counters. Byte figures are device wire bytes
/// unless named `host`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SrsStats {
    pub acquires: u64,
    pub hits: u64,
    pub misses: u64,
    pub uploads: u64,
    pub uploaded_bytes: u64,
    pub evictions: u64,
    pub evicted_bytes: u64,
    pub rejected_invalid: u64,
    pub rejected_capacity: u64,
    /// Entries dropped because their device generation ended.
    pub stale_purged: u64,
    /// Points compared exactly while deciding hits.
    pub compared_points: u64,
    pub resident_entries: u64,
    /// Persistent resident wire bytes now.
    pub resident_bytes: u64,
    /// Bytes reserved for in-flight uploads.
    pub pending_bytes: u64,
    /// Bytes of evicted entries whose release has not completed, plus
    /// quarantined bytes after a failed release.
    pub retiring_bytes: u64,
    /// High-water of `resident + pending + retiring`.
    pub high_water_bytes: u64,
    /// Host bytes held by validated snapshots (`size_of::<G>()` per point).
    pub snapshot_host_bytes: u64,
    pub active_leases: u64,
}

struct Entry<G> {
    id: u64,
    curve_abi: CurveAbi,
    basis: SrsBasis,
    domain_log: u32,
    generation: DeviceGeneration,
    address: usize,
    snapshot: Arc<[G]>,
    buffer: ResidentBuffer<G>,
    bytes: u64,
    last_use: u64,
    leases: u64,
    /// In-progress exact comparisons that rely on this entry staying alive.
    pins: u64,
}

impl<G> Entry<G> {
    fn compatible(&self, key: &SrsCacheKey, generation: DeviceGeneration) -> bool {
        self.generation == generation
            && self.curve_abi == key.curve_abi
            && self.basis == key.basis
            && key.point_count <= self.snapshot.len()
            && match key.basis {
                SrsBasis::Coefficient => key.domain_log <= self.domain_log,
                SrsBasis::Lagrange => key.domain_log == self.domain_log,
            }
    }

    fn evictable(&self) -> bool {
        self.leases == 0 && self.pins == 0
    }

    fn handle(&self) -> ResidentBuffer<G> {
        copy_handle(&self.buffer)
    }
}

fn copy_handle<G>(b: &ResidentBuffer<G>) -> ResidentBuffer<G> {
    ResidentBuffer::from_untrusted_metadata(b.generation(), b.allocation(), b.len(), b.bytes())
}

#[derive(Clone, Copy, Debug)]
struct RuntimeState {
    generation: Option<DeviceGeneration>,
    healthy: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeaseRecord {
    entry: u64,
    key: SrsCacheKey,
    generation: DeviceGeneration,
    allocation: loki_backend_api::AllocationId,
    capacity: usize,
    bytes: u64,
}

struct State<G> {
    entries: Vec<Entry<G>>,
    leases: HashMap<SrsLeaseId, LeaseRecord>,
    pending_bytes: u64,
    retiring_bytes: u64,
    /// Bytes of entries whose release failed, by generation. They are never
    /// retried and are dropped from accounting when the generation ends.
    quarantined: Vec<(DeviceGeneration, u64)>,
    stats: SrsStats,
}

impl<G> State<G> {
    fn resident_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.bytes).sum()
    }

    fn quarantined_bytes(&self) -> u64 {
        self.quarantined.iter().map(|(_, b)| *b).sum()
    }

    fn committed(&self) -> u64 {
        self.resident_bytes()
            .saturating_add(self.pending_bytes)
            .saturating_add(self.retiring_bytes)
            .saturating_add(self.quarantined_bytes())
    }

    fn note_high_water(&mut self) {
        let committed = self.committed();
        self.stats.high_water_bytes = self.stats.high_water_bytes.max(committed);
    }

    /// Drop entries and quarantine records of ended generations. Their device
    /// memory belongs to the runtime's reset/quarantine path.
    fn purge(&mut self, current: DeviceGeneration) {
        let before = self.entries.len();
        self.entries.retain(|e| e.generation == current);
        self.stats.stale_purged += (before - self.entries.len()) as u64;
        self.quarantined.retain(|(g, _)| *g == current);
    }
}

/// Validated SRS residency registry for one point type.
pub struct GpuSrsRegistry<G: 'static> {
    runtime: &'static GpuRuntime,
    codec: Arc<dyn SrsPointCodec<G>>,
    budget_bytes: u64,
    state: OrderedMutex<State<G>>,
    next_id: AtomicU64,
}

impl<G: 'static> std::fmt::Debug for GpuSrsRegistry<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuSrsRegistry")
            .field("budget_bytes", &self.budget_bytes)
            .finish_non_exhaustive()
    }
}

const fn err(operation: Operation, kind: ErrorKind, effect: FailureEffect) -> BackendError {
    BackendError {
        operation,
        kind,
        effect,
    }
}

const fn not_started(operation: Operation, kind: ErrorKind) -> BackendError {
    err(operation, kind, FailureEffect::NotStarted)
}

/// Re-label a runtime error as an SRS operation, keeping cause and effect.
const fn relabel(operation: Operation, e: BackendError) -> BackendError {
    err(operation, e.kind, e.effect)
}

/// Parse a budget override. Invalid values are rejected rather than silently
/// disabling or enlarging the resident cache.
pub fn parse_budget(value: Option<&str>) -> Result<u64, BackendError> {
    match value {
        None => Ok(DEFAULT_SRS_BUDGET_BYTES),
        Some(raw) => raw
            .parse::<u64>()
            .map_err(|_| not_started(Operation::SrsAcquire, ErrorKind::InvalidInput)),
    }
}

impl<G> GpuSrsRegistry<G>
where
    G: Curve,
{
    /// Registry on the process runtime with the default 4 GiB budget.
    pub fn new(codec: Arc<dyn SrsPointCodec<G>>) -> Self {
        Self::with_runtime(GpuRuntime::process(), codec, DEFAULT_SRS_BUDGET_BYTES)
    }

    /// Registry on the process runtime with `LOKI_GPU_SRS_BUDGET_BYTES`.
    pub fn from_environment(codec: Arc<dyn SrsPointCodec<G>>) -> Result<Self, BackendError> {
        let budget = match std::env::var(SRS_BUDGET_ENV) {
            Ok(raw) => parse_budget(Some(&raw))?,
            Err(std::env::VarError::NotPresent) => DEFAULT_SRS_BUDGET_BYTES,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(not_started(Operation::SrsAcquire, ErrorKind::InvalidInput))
            }
        };
        Ok(Self::with_runtime(GpuRuntime::process(), codec, budget))
    }

    /// Registry on the process runtime with an explicit budget.
    pub fn with_budget(codec: Arc<dyn SrsPointCodec<G>>, budget_bytes: u64) -> Self {
        Self::with_runtime(GpuRuntime::process(), codec, budget_bytes)
    }

    /// Registry over an explicit runtime (tests use a fake-driver runtime).
    pub fn with_runtime(
        runtime: &'static GpuRuntime,
        codec: Arc<dyn SrsPointCodec<G>>,
        budget_bytes: u64,
    ) -> Self {
        Self {
            runtime,
            codec,
            budget_bytes,
            state: OrderedMutex::new(
                LEVEL_REGISTRY,
                State {
                    entries: Vec::new(),
                    leases: HashMap::new(),
                    pending_bytes: 0,
                    retiring_bytes: 0,
                    quarantined: Vec::new(),
                    stats: SrsStats::default(),
                },
            ),
            next_id: AtomicU64::new(1),
        }
    }

    /// Configured persistent SRS budget (wire bytes).
    pub const fn budget_bytes(&self) -> u64 {
        self.budget_bytes
    }

    /// Persistent wire bytes an entry of `points` points would occupy.
    pub fn persistent_bytes_for(&self, points: usize) -> Option<u64> {
        (points as u64).checked_mul(self.codec.wire_bytes() as u64)
    }

    /// Snapshot of the shared process runtime.
    pub fn runtime_snapshot(&self) -> RuntimeSnapshot {
        self.runtime.snapshot()
    }

    /// Registry counters. Accounting reflects the last operation; entries of
    /// an ended generation are purged at the next acquire.
    pub fn stats(&self) -> SrsStats {
        match self.state.lock() {
            Ok(st) => {
                let mut s = st.stats;
                s.resident_entries = st.entries.len() as u64;
                s.resident_bytes = st.resident_bytes();
                s.pending_bytes = st.pending_bytes;
                s.retiring_bytes = st.retiring_bytes + st.quarantined_bytes();
                s.snapshot_host_bytes = st
                    .entries
                    .iter()
                    .map(|e| (e.snapshot.len() * std::mem::size_of::<G>()) as u64)
                    .sum();
                s.active_leases = st.leases.len() as u64;
                s
            }
            Err(()) => SrsStats::default(),
        }
    }

    fn lock(&self, op: Operation) -> Result<OrderedGuard<'_, State<G>>, BackendError> {
        self.state
            .lock()
            .map_err(|()| err(op, ErrorKind::Internal, FailureEffect::SessionPoisoned))
    }

    fn fresh_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Generation and health of the shared runtime, without failing.
    fn runtime_state(&self) -> RuntimeState {
        let snap = self.runtime.snapshot();
        RuntimeState {
            generation: snap.generation,
            healthy: snap.healthy,
        }
    }

    /// Current healthy generation of the shared runtime. Must be called
    /// without the state lock held.
    fn current_generation(&self, op: Operation) -> Result<DeviceGeneration, BackendError> {
        let snap = self.runtime.snapshot();
        match snap.generation {
            None => Err(BackendError::unsupported(op)),
            Some(_) if !snap.healthy => {
                Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned))
            }
            Some(g) => Ok(g),
        }
    }

    fn validate_request(&self, request: &SrsRequest<'_, G>) -> Result<(), BackendError> {
        let op = Operation::SrsAcquire;
        let key = &request.key;
        let invalid = not_started(op, ErrorKind::InvalidInput);
        if key.point_count != request.points.len() || key.point_count == 0 {
            return Err(invalid);
        }
        if key.curve_abi != self.codec.curve_abi() {
            return Err(BackendError::unsupported(op));
        }
        // A basis for domain 2^k has at most 2^k points.
        let domain = 1usize
            .checked_shl(key.domain_log)
            .filter(|_| key.domain_log < usize::BITS)
            .ok_or(invalid)?;
        if key.point_count > domain {
            return Err(invalid);
        }
        if self.budget_bytes == 0 {
            return Err(not_started(op, ErrorKind::Capacity));
        }
        Ok(())
    }

    /// Validate untrusted lease metadata against the live lease table.
    fn check_lease(
        &self,
        st: &State<G>,
        op: Operation,
        lease: &SrsLease<G>,
        current: RuntimeState,
    ) -> Result<LeaseRecord, BackendError> {
        let invalid = not_started(op, ErrorKind::InvalidInput);
        let record = *st.leases.get(&lease.lease_id()).ok_or(invalid)?;
        let b = lease.bases();
        if record.key != lease.key()
            || record.generation != lease.generation()
            || record.generation != b.generation()
            || record.allocation != b.allocation()
            || record.capacity != b.len()
            || record.bytes != b.bytes()
        {
            return Err(invalid);
        }
        if Some(record.generation) != current.generation {
            return Err(not_started(op, ErrorKind::ContextLost));
        }
        if !current.healthy {
            return Err(err(op, ErrorKind::Poisoned, FailureEffect::SessionPoisoned));
        }
        Ok(record)
    }

    /// Download the device bytes of the leased prefix (`point_count` points).
    /// Diagnostic: proves that the resident data equals the codec encoding of
    /// the exact requested bases.
    pub fn resident_wire(&self, lease: &SrsLease<G>) -> Result<Vec<u8>, BackendError> {
        let op = Operation::SrsMsm;
        let current = self.runtime_state();
        let handle = {
            let st = self.lock(op)?;
            let record = self.check_lease(&st, op, lease, current)?;
            let entry = st
                .entries
                .iter()
                .find(|e| e.id == record.entry)
                .ok_or(not_started(op, ErrorKind::ContextLost))?;
            entry.handle()
        };
        // The lease pins the entry, so it cannot be released meanwhile.
        let mut wire = self
            .runtime
            .download_wire(&handle)
            .map_err(|e| relabel(op, e))?;
        wire.truncate(lease.key().point_count * self.codec.wire_bytes());
        Ok(wire)
    }

    fn make_lease(&self, st: &mut State<G>, index: usize, key: SrsCacheKey) -> SrsLease<G> {
        let lease_id = SrsLeaseId(self.fresh_id());
        let entry = &mut st.entries[index];
        entry.leases += 1;
        let seq = self.fresh_id();
        entry.last_use = seq;
        let handle = entry.handle();
        let record = LeaseRecord {
            entry: entry.id,
            key,
            generation: entry.generation,
            allocation: handle.allocation(),
            capacity: handle.len(),
            bytes: handle.bytes(),
        };
        st.leases.insert(lease_id, record);
        SrsLease::from_untrusted_metadata(lease_id, key, entry.generation, handle)
    }

    fn acquire(&self, request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        let op = Operation::SrsAcquire;
        self.validate_request(&request)?;
        let generation = self.current_generation(op)?;
        let key = request.key;
        if key.generation != generation {
            return Err(not_started(op, ErrorKind::ContextLost));
        }
        let points = request.points;
        let address = points.as_ptr() as usize;

        // 1. Pin compatible candidates (same address first) under the lock.
        let candidates: Vec<(u64, Arc<[G]>)> = {
            let mut st = self.lock(op)?;
            st.stats.acquires += 1;
            st.purge(generation);
            let mut found: Vec<(bool, u64, u64, Arc<[G]>)> = Vec::new();
            for e in st.entries.iter_mut() {
                if e.compatible(&key, generation) {
                    e.pins += 1;
                    found.push((e.address != address, u64::MAX - e.last_use, e.id, e.snapshot.clone()));
                }
            }
            found.sort_by_key(|(far, recency, _, _)| (*far, *recency));
            found.into_iter().map(|(_, _, id, s)| (id, s)).collect()
        };

        // 2. Exact comparison outside the lock against immutable snapshots.
        let mut hit = None;
        let mut compared = 0u64;
        for (id, snapshot) in &candidates {
            compared += points.len() as u64;
            if exact_prefix_eq(self.codec.as_ref(), snapshot, points) {
                hit = Some(*id);
                break;
            }
        }

        // 3. Unpin; convert a hit into a lease.
        {
            let mut st = self.lock(op)?;
            st.stats.compared_points += compared;
            for (id, _) in &candidates {
                if let Some(e) = st.entries.iter_mut().find(|e| e.id == *id) {
                    e.pins -= 1;
                }
            }
            if let Some(id) = hit {
                if let Some(index) = st.entries.iter().position(|e| e.id == id) {
                    st.stats.hits += 1;
                    return Ok(self.make_lease(&mut st, index, key));
                }
            }
            st.stats.misses += 1;
        }

        // 4. Miss: validate and encode the whole request before any upload.
        let wire_bytes = self.codec.wire_bytes();
        let capacity = not_started(op, ErrorKind::Capacity);
        let bytes = self.persistent_bytes_for(points.len()).ok_or(capacity)?;
        if wire_bytes == 0 {
            return Err(BackendError::unsupported(op));
        }
        if bytes > self.budget_bytes {
            self.lock(op)?.stats.rejected_capacity += 1;
            return Err(capacity);
        }
        let host_len = usize::try_from(bytes).map_err(|_| capacity)?;
        let mut wire = vec![0u8; host_len];
        if self.codec.validate_encode(points, &mut wire).is_err() {
            self.lock(op)?.stats.rejected_invalid += 1;
            return Err(not_started(op, ErrorKind::InvalidInput));
        }
        let snapshot: Arc<[G]> = Arc::from(points);

        // 5. Reserve budget, choosing LRU victims only among unleased,
        //    unpinned entries; refuse before evicting if they cannot cover it.
        let victims: Vec<Entry<G>> = {
            let mut st = self.lock(op)?;
            st.purge(generation);
            let deficit = st
                .committed()
                .saturating_add(bytes)
                .saturating_sub(self.budget_bytes);
            let mut chosen: Vec<u64> = Vec::new();
            if deficit > 0 {
                let mut order: Vec<(u64, u64, u64)> = st
                    .entries
                    .iter()
                    .filter(|e| e.evictable())
                    .map(|e| (e.last_use, e.id, e.bytes))
                    .collect();
                order.sort_unstable();
                let mut freed = 0u64;
                for (_, id, b) in order {
                    if freed >= deficit {
                        break;
                    }
                    chosen.push(id);
                    freed += b;
                }
                if freed < deficit {
                    st.stats.rejected_capacity += 1;
                    return Err(capacity);
                }
            }
            let mut victims = Vec::with_capacity(chosen.len());
            for id in chosen {
                let index = st.entries.iter().position(|e| e.id == id).expect("chosen");
                victims.push(st.entries.swap_remove(index));
            }
            let retiring: u64 = victims.iter().map(|v| v.bytes).sum();
            st.retiring_bytes += retiring;
            st.pending_bytes += bytes;
            if retiring == 0 {
                st.note_high_water();
            }
            victims
        };

        // 6. Release victims outside the lock. No lease or pin references
        //    them, and every v1 operation retires before returning.
        let mut victims = victims.into_iter();
        while let Some(victim) = victims.next() {
            let released = self.runtime.release_wire(victim.handle());
            let mut st = self.lock(op)?;
            st.retiring_bytes -= victim.bytes;
            match released {
                Ok(()) => {
                    st.stats.evictions += 1;
                    st.stats.evicted_bytes += victim.bytes;
                }
                Err(e) => {
                    // Never retry a possibly freed handle; keep its bytes
                    // charged until the generation ends.
                    st.quarantined.push((victim.generation, victim.bytes));
                    for rest in victims.by_ref() {
                        st.retiring_bytes -= rest.bytes;
                        st.quarantined.push((rest.generation, rest.bytes));
                    }
                    st.pending_bytes -= bytes;
                    return Err(relabel(op, e));
                }
            }
            // Victims are freed before the upload allocates, so device
            // co-residency is resident + pending once they have retired.
            if st.retiring_bytes == 0 {
                st.note_high_water();
            }
        }

        // 7. Upload through the shared runtime (charges its device budget).
        let uploaded = self.runtime.upload_wire::<G>(&wire, wire_bytes);
        drop(wire);
        let buffer = match uploaded {
            Ok(buffer) => buffer,
            Err(e) => {
                self.lock(op)?.pending_bytes -= bytes;
                return Err(relabel(op, e));
            }
        };
        if buffer.generation() != generation {
            // The context was replaced during the upload; the new allocation
            // belongs to a generation this request did not name. A failed
            // release is never retried; like a failed victim release its
            // bytes stay charged (quarantined under the buffer's generation)
            // until that generation ends.
            let stray = buffer.generation();
            let released = self.runtime.release_wire(buffer);
            let mut st = self.lock(op)?;
            st.pending_bytes -= bytes;
            if released.is_err() {
                st.quarantined.push((stray, bytes));
            }
            return Err(err(op, ErrorKind::ContextLost, FailureEffect::InputsPreserved));
        }

        // 8. Publish the validated entry with its first lease.
        let mut st = self.lock(op)?;
        st.pending_bytes -= bytes;
        st.stats.uploads += 1;
        st.stats.uploaded_bytes += bytes;
        st.entries.push(Entry {
            id: self.fresh_id(),
            curve_abi: key.curve_abi,
            basis: key.basis,
            domain_log: key.domain_log,
            generation,
            address,
            snapshot,
            buffer,
            bytes,
            last_use: 0,
            leases: 0,
            pins: 0,
        });
        st.note_high_water();
        let index = st.entries.len() - 1;
        Ok(self.make_lease(&mut st, index, key))
    }

    fn msm_checked(
        &self,
        lease: &SrsLease<G>,
        scalars_len: usize,
        length_policy: MsmLengthPolicy,
    ) -> Result<usize, BackendError> {
        let op = Operation::SrsMsm;
        let current = self.runtime_state();
        let st = self.lock(op)?;
        let record = self.check_lease(&st, op, lease, current)?;
        let prefix = record.key.point_count;
        match length_policy {
            MsmLengthPolicy::ZipTruncate => Ok(scalars_len.min(prefix)),
            MsmLengthPolicy::RequireEqual if scalars_len == prefix => Ok(prefix),
            MsmLengthPolicy::RequireEqual => Err(not_started(op, ErrorKind::InvalidInput)),
        }
    }

    fn release(&self, lease: SrsLease<G>) -> Result<(), BackendError> {
        let op = Operation::SrsRelease;
        // Release is bookkeeping only (no device call), so it is accepted in
        // a poisoned-but-current generation.
        let current = RuntimeState {
            healthy: true,
            ..self.runtime_state()
        };
        let mut st = self.lock(op)?;
        match self.check_lease(&st, op, &lease, current) {
            Ok(record) => {
                st.leases.remove(&lease.lease_id());
                if let Some(e) = st.entries.iter_mut().find(|e| e.id == record.entry) {
                    e.leases -= 1;
                }
                Ok(())
            }
            Err(e) if e.kind == ErrorKind::ContextLost => {
                // Stale generation: the entry is gone with its context. Drop
                // the record so the lease table does not grow.
                st.leases.remove(&lease.lease_id());
                Err(e)
            }
            Err(e) => Err(e),
        }
    }
}

/// Exact representation comparison of `points` against the snapshot prefix.
/// Large inputs use at most four native scoped workers; every worker is
/// joined, and a panicking or unspawnable worker counts as a mismatch (a
/// miss is always safe).
fn exact_prefix_eq<G: Sync>(
    codec: &dyn SrsPointCodec<G>,
    snapshot: &[G],
    points: &[G],
) -> bool {
    if points.len() > snapshot.len() {
        return false;
    }
    let snapshot = &snapshot[..points.len()];
    // Cheap rejection before the full pass.
    let ends_match = |i: usize| codec.same_representation(&snapshot[i], &points[i]);
    if points.is_empty() || !ends_match(0) || !ends_match(points.len() - 1) {
        return points.is_empty();
    }
    if points.len() < PARALLEL_COMPARE_MIN {
        return snapshot
            .iter()
            .zip(points)
            .all(|(a, b)| codec.same_representation(a, b));
    }
    let chunk = points.len().div_ceil(COMPARE_WORKERS);
    std::thread::scope(|scope| {
        let mut jobs = Vec::with_capacity(COMPARE_WORKERS);
        let mut ok = true;
        for (a, b) in snapshot.chunks(chunk).zip(points.chunks(chunk)) {
            let spawned = std::thread::Builder::new()
                .name("loki-gpu-srs".into())
                .spawn_scoped(scope, move || {
                    a.iter().zip(b).all(|(x, y)| codec.same_representation(x, y))
                });
            match spawned {
                Ok(job) => jobs.push(job),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        // Join every worker, even after a mismatch or panic.
        for job in jobs {
            ok &= job.join().unwrap_or(false);
        }
        ok
    })
}

impl<G: 'static> Drop for GpuSrsRegistry<G> {
    fn drop(&mut self) {
        // Best-effort release of current entries. Stale-generation entries
        // belong to the runtime's reset path and are never freed here.
        let current = self.runtime.snapshot();
        let entries = match self.state.lock() {
            Ok(mut st) => std::mem::take(&mut st.entries),
            Err(()) => return,
        };
        if !current.healthy {
            return;
        }
        for e in entries {
            if Some(e.generation) == current.generation {
                let _ = self.runtime.release_wire(e.buffer);
            }
        }
    }
}

impl<F, G> SrsRegistry<F, G> for GpuSrsRegistry<G>
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        self.acquire(request)
    }

    /// Validates the lease and length policy, then reports `Unsupported`:
    /// the resident MSM kernel is package P6. Scalars are never read.
    fn srs_msm(
        &self,
        lease: &SrsLease<G>,
        scalars: &[F],
        length_policy: MsmLengthPolicy,
    ) -> Result<G, BackendError> {
        let _pairs = self.msm_checked(lease, scalars.len(), length_policy)?;
        Err(GpuRuntime::unsupported(Operation::SrsMsm))
    }

    fn release_srs(&self, lease: SrsLease<G>) -> Result<(), BackendError> {
        self.release(lease)
    }
}

/// The public backend shell has no point codec, so it reports the SRS
/// operations as unsupported; integrators construct a [`GpuSrsRegistry`] with
/// their concrete codec on the same process runtime.
impl<F, G> SrsRegistry<F, G> for LokiGpu
where
    F: PrimeField,
    G: Curve<Scalar = F>,
{
    fn acquire_srs(&self, _request: SrsRequest<'_, G>) -> Result<SrsLease<G>, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsAcquire))
    }

    fn srs_msm(
        &self,
        _lease: &SrsLease<G>,
        _scalars: &[F],
        _length_policy: MsmLengthPolicy,
    ) -> Result<G, BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsMsm))
    }

    fn release_srs(&self, _lease: SrsLease<G>) -> Result<(), BackendError> {
        Err(GpuRuntime::unsupported(Operation::SrsRelease))
    }
}
