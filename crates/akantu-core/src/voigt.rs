// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_voigthelper.hh,
// src/common/aka_voigthelper_tmpl.hh.

//! Voigt notation (upstream `VoigtHelper<dim>`), frozen contract.
//!
//! | dim | size | vector order (`VEC[I] = (i, j)`) | extended non-symmetric indices |
//! |---|---|---|---|
//! | 1 | 1 | `(xx)` | – |
//! | 2 | 3 | `(xx, yy, xy)` | `3 = (1,0)` |
//! | 3 | 6 | `(xx, yy, zz, yz, xz, xy)` | `6 = (2,1)`, `7 = (2,0)`, `8 = (1,0)` |
//!
//! [`VoigtTable::mat`] is upstream's `mat` (`I` as a function of `(i, j)`,
//! **including** the extended indices `≥ size` for the lower triangle:
//! 3-D `[[0,5,4],[8,1,3],[7,6,2]]`, 2-D `[[0,2],[3,1]]`); `vec` covers all
//! `dim²` entries. Strain factors (`matrix_to_voigt_with_factors`) are `2`
//! on shear entries (engineering strain), stress conversions use factor `1`.
//!
//! All loops follow upstream's index order exactly; the B-matrix helpers
//! accumulate with `+=` from `0.0` as upstream (`Bvoigt.zero()` then `+=`).

use crate::error::{Error, Result};
use crate::tensor::{Matrix, MatrixX};
use crate::types::Real;

/// Voigt tables for one spatial dimension (upstream `VoigtHelperData<dim>`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoigtTable {
    /// Spatial dimension.
    pub dim: usize,
    /// Voigt vector size `dim (dim − 1) / 2 + dim`.
    pub size: usize,
    mat: [[usize; 3]; 3],
    vec: [[usize; 2]; 9],
    factors: [Real; 6],
}

/// 1-D table.
pub const VOIGT_1D: VoigtTable = VoigtTable {
    dim: 1,
    size: 1,
    mat: [[0, 0, 0], [0, 0, 0], [0, 0, 0]],
    vec: [
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
    ],
    factors: [1., 0., 0., 0., 0., 0.],
};

/// 2-D table.
pub const VOIGT_2D: VoigtTable = VoigtTable {
    dim: 2,
    size: 3,
    mat: [[0, 2, 0], [3, 1, 0], [0, 0, 0]],
    vec: [
        [0, 0],
        [1, 1],
        [0, 1],
        [1, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
        [0, 0],
    ],
    factors: [1., 1., 2., 0., 0., 0.],
};

/// 3-D table.
pub const VOIGT_3D: VoigtTable = VoigtTable {
    dim: 3,
    size: 6,
    mat: [[0, 5, 4], [8, 1, 3], [7, 6, 2]],
    vec: [
        [0, 0],
        [1, 1],
        [2, 2],
        [1, 2],
        [0, 2],
        [0, 1],
        [2, 1],
        [2, 0],
        [1, 0],
    ],
    factors: [1., 1., 1., 2., 2., 2.],
};

/// Table for a runtime dimension (`1..=3`).
pub fn voigt_table(dim: usize) -> Result<&'static VoigtTable> {
    match dim {
        1 => Ok(&VOIGT_1D),
        2 => Ok(&VOIGT_2D),
        3 => Ok(&VOIGT_3D),
        _ => Err(Error::invalid(format!(
            "VoigtHelper: spatial dimension must be 1, 2 or 3 (got {dim})"
        ))),
    }
}

/// Voigt size for a compile-time dimension.
#[must_use]
pub const fn voigt_size(dim: usize) -> usize {
    dim * (dim - 1) / 2 + dim
}

impl VoigtTable {
    /// Upstream `mat[i][j]`: Voigt (or extended, `≥ size`) index of `(i, j)`.
    #[must_use]
    pub fn mat(&self, i: usize, j: usize) -> usize {
        debug_assert!(i < self.dim && j < self.dim);
        self.mat[i][j]
    }

    /// Upstream `vec[I]`: tensor indices of Voigt/extended index `I < dim²`.
    #[must_use]
    pub fn vec(&self, big_i: usize) -> (usize, usize) {
        debug_assert!(big_i < self.dim * self.dim);
        (self.vec[big_i][0], self.vec[big_i][1])
    }

    /// Upstream `factors[I]` (strain → engineering strain), `I < size`.
    #[must_use]
    pub fn factor(&self, big_i: usize) -> Real {
        debug_assert!(big_i < self.size);
        self.factors[big_i]
    }
}

/// Upstream `matrixToVoigt`: `v[I] = m(i, j)` for `I < size` (stress-like,
/// factor 1). `m` is column-major `D × D`.
#[must_use]
pub fn matrix_to_voigt<const D: usize, const S: usize>(m: &Matrix<D, D>) -> [Real; S] {
    let t = fixed_table::<D, S>();
    std::array::from_fn(|big_i| {
        let (i, j) = t.vec(big_i);
        m[j][i]
    })
}

/// Upstream `matrixToVoigtWithFactors`: `v[I] = factors[I] · m(i, j)`
/// (strain → engineering strain: shear entries doubled).
#[must_use]
pub fn matrix_to_voigt_with_factors<const D: usize, const S: usize>(m: &Matrix<D, D>) -> [Real; S] {
    let t = fixed_table::<D, S>();
    std::array::from_fn(|big_i| {
        let (i, j) = t.vec(big_i);
        t.factor(big_i) * m[j][i]
    })
}

/// Upstream `voigtToMatrix`: `m(i, j) = m(j, i) = v[I]` for `I < size`, in
/// increasing `I` (no factor).
#[must_use]
pub fn voigt_to_matrix<const D: usize, const S: usize>(v: &[Real; S]) -> Matrix<D, D> {
    let t = fixed_table::<D, S>();
    let mut m = [[0.0; D]; D];
    for (big_i, &value) in v.iter().enumerate() {
        let (i, j) = t.vec(big_i);
        m[j][i] = value;
        m[i][j] = value;
    }
    m
}

fn fixed_table<const D: usize, const S: usize>() -> &'static VoigtTable {
    const {
        assert!(D >= 1 && D <= 3, "VoigtHelper: dimension must be 1, 2 or 3");
        assert!(
            S == voigt_size(D),
            "VoigtHelper: S must be the Voigt size of D"
        );
    }
    match D {
        1 => &VOIGT_1D,
        2 => &VOIGT_2D,
        _ => &VOIGT_3D,
    }
}

fn check_b_shapes(
    context: &'static str,
    dim: usize,
    b: &MatrixX,
    b_rows: usize,
    out: &MatrixX,
    out_rows: usize,
    nb_nodes: usize,
) -> Result<()> {
    if b.rows() != b_rows || b.cols() < nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: b_rows,
            got: b.rows(),
        });
    }
    if out.rows() != out_rows || out.cols() < dim * nb_nodes {
        return Err(Error::DimensionMismatch {
            context,
            expected: dim * nb_nodes,
            got: out.cols(),
        });
    }
    Ok(())
}

/// Upstream `transferBMatrixToSymVoigtBMatrix`: `b` is `dim × nb_nodes`
/// (`∂N/∂x`), `out` is `size × dim·nb_nodes` (zeroed first).
pub fn b_matrix_to_sym_voigt(
    dim: usize,
    b: &MatrixX,
    out: &mut MatrixX,
    nb_nodes: usize,
) -> Result<()> {
    let t = voigt_table(dim)?;
    check_b_shapes("b_matrix_to_sym_voigt", dim, b, dim, out, t.size, nb_nodes)?;
    out.as_mut_slice().fill(0.0);
    for i in 0..dim {
        for n in 0..nb_nodes {
            *out.at_mut(i, i + n * dim) = b.at(i, n);
        }
    }
    if dim == 2 {
        for n in 0..nb_nodes {
            *out.at_mut(2, 1 + n * 2) = b.at(0, n);
            *out.at_mut(2, n * 2) = b.at(1, n);
        }
    }
    if dim == 3 {
        for n in 0..nb_nodes {
            let (dndx, dndy, dndz) = (b.at(0, n), b.at(1, n), b.at(2, n));
            *out.at_mut(3, 1 + n * 3) = dndz;
            *out.at_mut(3, 2 + n * 3) = dndy;
            *out.at_mut(4, n * 3) = dndz;
            *out.at_mut(4, 2 + n * 3) = dndx;
            *out.at_mut(5, n * 3) = dndy;
            *out.at_mut(5, 1 + n * 3) = dndx;
        }
    }
    Ok(())
}

/// Upstream `transferBMatrixToBNL` (Bathe et al. 1975): `out` is
/// `dim² × dim·nb_nodes` (zeroed first), `out(i·dim + n, m·dim + i) = b(n, m)`.
pub fn b_matrix_to_bnl(dim: usize, b: &MatrixX, out: &mut MatrixX, nb_nodes: usize) -> Result<()> {
    voigt_table(dim)?;
    check_b_shapes("b_matrix_to_bnl", dim, b, dim, out, dim * dim, nb_nodes)?;
    out.as_mut_slice().fill(0.0);
    for i in 0..dim {
        for m in 0..nb_nodes {
            for n in 0..dim {
                *out.at_mut(i * dim + n, m * dim + i) = b.at(n, m);
            }
        }
    }
    Ok(())
}

/// Upstream `transferBMatrixToBL2` (Bathe et al. 1975): `dndx` is
/// `dim × nb_nodes`, `grad_u` is `dim × dim`, `out` is `size × dim·nb_nodes`
/// (1-D: `1 × nb_nodes`). Same loop order and `+=` accumulation from `0.0`
/// as upstream.
pub fn b_matrix_to_bl2(
    dim: usize,
    dndx: &MatrixX,
    grad_u: &MatrixX,
    out: &mut MatrixX,
    nb_nodes: usize,
) -> Result<()> {
    let t = voigt_table(dim)?;
    if grad_u.rows() != dim || grad_u.cols() != dim {
        return Err(Error::DimensionMismatch {
            context: "b_matrix_to_bl2 (grad_u)",
            expected: dim,
            got: grad_u.rows(),
        });
    }
    match dim {
        1 => {
            if dndx.rows() != 1
                || dndx.cols() < nb_nodes
                || out.rows() != 1
                || out.cols() < nb_nodes
            {
                return Err(Error::DimensionMismatch {
                    context: "b_matrix_to_bl2 (1D)",
                    expected: nb_nodes,
                    got: out.cols(),
                });
            }
            out.as_mut_slice().fill(0.0);
            for j in 0..nb_nodes {
                *out.at_mut(0, j) = grad_u.at(0, 0) * dndx.at(0, j);
            }
        }
        2 => {
            check_b_shapes("b_matrix_to_bl2", dim, dndx, dim, out, t.size, nb_nodes)?;
            out.as_mut_slice().fill(0.0);
            for i in 0..2 {
                for j in 0..nb_nodes {
                    for k in 0..2 {
                        *out.at_mut(i, j * 2 + k) = grad_u.at(k, i) * dndx.at(i, j);
                    }
                }
            }
            for j in 0..nb_nodes {
                for k in 0..2 {
                    for m in 0..2 {
                        let (index1, index2) = (m, 1 - m);
                        *out.at_mut(2, j * 2 + k) += grad_u.at(k, index1) * dndx.at(index2, j);
                    }
                }
            }
        }
        _ => {
            check_b_shapes("b_matrix_to_bl2", dim, dndx, dim, out, t.size, nb_nodes)?;
            out.as_mut_slice().fill(0.0);
            for big_i in 0..3 {
                for a in 0..nb_nodes {
                    for i in 0..3 {
                        *out.at_mut(big_i, a * 3 + i) = grad_u.at(i, big_i) * dndx.at(big_i, a);
                    }
                }
            }
            for iv in 3..6 {
                for a in 0..nb_nodes {
                    for k in 0..3 {
                        let aux = iv - 3;
                        for m in 0..3 {
                            if m != aux {
                                let (index1, index2) = (m, 3 - m - aux);
                                *out.at_mut(iv, a * 3 + k) +=
                                    grad_u.at(k, index1) * dndx.at(index2, a);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::MatrixExt;

    #[test]
    fn tables_match_upstream() {
        assert_eq!(VOIGT_3D.mat, [[0, 5, 4], [8, 1, 3], [7, 6, 2]]);
        assert_eq!(
            VOIGT_3D.vec,
            [
                [0, 0],
                [1, 1],
                [2, 2],
                [1, 2],
                [0, 2],
                [0, 1],
                [2, 1],
                [2, 0],
                [1, 0]
            ]
        );
        assert_eq!(VOIGT_3D.factors, [1., 1., 1., 2., 2., 2.]);
        assert_eq!(VOIGT_2D.mat[0][..2], [0, 2]);
        assert_eq!(VOIGT_2D.mat[1][..2], [3, 1]);
        assert_eq!(VOIGT_2D.vec[..4], [[0, 0], [1, 1], [0, 1], [1, 0]]);
        assert_eq!(VOIGT_2D.factors[..3], [1., 1., 2.]);
        assert_eq!(VOIGT_1D.size, 1);
        assert_eq!((voigt_size(1), voigt_size(2), voigt_size(3)), (1, 3, 6));
        assert!(voigt_table(4).is_err());
        assert!(voigt_table(0).is_err());
    }

    #[test]
    fn mat_and_vec_are_inverse() {
        for t in [&VOIGT_1D, &VOIGT_2D, &VOIGT_3D] {
            for big_i in 0..t.dim * t.dim {
                let (i, j) = t.vec(big_i);
                assert_eq!(t.mat(i, j), big_i);
            }
            // symmetric part maps both triangles onto the first `size` indices
            for i in 0..t.dim {
                for j in 0..t.dim {
                    let (a, b) = (t.mat(i, j), t.mat(j, i));
                    assert!(a.min(b) < t.size);
                    if i != j {
                        assert!(a.max(b) >= t.size);
                    }
                }
            }
        }
    }

    #[test]
    fn conversions() {
        let m: Matrix<3, 3> = Matrix::from_rows([[1., 6., 5.], [6., 2., 4.], [5., 4., 3.]]);
        assert_eq!(matrix_to_voigt::<3, 6>(&m), [1., 2., 3., 4., 5., 6.]);
        assert_eq!(
            matrix_to_voigt_with_factors::<3, 6>(&m),
            [1., 2., 3., 8., 10., 12.]
        );
        assert_eq!(voigt_to_matrix::<3, 6>(&[1., 2., 3., 4., 5., 6.]), m);
        let m2: Matrix<2, 2> = Matrix::from_rows([[1., 3.], [3., 2.]]);
        assert_eq!(matrix_to_voigt::<2, 3>(&m2), [1., 2., 3.]);
        assert_eq!(matrix_to_voigt_with_factors::<2, 3>(&m2), [1., 2., 6.]);
        assert_eq!(voigt_to_matrix::<2, 3>(&[1., 2., 3.]), m2);
        assert_eq!(matrix_to_voigt::<1, 1>(&[[7.]]), [7.]);
        // non-symmetric input: upper triangle is read (vec[I] has i <= j)
        let ns: Matrix<2, 2> = Matrix::from_rows([[1., 3.], [-3., 2.]]);
        assert_eq!(matrix_to_voigt::<2, 3>(&ns)[2], ns.at(0, 1));
    }

    #[test]
    fn b_matrices_2d() {
        // 2 nodes, dN/dx = [[a0, a1], [b0, b1]]
        let b = MatrixX::from_col_major(2, 2, vec![1., 2., 3., 4.]).unwrap();
        let mut out = MatrixX::zeros(3, 4);
        b_matrix_to_sym_voigt(2, &b, &mut out, 2).unwrap();
        let rows = |m: &MatrixX| -> Vec<Vec<Real>> {
            (0..m.rows())
                .map(|i| (0..m.cols()).map(|j| m.at(i, j)).collect())
                .collect()
        };
        assert_eq!(
            rows(&out),
            vec![
                vec![1., 0., 3., 0.],
                vec![0., 2., 0., 4.],
                vec![2., 1., 4., 3.]
            ]
        );
        let mut bnl = MatrixX::zeros(4, 4);
        b_matrix_to_bnl(2, &b, &mut bnl, 2).unwrap();
        assert_eq!(bnl.at(1, 0), 2.0); // out(0*2+1, 0) = b(1, 0)
        assert_eq!(bnl.at(3, 1), 2.0); // out(1*2+1, 0*2+1) = b(1, 0)
        let gu = MatrixX::from_col_major(2, 2, vec![0.1, 0.2, 0.3, 0.4]).unwrap();
        let mut bl2 = MatrixX::zeros(3, 4);
        b_matrix_to_bl2(2, &b, &gu, &mut bl2, 2).unwrap();
        // row 2, node 0, k = 0: grad_u(0,0)*dndx(1,0) + grad_u(0,1)*dndx(0,0)
        assert_eq!(bl2.at(2, 0), (0.0 + 0.1 * 2.0) + 0.3 * 1.0);
        assert!(b_matrix_to_sym_voigt(2, &b, &mut MatrixX::zeros(3, 2), 2).is_err());
        assert!(b_matrix_to_bl2(4, &b, &gu, &mut bl2, 2).is_err());
    }
}
