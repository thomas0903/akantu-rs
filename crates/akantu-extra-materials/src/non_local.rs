// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_damage_iterative_non_local{.hh,.cc,_inline_impl.hh},
// material_brittle_non_local{.hh,_inline_impl.hh},
// material_vreepeerlings_non_local{.hh,.cc,_inline_impl.hh},
// material_orthotropic_damage_iterative_non_local{.hh,_inline_impl.hh},
// material_orthotropic_damage_non_local.hh, src/material_non_local_extra_includes.hh.

//! Non-local variants of the extra damage laws (XMAT-002, XMAT-006,
//! XMAT-008, XMAT-011) on the non-local framework of `akantu-solid`
//! (integrated).
//!
//! Each wrapper implements
//! [`akantu_solid::materials::non_local::MaterialNonLocal`] (upstream
//! `MaterialDamageNonLocal<dim, Local>` = `MaterialNonLocalInterface` +
//! `ConstitutiveLawNonLocalInterface`) and returns it from
//! [`Material::as_non_local`], so the solid model's `NonLocalManager`
//! drives it exactly as the damage laws of `akantu-solid`: at `init_full`
//! the model creates the manager (a law returns `Some`), registers the
//! law's neighbourhood under the law's name with its declared variables
//! (`non_local <law name> <weight function> [ … ]` sections; per
//! neighbourhood by default, UD-558 / D-062), inserts the law's integration
//! points; at every residual assembly it runs the local pass
//! (`computeAllStresses`), averages the declared local internals into the
//! non-local ones, calls [`MaterialNonLocal::compute_non_local_stresses`]
//! and then issues the reductions the laws declare
//! ([`MaterialNonLocal::reduce_after_non_local_stresses`]):
//!
//! | Keyword | Local pass (`computeStress`) | Averaged variable | Non-local pass |
//! |---|---|---|---|
//! | `damage_iterative_non_local` | nothing | `grad_u` → `grad_u non local` (`dim²`) | elastic stress `·(1 − d)`, equivalent stress from the averaged `∇u`, maximum (reset before / `Max` all-reduce declared after the pass) |
//! | `brittle_non_local` | brittle stress without damage; `Sigma max`, `Sigma_fracture` | `Sigma max` → `Sigma_max non local` | brittle damage criterion with the averaged `σ_eq` |
//! | `vreepeerlings_non_local` | Vree–Peerlings equivalent strains without damage | `equi-strain`, `equi-strain-rate` → `equi-strain_non_local`, `equi-strain-rate_non_local` | damage update with the averaged values |
//! | `damage_orthotropic_iterative_non_local` | nothing | `grad_u` → `grad_u non local` | orthotropic stress, equivalent stress from the averaged `∇u`, maximum (`Max` all-reduce declared after the pass, UD-1005) |
//!
//! The variants are registered only by `register_extra_non_local_materials`
//! (not by `register_extra_materials`; crate feature `damage-non-local`,
//! which forwards to `akantu-solid/damage-non-local`, off by default like
//! upstream's ADVANCED package `extra_materials_non_local`, which depends on
//! `damage_non_local`). The non-local stress pass skips empty element blocks
//! and non-regular kinds ([`non_local_pass_types`]). Like every non-local
//! law they run on distributed meshes through the solid model's grid
//! synchronisers (TEST-187 in `tests/non_local_parallel.rs`); the
//! orthotropic variant refuses UD-1005's compat with UD-870's default there.
//!
//! Upstream defects (UD-572): the classes lack the override of the pure
//! virtual `registerNonLocalVariables()` (the manager-level
//! `registerNonLocalVariable(variable_name, nl_variable_name, nb_component)`
//! exists; the removed call is `nonLocalVariableToNeighborhood`, which
//! associated a variable with a neighbourhood; UD-1002), the
//! `aka_debug_tools.hh` header, `brittle_non_local` has no `.cc` (never
//! registered; legacy-list keyword), `vreepeerlings_non_local` has its
//! registration commented out, and `material_orthotropic_damage_non_local.hh`
//! (an even older `MaterialNonLocal` API) is superseded by the iterative
//! variant. Keywords follow the legacy material lists. The readings of the
//! four classes against the port are UD-1002…1006.

use akantu_core::prelude::ElementKind;
use akantu_core::tensor::{Matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result, UInt,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawReducer, LawSpec,
};
use akantu_model::non_local::NonLocalVariable;
use akantu_parser::param::ParamDescriptor;
use akantu_solid::Material;
use akantu_solid::material::{DerivedMaterialParams, LawOptions, MaterialBase};
use akantu_solid::materials::non_local::MaterialNonLocal;

use crate::brittle::MaterialBrittle;
use crate::damage_iterative::{IterativeDamage, MaterialDamageIterative, all_stresses};
use crate::kernels::{check_lengths, scale_by_integrity};
use crate::orthotropic_damage::MaterialOrthotropicDamageIterative;
use crate::vreepeerlings::MaterialVreePeerlings;

const NG: GhostType = GhostType::NotGhost;

/// One declared non-local variable (upstream
/// `registerNonLocalVariable(local, non_local, nb_component)`).
fn variable(local: &str, non_local: &str, nb_component: usize) -> NonLocalVariable {
    NonLocalVariable {
        local: local.into(),
        non_local: non_local.into(),
        nb_component,
    }
}

/// The element types the non-local stress pass visits: the regular kinds of
/// the law's filter with at least one element (empty blocks and
/// non-regular kinds are skipped, as the other non-local laws do;
/// upstream `MaterialDamageNonLocal::computeNonLocalStresses`
/// skips the empty filters).
pub fn non_local_pass_types<M: Material + ?Sized>(law: &M, ghost: GhostType) -> Vec<ElementType> {
    let base = law.base();
    base.filter_types(ghost)
        .into_iter()
        .filter(|&ty| {
            ty.kind() == ElementKind::Regular
                && base
                    .element_filter
                    .get(ty, ghost)
                    .is_ok_and(|f| !f.is_empty())
        })
        .collect()
}

/// The `Max` all-reduce of a maximum normalised equivalent stress through
/// the model's reducer (nothing on one rank).
fn all_reduce_max(v: &mut Real, reducer: &mut dyn LawReducer) -> Result<()> {
    if reducer.nb_ranks() <= 1 {
        return Ok(());
    }
    let mut buf = [*v];
    reducer.all_reduce_real(&mut buf, akantu_comm::communicator::ReduceOp::Max)?;
    *v = buf[0];
    Ok(())
}

/// Option combinations a wrapper refuses on a mesh distributed over more
/// than one rank, decided at `init_material` (`init_full`; the model agrees
/// an `init_material` failure on every rank). `xn11_*` of
/// the orthotropic wrapper, also checked (agreed) at `update_damage`.
trait DistributedOptions {
    fn refuse_on_distributed_mesh(&self, _ctx: &LawContext<'_>) -> Result<()> {
        Ok(())
    }
}

/// `ParameterRegistry` (the local law's parameters), `ConstitutiveLaw`
/// and the delegating part of `Material` for a wrapper `$ty<DIM>` with the
/// field `local` and the extra internals `$($f: $name),*`.
macro_rules! non_local_wrapper {
    ($ty:ident, $local:ident, [$($f:ident : $name:literal),*]) => {
        impl<const DIM: usize> DerivedMaterialParams for $ty<DIM> {
            type Parent = $local<DIM>;
            fn parent(&self) -> &Self::Parent {
                &self.local
            }
            fn parent_mut(&mut self) -> &mut Self::Parent {
                &mut self.local
            }
            fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
                &[]
            }
            fn update_derived(&mut self) -> Result<()> {
                akantu_parser::ParameterRegistry::update_internal_parameters(&mut self.local)
            }
            fn derived_id(&self) -> &str {
                &self.local.base().id
            }
        }

        akantu_solid::impl_derived_material_registry!($ty<DIM>, const DIM);

        impl<const DIM: usize> ConstitutiveLaw for $ty<DIM> {
            fn name(&self) -> &str {
                self.local.name()
            }
            fn law_id(&self) -> &str {
                self.local.law_id()
            }
            fn law_type(&self) -> &str {
                self.local.law_type()
            }
            fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
                self.local.element_filter()
            }
            fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
                self.local.element_filter_mut()
            }
            fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
                self.local.for_each_internal(f);
                $(f($name, InternalFieldRef::Real(&self.$f));)*
            }
            fn for_each_internal_mut(
                &mut self,
                f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
            ) -> Result<()> {
                self.local.for_each_internal_mut(f)?;
                $(f($name, InternalFieldMut::Real(&mut self.$f))?;)*
                Ok(())
            }
        }

        impl<const DIM: usize> Material for $ty<DIM> {
            fn base(&self) -> &MaterialBase {
                self.local.base()
            }
            fn base_mut(&mut self) -> &mut MaterialBase {
                self.local.base_mut()
            }
            fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
                DistributedOptions::refuse_on_distributed_mesh(self, ctx)?;
                self.local.init_material(ctx)?;
                let blocks = self.local.base().blocks(ctx);
                $(self.$f.resize_blocks(&blocks)?;)*
                Ok(())
            }
            fn compute_stress(
                &mut self,
                ctx: &LawContext<'_>,
                ty: ElementType,
                ghost: GhostType,
            ) -> Result<()> {
                self.local_pass(ctx, ty, ghost)
            }
            fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
                self.all_stresses_impl(ctx, ghost)
            }
            fn reduce_after_stresses(
                &mut self,
                ghost: GhostType,
                reducer: &mut dyn LawReducer,
            ) -> Result<()> {
                self.reduce_after_stresses_impl(ghost, reducer)
            }
            fn compute_tangent_moduli(
                &mut self,
                ctx: &LawContext<'_>,
                ty: ElementType,
                ghost: GhostType,
                tangent: &mut Array<Real>,
            ) -> Result<()> {
                self.local.compute_tangent_moduli(ctx, ty, ghost, tangent)
            }
            fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
                self.local.compute_potential_energy(ctx, ty)
            }
            fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
                self.local.energy_of(ctx, energy_id)
            }
            fn potential_energy_by_element(
                &self,
                ctx: &LawContext<'_>,
                element: Element,
            ) -> Result<Vec<Real>> {
                self.local.potential_energy_by_element(ctx, element)
            }
            fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
                self.local.update_energies(ctx, ty)
            }
            fn update_energies_after_damage(
                &mut self,
                ctx: &LawContext<'_>,
                ty: ElementType,
            ) -> Result<()> {
                self.local.update_energies_after_damage(ctx, ty)
            }
            fn push_wave_speed(&self, element: Element) -> Result<Real> {
                self.local.push_wave_speed(element)
            }
            fn shear_wave_speed(&self, element: Element) -> Result<Real> {
                self.local.shear_wave_speed(element)
            }
            fn tangent_type(&self) -> akantu_core::enums::MatrixType {
                self.local.tangent_type()
            }
            fn has_stiffness_matrix_changed(&self) -> bool {
                self.local.has_stiffness_matrix_changed()
            }
            fn options(&self) -> Option<&LawOptions> {
                self.local.options()
            }
            fn options_mut(&mut self) -> Option<&mut LawOptions> {
                self.local.options_mut()
            }
            /// Upstream `dynamic_cast<MaterialNonLocalInterface *>`: the
            /// solid model drives the law with its non-local manager.
            fn as_non_local(&self) -> Option<&dyn MaterialNonLocal> {
                Some(self)
            }
            fn as_non_local_mut(&mut self) -> Option<&mut dyn MaterialNonLocal> {
                Some(self)
            }
            fn as_any(&self) -> Option<&dyn std::any::Any> {
                Some(self)
            }
            fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
                Some(self)
            }
        }
    };
}

// ------------------------------------------------- damage_iterative_non_local

/// `damage_iterative_non_local` (upstream `MaterialDamageIterativeNonLocal<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialDamageIterativeNonLocal<const DIM: usize> {
    /// The local law (`is_non_local = true`).
    pub local: MaterialDamageIterative<DIM>,
    /// `grad_u non local` (`dim²`).
    pub grad_u_nl: InternalField<Real>,
}

impl<const DIM: usize> MaterialDamageIterativeNonLocal<DIM> {
    /// New law.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut local =
            MaterialDamageIterative::with_type(spec, mesh, "damage_iterative_non_local")?;
        local.base_mut().is_non_local = true;
        Ok(Self {
            local,
            grad_u_nl: InternalField::new("grad_u non local", DIM * DIM, 0.0)?,
        })
    }

    /// Upstream `computeStress`: empty.
    fn local_pass(&mut self, _: &LawContext<'_>, _: ElementType, _: GhostType) -> Result<()> {
        Ok(())
    }

    /// Upstream (inherited) `MaterialDamageIterative::computeAllStresses`
    /// (its all-reduce of the maximum is declared).
    fn all_stresses_impl(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == NG {
            self.local.reset_norm_max_equivalent_stress();
        }
        all_stresses(self, ctx, ghost)
    }

    /// The inherited epilogue's all-reduce of the maximum.
    fn reduce_after_stresses_impl(
        &mut self,
        _ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        self.local.reduce_norm_max_in(reducer)
    }
}

impl<const DIM: usize> DistributedOptions for MaterialDamageIterativeNonLocal<DIM> {}

non_local_wrapper!(MaterialDamageIterativeNonLocal, MaterialDamageIterative, [grad_u_nl: "grad_u non local"]);

impl<const DIM: usize> MaterialDamageIterativeNonLocal<DIM> {
    /// Upstream `computeNonLocalStress(type, ghost)`
    /// (`material_damage_iterative_non_local_inline_impl.hh:68-94`): the
    /// elastic stress of the local `∇u`, `σ *= 1 − d`, the normalised
    /// equivalent stress from the **averaged** `grad_u non local`, the
    /// maximum (UD-573 for the per-type reset).
    ///
    /// # Errors
    /// Missing blocks, size mismatches, eigen-solver failures.
    pub fn compute_non_local_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let nl = self.grad_u_nl.get(ty, ghost)?.clone();
        self.local
            .compute_stress_with_criterion(ctx, ty, ghost, Some(&nl))
    }
}

impl<const DIM: usize> MaterialNonLocal for MaterialDamageIterativeNonLocal<DIM> {
    /// Upstream `getNeighborhoodName()`: the law's name.
    fn neighborhood_name(&self) -> String {
        self.local.name().to_owned()
    }

    /// Upstream constructor + `initMaterial`:
    /// `registerNonLocalVariable(gradu, grad_u_nl, dim²)`, then
    /// `nonLocalVariableToNeighborhood(grad_u_nl, name)` (UD-1002).
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![variable("grad_u", "grad_u non local", DIM * DIM)]
    }

    /// Upstream `computeNonLocalStresses` (`material_damage_iterative_non_local.cc:27-41`):
    /// reset the maximum (not ghost), then every type
    /// ([`non_local_pass_types`]); its closing all-reduce of the maximum is
    /// declared ([`MaterialNonLocal::reduce_after_non_local_stresses`]).
    fn compute_non_local_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == NG {
            self.local.reset_norm_max_equivalent_stress();
        }
        for ty in non_local_pass_types(self, ghost) {
            self.compute_non_local_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// The closing `comm.allReduce(norm_max_equivalent_stress, _max)` of
    /// upstream `computeNonLocalStresses`
    /// (`material_damage_iterative_non_local.cc:37-39`), declared:
    /// never a raw communicator call inside the residual
    /// assembly. Upstream has no RVE exception there, unlike
    /// `computeAllStresses` (UD-1003: the port's RVE meshes are serial, so
    /// the reduction is a no-op inside an RVE).
    fn reduce_after_non_local_stresses(
        &mut self,
        _ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        all_reduce_max(self.local.norm_max_equivalent_stress_mut(), reducer)
    }
}

impl<const DIM: usize> IterativeDamage for MaterialDamageIterativeNonLocal<DIM> {
    fn norm_max_equivalent_stress(&self) -> Real {
        self.local.norm_max_equivalent_stress()
    }
    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        self.local.update_damage(ctx)
    }
    fn set_rve_mode(&mut self, in_rve: bool) {
        self.local.set_rve_mode(in_rve);
    }
}

// ---------------------------------------------------------- brittle_non_local

/// `brittle_non_local` (upstream `MaterialBrittleNonLocal<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialBrittleNonLocal<const DIM: usize> {
    /// The local law (`is_non_local = true`).
    pub local: MaterialBrittle<DIM>,
    /// `Sigma max` (local equivalent stress).
    pub sigma_max: InternalField<Real>,
    /// `Sigma_max non local`.
    pub sigma_max_nl: InternalField<Real>,
    /// `Sigma_fracture`.
    pub sigma_fracture: InternalField<Real>,
}

impl<const DIM: usize> MaterialBrittleNonLocal<DIM> {
    /// New law.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut local = MaterialBrittle::with_type(spec, mesh, "brittle_non_local")?;
        local.base_mut().is_non_local = true;
        Ok(Self {
            local,
            sigma_max: InternalField::new("Sigma max", 1, 0.0)?,
            sigma_max_nl: InternalField::new("Sigma_max non local", 1, 0.0)?,
            sigma_fracture: InternalField::new("Sigma_fracture", 1, 0.0)?,
        })
    }

    /// Upstream `computeStress`: the brittle stress without damage, storing
    /// `σ_eq` and the fracture stress.
    fn local_pass(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        crate::kernels::small_strain_only(
            "brittle_non_local",
            self.local.base().finite_deformation,
        )?;
        self.local.update_strain_rate(ctx, ty, ghost)?;
        let mut smax = self.sigma_max.get(ty, ghost)?.clone();
        let mut sfrac = self.sigma_fracture.get(ty, ghost)?.clone();
        self.local.stress_loop(ty, ghost, |q, st| {
            if let (Some(a), Some(b)) = (
                smax.as_mut_slice().get_mut(q),
                sfrac.as_mut_slice().get_mut(q),
            ) {
                *a = st.sigma_equivalent;
                *b = st.fracture_stress;
            }
        })?;
        self.sigma_max.get_mut(ty, ghost)?.copy_from(&smax)?;
        self.sigma_fracture.get_mut(ty, ghost)?.copy_from(&sfrac)?;
        Ok(())
    }

    fn all_stresses_impl(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        all_stresses(self, ctx, ghost)
    }

    /// No rank reduction after the stresses (reduction hook).
    fn reduce_after_stresses_impl(&mut self, _: GhostType, _: &mut dyn LawReducer) -> Result<()> {
        Ok(())
    }
}

impl<const DIM: usize> DistributedOptions for MaterialBrittleNonLocal<DIM> {}

non_local_wrapper!(MaterialBrittleNonLocal, MaterialBrittle, [
    sigma_max: "Sigma max",
    sigma_max_nl: "Sigma_max non local",
    sigma_fracture: "Sigma_fracture"
]);

impl<const DIM: usize> MaterialNonLocal for MaterialBrittleNonLocal<DIM> {
    /// Upstream `getNeighborhoodName()`: the law's name.
    fn neighborhood_name(&self) -> String {
        self.local.name().to_owned()
    }

    /// Upstream `initMaterial`: `registerNonLocalVariable(Sigma max,
    /// Sigma_max non local, 1)` and `nonLocalVariableToNeighborhood()`
    /// (UD-1002).
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![variable("Sigma max", "Sigma_max non local", 1)]
    }

    /// Upstream `MaterialDamageNonLocal::computeNonLocalStresses`: every
    /// type of the filter with elements ([`non_local_pass_types`]).
    fn compute_non_local_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in non_local_pass_types(self, ghost) {
            self.compute_non_local_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }
}

impl<const DIM: usize> MaterialBrittleNonLocal<DIM> {
    /// Upstream `computeNonLocalStress`
    /// (`material_brittle_non_local_inline_impl.hh:94-115`): the brittle
    /// criterion with the **averaged** `σ_eq` on the stress of the local
    /// pass (`d = 1` if `σ_eq,nl > σ_fracture`, `σ *= 1 − d`).
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn compute_non_local_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let nl = self.sigma_max_nl.get(ty, ghost)?.clone();
        let frac = self.sigma_fracture.get(ty, ghost)?.clone();
        let mut sigma = self.local.base().stress.get(ty, ghost)?.clone();
        let mut dam = self.local.damage.damage.get(ty, ghost)?.clone();
        check_lengths(
            "MaterialBrittleNonLocal::computeNonLocalStress",
            sigma.len(),
            &[nl.len(), frac.len(), dam.len()],
        )?;
        for (q, s) in sigma.as_mut_slice().chunks_exact_mut(n).enumerate() {
            let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
            let d = &mut dam.as_mut_slice()[q];
            *d = MaterialBrittle::<DIM>::damage_and_stress_on_quad(
                s,
                *d,
                nl.as_slice()[q],
                frac.as_slice()[q],
            );
        }
        self.local
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.local
            .damage
            .damage
            .get_mut(ty, ghost)?
            .copy_from(&dam)?;
        Ok(())
    }
}

// ----------------------------------------------------- vreepeerlings_non_local

/// `vreepeerlings_non_local` (upstream `MaterialVreePeerlingsNonLocal<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialVreePeerlingsNonLocal<const DIM: usize> {
    /// The local law (`is_non_local = true`).
    pub local: MaterialVreePeerlings<DIM>,
    /// `equi-strain_non_local`.
    pub equi_strain_nl: InternalField<Real>,
    /// `equi-strain-rate_non_local`.
    pub equi_strain_rate_nl: InternalField<Real>,
}

impl<const DIM: usize> MaterialVreePeerlingsNonLocal<DIM> {
    /// New law.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut local = MaterialVreePeerlings::with_type(spec, mesh, "vreepeerlings_non_local")?;
        local.base_mut().is_non_local = true;
        Ok(Self {
            local,
            equi_strain_nl: InternalField::new("equi-strain_non_local", 1, 0.0)?,
            equi_strain_rate_nl: InternalField::new("equi-strain-rate_non_local", 1, 0.0)?,
        })
    }

    /// Upstream `computeStress` (inherited): the local law without damage.
    fn local_pass(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.local.compute_stress(ctx, ty, ghost)
    }

    fn all_stresses_impl(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        all_stresses(self, ctx, ghost)
    }

    /// No rank reduction after the stresses (reduction hook).
    fn reduce_after_stresses_impl(&mut self, _: GhostType, _: &mut dyn LawReducer) -> Result<()> {
        Ok(())
    }
}

impl<const DIM: usize> DistributedOptions for MaterialVreePeerlingsNonLocal<DIM> {}

non_local_wrapper!(MaterialVreePeerlingsNonLocal, MaterialVreePeerlings, [
    equi_strain_nl: "equi-strain_non_local",
    equi_strain_rate_nl: "equi-strain-rate_non_local"
]);

impl<const DIM: usize> MaterialNonLocal for MaterialVreePeerlingsNonLocal<DIM> {
    /// Upstream `getNeighborhoodName()`: the law's name.
    fn neighborhood_name(&self) -> String {
        self.local.name().to_owned()
    }

    /// Upstream `initMaterial`: `registerNonLocalVariable(equi_strain,
    /// equi_strain_non_local, 1)` and the same for the rate. That call is the
    /// pre-2017 material-level API (`MaterialNonLocal::registerNonLocalVariable(
    /// local, non_local, nb_degree_of_freedom)`, v2.2), in which the material
    /// averages every registered variable over its own pair list: the port's
    /// declaration in the law's neighbourhood is that behaviour (UD-1004).
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![
            variable("equi-strain", "equi-strain_non_local", 1),
            variable("equi-strain-rate", "equi-strain-rate_non_local", 1),
        ]
    }

    /// Upstream `MaterialDamageNonLocal::computeNonLocalStresses`: every
    /// type of the filter with elements ([`non_local_pass_types`]).
    fn compute_non_local_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in non_local_pass_types(self, ghost) {
            self.compute_non_local_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }
}

impl<const DIM: usize> MaterialVreePeerlingsNonLocal<DIM> {
    /// Upstream `computeNonLocalStress`
    /// (`material_vreepeerlings_non_local_inline_impl.hh:124-160`): the
    /// damage update with the **averaged** equivalent strain and rate on
    /// the stress of the local pass, `σ *= 1 − d`.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn compute_non_local_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let es = self.equi_strain_nl.get(ty, ghost)?.clone();
        let esr = self.equi_strain_rate_nl.get(ty, ghost)?.clone();
        let l = &mut self.local;
        let kapa = l.kapa.get(ty, ghost)?.clone();
        let mut sigma = l.base().stress.get(ty, ghost)?.clone();
        let mut dam = l.damage.damage.get(ty, ghost)?.clone();
        let mut nb = l.number_damage.get(ty, ghost)?.clone();
        let mut fs = l.full_dam_value_strain.get(ty, ghost)?.clone();
        let mut fsr = l.full_dam_value_strain_rate.get(ty, ghost)?.clone();
        check_lengths(
            "MaterialVreePeerlingsNonLocal::computeNonLocalStress",
            sigma.len(),
            &[
                es.len(),
                esr.len(),
                kapa.len(),
                dam.len(),
                nb.len(),
                fs.len(),
                fsr.len(),
            ],
        )?;
        for (q, s) in sigma.as_mut_slice().chunks_exact_mut(n).enumerate() {
            let (d, n_d, f_s, f_r) = l.damage_on_quad(
                dam.as_slice()[q],
                es.as_slice()[q],
                esr.as_slice()[q],
                kapa.as_slice()[q],
                (nb.as_slice()[q], fs.as_slice()[q], fsr.as_slice()[q]),
            );
            dam.as_mut_slice()[q] = d;
            nb.as_mut_slice()[q] = n_d;
            fs.as_mut_slice()[q] = f_s;
            fsr.as_mut_slice()[q] = f_r;
            scale_by_integrity(as_matrix_mut::<DIM, DIM>(s)?, d);
        }
        l.base_mut().stress.get_mut(ty, ghost)?.copy_from(&sigma)?;
        l.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        l.number_damage.get_mut(ty, ghost)?.copy_from(&nb)?;
        l.full_dam_value_strain.get_mut(ty, ghost)?.copy_from(&fs)?;
        l.full_dam_value_strain_rate
            .get_mut(ty, ghost)?
            .copy_from(&fsr)?;
        Ok(())
    }
}

// ------------------------------------- damage_orthotropic_iterative_non_local

/// `damage_orthotropic_iterative_non_local` (upstream
/// `MaterialOrthotropicDamageIterativeNonLocal<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialOrthotropicDamageIterativeNonLocal<const DIM: usize> {
    /// The local law (`is_non_local = true`).
    pub local: MaterialOrthotropicDamageIterative<DIM>,
    /// `grad_u non local` (`dim²`).
    pub grad_u_nl: InternalField<Real>,
}

impl<const DIM: usize> MaterialOrthotropicDamageIterativeNonLocal<DIM> {
    /// New law.
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut local = MaterialOrthotropicDamageIterative::with_type(
            spec,
            mesh,
            "damage_orthotropic_iterative_non_local",
        )?;
        local.base_mut().is_non_local = true;
        Ok(Self {
            local,
            grad_u_nl: InternalField::new("grad_u non local", DIM * DIM, 0.0)?,
        })
    }

    /// Upstream `computeStress`: empty.
    fn local_pass(&mut self, _: &LawContext<'_>, _: ElementType, _: GhostType) -> Result<()> {
        Ok(())
    }

    /// Upstream (inherited) `MaterialOrthotropicDamageIterative::computeAllStresses`.
    fn all_stresses_impl(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        // reset + empty local pass (the all-reduce of the reset maximum is 0)
        if ghost == NG {
            self.local.reset_norm_max_equivalent_stress();
        }
        all_stresses(self, ctx, ghost)
    }

    /// The inherited epilogue's `Max` all-reduce of the (reset) maximum
    /// (upstream `MaterialOrthotropicDamageIterative::computeAllStresses`,
    /// `material_orthotropic_damage_iterative.cc:164`), as
    /// `damage_iterative_non_local` (the two
    /// iterative wrappers now declare the same reduction).
    fn reduce_after_stresses_impl(
        &mut self,
        ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        self.local.reduce_after_stresses(ghost, reducer)
    }
}

non_local_wrapper!(MaterialOrthotropicDamageIterativeNonLocal, MaterialOrthotropicDamageIterative, [grad_u_nl: "grad_u non local"]);

impl<const DIM: usize> MaterialOrthotropicDamageIterativeNonLocal<DIM> {
    /// Upstream `computeNonLocalStress(type, ghost)`
    /// (`material_orthotropic_damage_iterative_non_local_inline_impl.hh:67-133`):
    /// the orthotropic damaged stress of the local `∇u`, the normalised
    /// equivalent stress from the **averaged** `grad_u non local`, the
    /// maximum (UD-573 for the per-type reset).
    ///
    /// # Errors
    /// Missing blocks, size mismatches, eigen-solver failures.
    pub fn compute_non_local_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let nl = self.grad_u_nl.get(ty, ghost)?.clone();
        self.local
            .compute_stress_with_criterion(ctx, ty, ghost, Some(&nl))
    }
}

impl<const DIM: usize> MaterialNonLocal for MaterialOrthotropicDamageIterativeNonLocal<DIM> {
    /// Upstream `getNeighborhoodName()`: the law's name.
    fn neighborhood_name(&self) -> String {
        self.local.name().to_owned()
    }

    /// Upstream `initMaterial`: `registerNonLocalVariable(gradu, grad_u_nl,
    /// dim²)` and `nonLocalVariableToNeighborhood()` (UD-1002).
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![variable("grad_u", "grad_u non local", DIM * DIM)]
    }

    /// Upstream (inherited) `MaterialDamageNonLocal::computeNonLocalStresses`:
    /// every type of the filter with elements ([`non_local_pass_types`]);
    /// the maximum was reset by the local pass (`computeAllStresses`, not
    /// ghost).
    fn compute_non_local_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        for ty in non_local_pass_types(self, ghost) {
            self.compute_non_local_stress(ctx, ty, ghost)?;
        }
        Ok(())
    }

    /// UD-1005 (category B): upstream's orthotropic variant does not
    /// override `computeNonLocalStresses`, so the maximum it computes is
    /// never all-reduced (its only all-reduce is the local
    /// `computeAllStresses` epilogue, before the non-local pass, on the
    /// reset value). Corrected default: the `Max` all-reduce of the maximum
    /// after the non-local pass, declared as
    /// `damage_iterative_non_local` does, which UD-870's global-maximum
    /// rule needs; `ExtraLawOptions::orthotropic_non_local_max_all_reduced
    /// = false`: none (upstream).
    fn reduce_after_non_local_stresses(
        &mut self,
        _ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        if !self
            .local
            .extra_options
            .orthotropic_non_local_max_all_reduced
        {
            return Ok(());
        }
        all_reduce_max(self.local.norm_max_equivalent_stress_mut(), reducer)
    }
}

/// UD-1005's compat with UD-870's default on more than
/// one rank (see [`MaterialOrthotropicDamageIterativeNonLocal`]'s
/// `update_damage`).
fn xn11_check(o: crate::ExtraLawOptions) -> Result<()> {
    if !o.orthotropic_non_local_max_all_reduced && o.orthotropic_damage_global_maximum_only {
        return Err(akantu_core::Error::Unsupported(
            "orthotropic_damage_iterative_non_local on a distributed mesh with UD-1005's compat \
             (orthotropic_non_local_max_all_reduced = false) and UD-870's default \
             (orthotropic_damage_global_maximum_only = true): the rank-local maxima cannot \
             designate the global one; use both compat switches (upstream) \
             or both defaults"
                .into(),
        ));
    }
    Ok(())
}

/// The combination that `xn11_check` rejects is refused at `init_full`: the
/// options are known at construction, so a run does not spend its first
/// solve before the refusal; the agreed check of `update_damage` is a
/// second guard.
impl<const DIM: usize> DistributedOptions for MaterialOrthotropicDamageIterativeNonLocal<DIM> {
    fn refuse_on_distributed_mesh(&self, ctx: &LawContext<'_>) -> Result<()> {
        if ctx.services.mesh().communicator().size() > 1 {
            xn11_check(self.local.extra_options)?;
        }
        Ok(())
    }
}

impl<const DIM: usize> IterativeDamage for MaterialOrthotropicDamageIterativeNonLocal<DIM> {
    fn norm_max_equivalent_stress(&self) -> Real {
        self.local.norm_max_equivalent_stress()
    }
    /// Upstream `updateDamage()` (the local law's).
    /// With UD-1005's compat (`orthotropic_non_local_max_all_reduced
    /// = false`) the maximum is rank-local, and UD-870's default
    /// (`orthotropic_damage_global_maximum_only`) designates the damaging
    /// rank by comparing each rank's maximum with the global one: with
    /// rank-local maxima the ranks would disagree on whether to enter that
    /// collective. On more than one rank the combination is refused, agreed
    /// on every rank before the update (`Error::Unsupported`); upstream's
    /// own behaviour is both compat switches together (each rank damages
    /// its local maximum, UD-870), and the default is both corrected
    /// switches (the global maximum damages once).
    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        let comm = ctx.services.mesh().communicator();
        if comm.size() > 1 {
            let local = xn11_check(self.local.extra_options);
            let mut ph = akantu_comm::phase::Phases::new(
                comm,
                "orthotropic_damage_iterative_non_local:update_damage",
            );
            let r = ph.agree("options", local, akantu_comm::phase::Severity::Recoverable);
            ph.settle(r)?;
        }
        self.local.update_damage(ctx)
    }
    fn set_rve_mode(&mut self, _: bool) {}
}

/// Register the non-local extra materials (legacy-list keywords
/// `damage_iterative_non_local`, `brittle_non_local`,
/// `vreepeerlings_non_local`, `damage_orthotropic_iterative_non_local`),
/// driven by the solid model's non-local manager (module docs), with the
/// corrected defaults.
///
/// Crate feature `damage-non-local` (upstream package
/// `extra_materials_non_local`, off by default).
///
/// # Errors
/// A duplicate registration.
#[cfg(feature = "damage-non-local")]
pub fn register_extra_non_local_materials(
    factory: &mut akantu_solid::material::MaterialFactory,
) -> Result<()> {
    register_extra_non_local_materials_with(factory, crate::ExtraLawOptions::default())
}

/// [`register_extra_non_local_materials`] with the upstream-defect
/// switches `options` (set at construction, before the input is parsed).
/// Crate feature `damage-non-local`.
///
/// # Errors
/// A duplicate registration.
#[cfg(feature = "damage-non-local")]
pub fn register_extra_non_local_materials_with(
    factory: &mut akantu_solid::material::MaterialFactory,
    options: crate::ExtraLawOptions,
) -> Result<()> {
    crate::damage_law::register_law!(
        factory,
        "damage_iterative_non_local",
        MaterialDamageIterativeNonLocal,
        options
    )?;
    crate::damage_law::register_law!(
        factory,
        "brittle_non_local",
        MaterialBrittleNonLocal,
        options
    )?;
    crate::damage_law::register_law!(
        factory,
        "vreepeerlings_non_local",
        MaterialVreePeerlingsNonLocal,
        options
    )?;
    crate::damage_law::register_law!(
        factory,
        "damage_orthotropic_iterative_non_local",
        MaterialOrthotropicDamageIterativeNonLocal,
        options
    )
}
