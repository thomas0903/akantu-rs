// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_finite_deformation/material_neohookean.hh, material_neohookean.cc,
// material_neohookean_inline_impl.hh, plane_stress_toolbox.hh,
// plane_stress_toolbox_tmpl.hh; the energy follows
// doc/dev-doc/manual/constitutive-laws.rst (Neo-Hookean section).

//! `neohookean` (MAT-025: 1-D, 3-D, 2-D plane strain; MAT-026: 2-D plane
//! stress): compressible Neo-Hookean hyperelasticity, total Lagrangian
//! (`finite_deformation` is forced to `true` by the constructor).
//!
//! # The documented law (default, UD-026 / UD-057)
//!
//! With `F = I + ∇u`, `C = FᵀF`, `J = det F·√C33` and the Lamé constants
//! `λ = νE/((1 + ν)(1 − 2ν))`, `μ = E/(2(1 + ν))` (3-D formulas in every
//! case; `ν = 0` in 1-D):
//!
//! ```text
//! Ψ(C) = λ/2 (ln J)² − μ ln J + μ/2 (tr C₃ − 3)
//! S    = 2 ∂Ψ/∂C = μ I + (λ ln J − μ) C⁻¹
//! ℂ_ijkl = λ C⁻¹_ij C⁻¹_kl + (μ − λ ln J)(C⁻¹_ik C⁻¹_jl + C⁻¹_il C⁻¹_kj)
//! ```
//!
//! where `C₃` is the 3-D right Cauchy–Green tensor of the represented
//! state: `tr C₃ = tr C + 2` in 1-D (lateral stretches 1), `tr C + C33` in
//! 2-D (`C33 = 1` in plane strain), `tr C` in 3-D. The stress kernel and the
//! tangent follow upstream's expressions and operation order
//! (`material_neohookean_inline_impl.hh`); `ln` is the glibc port of
//! `akantu_core::glibc_libm`. The tangent acts on Voigt
//! engineering strains (`2E_ij` for shears), as every Akantu tangent.
//!
//! **2-D plane stress** (`Plane_Stress = true`): before the stress, `C33 =
//! x > 0` solves `S33 = 0`, i.e. `f(x) = λ/2 (ln x + ln det C₂) + μ(x − 1)
//! = 0` (upstream `C33_NR`), started from the stored
//! `third_axis_deformation` with a bracketed Newton iteration to machine
//! precision ([`solve_c33`]; upstream: plain Newton, `|f| ≤ 1e-5`). The
//! tangent is the **condensed** in-plane tangent
//! `ℂ̃_mn = ℂ_mn − ℂ_m33 ℂ_33n / ℂ_3333` (Schur complement of the
//! out-of-plane block, UD-026), the Cauchy stress
//! `σ = F S Fᵀ / J` with `J = det F₂ √C33`.
//!
//! # Upstream behaviour
//!
//! Upstream's `computeStress`, `computeTangentModuli` and
//! `computePotentialEnergy` reach `AKANTU_TO_IMPLEMENT` (UD-026); its energy
//! kernel has wrong factors and omits `C33` and its plane-stress tangent is
//! not condensed (UD-057). [`NeohookeanMode::UpstreamNotImplemented`]
//! (part of `LawOptions::upstream()`) reproduces the errors, which is what
//! the oracle records. Upstream's `updateInternalParameters` also skips
//! `Material::updateInternalParameters` (the `eigen_grad_u` parameter is
//! never copied into the internal; UD-147): reproduced in upstream
//! mode, the parent update runs by default.

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::voigt::voigt_table;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};

use crate::material::{
    LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams, NeohookeanMode,
    check_tangent,
};
use crate::material_tensor::{
    grad_u_to_f, right_cauchy, s_to_cauchy, tangent_stiffness_voigt_size,
};

const NG: GhostType = GhostType::NotGhost;

/// Kinematics of one point: `C`, `C⁻¹`, `ln J`.
struct Kinematics<const D: usize> {
    c: Matrix<D, D>,
    c_inv: Matrix<D, D>,
    ln_j: Real,
}

fn kinematics<const D: usize>(grad_u: &Matrix<D, D>, c33: Real) -> Result<Kinematics<D>> {
    let f = grad_u_to_f(grad_u);
    let c = right_cauchy(&f);
    let j = jacobian(&f, c33);
    if !jacobian_is_valid(j) {
        return Err(Error::Numerical(format!(
            "neohookean: J = det(F)·√C33 = {j} is not positive (inverted element)"
        )));
    }
    Ok(Kinematics {
        c,
        c_inv: c.inverse_unchecked(),
        ln_j: glibc_libm::log(j),
    })
}

/// `J = det F·√C33`.
fn jacobian<const D: usize>(f: &Matrix<D, D>, c33: Real) -> Real {
    f.det() * c33.sqrt()
}

/// The condition under which [`kinematics`] accepts `J`.
fn jacobian_is_valid(j: Real) -> bool {
    j.is_finite() && j > 0.
}

/// Index of the first point of `grad_u` (tuples of `D²`) whose `J` is
/// refused by [`kinematics`] (the first point at which the stress loop
/// fails), `grad_u.len() / D²` when none: a read-only pass on `exec`'s
/// threads, the same predicate on the same values as the kernel.
fn first_invalid_jacobian<const D: usize>(
    exec: &akantu_core::exec::Exec,
    grad_u: &[Real],
    c33: &[Real],
) -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let n = D * D;
    let nq = c33.len();
    let first = AtomicUsize::new(nq);
    let items = (
        akantu_core::exec::TuplesRef::new(&grad_u[..nq * n], n),
        akantu_core::exec::TuplesRef::new(c33, 1),
    );
    exec.for_each(items, POINTS_PER_PART, |start, (g, c)| {
        for (k, (g, &x)) in g.as_slice().chunks_exact(n).zip(c.as_slice()).enumerate() {
            let valid =
                as_matrix::<D, D>(g).is_ok_and(|g| jacobian_is_valid(jacobian(&grad_u_to_f(g), x)));
            if !valid {
                first.fetch_min(start + k, Ordering::Relaxed);
                break;
            }
        }
    });
    first.into_inner()
}

/// Smallest part of the threaded point loops.
const POINTS_PER_PART: usize = 512;

/// The documented second Piola–Kirchhoff stress `S = μI + (λ ln J − μ)C⁻¹`
/// of one point (upstream `computeStressOnQuad`, Eigen order:
/// `I(i, j)·μ + s·C⁻¹(i, j)` with `s = λ·ln J − μ`).
///
/// # Errors
/// [`Error::Numerical`] for `J ≤ 0`.
pub fn neohookean_stress<const D: usize>(
    lambda: Real,
    mu: Real,
    grad_u: &Matrix<D, D>,
    c33: Real,
) -> Result<Matrix<D, D>> {
    let k = kinematics(grad_u, c33)?;
    let s = lambda * k.ln_j - mu;
    Ok(std::array::from_fn(|j| {
        std::array::from_fn(|i| {
            let id = if i == j { 1. } else { 0. };
            id * mu + s * k.c_inv[j][i]
        })
    }))
}

/// The material tangent `ℂ = ∂S/∂E` in Voigt form (column-major
/// `voigt × voigt` into `out`), upstream's expression
/// `λ·C⁻¹_ij·C⁻¹_kl + (μ − λ ln J)(C⁻¹_ik C⁻¹_jl + C⁻¹_il C⁻¹_kj)`; not
/// condensed (1-D, 3-D, 2-D plane strain).
///
/// # Errors
/// [`Error::Numerical`] for `J ≤ 0`; [`Error::DimensionMismatch`] if `out`
/// does not hold exactly `voigt²` values (a malformed
/// buffer must not panic).
pub fn neohookean_tangent<const D: usize>(
    lambda: Real,
    mu: Real,
    grad_u: &Matrix<D, D>,
    c33: Real,
    out: &mut [Real],
) -> Result<()> {
    let vt = voigt_table(D)?;
    let v = tangent_stiffness_voigt_size(D);
    if out.len() != v * v {
        return Err(Error::DimensionMismatch {
            context: "neohookean_tangent: tangent buffer",
            expected: v * v,
            got: out.len(),
        });
    }
    let k = kinematics(grad_u, c33)?;
    let ci = |a: usize, b: usize| k.c_inv[b][a];
    let shear = mu - lambda * k.ln_j;
    for m in 0..v {
        let (i, j) = vt.vec(m);
        for n in 0..v {
            let (kk, l) = vt.vec(n);
            out[m + n * v] = lambda * ci(i, j) * ci(kk, l)
                + shear * (ci(i, kk) * ci(j, l) + ci(i, l) * ci(kk, j));
        }
    }
    Ok(())
}

/// The condensed plane-stress tangent (2-D, `Plane_Stress = true`):
/// `ℂ̃_mn = ℂ_mn − ℂ_m33·ℂ_33n/ℂ_3333` with the 3-D tangent of
/// `C₃ = diag(C₂, C33)` (`C⁻¹_α3 = 0`, so `ℂ_αβ33 = λ C⁻¹_αβ / C33` and
/// `ℂ_3333 = (λ + 2(μ − λ ln J)) / C33²`).
///
/// # Errors
/// [`Error::Numerical`] for `J ≤ 0`; [`Error::DimensionMismatch`] if `out`
/// does not hold exactly 9 values.
pub fn neohookean_tangent_plane_stress(
    lambda: Real,
    mu: Real,
    grad_u: &Matrix<2, 2>,
    c33: Real,
    out: &mut [Real],
) -> Result<()> {
    neohookean_tangent::<2>(lambda, mu, grad_u, c33, out)?;
    let k = kinematics(grad_u, c33)?;
    let vt = voigt_table(2)?;
    let inv33 = 1. / c33;
    let c3333 = (lambda + 2. * (mu - lambda * k.ln_j)) * inv33 * inv33;
    let c_m33: [Real; 3] = std::array::from_fn(|m| {
        let (i, j) = vt.vec(m);
        lambda * k.c_inv[j][i] * inv33
    });
    for m in 0..3 {
        for n in 0..3 {
            out[m + n * 3] -= c_m33[m] * c_m33[n] / c3333;
        }
    }
    Ok(())
}

/// The documented strain energy density `Ψ = λ/2 (ln J)² − μ ln J +
/// μ/2 (tr C₃ − 3)` (module docs; corrects upstream's kernel, UD-057).
///
/// # Errors
/// [`Error::Numerical`] for `J ≤ 0`.
pub fn neohookean_energy<const D: usize>(
    lambda: Real,
    mu: Real,
    grad_u: &Matrix<D, D>,
    c33: Real,
) -> Result<Real> {
    let k = kinematics(grad_u, c33)?;
    let extra = match D {
        1 => 2.,
        2 => c33,
        _ => 0.,
    };
    let tr3 = k.c.trace() + extra;
    Ok(0.5 * lambda * k.ln_j * k.ln_j - mu * k.ln_j + 0.5 * mu * (tr3 - 3.))
}

/// Plane-stress condition `S33 = 0` for `C33 = x`: upstream `C33_NR::f`
/// `λ/2·(ln x + ln(C11·C22 − C12²)) + μ(x − 1)`.
fn c33_residual(lambda: Real, mu: Real, ln_det_c: Real, x: Real) -> Real {
    lambda / 2. * (glibc_libm::log(x) + ln_det_c) + mu * (x - 1.)
}

/// Solve the plane-stress condition for `C33 > 0` (module docs) from the
/// initial guess `x0` (the stored `third_axis_deformation`).
///
/// For `λ ≥ 0, μ > 0` the residual increases strictly on `(0, ∞)` from
/// `−∞` (λ > 0) and the root is unique; for `λ < 0` the physical root is
/// the one on the increasing branch `x > −λ/(2μ)`. The iteration keeps a
/// bracket `f(lo) < 0 < f(hi)` and takes Newton steps, bisecting when a
/// step leaves the bracket, until the step is below `4ε·x`.
///
/// # Errors
/// [`Error::Numerical`] if `det C₂ ≤ 0`, `μ ≤ 0`, no root exists on the
/// physical branch, or the iteration does not converge.
pub fn solve_c33(lambda: Real, mu: Real, grad_u: &Matrix<2, 2>, x0: Real) -> Result<Real> {
    let f = grad_u_to_f(grad_u);
    let c = right_cauchy(&f);
    let det_c = c[0][0] * c[1][1] - c[1][0] * c[1][0];
    let valid = det_c.is_finite() && det_c > 0. && mu > 0. && lambda.is_finite();
    if !valid {
        return Err(Error::Numerical(format!(
            "neohookean plane stress: cannot solve for C33 (det C2 = {det_c}, mu = {mu})"
        )));
    }
    let ln_det_c = glibc_libm::log(det_c);
    let res = |x: Real| c33_residual(lambda, mu, ln_det_c, x);
    let floor = if lambda < 0. { -lambda / (2. * mu) } else { 0. };
    if lambda < 0. && res(floor) >= 0. {
        return Err(Error::Numerical(
            "neohookean plane stress: no C33 with S33 = 0 on the physical branch".into(),
        ));
    }
    let mut x = if x0.is_finite() && x0 > floor {
        x0
    } else {
        floor.max(0.) + 1.
    };
    let mut fx = res(x);
    if fx == 0. {
        return Ok(x);
    }
    // bracket the root
    let (mut lo, mut hi);
    if fx > 0. {
        hi = x;
        lo = x;
        let mut k = 0;
        loop {
            lo = floor + 0.5 * (lo - floor);
            if res(lo) < 0. {
                break;
            }
            k += 1;
            if k > 2000 || lo <= floor {
                return Err(Error::Numerical(
                    "neohookean plane stress: C33 bracket".into(),
                ));
            }
        }
    } else {
        lo = x;
        hi = x;
        let mut k = 0;
        loop {
            hi = 2. * hi + 1.;
            if res(hi) > 0. {
                break;
            }
            k += 1;
            if k > 2000 || !hi.is_finite() {
                return Err(Error::Numerical(
                    "neohookean plane stress: C33 bracket".into(),
                ));
            }
        }
    }
    for _ in 0..200 {
        let df = lambda / (2. * x) + mu;
        let mut next = x - fx / df;
        if !(next > lo && next < hi) || !next.is_finite() {
            next = 0.5 * (lo + hi);
        }
        let fnext = res(next);
        if fnext == 0. {
            return Ok(next);
        }
        if fnext < 0. {
            lo = next;
        } else {
            hi = next;
        }
        let step = (next - x).abs();
        x = next;
        fx = fnext;
        if step <= 4. * Real::EPSILON * x || hi - lo <= 4. * Real::EPSILON * x {
            return Ok(x);
        }
    }
    Err(Error::Numerical(
        "neohookean plane stress: C33 iteration did not converge".into(),
    ))
}

/// `neohookean` (upstream `MaterialNeohookean<DIM> : PlaneStressToolbox<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialNeohookean<const DIM: usize> {
    base: MaterialBase,
    /// `Plane_Stress` (2-D only).
    pub plane_stress: bool,
    /// `C33` per quadrature point (2-D only, default 1; solved in plane
    /// stress).
    pub third_axis_deformation: Option<InternalField<Real>>,
    /// Young's modulus `E`.
    pub e: Real,
    /// Poisson's ratio `nu`.
    pub nu: Real,
    /// Upstream-defect switches (UD-026/UD-057: `neohookean`).
    pub options: LawOptions,
    lambda: Real,
    mu: Real,
    kpa: Real,
}

impl<const DIM: usize> MaterialNeohookean<DIM> {
    /// New law (upstream constructor: `finite_deformation = true`).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        if spec.spatial_dimension != DIM {
            return Err(Error::DimensionMismatch {
                context: "MaterialNeohookean",
                expected: DIM,
                got: spec.spatial_dimension,
            });
        }
        let mut base = match mesh {
            Some(m) => MaterialBase::with_mesh(spec, "neohookean", m)?,
            None => MaterialBase::new(spec, "neohookean")?,
        };
        base.finite_deformation = true;
        let third_axis_deformation = if DIM == 2 {
            Some(InternalField::new("third_axis_deformation", 1, 1.0)?)
        } else {
            None
        };
        Ok(Self {
            base,
            plane_stress: false,
            third_axis_deformation,
            e: 0.,
            nu: 0.5,
            options: LawOptions::default(),
            lambda: 0.,
            mu: 0.,
            kpa: 0.,
        })
    }

    /// First Lamé coefficient.
    #[must_use]
    pub fn lambda(&self) -> Real {
        self.lambda
    }

    /// Shear modulus.
    #[must_use]
    pub fn mu(&self) -> Real {
        self.mu
    }

    fn upstream_mode(&self) -> bool {
        self.options.neohookean == NeohookeanMode::UpstreamNotImplemented
    }

    /// Upstream `MaterialNeohookean::updateInternalParameters` (plus the
    /// skipped `Material::updateInternalParameters` by default).
    fn update(&mut self) -> Result<()> {
        if !self.upstream_mode() {
            self.base.update_internal_parameters()?;
        }
        let (e, nu) = (self.e, self.nu);
        self.lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        self.mu = e / (2. * (1. + nu));
        self.kpa = self.lambda + 2. / 3. * self.mu;
        Ok(())
    }

    fn c33_values(&self, ty: ElementType, ghost: GhostType, nq: usize) -> Result<Vec<Real>> {
        match &self.third_axis_deformation {
            Some(t) => Ok(t.get(ty, ghost)?.as_slice().to_vec()),
            None => Ok(vec![1.; nq]),
        }
    }

    fn plane_stress_active(&self) -> bool {
        DIM == 2 && self.plane_stress
    }
}

impl<const DIM: usize> MaterialParams for MaterialNeohookean<DIM> {
    fn params_base(&self) -> &MaterialBase {
        &self.base
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        self.update()
    }
}

macro_rules! neohookean_params {
    ($($extra:expr),*) => {
        &[
            $($extra,)*
            crate::real_param!(Self, "E", ParameterAccess::PARSMOD, "Young's modulus", e),
            crate::real_param!(Self, "nu", ParameterAccess::PARSMOD, "Poisson's ratio", nu),
            crate::readable_real_param!(Self, "lambda", "First Lamé coefficient", lambda),
            crate::readable_real_param!(Self, "mu", "Second Lamé coefficient", mu),
            crate::readable_real_param!(Self, "kapa", "Bulk coefficient", kpa),
        ]
    };
}

impl<const DIM: usize> MaterialNeohookean<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = neohookean_params!();
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = neohookean_params!(ParamDescriptor::new(
        "Plane_Stress",
        ParamKind::Bool,
        ParameterAccess::PARSMOD,
        "Is plane stress",
        |s| ParamValue::Bool(s.plane_stress),
        Some(|s, v| {
            s.plane_stress = v.as_bool()?;
            Ok(())
        }),
    ));
}

crate::impl_material_registry!(MaterialNeohookean<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialNeohookean<DIM> {
    fn name(&self) -> &str {
        &self.base.name
    }
    fn law_id(&self) -> &str {
        &self.base.id
    }
    fn law_type(&self) -> &str {
        &self.base.law_type
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.base.element_filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.base.element_filter
    }
    /// The base's `(dimension, kind)` restriction for the law's own
    /// internals too (upstream `registerInternal` / `InternalField` with the
    /// law's dimension and `_ek_regular`; row CL-002).
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(Material::base(self).internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.base.internals() {
            f(n, InternalFieldRef::Real(field));
        }
        if let Some(t) = &self.third_axis_deformation {
            f("third_axis_deformation", InternalFieldRef::Real(t));
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        for field in self.base.internals_mut() {
            let name = match field.name() {
                "stress" => "stress",
                "eigen_grad_u" => "eigen_grad_u",
                "grad_u" => "grad_u",
                "potential_energy" => "potential_energy",
                "piola_kirchhoff_2" => "piola_kirchhoff_2",
                _ => "green_strain",
            };
            f(name, InternalFieldMut::Real(field))?;
        }
        if let Some(t) = &mut self.third_axis_deformation {
            f("third_axis_deformation", InternalFieldMut::Real(t))?;
        }
        Ok(())
    }
}

impl<const DIM: usize> Material for MaterialNeohookean<DIM> {
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream `MaterialNeohookean::initMaterial`: `Material::initMaterial`
    /// (registers `piola_kirchhoff_2` and `green_strain`, history,
    /// `resizeInternals`, `updateInternalParameters`), `ν = 0` in 1-D, the
    /// update again.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base.set_init();
        if DIM == 1 {
            self.nu = 0.;
        }
        self.update()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let exec = crate::material::points_exec(self, ctx);
        if self.upstream_mode() {
            return Err(Error::NotImplemented(
                "computeStress : not implemented yet !",
            ));
        }
        let n = DIM * DIM;
        let (lambda, mu) = (self.lambda, self.mu);
        // a guard on the stored gradient instead of a per-step copy (the
        // stress field is reached through its own field below)
        let grad_u = self.base.grad_u.get(ty, ghost)?;
        let nq = grad_u.len();
        // plane stress: C33 first (upstream PlaneStressToolbox<2>::computeStress
        // then computeThirdAxisDeformation, then the stress loop)
        if self.plane_stress_active()
            && let Some(t) = &mut self.third_axis_deformation
        {
            let mut c33 = t.get_mut(ty, ghost)?;
            if c33.len() != nq {
                return Err(Error::DimensionMismatch {
                    context: "MaterialNeohookean::computeThirdAxisDeformation",
                    expected: nq,
                    got: c33.len(),
                });
            }
            // every point from its own gradient and stored C33, into a
            // copy on the model's threads; the field takes the values of
            // the points before the first failing one (those the serial
            // loop writes before it stops), all of them without a failure
            let mut new = c33.as_slice().to_vec();
            let failed = std::sync::atomic::AtomicUsize::new(nq);
            let items = (
                akantu_core::exec::TuplesRef::new(grad_u.as_slice(), n),
                akantu_core::exec::Tuples::new(&mut new, 1),
            );
            let r = exec.try_for_each(items, POINTS_PER_PART, |start, (g, mut x)| {
                for (k, (g, x)) in g
                    .as_slice()
                    .chunks_exact(n)
                    .zip(x.as_mut_slice())
                    .enumerate()
                {
                    let g: &Matrix<2, 2> = as_matrix(g)?;
                    match solve_c33(lambda, mu, g, *x) {
                        Ok(v) => *x = v,
                        Err(e) => {
                            failed.fetch_min(start + k, std::sync::atomic::Ordering::Relaxed);
                            return Err(e);
                        }
                    }
                }
                Ok(())
            });
            let done = failed.into_inner();
            c33.as_mut_slice()[..done].copy_from_slice(&new[..done]);
            r?;
        }
        let c33 = self.c33_values(ty, ghost, nq)?;
        let sigma_field = if self.base.finite_deformation {
            self.base.piola_kirchhoff_2.as_mut().ok_or_else(|| {
                Error::invalid(
                    "finite_deformation: piola_kirchhoff_2 is not registered (init the material first)",
                )
            })?
        } else {
            &mut self.base.stress
        };
        let mut sigma = sigma_field.get_mut(ty, ghost)?;
        if sigma.len() != nq || c33.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialNeohookean::computeStress",
                expected: nq,
                got: sigma.len(),
            });
        }
        if grad_u.nb_component() != n || sigma.nb_component() != n {
            return Err(Error::DimensionMismatch {
                context: "MaterialNeohookean::computeStress: components",
                expected: n,
                got: sigma.nb_component(),
            });
        }
        let stress_part = |g: &[Real], s: &mut [Real], c33: &[Real]| -> Result<()> {
            for ((g, s), &x) in g.chunks_exact(n).zip(s.chunks_exact_mut(n)).zip(c33) {
                let g: &Matrix<DIM, DIM> = as_matrix(g)?;
                let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
                *s = neohookean_stress(lambda, mu, g, x)?;
            }
            Ok(())
        };
        // The serial loop stops at the first point with J ≤ 0 and leaves
        // the later points unchanged. Split over threads, the points before
        // the first such point (found by a read-only pass with the kernel's
        // predicate) run on the threads, the rest serially, which fails at
        // its first point: the same error and the same state for every
        // thread count. One part: the serial loop alone.
        let (g_all, s_all) = (grad_u.as_slice(), sigma.as_mut_slice());
        let split = if exec.nb_parts(nq, POINTS_PER_PART) > 1 {
            first_invalid_jacobian::<DIM>(exec, g_all, &c33)
        } else {
            0
        };
        if split > 0 {
            let items = (
                akantu_core::exec::TuplesRef::new(&g_all[..split * n], n),
                akantu_core::exec::Tuples::new(&mut s_all[..split * n], n),
                akantu_core::exec::TuplesRef::new(&c33[..split], 1),
            );
            exec.try_for_each(items, POINTS_PER_PART, |_, (g, mut s, c)| {
                stress_part(g.as_slice(), s.as_mut_slice(), c.as_slice())
            })?;
        }
        stress_part(&g_all[split * n..], &mut s_all[split * n..], &c33[split..])
    }

    /// A point's stress (and its plane-stress `C33`) reads only that
    /// point's gradient and stored `C33` and the Lamé constants. The first
    /// point with `J ≤ 0` (or without a `C33`) ends the loop with the error
    /// and the state of the serial loop (see `compute_stress`).
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
        if self.upstream_mode() {
            return Err(Error::NotImplemented(
                "computeStress : not implemented yet !",
            ));
        }
        let n = DIM * DIM;
        let v = tangent_stiffness_voigt_size(DIM);
        let grad_u = self.base.grad_u.get(ty, ghost)?;
        let nq = grad_u.len();
        check_tangent(tangent, nq, v)?;
        let c33 = self.c33_values(ty, ghost, nq)?;
        let plane_stress = self.plane_stress_active();
        for ((g, t), &x) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(tangent.as_mut_slice().chunks_exact_mut(v * v))
            .zip(&c33)
        {
            if plane_stress {
                neohookean_tangent_plane_stress(self.lambda, self.mu, as_matrix(g)?, x, t)?;
            } else {
                neohookean_tangent::<DIM>(self.lambda, self.mu, as_matrix(g)?, x, t)?;
            }
        }
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        if self.upstream_mode() {
            return Err(Error::NotImplemented(
                "computePotentialEnergy : not implemented yet !",
            ));
        }
        let n = DIM * DIM;
        let grad_u = self.base.grad_u.get(ty, NG)?.clone();
        let nq = grad_u.len();
        let c33 = self.c33_values(ty, NG, nq)?;
        let mut density = Vec::with_capacity(nq);
        for (g, &x) in grad_u.as_slice().chunks_exact(n).zip(&c33) {
            density.push(neohookean_energy::<DIM>(
                self.lambda,
                self.mu,
                as_matrix(g)?,
                x,
            )?);
        }
        let mut epot = self.base.potential_energy.get_mut(ty, NG)?;
        if epot.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialNeohookean::computePotentialEnergy",
                expected: nq,
                got: epot.len(),
            });
        }
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        Ok(((self.lambda + 2. * self.mu) / self.base.rho).sqrt())
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        Ok((self.mu / self.base.rho).sqrt())
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.options)
    }

    /// Plane stress: upstream `MaterialNeohookean<2>::computeCauchyStressPlaneStress`
    /// (`σ = StoCauchy(F, S, C33)`); otherwise `Material::computeAllCauchyStresses`.
    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        if !self.plane_stress_active() {
            return self.base.compute_all_cauchy_stresses(ghost);
        }
        for ty in self.base.filter_types(ghost) {
            let g = self.base.grad_u.get(ty, ghost)?.clone();
            let s = self
                .base
                .piola_kirchhoff_2
                .as_ref()
                .ok_or_else(|| Error::invalid("piola_kirchhoff_2 is not registered"))?
                .get(ty, ghost)?
                .clone();
            let nq = g.len();
            let c33 = self.c33_values(ty, ghost, nq)?;
            let mut sigma = self.base.stress.get_mut(ty, ghost)?;
            for (((out, gu), pk2), &x) in sigma
                .as_mut_slice()
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(g.as_slice().as_chunks::<4>().0)
                .zip(s.as_slice().as_chunks::<4>().0)
                .zip(&c33)
            {
                let gu: &Matrix<2, 2> = as_matrix(gu)?;
                let pk2: &Matrix<2, 2> = as_matrix(pk2)?;
                let c = s_to_cauchy(&grad_u_to_f(gu), pk2, x);
                out.copy_from_slice(c.as_flattened());
            }
        }
        Ok(())
    }
}

/// Register `neohookean` (upstream `instantiateMaterial<MaterialNeohookean>`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "neohookean",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialNeohookean::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
