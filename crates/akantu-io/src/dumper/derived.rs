// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/dumper_material_padders.hh
// (ComputeStrain, ComputePrincipalStrain, ComputeVonMisesStress,
// StressPadder, StrainPadder), src/io/dumper/dumper_homogenizing_field.hh
// (AvgHomogenizingFunctor) and the tensor helpers they call in
// src/model/solid_mechanics/material_inline_impl.hh (gradUToEpsilon,
// gradUToE, stressToVonMises).

//! Derived solid-mechanics dump fields (DUMP-024…030) and the quadrature
//! homogeniser (DUMP-012).
//!
//! Every function works on the tuple of one element: `nq` quadrature-point
//! blocks of `dim × dim` column-major matrices (upstream's per-element
//! `Vector<Real>` of an `InternalMaterialField`). Evaluation orders follow
//! Eigen 3.4 without vectorisation (`akantu_core::eigen_order`): upstream
//! views each block through a run-time-size `MatrixProxy`, so reductions
//! (products, traces, `doubleDot`) are sequential from the first term.
//!
//! Upstream defects handled here (oracle evidence in
//! `fixtures/dump/smm2d*`):
//! * UD-111: `StressPadder` allocates the padded 3×3 blocks without
//!   initialising them: the out-of-plane shear entries (and σzz for plane
//!   stress) are heap garbage (`fixtures/dump/smm2d/NONDETERMINISTIC`).
//!   akantu-rs writes zeros there.
//! * UD-112: the padded 2-D strain field cannot even be registered:
//!   attaching the homogeniser after compute + pad selects
//!   `FieldComputeProxy::connectToFunctor(FieldCompute<FieldCompute<…>>)`,
//!   whose bare `throw;` (`dumper_compute.hh:354`) terminates the oracle at
//!   registration (`fixtures/dump/smm2d_strain_pad`, exit 134). Latent
//!   behind it, never reached: `StrainPadder` calls the generic
//!   (non-padding) `pad` and writes εzz out of bounds. akantu-rs pads to
//!   3×3 with zeros and sets εzz = ν/(ν−1)(εxx+εyy) for plane stress, as
//!   the source intends (DUMP-030).

use akantu_core::eigen_order::redux_seq;
use akantu_core::tensor::MatrixX;
use akantu_core::{Error, Real, Result};
use akantu_linalg::dense::{Storage, eig};

fn add(a: Real, b: Real) -> Real {
    a + b
}

/// Coefficient `(i, j)` of the column-major `dim × dim` block `g`.
#[inline]
fn at(g: &[Real], dim: usize, i: usize, j: usize) -> Real {
    g[i + dim * j]
}

/// `Material::gradUToEpsilon`: `ε = ½(∇uᵀ + ∇u)` into `out` (column-major).
pub(crate) fn small_strain(g: &[Real], dim: usize, out: &mut [Real]) {
    for j in 0..dim {
        for i in 0..dim {
            out[i + dim * j] = 0.5 * (at(g, dim, j, i) + at(g, dim, i, j));
        }
    }
}

/// `Material::gradUToE`: `E = (∇uᵀ∇u + ∇uᵀ + ∇u) / 2`; the product of two
/// run-time-size views is coefficient-based with a sequential sum over the
/// depth (Eigen GEMM below the coefficient threshold).
pub(crate) fn green_strain(g: &[Real], dim: usize, out: &mut [Real]) {
    for j in 0..dim {
        for i in 0..dim {
            let p = redux_seq(dim, &|k| at(g, dim, k, i) * at(g, dim, k, j), add);
            out[i + dim * j] = ((p + at(g, dim, j, i)) + at(g, dim, i, j)) / 2.;
        }
    }
}

/// Strain measure of a derived field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrainMeasure {
    Small,
    Green,
}

/// `ComputeStrain<green>`: per element, the strain of every quadrature
/// point (`dim × (dim·nq)` matrix, column-major = `nq` blocks).
pub(crate) fn compute_strain(input: &[Real], dim: usize, measure: StrainMeasure) -> Vec<Real> {
    let block = dim * dim;
    let mut out = vec![0.0; input.len()];
    for (g, o) in input.chunks_exact(block).zip(out.chunks_exact_mut(block)) {
        match measure {
            StrainMeasure::Small => small_strain(g, dim, o),
            StrainMeasure::Green => green_strain(g, dim, o),
        }
    }
    out
}

/// `ComputePrincipalStrain<green>`: unsorted eigenvalues (Eigen
/// `EigenSolver<MatrixXd>` order, real parts) of every quadrature point's
/// strain (`dim × nq` matrix).
pub(crate) fn compute_principal_strain(
    input: &[Real],
    dim: usize,
    measure: StrainMeasure,
) -> Result<Vec<Real>> {
    let block = dim * dim;
    let mut out = Vec::with_capacity(input.len() / block.max(1) * dim);
    let mut strain = vec![0.0; block];
    for g in input.chunks_exact(block) {
        match measure {
            StrainMeasure::Small => small_strain(g, dim, &mut strain),
            StrainMeasure::Green => green_strain(g, dim, &mut strain),
        }
        let m = MatrixX::from_col_major(dim, dim, strain.clone())?;
        let values = eig(&m, Storage::Dynamic)?;
        out.extend_from_slice(&values);
    }
    Ok(out)
}

/// `Material::stressToVonMises` of one `dim × dim` block: deviator with the
/// trace divided by 3 in every dimension (upstream semantics), then
/// `sqrt(3·(s:s)/2)`.
pub(crate) fn von_mises(s: &[Real], dim: usize) -> Real {
    let tr = redux_seq(dim, &|k| at(s, dim, k, k), add);
    let dev = |l: usize| {
        let (i, j) = (l % dim, l / dim);
        let id = if i == j { 1.0 } else { 0.0 };
        s[l] - (id * tr) / 3.
    };
    let dd = redux_seq(dim * dim, &|l| dev(l) * dev(l), add);
    ((3. * dd) / 2.).sqrt()
}

/// Upstream `Material::stressToVonMises(stress)` of a column-major
/// `dim × dim` stress (SMM-028): the dump pipeline's computation (both
/// reductions sequential, as the dynamic Eigen expression), compared
/// bitwise with the oracle by the dump differentials; also exported as
/// `akantu_solid::material_tensor::stress_to_von_mises`.
///
/// # Errors
/// `dim` outside `1..=3` or fewer than `dim²` values.
pub fn stress_to_von_mises(stress: &[Real], dim: usize) -> Result<Real> {
    if !(1..=3).contains(&dim) {
        return Err(Error::InvalidArgument(format!(
            "stressToVonMises: dimension {dim} (expected 1, 2 or 3)"
        )));
    }
    if stress.len() < dim * dim {
        return Err(Error::DimensionMismatch {
            context: "stressToVonMises: stress values",
            expected: dim * dim,
            got: stress.len(),
        });
    }
    Ok(von_mises(stress, dim))
}

/// `ComputeVonMisesStress`: one value per quadrature point.
pub(crate) fn compute_von_mises(input: &[Real], dim: usize) -> Vec<Real> {
    input
        .chunks_exact(dim * dim)
        .map(|s| von_mises(s, dim))
        .collect()
}

/// 2-D → 3-D padding of the `nq` column-major 2×2 blocks of one element
/// into 3×3 blocks (zeros out of plane), then the out-of-plane normal
/// component `zz(d)` of every block.
fn pad_2d(input: &[Real], zz: impl Fn(Real, Real) -> Real) -> Vec<Real> {
    let nq = input.len() / 4;
    let mut out = vec![0.0; 9 * nq];
    for (b, o) in input
        .as_chunks::<4>()
        .0
        .iter()
        .zip(out.as_chunks_mut::<9>().0)
    {
        o[0] = b[0];
        o[1] = b[1];
        o[3] = b[2];
        o[4] = b[3];
        o[8] = zz(o[0], o[4]);
    }
    out
}

/// `StressPadder<2>`: plane strain (the element's law has `Plane_Stress =
/// false`): `σzz = ν(σxx + σyy)`; plane stress: `σzz = 0`.
pub(crate) fn pad_stress_2d(input: &[Real], nu: Real, plane_stress: bool) -> Vec<Real> {
    pad_2d(
        input,
        |xx, yy| if plane_stress { 0.0 } else { nu * (xx + yy) },
    )
}

/// `StrainPadder<2>` (as intended, DUMP-030): plane stress:
/// `εzz = ν/(ν−1)(εxx + εyy)`; plane strain: `εzz = 0`.
pub(crate) fn pad_strain_2d(input: &[Real], nu: Real, plane_stress: bool) -> Vec<Real> {
    pad_2d(input, |xx, yy| {
        if plane_stress {
            nu / (nu - 1.) * (xx + yy)
        } else {
            0.0
        }
    })
}

/// `AvgHomogenizingFunctor::func`: average of the `len / nb_data` blocks
/// of `nb_data` values (`res = 0; res += block…; res /= n`).
pub(crate) fn homogenize(input: &[Real], nb_data: usize) -> Result<Vec<Real>> {
    if nb_data == 0 || !input.len().is_multiple_of(nb_data) {
        return Err(Error::DimensionMismatch {
            context: "dumper homogenisation (values per element vs values per point)",
            expected: nb_data,
            got: input.len(),
        });
    }
    let n = input.len() / nb_data;
    let mut res = vec![0.0; nb_data];
    for block in input.chunks_exact(nb_data) {
        for (r, &v) in res.iter_mut().zip(block) {
            *r += v;
        }
    }
    // `Vector /= Int`: the count converted to double.
    #[allow(clippy::cast_precision_loss)] // element point counts are tiny
    let nf = n as Real;
    for r in &mut res {
        *r /= nf;
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strains_of_a_known_gradient() {
        // grad_u = [[1, 2], [3, 4]] column-major: [1, 3, 2, 4]
        let g = [1.0, 3.0, 2.0, 4.0];
        let mut e = [0.0; 4];
        small_strain(&g, 2, &mut e);
        assert_eq!(e, [1.0, 2.5, 2.5, 4.0]);
        green_strain(&g, 2, &mut e);
        // gᵀg = [[10, 14], [14, 20]]
        assert_eq!(e, [(10.0 + 1.0 + 1.0) / 2.0, 9.5, 9.5, (20.0 + 8.0) / 2.0]);
    }

    #[test]
    fn von_mises_of_uniaxial_stress() {
        let s = [2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert!((von_mises(&s, 3) - 2.0).abs() < 1e-15);
    }

    #[test]
    fn padders_fill_out_of_plane_components() {
        let s = [1.0, 0.5, 0.5, 3.0];
        let p = pad_stress_2d(&s, 0.25, false);
        assert_eq!(p, vec![1.0, 0.5, 0.0, 0.5, 3.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(pad_stress_2d(&s, 0.25, true)[8], 0.0);
        let e = pad_strain_2d(&s, 0.25, true);
        assert_eq!(e[8], 0.25 / (0.25 - 1.0) * 4.0);
        assert_eq!(pad_strain_2d(&s, 0.25, false)[8], 0.0);
    }

    #[test]
    fn homogenizer_averages_blocks_in_order() {
        assert_eq!(
            homogenize(&[1.0, 2.0, 3.0, 5.0], 2).unwrap(),
            vec![2.0, 3.5]
        );
        assert!(homogenize(&[1.0, 2.0, 3.0], 2).is_err());
        // (0 + −0)/1 = +0 as upstream (sum starts from +0)
        assert!(homogenize(&[-0.0], 1).unwrap()[0].is_sign_positive());
    }
}
