// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_constitutive_law.cc
// (InternalFieldReal/Int, ConstitutiveLawInternalHandler),
// python/py_constitutive_law.hh (PyConstitutiveLaw trampoline,
// ConstitutiveLaw<Handler>.setReal/setBool/setString/setInt),
// python/py_material.cc (PyMaterial trampoline, MaterialFactory),
// python/py_constitutive_law_selector.cc (ConstitutiveLawSelector,
// DefaultMaterialSelector, MeshDataMaterialSelectorString).

//! Constitutive laws seen from Python (PY-070…073, PY-076, PY-079).
//!
//! **One link per Python law object.** A Python `Material` object holds a
//! [`LawLink`]: either an existing law of a model (`model.getMaterial(0)`:
//! the model and the law index), or — for an object constructed from Python
//! (`LocalElastic(model, id)` inside an allocator registered with
//! `MaterialFactory.registerAllocator` / `registerCustomMaterials`) — a
//! *pending* law (the parameters and internals registered in `__init__`)
//! that the model's factory turns into an [`akantu::solid::ForeignMaterial`]
//! whose behaviour is the Python object ([`PyLawMethods`], the trampoline
//! of upstream's `PyMaterial`).
//!
//! **Callbacks.** When the model calls a method the Python class overrides
//! (`initMaterial`, `computeStress`, `computeTangentModuli`,
//! `computePotentialEnergy`, `getPushWaveSpeed`, `getShearWaveSpeed`), the
//! law's state is lent to the call as phase-scoped handles
//! ([`crate::scoped::with_scoped_mut`], [`crate::scoped::with_scoped`] for
//! the `const` wave speeds) together with the phase's model services
//! ([`crate::scoped::with_model_services`]); the call itself goes through
//! the runtime trampoline (refused while a native guard is alive; Python
//! exceptions are re-raised at the outer call). During the call the model
//! is held by its running operation: the law's getters (`getStress`,
//! `getGradU`, `getInternalReal(…)(type)`, `getElementFilter`,
//! `getReal`/`setReal`, `registerParamReal`, `super().initMaterial()`) work on
//! the lent state — internals are zero-copy views of the law's own storage —
//! and `self.getModel()` returns the model, whose FE engines are reached
//! through the lent services. Every handle dies with the call.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use akantu::core::{Array, Element, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu::model::constitutive_law::{
    ConstitutiveLaw, ConstitutiveLawSelector, ElementDataSelector, InternalFieldRef, LawContext,
    LawIndexSelector, LawPhase, LawSpec, ModelServices, SelectorChain, SelectorContext,
};
use akantu::model::internal_field::AnyInternalField;
use akantu::model::params::DynamicParameters;
use akantu::parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry};
use akantu::solid::SolidMechanicsModel;
use akantu::solid::material::{Material, MaterialFactory};
use akantu::solid::{
    DerivedLaw, DerivedLawState, DerivedMaterial, DerivedMaterialMethods, ForeignLaw,
    ForeignLawState, ForeignMaterial, ForeignMaterialMethods, RuntimeLawState,
};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use super::arrays::owned_numpy;
use super::common::PyElement;
use super::enums::{EtArg, GhostArg, element_type_obj, ghost_type_obj};
use super::etma::{EtmaInt, EtmaProvider, EtmaReal};
use super::exports::{ExportTable, into_numpy};
use super::handle::{Handler, ModelHandle, ModelKey, ModelRef, Part};
use super::parser::{Prepared, PyParameterRegistry, PyParsable, RegistryAccess};
use super::runtime::{IntoPy, LendContext, callback, lending_callback, native};
use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::Layout;
use crate::scoped::{
    ContextKey, Scoped, ScopedMut, with_model_services, with_scoped, with_scoped_mut,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // only short, non-panicking critical sections
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---- the link of a Python law object ---------------------------------------------

/// What a Python `__init__` registered before the law exists.
#[derive(Debug)]
struct PendingLaw {
    /// `name`, `rho` (as `ForeignMaterial::new`) and the user's parameters.
    params: DynamicParameters,
    /// `(name, nb_component, integer)` runtime internals.
    internals: Vec<(String, usize, bool)>,
}

impl PendingLaw {
    fn new(id: &str) -> Result<Self> {
        let mut params = DynamicParameters::new();
        params.register(
            "name",
            ParamKind::ByteString,
            ParameterAccess::READABLE | ParameterAccess::PARSABLE,
            "Name of the material",
            Some(ParamValue::ByteString(id.into())),
        )?;
        params.register(
            "rho",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "Density",
            Some(ParamValue::Real(0.0)),
        )?;
        Ok(Self {
            params,
            internals: Vec::new(),
        })
    }

    /// Replay the registrations onto a new runtime cohesive law:
    /// parameters the cohesive base does not have become runtime ones,
    /// internals runtime internals on the law's cohesive elements.
    fn apply_cohesive(self, mc: &mut akantu::cohesive::MaterialCohesive) -> Result<()> {
        use akantu::parser::ParameterRegistry;
        for p in self.params.iter() {
            let known = ParameterRegistry::param_info(mc, &p.name).is_some();
            refuse_second_registration(&p.name, known)?;
            if !known {
                if p.kind != ParamKind::Real {
                    return Err(Error::Unsupported(format!(
                        "runtime parameter '{}' of kind {:?}",
                        p.name, p.kind
                    )));
                }
                mc.runtime_params.register(
                    &p.name,
                    ParamKind::Real,
                    p.access,
                    &p.description,
                    None,
                )?;
            }
            // `name` is the law id (set by the constructor, read-only)
            if p.name != "name"
                && let Ok(v) = self.params.get(&p.name)
            {
                ParameterRegistry::set_param(mc, &p.name, v)?;
            }
        }
        for (name, nbc, int) in self.internals {
            if int {
                mc.runtime_internals.register_int(&name, nbc)?;
            } else {
                mc.runtime_internals.register_real(&name, nbc)?;
            }
        }
        Ok(())
    }

    /// Replay the registrations onto a new law derived from a built-in law:
    /// parameters the built-in law has keep its registration
    /// (their values are set, but `name`), the others become runtime ones.
    fn apply_derived(self, dm: &mut DerivedMaterial) -> Result<()> {
        for p in self.params.iter() {
            let known = dm.param_info(&p.name).is_some();
            refuse_second_registration(&p.name, known)?;
            if !known {
                if p.kind != ParamKind::Real {
                    return Err(Error::Unsupported(format!(
                        "runtime parameter '{}' of kind {:?}",
                        p.name, p.kind
                    )));
                }
                dm.register_param_real(&p.name, p.access, &p.description)?;
            }
            if p.name != "name"
                && let Ok(v) = self.params.get(&p.name)
            {
                dm.set_param(&p.name, v)?;
            }
        }
        for (name, nbc, int) in self.internals {
            if int {
                dm.register_internal_int(&name, nbc)?;
            } else {
                dm.register_internal_real(&name, nbc)?;
            }
        }
        Ok(())
    }

    /// Replay the registrations onto the new law.
    fn apply(self, fm: &mut ForeignMaterial) -> Result<()> {
        for p in self.params.iter() {
            let known = fm.param_info(&p.name).is_some();
            refuse_second_registration(&p.name, known)?;
            if !known {
                if p.kind != ParamKind::Real {
                    return Err(Error::Unsupported(format!(
                        "runtime parameter '{}' of kind {:?}",
                        p.name, p.kind
                    )));
                }
                fm.register_param_real(&p.name, p.access, &p.description)?;
            }
            // `name` is the law id (set by `ForeignMaterial::new`, read-only)
            if p.name != "name"
                && let Ok(v) = self.params.get(&p.name)
            {
                fm.set_param(&p.name, v)?;
            }
        }
        for (name, nbc, int) in self.internals {
            if int {
                fm.state.internals.register_int(&name, nbc)?;
            } else {
                fm.register_internal_real(&name, nbc)?;
            }
        }
        Ok(())
    }
}

/// The parameters [`PendingLaw::new`] registers itself (the law's `name`
/// and `rho`, which the law being built registers too); every other pending
/// parameter was registered by the Python `__init__`.
const PENDING_SEEDS: [&str; 2] = ["name", "rho"];

/// Upstream `ParameterRegistry::registerParam` refuses a name that is
/// already registered (`parameter_registry_tmpl.hh:374-386`): the base law
/// registers its own parameters in `super().__init__`, so a Python
/// `registerParamReal(…, "E", …)` on a `MaterialLinearElastic<d>D` subclass
/// raises there. The base law does not exist yet while
/// `__init__` runs (a documented limit), so the port refuses it when the law
/// is built, with upstream's message.
fn refuse_second_registration(name: &str, known_to_the_base: bool) -> Result<()> {
    if known_to_the_base && !PENDING_SEEDS.contains(&name) {
        return Err(Error::invalid(format!(
            "Parameter named {name} already registered."
        )));
    }
    Ok(())
}

/// The law state lent to a running Python callback.
#[derive(Clone, Debug)]
pub(crate) enum Lent {
    /// `&mut` state (every method but the wave speeds).
    Mut {
        state: ScopedMut<dyn RuntimeLawState>,
        phase: LawPhase,
        services: Scoped<dyn ModelServices>,
    },
    /// `&` state (`getPushWaveSpeed`, `getShearWaveSpeed`).
    Shared(Scoped<dyn RuntimeLawState>),
}

/// The lent state of the innermost running callback of a law, admitted for
/// the calling Python code (it runs inside that callback), with the key of
/// the callback's execution context.
#[derive(Clone, Debug)]
pub(crate) struct AdmittedLent(Lent, ContextKey);

#[derive(Debug, Default)]
struct LinkState {
    /// The model (weak: the model owns the law, which owns this object).
    model: Option<(Weak<ModelHandle>, Part)>,
    /// A strong reference for wrappers of existing laws.
    keep: Option<ModelRef>,
    /// Law id once the law exists.
    id: Option<String>,
    /// Cached law index.
    index: Option<usize>,
    /// Registrations of a Python object not yet installed.
    pending: Option<PendingLaw>,
    /// The object was installed as a law (a second installation is refused).
    installed: bool,
}

/// The link between a Python law object and its Rust law.
#[derive(Debug)]
pub(crate) struct LawLink {
    state: Mutex<LinkState>,
    /// Export leases of the law's internals taken through a lent state.
    leases: Mutex<ExportTable<(String, ElementType, GhostType)>>,
    /// Lent states of running callbacks (innermost last), with the
    /// execution context of each lend.
    lent: Mutex<Vec<(Lent, Arc<LendContext>)>>,
}

/// Pops the innermost lent state on drop.
struct LentGuard(Arc<LawLink>);

impl Drop for LentGuard {
    fn drop(&mut self) {
        lock(&self.0.lent).pop();
    }
}

fn not_initialised() -> Error {
    Error::invalid(
        "this Material object is not attached to a model law (Material.__init__(model, id) not \
         called, or the object was not returned by a factory allocator)",
    )
}

impl LawLink {
    /// A link before `__init__`.
    pub(crate) fn uninit() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(LinkState::default()),
            leases: Mutex::new(ExportTable::default()),
            lent: Mutex::new(Vec::new()),
        })
    }

    /// The link of the existing law `index` of `model`.
    pub(crate) fn existing(model: ModelRef, index: usize) -> Arc<Self> {
        let link = Self::uninit();
        {
            let mut s = lock(&link.state);
            s.model = Some((Arc::downgrade(&model.h), model.part));
            s.keep = Some(model);
            s.index = Some(index);
            s.installed = true;
        }
        link
    }

    /// `Material.__init__(model, id)`: a pending law of `model`.
    pub(crate) fn init_pending(&self, model: &ModelRef, id: &str) -> Result<()> {
        let mut s = lock(&self.state);
        if s.installed {
            return Ok(());
        }
        s.model = Some((Arc::downgrade(&model.h), model.part));
        s.pending = Some(PendingLaw::new(id)?);
        s.id = Some(id.to_owned());
        Ok(())
    }

    /// The model of this law.
    pub(crate) fn model(&self) -> Result<ModelRef> {
        let s = lock(&self.state);
        if let Some(k) = &s.keep {
            return Ok(k.clone());
        }
        let (w, part) = s.model.as_ref().ok_or_else(not_initialised)?;
        let h = w
            .upgrade()
            .ok_or_else(|| Error::invalid("the model of this Material was destroyed"))?;
        Ok(ModelRef { h, part: *part })
    }

    /// The state lent to the innermost running callback of this law, if
    /// any, admitted for the calling Python code; an error if a callback is
    /// running and the caller is not inside it (another thread, greenlet, or
    /// a generator / coroutine resumed outside it).
    fn lent_top(&self, py: Python<'_>) -> Result<Option<AdmittedLent>> {
        // clone out and release the lock before looking at Python frames
        let top = lock(&self.lent).last().cloned();
        match top {
            None => Ok(None),
            Some((lent, cx)) => Ok(Some(AdmittedLent(lent, cx.admit(py)?))),
        }
    }

    fn push_lent(self: &Arc<Self>, lent: Lent, cx: &Arc<LendContext>) -> LentGuard {
        lock(&self.lent).push((lent, Arc::clone(cx)));
        LentGuard(Arc::clone(self))
    }

    /// The law index in `smm` (resolved by law id once).
    fn index_in(&self, smm: &SolidMechanicsModel) -> Result<usize> {
        let mut s = lock(&self.state);
        if let Some(i) = s.index {
            return Ok(i);
        }
        let id = s.id.clone().ok_or_else(not_initialised)?;
        for i in 0..smm.nb_materials() {
            if smm.material(i)?.law_id() == id {
                s.index = Some(i);
                return Ok(i);
            }
        }
        Err(Error::not_found("material with id", id))
    }

    /// Run `f` on the law in the model (native section).
    pub(crate) fn with_law<R>(
        &self,
        f: impl FnOnce(&mut SolidMechanicsModel, usize) -> Result<R>,
    ) -> Result<R> {
        let model = self.model()?;
        model.h.with(|m| {
            let smm = m.solid(model.part)?;
            let i = self.index_in(smm)?;
            f(smm, i)
        })
    }

    /// The model and the law index (Python side).
    pub(crate) fn model_and_index(&self, py: Python<'_>) -> PyResult<(ModelRef, usize)> {
        let model = self.model().map_err(|e| super::runtime::to_py(py, e))?;
        if let Some(i) = lock(&self.state).index {
            return Ok((model, i));
        }
        let m2 = model.clone();
        let i = native(py, || {
            m2.h.with(|m| {
                let smm = m.solid(m2.part)?;
                self.index_in(smm)
            })
        })?;
        Ok((model, i))
    }

    fn leases(&self) -> MutexGuard<'_, ExportTable<(String, ElementType, GhostType)>> {
        lock(&self.leases)
    }

    /// Export the block `(ty, ghost)` of the real internal `name` of a lent
    /// state.
    fn export_real(
        &self,
        s: &mut dyn RuntimeLawState,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ForeignView<Real>> {
        let key = (name.to_owned(), ty, ghost);
        let mut t = self.leases();
        for f in s.law_base_mut().internals_mut() {
            if f.name() == name {
                return t.view(key, f.values_mut().shared_mut(ty, ghost)?);
            }
        }
        if let Some(f) = s.own_real_internal_mut(name) {
            return t.view(key, f.values_mut().shared_mut(ty, ghost)?);
        }
        match s.runtime_internals_mut().get_mut(name)? {
            AnyInternalField::Real(f) => t.view(key, f.values_mut().shared_mut(ty, ghost)?),
            _ => Err(Error::invalid(format!(
                "internal '{name}' is not a real field"
            ))),
        }
    }

    /// Export the block of the integer internal `name` of a lent state.
    fn export_int(
        &self,
        s: &mut dyn RuntimeLawState,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<ForeignView<i32>> {
        let key = (name.to_owned(), ty, ghost);
        match s.runtime_internals_mut().get_mut(name)? {
            AnyInternalField::Int(f) => self
                .leases()
                .view(key, f.values_mut().shared_mut(ty, ghost)?),
            _ => Err(Error::invalid(format!(
                "internal '{name}' is not an integer field"
            ))),
        }
    }

    /// `getStress`/`getGradU`/`getInternalReal(name)(type, ghost)`: a view of
    /// a real internal block (the lent state inside callbacks of this law).
    pub(crate) fn real_view<'py>(
        self: &Arc<Self>,
        py: Python<'py>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? {
            let link = Arc::clone(self);
            let n = name.to_owned();
            let v = native(py, move || match lent {
                Lent::Mut { state, .. } => {
                    state.with_mut(key, |s| link.export_real(s, &n, ty, ghost))?
                }
                Lent::Shared(_) => link.leases().cached::<Real>(&(n, ty, ghost)),
            })?;
            return into_numpy(py, v, Layout::Rows);
        }
        let (model, law) = self.model_and_index(py)?;
        model.view::<f64>(
            py,
            ModelKey::Internal {
                handler: Handler::Material,
                law,
                name: name.to_owned(),
                ty,
                ghost,
            },
        )
    }

    /// A view (lent state) or a read-only copy (model) of an integer
    /// internal block.
    fn int_view<'py>(
        self: &Arc<Self>,
        py: Python<'py>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Some(AdmittedLent(Lent::Mut { state, .. }, key)) = self.lent_top(py).py_err(py)? {
            let link = Arc::clone(self);
            let n = name.to_owned();
            let v = native(py, move || {
                state.with_mut(key, |s| link.export_int(s, &n, ty, ghost))?
            })?;
            return into_numpy(py, v, Layout::Rows);
        }
        let n = name.to_owned();
        let (vals, rows, nbc) = native(py, || {
            self.with_law(|smm, i| {
                let mut out = None;
                smm.material(i)?.for_each_internal(&mut |fname, f| {
                    if fname == n
                        && out.is_none()
                        && let InternalFieldRef::Int(f) = f
                    {
                        out = Some(
                            f.values()
                                .get(ty, ghost)
                                .map(|a| (a.as_slice().to_vec(), a.len(), a.nb_component())),
                        );
                    }
                });
                out.unwrap_or_else(|| Err(Error::not_found("integer internal", n.clone())))
            })
        })?;
        let a = owned_numpy(py, vals, rows, nbc, Layout::Rows)?;
        a.call_method1("setflags", (false,))?;
        Ok(a)
    }

    /// Element types of internal `name` for `filter`.
    fn internal_types(
        self: &Arc<Self>,
        py: Python<'_>,
        name: &str,
        filter: TypeFilter,
    ) -> PyResult<Vec<ElementType>> {
        let n = name.to_owned();
        if let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? {
            return native(py, move || {
                let types = |s: &dyn RuntimeLawState| -> Vec<ElementType> {
                    for (fname, f) in s.law_base().internals() {
                        if fname == n {
                            return f.values().element_types(filter).collect();
                        }
                    }
                    for (fname, f) in s.own_real_internals() {
                        if fname == n {
                            return f.values().element_types(filter).collect();
                        }
                    }
                    match s.runtime_internals().get(&n) {
                        Ok(AnyInternalField::Real(f)) => f.values().element_types(filter).collect(),
                        Ok(AnyInternalField::Int(f)) => f.values().element_types(filter).collect(),
                        _ => Vec::new(),
                    }
                };
                match lent {
                    Lent::Mut { state, .. } => state.with_mut(key, |s| types(s)),
                    Lent::Shared(s) => s.with(key, |s| types(s)),
                }
            });
        }
        native(py, move || {
            self.with_law(|smm, i| {
                let mut out = Vec::new();
                smm.material(i)?.for_each_internal(&mut |fname, f| {
                    if fname == n && out.is_empty() {
                        out = match f {
                            InternalFieldRef::Real(f) => f.values().element_types(filter).collect(),
                            InternalFieldRef::Int(f) => f.values().element_types(filter).collect(),
                            _ => Vec::new(),
                        };
                    }
                });
                Ok(out)
            })
        })
    }

    /// Whether this law has an internal `name` of the requested kind
    /// (`int`), with the error upstream raises when it has not
    /// (`ConstitutiveLawInternalHandler::getSharedPtrInternal`).
    /// `Ok(None)` when it cannot be decided now — a pending law
    /// (its base internals do not exist before installation) or a model held
    /// by a running call this caller is not inside — and the object stays
    /// lazy as before.
    fn has_internal(
        self: &Arc<Self>,
        py: Python<'_>,
        name: &str,
        int: bool,
    ) -> PyResult<Option<std::result::Result<(), String>>> {
        let n = name.to_owned();
        let missing = move |law: &str, id: &str, n: &str| {
            format!("The constitutive law {law}({id}) does not contain an internal {n}")
        };
        let in_state = move |s: &dyn RuntimeLawState| -> std::result::Result<(), String> {
            let base = s.law_base().internals().iter().any(|(f, _)| *f == n)
                || s.own_real_internals().iter().any(|(f, _)| *f == n);
            let ok = match s.runtime_internals().get(&n) {
                Ok(AnyInternalField::Real(_)) => !int,
                Ok(AnyInternalField::Int(_)) => int,
                _ => base && !int,
            };
            if ok {
                Ok(())
            } else {
                Err(missing(&s.law_base().name, &s.law_base().id, &n))
            }
        };
        let lent = match self.lent_top(py) {
            Ok(l) => l,
            Err(_) => return Ok(None),
        };
        if let Some(AdmittedLent(lent, key)) = lent {
            let r = native(py, move || match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| in_state(s)),
                Lent::Shared(s) => s.with(key, |s| in_state(s)),
            });
            return Ok(r.ok());
        }
        if lock(&self.state).pending.is_some() {
            return Ok(None);
        }
        let n = name.to_owned();
        let r = native(py, move || {
            self.with_law(|smm, i| {
                let law = smm.material(i)?;
                let mut found = false;
                law.for_each_internal(&mut |fname, f| {
                    if fname == n {
                        found |= match f {
                            InternalFieldRef::Real(_) => !int,
                            InternalFieldRef::Int(_) => int,
                            _ => false,
                        };
                    }
                });
                Ok(if found {
                    Ok(())
                } else {
                    Err(missing(law.name(), law.law_id(), &n))
                })
            })
        });
        Ok(r.ok())
    }

    /// `registerParamReal` on this law (`admitted`: the lent state admitted
    /// for the caller before its native section, [`Self::lent_top`]).
    fn register_param_real(
        &self,
        admitted: Option<AdmittedLent>,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        // upstream's text for a name registered twice
        let upstream_text = |e: Error| match e {
            Error::AlreadyExists {
                kind: "parameter",
                id,
            } => Error::invalid(format!("Parameter named {id} already registered.")),
            e => e,
        };
        if let Some(AdmittedLent(lent, key)) = admitted {
            return match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| {
                    s.register_param_real(name, access, description, default)
                })?,
                Lent::Shared(_) => Err(Error::invalid(
                    "registerParamReal is not available inside a const method",
                )),
            }
            .map_err(upstream_text);
        }
        let mut s = lock(&self.state);
        match s.pending.as_mut() {
            Some(p) => p
                .params
                .register(
                    name,
                    ParamKind::Real,
                    access,
                    description,
                    default.map(ParamValue::Real),
                )
                .map_err(upstream_text),
            None => Err(Error::Unsupported(
                "registering parameters on a law outside its constructor or callbacks".into(),
            )),
        }
    }

    /// `registerInternalReal/Int(name, nb_component)` on this law
    /// (`admitted`: the lent state admitted for the caller before its native
    /// section, [`Self::lent_top`]).
    fn register_internal(
        &self,
        admitted: Option<AdmittedLent>,
        name: &str,
        nb_component: usize,
        int: bool,
    ) -> Result<()> {
        if let Some(AdmittedLent(lent, key)) = admitted {
            return match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| {
                    if int {
                        s.runtime_internals_mut().register_int(name, nb_component)
                    } else {
                        s.runtime_internals_mut().register_real(name, nb_component)
                    }
                })?,
                Lent::Shared(_) => Err(Error::invalid(
                    "registerInternal is not available inside a const method",
                )),
            };
        }
        let mut s = lock(&self.state);
        match s.pending.as_mut() {
            Some(p) => {
                if p.internals.iter().any(|(n, _, _)| n == name) {
                    return Err(Error::AlreadyExists {
                        kind: "internal",
                        id: name.into(),
                    });
                }
                p.internals.push((name.to_owned(), nb_component, int));
                Ok(())
            }
            None => Err(Error::Unsupported(
                "registering internals on a law outside its constructor or callbacks".into(),
            )),
        }
    }

    /// The pending registrations, for the factory (once).
    fn take_pending(&self) -> Result<PendingLaw> {
        let mut s = lock(&self.state);
        if s.installed {
            return Err(Error::invalid(
                "this Material object is already a law of a model (an allocator must return a \
                 new object)",
            ));
        }
        let p = s.pending.take().ok_or_else(not_initialised)?;
        Ok(p)
    }

    fn install(&self, id: String, model: &Arc<ModelHandle>, part: Part) {
        let mut s = lock(&self.state);
        s.model = Some((Arc::downgrade(model), part));
        s.id = Some(id);
        s.index = None;
        s.installed = true;
    }

    /// The element filter as `(type, ghost) → (values, rows, cols)` copies.
    fn filter_block(
        self: &Arc<Self>,
        py: Python<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<(Vec<i32>, usize, usize)> {
        let get = move |s: &dyn RuntimeLawState| -> Result<(Vec<i32>, usize, usize)> {
            let a = s.law_base().element_filter.get(ty, ghost)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        };
        if let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? {
            return native(py, move || match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| get(s))?,
                Lent::Shared(s) => s.with(key, |s| get(s))?,
            });
        }
        native(py, || {
            self.with_law(|smm, i| {
                let a = smm.material(i)?.element_filter().get(ty, ghost)?;
                Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
            })
        })
    }

    fn filter_types(
        self: &Arc<Self>,
        py: Python<'_>,
        filter: TypeFilter,
    ) -> PyResult<Vec<ElementType>> {
        if let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? {
            return native(py, move || match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| {
                    s.law_base().element_filter.element_types(filter).collect()
                }),
                Lent::Shared(s) => s.with(key, |s| {
                    s.law_base().element_filter.element_types(filter).collect()
                }),
            });
        }
        native(py, || {
            self.with_law(|smm, i| {
                Ok(smm
                    .material(i)?
                    .element_filter()
                    .element_types(filter)
                    .collect())
            })
        })
    }

    /// `true` while a callback of this law runs and the calling Python code
    /// runs inside it (the lend is admitted, D-086); an error if a callback
    /// runs and the caller is not inside it; `false` outside callbacks.
    pub(crate) fn has_lent(self: &Arc<Self>, py: Python<'_>) -> PyResult<bool> {
        Ok(self.lent_top(py).py_err(py)?.is_some())
    }

    /// `f` on the state lent to the innermost running callback of this law
    /// (`Ok(None)` outside callbacks): the base-class answers of
    /// `super().getPushWaveSpeed(...)`, `super().getShearWaveSpeed(...)`,
    /// `super().hasStiffnessMatrixChanged()` inside an override
    /// (pybind11 calls the C++ base there, never the Python override again).
    pub(crate) fn with_lent_state<R: Send>(
        self: &Arc<Self>,
        py: Python<'_>,
        f: impl FnOnce(&dyn RuntimeLawState) -> Result<R> + Send,
    ) -> PyResult<Option<R>> {
        let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? else {
            return Ok(None);
        };
        native(py, move || match lent {
            Lent::Mut { state, .. } => state.with_mut(key, |s| f(&*s))?,
            Lent::Shared(s) => s.with(key, |st| f(st))?,
        })
        .map(Some)
    }

    /// The base initialisation of the law (a `super().initMaterial()`, or
    /// a call inside a callback when the class does not override
    /// `initMaterial`): on the state lent to the running callback, else on
    /// the model's law — the base's for a super-call (`super_call`), the
    /// law's own `initMaterial` (which runs the Python override, if any)
    /// otherwise.
    pub(crate) fn init_material(
        self: &Arc<Self>,
        py: Python<'_>,
        super_call: bool,
    ) -> PyResult<()> {
        if let Some(AdmittedLent(lent, key)) = self.lent_top(py).py_err(py)? {
            return native(py, move || match lent {
                Lent::Mut {
                    state,
                    phase,
                    services,
                } => services.with(key, |sv| {
                    let ctx = LawContext {
                        phase,
                        services: sv,
                    };
                    state.with_mut(key, |s| s.base_init(&ctx))?
                })?,
                Lent::Shared(_) => Err(Error::invalid(
                    "initMaterial is not available inside a const method",
                )),
            });
        }
        native(py, || {
            self.with_law(|smm, i| {
                smm.with_material(i, LawPhase::Init, |law, ctx| {
                    if super_call && let Some(any) = law.as_any_mut() {
                        if let Some(d) = any.downcast_mut::<DerivedMaterial>() {
                            return d.state.base_init(ctx);
                        }
                        if let Some(f) = any.downcast_mut::<ForeignMaterial>() {
                            return f.state.base_init(ctx);
                        }
                    }
                    law.init_material(ctx)
                })
            })
        })
    }
}

// ---- parameter registries -------------------------------------------------------------

/// The parameters of a lent law state.
struct StateRegistry<'a>(&'a mut dyn RuntimeLawState);

impl ParameterRegistry for StateRegistry<'_> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.0.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.0.set_param(name, value)
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.0.update_internal_parameters()
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.0.param_info(name)
    }
}

/// The read-only parameters of a state lent shared.
struct ReadRegistry<'a>(&'a dyn RuntimeLawState);

impl ParameterRegistry for ReadRegistry<'_> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.0.get_param(name)
    }
    fn set_param(&mut self, name: &str, _value: ParamValue) -> Result<()> {
        Err(Error::invalid(format!(
            "parameter '{name}' cannot be set inside a const method"
        )))
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.0.param_info(name)
    }
}

/// The parameters registered by a pending law's `__init__`.
struct PendingRegistry<'a>(&'a mut DynamicParameters);

impl ParameterRegistry for PendingRegistry<'_> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.0.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.0.set(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.0.param_info(name)
    }
}

/// The registry behind a Python law object.
#[derive(Debug)]
pub(crate) struct LawRegistry(pub(crate) Arc<LawLink>);

impl RegistryAccess for LawRegistry {
    fn prepare(&self, py: Python<'_>) -> Result<Prepared> {
        Ok(Prepared(self.0.lent_top(py)?, None))
    }

    fn with_registry(
        &self,
        prepared: &Prepared,
        f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        if let Some(AdmittedLent(lent, key)) = prepared.0.clone() {
            return match lent {
                Lent::Mut { state, .. } => state.with_mut(key, |s| f(&mut StateRegistry(s)))?,
                Lent::Shared(s) => s.with(key, |s| f(&mut ReadRegistry(s)))?,
            };
        }
        {
            let mut s = lock(&self.0.state);
            if let Some(p) = s.pending.as_mut() {
                return f(&mut PendingRegistry(&mut p.params));
            }
        }
        // through the handle: the export leases are refreshed afterwards
        // (`setReal` may reset internals, upstream `updateInternalParameters`)
        self.0.with_law(|smm, i| {
            let law: &mut dyn Material = smm.material_mut(i)?;
            f(law)
        })
    }

    fn register_param_real(
        &self,
        prepared: &Prepared,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        self.0
            .register_param_real(prepared.0.clone(), name, access, description, default)
    }
}

// ---- ConstitutiveLawInternalHandler / ConstitutiveLaw<SolidMechanicsModel> ------------

/// What a `ConstitutiveLawInternalHandler` object designates.
#[derive(Debug)]
pub(crate) enum HandlerRef {
    /// A law (a `Material` object).
    Law(Arc<LawLink>),
    /// A law of another handler (diffusion laws, phase fields).
    Other(OtherLaw),
    /// A Python `PhaseField` subclass object (a runtime phase field once
    /// a factory allocator returned it).
    PhaseFieldLaw(Arc<super::pf_laws::PfLink>),
    /// A handler constructed from Python
    /// (`ConstitutiveLawInternalHandler(id, spatial_dimension, parent_id)`):
    /// it owns nothing a model uses.
    Standalone {
        id: String,
        spatial_dimension: i64,
        internals: Mutex<BTreeMap<String, (usize, bool)>>,
    },
}

/// A law of a diffusion or phase-field model (by handler and index).
#[derive(Clone, Debug)]
pub(crate) struct OtherLaw {
    pub(crate) model: ModelRef,
    pub(crate) handler: Handler,
    pub(crate) index: usize,
}

impl OtherLaw {
    pub(crate) fn with<R>(
        &self,
        f: impl FnOnce(&mut dyn ConstitutiveLaw) -> Result<R>,
    ) -> Result<R> {
        let (handler, index, part) = (self.handler, self.index, self.model.part);
        self.model.h.with(|m| {
            let mut f = Some(f);
            let mut out = None;
            m.ops(part)?.with_handler_law(handler, index, &mut |l| {
                let f = f
                    .take()
                    .ok_or_else(|| Error::invalid("internal error: law callback reused"))?;
                out = Some(f(l)?);
                Ok(())
            })?;
            out.ok_or_else(|| Error::invalid("internal error: law callback not run"))
        })
    }
}

impl RegistryAccess for OtherLaw {
    fn with_registry(
        &self,
        _prepared: &Prepared,
        f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        self.with(|l| f(l as &mut dyn ParameterRegistry))
    }
}

/// An internal of a diffusion law or a phase field.
#[derive(Debug)]
struct OtherInternal {
    law: OtherLaw,
    name: String,
}

impl EtmaProvider<f64> for OtherInternal {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        let n = self.name.clone();
        native(py, || self.law.with(|l| Ok(format!("{}:{n}", l.law_id()))))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let n = self.name.clone();
        native(py, || {
            self.law.with(|l| {
                let mut out = Vec::new();
                l.for_each_internal(&mut |fname, f| {
                    if fname == n
                        && out.is_empty()
                        && let InternalFieldRef::Real(f) = f
                    {
                        out = f.values().element_types(filter).collect();
                    }
                });
                Ok(out)
            })
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.law.model.view::<f64>(
            py,
            ModelKey::Internal {
                handler: self.law.handler,
                law: self.law.index,
                name: self.name.clone(),
                ty,
                ghost,
            },
        )
    }
}

/// The element filter of a diffusion law or a phase field (copies).
#[derive(Debug)]
struct OtherFilter(OtherLaw);

impl EtmaProvider<i32> for OtherFilter {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        native(py, || {
            self.0.with(|l| Ok(l.element_filter().id().to_owned()))
        })
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        native(py, || {
            self.0
                .with(|l| Ok(l.element_filter().element_types(filter).collect()))
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, r, c) = native(py, || {
            self.0.with(|l| {
                let a = l.element_filter().get(ty, ghost)?;
                Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
            })
        })?;
        let a = owned_numpy(py, v, r, c, Layout::Rows)?;
        a.call_method1("setflags", (false,))?;
        Ok(a)
    }
}

impl PyConstitutiveLawInternalHandler {
    /// The base initializer of a diffusion law or phase-field object.
    pub(crate) fn init_other(law: OtherLaw) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyParameterRegistry::external(Arc::new(law.clone())))
            .add_subclass(PyParsable)
            .add_subclass(Self {
                handler: HandlerRef::Other(law),
            })
    }
}

/// `ConstitutiveLawInternalHandler` (PY-072).
#[pyclass(extends = PyParsable, subclass, frozen, name = "ConstitutiveLawInternalHandler", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyConstitutiveLawInternalHandler {
    pub(crate) handler: HandlerRef,
}

impl PyConstitutiveLawInternalHandler {
    /// A handler that owns nothing a model uses (a law constructed from
    /// Python outside any model: its internals are standalone).
    pub(crate) fn standalone(id: String, spatial_dimension: i64) -> Self {
        Self {
            handler: HandlerRef::Standalone {
                id,
                spatial_dimension,
                internals: Mutex::new(BTreeMap::new()),
            },
        }
    }

    /// The base initializer of a law object.
    pub(crate) fn init(link: Arc<LawLink>) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyParameterRegistry::external(Arc::new(LawRegistry(
            Arc::clone(&link),
        ))))
        .add_subclass(PyParsable)
        .add_subclass(Self {
            handler: HandlerRef::Law(link),
        })
    }

    /// The law link (law objects only).
    pub(crate) fn link(&self) -> Option<&Arc<LawLink>> {
        match &self.handler {
            HandlerRef::Law(l) => Some(l),
            HandlerRef::Other(_) | HandlerRef::Standalone { .. } | HandlerRef::PhaseFieldLaw(_) => {
                None
            }
        }
    }
}

fn standalone_internal<'py>(
    py: Python<'py>,
    id: &str,
    name: &str,
    int: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let full = format!("{id}:{name}");
    if int {
        let base = EtmaInt::from_provider(Arc::new(EmptyProvider(full)));
        Ok(Bound::new(
            py,
            PyClassInitializer::from(base).add_subclass(PyInternalFieldInt { law: None }),
        )?
        .into_any())
    } else {
        let base = EtmaReal::from_provider(Arc::new(EmptyProvider(full)));
        Ok(Bound::new(
            py,
            PyClassInitializer::from(base).add_subclass(PyInternalFieldReal { law: None }),
        )?
        .into_any())
    }
}

#[pymethods]
impl PyConstitutiveLawInternalHandler {
    /// `ConstitutiveLawInternalHandler(id, spatial_dimension, parent_id)`.
    #[new]
    #[pyo3(signature = (id, spatial_dimension, parent_id))]
    fn new(id: String, spatial_dimension: i64, parent_id: String) -> PyClassInitializer<Self> {
        let full = format!("{parent_id}:{id}");
        PyClassInitializer::from(PyParameterRegistry::external(Arc::new(StandaloneParams)))
            .add_subclass(PyParsable)
            .add_subclass(Self {
                handler: HandlerRef::Standalone {
                    id: full,
                    spatial_dimension,
                    internals: Mutex::new(BTreeMap::new()),
                },
            })
    }

    /// `registerInternalReal(name, nb_component)` → `InternalFieldReal`.
    #[pyo3(name = "registerInternalReal")]
    fn register_internal_real<'py>(
        slf: &Bound<'py, Self>,
        name: String,
        nb_component: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        register_internal(slf, name, nb_component, false)
    }

    /// `registerInternalInt(name, nb_component)` → `InternalFieldInt`.
    #[pyo3(name = "registerInternalInt")]
    fn register_internal_int<'py>(
        slf: &Bound<'py, Self>,
        name: String,
        nb_component: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        register_internal(slf, name, nb_component, true)
    }

    /// `getInternalReal(id)` → `InternalFieldReal`.
    #[pyo3(name = "getInternalReal")]
    fn get_internal_real<'py>(slf: &Bound<'py, Self>, id: String) -> PyResult<Bound<'py, PyAny>> {
        internal_object(slf, id, false)
    }

    /// `getInternalInt(id)` → `InternalFieldInt`.
    #[pyo3(name = "getInternalInt")]
    fn get_internal_int<'py>(slf: &Bound<'py, Self>, id: String) -> PyResult<Bound<'py, PyAny>> {
        internal_object(slf, id, true)
    }

    /// `getElementFilter()`: `ElementTypeMapArrayInt` whose blocks are
    /// read-only snapshot copies (D-056 item 2, UD-747; upstream returns
    /// writable views of the law's filter).
    #[pyo3(name = "getElementFilter")]
    fn get_element_filter(&self) -> EtmaInt {
        match &self.handler {
            HandlerRef::Law(l) => EtmaInt::from_provider(Arc::new(FilterProvider(Arc::clone(l)))),
            HandlerRef::Other(o) => EtmaInt::from_provider(Arc::new(OtherFilter(o.clone()))),
            HandlerRef::Standalone { id, .. } => {
                EtmaInt::from_provider(Arc::new(EmptyProvider(format!("{id}:element_filter"))))
            }
            HandlerRef::PhaseFieldLaw(l) => match l.other() {
                Ok(o) => EtmaInt::from_provider(Arc::new(OtherFilter(o))),
                Err(_) => {
                    EtmaInt::from_provider(Arc::new(EmptyProvider("element_filter".to_owned())))
                }
            },
        }
    }

    #[pyo3(name = "getSpatialDimension")]
    fn get_spatial_dimension(&self, py: Python<'_>) -> PyResult<i64> {
        match &self.handler {
            HandlerRef::Standalone {
                spatial_dimension, ..
            } => Ok(*spatial_dimension),
            HandlerRef::Other(o) => {
                let d = o.model.with_ops(py, |m| Ok(m.spatial_dimension()))?;
                Ok(i64::try_from(d).unwrap_or(i64::MAX))
            }
            HandlerRef::PhaseFieldLaw(l) => {
                let o = l.other().py_err(py)?;
                let d = o.model.with_ops(py, |m| Ok(m.spatial_dimension()))?;
                Ok(i64::try_from(d).unwrap_or(i64::MAX))
            }
            HandlerRef::Law(l) => {
                if let Some(AdmittedLent(lent, key)) = l.lent_top(py).py_err(py)? {
                    return native(py, move || match lent {
                        Lent::Mut { state, .. } => {
                            state.with_mut(key, |s| s.law_base().spatial_dimension)
                        }
                        Lent::Shared(s) => s.with(key, |s| s.law_base().spatial_dimension),
                    })
                    .map(|d| i64::try_from(d).unwrap_or(i64::MAX));
                }
                let d = native(py, || {
                    l.with_law(|smm, i| Ok(smm.material(i)?.base().spatial_dimension))
                })?;
                Ok(i64::try_from(d).unwrap_or(i64::MAX))
            }
        }
    }
}

/// The (parameter-less) registry of a standalone internal handler.
#[derive(Debug)]
struct StandaloneParams;

impl RegistryAccess for StandaloneParams {
    fn with_registry(
        &self,
        _prepared: &Prepared,
        _f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        Err(Error::Unsupported(
            "parameters of a standalone ConstitutiveLawInternalHandler".into(),
        ))
    }
}

fn register_internal<'py>(
    slf: &Bound<'py, PyConstitutiveLawInternalHandler>,
    name: String,
    nb_component: usize,
    int: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    match &slf.get().handler {
        HandlerRef::Law(l) => {
            let n = name.clone();
            let link = Arc::clone(l);
            let admitted = link.lent_top(py).py_err(py)?;
            native(py, move || {
                link.register_internal(admitted, &n, nb_component, int)
            })?;
            internal_object(slf, name, int)
        }
        HandlerRef::Standalone { id, internals, .. } => {
            lock(internals).insert(name.clone(), (nb_component, int));
            standalone_internal(py, id, &name, int)
        }
        HandlerRef::Other(_) => Err(super::errors::exception(
            py,
            "registering internals on a built-in law is not supported".into(),
        )),
        HandlerRef::PhaseFieldLaw(l) => {
            l.register_internal(py, &name, nb_component, int)?;
            internal_object(slf, name, int)
        }
    }
}

fn internal_object<'py>(
    slf: &Bound<'py, PyConstitutiveLawInternalHandler>,
    name: String,
    int: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let py = slf.py();
    match &slf.get().handler {
        HandlerRef::Law(l) => {
            // upstream looks the internal up at once
            if let Some(Err(msg)) = l.has_internal(py, &name, int)? {
                return Err(super::errors::exception(py, msg));
            }
            if int {
                let p = Arc::new(LawInternalProvider {
                    link: Arc::clone(l),
                    name,
                    previous: false,
                });
                Ok(Bound::new(
                    py,
                    PyClassInitializer::from(EtmaInt::from_provider(p.clone()))
                        .add_subclass(PyInternalFieldInt { law: Some(p) }),
                )?
                .into_any())
            } else {
                let p = Arc::new(LawInternalProvider {
                    link: Arc::clone(l),
                    name,
                    previous: false,
                });
                Ok(Bound::new(
                    py,
                    PyClassInitializer::from(EtmaReal::from_provider(p.clone()))
                        .add_subclass(PyInternalFieldReal { law: Some(p) }),
                )?
                .into_any())
            }
        }
        HandlerRef::Other(o) => {
            if int {
                return Err(super::errors::exception(
                    py,
                    "integer internals of this law are not bound".into(),
                ));
            }
            let n = name.clone();
            let found = native(py, || {
                o.with(|l| {
                    let mut found = false;
                    l.for_each_internal(&mut |fname, f| {
                        found |= fname == n && matches!(f, InternalFieldRef::Real(_));
                    });
                    Ok(if found {
                        Ok(())
                    } else {
                        Err(format!(
                            "The constitutive law {}({}) does not contain an internal {n}",
                            l.name(),
                            l.law_id()
                        ))
                    })
                })
            });
            // undecidable (the model is held by a running call): lazy
            if let Ok(Err(msg)) = found {
                return Err(super::errors::exception(py, msg));
            }
            let base = EtmaReal::from_provider(Arc::new(OtherInternal {
                law: o.clone(),
                name,
            }));
            Ok(Bound::new(
                py,
                PyClassInitializer::from(base).add_subclass(PyInternalFieldReal { law: None }),
            )?
            .into_any())
        }
        HandlerRef::Standalone { id, internals, .. } => {
            if !lock(internals).contains_key(&name) {
                return Err(super::errors::exception(
                    py,
                    format!("The internal {name} is not registered in {id}"),
                ));
            }
            standalone_internal(py, id, &name, int)
        }
        HandlerRef::PhaseFieldLaw(l) => {
            if int {
                return Err(super::errors::exception(
                    py,
                    "integer internals of a Python phase field are not bound".into(),
                ));
            }
            let base = EtmaReal::from_provider(Arc::new(PfInternal {
                link: Arc::clone(l),
                name,
            }));
            Ok(Bound::new(
                py,
                PyClassInitializer::from(base).add_subclass(PyInternalFieldReal { law: None }),
            )?
            .into_any())
        }
    }
}

/// A real internal of a Python phase field: the lent state inside its
/// callbacks, the model's law otherwise.
#[derive(Debug)]
struct PfInternal {
    link: Arc<super::pf_laws::PfLink>,
    name: String,
}

impl EtmaProvider<f64> for PfInternal {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        match self.link.other() {
            Ok(o) => native(py, || {
                o.with(|l| Ok(format!("{}:{}", l.law_id(), self.name)))
            }),
            Err(_) => Ok(self.name.clone()),
        }
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let o = self.link.other().py_err(py)?;
        native(py, || {
            o.with(|l| Ok(l.element_filter().element_types(filter).collect()))
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.link.real_view(py, &self.name, ty, ghost)
    }
}

/// `ConstitutiveLaw<SolidMechanicsModel>` (PY-073; upstream class name
/// `ConstitutiveLawakantu::SolidMechanicsModel`): the `set*` methods that
/// call `updateInternalParameters()` after setting.
#[pyclass(extends = PyConstitutiveLawInternalHandler, subclass, frozen, name = "ConstitutiveLaw_SolidMechanicsModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyConstitutiveLawSolid;

fn registry_of<'a, 'py>(
    slf: &'a Bound<'py, PyConstitutiveLawSolid>,
) -> &'a Bound<'py, PyParameterRegistry> {
    slf.as_super().as_super().as_super()
}

#[pymethods]
impl PyConstitutiveLawSolid {
    #[pyo3(name = "setReal", signature = (name, value))]
    fn set_real(slf: &Bound<'_, Self>, name: &str, value: Real) -> PyResult<()> {
        registry_of(slf)
            .get()
            .set_value(slf.py(), name, ParamValue::Real(value))
    }

    #[pyo3(name = "setBool", signature = (name, value))]
    fn set_bool(slf: &Bound<'_, Self>, name: &str, value: bool) -> PyResult<()> {
        registry_of(slf)
            .get()
            .set_value(slf.py(), name, ParamValue::Bool(value))
    }

    #[pyo3(name = "setString", signature = (name, value))]
    fn set_string(slf: &Bound<'_, Self>, name: &str, value: String) -> PyResult<()> {
        registry_of(slf)
            .get()
            .set_value(slf.py(), name, ParamValue::String(value))
    }

    #[pyo3(name = "setInt", signature = (name, value))]
    fn set_int(slf: &Bound<'_, Self>, name: &str, value: i32) -> PyResult<()> {
        registry_of(slf)
            .get()
            .set_value(slf.py(), name, ParamValue::Int(i64::from(value)))
    }
}

// ---- internal fields ----------------------------------------------------------------

/// `(values, rows, nb_component)` copy of an array block.
type Block<T> = (Vec<T>, usize, usize);

/// Copy the history block of a typed internal (`None` for another type).
type PickBlock<T> = fn(InternalFieldRef<'_>, ElementType, GhostType) -> Option<Result<Block<T>>>;

/// An internal of a law (`getInternalReal(name)`, `registerInternalReal`).
#[derive(Debug)]
pub(crate) struct LawInternalProvider {
    link: Arc<LawLink>,
    name: String,
    /// `previous()`: read-only copies of the history.
    previous: bool,
}

impl LawInternalProvider {
    fn previous_block<T: Copy + Default + Send + 'static>(
        &self,
        py: Python<'_>,
        ty: ElementType,
        ghost: GhostType,
        pick: PickBlock<T>,
    ) -> PyResult<Block<T>> {
        let n = self.name.clone();
        native(py, || {
            self.link.with_law(|smm, i| {
                let mut out = None;
                smm.material(i)?.for_each_internal(&mut |fname, f| {
                    if fname == n && out.is_none() {
                        out = pick(f, ty, ghost);
                    }
                });
                out.unwrap_or_else(|| Err(Error::not_found("internal history", n.clone())))
            })
        })
    }
}

fn previous_real(
    f: InternalFieldRef<'_>,
    ty: ElementType,
    ghost: GhostType,
) -> Option<Result<(Vec<Real>, usize, usize)>> {
    match f {
        InternalFieldRef::Real(f) => Some(
            f.previous(ty, ghost)
                .map(|a| (a.as_slice().to_vec(), a.len(), a.nb_component())),
        ),
        _ => None,
    }
}

fn previous_int(
    f: InternalFieldRef<'_>,
    ty: ElementType,
    ghost: GhostType,
) -> Option<Result<(Vec<i32>, usize, usize)>> {
    match f {
        InternalFieldRef::Int(f) => Some(
            f.previous(ty, ghost)
                .map(|a| (a.as_slice().to_vec(), a.len(), a.nb_component())),
        ),
        _ => None,
    }
}

fn law_internal_id(link: &LawLink, py: Python<'_>, name: &str) -> PyResult<String> {
    let id = lock(&link.state).id.clone();
    match id {
        Some(id) => Ok(format!("{id}:{name}")),
        None => {
            let n = name.to_owned();
            native(py, || {
                link.with_law(|smm, i| Ok(format!("{}:{n}", smm.material(i)?.law_id())))
            })
        }
    }
}

impl EtmaProvider<f64> for LawInternalProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        let id = law_internal_id(&self.link, py, &self.name)?;
        Ok(if self.previous {
            format!("previous_{id}")
        } else {
            id
        })
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.link.internal_types(py, &self.name, filter)
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.previous {
            let (v, r, c) = self.previous_block(py, ty, ghost, previous_real)?;
            let a = owned_numpy(py, v, r, c, Layout::Rows)?;
            a.call_method1("setflags", (false,))?;
            return Ok(a);
        }
        self.link.real_view(py, &self.name, ty, ghost)
    }
}

impl EtmaProvider<i32> for LawInternalProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        <Self as EtmaProvider<f64>>::id(self, py)
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.link.internal_types(py, &self.name, filter)
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        if self.previous {
            let (v, r, c) = self.previous_block(py, ty, ghost, previous_int)?;
            let a = owned_numpy(py, v, r, c, Layout::Rows)?;
            a.call_method1("setflags", (false,))?;
            return Ok(a);
        }
        self.link.int_view(py, &self.name, ty, ghost)
    }
}

/// An empty map (internals of a standalone handler).
#[derive(Debug)]
struct EmptyProvider(String);

impl<T: crate::foreign_view::numpy_export::Exportable + Default + std::fmt::Debug> EtmaProvider<T>
    for EmptyProvider
{
    fn id(&self, _py: Python<'_>) -> PyResult<String> {
        Ok(self.0.clone())
    }
    fn types(&self, _py: Python<'_>, _filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        Ok(Vec::new())
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        Err(super::errors::exception(
            py,
            format!("{}: no block ({ty}, {ghost})", self.0),
        ))
    }
}

/// The element filter of a law (read-only snapshot copies, UD-747).
#[derive(Debug)]
struct FilterProvider(Arc<LawLink>);

impl EtmaProvider<i32> for FilterProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        law_internal_id(&self.0, py, "element_filter")
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.0.filter_types(py, filter)
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, r, c) = self.0.filter_block(py, ty, ghost)?;
        let a = owned_numpy(py, v, r, c, Layout::Rows)?;
        a.call_method1("setflags", (false,))?;
        Ok(a)
    }
}

macro_rules! internal_field_class {
    ($rust:ident, $base:ident, $pyname:literal, $t:ty) => {
        #[doc = concat!("`", $pyname, "` (PY-071): an internal of a law.")]
        #[pyclass(extends = $base, frozen, name = $pyname, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust {
            law: Option<Arc<LawInternalProvider>>,
        }

        #[pymethods]
        impl $rust {
            /// `previous(type, ghost_type=_not_ghost)` (read-only copy of
            /// the history block) or `previous()` (the history field).
            #[pyo3(signature = (r#type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
            fn previous<'py>(
                slf: &Bound<'py, Self>,
                r#type: Option<EtArg>,
                ghost_type: GhostArg,
            ) -> PyResult<Bound<'py, PyAny>> {
                let py = slf.py();
                let p = slf.get().law.as_ref().ok_or_else(|| {
                    super::errors::exception(py, "no history in a standalone handler".into())
                })?;
                let prev = Arc::new(LawInternalProvider {
                    link: Arc::clone(&p.link),
                    name: p.name.clone(),
                    previous: true,
                });
                match r#type {
                    Some(t) => {
                        let ghost = ghost_type.single()?;
                        EtmaProvider::<$t>::call(prev.as_ref(), py, t.0, ghost)
                    }
                    None => Ok(Bound::new(
                        py,
                        PyClassInitializer::from($base::from_provider(prev.clone()))
                            .add_subclass(Self { law: Some(prev) }),
                    )?
                    .into_any()),
                }
            }

            /// `self(type, ghost_type=_not_ghost)`.
            #[pyo3(signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
            fn __call__<'py>(
                slf: &Bound<'py, Self>,
                r#type: EtArg,
                ghost_type: GhostArg,
            ) -> PyResult<Bound<'py, PyAny>> {
                slf.as_super()
                    .get()
                    .provider
                    .call(slf.py(), r#type.0, ghost_type.single()?)
            }

            fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
                let py = slf.py();
                let p = &slf.as_super().get().provider;
                let id = p.id(py)?;
                let mut s = format!(concat!("<", $pyname, ": {} "), id);
                let mut first = true;
                for ghost in [GhostType::NotGhost, GhostType::Ghost] {
                    let filter = TypeFilter {
                        spatial_dimension: None,
                        ghost_type: ghost,
                        kind: None,
                    };
                    for ty in p.types(py, filter)? {
                        let a = p.call(py, ty, ghost)?;
                        let shape: (usize, usize) = a.getattr("shape")?.extract()?;
                        if !first {
                            s.push_str(", ");
                        }
                        first = false;
                        s.push_str(&format!(
                            "[{ty}:{ghost} - Array<{} {}, {}>]",
                            stringify!($t),
                            shape.0,
                            shape.1
                        ));
                    }
                }
                s.push('>');
                Ok(s)
            }

            #[pyo3(name = "getID")]
            fn get_id(slf: &Bound<'_, Self>) -> PyResult<String> {
                slf.as_super().get().provider.id(slf.py())
            }

            /// `initializeHistory()`.
            #[pyo3(name = "initializeHistory")]
            fn initialize_history(slf: &Bound<'_, Self>) -> PyResult<()> {
                let py = slf.py();
                let Some(p) = slf.get().law.as_ref() else {
                    return Ok(());
                };
                let n = p.name.clone();
                let link = Arc::clone(&p.link);
                history(py, &link, n, true).map(drop)
            }

            /// `hasHistory()`.
            #[pyo3(name = "hasHistory")]
            fn has_history(slf: &Bound<'_, Self>) -> PyResult<bool> {
                let py = slf.py();
                let Some(p) = slf.get().law.as_ref() else {
                    return Ok(false);
                };
                let n = p.name.clone();
                let link = Arc::clone(&p.link);
                history(py, &link, n, false)
            }
        }
    };
}

/// `initializeHistory()` (`init == true`) or `hasHistory()` of an internal.
fn history(py: Python<'_>, link: &Arc<LawLink>, name: String, init: bool) -> PyResult<bool> {
    let n2 = name.clone();
    let in_state = move |s: &mut (dyn RuntimeLawState + 'static)| -> Result<bool> {
        let name = n2;
        if let Some(f) = s.own_real_internal_mut(&name) {
            if init {
                f.initialize_history();
            }
            return Ok(f.has_history());
        }
        for f in s.law_base_mut().internals_mut() {
            if f.name() == name {
                if init {
                    f.initialize_history();
                }
                return Ok(f.has_history());
            }
        }
        match s.runtime_internals_mut().get_mut(&name)? {
            AnyInternalField::Real(f) => {
                if init {
                    f.initialize_history();
                }
                Ok(f.has_history())
            }
            AnyInternalField::Int(f) => {
                if init {
                    f.initialize_history();
                }
                Ok(f.has_history())
            }
            _ => Err(Error::invalid(format!("internal '{name}' of another type"))),
        }
    };
    if let Some(AdmittedLent(lent, key)) = link.lent_top(py).py_err(py)? {
        return native(py, move || match lent {
            Lent::Mut { state, .. } => state.with_mut(key, in_state)?,
            Lent::Shared(s) => s.with(key, |s| {
                for (fname, f) in s.law_base().internals() {
                    if fname == name {
                        return Ok(f.has_history());
                    }
                }
                for (fname, f) in s.own_real_internals() {
                    if fname == name {
                        return Ok(f.has_history());
                    }
                }
                match s.runtime_internals().get(&name)? {
                    AnyInternalField::Real(f) => Ok(f.has_history()),
                    AnyInternalField::Int(f) => Ok(f.has_history()),
                    _ => Ok(false),
                }
            })?,
        });
    }
    native(py, || {
        link.with_law(|smm, i| {
            let law = smm.material_mut(i)?;
            let mut out = None;
            law.for_each_internal_mut(&mut |fname, f| {
                if fname == name && out.is_none() {
                    out = Some(match f {
                        akantu::model::constitutive_law::InternalFieldMut::Real(f) => {
                            if init {
                                f.initialize_history();
                            }
                            f.has_history()
                        }
                        akantu::model::constitutive_law::InternalFieldMut::Int(f) => {
                            if init {
                                f.initialize_history();
                            }
                            f.has_history()
                        }
                        _ => false,
                    });
                }
                Ok(())
            })?;
            out.ok_or_else(|| Error::not_found("internal", name.clone()))
        })
    })
}

internal_field_class!(PyInternalFieldReal, EtmaReal, "InternalFieldReal", f64);
internal_field_class!(PyInternalFieldInt, EtmaInt, "InternalFieldInt", i32);

// ---- the trampoline ---------------------------------------------------------------------

/// Overridable methods of a Python `Material` subclass.
const LAW_METHODS: [&str; 7] = [
    "initMaterial",
    "computeStress",
    "computeTangentModuli",
    "computePotentialEnergy",
    "getPushWaveSpeed",
    "getShearWaveSpeed",
    "hasStiffnessMatrixChanged",
];

/// Whether the class of `obj` defines the method `name` beyond the
/// `Material` base (pybind11's `get_override` finds a Python function there,
/// not the bound C++ method).
///
/// The class only (D-104): pybind11's `get_override` looks up
/// `getattr(self, name)`, so a function assigned to the instance can run
/// upstream (subject to pybind11's per-type cache of types without
/// overrides); the port ignores instance attributes (UD-1290, category C).
pub(crate) fn class_overrides(obj: &Bound<'_, PyAny>, name: &str) -> bool {
    let base = obj.py().get_type::<super::solid::PyMaterial>();
    let Ok(m) = obj.get_type().getattr(name) else {
        return false;
    };
    match base.getattr(name) {
        Ok(b) => !m.is(&b),
        Err(_) => true,
    }
}

/// Which [`LAW_METHODS`] the class of `obj` defines beyond the `Material`
/// base.
fn law_overrides(obj: &Bound<'_, PyAny>) -> PyResult<[bool; 7]> {
    let mut out = [false; 7];
    for (k, name) in LAW_METHODS.iter().enumerate() {
        out[k] = class_overrides(obj, name);
    }
    Ok(out)
}

/// The trampoline of a Python `Material` subclass (upstream `PyMaterial`).
#[derive(Debug)]
struct PyLawMethods {
    obj: Arc<Py<PyAny>>,
    link: Arc<LawLink>,
    overrides: [bool; 7],
}

fn not_overridden(name: &str) -> Error {
    Error::Unsupported(format!(
        "{name} of a Python Material subclass (the subclass does not define it)"
    ))
}

impl PyLawMethods {
    /// Call the Python method with the law state lent `&mut`.
    fn lend<R>(
        &self,
        law: &mut ForeignLaw<'_, '_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> Result<R> {
        let phase = law.ctx.phase;
        let services = law.ctx.services;
        let state: &mut dyn RuntimeLawState = &mut *law.state;
        let link = &self.link;
        let obj = &self.obj;
        let model = link.model()?;
        lending_callback(|py, cx| {
            with_model_services(services, cx.key(), |hs| {
                with_scoped_mut(state, cx.key(), |st| {
                    let _l = link.push_lent(
                        Lent::Mut {
                            state: st,
                            phase,
                            services: hs.clone(),
                        },
                        cx,
                    );
                    let _s = model.h.push_services(model.part, hs, cx);
                    f(obj.bind(py))
                })
            })
        })
    }

    /// Call the Python method with the law state lent `&` (const methods).
    fn lend_shared<R>(
        &self,
        state: &ForeignLawState,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> Result<R> {
        let link = &self.link;
        let obj = &self.obj;
        let state: &dyn RuntimeLawState = state;
        lending_callback(|py, cx| {
            with_scoped(state, cx.key(), |st| {
                let _l = link.push_lent(Lent::Shared(st), cx);
                f(obj.bind(py))
            })
        })
    }
}

impl ForeignMaterialMethods for PyLawMethods {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        if !self.overrides[0] {
            return law.base_init_material();
        }
        self.lend(law, |o| o.call_method0("initMaterial").map(drop))
    }

    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if !self.overrides[1] {
            return Err(not_overridden("computeStress"));
        }
        self.lend(law, |o| {
            let py = o.py();
            o.call_method1(
                "computeStress",
                (element_type_obj(py, ty)?, ghost_type_obj(py, ghost)?),
            )
            .map(drop)
        })
    }

    fn compute_tangent_moduli(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        if !self.overrides[2] {
            return Err(not_overridden("computeTangentModuli"));
        }
        let (values, rows, nbc) = (
            tangent.as_slice().to_vec(),
            tangent.len(),
            tangent.nb_component(),
        );
        let out: Vec<Real> = self.lend(law, |o| {
            let py = o.py();
            // upstream passes a view of the temporary tangent array; the
            // port passes a NumPy copy and copies it back
            let arr = owned_numpy(py, values, rows, nbc, Layout::Rows)?;
            o.call_method1(
                "computeTangentModuli",
                (element_type_obj(py, ty)?, &arr, ghost_type_obj(py, ghost)?),
            )?;
            arr.call_method0("ravel")?.call_method0("tolist")?.extract()
        })?;
        if out.len() != tangent.as_slice().len() {
            return Err(Error::DimensionMismatch {
                context: "computeTangentModuli (tangent_matrix)",
                expected: tangent.as_slice().len(),
                got: out.len(),
            });
        }
        tangent.as_mut_slice().copy_from_slice(&out);
        Ok(())
    }

    fn compute_potential_energy(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
    ) -> Result<()> {
        if !self.overrides[3] {
            return Ok(());
        }
        self.lend(law, |o| {
            o.call_method1("computePotentialEnergy", (element_type_obj(o.py(), ty)?,))
                .map(drop)
        })
    }

    fn push_wave_speed(&self, state: &ForeignLawState, element: Element) -> Result<Real> {
        if !self.overrides[4] {
            return Err(not_overridden("getPushWaveSpeed"));
        }
        self.lend_shared(state, |o| {
            o.call_method1("getPushWaveSpeed", (PyElement { element },))?
                .extract()
        })
    }

    fn shear_wave_speed(&self, state: &ForeignLawState, element: Element) -> Result<Real> {
        if !self.overrides[5] {
            return Err(not_overridden("getShearWaveSpeed"));
        }
        self.lend_shared(state, |o| {
            o.call_method1("getShearWaveSpeed", (PyElement { element },))?
                .extract()
        })
    }

    /// Upstream `PYBIND11_OVERRIDE(bool, _Material, hasStiffnessMatrixChanged)`
    /// (`py_material.cc:85-88`): the Python answer
    /// decides whether `K` is re-assembled.
    fn has_stiffness_matrix_changed(
        &self,
        state: &ForeignLawState,
        _ctx: &LawContext<'_>,
    ) -> Result<bool> {
        if !self.overrides[6] {
            return Ok(true);
        }
        self.lend_shared(state, |o| {
            o.call_method0("hasStiffnessMatrixChanged")?.extract()
        })
    }
}

// ---- laws derived from a built-in law (D-078, PY-077) -------------------------

/// Call a Python law method with `state` lent `&mut` and the phase
/// services (the lending of [`PyLawMethods::lend`] for any runtime state).
fn lend_state<R>(
    link: &Arc<LawLink>,
    obj: &Arc<Py<PyAny>>,
    state: &mut (dyn RuntimeLawState + 'static),
    ctx: &LawContext<'_>,
    f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
) -> Result<R> {
    let phase = ctx.phase;
    let services = ctx.services;
    let model = link.model()?;
    lending_callback(|py, cx| {
        with_model_services(services, cx.key(), |hs| {
            with_scoped_mut(state, cx.key(), |st| {
                let _l = link.push_lent(
                    Lent::Mut {
                        state: st,
                        phase,
                        services: hs.clone(),
                    },
                    cx,
                );
                let _s = model.h.push_services(model.part, hs, cx);
                f(obj.bind(py))
            })
        })
    })
}

/// Call a Python law method with `state` lent `&` (const methods).
fn lend_state_shared<R>(
    link: &Arc<LawLink>,
    obj: &Arc<Py<PyAny>>,
    state: &(dyn RuntimeLawState + 'static),
    f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
) -> Result<R> {
    lending_callback(|py, cx| {
        with_scoped(state, cx.key(), |st| {
            let _l = link.push_lent(Lent::Shared(st), cx);
            f(obj.bind(py))
        })
    })
}

/// The trampoline of a Python `MaterialLinearElastic1D/2D/3D` subclass
/// (upstream `PyMaterial<MaterialElastic<dim>>`): an overridden method
/// calls Python, every other one is the elastic law's.
#[derive(Debug)]
struct PyDerivedMethods {
    obj: Arc<Py<PyAny>>,
    link: Arc<LawLink>,
    overrides: [bool; 7],
}

impl DerivedMaterialMethods for PyDerivedMethods {
    fn init_material(&mut self, law: &mut DerivedLaw<'_, '_>) -> Result<()> {
        if !self.overrides[0] {
            return law.base_init_material();
        }
        lend_state(&self.link, &self.obj, law.state, law.ctx, |o| {
            o.call_method0("initMaterial").map(drop)
        })
    }

    fn compute_stress(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if !self.overrides[1] {
            return law.state.law.compute_stress(law.ctx, ty, ghost);
        }
        lend_state(&self.link, &self.obj, law.state, law.ctx, |o| {
            let py = o.py();
            o.call_method1(
                "computeStress",
                (element_type_obj(py, ty)?, ghost_type_obj(py, ghost)?),
            )
            .map(drop)
        })
    }

    fn compute_tangent_moduli(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        if !self.overrides[2] {
            return law
                .state
                .law
                .compute_tangent_moduli(law.ctx, ty, ghost, tangent);
        }
        let (values, rows, nbc) = (
            tangent.as_slice().to_vec(),
            tangent.len(),
            tangent.nb_component(),
        );
        let out: Vec<Real> = lend_state(&self.link, &self.obj, law.state, law.ctx, |o| {
            let py = o.py();
            // a NumPy copy of the temporary tangent array, copied back (as
            // for a `Material` subclass)
            let arr = owned_numpy(py, values, rows, nbc, Layout::Rows)?;
            o.call_method1(
                "computeTangentModuli",
                (element_type_obj(py, ty)?, &arr, ghost_type_obj(py, ghost)?),
            )?;
            arr.call_method0("ravel")?.call_method0("tolist")?.extract()
        })?;
        if out.len() != tangent.as_slice().len() {
            return Err(Error::DimensionMismatch {
                context: "computeTangentModuli (tangent_matrix)",
                expected: tangent.as_slice().len(),
                got: out.len(),
            });
        }
        tangent.as_mut_slice().copy_from_slice(&out);
        Ok(())
    }

    fn compute_potential_energy(
        &mut self,
        law: &mut DerivedLaw<'_, '_>,
        ty: ElementType,
    ) -> Result<()> {
        if !self.overrides[3] {
            return law.state.law.compute_potential_energy(law.ctx, ty);
        }
        lend_state(&self.link, &self.obj, law.state, law.ctx, |o| {
            o.call_method1("computePotentialEnergy", (element_type_obj(o.py(), ty)?,))
                .map(drop)
        })
    }

    fn push_wave_speed(&self, state: &DerivedLawState, element: Element) -> Result<Real> {
        if !self.overrides[4] {
            return state.law.push_wave_speed(element);
        }
        lend_state_shared(&self.link, &self.obj, state, |o| {
            o.call_method1("getPushWaveSpeed", (PyElement { element },))?
                .extract()
        })
    }

    fn shear_wave_speed(&self, state: &DerivedLawState, element: Element) -> Result<Real> {
        if !self.overrides[5] {
            return state.law.shear_wave_speed(element);
        }
        lend_state_shared(&self.link, &self.obj, state, |o| {
            o.call_method1("getShearWaveSpeed", (PyElement { element },))?
                .extract()
        })
    }

    fn has_stiffness_matrix_changed(
        &self,
        state: &DerivedLawState,
        ctx: &LawContext<'_>,
    ) -> Result<bool> {
        if !self.overrides[6] {
            return Ok(state.law.has_stiffness_matrix_changed_in(ctx));
        }
        lend_state_shared(&self.link, &self.obj, state, |o| {
            o.call_method0("hasStiffnessMatrixChanged")?.extract()
        })
    }
}

/// The dimension of the elastic base class of `obj`
/// (`MaterialLinearElastic<d>D`), if it derives from one.
fn elastic_base_dim(obj: &Bound<'_, PyAny>) -> Option<usize> {
    if obj.is_instance_of::<super::solid::PyMaterialLinearElastic1D>() {
        Some(1)
    } else if obj.is_instance_of::<super::solid::PyMaterialLinearElastic2D>() {
        Some(2)
    } else if obj.is_instance_of::<super::solid::PyMaterialLinearElastic3D>() {
        Some(3)
    } else {
        None
    }
}

// ---- runtime cohesive laws (D-078) -------------------------------------------

/// The methods a Python `MaterialCohesive` subclass overrides (upstream
/// `PyMaterialCohesive`, `py_material.cc:99-130`).
const COHESIVE_LAW_METHODS: [&str; 4] = [
    "initMaterial",
    "computeTraction",
    "computeTangentTraction",
    "checkInsertion",
];

/// Which [`COHESIVE_LAW_METHODS`] the class of `obj` defines beyond the
/// `MaterialCohesive` base.
fn cohesive_overrides(obj: &Bound<'_, PyAny>) -> PyResult<[bool; 4]> {
    let py = obj.py();
    let base = py.get_type::<super::cohesive::PyMaterialCohesive>();
    let ty = obj.get_type();
    let mut out = [false; 4];
    for (k, name) in COHESIVE_LAW_METHODS.iter().enumerate() {
        let Ok(m) = ty.getattr(*name) else { continue };
        out[k] = match base.getattr(*name) {
            Ok(b) => !m.is(&b),
            Err(_) => true,
        };
    }
    Ok(out)
}

/// The trampoline of a Python `MaterialCohesive` subclass: every call goes
/// through [`lending_callback`] with the law lent as a
/// [`RuntimeLawState`] (the same lending layer as `Material` subclasses);
/// `checkInsertion` runs with the model released
/// (the model's `checkCohesiveStress` binding), never from native code.
#[derive(Debug)]
struct PyCohesiveLawMethods {
    obj: Arc<Py<PyAny>>,
    link: Arc<LawLink>,
    overrides: [bool; 4],
}

fn cohesive_not_overridden(name: &str) -> Error {
    Error::NotImplemented(match name {
        "computeTraction" => {
            "computeTraction of a Python MaterialCohesive subclass that does not define it \
             (upstream: pure virtual)"
        }
        _ => {
            "computeTangentTraction of a Python MaterialCohesive subclass that does not define it \
             (upstream MaterialCohesive: AKANTU_TO_IMPLEMENT)"
        }
    })
}

impl PyCohesiveLawMethods {
    /// Call the Python method with the law lent `&mut`.
    fn lend<R>(
        &self,
        law: &mut akantu::cohesive::MaterialCohesive,
        ctx: &LawContext<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> Result<R> {
        let phase = ctx.phase;
        let services = ctx.services;
        let state: &mut dyn RuntimeLawState = law;
        let link = &self.link;
        let obj = &self.obj;
        let model = link.model()?;
        lending_callback(|py, cx| {
            with_model_services(services, cx.key(), |hs| {
                with_scoped_mut(state, cx.key(), |st| {
                    let _l = link.push_lent(
                        Lent::Mut {
                            state: st,
                            phase,
                            services: hs.clone(),
                        },
                        cx,
                    );
                    let _s = model.h.push_services(model.part, hs, cx);
                    f(obj.bind(py))
                })
            })
        })
    }
}

impl akantu::cohesive::laws::foreign::ForeignCohesiveMethods for PyCohesiveLawMethods {
    fn init_material(
        &mut self,
        law: &mut akantu::cohesive::MaterialCohesive,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        if !self.overrides[0] {
            return law.base_init(ctx);
        }
        self.lend(law, ctx, |o| o.call_method0("initMaterial").map(drop))
    }

    fn compute_traction(
        &mut self,
        law: &mut akantu::cohesive::MaterialCohesive,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if !self.overrides[1] {
            return Err(cohesive_not_overridden("computeTraction"));
        }
        self.lend(law, ctx, |o| {
            let py = o.py();
            o.call_method1(
                "computeTraction",
                (element_type_obj(py, ty)?, ghost_type_obj(py, ghost)?),
            )
            .map(drop)
        })
    }

    fn compute_tangent_traction(
        &mut self,
        law: &mut akantu::cohesive::MaterialCohesive,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut [Real],
    ) -> Result<()> {
        if !self.overrides[2] {
            return Err(cohesive_not_overridden("computeTangentTraction"));
        }
        let nbc = law.core.dim * law.core.dim;
        let rows = tangent.len().checked_div(nbc).unwrap_or(0);
        let values = tangent.to_vec();
        let out: Vec<Real> = self.lend(law, ctx, |o| {
            let py = o.py();
            // upstream passes a view of the temporary tangent array; the
            // port passes a NumPy copy and copies it back (as
            // `computeTangentModuli` of a Python `Material`)
            let arr = owned_numpy(py, values, rows, nbc, Layout::Rows)?;
            o.call_method1(
                "computeTangentTraction",
                (element_type_obj(py, ty)?, &arr, ghost_type_obj(py, ghost)?),
            )?;
            arr.call_method0("ravel")?.call_method0("tolist")?.extract()
        })?;
        if out.len() != tangent.len() {
            return Err(Error::DimensionMismatch {
                context: "computeTangentTraction (tangent_matrix)",
                expected: tangent.len(),
                got: out.len(),
            });
        }
        tangent.copy_from_slice(&out);
        Ok(())
    }

    fn has_insertion_check(&self) -> bool {
        self.overrides[3]
    }
}

// ---- MaterialFactory ---------------------------------------------------------------------

/// Python allocators registered with `MaterialFactory.registerAllocator`.
static PY_MATERIALS: Mutex<BTreeMap<String, Arc<Py<PyAny>>>> = Mutex::new(BTreeMap::new());

fn builtin_material_names() -> Result<Vec<String>> {
    let mut f = MaterialFactory::default();
    akantu::solid::material::register_builtin_materials(&mut f)?;
    Ok(f.names().map(str::to_owned).collect())
}

/// The allocator of the Python material `name` for the model `model`.
fn python_allocator(
    name: String,
    func: Arc<Py<PyAny>>,
    model: Weak<ModelHandle>,
    part: Part,
) -> akantu::model::constitutive_law::Allocator<dyn Material> {
    Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
        let h = model
            .upgrade()
            .ok_or_else(|| Error::invalid("the model of this material factory was destroyed"))?;
        let mref = ModelRef {
            h: Arc::clone(&h),
            part,
        };
        let (obj, link, overrides, cohesive, elastic_dim) = callback(|py| {
            let model_obj = super::solid::model_object(py, &mref)?;
            let obj = func.bind(py).call1((
                spec.spatial_dimension,
                spec.option.as_str(),
                model_obj,
                spec.id.as_str(),
            ))?;
            let mat = obj.cast::<super::solid::PyMaterial>().map_err(|_| {
                PyTypeError::new_err(format!(
                    "the allocator of material '{name}' must return a Material object"
                ))
            })?;
            let link = super::solid::link_of(mat).ok_or_else(|| {
                PyTypeError::new_err("the allocator returned a Material without a law link")
            })?;
            // a `MaterialCohesive` subclass is a runtime cohesive law
            let cohesive = if obj.is_instance_of::<super::cohesive::PyMaterialCohesive>() {
                Some(cohesive_overrides(&obj)?)
            } else {
                None
            };
            let overrides = law_overrides(&obj)?;
            let elastic_dim = elastic_base_dim(&obj);
            Ok((
                Arc::new(obj.unbind()),
                link,
                overrides,
                cohesive,
                elastic_dim,
            ))
        })?;
        let pending = link.take_pending()?;
        if let Some(cohesive_overrides) = cohesive {
            let mut mc = akantu::cohesive::MaterialCohesive::new_foreign(
                spec,
                &name,
                services.mesh(),
                Box::new(PyCohesiveLawMethods {
                    obj: Arc::clone(&obj),
                    link: Arc::clone(&link),
                    overrides: cohesive_overrides,
                }),
            )?;
            pending.apply_cohesive(&mut mc)?;
            link.install(spec.id.clone(), &h, part);
            h.register_law_object(part, spec.id.clone(), obj);
            return Ok(Box::new(mc) as Box<dyn Material>);
        }
        if let Some(dim) = elastic_dim {
            // a `MaterialLinearElastic<d>D` subclass: the elastic law with
            // the Python overrides (D-078)
            if dim != spec.spatial_dimension {
                return Err(Error::invalid(format!(
                    "the allocator of material '{name}' returned a MaterialLinearElastic{dim}D \
                     subclass for a {}-D model",
                    spec.spatial_dimension
                )));
            }
            let base = akantu::solid::materials::elastic::new_elastic_base(spec, services)?;
            let mut dm = DerivedMaterial::new(
                base,
                &name,
                Box::new(PyDerivedMethods {
                    obj: Arc::clone(&obj),
                    link: Arc::clone(&link),
                    overrides,
                }),
            );
            pending.apply_derived(&mut dm)?;
            link.install(spec.id.clone(), &h, part);
            h.register_law_object(part, spec.id.clone(), obj);
            return Ok(Box::new(dm) as Box<dyn Material>);
        }
        let mut fm = ForeignMaterial::new(
            spec,
            &name,
            Box::new(PyLawMethods {
                obj: Arc::clone(&obj),
                link: Arc::clone(&link),
                overrides,
            }),
        )?;
        pending.apply(&mut fm)?;
        link.install(spec.id.clone(), &h, part);
        h.register_law_object(part, spec.id.clone(), obj);
        Ok(Box::new(fm) as Box<dyn Material>)
    })
}

/// Install every Python allocator into the factory of `smm` (replacing
/// earlier installations).
pub(crate) fn install_python_materials(
    smm: &mut SolidMechanicsModel,
    model: &Arc<ModelHandle>,
    part: Part,
) {
    let allocs: Vec<(String, Arc<Py<PyAny>>)> = lock(&PY_MATERIALS)
        .iter()
        .map(|(k, v)| (k.clone(), Arc::clone(v)))
        .collect();
    for (name, func) in allocs {
        let a = python_allocator(name.clone(), func, Arc::downgrade(model), part);
        smm.physics.factory.replace(&name, a);
    }
}

/// `MaterialFactory` (PY-079).
#[pyclass(frozen, name = "MaterialFactory", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMaterialFactory;

#[pymethods]
impl PyMaterialFactory {
    /// `MaterialFactory.getInstance()`.
    #[staticmethod]
    #[pyo3(name = "getInstance")]
    fn get_instance() -> Self {
        Self
    }

    /// `registerAllocator(id, func)`: `func(dim, option, model, id)` must
    /// return a new `Material` object; an id already registered (built-in
    /// or Python) raises, as upstream's factory.
    #[pyo3(name = "registerAllocator")]
    fn register_allocator(&self, py: Python<'_>, id: String, func: Py<PyAny>) -> PyResult<()> {
        let builtin = builtin_material_names().map_err(|e| super::runtime::to_py(py, e))?;
        let mut reg = lock(&PY_MATERIALS);
        if reg.contains_key(&id) || builtin.contains(&id) {
            return Err(super::errors::exception(
                py,
                format!("The id {id} is already registered in the akantu::Material factory"),
            ));
        }
        reg.insert(id, Arc::new(func));
        Ok(())
    }

    /// `getPossibleAllocators()`: every registered id, sorted.
    #[pyo3(name = "getPossibleAllocators")]
    fn get_possible_allocators(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let mut names = builtin_material_names().map_err(|e| super::runtime::to_py(py, e))?;
        names.extend(lock(&PY_MATERIALS).keys().cloned());
        names.sort();
        names.dedup();
        Ok(names)
    }
}

// ---- selectors ---------------------------------------------------------------------------

/// The behaviour of a selector object.
#[derive(Debug, Clone)]
pub(crate) enum SelectorKind {
    /// `ConstitutiveLawSelector()` (Python subclasses override `__call__`).
    Base,
    /// `DefaultMaterialSelector(material_index)`.
    LawIndex,
    /// `MeshDataMaterialSelectorString(name, model, first_index)`.
    MeshData(String),
    /// The cohesive selectors (evaluated with the cohesive model's state).
    Cohesive(super::cohesive::CohesiveKind),
}

/// The fallback of a selector.
#[derive(Debug)]
enum Fallback {
    Value(usize),
    Selector(Py<PyConstitutiveLawSelector>),
}

/// `ConstitutiveLawSelector` (PY-070) and its subclasses.
#[pyclass(
    subclass,
    frozen,
    name = "ConstitutiveLawSelector",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyConstitutiveLawSelector {
    pub(crate) kind: SelectorKind,
    fallback: Mutex<Fallback>,
}

impl PyConstitutiveLawSelector {
    pub(crate) fn of_kind(kind: SelectorKind) -> Self {
        Self {
            kind,
            fallback: Mutex::new(Fallback::Value(0)),
        }
    }
}

/// A Python selector called back by the model.
#[derive(Debug)]
struct SelectorTrampoline(Py<PyAny>);

impl ConstitutiveLawSelector for SelectorTrampoline {
    fn select(&self, _ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let i: i64 = callback(|py| self.0.bind(py).call1((PyElement { element },))?.extract())?;
        usize::try_from(i)
            .map(Some)
            .map_err(|_| Error::InvalidIndex {
                what: "law index from a Python selector",
                value: i,
            })
    }
}

/// The selector chain of a Python selector object (the object, then its
/// fallbacks; a Python `__call__` override decides alone, its own
/// `super().__call__` reaching the fallbacks in Python).
pub(crate) fn selector_chain(
    obj: &Bound<'_, PyConstitutiveLawSelector>,
) -> PyResult<SelectorChain> {
    let py = obj.py();
    let base = py.get_type::<PyConstitutiveLawSelector>();
    let overridden = !obj
        .get_type()
        .getattr("__call__")?
        .is(&base.getattr("__call__")?);
    let mut chain = SelectorChain {
        selectors: Vec::new(),
        fallback_value: 0,
    };
    if overridden {
        chain.selectors.push(Box::new(SelectorTrampoline(
            obj.clone().into_any().unbind(),
        )));
        return Ok(chain);
    }
    match &obj.get().kind {
        SelectorKind::Base => {}
        SelectorKind::LawIndex => chain.selectors.push(Box::new(LawIndexSelector)),
        SelectorKind::MeshData(name) => chain
            .selectors
            .push(Box::new(ElementDataSelector::new(name.clone()))),
        SelectorKind::Cohesive(_) => {
            return Err(super::errors::exception(
                py,
                "a cohesive selector can only be set on a SolidMechanicsModelCohesive".into(),
            ));
        }
    }
    match &*lock(&obj.get().fallback) {
        Fallback::Value(v) => chain.fallback_value = *v,
        Fallback::Selector(s) => {
            let sub = selector_chain(s.bind(py))?;
            chain.selectors.extend(sub.selectors);
            chain.fallback_value = sub.fallback_value;
        }
    }
    Ok(chain)
}

#[pymethods]
impl PyConstitutiveLawSelector {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Self::of_kind(SelectorKind::Base)
    }

    /// `super().__init__()` from a Python subclass.
    #[pyo3(name = "__init__", signature = (*_args, **_kwargs))]
    fn init(&self, _args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) {}

    /// `setFallback(index)` or `setFallback(selector)`.
    #[pyo3(name = "setFallback")]
    fn set_fallback(&self, py: Python<'_>, fallback: &Bound<'_, PyAny>) -> PyResult<()> {
        let f =
            if let Ok(s) = fallback.cast::<PyConstitutiveLawSelector>() {
                Fallback::Selector(s.clone().unbind())
            } else {
                let v: i64 = fallback.extract()?;
                Fallback::Value(usize::try_from(v).map_err(|_| {
                    super::errors::exception(py, format!("invalid fallback index {v}"))
                })?)
            };
        *lock(&self.fallback) = f;
        Ok(())
    }

    /// `self(element)`: the base selector returns its fallback (value or
    /// the fallback selector's choice); the model's built-in selectors are
    /// evaluated by the model only.
    fn __call__(slf: &Bound<'_, Self>, element: &Bound<'_, PyElement>) -> PyResult<i64> {
        let py = slf.py();
        if !matches!(slf.get().kind, SelectorKind::Base) {
            return Err(super::errors::exception(
                py,
                "this selector reads the model's data: it is evaluated by the model \
                 (setMaterialSelector), not from Python"
                    .into(),
            ));
        }
        match &*lock(&slf.get().fallback) {
            Fallback::Value(v) => Ok(i64::try_from(*v).unwrap_or(i64::MAX)),
            Fallback::Selector(s) => s.bind(py).call1((element,))?.extract(),
        }
    }
}

/// `DefaultMaterialSelector(material_index)`.
#[pyclass(extends = PyConstitutiveLawSelector, subclass, frozen, name = "DefaultMaterialSelector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDefaultMaterialSelector;

#[pymethods]
impl PyDefaultMaterialSelector {
    /// `DefaultMaterialSelector(material_index)`: the index stored in the
    /// model's `getMaterialByElement()` map (the map of the model the
    /// selector is given to).
    #[new]
    fn new(material_index: &Bound<'_, EtmaInt>) -> PyClassInitializer<Self> {
        let _ = material_index;
        PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::LawIndex))
            .add_subclass(Self)
    }
}

/// `MeshDataMaterialSelectorString(name, model, first_index=1)`.
#[pyclass(extends = PyConstitutiveLawSelector, subclass, frozen, name = "MeshDataMaterialSelectorString", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMeshDataMaterialSelectorString;

#[pymethods]
impl PyMeshDataMaterialSelectorString {
    #[new]
    #[pyo3(signature = (name, model, first_index = 1))]
    fn new(name: String, model: &Bound<'_, PyAny>, first_index: i64) -> PyClassInitializer<Self> {
        // upstream stores `first_index` but never uses it (string data name
        // the law, see `ElementDataSelector`)
        let _ = (model, first_index);
        PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::MeshData(
            name,
        )))
        .add_subclass(Self)
    }
}

/// Register the classes of `register_constitutive_law_selector` and
/// `register_constitutive_law_internal_handler`.
pub(crate) fn register_selectors(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyConstitutiveLawSelector>()?;
    m.add_class::<PyDefaultMaterialSelector>()?;
    m.add_class::<PyMeshDataMaterialSelectorString>()?;
    Ok(())
}

/// `register_constitutive_law_internal_handler`.
pub(crate) fn register_internal_handler(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyInternalFieldReal>()?;
    m.add_class::<PyInternalFieldInt>()?;
    m.add_class::<PyConstitutiveLawInternalHandler>()?;
    Ok(())
}

/// `register_constitutive_law<SolidMechanicsModel>` and `MaterialFactory`.
pub(crate) fn register_material_base(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_class_as::<PyConstitutiveLawSolid>(m, "ConstitutiveLawakantu::SolidMechanicsModel")?;
    m.add_class::<PyMaterialFactory>()?;
    Ok(())
}

/// Register class `T` under upstream's name `name` (pybind11 class names of
/// templates contain `::`, which PyO3 class names cannot: the class is
/// created under its Rust-side name and renamed). The class must NOT also
/// be registered with `m.add_class::<T>()`: PyO3 appends that name to the
/// module's `__all__`, which this function cannot undo
/// (the module's `__all__` is deleted at the end of its initialisation in any
/// case, like pybind11's module has none).
pub(crate) fn add_class_as<T: pyo3::PyClass>(m: &Bound<'_, PyModule>, name: &str) -> PyResult<()> {
    let py = m.py();
    let ty = py.get_type::<T>();
    let internal: String = ty.getattr("__name__")?.extract()?;
    ty.setattr("__name__", name)?;
    ty.setattr("__qualname__", name)?;
    m.setattr(name, &ty)?;
    if m.hasattr(internal.as_str())? {
        m.delattr(internal.as_str())?;
    }
    Ok(())
}
