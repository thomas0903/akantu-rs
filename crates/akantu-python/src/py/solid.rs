// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_solid_mechanics_model.cc
// (SolidMechanicsModel), python/py_material.cc (Material),
// python/py_constitutive_law.cc (setReal…, getElementFilter,
// getInternalReal), python/py_constitutive_laws_handler.hh
// (getConstitutiveLaw…, registerNewConstitutiveLaw).

//! Solid mechanics bindings (PY-075, PY-076).
//!
//! A Python `SolidMechanicsModel` is a [`ModelRef`] whose handle owns the
//! Rust model (also the solid part of a cohesive model or of a coupler).
//! Nodal fields (`getDisplacement`, `getBlockedDOFs`, …),
//! `getMaterialByElement` and material internals (`getStress`, `getGradU`,
//! `getInternalReal`, …) are **zero-copy views of the model's own storage**
//! (D-047, [`super::exports`]); element filters are read-only snapshot
//! copies (UD-747). Python boundary-condition functors are called back
//! through the runtime trampoline with no native guard alive.

use std::sync::{Arc, PoisonError};

use akantu::core::enums::AnalysisMethod as RsAnalysisMethod;
use akantu::core::{ElementKind, ElementType, Error, GhostType, Real, TypeFilter};
use akantu::model::ModelOptions;
use akantu::model::constitutive_law::{InternalFieldRef, LawPhase};
use akantu::parser::param::ParamValue;
use akantu::solid::SolidMechanicsModel;
use akantu::solid::material::Material;
use pyo3::exceptions::{PyDeprecationWarning, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyFloat, PyInt, PyString, PyTuple};

use super::arrays::owned_numpy;
use super::common::PyElement;
use super::enums::{EtArg, GhostArg, KindArg, ModelType};
use super::etma::{EtmaInt, EtmaProvider, EtmaReal};
use super::handle::{AnyModel, Handler, ModelHandle, ModelKey, ModelRef, Part};
use super::laws::{
    LawLink, PyConstitutiveLawInternalHandler, PyConstitutiveLawSelector, PyConstitutiveLawSolid,
    PyDefaultMaterialSelector, SelectorKind, class_overrides, install_python_materials,
    selector_chain,
};
use super::mesh::{PY_ALL_DIMENSIONS, PyMesh};
use super::model::{AnalysisArg, PyModel, PyModelOptions, PyModelSolver};
use super::runtime::native;
use crate::foreign_view::numpy_export::Layout;

// --- ElementTypeMapArray providers of the model ----------------------------------------

#[derive(Debug)]
struct LawByElementProvider(ModelRef);

impl EtmaProvider<i32> for LawByElementProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        self.0
            .with(py, |m| Ok(m.material_by_element().id().to_owned()))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.0.with(py, move |m| {
            Ok(m.material_by_element().element_types(filter).collect())
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.0
            .view::<i32>(py, ModelKey::LawByElement(Handler::Material, ty, ghost))
    }
}

// --- Material ------------------------------------------------------------------------------

/// `Material` (PY-076): a law of a model, or a Python subclass instance (a
/// runtime law once a factory allocator returned it, see
/// [`super::laws`]).
#[pyclass(extends = PyConstitutiveLawSolid, subclass, frozen, name = "Material", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMaterial;

/// The law link of a `Material` object.
pub(crate) fn link_of(mat: &Bound<'_, PyMaterial>) -> Option<Arc<LawLink>> {
    mat.as_super().as_super().get().link().cloned()
}

fn link(slf: &Bound<'_, PyMaterial>) -> PyResult<Arc<LawLink>> {
    link_of(slf).ok_or_else(|| PyTypeError::new_err("not a law object"))
}

/// The initializer of a `Material` object with link `l`.
pub(crate) fn material_init(l: Arc<LawLink>) -> PyClassInitializer<PyMaterial> {
    PyConstitutiveLawInternalHandler::init(l)
        .add_subclass(PyConstitutiveLawSolid)
        .add_subclass(PyMaterial)
}

/// The Python object of law `index` of `model`: the Python object of a
/// runtime law, else a wrapper of the most derived bound class
/// (`MaterialLinearElastic<dim>D` for built-in elastic laws, as pybind11's
/// polymorphic return).
pub(crate) fn material_object<'py>(
    py: Python<'py>,
    model: &ModelRef,
    index: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let m = model.clone();
    let (id, ty, dim) = model.with(py, move |s| {
        let law = s.material(index)?;
        Ok((
            law.law_id().to_owned(),
            law.law_type().to_owned(),
            law.base().spatial_dimension,
        ))
    })?;
    if let Some(obj) = m.h.law_object(m.part, &id) {
        return Ok(obj.bind(py).clone());
    }
    let init = material_init(LawLink::existing(m, index));
    if ty.starts_with("cohesive") {
        if let Some(o) = super::cohesive::cohesive_material_object(py, init, &ty, dim)? {
            return Ok(o);
        }
        return Err(PyTypeError::new_err("internal error: cohesive law object"));
    }
    Ok(match (ty.as_str(), dim) {
        ("elastic", 1) => Bound::new(py, init.add_subclass(PyMaterialLinearElastic1D))?.into_any(),
        ("elastic", 2) => Bound::new(py, init.add_subclass(PyMaterialLinearElastic2D))?.into_any(),
        ("elastic", 3) => Bound::new(py, init.add_subclass(PyMaterialLinearElastic3D))?.into_any(),
        _ => Bound::new(py, init)?.into_any(),
    })
}

/// The Python model object of a handle part (the class of its model).
pub(crate) fn model_object<'py>(py: Python<'py>, model: &ModelRef) -> PyResult<Bound<'py, PyAny>> {
    super::handle::model_object(py, model)
}

/// pybind11's rule for a bound method of a class with a trampoline
/// (`get_override`): when the calling Python frame runs the method `name`
/// with `slf` as its first argument — a Python override calling
/// `super().name(...)` — the C++ base runs, not the override again.
/// The frame comes from the C API (`scoped::python`); its code
/// object and locals are read with attribute access (no user code runs).
fn is_super_call(slf: &Bound<'_, PyAny>, name: &str) -> PyResult<bool> {
    let py = slf.py();
    let Some(frame) = crate::scoped::python::current_frame(py) else {
        return Ok(false);
    };
    let code = frame.getattr("f_code")?;
    if code.getattr("co_name")?.extract::<String>()? != name
        || code.getattr("co_argcount")?.extract::<usize>()? == 0
    {
        return Ok(false);
    }
    let first = code.getattr("co_varnames")?.get_item(0)?;
    let locals = frame.getattr("f_locals")?;
    Ok(locals.get_item(first).is_ok_and(|v| v.is(slf)))
}

/// pybind11's `get_override` for a call that is **not** a super-call:
/// the class's Python override answers when the class
/// defines `name`, inside and outside the law's callbacks (e.g.
/// `super().getPushWaveSpeed(el)` or `Material.getPushWaveSpeed(self, el)`
/// inside a Python `computeStress` reaches the trampoline, whose frame test
/// fails, so the override runs; so does a top-level
/// `mat.hasStiffnessMatrixChanged()`). `Ok(None)`: the class does not
/// define `name` (the base answers). The D-086 admission is checked first:
/// while a callback of the law runs and the caller is not inside it, the
/// call is refused before any Python code runs. Inside a callback the
/// override runs under the same admitted lend; its own `super().name(...)`
/// is a super-call and reaches the base.
fn python_override<'py>(
    slf: &Bound<'py, PyMaterial>,
    l: &Arc<LawLink>,
    name: &str,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    l.has_lent(slf.py())?;
    if !class_overrides(slf.as_any(), name) {
        return Ok(None);
    }
    slf.as_any().getattr(name).map(Some)
}

/// The base class's answer of a const method of law `l` (a super-call):
/// on the state lent to a running callback, or on the model's law.
fn base_answer<R: Send>(
    py: Python<'_>,
    l: &Arc<LawLink>,
    lent: impl FnOnce(&dyn akantu::solid::RuntimeLawState) -> akantu::core::Result<R> + Send,
    law: impl FnOnce(&dyn Material) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    if let Some(v) = l.with_lent_state(py, lent)? {
        return Ok(v);
    }
    native(py, || l.with_law(|smm, i| law(smm.material(i)?)))
}

/// The base answer of a model law: a law derived from a built-in law
/// answers the built-in law's (`derived`), a plain runtime law the
/// `Material` base's (`foreign`), a built-in law its own (`other`).
fn base_law_answer<R>(
    m: &dyn Material,
    derived: impl FnOnce(&dyn Material) -> akantu::core::Result<R>,
    foreign: impl FnOnce() -> akantu::core::Result<R>,
    other: impl FnOnce() -> akantu::core::Result<R>,
) -> akantu::core::Result<R> {
    let any = m.as_any();
    if let Some(d) = any.and_then(|a| a.downcast_ref::<akantu::solid::DerivedMaterial>()) {
        return derived(d.state.law.as_ref());
    }
    if any.is_some_and(|a| a.is::<akantu::solid::ForeignMaterial>()) {
        return foreign();
    }
    other()
}

/// The `Material` base's wave speed (`AKANTU_TO_IMPLEMENT` upstream).
fn no_base_wave_speed() -> akantu::core::Result<Real> {
    Err(Error::NotImplemented(
        "the wave speed of the Material base (upstream AKANTU_TO_IMPLEMENT)",
    ))
}

#[pymethods]
impl PyMaterial {
    /// `Material(model, id)` (Python subclasses: `super().__init__(model,
    /// id)`); the object becomes a law when a factory allocator returns it.
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyClassInitializer<Self> {
        material_init(LawLink::uninit())
    }

    /// `super().__init__(model, id)`.
    #[pyo3(name = "__init__", signature = (model, id, *_args, **_kwargs))]
    fn init(
        slf: &Bound<'_, Self>,
        model: &Bound<'_, PyAny>,
        id: String,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let m = model
            .cast::<PyModelSolver>()
            .map_err(|_| PyTypeError::new_err("Material(model, id): model must be a model object"))?
            .get()
            .model
            .clone();
        let l = link(slf)?;
        native(slf.py(), move || l.init_pending(&m, &id))
    }

    /// `getGradU(el_type, ghost_type=_not_ghost)`: `(nb_quad, dim²)` view.
    #[pyo3(name = "getGradU", signature = (el_type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_grad_u<'py>(
        slf: &Bound<'py, Self>,
        el_type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        link(slf)?.real_view(slf.py(), "grad_u", el_type.0, ghost_type.single()?)
    }

    /// `getStress(el_type, ghost_type=_not_ghost)`: `(nb_quad, dim²)` view.
    #[pyo3(name = "getStress", signature = (el_type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_stress<'py>(
        slf: &Bound<'py, Self>,
        el_type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        link(slf)?.real_view(slf.py(), "stress", el_type.0, ghost_type.single()?)
    }

    /// `getPotentialEnergy(el_type)` (view of the energy density),
    /// `getPotentialEnergy(element)` or `getPotentialEnergy()` (integrals).
    #[pyo3(name = "getPotentialEnergy", signature = (arg = None))]
    fn get_potential_energy<'py>(
        slf: &Bound<'py, Self>,
        arg: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let l = link(slf)?;
        match arg {
            None => {
                let e = native(py, || {
                    l.with_law(|s, i| {
                        s.with_material(i, LawPhase::Energy, |law, ctx| law.potential_energy(ctx))
                    })
                })?;
                Ok(PyFloat::new(py, e).into_any())
            }
            Some(a) => {
                if let Ok(el) = a.cast::<PyElement>() {
                    let el = el.get().element;
                    let e = native(py, || {
                        l.with_law(|s, i| {
                            s.with_material(i, LawPhase::Energy, move |law, ctx| {
                                law.potential_energy_of_element(ctx, el)
                            })
                        })
                    })?;
                    Ok(PyFloat::new(py, e).into_any())
                } else {
                    let t: EtArg = a.extract()?;
                    l.real_view(py, "potential_energy", t.0, GhostType::NotGhost)
                }
            }
        }
    }

    /// `initMaterial()` (pybind11's rule: `super().initMaterial()` inside a
    /// Python `initMaterial` is the base initialisation of the law; another
    /// call runs the class's override, if any).
    #[pyo3(name = "initMaterial")]
    fn init_material(slf: &Bound<'_, Self>) -> PyResult<()> {
        let l = link(slf)?;
        let super_call = is_super_call(slf.as_any(), "initMaterial")?;
        if !super_call && let Some(f) = python_override(slf, &l, "initMaterial")? {
            return f.call0().map(drop);
        }
        l.init_material(slf.py(), super_call)
    }

    /// `getName()` (akantu-rs helper; upstream reads the `name` parameter).
    #[pyo3(name = "getName")]
    fn get_name(slf: &Bound<'_, Self>) -> PyResult<String> {
        let l = link(slf)?;
        native(slf.py(), || {
            l.with_law(|s, i| Ok(s.material(i)?.base().name.clone()))
        })
    }

    /// `getID()` (akantu-rs helper).
    #[pyo3(name = "getID")]
    fn get_id(slf: &Bound<'_, Self>) -> PyResult<String> {
        let l = link(slf)?;
        native(slf.py(), || {
            l.with_law(|s, i| Ok(s.material(i)?.base().id.clone()))
        })
    }

    /// `getModel()`: a `SolidMechanicsModel` object sharing this model.
    #[pyo3(name = "getModel")]
    fn get_model<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let m = link(slf)?
            .model()
            .map_err(|e| super::runtime::to_py(py, e))?;
        model_object(py, &m)
    }

    /// `getPushWaveSpeed(element)` with pybind11's `get_override` rule: a
    /// super-call (a Python `getPushWaveSpeed` calling
    /// `super().getPushWaveSpeed(...)`) is the base class's answer;
    /// any other call is the class's override if it defines one, inside and
    /// outside the law's callbacks, else the base's.
    #[pyo3(name = "getPushWaveSpeed")]
    fn get_push_wave_speed(
        slf: &Bound<'_, Self>,
        element: &Bound<'_, PyElement>,
    ) -> PyResult<Real> {
        let (l, el) = (link(slf)?, element.get().element);
        if is_super_call(slf.as_any(), "getPushWaveSpeed")? {
            return base_answer(
                slf.py(),
                &l,
                move |s| s.base_push_wave_speed(el),
                move |m| {
                    base_law_answer(
                        m,
                        |d| d.push_wave_speed(el),
                        no_base_wave_speed,
                        || m.push_wave_speed(el),
                    )
                },
            );
        }
        if let Some(f) = python_override(slf, &l, "getPushWaveSpeed")? {
            return f.call1((element,))?.extract();
        }
        if let Some(v) = l.with_lent_state(slf.py(), move |s| s.base_push_wave_speed(el))? {
            return Ok(v);
        }
        native(slf.py(), || {
            l.with_law(|s, i| s.material(i)?.push_wave_speed(el))
        })
    }

    /// `getShearWaveSpeed(element)` (see `getPushWaveSpeed`).
    #[pyo3(name = "getShearWaveSpeed")]
    fn get_shear_wave_speed(
        slf: &Bound<'_, Self>,
        element: &Bound<'_, PyElement>,
    ) -> PyResult<Real> {
        let (l, el) = (link(slf)?, element.get().element);
        if is_super_call(slf.as_any(), "getShearWaveSpeed")? {
            return base_answer(
                slf.py(),
                &l,
                move |s| s.base_shear_wave_speed(el),
                move |m| {
                    base_law_answer(
                        m,
                        |d| d.shear_wave_speed(el),
                        no_base_wave_speed,
                        || m.shear_wave_speed(el),
                    )
                },
            );
        }
        if let Some(f) = python_override(slf, &l, "getShearWaveSpeed")? {
            return f.call1((element,))?.extract();
        }
        if let Some(v) = l.with_lent_state(slf.py(), move |s| s.base_shear_wave_speed(el))? {
            return Ok(v);
        }
        native(slf.py(), || {
            l.with_law(|s, i| s.material(i)?.shear_wave_speed(el))
        })
    }

    /// `hasStiffnessMatrixChanged()` (pybind11's rule, see
    /// `getPushWaveSpeed`).
    #[pyo3(name = "hasStiffnessMatrixChanged")]
    fn has_stiffness_matrix_changed(slf: &Bound<'_, Self>) -> PyResult<bool> {
        let l = link(slf)?;
        if is_super_call(slf.as_any(), "hasStiffnessMatrixChanged")? {
            return base_answer(
                slf.py(),
                &l,
                |s| Ok(s.base_has_stiffness_matrix_changed()),
                |m| {
                    base_law_answer(
                        m,
                        |d| Ok(d.has_stiffness_matrix_changed()),
                        || Ok(true),
                        || Ok(m.has_stiffness_matrix_changed()),
                    )
                },
            );
        }
        if let Some(f) = python_override(slf, &l, "hasStiffnessMatrixChanged")? {
            return f.call0()?.extract();
        }
        if let Some(v) =
            l.with_lent_state(slf.py(), |s| Ok(s.base_has_stiffness_matrix_changed()))?
        {
            return Ok(v);
        }
        native(slf.py(), || {
            l.with_law(|s, i| Ok(s.material(i)?.has_stiffness_matrix_changed()))
        })
    }

    /// `hasMatrixChanged(id)` as upstream's `Material::hasMatrixChanged`
    /// (`material.hh:479-490`): `"K"` →
    /// `hasStiffnessMatrixChanged() or finite_deformation` (the first through
    /// the trampoline rule), `"M"` → the law's `hasMassMatrixChanged()` (not
    /// overridable from Python: `false` but for the phase-field law with
    /// `degrade_mass`), any other id → `True`.
    #[pyo3(name = "hasMatrixChanged")]
    fn has_matrix_changed(slf: &Bound<'_, Self>, id: &str) -> PyResult<bool> {
        let py = slf.py();
        match id {
            "K" => {
                if Self::has_stiffness_matrix_changed(slf)? {
                    return Ok(true);
                }
                let l = link(slf)?;
                if let Some(v) = l.with_lent_state(py, |s| Ok(s.law_base().finite_deformation))? {
                    return Ok(v);
                }
                native(py, || {
                    l.with_law(|s, i| Ok(s.material(i)?.base().finite_deformation))
                })
            }
            "M" => {
                let l = link(slf)?;
                if let Some(v) = l.with_lent_state(py, |s| Ok(s.base_has_mass_matrix_changed()))? {
                    return Ok(v);
                }
                native(py, || {
                    l.with_law(|s, i| Ok(s.material(i)?.has_mass_matrix_changed()))
                })
            }
            _ => Ok(true),
        }
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let l = link(slf)?;
        let r = native(slf.py(), || {
            l.with_law(|s, i| {
                let b = s.material(i)?.base();
                Ok((b.name.clone(), b.law_type.clone()))
            })
        });
        Ok(match r {
            Ok((name, ty)) => format!("<Material '{name}' ({ty})>"),
            Err(_) => "<Material (not a model law yet)>".to_owned(),
        })
    }
}

macro_rules! elastic_class {
    ($rust:ident, $name:literal) => {
        #[doc = concat!("`", $name, "` (PY-077): upstream's `MaterialElastic<dim>`; built-in elastic laws of that dimension are returned as this class. A Python subclass returned by a factory allocator is the elastic law with the subclass's overrides (D-078): every method it does not define, and `super().initMaterial()` / `super().getPushWaveSpeed(...)` / `super().getShearWaveSpeed(...)` / `super().hasStiffnessMatrixChanged()` inside an override, are the elastic law's (`akantu::solid::DerivedMaterial`).")]
        #[pyclass(extends = PyMaterial, subclass, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;

        #[pymethods]
        impl $rust {
            #[new]
            #[pyo3(signature = (*_args, **_kwargs))]
            fn new(
                _args: &Bound<'_, PyTuple>,
                _kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyClassInitializer<Self> {
                material_init(LawLink::uninit()).add_subclass(Self)
            }
        }
    };
}

elastic_class!(PyMaterialLinearElastic1D, "MaterialLinearElastic1D");
elastic_class!(PyMaterialLinearElastic2D, "MaterialLinearElastic2D");
elastic_class!(PyMaterialLinearElastic3D, "MaterialLinearElastic3D");

// --- ConstitutiveLawsHandler<Material, Model> ----------------------------------------------

/// `ConstitutiveLawsHandler<Material, Model>` (PY-074; upstream class name
/// `ConstitutiveLawsHandlerakantu::Materialakantu::Model`): the law-handler
/// methods of `SolidMechanicsModel`.
#[pyclass(extends = PyModel, subclass, frozen, name = "ConstitutiveLawsHandler_Material_Model", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyClhMaterial;

fn clh_model(slf: &Bound<'_, PyClhMaterial>) -> PyResult<ModelRef> {
    super::model::model_of(slf)
}

/// Read-only snapshot copies of the law-local numbering.
#[derive(Debug)]
struct LocalNumberingProvider(ModelRef);

impl EtmaProvider<i32> for LocalNumberingProvider {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        self.0
            .with(py, |m| Ok(m.material_local_numbering().id().to_owned()))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.0.with(py, move |m| {
            Ok(m.material_local_numbering().element_types(filter).collect())
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, r, c) = self.0.with(py, move |m| {
            let a = m.material_local_numbering().get(ty, ghost)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        let a = owned_numpy(py, v, r, c, Layout::Rows)?;
        a.call_method1("setflags", (false,))?;
        Ok(a)
    }
}

fn nb_quad(fe: &akantu::fe::engine::FeEngine, ty: ElementType, ghost: GhostType) -> usize {
    fe.nb_integration_points(ty, ghost).unwrap_or(0)
}

#[pymethods]
impl PyClhMaterial {
    /// `getConstitutiveLaw(index | name | element)`.
    #[pyo3(name = "getConstitutiveLaw")]
    fn get_constitutive_law<'py>(
        slf: &Bound<'py, Self>,
        key: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let model = clh_model(slf)?;
        let i = material_index(slf.py(), &model, key)?;
        material_object(slf.py(), &model, i)
    }

    /// `getConstitutiveLaws()`: the laws as a list (UD-832: upstream's
    /// binding returns an unbound iterator type and raises `TypeError`).
    #[pyo3(name = "getConstitutiveLaws")]
    fn get_constitutive_laws<'py>(slf: &Bound<'py, Self>) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let model = clh_model(slf)?;
        let n = model.with(slf.py(), |m| Ok(m.nb_materials()))?;
        (0..n)
            .map(|i| material_object(slf.py(), &model, i))
            .collect()
    }

    #[pyo3(name = "getNbConstitutiveLaws")]
    fn get_nb_constitutive_laws(slf: &Bound<'_, Self>) -> PyResult<usize> {
        clh_model(slf)?.with(slf.py(), |m| Ok(m.nb_materials()))
    }

    #[pyo3(name = "getConstitutiveLawIndex")]
    fn get_constitutive_law_index(slf: &Bound<'_, Self>, name: String) -> PyResult<usize> {
        clh_model(slf)?.with(slf.py(), move |m| m.material_index(&name))
    }

    /// `getConstitutiveLawByElement()`: writable views.
    #[pyo3(name = "getConstitutiveLawByElement")]
    fn get_constitutive_law_by_element(slf: &Bound<'_, Self>) -> PyResult<EtmaInt> {
        Ok(EtmaInt::from_provider(Arc::new(LawByElementProvider(
            clh_model(slf)?,
        ))))
    }

    /// `getConstitutiveLawLocalNumbering()`: read-only snapshot copies
    /// (law-local numberings are not exportable storage, D-056 item 2).
    #[pyo3(name = "getConstitutiveLawLocalNumbering")]
    fn get_constitutive_law_local_numbering(slf: &Bound<'_, Self>) -> PyResult<EtmaInt> {
        Ok(EtmaInt::from_provider(Arc::new(LocalNumberingProvider(
            clh_model(slf)?,
        ))))
    }

    #[pyo3(name = "reassignConstitutiveLaw")]
    fn reassign_constitutive_law(slf: &Bound<'_, Self>) -> PyResult<()> {
        clh_model(slf)?.with(slf.py(), SolidMechanicsModel::reassign_material)
    }

    /// `registerNewConstitutiveLaw(name, type, opt_param="")` → the law.
    #[pyo3(name = "registerNewConstitutiveLaw", signature = (name, r#type, opt_param = String::new()))]
    fn register_new_constitutive_law<'py>(
        slf: &Bound<'py, Self>,
        name: String,
        r#type: String,
        opt_param: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let model = clh_model(slf)?;
        let i = register_law(slf.py(), &model, name, r#type, opt_param)?;
        material_object(slf.py(), &model, i)
    }

    /// `isInternal(field_name, element_kind=_ek_regular)`: some law has a
    /// real internal of that name.
    #[pyo3(name = "isInternal", signature = (field_name, element_kind = KindArg(Some(ElementKind::Regular))))]
    fn is_internal(
        slf: &Bound<'_, Self>,
        field_name: String,
        element_kind: KindArg,
    ) -> PyResult<bool> {
        let _ = element_kind;
        clh_model(slf)?.with(slf.py(), move |m| {
            let mut found = false;
            for i in 0..m.nb_materials() {
                m.material(i)?.for_each_internal(&mut |n, f| {
                    if n == field_name && matches!(f, InternalFieldRef::Real(_)) {
                        found = true;
                    }
                });
            }
            Ok(found)
        })
    }

    /// `flattenInternalReal(field_name, kind=_ek_regular,
    /// ghost_type=_not_ghost)`: the laws' internal scattered into
    /// mesh-indexed blocks (a new map; upstream: a map owned by the model).
    #[pyo3(name = "flattenInternalReal", signature = (field_name, kind = KindArg(Some(ElementKind::Regular)), ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn flatten_internal_real(
        slf: &Bound<'_, Self>,
        field_name: String,
        kind: KindArg,
        ghost_type: GhostArg,
    ) -> PyResult<EtmaReal> {
        let ghost = ghost_type.single()?;
        let kind = kind.0;
        let name = field_name.clone();
        let blocks = clh_model(slf)?.with(slf.py(), move |m| {
            let mesh_h = m.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let fe = m.physics.base.fems.get("")?;
            let mut out = akantu::core::ElementTypeMapArray::<Real>::new(&name);
            for i in 0..m.nb_materials() {
                let law = m.material(i)?;
                let filter = law.element_filter();
                let mut err = None;
                law.for_each_internal(&mut |n, f| {
                    if n != name || err.is_some() {
                        return;
                    }
                    if let InternalFieldRef::Real(field) = f
                        && let Err(e) = field.flatten(
                            filter,
                            ghost,
                            &|ty| mesh.nb_element(ty, ghost),
                            &|ty| nb_quad(fe, ty, ghost),
                            0.0,
                            &mut out,
                        )
                    {
                        err = Some(e);
                    }
                });
                if let Some(e) = err {
                    return Err(e);
                }
            }
            let mut blocks = Vec::new();
            for (ty, a) in out.as_map().iter(ghost) {
                if kind.is_some_and(|k| ty.kind() != k) {
                    continue;
                }
                blocks.push((ty, a.clone()));
            }
            Ok(blocks)
        })?;
        Ok(super::etma::owned_real(&field_name, ghost, blocks))
    }

    /// `inflateInternalReal(field_name, field, kind=_ek_regular,
    /// ghost_type=_not_ghost)`: gather mesh-indexed values back into the
    /// laws' internal.
    #[pyo3(name = "inflateInternalReal", signature = (field_name, field, kind = KindArg(Some(ElementKind::Regular)), ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn inflate_internal_real(
        slf: &Bound<'_, Self>,
        field_name: String,
        field: &Bound<'_, EtmaReal>,
        kind: KindArg,
        ghost_type: GhostArg,
    ) -> PyResult<()> {
        let py = slf.py();
        let ghost = ghost_type.single()?;
        let filter = TypeFilter {
            spatial_dimension: None,
            ghost_type: ghost,
            kind: kind.0,
        };
        // the field's blocks are copied before the native section
        let p = &field.get().provider;
        let mut flat = akantu::core::ElementTypeMapArray::<Real>::new(&field_name);
        for ty in p.types(py, filter)? {
            let a = p.call(py, ty, ghost)?;
            let (values, shape) = super::arrays::input_f64(&a)?;
            let nbc = shape.get(1).copied().unwrap_or(1).max(1);
            let arr = akantu::core::Array::from_vec(values, nbc)
                .map_err(|e| super::runtime::to_py(py, e))?;
            flat.as_map_mut()
                .insert(ty, ghost, arr)
                .map_err(|e| super::runtime::to_py(py, e))?;
        }
        clh_model(slf)?.with(py, move |m| {
            let nqs: std::collections::BTreeMap<ElementType, usize> = {
                let fe = m.physics.base.fems.get("")?;
                flat.as_map()
                    .iter(ghost)
                    .map(|(ty, _)| (ty, nb_quad(fe, ty, ghost)))
                    .collect()
            };
            for i in 0..m.nb_materials() {
                let law = m.material_mut(i)?;
                let lfilter = law.element_filter().clone();
                law.for_each_internal_mut(&mut |n, f| {
                    if n == field_name
                        && let akantu::model::constitutive_law::InternalFieldMut::Real(field) = f
                    {
                        field.inflate(
                            &lfilter,
                            ghost,
                            &|ty| nqs.get(&ty).copied().unwrap_or(0),
                            &flat,
                        )?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    }
}

// --- SolidMechanicsModel ---------------------------------------------------------------------

/// `SolidMechanicsModel(mesh, spatial_dimension=_all_dimensions,
/// id="solid_mechanics_model", dof_manager=None,
/// model_type=ModelType.solid_mechanics_model)` (PY-075).
#[pyclass(extends = PyClhMaterial, subclass, frozen, name = "SolidMechanicsModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySolidMechanicsModel;

impl PySolidMechanicsModel {
    pub(crate) fn wrap(py: Python<'_>, model: ModelRef) -> PyResult<Bound<'_, Self>> {
        Bound::new(
            py,
            PyModelSolver::init(model)
                .add_subclass(PyModel)
                .add_subclass(PyClhMaterial)
                .add_subclass(Self),
        )
    }
}

fn sid(id: &str) -> Option<&str> {
    (!id.is_empty()).then_some(id)
}

/// The model handle of a Python `SolidMechanicsModel`.
pub(crate) fn mo(slf: &Bound<'_, PySolidMechanicsModel>) -> ModelRef {
    slf.as_super().as_super().as_super().get().model.clone()
}

/// Refresh the model's input-file snapshot from the static parser (upstream
/// `instantiateMaterials` reads `getStaticParser()` at `initFull` time).
fn refresh_parser(m: &mut SolidMechanicsModel) {
    let rt = akantu::runtime::global()
        .read()
        .unwrap_or_else(PoisonError::into_inner);
    m.physics.base.parser = rt.parser.clone();
}

fn material_index(py: Python<'_>, model: &ModelRef, key: &Bound<'_, PyAny>) -> PyResult<usize> {
    if let Ok(el) = key.cast::<PyElement>() {
        let el = el.get().element;
        return model.with(py, move |m| {
            let b = m.material_by_element();
            let idx = b.element_ref(el)?.first().copied().unwrap_or(-1);
            usize::try_from(idx)
                .map_err(|_| Error::not_found("material of element", el.to_string()))
        });
    }
    if let Ok(name) = key.cast::<PyString>() {
        let name = name.to_cow()?.into_owned();
        return model.with(py, move |m| m.material_index(&name));
    }
    let i: i64 = key.extract()?;
    let n = model.with(py, |m| Ok(m.nb_materials()))?;
    usize::try_from(i)
        .ok()
        .filter(|&i| i < n)
        .ok_or_else(|| super::errors::exception(py, format!("The model has no material no {i}")))
}

/// `registerNewMaterial` / `registerNewConstitutiveLaw` (Python allocators
/// installed first): the new law's index.
fn register_law(
    py: Python<'_>,
    model: &ModelRef,
    name: String,
    ty: String,
    option: String,
) -> PyResult<usize> {
    let h = Arc::clone(&model.h);
    let part = model.part;
    model.with(py, move |m| {
        install_python_materials(m, &h, part);
        m.register_new_material(&name, &ty, &option)
    })
}

/// `(name, type, [(parameter, value)])` of `initFull(materials, …)`.
type MaterialSpec = (String, String, Vec<(String, ParamValue)>);

/// Python value → material parameter value (`initFull(materials, …)`).
fn material_param(value: &Bound<'_, PyAny>) -> PyResult<ParamValue> {
    if value.is_instance_of::<pyo3::types::PyBool>() {
        return Ok(ParamValue::Bool(value.extract()?));
    }
    if value.is_instance_of::<PyInt>() {
        return Ok(ParamValue::Int(value.extract()?));
    }
    if value.is_instance_of::<PyFloat>() {
        return Ok(ParamValue::Real(value.extract()?));
    }
    if value.is_instance_of::<PyString>() {
        return Ok(ParamValue::String(value.extract()?));
    }
    Err(super::errors::exception(
        value.py(),
        "to be implemented: material parameter of this Python type".into(),
    ))
}

#[pymethods]
impl PySolidMechanicsModel {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "solid_mechanics_model".to_owned(), dof_manager = None, model_type = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        dof_manager: Option<&Bound<'_, PyAny>>,
        model_type: Option<&Bound<'_, ModelType>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        super::solver::refuse_user_dof_manager(dof_manager)?;
        if let Some(t) = model_type
            && t.get().value != 1
        {
            return Err(super::errors::exception(
                py,
                "SolidMechanicsModel supports model_type=ModelType.solid_mechanics_model only"
                    .into(),
            ));
        }
        let mesh = PyMesh::mesh_ref(mesh);
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let shared = mesh.h.shared.clone();
        let mh = Arc::clone(&mesh.h);
        let model = native(py, move || {
            // values written from Python before the model existed are
            // validated (no out-of-range node or element index reaches
            // native code)
            mh.check_topology()?;
            let parser = akantu::runtime::global()
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .parser
                .clone();
            let m = SolidMechanicsModel::with_parser(shared, dim, &id, parser)?;
            // topology views fetched before become stale (UD-532)
            mh.detach_topology()?;
            Ok(m)
        })?;
        let handle = ModelHandle::new(mesh, AnyModel::Solid(Box::new(model)));
        let h = Arc::clone(&handle);
        native(py, move || {
            h.with(|m| {
                install_python_materials(m.solid(Part::Root)?, &h, Part::Root);
                Ok(())
            })
        })?;
        Ok(PyModelSolver::init(ModelRef::root(handle))
            .add_subclass(PyModel)
            .add_subclass(PyClhMaterial)
            .add_subclass(Self))
    }

    /// `initFull(option=ModelOptions())`, `initFull(_analysis_method=…)`,
    /// `initFull(materials, _analysis_method)` (`materials`: `{name: {"type":
    /// …, param: value}}`).
    #[pyo3(name = "initFull", signature = (option = None, _analysis_method = None, *, materials = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        option: Option<&Bound<'_, PyAny>>,
        _analysis_method: Option<AnalysisArg>,
        materials: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let py = slf.py();
        let mut options = ModelOptions::default();
        let mut mats: Option<Bound<'_, PyDict>> = materials.cloned();
        if let Some(o) = option {
            if let Ok(opts) = o.cast::<PyModelOptions>() {
                options = opts.get().options.clone();
            } else if let Ok(d) = o.cast::<PyDict>() {
                mats = Some(d.clone());
            } else {
                let a: AnalysisArg = o.extract()?;
                options.analysis_method = a.0.unwrap_or(RsAnalysisMethod::ExplicitLumpedMass);
            }
        }
        if let Some(a) = _analysis_method {
            options.analysis_method = a.0.unwrap_or(RsAnalysisMethod::ExplicitLumpedMass);
        }
        // `{name: (type, [(param, value)])}` decoded before the native section
        let mut decoded: Vec<MaterialSpec> = Vec::new();
        if let Some(d) = &mats {
            for (name, params) in d.iter() {
                let name: String = name.str()?.extract()?;
                let params = params.cast::<PyDict>()?;
                let ty = match params.get_item("type")? {
                    Some(t) => t.str()?.extract::<String>()?,
                    None => {
                        return Err(super::errors::exception(
                            py,
                            format!("The material {name} does not have a 'type'"),
                        ));
                    }
                };
                let mut ps = Vec::new();
                for (k, v) in params.iter() {
                    let k: String = k.str()?.extract()?;
                    if k == "type" {
                        continue;
                    }
                    ps.push((k, material_param(&v)?));
                }
                decoded.push((name, ty, ps));
            }
        }
        let with_dict = mats.is_some();
        let model = mo(slf);
        let h = Arc::clone(&model.h);
        let part = model.part;
        model.with(py, move |m| {
            refresh_parser(m);
            install_python_materials(m, &h, part);
            m.init_full(&options)?;
            if with_dict {
                for (name, ty, params) in &decoded {
                    let i = m.register_new_material(name, ty, "")?;
                    let law: &mut dyn Material = m.material_mut(i)?;
                    for (k, v) in params {
                        law.set_param(k, v.clone())?;
                        law.update_internal_parameters()?;
                    }
                }
                m.init_materials()?;
            }
            Ok(())
        })
    }

    /// `applyDirichletBC()`: deprecated no-op (as upstream).
    #[pyo3(name = "applyDirichletBC")]
    fn apply_dirichlet_bc_deprecated(slf: &Bound<'_, Self>) -> PyResult<()> {
        PyErr::warn(
            slf.py(),
            &slf.py().get_type::<PyDeprecationWarning>(),
            c"applyDirichletBC() is deprecated, use applyBC instead",
            1,
        )
    }

    /// `applyBC(functor, element_group)` with a `DirichletFunctor` or a
    /// `NeumannFunctor` (built-in or Python subclass).
    #[pyo3(name = "applyBC")]
    fn apply_bc(
        slf: &Bound<'_, Self>,
        func: &Bound<'_, PyAny>,
        element_group: String,
    ) -> PyResult<()> {
        super::bc::apply_bc(slf.py(), &mo(slf), func, element_group)
    }

    /// `setTimeStep(time_step, solver_id="")`: the model's own
    /// `setTimeStep` (the cohesive model's also sets the time step of the
    /// `"cohesive elements"` dumper, upstream
    /// `SolidMechanicsModelCohesive::setTimeStep`; through the solid
    /// part only, the cohesive dumper wrote no `.pvd`).
    ///
    /// The solid part of a `CouplerSolidContact`: upstream's
    /// `mesh.getDumper().setTimeStep(dt)` reaches the mesh's default
    /// dumper, the coupler's series.
    #[pyo3(name = "setTimeStep", signature = (time_step, solver_id = String::new()))]
    fn set_time_step(slf: &Bound<'_, Self>, time_step: Real, solver_id: String) -> PyResult<()> {
        mo(slf).with_any(slf.py(), move |a, part| match (a, part) {
            (super::handle::AnyModel::SolidContact(c), super::handle::Part::Solid) => {
                c.solid_set_time_step(time_step, sid(&solver_id))
            }
            (a, part) => a.ops(part)?.set_time_step(time_step, sid(&solver_id)),
        })
    }

    /// `getEnergy(energy_id)` or `getEnergy(energy_id, group_id)`.
    #[pyo3(name = "getEnergy", signature = (energy_id, group_id = None))]
    fn get_energy(
        slf: &Bound<'_, Self>,
        energy_id: String,
        group_id: Option<String>,
    ) -> PyResult<Real> {
        mo(slf).with(slf.py(), move |m| match group_id {
            Some(g) => m.energy_of_group(&energy_id, &g),
            None => m.energy(&energy_id),
        })
    }

    #[pyo3(name = "assembleStiffnessMatrix")]
    fn assemble_stiffness_matrix(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), |m| m.assemble_stiffness_matrix(false))
    }

    #[pyo3(name = "assembleInternalForces")]
    fn assemble_internal_forces(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), SolidMechanicsModel::assemble_internal_forces)
    }

    /// Port extension (not in upstream's Python API): release the static
    /// solver's factorisation and the storage of `J` and `K` once no
    /// re-solve with them follows (e.g. after a static preload before an
    /// explicit loop); a later static solve assembles and factorises
    /// again. Collective. `SolidMechanicsModel::release_static_solver`.
    #[pyo3(name = "releaseStaticSolver")]
    fn release_static_solver(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), SolidMechanicsModel::release_static_solver)
    }

    #[pyo3(name = "assembleMass")]
    fn assemble_mass(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), SolidMechanicsModel::assemble_mass)
    }

    #[pyo3(name = "assembleMassLumped")]
    fn assemble_mass_lumped(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), SolidMechanicsModel::assemble_mass_lumped)
    }

    #[pyo3(name = "getStableTimeStep")]
    fn get_stable_time_step(slf: &Bound<'_, Self>) -> PyResult<Real> {
        mo(slf).with(slf.py(), SolidMechanicsModel::stable_time_step)
    }

    #[pyo3(name = "getExternalForce")]
    fn get_external_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("external_force"))
    }

    #[pyo3(name = "getDisplacement")]
    fn get_displacement<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("displacement"))
    }

    #[pyo3(name = "getPreviousDisplacement")]
    fn get_previous_displacement<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("previous_displacement"))
    }

    #[pyo3(name = "getCurrentPosition")]
    fn get_current_position<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).current_position_view(slf.py())
    }

    #[pyo3(name = "getIncrement")]
    fn get_increment<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("displacement_increment"))
    }

    #[pyo3(name = "getInternalForce")]
    fn get_internal_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("internal_force"))
    }

    #[pyo3(name = "getMass")]
    fn get_mass<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("mass"))
    }

    #[pyo3(name = "getVelocity")]
    fn get_velocity<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("velocity"))
    }

    #[pyo3(name = "getAcceleration")]
    fn get_acceleration<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<f64>(slf.py(), ModelKey::Nodal("acceleration"))
    }

    #[pyo3(name = "getBlockedDOFs")]
    fn get_blocked_dofs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mo(slf).view::<bool>(slf.py(), ModelKey::BlockedDofs)
    }

    /// `getMesh()`: the model's mesh (same domain).
    #[pyo3(name = "getMesh")]
    fn get_mesh<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMesh>> {
        PyMesh::wrap(slf.py(), mo(slf).h.mesh.clone())
    }

    /// `getMaterial(material_id | material_name | element)`: one positional
    /// argument, or one of upstream's keyword names (three pybind11
    /// overloads: `py::arg("material_id")` an unsigned integer,
    /// `py::arg("material_name")` a string, `py::arg("element")` an
    /// `Element`).
    #[pyo3(
        name = "getMaterial",
        signature = (key = None, /, *, material_id = None, material_name = None, element = None)
    )]
    fn get_material<'py>(
        slf: &Bound<'py, Self>,
        key: Option<&Bound<'py, PyAny>>,
        material_id: Option<&Bound<'py, PyAny>>,
        material_name: Option<&Bound<'py, PyAny>>,
        element: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let given = [key, material_id, material_name, element];
        let obj = match given {
            [Some(k), None, None, None] => k,
            [None, Some(i), None, None] if i.is_instance_of::<PyInt>() => i,
            [None, None, Some(n), None] if n.is_instance_of::<PyString>() => n,
            [None, None, None, Some(e)] if e.is_instance_of::<PyElement>() => e,
            _ => {
                return Err(PyTypeError::new_err(
                    "getMaterial(): incompatible function arguments. The following argument types \
                     are supported: (material_id: int), (material_name: str), (element: Element)",
                ));
            }
        };
        let model = mo(slf);
        let i = material_index(slf.py(), &model, obj)?;
        material_object(slf.py(), &model, i)
    }

    #[pyo3(name = "getNbMaterials")]
    fn get_nb_materials(slf: &Bound<'_, Self>) -> PyResult<usize> {
        mo(slf).with(slf.py(), |m| Ok(m.nb_materials()))
    }

    #[pyo3(name = "getMaterialIndex")]
    fn get_material_index(slf: &Bound<'_, Self>, name: String) -> PyResult<usize> {
        mo(slf).with(slf.py(), move |m| m.material_index(&name))
    }

    /// `getMaterialByElement()`: `ElementTypeMapArrayInt` (material index
    /// per element; writable views).
    #[pyo3(name = "getMaterialByElement")]
    fn get_material_by_element(slf: &Bound<'_, Self>) -> EtmaInt {
        EtmaInt::from_provider(Arc::new(LawByElementProvider(mo(slf))))
    }

    #[pyo3(name = "reassignMaterial")]
    fn reassign_material(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with(slf.py(), SolidMechanicsModel::reassign_material)
    }

    /// `registerNewMaterial(material_name, material_type, option="")`
    /// (returns `None`, as upstream).
    #[pyo3(name = "registerNewMaterial", signature = (material_name, material_type, option = String::new()))]
    fn register_new_material(
        slf: &Bound<'_, Self>,
        material_name: String,
        material_type: String,
        option: String,
    ) -> PyResult<()> {
        register_law(slf.py(), &mo(slf), material_name, material_type, option).map(drop)
    }

    /// `setMaterialSelector(selector)`: the selector (and its fallbacks)
    /// replace the model's selector chain (upstream
    /// `setConstitutiveLawSelector`).
    #[pyo3(name = "setMaterialSelector")]
    fn set_material_selector(
        slf: &Bound<'_, Self>,
        selector: &Bound<'_, PyConstitutiveLawSelector>,
    ) -> PyResult<()> {
        let chain = selector_chain(selector)?;
        let model = mo(slf);
        model.h.set_selector_object(
            model.part,
            Handler::Material,
            selector.clone().into_any().unbind(),
        );
        model.with(slf.py(), move |m| {
            m.set_material_selector_chain(chain);
            Ok(())
        })
    }

    /// `getMaterialSelector()`: the selector set last (else the default
    /// `DefaultMaterialSelector`).
    #[pyo3(name = "getMaterialSelector")]
    fn get_material_selector<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let model = mo(slf);
        let py = slf.py();
        if let Some(s) = model.h.selector_object(py, model.part, Handler::Material) {
            return Ok(s.into_bound(py));
        }
        Ok(Bound::new(
            py,
            PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::LawIndex))
                .add_subclass(PyDefaultMaterialSelector),
        )?
        .into_any())
    }

    /// `initConstitutiveLaws()` (akantu-rs helper; upstream `initMaterials`
    /// is not bound): the model's own initialisation of its laws (on a
    /// `SolidMechanicsModelCohesive` the cohesive model's, with the facet
    /// laws and the insertion).
    #[pyo3(name = "initConstitutiveLaws")]
    fn init_constitutive_laws(slf: &Bound<'_, Self>) -> PyResult<()> {
        mo(slf).with_ops(slf.py(), |m| m.init_constitutive_laws())
    }

    /// `getTimeStep(solver_id="")` (akantu-rs helper).
    #[pyo3(name = "getTimeStep", signature = (solver_id = String::new()))]
    fn get_time_step(slf: &Bound<'_, Self>, solver_id: String) -> PyResult<Real> {
        mo(slf).with(slf.py(), move |m| m.time_step(sid(&solver_id)))
    }

    /// Spatial dimension (akantu-rs helper).
    #[pyo3(name = "getSpatialDimension")]
    fn get_spatial_dimension(slf: &Bound<'_, Self>) -> PyResult<usize> {
        mo(slf).with(slf.py(), |m| Ok(m.spatial_dimension()))
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let id = mo(slf).with(slf.py(), |m| Ok(m.id().to_owned()))?;
        Ok(format!("<SolidMechanicsModel '{id}'>"))
    }
}

/// Register `register_solid_mechanics_model` + `register_material`
/// classes.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    super::laws::add_class_as::<PyClhMaterial>(
        m,
        "ConstitutiveLawsHandlerakantu::Materialakantu::Model",
    )?;
    m.add("SolidMechanicsModelOptions", m.getattr("ModelOptions")?)?;
    m.add_class::<PySolidMechanicsModel>()?;
    Ok(())
}

/// `register_material`: `Material` and the elastic laws.
pub(crate) fn register_material(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMaterial>()?;
    m.add_class::<PyMaterialLinearElastic1D>()?;
    m.add_class::<PyMaterialLinearElastic2D>()?;
    m.add_class::<PyMaterialLinearElastic3D>()?;
    Ok(())
}
