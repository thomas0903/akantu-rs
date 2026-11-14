// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_material.cc (PyMaterial
// trampoline, PYBIND11_OVERRIDE of initMaterial/computeStress/...),
// python/py_constitutive_law.cc:113-160 (registerInternalReal/Int),
// python/py_parser.cc:42-160 (registerParamReal), src/model/solid_mechanics/
// material.cc (Material::initMaterial resizing internals).

//! Runtime-defined materials (D-016 revised).
//!
//! A [`ForeignMaterial`] is a solid law whose behaviour is provided at run
//! time by an object implementing [`ForeignMaterialMethods`] — the Python
//! trampoline of an `aka.Material` subclass, or a user Rust law registered
//! by name. Its state is a [`ForeignLawState`] owned by the law:
//! common material internals, **law-owned** runtime parameters
//! ([`DynamicParameters`]) and runtime internals ([`DynamicInternals`]),
//! with the same reflection, history, resize and synchronisation contract
//! as typed built-in members.
//!
//! Each method gets a [`ForeignLaw`]: the law's state (`&mut`) plus the
//! phase-scoped [`LawContext`] (`&`), so the callback can read the model
//! (`getModel().getFEEngine()...`) and modify its own state without a
//! second borrow of anything; `super().initMaterial()` is
//! [`ForeignLaw::base_init_material`] on the same state.

use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_model::internal_field::{AnyInternalField, DynamicInternals};
use akantu_model::params::DynamicParameters;
use akantu_parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry};
use akantu_parser::{Parsable, ParserType};

use crate::material::{Material, MaterialBase};

/// State of a runtime-defined material.
#[derive(Clone, Debug)]
pub struct ForeignLawState {
    /// Common material state.
    pub base: MaterialBase,
    /// Runtime parameters (`rho`, `name`, then the user's).
    pub params: DynamicParameters,
    /// Runtime internals.
    pub internals: DynamicInternals,
    /// Internals exchanged with the ghost copies of the law's elements per
    /// synchronisation tag, in pack order (upstream: a user material's
    /// `packData` / `unpackData` override; [`Material::exchanged_internals`],
    /// MPI-053): typically a user extension tag (`_user_1`, …) registered on
    /// the model with `SolidMechanicsModel::register_synchronization_tag`.
    pub exchanged: Vec<(akantu_core::enums::SynchronizationTag, String)>,
}

/// What a callback sees: its own state and the phase context.
#[derive(Debug)]
pub struct ForeignLaw<'a, 'c> {
    /// The law's state.
    pub state: &'a mut ForeignLawState,
    /// Phase context (model services).
    pub ctx: &'a LawContext<'c>,
}

impl ForeignLaw<'_, '_> {
    /// `super().initMaterial()`: read the base parameters and size every
    /// internal (base and runtime) for the element filter.
    pub fn base_init_material(&mut self) -> Result<()> {
        self.state.base.rho = self.state.params.get_real("rho")?;
        self.state.base.init_history()?;
        // Upstream `resizeInternals`: every internal (base and runtime) in
        // register-id order (`std::map`), blocks ghost-major in enum order —
        // the draw order of random fields (D-006).
        let blocks = self.state.base.blocks(self.ctx);
        let mut names: Vec<String> = self
            .state
            .base
            .internals()
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .chain(self.state.internals.iter().map(|(n, _)| n.to_owned()))
            .collect();
        names.sort();
        for name in names {
            if let Some(f) = self
                .state
                .base
                .internals_mut()
                .into_iter()
                .find(|f| f.name() == name)
            {
                f.resize_blocks(&blocks)?;
            } else if let Ok(f) = self.state.internals.get_mut(&name) {
                // runtime internals take the law's restriction (upstream
                // `registerInternal` builds them with the law's dimension
                // and `_ek_regular`); one registered on
                // another FE engine or element filter is sized on its own
                // (CL-001)
                let r = self.state.base.internal_restriction();
                let law_filter = &self.state.base.element_filter;
                let ctx = self.ctx;
                macro_rules! resize {
                    ($f:expr) => {{
                        $f.adopt_restriction(r);
                        if $f.has_own_support() {
                            let own = crate::material::internal_blocks(
                                ctx,
                                $f.fe_engine_id(),
                                $f.element_filter().unwrap_or(law_filter),
                            )?;
                            $f.resize_blocks(&own)?;
                        } else {
                            $f.resize_blocks(&blocks)?;
                        }
                    }};
                }
                match f {
                    AnyInternalField::Real(f) => resize!(f),
                    AnyInternalField::Int(f) => resize!(f),
                    AnyInternalField::UInt(f) => resize!(f),
                    AnyInternalField::Bool(f) => resize!(f),
                }
            }
        }
        Ok(())
    }
}

/// The state of a runtime-defined law as the bindings reach it inside one
/// of its callbacks: the solid [`ForeignLawState`], or the state of a
/// runtime cohesive law of `akantu-cohesive` (D-078). Parameters
/// (built-in first, then runtime), internals (base, the law's own, runtime),
/// the element filter and `super().initMaterial()`.
pub trait RuntimeLawState: std::fmt::Debug + Send + Sync {
    /// Common material state (name, id, element filter, base internals).
    fn law_base(&self) -> &MaterialBase;
    /// Mutable [`RuntimeLawState::law_base`].
    fn law_base_mut(&mut self) -> &mut MaterialBase;
    /// Runtime internals (registered from the binding).
    fn runtime_internals(&self) -> &DynamicInternals;
    /// Mutable runtime internals.
    fn runtime_internals_mut(&mut self) -> &mut DynamicInternals;
    /// The law's own real internals besides the base and runtime ones (a
    /// cohesive law's `opening`, `tractions`, …), by name.
    fn own_real_internals(&self) -> Vec<(&'static str, &akantu_model::InternalField<Real>)> {
        Vec::new()
    }
    /// Mutable entry of [`RuntimeLawState::own_real_internals`].
    fn own_real_internal_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut akantu_model::InternalField<Real>> {
        let _ = name;
        None
    }
    /// A parameter (built-in, then runtime).
    ///
    /// # Errors
    /// Unknown parameter.
    fn get_param(&self, name: &str) -> Result<ParamValue>;
    /// Set a parameter.
    ///
    /// # Errors
    /// Unknown or read-only parameter, wrong kind.
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()>;
    /// Description of a parameter.
    fn param_info(&self, name: &str) -> Option<ParamInfo>;
    /// `registerParamReal(name, access, description[, default])` inside a
    /// callback.
    ///
    /// # Errors
    /// A parameter of that name exists.
    fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()>;
    /// `super().initMaterial()` (the base initialisation).
    ///
    /// # Errors
    /// Those of the base initialisation.
    fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()>;
    /// The base class's `getPushWaveSpeed` (a Python
    /// `super().getPushWaveSpeed(element)`): upstream `Material`'s is
    /// `AKANTU_TO_IMPLEMENT`, a law derived from a built-in law answers the
    /// built-in law's.
    ///
    /// # Errors
    /// The base has none.
    fn base_push_wave_speed(&self, element: Element) -> Result<Real> {
        let _ = element;
        Err(Error::NotImplemented(
            "getPushWaveSpeed of the Material base",
        ))
    }
    /// The base class's `getShearWaveSpeed` (see
    /// [`RuntimeLawState::base_push_wave_speed`]).
    ///
    /// # Errors
    /// The base has none.
    fn base_shear_wave_speed(&self, element: Element) -> Result<Real> {
        let _ = element;
        Err(Error::NotImplemented(
            "getShearWaveSpeed of the Material base",
        ))
    }
    /// The base class's `hasStiffnessMatrixChanged()` (upstream
    /// `Material`'s: `true`).
    fn base_has_stiffness_matrix_changed(&self) -> bool {
        true
    }
    /// The law's `hasMassMatrixChanged()`, which the Python trampoline does
    /// not make overridable (upstream `Material`'s: `false`; a law derived
    /// from a built-in law answers the built-in law's).
    fn base_has_mass_matrix_changed(&self) -> bool {
        false
    }
    /// Recompute derived parameters after a parameter changed (upstream
    /// `ParameterRegistry::set` → `updateInternalParameters`; a law derived
    /// from a built-in law updates the built-in law's, e.g. the Lamé
    /// constants of the elastic law).
    ///
    /// # Errors
    /// Those of the law.
    fn update_internal_parameters(&mut self) -> Result<()> {
        Ok(())
    }
    /// Downcast (for the bindings' law-specific accessors).
    fn as_any(&self) -> &dyn std::any::Any;
    /// Mutable downcast.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl RuntimeLawState for ForeignLawState {
    fn law_base(&self) -> &MaterialBase {
        &self.base
    }
    fn law_base_mut(&mut self) -> &mut MaterialBase {
        &mut self.base
    }
    fn runtime_internals(&self) -> &DynamicInternals {
        &self.internals
    }
    fn runtime_internals_mut(&mut self) -> &mut DynamicInternals {
        &mut self.internals
    }
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.params.set(name, value)?;
        if name == "name" {
            let v = self.params.get("name")?;
            self.base.set_name(v.into_byte_string()?);
        }
        Ok(())
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.params.param_info(name)
    }
    fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        self.params.register(
            name,
            ParamKind::Real,
            access,
            description,
            default.map(ParamValue::Real),
        )
    }
    fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        ForeignLaw { state: self, ctx }.base_init_material()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Behaviour of a runtime-defined material (the Python trampoline).
pub trait ForeignMaterialMethods: std::fmt::Debug + Send {
    /// `initMaterial` (default: `super().initMaterial()`).
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        law.base_init_material()
    }
    /// `computeStress(el_type, ghost_type)`.
    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()>;
    /// `computeTangentModuli(el_type, tangent, ghost_type)`.
    fn compute_tangent_moduli(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let _ = (law, ty, ghost, tangent);
        Err(Error::NotImplemented(
            "computeTangentModuli of a runtime material",
        ))
    }
    /// `computePotentialEnergy(el_type)`.
    fn compute_potential_energy(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
    ) -> Result<()> {
        let _ = (law, ty);
        Ok(())
    }
    /// `getPushWaveSpeed`.
    fn push_wave_speed(&self, state: &ForeignLawState, element: Element) -> Result<Real>;
    /// `getShearWaveSpeed`.
    fn shear_wave_speed(&self, state: &ForeignLawState, element: Element) -> Result<Real> {
        let _ = (state, element);
        Err(Error::NotImplemented(
            "getShearWaveSpeed of a runtime material",
        ))
    }
    /// `hasStiffnessMatrixChanged()` (default: the `Material` base answer,
    /// `true`), asked before the stiffness assembly with the phase context.
    /// An error cannot be returned through the infallible
    /// [`Material::has_stiffness_matrix_changed_in`]: it is kept by the law
    /// and raised by the model right after the question
    /// ([`Material::take_deferred_error`]).
    fn has_stiffness_matrix_changed(
        &self,
        state: &ForeignLawState,
        ctx: &LawContext<'_>,
    ) -> Result<bool> {
        let _ = (state, ctx);
        Ok(true)
    }
}

/// A runtime-defined solid material.
#[derive(Debug)]
pub struct ForeignMaterial {
    /// Law state.
    pub state: ForeignLawState,
    methods: Box<dyn ForeignMaterialMethods>,
    /// The error of the last `hasStiffnessMatrixChanged` answer, until the
    /// model takes it ([`Material::take_deferred_error`]).
    deferred: std::sync::Mutex<Option<Error>>,
}

impl ForeignMaterial {
    /// New law of type `law_type` with behaviour `methods`; registers the
    /// base parameters `name` and `rho` (upstream `Material` constructor).
    pub fn new(
        spec: &LawSpec,
        law_type: &str,
        methods: Box<dyn ForeignMaterialMethods>,
    ) -> Result<Self> {
        let mut params = DynamicParameters::new();
        // Upstream `std::string name` (constitutive_law_tmpl.hh): the
        // byte-preserving kind (`docs/ARCHITECTURE.md`).
        params.register(
            "name",
            ParamKind::ByteString,
            ParameterAccess::READABLE | ParameterAccess::PARSABLE,
            "Name of the material",
            Some(ParamValue::ByteString(spec.id.as_str().into())),
        )?;
        params.register(
            "rho",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "Density",
            Some(ParamValue::Real(0.0)),
        )?;
        let mut base = MaterialBase::new(spec, law_type)?;
        // The runtime `name` parameter defaults to the law id (see above).
        base.set_name(spec.id.as_str());
        Ok(Self {
            state: ForeignLawState {
                base,
                params,
                internals: DynamicInternals::default(),
                exchanged: Vec::new(),
            },
            methods,
            deferred: std::sync::Mutex::new(None),
        })
    }

    /// Python `registerParamReal(name, access, description)`.
    pub fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
    ) -> Result<()> {
        self.state
            .params
            .register(name, ParamKind::Real, access, description, None)
    }

    /// Python `registerInternalReal(name, nb_component)`.
    pub fn register_internal_real(&mut self, name: &str, nb_component: usize) -> Result<()> {
        self.state.internals.register_real(name, nb_component)
    }

    /// Upstream `registerInternal<Real>(id, nb_component, fe_engine_id[,
    /// element_filter])` (CL-001): a `Real` internal on the model's FE
    /// engine `fe_engine_id`, sized on `element_filter` (law-local index →
    /// mesh element) or, when `None`, on the law's element filter.
    pub fn register_internal_real_on(
        &mut self,
        name: &str,
        nb_component: usize,
        fe_engine_id: &str,
        element_filter: Option<ElementTypeMapArray<Idx>>,
    ) -> Result<()> {
        self.state
            .internals
            .register_real_on(name, nb_component, fe_engine_id, element_filter)
    }

    /// Element filter (mutable, filled by the model's selector).
    pub fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.state.base.element_filter
    }
}

impl ParameterRegistry for ForeignMaterial {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.state.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.state.params.set(name, value)?;
        self.sync_name(name)
    }
    /// Runtime names are not `'static`: use
    /// [`ConstitutiveLaw::parameter_names`].
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.state.params.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.state.params.set_from_input(name, value)?;
        self.sync_name(name)
    }
}

impl ForeignMaterial {
    /// Keep [`MaterialBase::name`] equal to the runtime `name` parameter.
    fn sync_name(&mut self, name: &str) -> Result<()> {
        if name == "name" {
            let v = self.state.params.get("name")?;
            self.state.base.set_name(v.into_byte_string()?);
        }
        Ok(())
    }
}

/// Upstream `ConstitutiveLaw` is a `Parsable(handler.getConstitutiveLawParserType(),
/// id)`: a runtime solid law reads its `material <type> [ … ]` section,
/// runtime-registered parameters included, through the parser's
/// `Parsable::parse_section` (byte kinds keep every byte, ARCHITECTURE.md, "Laws (D-016)").
impl Parsable for ForeignMaterial {
    fn section_type(&self) -> ParserType {
        ParserType::Material
    }
    fn parsable_id(&self) -> &str {
        &self.state.base.id
    }
}

impl ConstitutiveLaw for ForeignMaterial {
    fn name(&self) -> &str {
        &self.state.base.name
    }
    fn law_id(&self) -> &str {
        &self.state.base.id
    }
    fn law_type(&self) -> &str {
        &self.state.base.law_type
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.state.base.element_filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.state.base.element_filter
    }
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(self.state.base.internal_restriction())
    }
    fn parameter_names(&self) -> Vec<String> {
        self.state.params.names()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        for (n, field) in self.state.base.internals() {
            f(n, InternalFieldRef::Real(field));
        }
        for (n, field) in self.state.internals.iter() {
            f(n, field.as_ref());
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        for field in self.state.base.internals_mut() {
            let name = field.name().to_owned();
            f(&name, InternalFieldMut::Real(field))?;
        }
        for (n, field) in self.state.internals.iter_mut() {
            f(n, field.as_mut())?;
        }
        Ok(())
    }
}

impl Material for ForeignMaterial {
    /// Downcast (the bindings reach the runtime law's base answers).
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    /// Mutable downcast (the bindings' `super().initMaterial()` outside a
    /// callback: the `Material` base's initialisation).
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn base(&self) -> &MaterialBase {
        &self.state.base
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        &mut self.state.base
    }
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site("ForeignMaterial::init_material");
        self.methods.init_material(&mut ForeignLaw {
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
        akantu_core::array_shared::export_all::callback_site("ForeignMaterial::compute_stress");
        self.methods.compute_stress(
            &mut ForeignLaw {
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
            "ForeignMaterial::compute_tangent_moduli",
        );
        self.methods.compute_tangent_moduli(
            &mut ForeignLaw {
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
            "ForeignMaterial::compute_potential_energy",
        );
        self.methods.compute_potential_energy(
            &mut ForeignLaw {
                state: &mut self.state,
                ctx,
            },
            ty,
        )
    }
    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        akantu_core::array_shared::export_all::callback_site("ForeignMaterial::push_wave_speed");
        self.methods.push_wave_speed(&self.state, element)
    }
    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        akantu_core::array_shared::export_all::callback_site("ForeignMaterial::shear_wave_speed");
        self.methods.shear_wave_speed(&self.state, element)
    }
    fn has_stiffness_matrix_changed_in(&self, ctx: &LawContext<'_>) -> bool {
        akantu_core::array_shared::export_all::callback_site(
            "ForeignMaterial::has_stiffness_matrix_changed_in",
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
    fn exchanged_internals(&self, tag: akantu_core::enums::SynchronizationTag) -> Vec<&str> {
        self.state
            .exchanged
            .iter()
            .filter(|(t, _)| *t == tag)
            .map(|(_, n)| n.as_str())
            .collect()
    }
}
