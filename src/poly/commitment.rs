//! Trait for a commitment scheme
use core::ops::{Add, Mul};
use std::{fmt::Debug, hash::Hash};

use ff::{FromUniformBytes, PrimeField};
#[cfg(feature = "cuda-argument-commit")]
use ff::WithSmallOrderMulGroup;

#[cfg(feature = "cuda-argument-commit")]
use crate::poly::EvaluationDomain;

use crate::{
    plonk::{k_from_circuit, Circuit},
    poly::{Coeff, Error, LagrangeCoeff, Polynomial, ProverQuery, VerifierQuery},
    transcript::{Hashable, Sampleable, Transcript},
    utils::helpers::ProcessedSerdeObject,
};

/// Public interface for a additively homomorphic Polynomial Commitment Scheme
/// (PCS)
pub trait PolynomialCommitmentScheme<F: PrimeField>: Clone + Debug {
    /// Parameters needed to generate a proof in the PCS
    type Parameters: Params;

    /// Parameters needed to verify a proof in the PCS
    type VerifierParameters;

    /// Type of a committed polynomial
    type Commitment: Clone
        + Debug
        + Default
        + PartialEq
        + ProcessedSerdeObject
        + Send
        + Sync
        + Add<Output = Self::Commitment>
        + Mul<F, Output = Self::Commitment>;

    /// Verification guard. Allows for batch verification
    type VerificationGuard: Guard<F, Self>;

    /// Generates the parameters of the polynomial commitment scheme
    fn gen_params(k: u32) -> Self::Parameters;

    /// Extract the `VerifierParameters` from `Parameters`
    fn get_verifier_params(params: &Self::Parameters) -> Self::VerifierParameters;

    /// Commit to a polynomial in coefficient form
    fn commit(params: &Self::Parameters, polynomial: &Polynomial<F, Coeff>) -> Self::Commitment;

    /// Commit to a polynomial expressed in Lagrange evaluations form (over the
    /// underlying domain specified in params).
    fn commit_lagrange(
        params: &Self::Parameters,
        poly: &Polynomial<F, LagrangeCoeff>,
    ) -> Self::Commitment;

    /// Create a multi-opening proof at a set of [ProverQuery]'s.
    fn multi_open<T: Transcript>(
        params: &Self::Parameters,
        prover_query: &[ProverQuery<F>],
        transcript: &mut T,
    ) -> Result<(), Error>
    where
        F: Sampleable<T::Hash> + Hash + Ord + Hashable<T::Hash>,
        Self::Commitment: Hashable<T::Hash>;

    /// Verify an multi-opening proof for a given set of [VerifierQuery]'s.
    /// The function fails if the transcript has trailing bytes.
    fn multi_prepare<'com, T: Transcript>(
        verifier_query: &[VerifierQuery<'com, F, Self>],
        transcript: &mut T,
    ) -> Result<Self::VerificationGuard, Error>
    where
        F: Sampleable<T::Hash> + Hash + Ord + Hashable<T::Hash>,
        Self::Commitment: 'com + Hashable<T::Hash>;
}

/// Interface for verifier finalizer
pub trait Guard<F: PrimeField, CS: PolynomialCommitmentScheme<F>>: Sized {
    /// Finalize the verification guard
    fn verify(self, params: &CS::VerifierParameters) -> Result<(), Error>;

    /// Finalize a batch of verification guards
    fn batch_verify<'a, I, J>(guards: I, params: J) -> Result<(), Error>
    where
        I: ExactSizeIterator<Item = Self>,
        J: ExactSizeIterator<Item = &'a CS::VerifierParameters>,
        CS::VerifierParameters: 'a,
    {
        assert_eq!(guards.len(), params.len());
        guards
            .into_iter()
            .zip(params)
            .try_for_each(|(guard, params)| guard.verify(params))
    }
}

/// Interface for PCS params
pub trait Params {
    /// Returns the max size of polynomials that these parameters can commit to
    fn max_k(&self) -> u32;

    /// Downsize the params to work with a circuit of size `new_k`
    fn downsize(&mut self, new_k: u32);

    /// Downsize the params to work with a circuit of unknown length. The
    /// function first computes the `k` of the provided circuit, and then
    /// downsizes the SRS.
    fn downsize_from_circuit<
        F: PrimeField + Ord + FromUniformBytes<64>,
        ConcreCircuit: Circuit<F>,
    >(
        &mut self,
        circuit: &ConcreCircuit,
    ) {
        let k = k_from_circuit(circuit);
        self.downsize(k);
    }
}

/// Reuse the coefficient polynomial needed for argument openings when the SRS
/// and evaluation domain match. This assumes valid PCS parameters: for KZG,
/// `g_lagrange` must be the inverse transform of `g` for this exact domain.
/// Matching sizes does not validate arbitrary supplied or deserialized bases.
/// With a larger SRS, preserve the original prefix-Lagrange commitment.
#[cfg(feature = "cuda-argument-commit")]
pub(crate) fn commit_argument<
    F: WithSmallOrderMulGroup<3>,
    CS: PolynomialCommitmentScheme<F>,
>(
    params: &CS::Parameters,
    domain: &EvaluationDomain<F>,
    values: Polynomial<F, LagrangeCoeff>,
) -> (Polynomial<F, Coeff>, CS::Commitment) {
    #[cfg(feature = "loki-profile")]
    let size = values.len();
    if params.max_k() == domain.k() {
        let poly = domain.lagrange_to_coeff(values);
        #[cfg(feature = "loki-profile")]
        crate::profile::record_backend("argument_commit.coeff", size);
        let commitment = CS::commit(params, &poly);
        (poly, commitment)
    } else {
        #[cfg(feature = "loki-profile")]
        crate::profile::record_backend("argument_commit.lagrange", size);
        let commitment = CS::commit_lagrange(params, &values);
        let poly = domain.lagrange_to_coeff(values);
        (poly, commitment)
    }
}

#[cfg(all(test, feature = "cuda-argument-commit"))]
mod argument_commit_tests {
    use ff::Field;
    use midnight_curves::{Bls12, Fq};
    use rand_chacha::{rand_core::SeedableRng, ChaCha20Rng};

    use super::*;
    use crate::poly::{
        kzg::{params::ParamsKZG, KZGCommitmentScheme},
        EvaluationDomain,
    };

    type Scheme = KZGCommitmentScheme<Bls12>;

    #[test]
    fn argument_commit_matches_full_domain_lagrange_for_edge_and_random_values() {
        let mut rng = ChaCha20Rng::from_seed([73; 32]);
        for k in [1, 3, 4] {
            let params = ParamsKZG::<Bls12>::unsafe_setup(k, &mut rng);
            let domain = EvaluationDomain::new(1, k);
            let n = 1 << k;
            let wide = Fq::random(&mut rng);
            let mut cases = vec![
                vec![Fq::ZERO; n],
                vec![Fq::ONE; n],
                vec![-Fq::ONE; n],
                (0..n).map(|i| Fq::from((i % 3) as u64)).collect(),
                vec![wide; n],
            ];
            cases.extend((0..4).map(|_| (0..n).map(|_| Fq::random(&mut rng)).collect()));
            for values in cases {
                let values = domain.lagrange_from_vec(values);
                let expected_poly = domain.lagrange_to_coeff(values.clone());
                let expected_commitment = Scheme::commit_lagrange(&params, &values);
                let (poly, commitment) = commit_argument::<Fq, Scheme>(&params, &domain, values);
                assert_eq!(poly, expected_poly);
                assert_eq!(poly.len(), n);
                assert_eq!(commitment, expected_commitment);
                assert_eq!(commitment, Scheme::commit(&params, &poly));
            }
        }
    }

    #[test]
    fn argument_commit_preserves_prefix_lagrange_with_oversized_params() {
        let mut rng = ChaCha20Rng::from_seed([74; 32]);
        let params = ParamsKZG::<Bls12>::unsafe_setup(4, &mut rng);
        let domain = EvaluationDomain::new(1, 3);
        let values = domain.lagrange_from_vec(vec![Fq::ONE; 8]);
        let expected_poly = domain.lagrange_to_coeff(values.clone());
        let expected_commitment = Scheme::commit_lagrange(&params, &values);
        let coefficient_commitment = Scheme::commit(&params, &expected_poly);
        // A prefix of the larger Lagrange basis is not the smaller domain's basis.
        assert_ne!(expected_commitment, coefficient_commitment);
        let (poly, commitment) = commit_argument::<Fq, Scheme>(&params, &domain, values);
        assert_eq!(poly, expected_poly);
        assert_eq!(poly.len(), 8);
        assert_eq!(commitment, expected_commitment);
    }

    #[test]
    fn argument_commit_matches_lagrange_after_downsize_regenerates_basis() {
        let mut rng = ChaCha20Rng::from_seed([75; 32]);
        let mut params = ParamsKZG::<Bls12>::unsafe_setup(4, &mut rng);
        params.downsize(3);
        let domain = EvaluationDomain::new(1, 3);
        assert_eq!(params.max_k(), domain.k());
        let values = domain.lagrange_from_vec((0..8).map(|_| Fq::random(&mut rng)).collect());
        let expected_poly = domain.lagrange_to_coeff(values.clone());
        let expected_commitment = Scheme::commit_lagrange(&params, &values);
        let (poly, commitment) = commit_argument::<Fq, Scheme>(&params, &domain, values);
        assert_eq!(poly, expected_poly);
        assert_eq!(poly.len(), 8);
        assert_eq!(commitment, expected_commitment);
        assert_eq!(commitment, Scheme::commit(&params, &poly));
    }
}
