// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_phasefield.cc (`PyPhaseField`
// trampoline: PYBIND11_OVERRIDE of initPhaseField, computeDrivingForce,
// computeDissipatedEnergy; `PhaseFieldFactory.registerAllocator`),
// src/model/phase_field/phasefield.{hh,cc} (the base the subclass derives
// from: `initPhaseField` = `initConstitutiveLaw`, `computeDrivingForce` and
// `computeDissipatedEnergy` `AKANTU_TO_IMPLEMENT`).

//! Runtime-defined phase fields (D-078, PY-092): a phase field
//! whose behaviour is provided at run time by an object implementing
//! [`ForeignPhaseFieldMethods`] — the Python trampoline of an
//! `aka.PhaseField` subclass returned by a `PhaseFieldFactory` allocator.
//!
//! The law owns everything the `PhaseField` base owns
//! ([`PhaseFieldBase`]: `l0`, `E`, `nu`, `gc`, `Plane_Stress`,
//! `irreversibility_type`, and the internals `gc`, `damage`, `gradd`, `phi`,
//! `strain`, `driving_force`, `driving_energy`, `damage_energy`,
//! `damage_energy_density`, `dissipated_energy`), plus **runtime**
//! parameters and internals registered by the subclass. The model calls it
//! at the built-in laws' points: `initPhaseField` (default: the base
//! initialisation), `computeDrivingForce(type, ghost)` after the damage
//! and its gradient are interpolated (the subclass writes `driving_force`,
//! `driving_energy`, `damage_energy`, `damage_energy_density`, which the
//! model assembles), `computeDissipatedEnergy(type)` for the energy. The
//! behaviour object is taken out of the law during a call; a law without it
//! (a re-entrant call) refuses.

use akantu_core::{ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_model::internal_field::{AnyInternalField, DynamicInternals};
use akantu_model::params::DynamicParameters;
use akantu_parser::param::{
    ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry, table_get, table_info,
    table_names, table_set, table_set_from_input,
};
use akantu_parser::{Parsable, ParserType};

use crate::energy_split::EnergySplit;
use crate::phasefield::{BASE_PARAMS, PhaseField, PhaseFieldBase};

/// State of a runtime phase field: the base state and the runtime
/// parameters and internals.
#[derive(Debug)]
pub struct ForeignPhaseFieldState {
    /// The `PhaseField` base (parameters and internals).
    pub base: PhaseFieldBase,
    /// Runtime parameters (after the base's).
    pub params: DynamicParameters,
    /// Runtime internals (after the base's).
    pub internals: DynamicInternals,
}

impl ForeignPhaseFieldState {
    /// `PhaseField::initPhaseField()` of the base (upstream
    /// `initConstitutiveLaw`): every internal (base and runtime) sized for
    /// the element filter, `updateInternalParameters`, the law marked
    /// initialised.
    ///
    /// # Errors
    /// Those of the resize and of the parameters update.
    pub fn base_init(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let blocks = self.base.blocks(ctx);
        let r = self.base.internal_restriction();
        // upstream `resizeInternals`: every internal in register-id order
        // (a `std::map` keyed by name: base and runtime together), the
        // draw order of the random `g_c` (D-006)
        let mut names: Vec<String> = self
            .base
            .internals()
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .chain(self.internals.iter().map(|(n, _)| n.to_owned()))
            .collect();
        names.sort();
        for name in names {
            if let Some((_, f)) = self
                .base
                .internals_mut()
                .into_iter()
                .find(|(n, _)| *n == name)
            {
                f.adopt_restriction(r);
                f.resize_blocks(&blocks)?;
                continue;
            }
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
        self.base.update_internal_parameters()?;
        self.base.set_init();
        Ok(())
    }

    /// The real internal `name` (base, then runtime).
    ///
    /// # Errors
    /// No real internal of that name.
    pub fn real_internal_mut(
        &mut self,
        name: &str,
    ) -> Result<&mut akantu_model::InternalField<Real>> {
        if let Some(i) = self.base.internals().iter().position(|(n, _)| *n == name) {
            return Ok(self
                .base
                .internals_mut()
                .into_iter()
                .nth(i)
                .ok_or_else(|| Error::invalid("internal error: phase-field base internals"))?
                .1);
        }
        match self.internals.get_mut(name)? {
            AnyInternalField::Real(f) => Ok(f),
            _ => Err(Error::invalid(format!(
                "internal '{name}' is not a real field"
            ))),
        }
    }
}

impl ParameterRegistry for ForeignPhaseFieldState {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        if table_info(BASE_PARAMS, name).is_some() {
            return table_get(BASE_PARAMS, &self.base, name);
        }
        self.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if table_info(BASE_PARAMS, name).is_some() {
            table_set(BASE_PARAMS, &mut self.base, name, value)?;
            return self.base.update_internal_parameters();
        }
        self.params.set(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(BASE_PARAMS)
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.base.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(BASE_PARAMS, name).or_else(|| self.params.param_info(name))
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        if table_info(BASE_PARAMS, name).is_some() {
            return table_set_from_input(BASE_PARAMS, &mut self.base, name, value);
        }
        self.params.set_from_input(name, value)
    }
}

/// Behaviour of a runtime phase field (upstream `PyPhaseField`).
pub trait ForeignPhaseFieldMethods: std::fmt::Debug + Send {
    /// `initPhaseField()` (default: the base initialisation).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn init_phase_field(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        state.base_init(ctx)
    }
    /// `computeDrivingForce(el_type, ghost_type)` (the base's is
    /// `AKANTU_TO_IMPLEMENT`).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_driving_force(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let _ = (state, ty, ghost);
        Err(Error::NotImplemented(
            "computeDrivingForce of the PhaseField base",
        ))
    }
    /// `computeDissipatedEnergy(el_type)` (the base's is
    /// `AKANTU_TO_IMPLEMENT`).
    ///
    /// # Errors
    /// Those of the behaviour.
    fn compute_dissipated_energy(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ty: ElementType,
    ) -> Result<()> {
        let _ = (state, ty);
        Err(Error::NotImplemented(
            "computeDissipatedEnergy of the PhaseField base",
        ))
    }
}

/// A runtime phase field.
#[derive(Debug)]
pub struct ForeignPhaseField {
    /// Law state.
    pub state: ForeignPhaseFieldState,
    methods: Option<Box<dyn ForeignPhaseFieldMethods>>,
}

fn no_behaviour() -> Error {
    Error::invalid("runtime phase field without its behaviour (a re-entrant call)")
}

impl ForeignPhaseField {
    /// A runtime phase field of type `law_type` on `mesh` (upstream
    /// `PhaseField(model, id)`: the base parameters and internals) with
    /// behaviour `methods`.
    ///
    /// # Errors
    /// Those of the base construction.
    pub fn new(
        spec: &LawSpec,
        law_type: &str,
        mesh: &Mesh,
        methods: Box<dyn ForeignPhaseFieldMethods>,
    ) -> Result<Self> {
        Ok(Self {
            state: ForeignPhaseFieldState {
                base: PhaseFieldBase::new(spec, law_type, mesh)?,
                params: DynamicParameters::new(),
                internals: DynamicInternals::default(),
            },
            methods: Some(methods),
        })
    }

    /// Python `registerParamReal(name, access, description[, default])`.
    ///
    /// # Errors
    /// A parameter of that name exists.
    pub fn register_param_real(
        &mut self,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        if table_info(BASE_PARAMS, name).is_some() {
            return Err(Error::invalid(format!(
                "parameter '{name}' is already registered by the PhaseField base"
            )));
        }
        self.state.params.register(
            name,
            ParamKind::Real,
            access,
            description,
            default.map(ParamValue::Real),
        )
    }

    fn with_methods<R>(
        &mut self,
        f: impl FnOnce(&mut dyn ForeignPhaseFieldMethods, &mut ForeignPhaseFieldState) -> Result<R>,
    ) -> Result<R> {
        let mut m = self.methods.take().ok_or_else(no_behaviour)?;
        let r = f(m.as_mut(), &mut self.state);
        self.methods = Some(m);
        r
    }
}

impl ParameterRegistry for ForeignPhaseField {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.state.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.state.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.state.param_names()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.state.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.state.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.state.set_param_from_input(name, value)
    }
}

impl Parsable for ForeignPhaseField {
    fn section_type(&self) -> ParserType {
        ParserType::Phasefield
    }
    fn parsable_id(&self) -> &str {
        &self.state.base.id
    }
}

impl ConstitutiveLaw for ForeignPhaseField {
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
        let mut v: Vec<String> = table_names(BASE_PARAMS)
            .into_iter()
            .map(str::to_owned)
            .collect();
        v.extend(self.state.params.names());
        v
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
        for (n, field) in self.state.base.internals_mut() {
            f(n, InternalFieldMut::Real(field))?;
        }
        for (n, field) in self.state.internals.iter_mut() {
            f(n, field.as_mut())?;
        }
        Ok(())
    }
}

impl PhaseField for ForeignPhaseField {
    fn base(&self) -> &PhaseFieldBase {
        &self.state.base
    }
    fn base_mut(&mut self) -> &mut PhaseFieldBase {
        &mut self.state.base
    }
    fn energy_split(&self) -> Option<&EnergySplit> {
        None
    }
    fn init_phase_field(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site("ForeignPhaseField::init_phase_field");
        self.with_methods(|m, s| m.init_phase_field(s, ctx))
    }
    fn compute_driving_force(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site(
            "ForeignPhaseField::compute_driving_force",
        );
        self.with_methods(|m, s| m.compute_driving_force(s, ty, ghost))
    }
    fn compute_dissipated_energy(&mut self, ty: ElementType) -> Result<()> {
        akantu_core::array_shared::export_all::callback_site(
            "ForeignPhaseField::compute_dissipated_energy",
        );
        self.with_methods(|m, s| m.compute_dissipated_energy(s, ty))
    }
    fn dissipated_energy_by_element(
        &self,
        _ty: ElementType,
        _index: usize,
        _out: &mut [Real],
    ) -> Result<()> {
        Err(Error::NotImplemented(
            "computeDissipatedEnergyByElement of a runtime phase field (upstream \
             AKANTU_TO_IMPLEMENT in the PhaseField base)",
        ))
    }
}
