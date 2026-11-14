// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_phasefield.cc (`PyPhaseField`
// trampoline of `PhaseField(model, id)`: PYBIND11_OVERRIDE of
// initPhaseField, computeDrivingForce, computeDissipatedEnergy;
// `PhaseFieldFactory.registerAllocator(id, func)` with
// `func(dim, energy_split, model, id)`), python/py_constitutive_law.cc
// (`registerInternalReal`, `getInternalReal` of the subclass).

//! Python subclasses of `PhaseField` (D-078, PY-092).
//!
//! A Python `PhaseField` object constructed from Python
//! (`super().__init__(model, id)` inside a `PhaseFieldFactory` allocator)
//! holds a [`PfLink`]: the parameters and internals its `__init__`
//! registers wait as a *pending* law until the model's factory calls the
//! allocator and turns the object into an
//! [`akantu::phase_field::ForeignPhaseField`] whose behaviour is the Python
//! object ([`PyPfMethods`]). While the model calls an overridden method,
//! the law's state is lent to the call (the lending layer of
//! `crate::scoped`, admitted only from the callback's execution context,
//! D-086): `getDamage(type)`, `getStrain(type)`, `getInternalReal(name)(type)`
//! and the parameters work on the lent state (internals are zero-copy views
//! of the law's own storage); outside callbacks the object is a law of the
//! model (`OtherLaw`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use akantu::core::{ElementType, Error, GhostType, Real, Result};
use akantu::model::constitutive_law::{LawContext, LawSpec, ModelServices};
use akantu::model::params::DynamicParameters;
use akantu::parser::param::{ParamKind, ParamValue, ParameterAccess, ParameterRegistry};
use akantu::phase_field::{ForeignPhaseField, ForeignPhaseFieldMethods, ForeignPhaseFieldState};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;

use super::enums::{element_type_obj, ghost_type_obj};
use super::exports::{ExportTable, into_numpy};
use super::handle::{Handler, ModelHandle, ModelRef, Part};
use super::laws::OtherLaw;
use super::parser::{Prepared, RegistryAccess};
use super::runtime::{IntoPy, LendContext, callback, lending_callback, native};
use crate::foreign_view::numpy_export::Layout;
use crate::scoped::{ContextKey, ScopedMut, with_scoped_mut};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // only short, non-panicking critical sections
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a Python `__init__` registered before the law exists.
#[derive(Debug, Default)]
struct PendingPf {
    params: DynamicParameters,
    internals: Vec<(String, usize, bool)>,
}

#[derive(Debug, Default)]
struct PfLinkState {
    model: Option<(Weak<ModelHandle>, Part)>,
    id: Option<String>,
    index: Option<usize>,
    pending: Option<PendingPf>,
    installed: bool,
}

/// The link between a Python `PhaseField` object and its runtime law.
#[derive(Debug, Default)]
pub(crate) struct PfLink {
    state: Mutex<PfLinkState>,
    leases: Mutex<ExportTable<(String, ElementType, GhostType)>>,
    lent: Mutex<Vec<(ScopedMut<ForeignPhaseFieldState>, Arc<LendContext>)>>,
}

/// Pops the innermost lent state on drop.
struct LentGuard(Arc<PfLink>);

impl Drop for LentGuard {
    fn drop(&mut self) {
        lock(&self.0.lent).pop();
    }
}

fn not_initialised() -> Error {
    Error::invalid(
        "this PhaseField object is not attached to a phase-field law (PhaseField.__init__(model, \
         id) not called, or the object was not returned by a PhaseFieldFactory allocator)",
    )
}

impl PfLink {
    /// `PhaseField.__init__(model, id)`: a pending law of `model`.
    pub(crate) fn init_pending(&self, model: &ModelRef, id: &str) {
        let mut s = lock(&self.state);
        if s.installed {
            return;
        }
        s.model = Some((Arc::downgrade(&model.h), model.part));
        s.pending = Some(PendingPf::default());
        s.id = Some(id.to_owned());
    }

    fn model(&self) -> Result<ModelRef> {
        let s = lock(&self.state);
        let (w, part) = s.model.as_ref().ok_or_else(not_initialised)?;
        let h = w
            .upgrade()
            .ok_or_else(|| Error::invalid("the model of this PhaseField was destroyed"))?;
        Ok(ModelRef { h, part: *part })
    }

    /// The model law (installed objects; its index is resolved by law id
    /// once).
    pub(crate) fn other(&self) -> Result<OtherLaw> {
        let model = self.model()?;
        let (index, id) = {
            let s = lock(&self.state);
            if !s.installed {
                return Err(not_initialised());
            }
            (s.index, s.id.clone().ok_or_else(not_initialised)?)
        };
        let index = match index {
            Some(i) => i,
            None => {
                let part = model.part;
                let i = model.h.with(|m| {
                    let ops = m.ops(part)?;
                    for i in 0..ops.law_count(Handler::PhaseField)? {
                        let mut same = false;
                        ops.with_handler_law(Handler::PhaseField, i, &mut |l| {
                            same = l.law_id() == id;
                            Ok(())
                        })?;
                        if same {
                            return Ok(i);
                        }
                    }
                    Err(Error::not_found("phase field with id", id.clone()))
                })?;
                lock(&self.state).index = Some(i);
                i
            }
        };
        Ok(OtherLaw {
            model,
            handler: Handler::PhaseField,
            index,
        })
    }

    /// The state lent to the innermost running callback, admitted for the
    /// calling Python code.
    fn lent_top(
        &self,
        py: Python<'_>,
    ) -> Result<Option<(ScopedMut<ForeignPhaseFieldState>, ContextKey)>> {
        let top = lock(&self.lent).last().cloned();
        match top {
            None => Ok(None),
            Some((st, cx)) => Ok(Some((st, cx.admit(py)?))),
        }
    }

    fn push_lent(
        self: &Arc<Self>,
        st: ScopedMut<ForeignPhaseFieldState>,
        cx: &Arc<LendContext>,
    ) -> LentGuard {
        lock(&self.lent).push((st, Arc::clone(cx)));
        LentGuard(Arc::clone(self))
    }

    fn take_pending(&self) -> Result<PendingPf> {
        let mut s = lock(&self.state);
        if s.installed {
            return Err(Error::invalid(
                "this PhaseField object is already the law of a model",
            ));
        }
        s.pending.take().ok_or_else(not_initialised)
    }

    fn install(&self, id: String, model: &Arc<ModelHandle>, part: Part) {
        let mut s = lock(&self.state);
        s.model = Some((Arc::downgrade(model), part));
        s.id = Some(id);
        s.index = None;
        s.installed = true;
    }

    /// `registerInternalReal/Int(name, nb_component)`: before the law exists
    /// (in `__init__`), or on the lent state inside a callback.
    pub(crate) fn register_internal(
        self: &Arc<Self>,
        py: Python<'_>,
        name: &str,
        nb_component: usize,
        int: bool,
    ) -> PyResult<()> {
        if let Some((st, key)) = self.lent_top(py).py_err(py)? {
            let n = name.to_owned();
            return native(py, move || {
                st.with_mut(key, |s| {
                    if int {
                        s.internals.register_int(&n, nb_component)
                    } else {
                        s.internals.register_real(&n, nb_component)
                    }
                })?
            });
        }
        let mut s = lock(&self.state);
        let p = s.pending.as_mut().ok_or_else(|| {
            super::errors::exception(
                py,
                "internals of a phase-field law can be registered in __init__ or inside its \
                 callbacks only"
                    .into(),
            )
        })?;
        if p.internals.iter().any(|(n, _, _)| n == name) {
            return Err(super::errors::exception(
                py,
                format!("internal {name} already registered"),
            ));
        }
        p.internals.push((name.to_owned(), nb_component, int));
        Ok(())
    }

    /// A view of the block `(ty, ghost)` of the real internal `name`: the
    /// lent state inside callbacks (zero-copy), the model's law otherwise.
    pub(crate) fn real_view<'py>(
        self: &Arc<Self>,
        py: Python<'py>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        if let Some((st, key)) = self.lent_top(py).py_err(py)? {
            let link = Arc::clone(self);
            let n = name.to_owned();
            let v = native(py, move || {
                st.with_mut(key, |s| {
                    let f = s.real_internal_mut(&n)?;
                    link.leases_mut().view(
                        (n.clone(), ty, ghost),
                        f.values_mut().shared_mut(ty, ghost)?,
                    )
                })?
            })?;
            return into_numpy(py, v, Layout::Rows);
        }
        let o = self.other().py_err(py)?;
        o.model.view::<f64>(
            py,
            super::handle::ModelKey::Internal {
                handler: Handler::PhaseField,
                law: o.index,
                name: name.to_owned(),
                ty,
                ghost,
            },
        )
    }

    fn leases_mut(&self) -> MutexGuard<'_, ExportTable<(String, ElementType, GhostType)>> {
        lock(&self.leases)
    }
}

/// The parameters behind a Python `PhaseField` object.
#[derive(Debug)]
pub(crate) struct PfRegistry(pub(crate) Arc<PfLink>);

/// The parameters registered by a pending law's `__init__`.
struct Pending<'a>(&'a mut DynamicParameters);

impl ParameterRegistry for Pending<'_> {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.0.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.0.set(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new()
    }
    fn param_info(&self, name: &str) -> Option<akantu::parser::param::ParamInfo> {
        self.0.param_info(name)
    }
}

impl RegistryAccess for PfRegistry {
    fn prepare(&self, py: Python<'_>) -> Result<Prepared> {
        // the lend is looked up again in `with_registry` (it needs no frame
        // check beyond this one: the key is kept in the prepared value)
        Ok(match self.0.lent_top(py)? {
            Some((st, key)) => Prepared::pf(st, key),
            None => Prepared::default(),
        })
    }

    fn with_registry(
        &self,
        prepared: &Prepared,
        f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        if let Some((st, key)) = prepared.pf_lent() {
            return st.with_mut(key, |s| f(s))?;
        }
        {
            let mut s = lock(&self.0.state);
            if let Some(p) = s.pending.as_mut() {
                return f(&mut Pending(&mut p.params));
            }
        }
        self.0.other()?.with(|l| f(l))
    }

    fn register_param_real(
        &self,
        prepared: &Prepared,
        name: &str,
        access: ParameterAccess,
        description: &str,
        default: Option<Real>,
    ) -> Result<()> {
        let reg = |p: &mut DynamicParameters| {
            p.register(
                name,
                ParamKind::Real,
                access,
                description,
                default.map(ParamValue::Real),
            )
        };
        if let Some((st, key)) = prepared.pf_lent() {
            return st.with_mut(key, |s| reg(&mut s.params))?;
        }
        let mut s = lock(&self.0.state);
        match s.pending.as_mut() {
            Some(p) => reg(&mut p.params),
            None => Err(Error::invalid(
                "parameters of a phase-field law can be registered in __init__ or inside its \
                 callbacks only",
            )),
        }
    }
}

// ---- the trampoline ------------------------------------------------------------------------

/// The methods a Python `PhaseField` subclass overrides (upstream
/// `PyPhaseField`, `py_phasefield.cc:30-52`).
const PF_METHODS: [&str; 3] = [
    "initPhaseField",
    "computeDrivingForce",
    "computeDissipatedEnergy",
];

fn pf_overrides(obj: &Bound<'_, PyAny>) -> PyResult<[bool; 3]> {
    let py = obj.py();
    let base = py.get_type::<super::physics::PyPhaseField>();
    let ty = obj.get_type();
    let mut out = [false; 3];
    for (k, name) in PF_METHODS.iter().enumerate() {
        let Ok(m) = ty.getattr(*name) else { continue };
        out[k] = match base.getattr(*name) {
            Ok(b) => !m.is(&b),
            Err(_) => true,
        };
    }
    Ok(out)
}

/// The trampoline of a Python `PhaseField` subclass.
#[derive(Debug)]
struct PyPfMethods {
    obj: Arc<Py<PyAny>>,
    link: Arc<PfLink>,
    overrides: [bool; 3],
}

impl PyPfMethods {
    fn lend<R>(
        &self,
        state: &mut ForeignPhaseFieldState,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> Result<R> {
        let link = &self.link;
        let obj = &self.obj;
        lending_callback(|py, cx| {
            with_scoped_mut(state, cx.key(), |st| {
                let _l = link.push_lent(st, cx);
                f(obj.bind(py))
            })
        })
    }
}

impl ForeignPhaseFieldMethods for PyPfMethods {
    fn init_phase_field(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ctx: &LawContext<'_>,
    ) -> Result<()> {
        if !self.overrides[0] {
            return state.base_init(ctx);
        }
        // upstream binds no `initPhaseField` to call the base from Python:
        // an override replaces the base initialisation (the internals keep
        // their sizes, the law is not marked initialised)
        self.lend(state, |o| o.call_method0("initPhaseField").map(drop))
    }

    fn compute_driving_force(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if !self.overrides[1] {
            return Err(Error::NotImplemented(
                "computeDrivingForce of the PhaseField base",
            ));
        }
        self.lend(state, |o| {
            let py = o.py();
            o.call_method1(
                "computeDrivingForce",
                (element_type_obj(py, ty)?, ghost_type_obj(py, ghost)?),
            )
            .map(drop)
        })
    }

    fn compute_dissipated_energy(
        &mut self,
        state: &mut ForeignPhaseFieldState,
        ty: ElementType,
    ) -> Result<()> {
        if !self.overrides[2] {
            return Err(Error::NotImplemented(
                "computeDissipatedEnergy of the PhaseField base",
            ));
        }
        self.lend(state, |o| {
            o.call_method1("computeDissipatedEnergy", (element_type_obj(o.py(), ty)?,))
                .map(drop)
        })
    }
}

// ---- the factory ---------------------------------------------------------------------------

/// Python allocators registered with `PhaseFieldFactory.registerAllocator`.
static PY_PHASEFIELDS: Mutex<BTreeMap<String, Arc<Py<PyAny>>>> = Mutex::new(BTreeMap::new());

/// The names of the built-in phase fields.
pub(crate) fn builtin_phasefield_names() -> Result<Vec<String>> {
    let mut f = akantu::phase_field::PhaseFieldFactory::default();
    akantu::phase_field::laws::register_builtin_phasefields(&mut f)?;
    Ok(f.names().map(str::to_owned).collect())
}

/// `registerAllocator(id, func)` (an id registered already raises, as
/// upstream's factory).
pub(crate) fn register_allocator(py: Python<'_>, id: String, func: Py<PyAny>) -> PyResult<()> {
    let builtin = builtin_phasefield_names().py_err(py)?;
    let mut reg = lock(&PY_PHASEFIELDS);
    if reg.contains_key(&id) || builtin.contains(&id) {
        return Err(super::errors::exception(
            py,
            format!("The id {id} is already registered in the akantu::PhaseField factory"),
        ));
    }
    reg.insert(id, Arc::new(func));
    Ok(())
}

/// The allocator of the Python phase field `name` for `model`.
fn python_allocator(
    name: String,
    func: Arc<Py<PyAny>>,
    model: Weak<ModelHandle>,
    part: Part,
) -> akantu::model::constitutive_law::Allocator<dyn akantu::phase_field::PhaseField> {
    Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
        let h = model
            .upgrade()
            .ok_or_else(|| Error::invalid("the model of this phase-field factory was destroyed"))?;
        let mref = ModelRef {
            h: Arc::clone(&h),
            part,
        };
        let (obj, link, overrides) = callback(|py| {
            let model_obj = super::handle::model_object(py, &mref)?;
            let obj = func.bind(py).call1((
                spec.spatial_dimension,
                spec.option.as_str(),
                model_obj,
                spec.id.as_str(),
            ))?;
            let pf = obj.cast::<super::physics::PyPhaseField>().map_err(|_| {
                PyTypeError::new_err(format!(
                    "the allocator of phase field '{name}' must return a PhaseField object"
                ))
            })?;
            let link = super::physics::pf_link_of(pf).ok_or_else(|| {
                PyTypeError::new_err("the allocator returned a PhaseField of a model law")
            })?;
            let overrides = pf_overrides(&obj)?;
            Ok((Arc::new(obj.unbind()), link, overrides))
        })?;
        let pending = link.take_pending()?;
        let mut law = ForeignPhaseField::new(
            spec,
            &name,
            services.mesh(),
            Box::new(PyPfMethods {
                obj: Arc::clone(&obj),
                link: Arc::clone(&link),
                overrides,
            }),
        )?;
        for p in pending.params.iter() {
            if law.param_info(&p.name).is_none() {
                if p.kind != ParamKind::Real {
                    return Err(Error::Unsupported(format!(
                        "runtime parameter '{}' of kind {:?}",
                        p.name, p.kind
                    )));
                }
                law.register_param_real(&p.name, p.access, &p.description, None)?;
            }
            if let Ok(v) = pending.params.get(&p.name) {
                law.set_param(&p.name, v)?;
            }
        }
        for (n, nbc, int) in pending.internals {
            if int {
                law.state.internals.register_int(&n, nbc)?;
            } else {
                law.state.internals.register_real(&n, nbc)?;
            }
        }
        link.install(spec.id.clone(), &h, part);
        h.register_law_object(part, spec.id.clone(), obj);
        Ok(Box::new(law) as Box<dyn akantu::phase_field::PhaseField>)
    })
}

/// Install every Python allocator into the factory of `pfm` (replacing
/// earlier installations).
pub(crate) fn install_python_phasefields(
    pfm: &mut akantu::phase_field::PhaseFieldModel,
    model: &Arc<ModelHandle>,
    part: Part,
) {
    let allocs: Vec<(String, Arc<Py<PyAny>>)> = lock(&PY_PHASEFIELDS)
        .iter()
        .map(|(k, v)| (k.clone(), Arc::clone(v)))
        .collect();
    for (name, func) in allocs {
        let a = python_allocator(name.clone(), func, Arc::downgrade(model), part);
        pfm.physics.factory.replace(&name, a);
    }
}
