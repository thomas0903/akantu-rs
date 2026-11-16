// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/material.cc
// (assembleInternalForcesFiniteDeformation, assembleStiffnessMatrixNL,
// assembleStiffnessMatrixL2), material_inline_impl.hh (stressToVoigt,
// setCauchyStressMatrix), src/common/aka_voigthelper_tmpl.hh
// (transferBMatrixToSymVoigtBMatrix, transferBMatrixToBL2,
// transferBMatrixToBNL; ported in akantu_core::voigt).

//! Total-Lagrangian finite-deformation assembly (SMM-023, Bathe, Ramm &
//! Wilson, IJNME 9, 353–386, 1975).
//!
//! With `finite_deformation = true` the law computes the second
//! Piola–Kirchhoff stress `S` (into `piola_kirchhoff_2`) from the Green
//! strain of `grad_u`; the model then assembles, per quadrature point,
//!
//! * the residual `r = (B_L0 + B_L1(∇u))ᵀ · S_voigt`
//!   (`assembleInternalForcesFiniteDeformation`),
//! * the geometric stiffness `K_NL = B_NLᵀ · Ŝ · B_NL` with the Bathe
//!   stress matrix `Ŝ = diag(S, …, S)` (`assembleStiffnessMatrixNL`),
//! * the material stiffness `K_L = (B_L0 + B_L1)ᵀ · D · (B_L0 + B_L1)`
//!   (`assembleStiffnessMatrixL2`),
//!
//! integrated with the element quadrature. Every upstream operand is a
//! compile-time-sized Eigen matrix (`Matrix<Real, stress_size, dim·nn>`,
//! `Matrix<Real, dim², dim²>`, maps of compile-time-sized tuples), so every
//! product is described with [`Dims::fixed`] and evaluated by
//! `akantu_core::eigen_gebp` (D-008, D-036): the nested products
//! `Bᵀ · X · B` evaluate `Bᵀ · X` into a compile-time-sized column-major
//! temporary first, exactly as Eigen's `nested_eval` of the inner product.
//! The B-matrix transfers accumulate exactly as upstream
//! (`akantu_core::voigt`), `B += B2` is a coefficient-wise sum.

use akantu_core::array_shared::ArrayRef;
use akantu_core::tensor::MatrixX;
use akantu_core::voigt::{self, VoigtTable};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_fe::eigen::{Dims, product_slices};
use akantu_model::constitutive_law::LawContext;

use crate::material::Material;

mod fast;

/// Per-type sizes of the total-Lagrangian kernels.
struct Sizes {
    dim: usize,
    nn: usize,
    nq: usize,
    /// `getTangentStiffnessVoigtSize(dim)` (= Voigt size).
    ts: usize,
    /// `dim · nn`.
    bt: usize,
}

impl Sizes {
    fn of(fe: &FeEngine, dim: usize, ty: ElementType, ghost: GhostType) -> Result<Self> {
        let nn = ty.nb_nodes_per_element();
        let nq = fe.nb_integration_points(ty, ghost)?;
        let ts = voigt::voigt_table(dim)?.size;
        Ok(Self {
            dim,
            nn,
            nq,
            ts,
            bt: dim * nn,
        })
    }
}

/// Upstream `FEEngine::filterElementalData` of the shape derivatives
/// (checked, not copied): the `dim × nn` column-major tuple of quadrature
/// point `q` of mesh element `e`.
struct Derivatives<'a> {
    data: &'a [Real],
    stride: usize,
    nq: usize,
    nb_elements: usize,
}

impl<'a> Derivatives<'a> {
    fn of(fe: &'a FeEngine, s: &Sizes, ty: ElementType, ghost: GhostType) -> Result<Self> {
        let d = fe.shapes_derivatives(ty, ghost)?;
        if d.nb_component() != s.bt {
            return Err(Error::DimensionMismatch {
                context: "finite deformation: shape derivatives (element dimension must equal the material dimension)",
                expected: s.bt,
                got: d.nb_component(),
            });
        }
        Ok(Self {
            data: d.as_slice(),
            stride: s.bt,
            nq: s.nq,
            nb_elements: d.len().checked_div(s.nq).unwrap_or(0),
        })
    }

    fn check_filter(&self, filter: &[Idx]) -> Result<()> {
        for &e in filter {
            let ok = usize::try_from(e).is_ok_and(|e| e < self.nb_elements);
            if !ok {
                return Err(Error::InvalidIndex {
                    what: "finite deformation: element of the material filter",
                    value: i64::from(e),
                });
            }
        }
        Ok(())
    }

    /// Tuple of quadrature point `q` of element `e` (filter checked).
    fn at(&self, e: Idx, q: usize) -> &'a [Real] {
        let e = usize::try_from(e).unwrap_or(0);
        let t = e * self.nq + q;
        &self.data[t * self.stride..(t + 1) * self.stride]
    }
}

/// Check that a per-quadrature-point field of the material has one
/// `nb_component`-tuple per quadrature point of the filter.
fn check_field(
    context: &'static str,
    a: &Array<Real>,
    nb_points: usize,
    nb_component: usize,
) -> Result<()> {
    if a.nb_component() != nb_component {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_component,
            got: a.nb_component(),
        });
    }
    if a.len() != nb_points {
        return Err(Error::DimensionMismatch {
            context,
            expected: nb_points,
            got: a.len(),
        });
    }
    Ok(())
}

/// The `piola_kirchhoff_2` internal of `(ty, ghost)`.
fn pk2_of(law: &dyn Material, ty: ElementType, ghost: GhostType) -> Result<ArrayRef<'_, Real>> {
    law.base()
        .piola_kirchhoff_2
        .as_ref()
        .ok_or_else(|| {
            Error::invalid(
                "finite_deformation: piola_kirchhoff_2 is not registered (init the material first)",
            )
        })?
        .get(ty, ghost)
}

/// Upstream `Material::stressToVoigt<dim>` (`VoigtHelper::matrixToVoigt`):
/// `v[I] = S(i, j)` for `I < size` (`S` column-major `dim × dim`).
fn stress_to_voigt(t: &VoigtTable, s: &[Real], out: &mut [Real]) {
    for (big_i, v) in out.iter_mut().enumerate().take(t.size) {
        let (i, j) = t.vec(big_i);
        *v = s[i + j * t.dim];
    }
}

/// `B ← transferBMatrixToSymVoigtBMatrix(dN/dX)`,
/// `B2 ← transferBMatrixToBL2(dN/dX, ∇u)`, `B += B2` (upstream statement
/// order of `assembleInternalForcesFiniteDeformation` and
/// `assembleStiffnessMatrixL2`).
fn b_l0_plus_l1(s: &Sizes, dndx: &[Real], grad_u: &[Real], scratch: &mut Scratch) -> Result<()> {
    scratch.dndx.as_mut_slice().copy_from_slice(dndx);
    scratch.grad_u.as_mut_slice().copy_from_slice(grad_u);
    voigt::b_matrix_to_sym_voigt(s.dim, &scratch.dndx, &mut scratch.b, s.nn)?;
    voigt::b_matrix_to_bl2(s.dim, &scratch.dndx, &scratch.grad_u, &mut scratch.b2, s.nn)?;
    for (b, b2) in scratch
        .b
        .as_mut_slice()
        .iter_mut()
        .zip(scratch.b2.as_slice())
    {
        *b += *b2;
    }
    Ok(())
}

/// Per-quadrature-point work buffers (allocated once per element type).
struct Scratch {
    dndx: MatrixX,
    grad_u: MatrixX,
    /// `stress_size × dim·nn` (B_L0, then B_L0 + B_L1) or `dim² × dim·nn`
    /// (B_NL).
    b: MatrixX,
    b2: MatrixX,
}

impl Scratch {
    fn new(s: &Sizes, b_rows: usize) -> Self {
        Self {
            dndx: MatrixX::zeros(s.dim, s.nn),
            grad_u: MatrixX::zeros(s.dim, s.dim),
            b: MatrixX::zeros(b_rows, s.bt),
            b2: MatrixX::zeros(s.ts, s.bt),
        }
    }
}

/// Upstream `Material::assembleInternalForcesFiniteDeformation<dim, type>`
/// up to (excluding) the DOF-manager assembly: per quadrature point of the
/// filter `r = (B_L0 + B_L1(∇u))ᵀ · stressToVoigt(S)` with the material's
/// stored `grad_u` (`∇u − eigen_grad_u` of the last `computeAllStresses`)
/// and `piola_kirchhoff_2`, then `∫ r` per element (`dim·nn` components)
/// into `r_e`; `bt_s` receives the per-point vectors (both buffers are
/// resized and fully overwritten).
#[allow(clippy::too_many_arguments)] // upstream arguments + the two reused buffers
pub(super) fn internal_forces(
    law: &dyn Material,
    fe: &FeEngine,
    dim: usize,
    ty: ElementType,
    ghost: GhostType,
    filter: &[Idx],
    bt_s: &mut Array<Real>,
    r_e: &mut Array<Real>,
) -> Result<()> {
    let s = Sizes::of(fe, dim, ty, ghost)?;
    let table = voigt::voigt_table(dim)?;
    let dsh = Derivatives::of(fe, &s, ty, ghost)?;
    dsh.check_filter(filter)?;
    let nb_points = filter.len() * s.nq;
    let grad_u = law.base().grad_u.get(ty, ghost)?;
    check_field(
        "assembleInternalForcesFiniteDeformation: grad_u",
        &grad_u,
        nb_points,
        dim * dim,
    )?;
    let pk2 = pk2_of(law, ty, ghost)?;
    check_field(
        "assembleInternalForcesFiniteDeformation: piola_kirchhoff_2",
        &pk2,
        nb_points,
        dim * dim,
    )?;
    // upstream: a new `nb_points × bt` array per call; here a reused one,
    // resized (entries all written below)
    if bt_s.nb_component() != s.bt {
        *bt_s = Array::new(s.bt);
    }
    bt_s.try_resize(nb_points, 0.0)?;
    let (gs, ps, out) = (grad_u.as_slice(), pk2.as_slice(), bt_s.as_mut_slice());
    // The same statements on stack arrays (bitwise the same)
    if fast::residuals(fe.exec(), &s, table, &dsh, filter, gs, ps, out).is_none() {
        let mut scratch = Scratch::new(&s, s.ts);
        let mut s_voigt = vec![0.0; s.ts];
        let d2 = dim * dim;
        for (pos, &e) in filter.iter().enumerate() {
            for q in 0..s.nq {
                let t = pos * s.nq + q;
                b_l0_plus_l1(&s, dsh.at(e, q), &gs[t * d2..(t + 1) * d2], &mut scratch)?;
                stress_to_voigt(table, &ps[t * d2..(t + 1) * d2], &mut s_voigt);
                // r = B_tensor.transpose() * S_voight
                product_slices(
                    Dims::fixed(s.ts, s.bt).t(),
                    Dims::fixed(s.ts, 1),
                    scratch.b.as_slice(),
                    &s_voigt,
                    &mut out[t * s.bt..(t + 1) * s.bt],
                )?;
            }
        }
    }
    fe.integrate(bt_s.view(), r_e, s.bt, ty, ghost, Some(filter))
}

/// `Bᵀ · X · B` of one quadrature point (`B`: `rows × bt`, `X`:
/// `rows × rows`, both compile-time sized upstream) into `out`
/// (`bt × bt`, column-major) through the compile-time-sized column-major
/// temporary `tmp` (`bt × rows`) of the inner product.
fn bt_x_b(
    rows: usize,
    bt: usize,
    b: &[Real],
    x: &[Real],
    tmp: &mut [Real],
    out: &mut [Real],
) -> Result<()> {
    product_slices(
        Dims::fixed(rows, bt).t(),
        Dims::fixed(rows, rows),
        b,
        x,
        tmp,
    )?;
    product_slices(Dims::fixed(bt, rows), Dims::fixed(rows, bt), tmp, b, out)?;
    Ok(())
}

/// Elemental geometric (`NL`) and material (`L2`) stiffness parts of one
/// element type (upstream `assembleStiffnessMatrixFiniteDeformation`:
/// `assembleStiffnessMatrixNL` then `assembleStiffnessMatrixL2`, each
/// integrated and assembled on its own; returned in that order).
///
/// * `NL`: `B_NLᵀ · σ̂(S) · B_NL` with the stored `piola_kirchhoff_2`.
/// * `L2`: `grad_u ← ∇u` (stored in the material, as upstream), minus
///   `eigen_grad_u` when `subtract_eigen` (the default mode; upstream does
///   not subtract it, UD-208 — reproduced with `subtract_eigen = false`),
///   zeroed tangent, `computeTangentModuli`, then
///   `(B_L0 + B_L1)ᵀ · D · (B_L0 + B_L1)`.
pub(super) fn stiffness(
    law: &mut dyn Material,
    ctx: &LawContext<'_>,
    ty: ElementType,
    ghost: GhostType,
    filter: &[Idx],
    subtract_eigen: bool,
) -> Result<(Array<Real>, Array<Real>)> {
    let dim = law.base().spatial_dimension;
    let fe = ctx.services.fe_engine("")?;
    let s = Sizes::of(fe, dim, ty, ghost)?;
    let dsh = Derivatives::of(fe, &s, ty, ghost)?;
    dsh.check_filter(filter)?;
    let nb_points = filter.len() * s.nq;
    let d2 = dim * dim;
    let size = s.bt * s.bt;

    // --- assembleStiffnessMatrixNL ---
    let k_nl = {
        let pk2 = pk2_of(law, ty, ghost)?;
        check_field(
            "assembleStiffnessMatrixNL: piola_kirchhoff_2",
            &pk2,
            nb_points,
            d2,
        )?;
        let mut bt_s_b = Array::try_zeros(nb_points, size)?;
        let mut scratch = Scratch::new(&s, d2);
        let mut sigma = vec![0.0; d2 * d2];
        let mut tmp = vec![0.0; s.bt * d2];
        let (ps, out) = (pk2.as_slice(), bt_s_b.as_mut_slice());
        for (pos, &e) in filter.iter().enumerate() {
            for q in 0..s.nq {
                let t = pos * s.nq + q;
                crate::material_tensor::set_cauchy_stress_matrix(
                    dim,
                    &ps[t * d2..(t + 1) * d2],
                    &mut sigma,
                )?;
                scratch.dndx.as_mut_slice().copy_from_slice(dsh.at(e, q));
                voigt::b_matrix_to_bnl(dim, &scratch.dndx, &mut scratch.b, s.nn)?;
                bt_x_b(
                    d2,
                    s.bt,
                    scratch.b.as_slice(),
                    &sigma,
                    &mut tmp,
                    &mut out[t * size..(t + 1) * size],
                )?;
            }
        }
        let mut k_e = Array::new(size);
        fe.integrate(bt_s_b.view(), &mut k_e, size, ty, ghost, Some(filter))?;
        k_e
    };

    // --- assembleStiffnessMatrixL2 ---
    let mut gu = Array::new(d2);
    ctx.services
        .gradient_on_integration_points("displacement", &mut gu, dim, ty, ghost, filter)?;
    {
        let base = law.base_mut();
        if subtract_eigen {
            let eigen = base.eigen_grad_u.get(ty, ghost)?;
            if eigen.len() != gu.len() || eigen.nb_component() != gu.nb_component() {
                return Err(Error::DimensionMismatch {
                    context: "assembleStiffnessMatrixL2: grad_u -= eigen_grad_u",
                    expected: eigen.len(),
                    got: gu.len(),
                });
            }
            for (g, e) in gu.as_mut_slice().iter_mut().zip(eigen.as_slice()) {
                *g -= *e;
            }
        }
        base.grad_u.get_mut(ty, ghost)?.copy_from(&gu)?;
    }
    check_field("assembleStiffnessMatrixL2: grad_u", &gu, nb_points, d2)?;
    let mut tangent = Array::try_zeros(nb_points, s.ts * s.ts)?;
    law.compute_tangent_moduli(ctx, ty, ghost, &mut tangent)?;
    let mut bt_d_b = Array::try_zeros(nb_points, size)?;
    let mut scratch = Scratch::new(&s, s.ts);
    let mut tmp = vec![0.0; s.bt * s.ts];
    let ts2 = s.ts * s.ts;
    let (gs, ds, out) = (gu.as_slice(), tangent.as_slice(), bt_d_b.as_mut_slice());
    for (pos, &e) in filter.iter().enumerate() {
        for q in 0..s.nq {
            let t = pos * s.nq + q;
            b_l0_plus_l1(&s, dsh.at(e, q), &gs[t * d2..(t + 1) * d2], &mut scratch)?;
            bt_x_b(
                s.ts,
                s.bt,
                scratch.b.as_slice(),
                &ds[t * ts2..(t + 1) * ts2],
                &mut tmp,
                &mut out[t * size..(t + 1) * size],
            )?;
        }
    }
    let mut k_l2 = Array::new(size);
    fe.integrate(bt_d_b.view(), &mut k_l2, size, ty, ghost, Some(filter))?;
    Ok((k_nl, k_l2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cauchy_stress_matrix_is_block_diagonal() {
        // S column-major 2 × 2: S(0,0)=1, S(1,0)=2, S(0,1)=3, S(1,1)=4
        let s = [1.0, 2.0, 3.0, 4.0];
        let mut out = [9.0; 16];
        crate::material_tensor::set_cauchy_stress_matrix(2, &s, &mut out).unwrap();
        let at = |r: usize, c: usize| out[r + c * 4];
        for i in 0..2 {
            for m in 0..2 {
                for n in 0..2 {
                    assert_eq!(at(i * 2 + m, i * 2 + n), s[m + n * 2]);
                }
            }
        }
        assert_eq!(at(0, 2), 0.0);
        assert_eq!(at(3, 1), 0.0);
    }

    #[test]
    fn stress_to_voigt_order() {
        // 3-D: (xx, yy, zz, yz, xz, xy)
        let t = voigt::voigt_table(3).unwrap();
        let mut s = [0.0; 9];
        for j in 0..3 {
            for i in 0..3 {
                s[i + j * 3] = (10 * i + j) as Real;
            }
        }
        let mut v = [0.0; 6];
        stress_to_voigt(t, &s, &mut v);
        assert_eq!(v, [0.0, 11.0, 22.0, 12.0, 2.0, 1.0]);
    }
}
