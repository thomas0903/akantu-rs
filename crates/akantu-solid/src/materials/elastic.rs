// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_elastic.hh, material_elastic.cc, material_elastic_inline_impl.hh,
// material_thermal.hh, material_thermal.cc, plane_stress_toolbox.hh,
// plane_stress_toolbox_tmpl.hh.

//! `elastic`: isotropic linear elasticity (upstream `MaterialElastic<dim>`,
//! class chain `MaterialElastic → MaterialThermal → PlaneStressToolbox →
//! Material`), all spatial dimensions, plane strain (default) and plane
//! stress (`Plane_Stress = true`, 2-D), with the thermal pre-step and the
//! St-Venant–Kirchhoff variant (`finite_deformation = true`: the same
//! kernel on the Green–Lagrange strain, result stored as the second
//! Piola–Kirchhoff stress).
//!
//! Per quadrature point (upstream `material_elastic_inline_impl.hh`, operand
//! order kept for bit parity, decision D-008):
//!
//! * dim ≥ 2: `σ(i, j) = (2μ)·ε(i, j) + (λ·tr ∇u + σ_th)·I(i, j)` with
//!   `ε = .5·(∇uᵀ + ∇u)` and `tr` the halving-tree sum of the diagonal;
//! * dim 1: `σ = E·∇u + σ_th`;
//! * tangent (Voigt, column-major per quadrature point): zero, then
//!   `λ + 2μ` on the normal diagonal, `λ` off-diagonal normal, `μ` on the
//!   shear diagonal (`[E]` in 1-D);
//! * energy density `.5·(σ : ∇u)` (`cwiseProduct(..).sum()`, halving tree)
//!   plus, with a thermal stress, the UD-109 correction
//!   `½ σ_th·(tr ∇u − m·αΔT)` that makes it the stored elastic energy, a
//!   potential of the stress at fixed temperature (default; upstream's
//!   `.5·(σ : ∇u)` with [`ThermalEnergy::Upstream`], see
//!   [`super::thermal`]);
//! * wave speeds `√((λ + 2μ)/ρ)`, `√(μ/ρ)`; 1-D: `√(E/ρ)`, no shear wave.
//!
//! Derived parameters (`updateInternalParameters`, after every `set` and at
//! the end of every parsed section): `λ = νE/((1 + ν)(1 − 2ν))`,
//! `μ = E/(2(1 + ν))`, plane stress `λ = νE/((1 + ν)(1 − ν))`,
//! `κ = λ + (2/3)μ`; `ν` is forced to 0 in 1-D by `initMaterial`.

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};

use super::scratch::ScratchBlocks;
use super::thermal::{ThermalEnergy, ThermalParams, thermal_energy_correction, thermal_on_quad};
use crate::material::{
    LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams, check_tangent,
};
use crate::material_tensor::{
    grad_u_to_epsilon, grad_u_to_green, isotropic_stress_coeff, tangent_stiffness_voigt_size,
};

/// Isotropic linear elastic law (upstream `MaterialElastic<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialElastic<const DIM: usize> {
    base: MaterialBase,
    /// `Plane_Stress` (2-D only; upstream `PlaneStressToolbox<2>`).
    pub plane_stress: bool,
    /// Third-axis strain measure `C33` (2-D only, default 1; upstream
    /// `third_axis_deformation`, unused by `elastic`).
    pub third_axis_deformation: Option<InternalField<Real>>,
    /// `E`, `nu`, `alpha`.
    pub thermal: ThermalParams,
    /// Temperature change per quadrature point (`delta_T`).
    pub delta_t: InternalField<Real>,
    /// Thermal stress (history).
    pub sigma_th: InternalField<Real>,
    /// Thermal strain (history).
    pub epsilon_th: InternalField<Real>,
    /// Upstream-defect switches (UD-038: `thermal_strain`; UD-109:
    /// `thermal_energy`).
    pub options: LawOptions,
    lambda: Real,
    mu: Real,
    kpa: Real,
    was_stiffness_assembled: bool,
    /// The potential energy density of a block before it is committed
    /// (`compute_potential_energy`; kept between calls).
    energy_scratch: ScratchBlocks<1>,
}

impl<const DIM: usize> MaterialElastic<DIM> {
    /// New law (upstream constructor chain). `mesh` initialises the element
    /// filter blocks; `None` leaves the filter empty.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        if spec.spatial_dimension != DIM {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic",
                expected: DIM,
                got: spec.spatial_dimension,
            });
        }
        let base = match mesh {
            Some(m) => MaterialBase::with_mesh(spec, "elastic", m)?,
            None => MaterialBase::new(spec, "elastic")?,
        };
        let third_axis_deformation = if DIM == 2 {
            Some(InternalField::new("third_axis_deformation", 1, 1.0)?)
        } else {
            None
        };
        let mut sigma_th = InternalField::new("sigma_th", 1, 0.0)?;
        sigma_th.initialize_history();
        let mut epsilon_th = InternalField::new("epsilon_th", 1, 0.0)?;
        epsilon_th.initialize_history();
        Ok(Self {
            base,
            plane_stress: false,
            third_axis_deformation,
            thermal: ThermalParams::default(),
            delta_t: InternalField::new("delta_T", 1, 0.0)?,
            sigma_th,
            epsilon_th,
            options: LawOptions::default(),
            lambda: 0.0,
            mu: 0.0,
            kpa: 0.0,
            was_stiffness_assembled: false,
            energy_scratch: ScratchBlocks::default(),
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

    /// Bulk modulus `λ + (2/3)μ` (upstream `kapa`).
    #[must_use]
    pub fn kappa(&self) -> Real {
        self.kpa
    }

    /// Upstream `MaterialElastic<dim>::updateInternalParameters`.
    fn update(&mut self) -> Result<()> {
        // MaterialThermal / PlaneStressToolbox do not override it:
        // Material::updateInternalParameters.
        self.base.update_internal_parameters()?;
        let (e, nu) = (self.thermal.e, self.thermal.nu);
        self.lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        self.mu = e / (2. * (1. + nu));
        if DIM == 2 && self.plane_stress {
            self.lambda = nu * e / ((1. + nu) * (1. - nu));
        }
        self.kpa = self.lambda + 2. / 3. * self.mu;
        self.was_stiffness_assembled = false;
        Ok(())
    }

    /// The constants used by the stress kernel.
    #[must_use]
    pub fn constants(&self) -> ElasticConstants<DIM> {
        ElasticConstants {
            e: self.thermal.e,
            lambda: self.lambda,
            mu: self.mu,
        }
    }

    /// Upstream `computeStressOnQuad` for one quadrature point (see
    /// [`ElasticConstants::stress_on_quad`]).
    pub fn stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma_th: Real,
        sigma: &mut Matrix<DIM, DIM>,
    ) {
        self.constants().stress_on_quad(grad_u, sigma_th, sigma);
    }

    /// Upstream `computeTangentModuliOnQuad`: `out` is the column-major
    /// `voigt × voigt` tangent of one quadrature point.
    pub fn tangent_on_quad(&self, out: &mut [Real]) {
        let n = tangent_stiffness_voigt_size(DIM);
        debug_assert_eq!(out.len(), n * n);
        out.fill(0.0);
        let at = |i: usize, j: usize| i + j * n;
        if DIM == 1 {
            out[0] = self.thermal.e;
            return;
        }
        let miiii = self.lambda + 2. * self.mu;
        let miijj = self.lambda;
        let mijij = self.mu;
        out[at(0, 0)] = miiii;
        out[at(1, 1)] = miiii;
        out[at(0, 1)] = miijj;
        out[at(1, 0)] = miijj;
        out[at(n - 1, n - 1)] = mijij;
        if DIM == 3 {
            out[at(2, 2)] = miiii;
            out[at(0, 2)] = miijj;
            out[at(1, 2)] = miijj;
            out[at(2, 0)] = miijj;
            out[at(2, 1)] = miijj;
            out[at(3, 3)] = mijij;
            out[at(4, 4)] = mijij;
        }
    }

    /// Upstream `computePotentialEnergyOnQuad`: `.5·(σ : ∇u)`.
    #[must_use]
    pub fn energy_on_quad(sigma: &Matrix<DIM, DIM>, grad_u: &Matrix<DIM, DIM>) -> Real {
        0.5 * sigma.double_dot(grad_u)
    }

    /// Potential energy density of one quadrature point in the law's
    /// [`ThermalEnergy`] convention (UD-109): `strain` is `∇u` (the Green
    /// strain with finite deformation), `sigma` the stress computed from it
    /// (PK2 with finite deformation), `sigma_th` and `delta_t` the point's
    /// thermal internals. Without thermal stress the result is upstream's
    /// [`MaterialElastic::energy_on_quad`], bit for bit.
    #[must_use]
    pub fn energy_density(
        &self,
        sigma: &Matrix<DIM, DIM>,
        strain: &Matrix<DIM, DIM>,
        sigma_th: Real,
        delta_t: Real,
    ) -> Real {
        let w = Self::energy_on_quad(sigma, strain);
        if self.options.thermal_energy == ThermalEnergy::Upstream || sigma_th == 0.0 {
            return w;
        }
        w + thermal_energy_correction::<DIM>(
            self.plane_stress,
            sigma_th,
            strain.trace(),
            delta_t * self.thermal.alpha,
        )
    }

    /// Energy density of a point from the stored `grad_u` (Green strain
    /// with finite deformation) and stress.
    fn point_energy(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma: &Matrix<DIM, DIM>,
        sigma_th: Real,
        delta_t: Real,
    ) -> Real {
        if self.base.finite_deformation {
            self.energy_density(sigma, &grad_u_to_green(grad_u), sigma_th, delta_t)
        } else {
            self.energy_density(sigma, grad_u, sigma_th, delta_t)
        }
    }

    /// Record that the stiffness was assembled (end of upstream
    /// `computeTangentModuli` of laws deriving from `MaterialElastic`, e.g.
    /// `MaterialLinearIsotropicHardening`), so that
    /// [`Material::has_stiffness_matrix_changed`] reports `false` until the
    /// next parameter update.
    pub(crate) fn set_stiffness_assembled(&mut self) {
        self.was_stiffness_assembled = true;
    }

    /// Upstream `MaterialThermal<dim>::computeStress` for one block (also
    /// the thermal pre-step of the laws deriving from `MaterialElastic`).
    pub(crate) fn compute_thermal(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        self.compute_thermal_with(akantu_core::exec::Exec::serial_ref(), ty, ghost)
    }

    /// [`Self::compute_thermal`] with the per-point loop split by `exec`
    /// (every point from its own `ΔT`).
    pub(crate) fn compute_thermal_with(
        &mut self,
        exec: &akantu_core::exec::Exec,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let dt = self.delta_t.get(ty, ghost)?;
        let (mut eps, mut sig) = (
            self.epsilon_th.get_mut(ty, ghost)?,
            self.sigma_th.get_mut(ty, ghost)?,
        );
        if dt.len() != eps.len() || dt.len() != sig.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialThermal::computeStress",
                expected: dt.len(),
                got: eps.len().min(sig.len()),
            });
        }
        let (thermal, plane_stress, thermal_strain) = (
            &self.thermal,
            self.plane_stress,
            self.options.thermal_strain,
        );
        let items = (
            TuplesRef::new(dt.as_slice(), 1),
            Tuples::new(eps.as_mut_slice(), 1),
            Tuples::new(sig.as_mut_slice(), 1),
        );
        exec.for_each(items, 8192, |_, (dt, mut eps, mut sig)| {
            thermal_part::<DIM>(
                thermal,
                plane_stress,
                thermal_strain,
                dt.as_slice(),
                eps.as_mut_slice(),
                sig.as_mut_slice(),
            );
        });
        Ok(())
    }
}

/// The thermal strain and stress of the points of one part (no-alias
/// parameters, as [`stress_part`]).
#[inline(never)]
fn thermal_part<const DIM: usize>(
    thermal: &crate::materials::thermal::ThermalParams,
    plane_stress: bool,
    convention: crate::materials::thermal::ThermalStrain,
    delta_t: &[Real],
    epsilon_th: &mut [Real],
    sigma_th: &mut [Real],
) {
    for ((d, e), s) in delta_t.iter().zip(epsilon_th).zip(sigma_th) {
        (*e, *s) = thermal_on_quad::<DIM>(thermal, plane_stress, convention, *d);
    }
}

/// `computeStress` of the points of one part (`grad_u`, `sigma` and
/// `sigma_th` of the same points). Not inlined into the part closure, so
/// that its slices are no-alias parameters.
#[inline(never)]
fn stress_part<const DIM: usize>(
    constants: &ElasticConstants<DIM>,
    finite: bool,
    grad_u: &[Real],
    sigma: &mut [Real],
    sigma_th: &[Real],
) -> Result<()> {
    let n = DIM * DIM;
    for ((g, s), &th) in grad_u
        .chunks_exact(n)
        .zip(sigma.chunks_exact_mut(n))
        .zip(sigma_th)
    {
        let g: &Matrix<DIM, DIM> = as_matrix(g)?;
        let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
        if finite {
            constants.stress_on_quad(&grad_u_to_green(g), th, s);
        } else {
            constants.stress_on_quad(g, th, s);
        }
    }
    Ok(())
}

/// Constants of the isotropic stress kernel (`E`, `λ`, `μ`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElasticConstants<const DIM: usize> {
    /// Young's modulus (1-D kernel).
    pub e: Real,
    /// First Lamé coefficient.
    pub lambda: Real,
    /// Shear modulus.
    pub mu: Real,
}

impl<const DIM: usize> ElasticConstants<DIM> {
    /// Upstream `MaterialElastic<dim>::computeStressOnQuad`: `grad_u` (or
    /// the Green strain with finite deformation), the thermal stress
    /// `sigma_th`, output `sigma` (all `DIM × DIM`, column-major).
    pub fn stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma_th: Real,
        sigma: &mut Matrix<DIM, DIM>,
    ) {
        if DIM == 1 {
            sigma[0][0] = self.e * grad_u[0][0] + sigma_th;
            return;
        }
        let trace = grad_u.trace();
        let eps = grad_u_to_epsilon(grad_u);
        let two_mu = 2. * self.mu;
        let diag = self.lambda * trace + sigma_th;
        for (j, col) in sigma.iter_mut().enumerate() {
            for (i, s) in col.iter_mut().enumerate() {
                *s = isotropic_stress_coeff(two_mu, eps[j][i], diag, i, j);
            }
        }
    }
}

impl<const DIM: usize> MaterialParams for MaterialElastic<DIM> {
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

macro_rules! elastic_params {
    ($($extra:expr),*) => {
        &[
            $($extra,)*
            crate::real_param!(Self, "E", ParameterAccess::PARSMOD, "Young's modulus", thermal.e),
            crate::real_param!(Self, "nu", ParameterAccess::PARSMOD, "Poisson's ratio", thermal.nu),
            crate::real_param!(Self, "alpha", ParameterAccess::PARSMOD, "Thermal expansion coefficient", thermal.alpha),
            ParamDescriptor::new(
                "delta_T",
                ParamKind::Real,
                ParameterAccess::PARSMOD,
                "Uniform temperature field",
                |s| ParamValue::Real(s.delta_t.parameter_value()),
                Some(|s, v| {
                    s.delta_t.set_default_value(v.as_real()?);
                    Ok(())
                }),
            ),
            crate::readable_real_param!(Self, "lambda", "First Lamé coefficient", lambda),
            crate::readable_real_param!(Self, "mu", "Second Lamé coefficient", mu),
            crate::readable_real_param!(Self, "kapa", "Bulk coefficient", kpa),
        ]
    };
}

impl<const DIM: usize> MaterialElastic<DIM> {
    /// Parameters of the 1-D and 3-D laws (thermal then elastic).
    const PARAMS: &'static [ParamDescriptor<Self>] = elastic_params!();
    /// Parameters of the 2-D law (`Plane_Stress` first, as the toolbox is
    /// the base class).
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = elastic_params!(ParamDescriptor::new(
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

crate::impl_material_registry!(MaterialElastic<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialElastic<DIM> {
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
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(self.base.internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.base.internals() {
            f(n, InternalFieldRef::Real(field));
        }
        if let Some(t) = &self.third_axis_deformation {
            f("third_axis_deformation", InternalFieldRef::Real(t));
        }
        f("delta_T", InternalFieldRef::Real(&self.delta_t));
        f("sigma_th", InternalFieldRef::Real(&self.sigma_th));
        f("epsilon_th", InternalFieldRef::Real(&self.epsilon_th));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        let names = [
            "stress",
            "eigen_grad_u",
            "grad_u",
            "potential_energy",
            "piola_kirchhoff_2",
            "green_strain",
        ];
        let mut fields = self.base.internals_mut().into_iter();
        f(
            names[0],
            InternalFieldMut::Real(fields.next().ok_or_else(missing)?),
        )?;
        f(
            names[1],
            InternalFieldMut::Real(fields.next().ok_or_else(missing)?),
        )?;
        f(
            names[2],
            InternalFieldMut::Real(fields.next().ok_or_else(missing)?),
        )?;
        f(
            names[3],
            InternalFieldMut::Real(fields.next().ok_or_else(missing)?),
        )?;
        for field in fields {
            let n = if field.name() == "piola_kirchhoff_2" {
                names[4]
            } else {
                names[5]
            };
            f(n, InternalFieldMut::Real(field))?;
        }
        if let Some(t) = &mut self.third_axis_deformation {
            f("third_axis_deformation", InternalFieldMut::Real(t))?;
        }
        f("delta_T", InternalFieldMut::Real(&mut self.delta_t))?;
        f("sigma_th", InternalFieldMut::Real(&mut self.sigma_th))?;
        f("epsilon_th", InternalFieldMut::Real(&mut self.epsilon_th))?;
        Ok(())
    }
}

fn missing() -> Error {
    Error::invalid("material base internals are incomplete")
}

impl<const DIM: usize> MaterialElastic<DIM> {
    /// The law's real internals besides the [`MaterialBase`] ones, by name
    /// (for runtime laws derived from it, [`crate::derived`]).
    pub(crate) fn own_internals(&self) -> Vec<(&'static str, &InternalField<Real>)> {
        let mut v = Vec::new();
        if let Some(t) = &self.third_axis_deformation {
            v.push(("third_axis_deformation", t));
        }
        v.push(("delta_T", &self.delta_t));
        v.push(("sigma_th", &self.sigma_th));
        v.push(("epsilon_th", &self.epsilon_th));
        v
    }

    /// Mutable entry of [`MaterialElastic::own_internals`].
    pub(crate) fn own_internal_mut(&mut self, name: &str) -> Option<&mut InternalField<Real>> {
        match name {
            "third_axis_deformation" => self.third_axis_deformation.as_mut(),
            "delta_T" => Some(&mut self.delta_t),
            "sigma_th" => Some(&mut self.sigma_th),
            "epsilon_th" => Some(&mut self.epsilon_th),
            _ => None,
        }
    }
}

impl<const DIM: usize> Material for MaterialElastic<DIM> {
    /// Downcast (runtime laws derived from the elastic law reach its own
    /// internals, [`crate::derived`]).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn base(&self) -> &MaterialBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }

    /// Upstream `MaterialElastic::initMaterial`: `Material::initMaterial`
    /// (history, `resizeInternals`, `updateInternalParameters`), then
    /// `ν = 0` in 1-D and `updateInternalParameters` again.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base.init_history()?;
        let blocks = self.base.blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base.set_init();
        if DIM == 1 {
            self.thermal.nu = 0.;
        }
        self.update()
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        // every point from its own data: the loops split over the model's
        // threads (D-123)
        let exec = crate::material::points_exec(self, ctx);
        self.compute_thermal_with(exec, ty, ghost)?;
        let constants = self.constants();
        let n = DIM * DIM;
        let Self { base, sigma_th, .. } = self;
        let sigma_th = sigma_th.get(ty, ghost)?;
        let finite = base.finite_deformation;
        // Disjoint borrows of `grad_u` and the stress field
        // (`sigma_field_mut` without its borrow of the whole base) instead
        // of a per-step copy of `grad_u`
        let MaterialBase {
            grad_u,
            stress,
            piola_kirchhoff_2,
            ..
        } = base;
        let grad_u = grad_u.get(ty, ghost)?;
        let mut sigma = if finite {
            piola_kirchhoff_2.as_mut().ok_or_else(|| {
                Error::invalid(
                    "finite_deformation: piola_kirchhoff_2 is not registered (init the material first)",
                )
            })?
        } else {
            stress
        }
        .get_mut(ty, ghost)?;
        if grad_u.len() != sigma.len()
            || sigma_th.len() != sigma.len()
            || grad_u.nb_component() != n
            || sigma.nb_component() != n
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic::computeStress",
                expected: grad_u.len(),
                got: sigma.len(),
            });
        }
        let items = (
            TuplesRef::new(grad_u.as_slice(), n),
            Tuples::new(sigma.as_mut_slice(), n),
            TuplesRef::new(sigma_th.as_slice(), 1),
        );
        exec.try_for_each(items, 2048, |_, (g, mut s, th)| {
            stress_part(
                &constants,
                finite,
                g.as_slice(),
                s.as_mut_slice(),
                th.as_slice(),
            )
        })
    }

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
        let nq = self.base.grad_u.get(ty, ghost)?.len();
        let n = tangent_stiffness_voigt_size(DIM);
        check_tangent(tangent, nq, n)?;
        for t in tangent.as_mut_slice().chunks_exact_mut(n * n) {
            self.tangent_on_quad(t);
        }
        self.was_stiffness_assembled = true;
        Ok(())
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let g = GhostType::NotGhost;
        let n = DIM * DIM;
        // the inputs read in place, the densities into the law's scratch
        // block (every value written), committed by exchange
        let mut scratch = std::mem::take(&mut self.energy_scratch);
        let grad_u = self.base.grad_u.get(ty, g)?;
        let sigma = self.base.sigma_field()?.get(ty, g)?;
        let sigma_th = self.sigma_th.get(ty, g)?;
        let delta_t = self.delta_t.get(ty, g)?;
        let nq = grad_u.len();
        if sigma.len() != nq || sigma_th.len() != nq || delta_t.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic::computePotentialEnergy",
                expected: nq,
                got: sigma.len().min(sigma_th.len()).min(delta_t.len()),
            });
        }
        let [density] = scratch.sized([(nq, 1)])?;
        let mut written = 0;
        for ((((gu, s), &th), &dt), d) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(sigma.as_slice().chunks_exact(n))
            .zip(sigma_th.as_slice())
            .zip(delta_t.as_slice())
            .zip(density.as_mut_slice())
        {
            *d = self.point_energy(as_matrix(gu)?, as_matrix(s)?, th, dt);
            written += 1;
        }
        drop((grad_u, sigma, sigma_th, delta_t));
        let mut epot = self.base.potential_energy.get_mut(ty, g)?;
        if epot.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialElastic::computePotentialEnergy: potential_energy",
                expected: nq,
                got: epot.len(),
            });
        }
        if written == nq && epot.nb_component() == 1 {
            drop(epot);
            self.base
                .potential_energy
                .commit_overwrite(ty, g, scratch.get_mut(0))?;
        } else {
            // the values computed (as many as the shortest input), as before
            epot.as_mut_slice()[..written]
                .copy_from_slice(&scratch.get_mut(0).as_slice()[..written]);
        }
        self.energy_scratch = scratch;
        Ok(())
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let ty = element.ty();
        let nq = ctx.services.nb_integration_points(ty, GhostType::NotGhost);
        let n = DIM * DIM;
        let start = element.idx() * nq * n;
        let range = start..start + nq * n;
        let scalar_range = element.idx() * nq..(element.idx() + 1) * nq;
        let grad_u = self.base.grad_u.get(ty, GhostType::NotGhost)?;
        let sigma = self.base.sigma_field()?.get(ty, GhostType::NotGhost)?;
        let sigma_th = self.sigma_th.get(ty, GhostType::NotGhost)?;
        let delta_t = self.delta_t.get(ty, GhostType::NotGhost)?;
        let (Some(gu), Some(s), Some(th), Some(dt)) = (
            grad_u.as_slice().get(range.clone()),
            sigma.as_slice().get(range),
            sigma_th.as_slice().get(scalar_range.clone()),
            delta_t.as_slice().get(scalar_range),
        ) else {
            return Err(Error::InvalidIndex {
                what: "law-local element",
                value: i64::from(element.index()),
            });
        };
        gu.chunks_exact(n)
            .zip(s.chunks_exact(n))
            .zip(th.iter().zip(dt))
            .map(|((gu, s), (&th, &dt))| {
                Ok(self.point_energy(as_matrix(gu)?, as_matrix(s)?, th, dt))
            })
            .collect()
    }

    fn push_wave_speed(&self, _element: Element) -> Result<Real> {
        if DIM == 1 {
            return Ok((self.thermal.e / self.base.rho).sqrt());
        }
        Ok(((self.lambda + 2. * self.mu) / self.base.rho).sqrt())
    }

    fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
        if DIM == 1 {
            return Err(Error::invalid("There is no shear wave speed in 1D"));
        }
        Ok((self.mu / self.base.rho).sqrt())
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        !self.was_stiffness_assembled
    }

    fn mark_stiffness_changed(&mut self) {
        self.was_stiffness_assembled = false;
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.options)
    }

    /// `compute_stress` writes every `epsilon_th`, `sigma_th` and stress
    /// (PK2) value of the block from `delta_T`, `grad_u` and `sigma_th`
    /// before reading it, and fails only on the shapes checked here.
    fn prepare_stress_overwrite(&mut self, ty: ElementType, ghost: GhostType) {
        if !self.prepare_thermal_overwrite(ty, ghost) {
            return;
        }
        let n = shape(&self.base.grad_u, ty, ghost);
        if shape(&self.sigma_th, ty, ghost).0 != n.0 {
            return;
        }
        if let Ok(sigma) = self.base.sigma_field_mut()
            && n.1 == DIM * DIM
            && shape(sigma, ty, ghost) == n
        {
            let _ = sigma.prepare_overwrite(ty, ghost);
        }
    }

    /// 2-D plane stress: upstream `PlaneStressToolbox<2>::computeAllCauchyStresses`
    /// calls the (empty) `computeCauchyStressPlaneStress` for `elastic`.
    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        if DIM == 2 && self.plane_stress {
            if !self.base.finite_deformation {
                return Err(Error::invalid(
                    "The Cauchy stress can only be computed if you are working in finite deformation.",
                ));
            }
            return Ok(());
        }
        self.base.compute_all_cauchy_stresses(ghost)
    }
}

/// `(tuples, components)` of a block (`(usize::MAX, 0)` if missing).
fn shape(f: &InternalField<Real>, ty: ElementType, ghost: GhostType) -> (usize, usize) {
    f.values()
        .shared(ty, ghost)
        .map_or((usize::MAX, 0), |a| (a.len(), a.nb_component()))
}

impl<const DIM: usize> MaterialElastic<DIM> {
    /// [`InternalField::prepare_overwrite`] of the `epsilon_th` and
    /// `sigma_th` blocks before [`Self::compute_thermal`], which writes every
    /// value of both from `delta_T` when the three blocks have the same
    /// length and one component (checked here; `false`, nothing done,
    /// otherwise).
    pub(crate) fn prepare_thermal_overwrite(&mut self, ty: ElementType, ghost: GhostType) -> bool {
        let n = shape(&self.delta_t, ty, ghost);
        if n.1 != 1
            || shape(&self.epsilon_th, ty, ghost) != n
            || shape(&self.sigma_th, ty, ghost) != n
        {
            return false;
        }
        let _ = self.epsilon_th.prepare_overwrite(ty, ghost);
        let _ = self.sigma_th.prepare_overwrite(ty, ghost);
        true
    }
}

/// The built-in `elastic` law of `spec` (the base of a runtime law derived
/// from it, [`crate::DerivedMaterial`]; the factory allocator's law).
///
/// # Errors
/// An unsupported dimension, the law's construction errors.
pub fn new_elastic_base(
    spec: &LawSpec,
    services: &dyn ModelServices,
) -> Result<Box<dyn Material + Sync>> {
    let mesh = Some(services.mesh());
    akantu_core::with_dim!(spec.spatial_dimension, D => {
        Box::new(MaterialElastic::<D>::new(spec, mesh)?) as Box<dyn Material + Sync>
    })
}

/// Register `elastic` (upstream `instantiateMaterial<MaterialElastic>("elastic")`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "elastic",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialElastic::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}
