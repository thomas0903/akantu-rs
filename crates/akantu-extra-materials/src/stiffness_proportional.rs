// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_viscoelastic/material_stiffness_proportional.hh,
// material_stiffness_proportional.cc.

//! `ve_stiffness_prop` (XMAT-013): stiffness-proportional (Kelvin–Voigt,
//! Caughey) viscoelasticity, `σ = C:ε + α·C:ε̇` (upstream
//! `MaterialStiffnessProportional<dim> : MaterialElastic<dim>`; `η = αE`,
//! the damped solid keeps the elastic eigen-modes, Caughey 1960).
//!
//! `computeStress(type, ghost)`: `MaterialElastic::computeStress` (thermal
//! pre-step, `σ_el = C:∇u + σ_th`), the velocity gradient `∇v`, then per
//! point `σ_visc = α·MaterialElastic::computeStressOnQuad(∇v)` (`σ_th = 0`),
//! `stress_elastic = σ_el`, `σ = σ_el + σ_visc` (internals
//! `stress_viscosity`, `stress_elastic`, `dim²`).
//!
//! `computePotentialEnergy`: the elastic part only, `½ σ_el : ∇u` (with
//! the port's stored-energy thermal correction of `elastic`, UD-109,
//! [`akantu_solid::materials::elastic::MaterialElastic::energy_density`];
//! bit-identical to upstream without a temperature change).
//! The tangent is the elastic one (the viscous term is not in the
//! stiffness, as upstream). Parameter `Alpha` (0, parsable-modifiable).

use akantu_core::tensor::{Matrix, as_matrix, as_matrix_mut};
use akantu_core::{Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{LawOptions, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, small_strain_only};

const NG: GhostType = GhostType::NotGhost;

/// `ve_stiffness_prop` (upstream `MaterialStiffnessProportional<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialStiffnessProportional<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// `stress_viscosity` (`dim²`).
    pub stress_viscosity: InternalField<Real>,
    /// `stress_elastic` (`dim²`).
    pub stress_elastic: InternalField<Real>,
    /// `Alpha`: viscous ratio.
    pub alpha: Real,
}

impl<const DIM: usize> MaterialStiffnessProportional<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "ve_stiffness_prop".into();
        Ok(Self {
            elastic,
            stress_viscosity: InternalField::new("stress_viscosity", DIM * DIM, 0.0)?,
            stress_elastic: InternalField::new("stress_elastic", DIM * DIM, 0.0)?,
            alpha: 0.,
        })
    }

    /// The viscous stress of one point: `α·C:∇v`.
    #[must_use]
    pub fn viscous_stress(&self, grad_v: &Matrix<DIM, DIM>) -> Matrix<DIM, DIM> {
        let mut s = [[0.; DIM]; DIM];
        self.elastic.constants().stress_on_quad(grad_v, 0., &mut s);
        for col in &mut s {
            for x in col {
                *x *= self.alpha;
            }
        }
        s
    }

    const PARAMS: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(
        Self,
        [elastic],
        [akantu_solid::real_param!(
            Self,
            "Alpha",
            ParameterAccess::PARSMOD,
            "Artificial viscous ratio",
            alpha
        )]
    );
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = akantu_solid::elastic_parent_params!(plane_stress Self, [elastic], [
        akantu_solid::real_param!(Self, "Alpha", ParameterAccess::PARSMOD, "Artificial viscous ratio", alpha),
    ]);

    fn densities(&self, ty: ElementType, points: std::ops::Range<usize>) -> Result<Vec<Real>> {
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, NG)?;
        let sigma_el = self.stress_elastic.get(ty, NG)?;
        let sigma_th = self.elastic.sigma_th.get(ty, NG)?;
        let delta_t = self.elastic.delta_t.get(ty, NG)?;
        let nq = grad_u.len();
        check_lengths(
            "MaterialStiffnessProportional::computePotentialEnergy",
            nq,
            &[sigma_el.len(), sigma_th.len(), delta_t.len()],
        )?;
        if points.end > nq {
            return Err(akantu_core::Error::DimensionMismatch {
                context: "MaterialStiffnessProportional: points",
                expected: nq,
                got: points.end,
            });
        }
        points
            .map(|q| {
                let r = q * n..(q + 1) * n;
                Ok(self.elastic.energy_density(
                    as_matrix(&sigma_el.as_slice()[r.clone()])?,
                    as_matrix(&grad_u.as_slice()[r])?,
                    sigma_th.as_slice()[q],
                    delta_t.as_slice()[q],
                ))
            })
            .collect()
    }
}

impl<const DIM: usize> MaterialParams for MaterialStiffnessProportional<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)
    }
}

akantu_solid::impl_material_registry!(MaterialStiffnessProportional<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialStiffnessProportional<DIM> {
    fn name(&self) -> &str {
        self.elastic.name()
    }
    fn law_id(&self) -> &str {
        self.elastic.law_id()
    }
    fn law_type(&self) -> &str {
        self.elastic.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.elastic.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.elastic.element_filter_mut()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        f(
            "stress_viscosity",
            InternalFieldRef::Real(&self.stress_viscosity),
        );
        f(
            "stress_elastic",
            InternalFieldRef::Real(&self.stress_elastic),
        );
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f(
            "stress_viscosity",
            InternalFieldMut::Real(&mut self.stress_viscosity),
        )?;
        f(
            "stress_elastic",
            InternalFieldMut::Real(&mut self.stress_elastic),
        )
    }
}

impl<const DIM: usize> Material for MaterialStiffnessProportional<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial`.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        MaterialParams::update_params(self)?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        MaterialParams::update_params(self)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        small_strain_only("ve_stiffness_prop", self.elastic.base().finite_deformation)?;
        self.elastic.compute_stress(ctx, ty, ghost)?;
        let n = DIM * DIM;
        let filter = self
            .elastic
            .base()
            .element_filter
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        let mut grad_v = Array::new(n);
        ctx.services.gradient_on_integration_points(
            "velocity",
            &mut grad_v,
            DIM,
            ty,
            ghost,
            &filter,
        )?;
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        let mut visc = self.stress_viscosity.get(ty, ghost)?.clone();
        let mut el = self.stress_elastic.get(ty, ghost)?.clone();
        check_lengths(
            "MaterialStiffnessProportional::computeStress",
            sigma.len(),
            &[grad_v.len(), visc.len(), el.len()],
        )?;
        check_lengths(
            "MaterialStiffnessProportional::computeStress: components",
            n,
            &[
                sigma.nb_component(),
                grad_v.nb_component(),
                visc.nb_component(),
                el.nb_component(),
            ],
        )?;
        // every point from its own stress and velocity gradient (copies of
        // the blocks): parts on the model's threads (D-123)
        let items = (
            akantu_core::exec::Tuples::new(sigma.as_mut_slice(), n),
            akantu_core::exec::TuplesRef::new(grad_v.as_slice(), n),
            akantu_core::exec::Tuples::new(visc.as_mut_slice(), n),
            akantu_core::exec::Tuples::new(el.as_mut_slice(), n),
        );
        let this = &*self;
        akantu_solid::material::points_exec(this, ctx).try_for_each(
            items,
            1024,
            |_, (mut s, v, mut sv, mut se)| {
                for (((s, v), sv), se) in s
                    .as_mut_slice()
                    .chunks_exact_mut(n)
                    .zip(v.as_slice().chunks_exact(n))
                    .zip(sv.as_mut_slice().chunks_exact_mut(n))
                    .zip(se.as_mut_slice().chunks_exact_mut(n))
                {
                    let sigma_visc = this.viscous_stress(as_matrix(v)?);
                    as_matrix_mut::<DIM, DIM>(sv)?.clone_from(&sigma_visc);
                    se.copy_from_slice(s);
                    for (x, y) in s.iter_mut().zip(sigma_visc.as_flattened()) {
                        *x += *y;
                    }
                }
                Ok(())
            },
        )?;
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.stress_viscosity.get_mut(ty, ghost)?.copy_from(&visc)?;
        self.stress_elastic.get_mut(ty, ghost)?.copy_from(&el)?;
        Ok(())
    }

    /// A point's viscous stress reads only its own velocity gradient and
    /// the law's constants; its elastic part is the elastic law's (threaded
    /// as that law's).
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        small_strain_only("ve_stiffness_prop", self.elastic.base().finite_deformation)?;
        self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
    }

    /// Upstream `computePotentialEnergy`: `½ σ_el : ∇u` per not-ghost point.
    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        let nq = self.elastic.base().grad_u.get(ty, NG)?.len();
        let density = self.densities(ty, 0..nq)?;
        let mut epot = self.elastic.base_mut().potential_energy.get_mut(ty, NG)?;
        check_lengths(
            "MaterialStiffnessProportional: potential_energy",
            nq,
            &[epot.len()],
        )?;
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points =
            akantu_solid::materials::damage::element_points(self.elastic.base(), ctx, element)?;
        self.densities(element.ty(), points)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> akantu_core::enums::MatrixType {
        akantu_core::enums::MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        self.elastic.has_stiffness_matrix_changed()
    }

    fn mark_stiffness_changed(&mut self) {
        self.elastic.mark_stiffness_changed();
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.elastic.compute_all_cauchy_stresses(ghost)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

/// Register `ve_stiffness_prop` (upstream
/// `INSTANTIATE_MATERIAL(ve_stiffness_prop, MaterialStiffnessProportional)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    crate::damage_law::register_law!(factory, "ve_stiffness_prop", MaterialStiffnessProportional)
}
