// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_material.cc (the `PyMaterial`
// trampoline of `register_material_classes<MaterialElastic<dim>>`:
// PYBIND11_OVERRIDE falls back to the C++ base, `MaterialElastic<dim>`, for
// every method the Python subclass does not define), src/model/
// solid_mechanics/materials/material_elastic.{hh,cc} (the base).

//! Runtime laws derived from a built-in law (D-078, PY-077).
//!
//! Upstream's Python classes `MaterialLinearElastic1D/2D/3D` are
//! `MaterialElastic<dim>` with the `PyMaterial` trampoline: a Python
//! subclass *is* an elastic law whose overridden methods (`initMaterial`,
//! `computeStress`, `computeTangentModuli`, `computePotentialEnergy`,
//! `getPushWaveSpeed`, `getShearWaveSpeed`, `hasStiffnessMatrixChanged`)
//! call Python, every other method — and every method the subclass does
//! not define — being the elastic law's own. `super().initMaterial()` (and
//! `super().getPushWaveSpeed(...)`, …) inside a Python override run the
//! elastic implementation (pybind11's override lookup skips the calling
//! Python method).
//!
//! A [`DerivedMaterial`] reproduces that: it owns the built-in law (any
//! [`Material`]; the bindings pass the factory's `elastic` law of the
//! model's dimension), **runtime** parameters and internals registered by
//! the subclass, and a behaviour object ([`DerivedMaterialMethods`]) whose
//! default methods are the built-in law's. The built-in law keeps its own
//! parameters (`E`, `nu`, `Plane_Stress`, the derived `lambda`, `mu`,
//! `kapa`, …) and internals (stress, gradient, energies, thermal fields);
//! the runtime ones come after them, as in a [`crate::ForeignMaterial`].

use akantu_core::enums::MatrixType;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext,
};
use akantu_model::internal_field::{AnyInternalField, DynamicInternals};
use akantu_model::params::DynamicParameters;
use akantu_parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry};
use akantu_parser::{Parsable, ParserType};

use crate::foreign::RuntimeLawState;
use crate::material::{LawOptions, Material, MaterialBase};

/// State of a runtime law derived from a built-in law: the built-in law
/// and the runtime parameters and internals registered by the subclass.
#[derive(Debug)]
pub struct DerivedLawState {
    /// The built-in law (its parameters, internals and computations).
    pub law: Box<dyn Material + Sync>,
    /// Runtime parameters (after the built-in law's).
    pub params: DynamicParameters,
    /// Runtime internals (after the built-in law's).
    pub internals: DynamicInternals,
}

/// What a callback sees: the law's state and the phase context.
#[derive(Debug)]
pub struct DerivedLaw<'a, 'c> {
    /// The law's state.
    pub state: &'a mut DerivedLawState,
    /// Phase context (model services).
    pub ctx: &'a LawContext<'c>,
}

impl DerivedLawState {
    /// `super().initMaterial()`: the built-in law's initialisation, then
    /// the runtime internals sized for the element filter (with the law's
    /// restriction, as a runtime material's).
    ///
    /// # Errors
    /// Those of the built-in initialisation and of the resize.
    pub fn base_init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.law.init_material(ctx)?;
        let blocks = self.law.base().blocks(ctx);
        let r = self.law.base().internal_restriction();
        let mut names: Vec<String> = self.internals.iter().map(|(n, _)| n.to_owned()).collect();
        names.sort();
        for name in names {
            match self.internals.get_mut(&name)? {
                AnyInternalField::Real(f) => {
                    f.adopt_restriction(r);
                    f.resize_blocks(&blocks)?;
                }
                AnyInternalField::Int(f) => {
                    f.adopt_restriction(r);
                    f.resize_blocks(&blocks)?;
                }
                AnyInternalField::UInt(f) => {
                    f.adopt_restriction(r);
                    f.resize_blocks(&blocks)?;
                }
                AnyInternalField::Bool(f) => {
                    f.adopt_restriction(r);
                    f.resize_blocks(&blocks)?;
                }
            }
        }
        Ok(())
    }

    fn is_builtin_param(&self, name: &str) -> bool {
        self.law.param_info(name).is_some()
    }
}

impl DerivedLaw<'_, '_> {
    /// `super().initMaterial()`.
    ///
    /// # Errors
    /// See [`DerivedLawState::base_init_material`].
    pub fn base_init_material(&mut self) -> Result<()> {
        self.state.base_init_material(self.ctx)
    }
}

/// Behaviour of a runtime law derived from a built-in law (the Python
/// trampoline). Every default is the built-in law's method.
pub trait DerivedMaterialMethods: std::fmt::Debug + Send {
    /// `initMaterial` (default: the built-in initialisation and the
    /// runtime internals, [`DerivedLaw::base_init_material`]).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn init_material(&mut self, law: &mut DerivedLaw<'_, '_>) -> Result<()> {
        law.base_init_material()
    }
    /// `computeStress(el_type, ghost_type)`.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_stress(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        law.state.law.compute_stress(law.ctx, ty, ghost)
    }
    /// `computeTangentModuli(el_type, tangent, ghost_type)`.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_tangent_moduli(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        law.state
            .law
            .compute_tangent_moduli(law.ctx, ty, ghost, tangent)
    }
    /// `computePotentialEnergy(el_type)`.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_potential_energy(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
    ) -> Result<()> {
        law.state.law.compute_potential_energy(law.ctx, ty)
    }
    /// `getPushWaveSpeed`.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn push_wave_speed(&self, state: &DerivedLawState, element: Element) -> Result<Real> {
        state.law.push_wave_speed(element)
    }
    /// `getShearWaveSpeed`.
    ///
    /// # Errors
    /// Those of the behaviour.
    fn shear_wave_speed(&self, state: &DerivedLawState, element: Element) -> Result<Real> {
        state.law.shear_wave_speed(element)
    }
    /// `hasStiffnessMatrixChanged()` with the phase context (an error is
    /// deferred like [`crate::ForeignMaterialMethods::has_stiffness_matrix_changed`]).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn has_stiffness_matrix_changed(
        &self,
        state: &DerivedLawState,
        ctx: &LawContext<'_>,
    ) -> Result<bool> {
        Ok(state.law.has_stiffness_matrix_changed_in(ctx))
    }
}

/// A runtime law derived from a built-in law.
#[derive(Debug)]
pub struct DerivedMaterial {
    /// Law state.
    pub state: DerivedLawState,
    methods: Box<dyn DerivedMaterialMethods>,
    deferred: std::sync::Mutex<Option<Error>>,
}

impl DerivedMaterial {
    /// The runtime law of type `law_type` over the built-in `law` (built
    /// for the same spec), with behaviour `methods`. The law's type becomes
    /// `law_type` (the registered name, as a runtime material's).
    #[must_use]
    pub fn new(
        mut law: Box<dyn Material + Sync>,
        law_type: &str,
        methods: Box<dyn DerivedMaterialMethods>,
    ) -> Self {
        law.base_mut().law_type = law_type.to_owned();
        Self {
            state: DerivedLawState {
                law,
                params: DynamicParameters::new(),
                internals: DynamicInternals::default(),
            },
            methods,
            deferred: std::sync::Mutex::new(None),
        }
    }

    /// Python `registerParamReal(name, access, description)` (a name of the
    /// built-in law is refused, as a second registration upstream).
    ///
    /// # Errors
    /// A parameter of that name exists.
    pub fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
    ) -> Result<()> {
        if self.state.is_builtin_param(name) {
            return Err(Error::invalid(format!(
                "Parameter named {name} already registered."
            )));
        }
        self.state
            .params
            .register(name, ParamKind::Real, access, description, None)
    }

    /// Python `registerInternalReal(name, nb_component)`.
    ///
    /// # Errors
    /// An internal of that name exists.
    pub fn register_internal_real(&mut self, name: &str, nb_component: usize) -> Result<()> {
        self.state.internals.register_real(name, nb_component)
    }

    /// Python `registerInternalInt(name, nb_component)`.
    ///
    /// # Errors
    /// An internal of that name exists.
    pub fn register_internal_int(&mut self, name: &str, nb_component: usize) -> Result<()> {
        self.state.internals.register_int(name, nb_component)
    }
}

// ---- the state as the bindings see it inside a callback ------------------------------

impl RuntimeLawState for DerivedLawState {
    fn law_base(&self) -> &MaterialBase {
        self.law.base()
    }
    fn law_base_mut(&mut self) -> &mut MaterialBase {
        self.law.base_mut()
    }
    fn runtime_internals(&self) -> &DynamicInternals {
        &self.internals
    }
    fn runtime_internals_mut(&mut self) -> &mut DynamicInternals {
        &mut self.internals
    }
    fn own_real_internals(&self) -> Vec<(&'static str, &akantu_model::InternalField<Real>)> {
        // the built-in law's internals besides the base ones (the elastic
        // law's `third_axis_deformation`, `delta_T`, `sigma_th`,
        // `epsilon_th`)
        elastic_own_internals(self.law.as_ref())
    }
    fn own_real_internal_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut akantu_model::InternalField<Real>> {
        elastic_own_internal_mut(self.law.as_mut(), name)
    }
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        if self.is_builtin_param(name) {
            return self.law.get_param(name);
        }
        self.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if self.is_builtin_param(name) {
            return self.law.set_param(name, value);
        }
        self.params.set(name, value)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.law
            .param_info(name)
            .or_else(|| self.params.param_info(name))
    }
    fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        if self.is_builtin_param(name) {
            return Err(Error::invalid(format!(
                "Parameter named {name} already registered."
            )));
        }
        self.params.register(
            name,
            ParamKind::Real,
            access,
            description,
            default.map(ParamValue::Real),
        )
    }
    fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base_init_material(ctx)
    }
    fn base_push_wave_speed(&self, element: Element) -> Result<Real> {
        self.law.push_wave_speed(element)
    }
    fn base_shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.law.shear_wave_speed(element)
    }
    fn base_has_stiffness_matrix_changed(&self) -> bool {
        self.law.has_stiffness_matrix_changed()
    }
    fn base_has_mass_matrix_changed(&self) -> bool {
        self.law.has_mass_matrix_changed()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.law.update_internal_parameters()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// The own (non-base) real internals of a built-in elastic law, by name;
/// none for another base law.
fn elastic_own_internals(
    law: &(dyn Material + Sync),
) -> Vec<(&'static str, &akantu_model::InternalField<Real>)> {
    use crate::materials::elastic::MaterialElastic;
    let Some(any) = law.as_any() else {
        return Vec::new();
    };
    if let Some(e) = any.downcast_ref::<MaterialElastic<1>>() {
        return e.own_internals();
    }
    if let Some(e) = any.downcast_ref::<MaterialElastic<2>>() {
        return e.own_internals();
    }
    if let Some(e) = any.downcast_ref::<MaterialElastic<3>>() {
        return e.own_internals();
    }
    Vec::new()
}

/// Mutable entry of [`elastic_own_internals`].
fn elastic_own_internal_mut<'a>(
    law: &'a mut (dyn Material + Sync),
    name: &str,
) -> Option<&'a mut akantu_model::InternalField<Real>> {
    use crate::materials::elastic::MaterialElastic;
    let any = law.as_any_mut()?;
    if any.is::<MaterialElastic<1>>() {
        return any
            .downcast_mut::<MaterialElastic<1>>()?
            .own_internal_mut(name);
    }
    if any.is::<MaterialElastic<2>>() {
        return any
            .downcast_mut::<MaterialElastic<2>>()?
            .own_internal_mut(name);
    }
    any.downcast_mut::<MaterialElastic<3>>()?
        .own_internal_mut(name)
}

// ---- registry, parsable, law ---------------------------------------------------------------

impl ParameterRegistry for DerivedMaterial {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.state.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.state.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.state.law.param_names()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.state.law.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.state.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if self.state.is_builtin_param(name) {
            return self.state.law.set_param_from_input(name, value);
        }
        self.state.params.set_from_input(name, value)
    }
}

/// The law reads its `material <type> [ … ]` section: the built-in law's
/// parameters and the runtime ones.
impl Parsable for DerivedMaterial {
    fn section_type(&self) -> ParserType {
        ParserType::Material
    }
    fn parsable_id(&self) -> &str {
        &self.state.law.base().id
    }
}

impl ConstitutiveLaw for DerivedMaterial {
    fn name(&self) -> &str {
        self.state.law.name()
    }
    fn law_id(&self) -> &str {
        self.state.law.law_id()
    }
    fn law_type(&self) -> &str {
        self.state.law.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.state.law.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.state.law.element_filter_mut()
    }
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        self.state.law.internal_restriction()
    }
    fn parameter_names(&self) -> Vec<String> {
        let mut v = self.state.law.parameter_names();
        v.extend(self.state.params.names());
        v
    }
    fn energy(&self, energy_id: &str) -> Result<Real> {
        self.state.law.energy(energy_id)
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.state.law.for_each_internal(f);
        for (n, field) in self.state.internals.iter() {
            f(n, field.as_ref());
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.state.law.for_each_internal_mut(f)?;
        for (n, field) in self.state.internals.iter_mut() {
            f(n, field.as_mut())?;
        }
        Ok(())
    }
}

impl Material for DerivedMaterial {
    /// Downcast (the bindings reach the runtime law's base answers).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    /// Mutable downcast (the bindings' `super().initMaterial()` outside a
    /// callback: the built-in law's initialisation).
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn base(&self) -> &MaterialBase {
        self.state.law.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.state.law.base_mut()
    }
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site("DerivedMaterial::init_material");
        self.methods.init_material(&mut DerivedLaw {
            state: &mut self.state,
            ctx,
        })
    }
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site("DerivedMaterial::compute_stress");
        self.methods.compute_stress(
            &mut DerivedLaw {
                state: &mut self.state,
                ctx,
            },
            ty,
            ghost,
        )
    }
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site(
            "DerivedMaterial::compute_tangent_moduli",
        );
        self.methods.compute_tangent_moduli(
            &mut DerivedLaw {
                state: &mut self.state,
                ctx,
            },
            ty,
            ghost,
            tangent,
        )
    }
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site(
            "DerivedMaterial::compute_potential_energy",
        );
        self.methods.compute_potential_energy(
            &mut DerivedLaw {
                state: &mut self.state,
                ctx,
            },
            ty,
        )
    }
    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        akantu_core::array_shared::export_all::callback_site("DerivedMaterial::push_wave_speed");
        self.methods.push_wave_speed(&self.state, element)
    }
    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        akantu_core::array_shared::export_all::callback_site("DerivedMaterial::shear_wave_speed");
        self.methods.shear_wave_speed(&self.state, element)
    }
    fn rho(&self) -> Real {
        self.state.law.rho()
    }
    fn tangent_type(&self) -> MatrixType {
        self.state.law.tangent_type()
    }
    fn has_stiffness_matrix_changed(&self) -> bool {
        self.state.law.has_stiffness_matrix_changed()
    }
    fn has_stiffness_matrix_changed_in(&self, ctx: &LawContext<'_>) -> bool {
        akantu_core::array_shared::export_all::callback_site(
            "DerivedMaterial::has_stiffness_matrix_changed_in",
        );
        match self.methods.has_stiffness_matrix_changed(&self.state, ctx) {
            Ok(changed) => changed,
            Err(e) => {
                // only a flag update under this lock (no user code)
                self.deferred
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .get_or_insert(e);
                true
            }
        }
    }
    fn take_deferred_error(&self) -> Result<()> {
        match self
            .deferred
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
    fn mark_stiffness_changed(&mut self) {
        self.state.law.mark_stiffness_changed();
    }
    fn options(&self) -> Option<&LawOptions> {
        self.state.law.options()
    }
    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        self.state.law.options_mut()
    }
    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.state.law.compute_all_cauchy_stresses(ghost)
    }
    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        self.state.law.potential_energy_by_element(ctx, element)
    }
}
