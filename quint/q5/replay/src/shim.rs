//! Minimal stand-ins for the crate-private types that the snapshotted
//! production code references. Each mirrors only the members that code uses.
pub use ff::{BatchInvert, Field, FromUniformBytes, PrimeField, WithSmallOrderMulGroup};
pub use rand_core::{CryptoRng, RngCore};
pub use rayon::slice::ParallelSliceMut;
pub use std::{collections::HashMap, hash::Hash, iter};
pub use stock::utils::rational::Rational;
use std::{
    cell::RefCell,
    marker::PhantomData,
    ops::{Deref, DerefMut, Mul},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LagrangeCoeff;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Polynomial<F, B> {
    pub values: Vec<F>,
    pub _marker: PhantomData<B>,
}
impl<F, B> Deref for Polynomial<F, B> {
    type Target = [F];
    fn deref(&self) -> &[F] {
        &self.values
    }
}
impl<F, B> DerefMut for Polynomial<F, B> {
    fn deref_mut(&mut self) -> &mut [F] {
        &mut self.values
    }
}
impl<F: Field, B> Mul<F> for Polynomial<F, B> {
    type Output = Self;
    fn mul(mut self, rhs: F) -> Self {
        self.values.iter_mut().for_each(|v| *v *= rhs);
        self
    }
}
pub fn poly<F>(values: Vec<F>) -> Polynomial<F, LagrangeCoeff> {
    Polynomial { values, _marker: PhantomData }
}

#[derive(Clone, Debug)]
pub struct EvaluationDomain<F> {
    pub n: u64,
    pub omega: F,
}
impl<F: Field> EvaluationDomain<F> {
    pub fn lagrange_from_vec(&self, values: Vec<F>) -> Polynomial<F, LagrangeCoeff> {
        assert_eq!(values.len() as u64, self.n);
        poly(values)
    }
    pub fn get_omega(&self) -> F {
        self.omega
    }
}

#[derive(Clone, Debug)]
pub struct Cs {
    pub blinding: usize,
}
impl Cs {
    pub fn blinding_factors(&self) -> usize {
        self.blinding
    }
}
#[derive(Clone, Debug)]
pub struct Vk<F> {
    pub domain: EvaluationDomain<F>,
    pub cs: Cs,
    pub cs_degree: usize,
}
impl<F> Vk<F> {
    pub fn n(&self) -> u64 {
        self.domain.n
    }
}
pub trait PolynomialCommitmentScheme<F> {}
pub struct Pcs;
impl<F> PolynomialCommitmentScheme<F> for Pcs {}
pub struct ProvingKey<F, CS> {
    pub vk: Vk<F>,
    pub _cs: PhantomData<CS>,
}
pub fn pk<F: WithSmallOrderMulGroup<3>>(k: u32, blinding: usize, cs_degree: usize) -> ProvingKey<F, Pcs> {
    // Same omega as the production EvaluationDomain for this k.
    let mut omega = F::ROOT_OF_UNITY;
    for _ in k..F::S {
        omega = omega.square();
    }
    ProvingKey {
        vk: Vk { domain: EvaluationDomain { n: 1 << k, omega }, cs: Cs { blinding }, cs_degree },
        _cs: PhantomData,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    ConstraintSystemFailure,
}
pub type ExpressionPair<F> = (Polynomial<F, LagrangeCoeff>, Polynomial<F, LagrangeCoeff>);


// ---- permutation argument stand-ins -----------------------------------------
#[derive(Clone, Copy, Debug)]
pub enum Any {
    Advice(()),
    Fixed,
    Instance,
}
#[derive(Clone, Copy, Debug)]
pub struct PermColumn {
    pub ty: Any,
    pub idx: usize,
}
impl PermColumn {
    pub fn column_type(&self) -> Any {
        self.ty
    }
    pub fn index(&self) -> usize {
        self.idx
    }
}
pub struct PermProvingKey<F> {
    pub permutations: Vec<Polynomial<F, LagrangeCoeff>>,
}

// ---- order-injectable map ---------------------------------------------------
/// How `ShimMap::iter` orders its entries. Stock iterates a `RandomState`
/// `HashMap`, i.e. some permutation of the distinct keys; every variant here
/// is such a permutation, applied identically to stock and Loki.
#[derive(Clone, Debug)]
pub enum Order {
    /// Keys in `Ord` order (one particular HashMap order).
    Sorted,
    /// Keys in `Ord` order, then permuted: output position j holds rank perm[j].
    Perm(Vec<usize>),
    /// Seeded shuffle of the `Ord`-sorted keys.
    Seed(u64),
    /// A real `HashMap` with fixed SipHash keys (deterministic hash order).
    FixedHash,
}
thread_local! {
    pub static ORDER: RefCell<Order> = const { RefCell::new(Order::Sorted) };
}
pub fn with_order<R>(order: Order, f: impl FnOnce() -> R) -> R {
    let old = ORDER.with(|o| o.replace(order));
    let r = f();
    ORDER.with(|o| *o.borrow_mut() = old);
    r
}

pub struct ShimMap<K, V> {
    inner: std::collections::HashMap<K, V, std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>>,
}
impl<K: Hash + Eq + Ord, V> ShimMap<K, V> {
    pub fn with_capacity(n: usize) -> Self {
        Self { inner: std::collections::HashMap::with_capacity_and_hasher(n, Default::default()) }
    }
    pub fn entry(&mut self, k: K) -> std::collections::hash_map::Entry<'_, K, V> {
        self.inner.entry(k)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        self.inner.get_mut(k)
    }
    pub fn iter(&self) -> std::vec::IntoIter<(&K, &V)> {
        let order = ORDER.with(|o| o.borrow().clone());
        let mut items: Vec<(&K, &V)> = self.inner.iter().collect();
        match order {
            Order::FixedHash => {}
            Order::Sorted => items.sort_by(|a, b| a.0.cmp(b.0)),
            Order::Perm(perm) => {
                items.sort_by(|a, b| a.0.cmp(b.0));
                assert_eq!(perm.len(), items.len(), "order permutation length");
                items = perm.iter().map(|&r| items[r]).collect();
            }
            Order::Seed(seed) => {
                items.sort_by(|a, b| a.0.cmp(b.0));
                let mut s = seed | 1;
                for i in (1..items.len()).rev() {
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    items.swap(i, (s % (i as u64 + 1)) as usize);
                }
            }
        }
        items.into_iter()
    }
}
