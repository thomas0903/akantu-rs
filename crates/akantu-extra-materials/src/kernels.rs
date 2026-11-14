// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/*.cc (the shared point operations: `sigma *= 1 - dam`,
// `sigma.eig(...)` + `std::max_element`).

//! Point operations shared by the extra-materials laws.
//!
//! # Eigen-decompositions (UD-572)
//!
//! The extra code calls `sigma.eig(values)` and `sigma.eig(values,
//! vectors)` on `Matrix<Real>` (run-time size). Against the reference core
//! (Akantu 5, `src/common/aka_types.hh`) these are Eigen's `EigenSolver`:
//! real parts in Eigen's (unsorted) order for values only, values sorted
//! **descending** with their eigenvector columns when vectors are
//! requested. The port calls the bit-level Eigen port of
//! `akantu_linalg::dense` with [`Storage::Dynamic`] (run-time size), so a
//! future compilable upstream would agree bit for bit on these steps.

use akantu_core::tensor::{Matrix, MatrixX};
use akantu_core::{Error, Real, Result};
use akantu_linalg::dense::{SortOrder, Storage, eig, eig_vectors};

/// C++ `std::max(a, b)` (`(a < b) ? b : a`).
pub use akantu_solid::materials::damage::{std_max, std_min};

/// Upstream `sigma *= 1 - dam` (each entry times the evaluated factor).
#[inline]
pub fn scale_by_integrity<const D: usize>(sigma: &mut Matrix<D, D>, damage: Real) {
    let f = 1. - damage;
    for col in sigma.iter_mut() {
        for s in col.iter_mut() {
            *s *= f;
        }
    }
}

/// Dynamic copy of a fixed matrix (upstream `Matrix<Real>(dim, dim)`).
#[must_use]
pub fn dynamic<const D: usize>(m: &Matrix<D, D>) -> MatrixX {
    MatrixX::from_fixed(m)
}

/// `sigma.eig(eigenvalues)` (Eigen order, real parts) followed by
/// `*std::max_element(values, values + dim)` (the first largest value:
/// `operator<` comparisons from the front).
///
/// # Errors
/// [`Error::Numerical`] when the eigen-solver fails (non-finite input;
/// upstream would read uninitialised values).
pub fn max_eigenvalue<const D: usize>(sigma: &Matrix<D, D>) -> Result<Real> {
    let values = eig(&dynamic(sigma), Storage::Dynamic)?;
    let mut it = values.iter();
    let first = *it
        .next()
        .ok_or_else(|| Error::Numerical("eig: empty spectrum".into()))?;
    Ok(it.fold(first, |m, &v| if m < v { v } else { m }))
}

/// `sigma.eig(values)` (Eigen order, real parts) as a fixed array.
///
/// # Errors
/// As [`max_eigenvalue`].
pub fn eigenvalues<const D: usize>(sigma: &Matrix<D, D>) -> Result<[Real; D]> {
    let values = eig(&dynamic(sigma), Storage::Dynamic)?;
    values
        .try_into()
        .map_err(|v: Vec<Real>| Error::DimensionMismatch {
            context: "eig",
            expected: D,
            got: v.len(),
        })
}

/// `sigma.eig(values, vectors)` (Akantu 5 default `sort = true`): values
/// sorted descending, eigenvector columns permuted with them.
///
/// # Errors
/// As [`max_eigenvalue`]; [`Error::Numerical`] for complex eigenvectors
/// (upstream's exception).
pub fn sorted_eigen<const D: usize>(sigma: &Matrix<D, D>) -> Result<([Real; D], Matrix<D, D>)> {
    let (values, vectors) = eig_vectors(&dynamic(sigma), Storage::Dynamic, SortOrder::Descending)?;
    let values: [Real; D] = values
        .try_into()
        .map_err(|v: Vec<Real>| Error::DimensionMismatch {
            context: "eig",
            expected: D,
            got: v.len(),
        })?;
    let mut v = [[0.; D]; D];
    for (j, col) in v.iter_mut().enumerate() {
        for (i, x) in col.iter_mut().enumerate() {
            *x = vectors.at(i, j);
        }
    }
    Ok((values, v))
}

/// Check that all blocks of a computation have the expected length.
///
/// # Errors
/// [`Error::DimensionMismatch`] naming `context`.
pub fn check_lengths(context: &'static str, expected: usize, lengths: &[usize]) -> Result<()> {
    match lengths.iter().find(|&&l| l != expected) {
        Some(&got) => Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        }),
        None => Ok(()),
    }
}

/// UD-572: the extra-materials laws are small-strain laws (their Akantu 4
/// quadrature-point macros iterate `∇u` and the Cauchy stress); upstream
/// never defined `finite_deformation = true` for them.
///
/// # Errors
/// [`Error::Unsupported`] when `finite_deformation` is set.
pub fn small_strain_only(law: &str, finite_deformation: bool) -> Result<()> {
    if finite_deformation {
        return Err(Error::Unsupported(format!(
            "{law} with finite_deformation: the extra-materials laws are small-strain laws \
             (UD-572); no finite-strain formulation exists upstream"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_eigenvalue_of_diagonal_and_rotated_tensors() {
        let s: Matrix<2, 2> = [[3., 0.], [0., -1.]];
        assert_eq!(max_eigenvalue(&s).unwrap(), 3.);
        // [[1, 2], [2, 1]] has eigenvalues 3 and -1
        let s: Matrix<2, 2> = [[1., 2.], [2., 1.]];
        assert!((max_eigenvalue(&s).unwrap() - 3.).abs() < 1e-14);
        let s: Matrix<3, 3> = [[2., 0., 0.], [0., 5., 0.], [0., 0., -7.]];
        assert_eq!(max_eigenvalue(&s).unwrap(), 5.);
    }

    #[test]
    fn sorted_eigen_is_descending_with_unit_vectors() {
        let s: Matrix<2, 2> = [[1., 2.], [2., 1.]];
        let (v, w) = sorted_eigen(&s).unwrap();
        assert!(v[0] > v[1]);
        for (k, col) in w.iter().enumerate() {
            let n: Real = col.iter().map(|x| x * x).sum();
            assert!((n - 1.).abs() < 1e-14);
            // A·w = λ·w
            for i in 0..2 {
                let aw = s[0][i] * col[0] + s[1][i] * col[1];
                assert!((aw - v[k] * col[i]).abs() < 1e-13);
            }
        }
    }

    #[test]
    fn integrity_scaling() {
        let mut s: Matrix<2, 2> = [[2., 4.], [4., -8.]];
        scale_by_integrity(&mut s, 0.25);
        assert_eq!(s, [[1.5, 3.], [3., -6.]]);
    }
}
