//! Feature-gated polynomial work for the KZG multiopen.
//!
//! Value-preservation contract: `LokiLean/Multiopen.lean` in the sibling
//! `loki-lean` repository (indexed sets, coefficient chunks, and lhs length).
use super::*;
use ff::PrimeField;
use rayon::prelude::*;

// Avoid scheduling short polynomials. Each task owns a disjoint output chunk.
const PARALLEL_THRESHOLD: usize = 4096;
const MIN_CHUNK: usize = 1024;

/// Quotient by the monic product of `X - root`, dropping the remainder.
/// Ported from midnight-zk #565. Unlike a chain of synthetic divisions this
/// scans the coefficients once, skipping zero terms of the root product.
/// `LokiLean/Multiopen.lean` models the Kate fold as division by this product;
/// retain exactly `poly.len() - roots.len()` coefficients, including zeros.
fn divide_by_roots<F: ff::Field>(poly: &[F], roots: &[F]) -> Vec<F> {
    let degree = roots.len();
    if degree == 0 {
        return poly.to_vec();
    }
    if degree >= poly.len() {
        return vec![];
    }
    let mut vanishing = vec![F::ONE];
    for root in roots {
        let mut next = vec![F::ZERO; vanishing.len() + 1];
        for (i, coefficient) in vanishing.iter().enumerate() {
            next[i + 1] += coefficient;
            next[i] -= *coefficient * root;
        }
        vanishing = next;
    }
    let terms: Vec<_> = vanishing[..degree]
        .iter()
        .enumerate()
        .filter(|(_, coefficient)| !bool::from(coefficient.is_zero()))
        .map(|(j, coefficient)| (j, *coefficient))
        .collect();
    let mut remainder = poly.to_vec();
    let mut quotient = vec![F::ZERO; poly.len() - degree];
    for i in (degree..poly.len()).rev() {
        let lead = remainder[i];
        quotient[i - degree] = lead;
        for (j, coefficient) in &terms {
            remainder[i - degree + j] -= lead * coefficient;
        }
    }
    quotient
}

/// Scale and add in polynomial order, parallelizing only coefficient ranges.
/// The result retains the first polynomial's length, just like Polynomial Add;
/// in particular, f may be padded longer than the q polynomials in the final fold.
/// Powers are materialized once, including the per-power truncation when used.
pub(super) fn combine<F: PrimeField>(
    polys: &[&Polynomial<F, Coeff>],
    scalars: impl Iterator<Item = F>,
) -> Polynomial<F, Coeff> {
    let scalars: Vec<_> = scalars.take(polys.len()).collect();
    assert_eq!(scalars.len(), polys.len());
    let n = polys
        .first()
        .expect("nonempty polynomial combination")
        .values
        .len();
    let mut values = vec![F::ZERO; n];
    let combine_chunk = |output: &mut [F], start: usize| {
        for (poly, &scalar) in polys.iter().zip(&scalars) {
            if scalar == F::ZERO || start >= poly.values.len() {
                continue;
            }
            for (acc, &coefficient) in output.iter_mut().zip(&poly.values[start..]) {
                *acc += if scalar == F::ONE {
                    coefficient
                } else {
                    coefficient * scalar
                };
            }
        }
    };
    if n >= PARALLEL_THRESHOLD {
        let chunk = MIN_CHUNK.max(n.div_ceil(rayon::current_num_threads()));
        values
            .par_chunks_mut(chunk)
            .enumerate()
            .for_each(|(i, output)| {
                combine_chunk(output, i * chunk);
            });
    } else {
        combine_chunk(&mut values, 0);
    }
    Polynomial {
        values,
        _marker: PhantomData,
    }
}

/// Borrow queries, move the sorted buffers, and divide independent sets in
/// parallel. Indexed collection preserves the serial set and transcript order.
pub(super) fn prepare<F: PrimeField + Hash + Ord>(
    queries: &[ProverQuery<F>],
    x1: F,
    x2: F,
    k: u32,
) -> Result<(Vec<Polynomial<F, Coeff>>, Polynomial<F, Coeff>), Error> {
    let (map, point_sets) = super::utils::construct_prover_sets(queries)?;
    let mut groups = vec![vec![]; point_sets.len()];
    for data in &map {
        groups[data.set_index].push(data.commitment.poly);
    }
    let combine_set = |polys: &Vec<&Polynomial<F, Coeff>>| {
        #[cfg(feature = "truncated-challenges")]
        let scalars = truncated_powers(x1);
        #[cfg(not(feature = "truncated-challenges"))]
        let scalars = powers(x1);
        combine(polys, scalars)
    };
    let work: usize = groups.iter().flatten().map(|p| p.values.len()).sum();
    let q_polys: Vec<_> = if groups.len() > 1 && work >= PARALLEL_THRESHOLD {
        groups.par_iter().map(combine_set).collect()
    } else {
        groups.iter().map(combine_set).collect()
    };
    let mut sets: Vec<_> = point_sets.into_iter().zip(q_polys).enumerate().collect();
    sets.sort_by_key(|(i, (points, _))| (points.len(), *i));
    let (point_sets, q_polys): (Vec<_>, Vec<_>) = sets.into_iter().map(|(_, pair)| pair).unzip();

    let divide_set = |(points, q): (&Vec<F>, &Polynomial<F, Coeff>)| {
        let mut values = divide_by_roots(&q.values, points);
        values.resize(1 << k, F::ZERO);
        Polynomial {
            values,
            _marker: PhantomData,
        }
    };
    let f_polys: Vec<_> = if q_polys.len() > 1 && work >= PARALLEL_THRESHOLD {
        point_sets
            .par_iter()
            .zip(&q_polys)
            .map(divide_set)
            .collect()
    } else {
        point_sets.iter().zip(&q_polys).map(divide_set).collect()
    };
    let f = combine(&f_polys.iter().collect::<Vec<_>>(), powers(x2));
    Ok((q_polys, f))
}

#[cfg(test)]
mod tests;
