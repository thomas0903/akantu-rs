// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_elastic_linear_anisotropic.hh, material_elastic_linear_anisotropic.cc,
// material_elastic_linear_anisotropic_inline_impl.hh.

//! `elastic_anisotropic`: general linear elasticity (upstream
//! `MaterialElasticLinearAnisotropic<dim>`), and the anisotropic machinery
//! shared with `elastic_orthotropic` ([`AnisotropicCore`]).
//!
//! The stiffness is given in the material frame by the upper triangle
//! `Cij` (`1 ≤ i ≤ j ≤ voigt`) of the Voigt matrix `C'` and the material
//! axes `n1..n<dim>`, normalised. In 3-D they must form a right-handed
//! orthonormal base (`‖n1 × n2 − n3‖ ≤ 8ε`, an error otherwise, as
//! upstream). In 2-D upstream only checks that the normalised axes are not
//! parallel and are orthogonal (`‖v × w − (v × w)/‖v × w‖‖ ≤ 8ε`, i.e.
//! `|n1 × n2| = 1`): left-handed 2-D axes (reflections, which still define a
//! valid transformation of the stiffness) are accepted (UD-107, category
//! C, kept).
//! `updateInternalParameters` symmetrises `C'`, extends it to the `dim² ×
//! dim²` "extended Voigt" matrix (duplicated shear rows/columns), rotates it
//! (`Cfull = rotator · C' · revrotor`, `I = mat[i][j]`, `J = mat[k][l]`,
//! `R` the matrix whose columns are the normalised axes), keeps the leading
//! `voigt × voigt` block `C` and computes its eigenvalues (and the cached
//! Kelvin bound below).
//!
//! # Orientation of the material axes (UD-104)
//!
//! Upstream builds `rotator(I, J) = R(k, i)·R(l, j)`,
//! `revrotor(I, J) = R(i, k)·R(j, l)`, i.e. `C_ijkl = R_ai R_bj R_ck R_dl
//! C'_abcd`: it rotates with `Rᵀ`, so the stiffness given for material axis
//! 1 acts along the first **row** of `R` instead of `n1`
//! (category B). By default
//! ([`LawOptions::material_axes_as_directions`]) the port uses `R` itself,
//! `C_ijkl = R_ia R_jb R_kc R_ld C'_abcd`, so `n1..` are the directions of
//! the material axes; the upstream orientation is kept for differential
//! tests.
//!
//! Per quadrature point: `σ_voigt = C · [ε11, ε22, (ε33,) 2ε23, 2ε13, 2ε12]`,
//! `σ = voigtToMatrix(σ_voigt)`; tangent `C`; energy `(σ : ∇u)/2`.
//! Products follow Eigen's strict-build routes: the 3-D rotation
//! products are GEMMs (gebp, depth 9), the 2-D ones coefficient-based, the
//! stress product coefficient-based over the compile-time Voigt size.
//!
//! # Celerity (UD-033)
//!
//! Upstream returns `√(eig(C)[0]/ρ)` with the **unsorted** first eigenvalue
//! of Eigen's real Schur solver ([`CelerityMode::UpstreamFirstEigenvalue`]),
//! which depends on Eigen's internal ordering. The default (UD-033,
//! category B: an acoustic-tensor maximum or a proved Kelvin bound, never
//! the plain Voigt maximum) is the **proved conservative
//! bound** `√(λ_max(C_K)/ρ)` with the Kelvin/Mandel matrix `C_K = D·C·D`,
//! `D = diag(1 (normal), √2 (shear))` ([`CelerityMode::KelvinBound`]): for
//! unit `a`, `n`, `(a⊙n) : C : (a⊙n) ≤ λ_max(C_K)·|a⊙n|² ≤ λ_max(C_K)`, so
//! it bounds every bulk wave speed from above (it is not the exact maximum:
//! for an isotropic solid `λ_max(C_K) = max(3λ + 2μ, 2μ)`, which is
//! `3λ + 2μ` for `λ ≥ 0`, giving `√((3λ + 2μ)/ρ) ≥ √((λ + 2μ)/ρ)`). The
//! bound assumes a symmetric positive-definite `C` and `ρ > 0`. The upstream
//! value is available for differential tests.
//!
//! Both eigenvalue sets depend only on `C`, which changes only in
//! `updateInternalParameters`: they are computed there and cached
//! (as upstream caches `eigC`); a celerity query divides the
//! cached eigenvalue by the current density and takes the square root, with
//! no allocation or eigen-solve per element.

use std::sync::Arc;

use akantu_core::eigen_gebp::{Extent, ProductShape, product_into};
use akantu_core::enums::MatrixType;
use akantu_core::tensor::{Matrix, MatrixExt, MatrixX, as_matrix, as_matrix_mut};
use akantu_core::voigt::voigt_table;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Real, Result};
use akantu_linalg::dense::{Storage, eig, eigh};
use akantu_model::constitutive_law::{ConstitutiveLaw, LawContext, LawSpec, ModelServices};
use akantu_parser::param::ParamDescriptor;

use crate::material::{
    LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams, check_tangent,
};
use crate::material_tensor::{grad_u_to_epsilon, tangent_stiffness_voigt_size};

/// Wave speed reported by [`Material::celerity`] for anisotropic laws
/// (UD-033).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CelerityMode {
    /// `√(λ_max(D·C·D)/ρ)`, the Kelvin/Mandel conservative upper bound of the
    /// bulk wave speeds (default, UD-033 fix).
    #[default]
    KelvinBound,
    /// Upstream: `√(eig(C)[0]/ρ)`, first eigenvalue of Eigen's unsorted real
    /// Schur order (compatibility).
    UpstreamFirstEigenvalue,
}

/// `Cfull = (rotator · C') · revrotor` with the upstream operand types
/// (`material_elastic_linear_anisotropic.cc:147-149`): `rotator` and
/// `revrotor` are `Matrix<Real, D², D²>` (column-major slices here), `C'` is
/// a dynamic `Matrix<Real>`, so the temporary `rotator · C'` is
/// `Matrix<Real, D², Dynamic>`. Pinned to the oracle `anisorot` records by
/// `tests::products_match_the_oracle_anisotropic_records`.
pub(crate) fn rotate_full(
    d2: usize,
    rotator: &[Real],
    c_prime: &impl Fn(usize, usize) -> Real,
    revrotor: &[Real],
    cfull: &mut [Real],
) -> Result<()> {
    let mut p = vec![0.0; d2 * d2];
    product_into(
        &ProductShape::new(
            Extent::Fixed(d2),
            Extent::Fixed(d2),
            Extent::Dynamic(d2),
            Extent::Dynamic(d2),
        ),
        &|i, k| rotator[i + k * d2],
        c_prime,
        &mut p,
    )?;
    product_into(
        &ProductShape::new(
            Extent::Fixed(d2),
            Extent::Dynamic(d2),
            Extent::Fixed(d2),
            Extent::Fixed(d2),
        ),
        &|i, k| p[i + k * d2],
        &|k, j| revrotor[k + j * d2],
        cfull,
    )
}

/// `voigt_stress = C · voigt_strain` with the upstream operand types
/// (`material_elastic_linear_anisotropic_inline_impl.hh:38-40`): `C` dynamic
/// `Matrix<Real>`, the strain a `Vector<Real, voigt>` — coefficient-based
/// route, sequential reduction (dynamic left operand). Pinned to the oracle
/// `aniso` records.
pub(crate) fn voigt_stress_product(
    v: usize,
    c: &impl Fn(usize, usize) -> Real,
    strain: &[Real],
    stress: &mut [Real],
) -> Result<()> {
    if strain.len() < v {
        return Err(Error::DimensionMismatch {
            context: "anisotropic stress: Voigt strain",
            expected: v,
            got: strain.len(),
        });
    }
    product_into(
        &ProductShape::new(
            Extent::Dynamic(v),
            Extent::Dynamic(v),
            Extent::Fixed(v),
            Extent::Fixed(1),
        ),
        c,
        &|k, _| strain[k],
        stress,
    )
}

/// [`voigt_stress_product`] for the constant Voigt size `V` on the
/// column-major `V × V` slice `c`: per row the sequential reduction from
/// the first term, `r = c(i, 0)·ε_0; r += c(i, k)·ε_k`.
#[inline(always)]
pub(crate) fn voigt_stress_seq<const V: usize>(
    c: &[Real],
    strain: &[Real; 6],
    stress: &mut [Real; 6],
) {
    let c = &c[..V * V];
    for (i, out) in stress.iter_mut().enumerate().take(V) {
        let mut r = c[i] * strain[0];
        for k in 1..V {
            r += c[i + k * V] * strain[k];
        }
        *out = r;
    }
}

/// 3-D cross product as Eigen's `cross` evaluates it.
fn cross(a: &[Real; 3], b: &[Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Eigen `norm()` of a fixed 3-vector (halving-tree squared norm).
fn norm3(v: &[Real; 3]) -> Real {
    let m: Matrix<3, 1> = [*v];
    m.norm()
}

/// Eigen `normalize()` of a fixed 3-vector.
fn normalize3(v: &mut [Real; 3]) {
    let m: Matrix<3, 1> = [*v];
    let z = m.squared_norm();
    if z > 0.0 {
        let s = z.sqrt();
        for x in v.iter_mut() {
            *x /= s;
        }
    }
}

/// Anisotropic state shared by `elastic_anisotropic` and
/// `elastic_orthotropic` (upstream members of
/// `MaterialElasticLinearAnisotropic<dim>`).
#[derive(Clone, Debug)]
pub struct AnisotropicCore<const DIM: usize> {
    /// Material axes `n1..n<DIM>` (parameters, unit axes by default).
    pub dir_vecs: [[Real; DIM]; DIM],
    /// Rotation: column `j` is the normalised axis `n_{j+1}`.
    pub rot_mat: MatrixX,
    /// Stiffness in the material frame, extended Voigt (`DIM² × DIM²`).
    pub c_prime: MatrixX,
    /// Stiffness in the global frame, Voigt (`voigt × voigt`).
    pub c: MatrixX,
    /// Eigenvalues of `C`, Eigen's unsorted real Schur order.
    pub eig_c: Vec<Real>,
    /// Cached `λ_max(D·C·D)` (UD-033 Kelvin bound), refreshed with
    /// [`AnisotropicCore::compute_eigenvalues`]; `None` when the symmetric
    /// eigen-solver failed (non-finite stiffness).
    kelvin_lambda_max: Option<Real>,
    pub(crate) was_stiffness_assembled: bool,
}

impl<const DIM: usize> Default for AnisotropicCore<DIM> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const DIM: usize> AnisotropicCore<DIM> {
    /// Voigt size.
    pub const VOIGT: usize = DIM * (DIM + 1) / 2;

    /// Unit axes, zero stiffness (upstream constructor; the registered
    /// `Cij` default to 0).
    #[must_use]
    pub fn new() -> Self {
        let v = Self::VOIGT;
        Self {
            dir_vecs: std::array::from_fn(|i| {
                std::array::from_fn(|k| if i == k { 1.0 } else { 0.0 })
            }),
            rot_mat: MatrixX::zeros(DIM, DIM),
            c_prime: MatrixX::zeros(DIM * DIM, DIM * DIM),
            c: MatrixX::zeros(v, v),
            eig_c: vec![0.0; v],
            kelvin_lambda_max: Some(0.0),
            was_stiffness_assembled: false,
        }
    }

    /// Lower triangle of the Voigt block of `C'` from its upper triangle
    /// (upstream `updateInternalParameters` with `symmetric = true`).
    pub fn symmetrize(&mut self) {
        let v = Self::VOIGT;
        for i in 0..v {
            for j in i + 1..v {
                *self.c_prime.at_mut(j, i) = self.c_prime.at(i, j);
            }
        }
    }

    /// Upstream `rotateCprime()`; `axes_as_directions` rotates with `R`
    /// instead of upstream's `Rᵀ` ([`LawOptions::material_axes_as_directions`],
    /// UD-104).
    ///
    /// # Errors
    /// 3-D axes that do not form a right-handed orthonormal base
    /// (`‖n1 × n2 − n3‖ > 8ε`); 2-D axes that are parallel or not
    /// orthogonal (`‖v1 × v2 − (v1 × v2)/‖v1 × v2‖‖ > 8ε`; the 2-D handedness
    /// is not checked, UD-107).
    pub fn rotate_c_prime(&mut self, axes_as_directions: bool) -> Result<()> {
        let d2 = DIM * DIM;
        let v = Self::VOIGT;
        let diff = d2 - v;
        for i in v..d2 {
            for j in 0..d2 {
                *self.c_prime.at_mut(i, j) = self.c_prime.at(i - diff, j);
            }
        }
        for i in 0..d2 {
            for j in v..d2 {
                *self.c_prime.at_mut(i, j) = self.c_prime.at(i, j - diff);
            }
        }
        // Columns of rot_mat: normalised axes (dynamic-size column:
        // sequential squared norm, division by its square root).
        for j in 0..DIM {
            for k in 0..DIM {
                *self.rot_mat.at_mut(k, j) = self.dir_vecs[j][k];
            }
            let mut z = self.rot_mat.at(0, j) * self.rot_mat.at(0, j);
            for k in 1..DIM {
                z += self.rot_mat.at(k, j) * self.rot_mat.at(k, j);
            }
            if z > 0.0 {
                let s = z.sqrt();
                for k in 0..DIM {
                    *self.rot_mat.at_mut(k, j) /= s;
                }
            }
        }
        let col3 = |m: &MatrixX, j: usize| -> [Real; 3] {
            std::array::from_fn(|k| if k < DIM { m.at(k, j) } else { 0.0 })
        };
        let eps8 = 8. * Real::EPSILON;
        let mut test_axis = 0.0;
        if DIM == 2 {
            let v1 = col3(&self.rot_mat, 0);
            let v2 = col3(&self.rot_mat, 1);
            let mut v3 = cross(&v1, &v2);
            if norm3(&v3) < eps8 {
                return Err(Error::invalid("The axis vectors parallel."));
            }
            normalize3(&mut v3);
            let c = cross(&v1, &v2);
            test_axis = norm3(&[c[0] - v3[0], c[1] - v3[1], c[2] - v3[2]]);
        } else if DIM == 3 {
            let v1 = col3(&self.rot_mat, 0);
            let v2 = col3(&self.rot_mat, 1);
            let v3 = col3(&self.rot_mat, 2);
            let c = cross(&v1, &v2);
            test_axis = norm3(&[c[0] - v3[0], c[1] - v3[1], c[2] - v3[2]]);
        }
        if test_axis > eps8 {
            return Err(Error::invalid(format!(
                "The axis vectors do not form a right-handed coordinate system. I. e., ||n1 x n2 - n3|| should be zero, but it is {test_axis}."
            )));
        }
        // rotator and reverse rotator (fixed DIM² × DIM²).
        let t = voigt_table(DIM)?;
        let mut rotator = vec![0.0; d2 * d2];
        let mut revrotor = vec![0.0; d2 * d2];
        for i in 0..DIM {
            for j in 0..DIM {
                for k in 0..DIM {
                    for l in 0..DIM {
                        let big_i = t.mat(i, j);
                        let big_j = t.mat(k, l);
                        let r = |a: usize, b: usize| {
                            if axes_as_directions {
                                self.rot_mat.at(b, a)
                            } else {
                                self.rot_mat.at(a, b)
                            }
                        };
                        rotator[big_i + big_j * d2] = r(k, i) * r(l, j);
                        revrotor[big_i + big_j * d2] = r(i, k) * r(j, l);
                    }
                }
            }
        }
        // Cfull = (rotator · C') · revrotor: rotator fixed (D² × D²), C'
        // dynamic, the temporary `rotator · C'` is `Matrix<Real, D², Dynamic>`
        // (dynamic columns: sequential lazy reductions in 2-D).
        let mut cfull = vec![0.0; d2 * d2];
        rotate_full(
            d2,
            &rotator,
            &|k, j| self.c_prime.at(k, j),
            &revrotor,
            &mut cfull,
        )?;
        for i in 0..v {
            for j in 0..v {
                *self.c.at_mut(i, j) = cfull[i + j * d2];
            }
        }
        Ok(())
    }

    /// `C.eig(eigC)` (dynamic storage, unsorted real parts), and the cached
    /// Kelvin maximum eigenvalue `λ_max(D·C·D)`. Upstream ignores
    /// Eigen's `ComputationInfo` (e.g. a NaN stiffness while parameters are
    /// set one by one, `ν32 = ν23·E3/E2` with `E2 = 0`): the update must not
    /// fail there, so a solver failure stores NaN eigenvalues (defined values
    /// instead of upstream's unspecified ones; an upstream celerity computed
    /// from them is NaN) and no Kelvin bound (the Kelvin celerity is then an
    /// error).
    ///
    /// Call it after every change of `C` (the laws' updates do): the
    /// celerity reads these cached values.
    pub fn compute_eigenvalues(&mut self) -> Result<()> {
        self.eig_c =
            eig(&self.c, Storage::Dynamic).unwrap_or_else(|_| vec![Real::NAN; Self::VOIGT]);
        self.kelvin_lambda_max = eigh(&self.kelvin_stiffness(), Storage::Dynamic)
            .ok()
            .map(|ev| ev.last().copied().unwrap_or(0.0));
        Ok(())
    }

    /// Upstream `computeStressOnQuad`.
    ///
    /// # Errors
    /// Never for consistent sizes (product helper contract).
    pub fn stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma: &mut Matrix<DIM, DIM>,
    ) -> Result<()> {
        let v = Self::VOIGT;
        let t = voigt_table(DIM)?;
        let eps = grad_u_to_epsilon(grad_u);
        let mut strain = [0.0; 6];
        for (big_i, s) in strain.iter_mut().enumerate().take(v) {
            let (i, j) = t.vec(big_i);
            *s = t.factor(big_i) * eps[j][i];
        }
        // `C` (Matrix<Real>, dynamic) · strain (Vector<Real, voigt>):
        // coefficient-based route, sequential reduction (dynamic lhs);
        // `voigt_stress_seq` evaluates that route for the law's Voigt size
        let mut stress = [0.0; 6];
        let c = self.c.as_slice();
        if c.len() == v * v {
            match v {
                1 => voigt_stress_seq::<1>(c, &strain, &mut stress),
                3 => voigt_stress_seq::<3>(c, &strain, &mut stress),
                6 => voigt_stress_seq::<6>(c, &strain, &mut stress),
                _ => voigt_stress_product(v, &|i, k| c[i + k * v], &strain[..v], &mut stress[..v])?,
            }
        } else {
            voigt_stress_product(v, &|i, k| self.c.at(i, k), &strain[..v], &mut stress[..v])?;
        }
        for (big_i, &s) in stress.iter().enumerate().take(v) {
            let (i, j) = t.vec(big_i);
            sigma[j][i] = s;
            sigma[i][j] = s;
        }
        Ok(())
    }

    /// Upstream `computeTangentModuliOnQuad` (`tangent = C`, column-major).
    pub fn tangent_on_quad(&self, out: &mut [Real]) {
        out.copy_from_slice(self.c.as_slice());
    }

    /// Upstream `computePotentialEnergyOnQuad`: `(σ : ∇u) / 2`.
    #[must_use]
    pub fn energy_on_quad(sigma: &Matrix<DIM, DIM>, grad_u: &Matrix<DIM, DIM>) -> Real {
        sigma.double_dot(grad_u) / 2.
    }

    /// Kelvin/Mandel stiffness `D·C·D`.
    #[must_use]
    pub fn kelvin_stiffness(&self) -> MatrixX {
        let v = Self::VOIGT;
        let w = |i: usize| {
            if i < DIM {
                1.0
            } else {
                std::f64::consts::SQRT_2
            }
        };
        let mut k = MatrixX::zeros(v, v);
        for j in 0..v {
            for i in 0..v {
                *k.at_mut(i, j) = w(i) * self.c.at(i, j) * w(j);
            }
        }
        k
    }

    /// Upstream celerity `√(eigC(0)/ρ)` (UD-033 compatibility).
    #[must_use]
    pub fn upstream_celerity(&self, rho: Real) -> Real {
        (self.eig_c[0] / rho).sqrt()
    }

    /// Cached `λ_max(D·C·D)` of the last [`AnisotropicCore::compute_eigenvalues`]
    /// (`None` after a solver failure).
    #[must_use]
    pub fn kelvin_lambda_max(&self) -> Option<Real> {
        self.kelvin_lambda_max
    }

    /// Conservative bulk-wave-speed bound `√(λ_max(D·C·D)/ρ)` (UD-033) from
    /// the cached eigenvalue (no eigen-solve, no allocation).
    ///
    /// # Errors
    /// The eigen-solver failed at the last update (non-finite stiffness).
    pub fn kelvin_celerity(&self, rho: Real) -> Result<Real> {
        let max = self.kelvin_lambda_max.ok_or_else(|| {
            Error::invalid(
                "Kelvin celerity: the eigenvalues of the stiffness could not be computed at the last parameter update",
            )
        })?;
        Ok((max / rho).sqrt())
    }

    /// Celerity in the given [`CelerityMode`].
    pub fn celerity(&self, rho: Real, mode: CelerityMode) -> Result<Real> {
        match mode {
            CelerityMode::KelvinBound => self.kelvin_celerity(rho),
            CelerityMode::UpstreamFirstEigenvalue => Ok(self.upstream_celerity(rho)),
        }
    }

    /// Stiffness of the law in Voigt notation (upstream `getVoigtStiffness`).
    #[must_use]
    pub fn voigt_stiffness(&self) -> &MatrixX {
        &self.c
    }
}

/// A material axis parameter `n<k>` (upstream `Vector<Real, dim>`,
/// `_pat_parsmod`; input vectors longer than `dim` are truncated).
#[macro_export]
#[doc(hidden)]
macro_rules! axis_param {
    ($S:ty, $name:literal, $k:literal) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::VectorReal,
            ::akantu_parser::param::ParameterAccess::PARSMOD,
            "Direction of main material axis",
            |s| ::akantu_parser::param::ParamValue::VectorReal(s.core.dir_vecs[$k].to_vec()),
            Some(|s, v| {
                if let ::akantu_parser::param::ParamValue::VectorReal(x) = v {
                    let x = ::akantu_parser::param::fit_vector(s.core.dir_vecs[$k].len(), x)?;
                    s.core.dir_vecs[$k].copy_from_slice(&x);
                }
                Ok(())
            }),
        )
    };
}

/// A stiffness coefficient parameter `C<i><j>` of `C'` (1-based names).
#[macro_export]
#[doc(hidden)]
macro_rules! cij_param {
    ($S:ty, $name:literal, $i:literal, $j:literal) => {
        ::akantu_parser::param::ParamDescriptor::<$S>::new(
            $name,
            ::akantu_parser::param::ParamKind::Real,
            ::akantu_parser::param::ParameterAccess::PARSMOD,
            concat!("Coefficient ", $name),
            |s| ::akantu_parser::param::ParamValue::Real(s.core.c_prime.at($i, $j)),
            Some(|s, v| {
                *s.core.c_prime.at_mut($i, $j) = v.as_real()?;
                Ok(())
            }),
        )
    };
}

/// The axis and `Cij` parameters of dimension `$d`, followed by `$extra`.
#[macro_export]
#[doc(hidden)]
macro_rules! anisotropic_params {
    (1, $S:ty $(, $extra:expr)*) => {
        &[
            $crate::axis_param!($S, "n1", 0),
            $crate::cij_param!($S, "C11", 0, 0),
            $($extra,)*
        ]
    };
    (2, $S:ty $(, $extra:expr)*) => {
        &[
            $crate::axis_param!($S, "n1", 0),
            $crate::axis_param!($S, "n2", 1),
            $crate::cij_param!($S, "C11", 0, 0),
            $crate::cij_param!($S, "C12", 0, 1),
            $crate::cij_param!($S, "C13", 0, 2),
            $crate::cij_param!($S, "C22", 1, 1),
            $crate::cij_param!($S, "C23", 1, 2),
            $crate::cij_param!($S, "C33", 2, 2),
            $($extra,)*
        ]
    };
    (3, $S:ty $(, $extra:expr)*) => {
        &[
            $crate::axis_param!($S, "n1", 0),
            $crate::axis_param!($S, "n2", 1),
            $crate::axis_param!($S, "n3", 2),
            $crate::cij_param!($S, "C11", 0, 0),
            $crate::cij_param!($S, "C12", 0, 1),
            $crate::cij_param!($S, "C13", 0, 2),
            $crate::cij_param!($S, "C14", 0, 3),
            $crate::cij_param!($S, "C15", 0, 4),
            $crate::cij_param!($S, "C16", 0, 5),
            $crate::cij_param!($S, "C22", 1, 1),
            $crate::cij_param!($S, "C23", 1, 2),
            $crate::cij_param!($S, "C24", 1, 3),
            $crate::cij_param!($S, "C25", 1, 4),
            $crate::cij_param!($S, "C26", 1, 5),
            $crate::cij_param!($S, "C33", 2, 2),
            $crate::cij_param!($S, "C34", 2, 3),
            $crate::cij_param!($S, "C35", 2, 4),
            $crate::cij_param!($S, "C36", 2, 5),
            $crate::cij_param!($S, "C44", 3, 3),
            $crate::cij_param!($S, "C45", 3, 4),
            $crate::cij_param!($S, "C46", 3, 5),
            $crate::cij_param!($S, "C55", 4, 4),
            $crate::cij_param!($S, "C56", 4, 5),
            $crate::cij_param!($S, "C66", 5, 5),
            $($extra,)*
        ]
    };
}

/// General linear anisotropic elastic law (upstream
/// `MaterialElasticLinearAnisotropic<DIM>` with `symmetric = true`, the
/// only variant reachable from the factory).
#[derive(Clone, Debug)]
pub struct MaterialElasticLinearAnisotropic<const DIM: usize> {
    base: MaterialBase,
    /// Anisotropic state.
    pub core: AnisotropicCore<DIM>,
    /// Upstream-defect switches (UD-033: `celerity`; UD-104:
    /// `material_axes_as_directions`).
    pub options: LawOptions,
}

impl<const DIM: usize> MaterialElasticLinearAnisotropic<DIM> {
    /// New law; `mesh` initialises the element filter blocks.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        if spec.spatial_dimension != DIM {
            return Err(Error::DimensionMismatch {
                context: "MaterialElasticLinearAnisotropic",
                expected: DIM,
                got: spec.spatial_dimension,
            });
        }
        let base = match mesh {
            Some(m) => MaterialBase::with_mesh(spec, "elastic_anisotropic", m)?,
            None => MaterialBase::new(spec, "elastic_anisotropic")?,
        };
        Ok(Self {
            base,
            core: AnisotropicCore::new(),
            options: LawOptions::default(),
        })
    }

    const PARAMS_1D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(1, Self);
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(2, Self);
    const PARAMS_3D: &'static [ParamDescriptor<Self>] = crate::anisotropic_params!(3, Self);

    /// Upstream `updateInternalParameters`: `Material::` part, symmetrise,
    /// rotate, eigenvalues, stiffness flag.
    fn update(&mut self) -> Result<()> {
        self.base.update_internal_parameters()?;
        self.core.symmetrize();
        self.core
            .rotate_c_prime(self.options.material_axes_as_directions)?;
        self.core.compute_eigenvalues()?;
        self.core.was_stiffness_assembled = false;
        Ok(())
    }
}

impl<const DIM: usize> MaterialParams for MaterialElasticLinearAnisotropic<DIM> {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        match DIM {
            1 => Self::PARAMS_1D,
            2 => Self::PARAMS_2D,
            _ => Self::PARAMS_3D,
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.update()
    }
}

crate::impl_material_registry!(MaterialElasticLinearAnisotropic<DIM>, const DIM);

/// [`ConstitutiveLaw`] and the shared [`Material`] methods of the
/// anisotropic laws (both hold `base: MaterialBase` and `core:
/// AnisotropicCore<DIM>`).
#[macro_export]
#[doc(hidden)]
macro_rules! impl_anisotropic_law {
    ($ty:ident) => {
        impl<const DIM: usize> ::akantu_model::constitutive_law::ConstitutiveLaw for $ty<DIM> {
            fn name(&self) -> &str {
                &self.base.name
            }
            fn law_id(&self) -> &str {
                &self.base.id
            }
            fn law_type(&self) -> &str {
                &self.base.law_type
            }
            fn element_filter(&self) -> &::akantu_core::ElementTypeMapArray<::akantu_core::Idx> {
                &self.base.element_filter
            }
            fn element_filter_mut(
                &mut self,
            ) -> &mut ::akantu_core::ElementTypeMapArray<::akantu_core::Idx> {
                &mut self.base.element_filter
            }
            fn internal_restriction(
                &self,
            ) -> Option<::akantu_model::internal_field::TypeRestriction> {
                Some(self.base.internal_restriction())
            }
            fn for_each_internal(
                &self,
                f: &mut dyn FnMut(&str, ::akantu_model::constitutive_law::InternalFieldRef<'_>),
            ) {
                for (n, field) in self.base.internals() {
                    f(
                        n,
                        ::akantu_model::constitutive_law::InternalFieldRef::Real(field),
                    );
                }
            }
            fn for_each_internal_mut(
                &mut self,
                f: &mut dyn FnMut(
                    &str,
                    ::akantu_model::constitutive_law::InternalFieldMut<'_>,
                ) -> ::akantu_core::Result<()>,
            ) -> ::akantu_core::Result<()> {
                for field in self.base.internals_mut() {
                    let name = field.name().to_owned();
                    f(
                        &name,
                        ::akantu_model::constitutive_law::InternalFieldMut::Real(field),
                    )?;
                }
                Ok(())
            }
        }
    };
}

impl_anisotropic_law!(MaterialElasticLinearAnisotropic);

/// Shared stress/tangent/energy batch kernels.
///
/// The points split over `exec`'s threads ([`crate::material::points_exec`]
/// of the calling law): a point's stress is a function of its own gradient
/// and the law's constant `C`. A point fails only on the law's Voigt sizes,
/// the same for every point: the first point of every part fails before
/// writing, so an error leaves the stress as the serial loop leaves it
/// (unchanged), whatever the split.
pub(crate) fn anisotropic_compute_stress<const DIM: usize>(
    exec: &akantu_core::exec::Exec,
    base: &mut MaterialBase,
    core: &AnisotropicCore<DIM>,
    ty: ElementType,
    ghost: GhostType,
) -> Result<()> {
    let n = DIM * DIM;
    // Upstream uses Material::getArguments: "sigma" is the stress (or the
    // PK2 stress if finite_deformation was requested, which the
    // anisotropic laws do not support beyond this storage choice).
    // Disjoint borrows of `grad_u` and the stress field instead of a
    // per-step copy of `grad_u`.
    let MaterialBase {
        grad_u,
        stress,
        piola_kirchhoff_2,
        finite_deformation,
        ..
    } = base;
    let sigma_field = if *finite_deformation {
        piola_kirchhoff_2.as_mut().ok_or_else(|| {
            Error::invalid(
                "finite_deformation: piola_kirchhoff_2 is not registered (init the material first)",
            )
        })?
    } else {
        stress
    };
    let grad_u = grad_u.get(ty, ghost)?;
    // A point's kernel fails only on the law's Voigt sizes, the same for
    // every point: when the first point succeeds every point does, and the
    // loop below writes every value of a block of the gradient's shape,
    // so a deferred history save of it needs no copy.
    let shape = |a: &akantu_core::SharedArray<Real>| (a.len(), a.nb_component());
    if let (Ok(s), Some(g0)) = (
        sigma_field.values().shared(ty, ghost).map(shape),
        grad_u.as_slice().get(..n),
    ) && s == (grad_u.len(), n)
        && grad_u.nb_component() == n
        && core
            .stress_on_quad(as_matrix(g0)?, &mut [[0.0; DIM]; DIM])
            .is_ok()
    {
        let _ = sigma_field.prepare_overwrite(ty, ghost);
    }
    let mut sigma = sigma_field.get_mut(ty, ghost)?;
    if grad_u.len() != sigma.len() {
        return Err(Error::DimensionMismatch {
            context: "MaterialElasticLinearAnisotropic::computeStress",
            expected: grad_u.len(),
            got: sigma.len(),
        });
    }
    if grad_u.nb_component() != n || sigma.nb_component() != n {
        return Err(Error::DimensionMismatch {
            context: "MaterialElasticLinearAnisotropic::computeStress: components",
            expected: n,
            got: sigma.nb_component(),
        });
    }
    let items = (
        akantu_core::exec::TuplesRef::new(grad_u.as_slice(), n),
        akantu_core::exec::Tuples::new(sigma.as_mut_slice(), n),
    );
    exec.try_for_each(items, 1024, |_, (g, mut s)| {
        anisotropic_stress_part(core, g.as_slice(), s.as_mut_slice())
    })
}

/// The stress of a contiguous run of points (a part of
/// [`anisotropic_compute_stress`]): stops at the first failing point, as
/// the serial loop.
fn anisotropic_stress_part<const DIM: usize>(
    core: &AnisotropicCore<DIM>,
    grad_u: &[Real],
    sigma: &mut [Real],
) -> Result<()> {
    let n = DIM * DIM;
    for (g, s) in grad_u.chunks_exact(n).zip(sigma.chunks_exact_mut(n)) {
        let g: &Matrix<DIM, DIM> = as_matrix(g)?;
        let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
        core.stress_on_quad(g, s)?;
    }
    Ok(())
}

pub(crate) fn anisotropic_tangent<const DIM: usize>(
    base: &MaterialBase,
    core: &AnisotropicCore<DIM>,
    ty: ElementType,
    ghost: GhostType,
    tangent: &mut Array<Real>,
) -> Result<()> {
    let nq = base.grad_u.get(ty, ghost)?.len();
    let v = tangent_stiffness_voigt_size(DIM);
    check_tangent(tangent, nq, v)?;
    for t in tangent.as_mut_slice().chunks_exact_mut(v * v) {
        core.tangent_on_quad(t);
    }
    Ok(())
}

pub(crate) fn anisotropic_energy<const DIM: usize>(
    base: &mut MaterialBase,
    ty: ElementType,
) -> Result<()> {
    if base.finite_deformation {
        return Err(Error::NotImplemented(
            "finite deformation not possible in material anisotropic (TO BE IMPLEMENTED)",
        ));
    }
    let g = GhostType::NotGhost;
    let n = DIM * DIM;
    // disjoint borrows instead of per-call copies
    let MaterialBase {
        grad_u,
        stress,
        potential_energy,
        ..
    } = base;
    let grad_u = grad_u.get(ty, g)?;
    let sigma = stress.get(ty, g)?;
    let mut epot = potential_energy.get_mut(ty, g)?;
    for ((gu, s), e) in grad_u
        .as_slice()
        .chunks_exact(n)
        .zip(sigma.as_slice().chunks_exact(n))
        .zip(epot.as_mut_slice())
    {
        *e = AnisotropicCore::<DIM>::energy_on_quad(as_matrix(s)?, as_matrix(gu)?);
    }
    Ok(())
}

pub(crate) fn anisotropic_energy_by_element<const DIM: usize>(
    base: &MaterialBase,
    nq: usize,
    element: Element,
) -> Result<Vec<Real>> {
    let n = DIM * DIM;
    let start = element.idx() * nq * n;
    let range = start..start + nq * n;
    let grad_u = base.grad_u.get(element.ty(), GhostType::NotGhost)?;
    let sigma = base.stress.get(element.ty(), GhostType::NotGhost)?;
    let (Some(gu), Some(s)) = (
        grad_u.as_slice().get(range.clone()),
        sigma.as_slice().get(range),
    ) else {
        return Err(Error::InvalidIndex {
            what: "law-local element",
            value: i64::from(element.index()),
        });
    };
    gu.chunks_exact(n)
        .zip(s.chunks_exact(n))
        .map(|(gu, s)| {
            Ok(AnisotropicCore::<DIM>::energy_on_quad(
                as_matrix(s)?,
                as_matrix(gu)?,
            ))
        })
        .collect()
}

impl<const DIM: usize> Material for MaterialElasticLinearAnisotropic<DIM> {
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream: `Material::initMaterial` (not overridden).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base.set_init();
        Ok(())
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let exec = crate::material::points_exec(self, ctx);
        anisotropic_compute_stress(exec, &mut self.base, &self.core, ty, ghost)
    }

    /// A point's stress reads only its own gradient and the constant `C`;
    /// the tangent is the constant `C` (no state written per point).
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        anisotropic_tangent(&self.base, &self.core, ty, ghost, tangent)?;
        self.core.was_stiffness_assembled = true;
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        anisotropic_energy::<DIM>(&mut self.base, ty)
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let nq = ctx
            .services
            .nb_integration_points(element.ty(), GhostType::NotGhost);
        anisotropic_energy_by_element::<DIM>(&self.base, nq, element)
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getPushWaveSpeed of elastic_anisotropic",
        ))
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Err(Error::NotImplemented(
            "getShearWaveSpeed of elastic_anisotropic",
        ))
    }

    fn celerity(&self, _element: Element) -> Result<Real> {
        self.core.celerity(self.base.rho, self.options.celerity)
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.options)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        !self.core.was_stiffness_assembled
    }

    fn mark_stiffness_changed(&mut self) {
        self.core.was_stiffness_assembled = false;
    }
}

/// Register `elastic_anisotropic`.
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "elastic_anisotropic",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialElasticLinearAnisotropic::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constant-size Voigt product against the general product of the
    /// upstream operand types, bit for bit (D-046 rule; payload NaNs exact).
    #[test]
    fn constant_size_voigt_stress_matches_the_general_product_bitwise() {
        use akantu_core::bitwise_testing::{Values, same};
        let mut vals = Values::new(0xa915_0c0e);
        for round in 0..30_000 {
            let payload = round % 5 == 4;
            for v in [1usize, 3, 6] {
                if payload {
                    let k = 1 + vals.next_u64() % (v * v + v) as u64;
                    vals.payload_at(k);
                }
                let mut c = vec![0.0; v * v];
                vals.fill(&mut c);
                let mut strain = [0.0; 6];
                vals.fill(&mut strain[..v]);
                vals.default_mode();
                let (mut a, mut b) = ([0.0; 6], [0.0; 6]);
                match v {
                    1 => voigt_stress_seq::<1>(&c, &strain, &mut a),
                    3 => voigt_stress_seq::<3>(&c, &strain, &mut a),
                    _ => voigt_stress_seq::<6>(&c, &strain, &mut a),
                }
                voigt_stress_product(v, &|i, k| c[i + k * v], &strain[..v], &mut b[..v]).unwrap();
                for i in 0..v {
                    let ok = if payload {
                        a[i].to_bits() == b[i].to_bits()
                    } else {
                        same(a[i], b[i])
                    };
                    assert!(ok, "v={v} i={i}: {:e} {:e}", a[i], b[i]);
                }
            }
        }
    }

    fn hex(t: &str) -> Real {
        Real::from_bits(u64::from_str_radix(t, 16).expect("hex double"))
    }

    /// The material's own product code (not
    /// only the dense layer) reproduces every `anisorot` (`Cfull = rotator ·
    /// C' · revrotor`) and `aniso` (`C · strain`) record of the dense
    /// product oracle corpus bit for bit, so a change of an operand description
    /// (e.g. the 2-D rotation's left operand made dynamic) fails here even
    /// where the material fixtures cannot separate the orders.
    #[test]
    fn products_match_the_oracle_anisotropic_records() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../akantu-linalg/tests/fixtures/dense/dense_gemm_strict.txt"
        );
        let text = std::fs::read_to_string(path).expect("dense product corpus");
        let (mut rot, mut st) = (0, 0);
        for line in text.lines() {
            let fields: Vec<&str> = line.split(" | ").collect();
            let head: Vec<&str> = fields[0].split_whitespace().collect();
            let (op, dim) = match head.as_slice() {
                [op @ ("anisorot" | "aniso"), "M", d] => (*op, d.parse::<usize>().unwrap()),
                _ => continue,
            };
            let input: Vec<Real> = fields[1].split_whitespace().map(hex).collect();
            let expected: Vec<u64> = fields[2]
                .split_whitespace()
                .map(|t| u64::from_str_radix(t, 16).unwrap())
                .collect();
            if op == "anisorot" {
                let d2 = dim * dim;
                let n = d2 * d2;
                assert_eq!(input.len(), 3 * n, "{line}");
                let (rotator, rest) = input.split_at(n);
                let (c_prime, revrotor) = rest.split_at(n);
                let mut cfull = vec![0.0; n];
                rotate_full(
                    d2,
                    rotator,
                    &|k, j| c_prime[k + j * d2],
                    revrotor,
                    &mut cfull,
                )
                .unwrap();
                let got: Vec<u64> = cfull.iter().map(|x| x.to_bits()).collect();
                assert_eq!(got, expected, "anisorot M {dim}");
                rot += 1;
            } else {
                let v = dim * (dim + 1) / 2;
                assert_eq!(input.len(), v * v + v, "{line}");
                let (c, strain) = input.split_at(v * v);
                let mut stress = vec![0.0; v];
                voigt_stress_product(v, &|i, k| c[i + k * v], strain, &mut stress).unwrap();
                let got: Vec<u64> = stress.iter().map(|x| x.to_bits()).collect();
                assert_eq!(got, expected, "aniso M {dim}");
                st += 1;
            }
        }
        assert!(
            rot >= 6 && st >= 9,
            "records found: anisorot {rot}, aniso {st}"
        );
    }
}
