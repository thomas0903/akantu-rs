// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support of
// the spatial convergence studies (no upstream counterpart): the isotropic linear
// elastic law of src/model/solid_mechanics/materials/material_elastic.cc
// (plane strain, plane stress of PlaneStressToolbox), a manufactured smooth
// displacement field with its body force, and the L2 / energy errors of a
// displacement. Needs the generic harness at the crate root of the test
// binary as `convergence_common`
// (crates/akantu-fe/tests/convergence_common/mod.rs).

#![allow(dead_code)] // the binaries use most, not every helper in every profile

use std::f64::consts::PI;

use akantu_core::Real;

use crate::convergence_common::{RefData, StructuredMesh, field_errors, field_load};

/// Isotropic linear elastic constants of the law in `dim` dimensions:
/// `σ = λ tr(ε) I + 2 μ ε` (`dim ≥ 2`; plane stress uses upstream's
/// `λ = ν E / ((1 + ν)(1 − ν))`), `σ = E ε` in 1-D.
#[derive(Clone, Copy, Debug)]
pub struct Elastic {
    pub dim: usize,
    pub e: Real,
    pub nu: Real,
    pub rho: Real,
    pub plane_stress: bool,
}

impl Elastic {
    pub fn lambda(&self) -> Real {
        let (e, nu) = (self.e, self.nu);
        if self.dim == 2 && self.plane_stress {
            nu * e / ((1. + nu) * (1. - nu))
        } else {
            nu * e / ((1. + nu) * (1. - 2. * nu))
        }
    }
    pub fn mu(&self) -> Real {
        self.e / (2. * (1. + self.nu))
    }
    /// Material file of the law.
    pub fn dat(&self) -> String {
        let ps = if self.dim == 2 && self.plane_stress {
            "  Plane_Stress = true\n"
        } else {
            ""
        };
        format!(
            "material elastic [\n  name = conv\n  rho = {}\n  E = {}\n  nu = {}\n{ps}]\n",
            self.rho, self.e, self.nu
        )
    }
    /// Stress of the displacement gradient `g` (row-major `3 × 3`).
    pub fn stress(&self, g: &[[Real; 3]; 3]) -> [[Real; 3]; 3] {
        let d = self.dim;
        let mut s = [[0.0; 3]; 3];
        if d == 1 {
            s[0][0] = self.e * g[0][0];
            return s;
        }
        let (l, m) = (self.lambda(), self.mu());
        let tr: Real = (0..d).map(|i| g[i][i]).sum();
        for i in 0..d {
            for j in 0..d {
                s[i][j] = m * (g[i][j] + g[j][i]);
            }
            s[i][i] += l * tr;
        }
        s
    }
    /// Energy density `ε : C : ε` of the strain `sym(g)` (twice the stored
    /// energy).
    pub fn energy(&self, g: &[[Real; 3]; 3]) -> Real {
        let d = self.dim;
        let s = self.stress(g);
        let mut w = 0.0;
        for i in 0..d {
            for j in 0..d {
                w += s[i][j] * 0.5 * (g[i][j] + g[j][i]);
            }
        }
        w
    }
}

/// `u_i(x) = A_i Π_j sin(k_ij x_j + c_ij)`: smooth, not polynomial,
/// non-zero on the boundary, with divergence and shear in every dimension.
#[derive(Clone, Copy, Debug)]
pub struct Manufactured {
    pub dim: usize,
    pub law: Elastic,
}

const AMP: [Real; 3] = [0.010, 0.008, 0.006];
const K: [[Real; 3]; 3] = [[1.1, 0.7, 0.9], [0.8, 1.2, 0.6], [0.9, 0.6, 1.3]];
const PHASE: [[Real; 3]; 3] = [[0.3, 0.5, 0.2], [0.6, 0.1, 0.4], [0.2, 0.7, 0.5]];

impl Manufactured {
    /// `∂^o u_i` with `o[j]` derivatives along `x_j`.
    pub fn deriv(&self, i: usize, o: [u32; 3], x: &[Real]) -> Real {
        let mut v = AMP[i];
        for j in 0..self.dim {
            let k = K[i][j] * PI;
            v *= k.powi(o[j] as i32) * (k * x[j] + PHASE[i][j] + o[j] as Real * PI / 2.0).sin();
        }
        v
    }
    pub fn u(&self, x: &[Real]) -> [Real; 3] {
        let mut u = [0.0; 3];
        for (i, ui) in u.iter_mut().enumerate().take(self.dim) {
            *ui = self.deriv(i, [0; 3], x);
        }
        u
    }
    /// `g[i][j] = ∂u_i/∂x_j`.
    pub fn grad(&self, x: &[Real]) -> [[Real; 3]; 3] {
        let mut g = [[0.0; 3]; 3];
        for (i, gi) in g.iter_mut().enumerate().take(self.dim) {
            for (j, gij) in gi.iter_mut().enumerate().take(self.dim) {
                let mut o = [0; 3];
                o[j] = 1;
                *gij = self.deriv(i, o, x);
            }
        }
        g
    }
    /// `b = −div σ(u)`: `−E u''` in 1-D, else
    /// `−[(λ + μ) ∇(div u) + μ Δu]`.
    pub fn body_force(&self, x: &[Real]) -> [Real; 3] {
        let d = self.dim;
        let mut b = [0.0; 3];
        if d == 1 {
            b[0] = -self.law.e * self.deriv(0, [2, 0, 0], x);
            return b;
        }
        let (l, m) = (self.law.lambda(), self.law.mu());
        for (i, bi) in b.iter_mut().enumerate().take(d) {
            let mut grad_div = 0.0;
            for k in 0..d {
                let mut o = [0; 3];
                o[i] += 1;
                o[k] += 1;
                grad_div += self.deriv(k, o, x);
            }
            let mut lap = 0.0;
            for j in 0..d {
                let mut o = [0; 3];
                o[j] = 2;
                lap += self.deriv(i, o, x);
            }
            *bi = -((l + m) * grad_div + m * lap);
        }
        b
    }
}

/* ---------------------------------------------------------------------- */
/* Error norms and loads of a displacement                                 */
/* ---------------------------------------------------------------------- */

/// An exact displacement: `x ↦ (u(x), ∇u(x))` (`g[i][j] = ∂u_i/∂x_j`).
pub type ExactField<'a> = crate::convergence_common::ExactField<'a>;

/// Errors of the nodal displacement `u` (`dim` per node) against `exact`:
/// `(‖u − u_h‖_L2, ‖u − u_h‖_E, ‖u‖_L2, ‖u‖_E)` with
/// `‖v‖_E² = ∫ ε(v) : C : ε(v)`, integrated with the test's own rule.
pub fn errors(
    sm: &StructuredMesh,
    rd: &RefData,
    u: &[Real],
    exact: &ExactField<'_>,
    law: &Elastic,
) -> [Real; 4] {
    field_errors(sm, rd, u, sm.dim, exact, &|_, _, g| law.energy(g))
}

/// Consistent nodal load `F_a = ∫ N_a b dx` of the body force `b`
/// (the test's own rule, library shape functions).
pub fn nodal_load(
    sm: &StructuredMesh,
    rd: &RefData,
    b: &dyn Fn(&[Real]) -> [Real; 3],
) -> Vec<Real> {
    field_load(sm, rd, sm.dim, b)
}
