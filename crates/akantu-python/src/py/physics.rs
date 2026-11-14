// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_heat_transfer_model.cc,
// python/py_structural_mechanics_model.cc, python/py_phase_field_model.cc,
// python/py_phasefield.cc, python/py_contact_mechanics_model.cc,
// python/py_model_couplers.cc.

//! The other physics models (PY-083…087, PY-089, PY-091…093).
//!
//! DOF arrays (temperature, structural displacement / velocity /
//! acceleration, damage, blocked DOFs) are zero-copy views of the DOF
//! manager's storage. Arrays the Rust models keep as plain arrays
//! (structural external / internal forces and element materials, the
//! phase-field internal force, every contact nodal field) are returned as
//! **read-only snapshot copies**, so a write raises instead of being lost
//! (upstream: writable references). Python subclasses of
//! `DiffusionLaw` / `HeatDiffusion*D` and `PhaseField`, and Python phase-field
//! allocators, need runtime law types the Rust crates do not have: they
//! raise (same proposal).

use std::sync::{Arc, PoisonError};

use akantu::core::{Array, ElementKind, ElementType, Error, GhostType, Real, TypeFilter};
use akantu::model::ModelOptions;
use akantu::parser::param::ParameterRegistry as _;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple};

use super::arrays::owned_numpy;
use super::common::PyElement;
use super::enums::{EtArg, GhostArg, KindArg, ModelType};
use super::etma::{EtmaInt, EtmaProvider, EtmaReal};
use super::handle::{AnyModel, Handler, ModelHandle, ModelKey, ModelKind, ModelRef, Part};
use super::laws::{
    OtherLaw, PyConstitutiveLawInternalHandler, PyConstitutiveLawSelector, add_class_as,
};
use super::mesh::{PY_ALL_DIMENSIONS, PyClusteringFilter, PyMesh};
use super::model::{AnalysisArg, PyModel, PyModelOptions, PyModelSolver, model_of};
use super::parser::PyParameterRegistry;
use super::runtime::native;
use crate::foreign_view::numpy_export::{Exportable, Layout};

fn static_parser() -> akantu::parser::Parser {
    akantu::runtime::global()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .parser
        .clone()
}

fn read_only(a: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    a.call_method1("setflags", (false,))?;
    Ok(a)
}

/// `(values, rows, cols)` of an array.
fn parts<T: Clone>(a: &Array<T>) -> (Vec<T>, usize, usize) {
    (a.as_slice().to_vec(), a.len(), a.nb_component())
}

fn copy_parts<'py, T: Exportable>(
    py: Python<'py>,
    p: (Vec<T>, usize, usize),
) -> PyResult<Bound<'py, PyAny>> {
    read_only(owned_numpy(py, p.0, p.1, p.2, Layout::Rows)?)
}

/// Options from `initFull`'s first argument (`ModelOptions` or an analysis
/// method).
fn options_of(arg: Option<&Bound<'_, PyAny>>) -> PyResult<ModelOptions> {
    let mut o = ModelOptions::default();
    if let Some(a) = arg {
        if let Ok(m) = a.cast::<PyModelOptions>() {
            o = m.get().options.clone();
        } else {
            let m: AnalysisArg = a.extract()?;
            if let Some(m) = m.0 {
                o.analysis_method = m;
            }
        }
    }
    Ok(o)
}

fn new_handle(
    py: Python<'_>,
    mesh: &Bound<'_, PyMesh>,
    build: impl FnOnce(
        akantu::mesh::SharedMesh,
        akantu::parser::Parser,
    ) -> akantu::core::Result<AnyModel>
    + Send,
) -> PyResult<ModelRef> {
    let mesh = PyMesh::mesh_ref(mesh);
    let shared = mesh.h.shared.clone();
    let mh = Arc::clone(&mesh.h);
    let model = native(py, move || {
        mh.check_topology()?;
        let m = build(shared, static_parser())?;
        mh.detach_topology()?;
        Ok(m)
    })?;
    Ok(ModelRef::root(ModelHandle::new(mesh, model)))
}

fn base(model: ModelRef) -> PyClassInitializer<PyModel> {
    PyModelSolver::init(model).add_subclass(PyModel)
}

/// The Python model object of a model of another physics.
pub(crate) fn model_object<'py>(
    py: Python<'py>,
    model: &ModelRef,
    kind: ModelKind,
) -> PyResult<Bound<'py, PyAny>> {
    let b = base(model.clone());
    Ok(match kind {
        ModelKind::Heat => Bound::new(
            py,
            b.add_subclass(PyClhDiffusion)
                .add_subclass(PyDiffusionModel)
                .add_subclass(PyHeatTransferModel),
        )?
        .into_any(),
        ModelKind::Structural => {
            Bound::new(py, b.add_subclass(PyStructuralMechanicsModel))?.into_any()
        }
        ModelKind::PhaseField => Bound::new(
            py,
            b.add_subclass(PyClhPhaseField)
                .add_subclass(PyPhaseFieldModel),
        )?
        .into_any(),
        ModelKind::Contact => Bound::new(py, b.add_subclass(PyContactMechanicsModel))?.into_any(),
        ModelKind::SolidContact => {
            Bound::new(py, b.add_subclass(PyCouplerSolidContact))?.into_any()
        }
        ModelKind::SolidCohesiveContact => {
            Bound::new(py, b.add_subclass(PyCouplerSolidCohesiveContact))?.into_any()
        }
        ModelKind::SolidPhaseField => {
            Bound::new(py, b.add_subclass(PyCouplerSolidPhaseField))?.into_any()
        }
        ModelKind::Solid | ModelKind::Cohesive => super::handle::model_object(py, model)?,
    })
}

// --- law handlers of the diffusion and phase-field models -------------------------------

/// The Python object of law `index` of handler `handler`.
fn law_object<'py>(
    py: Python<'py>,
    model: &ModelRef,
    handler: Handler,
    index: usize,
) -> PyResult<Bound<'py, PyAny>> {
    let law = OtherLaw {
        model: model.clone(),
        handler,
        index,
    };
    let init = PyConstitutiveLawInternalHandler::init_other(law.clone());
    match handler {
        Handler::Diffusion => {
            let (ty, dim) = native(py, || {
                let part = model.part;
                model.h.with(|m| {
                    let ops = m.ops(part)?;
                    let dim = ops.spatial_dimension();
                    let mut ty = String::new();
                    ops.with_handler_law(handler, index, &mut |l| {
                        ty = l.law_type().to_owned();
                        Ok(())
                    })?;
                    Ok((ty, dim))
                })
            })?;
            let i = init
                .add_subclass(PyClDiffusion)
                .add_subclass(PyDiffusionLaw);
            Ok(match (ty.as_str(), dim) {
                ("heat_diffusion", 1) => {
                    Bound::new(py, i.add_subclass(PyHeatDiffusion1D))?.into_any()
                }
                ("heat_diffusion", 2) => {
                    Bound::new(py, i.add_subclass(PyHeatDiffusion2D))?.into_any()
                }
                ("heat_diffusion", 3) => {
                    Bound::new(py, i.add_subclass(PyHeatDiffusion3D))?.into_any()
                }
                _ => Bound::new(py, i)?.into_any(),
            })
        }
        Handler::PhaseField => {
            let id = native(py, || {
                let part = model.part;
                model.h.with(|m| {
                    let mut id = String::new();
                    m.ops(part)?.with_handler_law(handler, index, &mut |l| {
                        id = l.law_id().to_owned();
                        Ok(())
                    })?;
                    Ok(id)
                })
            })?;
            if let Some(obj) = model.h.law_object(model.part, &id) {
                return Ok(obj.bind(py).clone());
            }
            Ok(Bound::new(
                py,
                init.add_subclass(PyClPhaseField).add_subclass(PyPhaseField),
            )?
            .into_any())
        }
        Handler::Material => super::solid::material_object(py, model, index),
    }
}

fn law_index(
    py: Python<'_>,
    model: &ModelRef,
    handler: Handler,
    key: &Bound<'_, PyAny>,
) -> PyResult<usize> {
    if let Ok(el) = key.cast::<PyElement>() {
        let el = el.get().element;
        let h = Arc::clone(&model.h);
        let part = model.part;
        return native(py, move || {
            let v = h.export::<i32>(
                part,
                ModelKey::LawByElement(handler, el.ty(), el.ghost_type()),
            )?;
            let _ = v;
            h.with(|m| {
                let ops = m.ops(part)?;
                let a = ops.law_by_element_shared_mut(handler, el.ty(), el.ghost_type())?;
                let r = a.read()?;
                let i = r.as_slice().get(el.idx()).copied().unwrap_or(-1);
                usize::try_from(i).map_err(|_| Error::not_found("law of element", el.to_string()))
            })
        });
    }
    if let Ok(s) = key.cast::<PyString>() {
        let name = s.to_cow()?.into_owned();
        return model.with_ops(py, move |m| m.law_index_by_name(handler, &name));
    }
    let i: usize = key.extract()?;
    Ok(i)
}

/// `getConstitutiveLawByElement()` of a handler (writable views).
#[derive(Debug)]
struct LawByElement(ModelRef, Handler);

impl EtmaProvider<i32> for LawByElement {
    fn id(&self, _py: Python<'_>) -> PyResult<String> {
        Ok("constitutive_law_index".to_owned())
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let mesh = self.0.h.mesh.clone();
        let dim = self.0.with_ops(py, |m| Ok(m.spatial_dimension()))?;
        mesh.read(py, move |m| {
            Ok(m.element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ..filter
            })
            .collect())
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.0
            .view::<i32>(py, ModelKey::LawByElement(self.1, ty, ghost))
    }
}

/// `getConstitutiveLawLocalNumbering()` of a handler: read-only snapshot
/// copies (law-local numberings are not exportable storage, D-056 item 2).
#[derive(Debug)]
struct LocalNumbering(ModelRef, Handler);

impl EtmaProvider<i32> for LocalNumbering {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        let h = self.1;
        self.0
            .with_ops(py, move |o| Ok(o.law_local_numbering(h)?.id().to_owned()))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        let h = self.1;
        self.0.with_ops(py, move |o| {
            Ok(o.law_local_numbering(h)?.element_types(filter).collect())
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = self.1;
        let (v, r, c) = self.0.with_ops(py, move |o| {
            let n = o.law_local_numbering(h)?;
            let a = n.get(ty, ghost)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        read_only(owned_numpy(py, v, r, c, Layout::Rows)?)
    }
}

/// Elements and integration points per element type of the model's FE
/// engine `engine` (`""`: the default engine), for flattening / inflating
/// law internals.
type QuadratureSizes = std::collections::BTreeMap<ElementType, (usize, usize)>;

fn quadrature_sizes(
    o: &mut dyn super::model_ops::ModelOps,
    engine: &str,
    ghost: GhostType,
) -> akantu::core::Result<QuadratureSizes> {
    let mut sizes = std::collections::BTreeMap::new();
    o.with_fe_engine(engine, false, &mut |fe, mesh| {
        for t in fe.element_types(mesh, ghost) {
            let nq = fe.nb_integration_points(t, ghost).unwrap_or(0);
            sizes.insert(t, (mesh.nb_element(t, ghost), nq));
        }
        Ok(())
    })?;
    Ok(sizes)
}

/// The FE engine (`""`: the default) of law `law`'s internal `name`, `None`
/// when the law has no such internal (CL-001: upstream's
/// `flattenInternal` / `inflateInternal` read an internal with
/// `internal_field.getFEEngine()`).
fn internal_engine(
    o: &mut dyn super::model_ops::ModelOps,
    handler: Handler,
    law: usize,
    name: &str,
) -> akantu::core::Result<Option<String>> {
    let mut engine = None;
    o.with_handler_law(handler, law, &mut |law| {
        law.for_each_internal(&mut |n, f| {
            if n == name && engine.is_none() {
                engine = Some(f.fe_engine_id().unwrap_or("").to_owned());
            }
        });
        Ok(())
    })?;
    Ok(engine)
}

/// The quadrature sizes of `engine`, computed once per engine.
fn sizes_of<'a>(
    o: &mut dyn super::model_ops::ModelOps,
    cache: &'a mut std::collections::BTreeMap<String, QuadratureSizes>,
    engine: &str,
    ghost: GhostType,
) -> akantu::core::Result<&'a QuadratureSizes> {
    if !cache.contains_key(engine) {
        let s = quadrature_sizes(o, engine, ghost)?;
        cache.insert(engine.to_owned(), s);
    }
    cache
        .get(engine)
        .ok_or_else(|| akantu::core::Error::not_found("FE engine", engine))
}

/// `flattenInternalReal(field_name, kind, ghost_type)` of a handler: the
/// laws' internal scattered into mesh-indexed blocks (a new map; upstream: a
/// map owned by the model), as the solid handler. Each law's internal is
/// read on its own support (CL-001; upstream
/// `ConstitutiveLaw::flattenInternal`): the points per element of the
/// field's FE engine and the field's element filter (its own, else the
/// law's). Two laws that write elements of one type with a different
/// number of values per element are refused (upstream resizes the shared
/// block and reinterprets the first law's values, UD-1083 (d)).
fn flatten_internal(
    py: Python<'_>,
    model: &ModelRef,
    handler: Handler,
    field_name: String,
    kind: Option<ElementKind>,
    ghost: GhostType,
) -> PyResult<EtmaReal> {
    let name = field_name.clone();
    let blocks = model.with_ops(py, move |o| {
        let mut cache = std::collections::BTreeMap::new();
        let mut out = akantu::core::ElementTypeMapArray::<Real>::new(&name);
        // values per element of the first law writing elements of a type
        let mut written: std::collections::BTreeMap<ElementType, usize> =
            std::collections::BTreeMap::new();
        for i in 0..o.law_count(handler)? {
            let Some(engine) = internal_engine(o, handler, i, &name)? else {
                continue;
            };
            let sizes = sizes_of(o, &mut cache, &engine, ghost)?.clone();
            o.with_handler_law(handler, i, &mut |law| {
                let law = &*law;
                let mut err = None;
                law.for_each_internal(&mut |n, f| {
                    if n != name || err.is_some() {
                        return;
                    }
                    if let akantu::model::constitutive_law::InternalFieldRef::Real(field) = f {
                        let filter = field.element_filter().unwrap_or(law.element_filter());
                        for (ty, _) in field.values().as_map().iter(ghost) {
                            if !filter.get(ty, ghost).is_ok_and(|f| !f.is_empty()) {
                                continue;
                            }
                            let per = sizes.get(&ty).map_or(0, |s| s.1) * field.nb_component();
                            match written.get(&ty) {
                                None => {
                                    written.insert(ty, per);
                                }
                                Some(&w) if w != per => {
                                    err = Some(akantu::core::Error::Unsupported(format!(
                                        "flattenInternal('{name}') on {ty:?}: laws hold it with \
                                         {w} and {per} values per element (upstream \
                                         reinterprets the first law's values, UD-1083)"
                                    )));
                                    return;
                                }
                                Some(_) => {}
                            }
                        }
                        if let Err(e) = field.flatten(
                            filter,
                            ghost,
                            &|ty| sizes.get(&ty).map_or(0, |s| s.0),
                            &|ty| sizes.get(&ty).map_or(0, |s| s.1),
                            0.0,
                            &mut out,
                        ) {
                            err = Some(e);
                        }
                    }
                });
                err.map_or(Ok(()), Err)
            })?;
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

/// `inflateInternalReal(field_name, field, kind, ghost_type)` of a handler:
/// mesh-indexed values gathered back into the laws' internal.
fn inflate_internal(
    py: Python<'_>,
    model: &ModelRef,
    handler: Handler,
    field_name: String,
    field: &Bound<'_, EtmaReal>,
    kind: Option<ElementKind>,
    ghost: GhostType,
) -> PyResult<()> {
    let filter = TypeFilter {
        spatial_dimension: None,
        ghost_type: ghost,
        kind,
    };
    // the field's blocks are copied before the native section
    let p = &field.get().provider;
    let mut flat = akantu::core::ElementTypeMapArray::<Real>::new(&field_name);
    for ty in p.types(py, filter)? {
        let a = p.call(py, ty, ghost)?;
        let (values, shape) = super::arrays::input_f64(&a)?;
        let nbc = shape.get(1).copied().unwrap_or(1).max(1);
        let arr = Array::from_vec(values, nbc).map_err(|e| super::runtime::to_py(py, e))?;
        flat.as_map_mut()
            .insert(ty, ghost, arr)
            .map_err(|e| super::runtime::to_py(py, e))?;
    }
    model.with_ops(py, move |o| {
        let mut cache = std::collections::BTreeMap::new();
        for i in 0..o.law_count(handler)? {
            // the internal's own engine and filter (CL-001;
            // upstream `inflateInternal`: `getFEEngine()`,
            // `getFilter(type, ghost)`)
            let Some(engine) = internal_engine(o, handler, i, &field_name)? else {
                continue;
            };
            let sizes = sizes_of(o, &mut cache, &engine, ghost)?.clone();
            o.with_handler_law(handler, i, &mut |law| {
                let lfilter = law.element_filter().clone();
                law.for_each_internal_mut(&mut |n, f| {
                    if n == field_name
                        && let akantu::model::constitutive_law::InternalFieldMut::Real(field) = f
                    {
                        let filter = field
                            .element_filter()
                            .cloned()
                            .unwrap_or_else(|| lfilter.clone());
                        field.inflate(
                            &filter,
                            ghost,
                            &|ty| sizes.get(&ty).map_or(0, |s| s.1),
                            &flat,
                        )?;
                    }
                    Ok(())
                })
            })?;
        }
        Ok(())
    })
}

macro_rules! clh_class {
    ($rust:ident, $internal:literal, $handler:expr) => {
        #[doc = concat!("`ConstitutiveLawsHandler<…, Model>` of ", stringify!($handler), " (PY-074).")]
        #[pyclass(extends = PyModel, subclass, frozen, name = $internal, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;

        #[pymethods]
        impl $rust {
            #[pyo3(name = "getConstitutiveLaw")]
            fn get_constitutive_law<'py>(slf: &Bound<'py, Self>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
                let m = model_of(slf)?;
                let i = law_index(slf.py(), &m, $handler, key)?;
                law_object(slf.py(), &m, $handler, i)
            }
            #[pyo3(name = "getConstitutiveLaws")]
            fn get_constitutive_laws<'py>(slf: &Bound<'py, Self>) -> PyResult<Vec<Bound<'py, PyAny>>> {
                let m = model_of(slf)?;
                let n = m.with_ops(slf.py(), |o| o.law_count($handler))?;
                (0..n).map(|i| law_object(slf.py(), &m, $handler, i)).collect()
            }
            #[pyo3(name = "getNbConstitutiveLaws")]
            fn get_nb_constitutive_laws(slf: &Bound<'_, Self>) -> PyResult<usize> {
                model_of(slf)?.with_ops(slf.py(), |o| o.law_count($handler))
            }
            #[pyo3(name = "getConstitutiveLawIndex")]
            fn get_constitutive_law_index(slf: &Bound<'_, Self>, name: String) -> PyResult<usize> {
                model_of(slf)?.with_ops(slf.py(), move |o| o.law_index_by_name($handler, &name))
            }
            #[pyo3(name = "getConstitutiveLawByElement")]
            fn get_constitutive_law_by_element(slf: &Bound<'_, Self>) -> PyResult<EtmaInt> {
                Ok(EtmaInt::from_provider(Arc::new(LawByElement(model_of(slf)?, $handler))))
            }
            #[pyo3(name = "registerNewConstitutiveLaw", signature = (name, r#type, opt_param = String::new()))]
            fn register_new_constitutive_law<'py>(slf: &Bound<'py, Self>, name: String, r#type: String, opt_param: String) -> PyResult<Bound<'py, PyAny>> {
                let m = model_of(slf)?;
                let i = m.with_ops(slf.py(), move |o| o.register_handler_law($handler, &name, &r#type, &opt_param))?;
                law_object(slf.py(), &m, $handler, i)
            }
            #[pyo3(name = "getConstitutiveLawLocalNumbering")]
            fn get_constitutive_law_local_numbering(slf: &Bound<'_, Self>) -> PyResult<EtmaInt> {
                Ok(EtmaInt::from_provider(Arc::new(LocalNumbering(model_of(slf)?, $handler))))
            }
            /// `reassignConstitutiveLaw()`: the selector is asked again
            /// and elements move between laws (D-078).
            #[pyo3(name = "reassignConstitutiveLaw")]
            fn reassign_constitutive_law(slf: &Bound<'_, Self>) -> PyResult<()> {
                model_of(slf)?.with_ops(slf.py(), |o| o.reassign_handler_laws($handler))
            }
            #[pyo3(name = "flattenInternalReal", signature = (field_name, kind = KindArg(Some(ElementKind::Regular)), ghost_type = GhostArg(Some(GhostType::NotGhost))))]
            fn flatten_internal_real(slf: &Bound<'_, Self>, field_name: String, kind: KindArg, ghost_type: GhostArg) -> PyResult<EtmaReal> {
                flatten_internal(slf.py(), &model_of(slf)?, $handler, field_name, kind.0, ghost_type.single()?)
            }
            #[pyo3(name = "inflateInternalReal", signature = (field_name, field, kind = KindArg(Some(ElementKind::Regular)), ghost_type = GhostArg(Some(GhostType::NotGhost))))]
            fn inflate_internal_real(slf: &Bound<'_, Self>, field_name: String, field: &Bound<'_, EtmaReal>, kind: KindArg, ghost_type: GhostArg) -> PyResult<()> {
                inflate_internal(slf.py(), &model_of(slf)?, $handler, field_name, field, kind.0, ghost_type.single()?)
            }
            #[pyo3(name = "isInternal", signature = (field_name, element_kind = KindArg(Some(ElementKind::Regular))))]
            fn is_internal(slf: &Bound<'_, Self>, field_name: String, element_kind: KindArg) -> PyResult<bool> {
                let _ = element_kind;
                model_of(slf)?.with_ops(slf.py(), move |o| {
                    let mut found = false;
                    for i in 0..o.law_count($handler)? {
                        o.with_handler_law($handler, i, &mut |l| {
                            l.for_each_internal(&mut |n, _| found |= n == field_name);
                            Ok(())
                        })?;
                    }
                    Ok(found)
                })
            }
        }
    };
}

clh_class!(
    PyClhDiffusion,
    "ConstitutiveLawsHandler_DiffusionLaw_Model",
    Handler::Diffusion
);
clh_class!(
    PyClhPhaseField,
    "ConstitutiveLawsHandler_PhaseField_Model",
    Handler::PhaseField
);

macro_rules! cl_class {
    ($rust:ident, $internal:literal) => {
        #[doc = "`ConstitutiveLaw<…Model>` (PY-073): `set*` methods."]
        #[pyclass(extends = PyConstitutiveLawInternalHandler, subclass, frozen, name = $internal, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;

        #[pymethods]
        impl $rust {
            #[pyo3(name = "setReal", signature = (name, value))]
            fn set_real(slf: &Bound<'_, Self>, name: &str, value: Real) -> PyResult<()> {
                reg(slf.as_super().as_super().as_super()).set_value(slf.py(), name, akantu::parser::param::ParamValue::Real(value))
            }
            #[pyo3(name = "setBool", signature = (name, value))]
            fn set_bool(slf: &Bound<'_, Self>, name: &str, value: bool) -> PyResult<()> {
                reg(slf.as_super().as_super().as_super()).set_value(slf.py(), name, akantu::parser::param::ParamValue::Bool(value))
            }
            #[pyo3(name = "setString", signature = (name, value))]
            fn set_string(slf: &Bound<'_, Self>, name: &str, value: String) -> PyResult<()> {
                reg(slf.as_super().as_super().as_super()).set_value(slf.py(), name, akantu::parser::param::ParamValue::String(value))
            }
            #[pyo3(name = "setInt", signature = (name, value))]
            fn set_int(slf: &Bound<'_, Self>, name: &str, value: i32) -> PyResult<()> {
                reg(slf.as_super().as_super().as_super()).set_value(slf.py(), name, akantu::parser::param::ParamValue::Int(i64::from(value)))
            }
        }
    };
}

fn reg<'a>(b: &'a Bound<'_, PyParameterRegistry>) -> &'a PyParameterRegistry {
    b.get()
}

cl_class!(PyClDiffusion, "ConstitutiveLaw_DiffusionModel");
cl_class!(PyClPhaseField, "ConstitutiveLaw_PhaseFieldModel");

/// `DiffusionLaw` (PY-083).
#[pyclass(extends = PyClDiffusion, subclass, frozen, name = "DiffusionLaw", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDiffusionLaw;

/// Model services over a mesh alone: what a law constructor needs (the
/// mesh and the dimension) for a law built outside any model phase.
struct MeshOnly<'a> {
    mesh: &'a akantu::mesh::Mesh,
    dim: usize,
}

impl akantu::model::constitutive_law::ModelServices for MeshOnly<'_> {
    fn spatial_dimension(&self) -> usize {
        self.dim
    }
    fn mesh(&self) -> &akantu::mesh::Mesh {
        self.mesh
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
        0
    }
    fn integration_point_coordinates(
        &self,
        _: ElementType,
        _: GhostType,
        _: &[akantu::core::Idx],
        _: &mut Array<Real>,
    ) -> akantu::core::Result<()> {
        Err(Error::Unsupported(
            "integration points of a law constructed from Python".into(),
        ))
    }
    fn nodal_field(
        &self,
        name: &str,
    ) -> akantu::core::Result<akantu::core::array_shared::ArrayRef<'_, Real>> {
        Err(Error::not_found("nodal field", name))
    }
}

/// The parameters of a `HeatDiffusion<d>D` object constructed from Python
/// (a Python subclass): the law's own parameters (`name`,
/// `density`, `conductivity`, `conductivity_variation`,
/// `temperature_reference`, `capacity`) and those registered from Python.
#[derive(Debug)]
struct PythonHeatDiffusion {
    law: std::sync::Mutex<(
        akantu::diffusion::HeatDiffusion,
        akantu::model::params::DynamicParameters,
    )>,
}

/// The two registries of a [`PythonHeatDiffusion`] as one.
struct HeatParams<'a> {
    law: &'a mut akantu::diffusion::HeatDiffusion,
    extra: &'a mut akantu::model::params::DynamicParameters,
}

impl akantu::parser::param::ParameterRegistry for HeatParams<'_> {
    fn get_param(&self, name: &str) -> akantu::core::Result<akantu::parser::param::ParamValue> {
        if self.law.param_info(name).is_some() {
            return self.law.get_param(name);
        }
        self.extra.get(name)
    }
    fn set_param(
        &mut self,
        name: &str,
        value: akantu::parser::param::ParamValue,
    ) -> akantu::core::Result<()> {
        if self.law.param_info(name).is_some() {
            return self.law.set_param(name, value);
        }
        self.extra.set(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.law.param_names()
    }
    fn update_internal_parameters(&mut self) -> akantu::core::Result<()> {
        self.law.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<akantu::parser::param::ParamInfo> {
        self.law
            .param_info(name)
            .or_else(|| self.extra.param_info(name))
    }
}

impl super::parser::RegistryAccess for PythonHeatDiffusion {
    fn with_registry(
        &self,
        _prepared: &super::parser::Prepared,
        f: &mut dyn FnMut(
            &mut dyn akantu::parser::param::ParameterRegistry,
        ) -> akantu::core::Result<()>,
    ) -> akantu::core::Result<()> {
        let mut g = self.law.lock().unwrap_or_else(PoisonError::into_inner);
        let (law, extra) = &mut *g;
        f(&mut HeatParams { law, extra })
    }
    fn register_param_real(
        &self,
        _prepared: &super::parser::Prepared,
        name: &str,
        access: akantu::parser::param::ParameterAccess,
        description: &str,
        default: Option<f64>,
    ) -> akantu::core::Result<()> {
        let mut g = self.law.lock().unwrap_or_else(PoisonError::into_inner);
        let (law, extra) = &mut *g;
        if law.param_info(name).is_some() {
            return Err(Error::invalid(format!(
                "parameter '{name}' is already registered by HeatDiffusion"
            )));
        }
        extra.register(
            name,
            akantu::parser::param::ParamKind::Real,
            access,
            description,
            default.map(akantu::parser::param::ParamValue::Real),
        )
    }
}

/// `HeatDiffusion<d>D(model, id)` built from Python: upstream's constructor
/// (`py_heat_transfer_model.cc:65-70`) creates a law that is **not** added
/// to the model (`ConstitutiveLaw`'s constructor registers nothing in the
/// handler, and upstream binds no allocator of the diffusion factory): the
/// object carries the law's parameters, its element filter (empty blocks)
/// and the internals registered from Python; the model never calls it.
fn python_heat_diffusion(
    model: &Bound<'_, PyAny>,
    id: String,
    dim: usize,
) -> PyResult<PyClassInitializer<PyDiffusionLaw>> {
    let py = model.py();
    let m = model_of(model.cast::<PyDiffusionModel>()?)?;
    let spec = akantu::model::constitutive_law::LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: id.clone(),
    };
    let law = m.h.mesh.read(py, move |mesh| {
        akantu::diffusion::HeatDiffusion::new(&spec, &MeshOnly { mesh, dim })
    })?;
    let access = PythonHeatDiffusion {
        law: std::sync::Mutex::new((law, akantu::model::params::DynamicParameters::new())),
    };
    Ok(
        PyClassInitializer::from(PyParameterRegistry::external(Arc::new(access)))
            .add_subclass(super::parser::PyParsable)
            .add_subclass(PyConstitutiveLawInternalHandler::standalone(
                id,
                i64::try_from(dim).unwrap_or(3),
            ))
            .add_subclass(PyClDiffusion)
            .add_subclass(PyDiffusionLaw),
    )
}

macro_rules! heat_class {
    ($rust:ident, $name:literal, $dim:literal) => {
        #[doc = concat!("`", $name, "` (PY-083). A Python subclass is constructible (`super().__init__(model, id)`) as upstream; as upstream, nothing adds the object to a model (no allocator of the diffusion factory is bound), so its overrides are never called.")]
        #[pyclass(extends = PyDiffusionLaw, subclass, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;

        #[pymethods]
        impl $rust {
            /// `HeatDiffusion<d>D(model, id)` (the law of a heat transfer
            /// model, built outside the model).
            #[new]
            #[pyo3(signature = (model, id, *_args, **_kwargs))]
            fn new(
                model: &Bound<'_, PyAny>,
                id: String,
                _args: &Bound<'_, PyTuple>,
                _kwargs: Option<&Bound<'_, PyDict>>,
            ) -> PyResult<PyClassInitializer<Self>> {
                Ok(python_heat_diffusion(model, id, $dim)?.add_subclass(Self))
            }

            /// `super().__init__(model, id)` of a Python subclass (the law
            /// was built by the constructor).
            #[pyo3(name = "__init__", signature = (*_args, **_kwargs))]
            fn init(&self, _args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) {}
        }
    };
}

heat_class!(PyHeatDiffusion1D, "HeatDiffusion1D", 1);
heat_class!(PyHeatDiffusion2D, "HeatDiffusion2D", 2);
heat_class!(PyHeatDiffusion3D, "HeatDiffusion3D", 3);

/// `DiffusionModel` (PY-084).
#[pyclass(extends = PyClhDiffusion, subclass, frozen, name = "DiffusionModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDiffusionModel;

#[pymethods]
impl PyDiffusionModel {
    /// `initFull(_analysis_method)`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        let m = model_of(slf)?;
        m.with_any(slf.py(), move |a, part| {
            let h = a.heat(part)?;
            h.0.physics.base.parser = static_parser();
            h.0.init_full(&o)
        })
    }
    #[pyo3(name = "setTimeStep", signature = (time_step, solver_id = String::new()))]
    fn set_time_step(slf: &Bound<'_, Self>, time_step: Real, solver_id: String) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.set_time_step(time_step, Some(&solver_id))
        })
    }
    #[pyo3(name = "getStableTimeStep")]
    fn get_stable_time_step(slf: &Bound<'_, Self>) -> PyResult<Real> {
        model_of(slf)?.with_any(slf.py(), |a, p| a.heat(p)?.0.stable_time_step())
    }
    #[pyo3(name = "getBlockedDOFs")]
    fn get_blocked_dofs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<bool>(slf.py(), ModelKey::BlockedDofs)
    }
    /// `applyBC(DirichletFunctor, group)` (akantu-rs: also on diffusion
    /// models; upstream binds it on the solid and phase-field models only).
    #[pyo3(name = "applyBC")]
    fn apply_bc(
        slf: &Bound<'_, Self>,
        func: &Bound<'_, PyAny>,
        element_group: String,
    ) -> PyResult<()> {
        super::bc::apply_bc(slf.py(), &model_of(slf)?, func, element_group)
    }
    #[pyo3(name = "getEnergy", signature = (energy_id))]
    fn get_energy(slf: &Bound<'_, Self>, energy_id: String) -> PyResult<Real> {
        model_of(slf)?.with_any(slf.py(), move |a, p| a.heat(p)?.0.energy(&energy_id))
    }
}

/// `HeatTransferModel(mesh, spatial_dimension=_all_dimensions,
/// id="diffusion_model")` (PY-084).
#[pyclass(extends = PyDiffusionModel, frozen, name = "HeatTransferModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyHeatTransferModel;

#[pymethods]
impl PyHeatTransferModel {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "diffusion_model".to_owned()))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let m = new_handle(py, mesh, move |sh, parser| {
            Ok(AnyModel::Heat(Box::new(
                akantu::diffusion::HeatTransferModel::with_parser(sh, dim, &id, parser)?,
            )))
        })?;
        Ok(base(m)
            .add_subclass(PyClhDiffusion)
            .add_subclass(PyDiffusionModel)
            .add_subclass(Self))
    }
    #[pyo3(name = "getTemperature")]
    fn get_temperature<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("temperature"))
    }
}

// --- structural mechanics -----------------------------------------------------------------

/// `StructuralMaterial()` / `StructuralMaterial(other)` (PY-085).
#[pyclass(
    get_all,
    set_all,
    name = "StructuralMaterial",
    module = "akantu.py11_akantu"
)]
#[derive(Clone, Debug, Default)]
#[allow(non_snake_case)] // upstream attribute names (`def_readwrite`)
pub(crate) struct PyStructuralMaterial {
    E: Real,
    A: Real,
    I: Real,
    Iz: Real,
    Iy: Real,
    GJ: Real,
    rho: Real,
    t: Real,
    nu: Real,
}

impl PyStructuralMaterial {
    fn to_rust(&self) -> akantu::structural::StructuralMaterial {
        akantu::structural::StructuralMaterial {
            e: self.E,
            a: self.A,
            i: self.I,
            iz: self.Iz,
            iy: self.Iy,
            gj: self.GJ,
            rho: self.rho,
            t: self.t,
            nu: self.nu,
        }
    }
    fn of(m: &akantu::structural::StructuralMaterial) -> Self {
        Self {
            E: m.e,
            A: m.a,
            I: m.i,
            Iz: m.iz,
            Iy: m.iy,
            GJ: m.gj,
            rho: m.rho,
            t: m.t,
            nu: m.nu,
        }
    }
}

#[pymethods]
impl PyStructuralMaterial {
    #[new]
    #[pyo3(signature = (other = None))]
    fn new(other: Option<PyRef<'_, Self>>) -> Self {
        other.map(|o| o.clone()).unwrap_or_default()
    }
}

/// `StructuralMechanicsModel(mesh, spatial_dimension=_all_dimensions,
/// id="structural_mechanics_model")` (PY-085).
#[pyclass(extends = PyModel, frozen, name = "StructuralMechanicsModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyStructuralMechanicsModel;

fn with_structural<R: Send>(
    slf: &Bound<'_, PyStructuralMechanicsModel>,
    f: impl FnOnce(&mut akantu::structural::StructuralMechanicsModel) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    model_of(slf)?.with_any(slf.py(), move |a, p| f(a.structural(p)?))
}

#[pymethods]
impl PyStructuralMechanicsModel {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "structural_mechanics_model".to_owned()))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let m = new_handle(py, mesh, move |sh, _parser| {
            Ok(AnyModel::Structural(Box::new(
                akantu::structural::StructuralMechanicsModel::new(sh, dim, &id)?,
            )))
        })?;
        Ok(base(m).add_subclass(Self))
    }
    /// `initFull(_analysis_method)` / `initFull()`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        with_structural(slf, move |m| m.init_full(&o))
    }
    /// `getExternalForce()`: a zero-copy writable view (D-047).
    #[pyo3(name = "getExternalForce")]
    fn get_external_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("external_force"))
    }
    /// `getInternalForce()`: a zero-copy writable view (D-047).
    #[pyo3(name = "getInternalForce")]
    fn get_internal_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("internal_force"))
    }
    #[pyo3(name = "getDisplacement")]
    fn get_displacement<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("displacement"))
    }
    #[pyo3(name = "getVelocity")]
    fn get_velocity<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("velocity"))
    }
    #[pyo3(name = "getAcceleration")]
    fn get_acceleration<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("acceleration"))
    }
    #[pyo3(name = "getBlockedDOFs")]
    fn get_blocked_dofs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<bool>(slf.py(), ModelKey::BlockedDofs)
    }
    #[pyo3(name = "getMesh")]
    fn get_mesh<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMesh>> {
        PyMesh::wrap(slf.py(), model_of(slf)?.h.mesh.clone())
    }
    #[pyo3(name = "setTimeStep", signature = (time_step, solver_id = String::new()))]
    fn set_time_step(slf: &Bound<'_, Self>, time_step: Real, solver_id: String) -> PyResult<()> {
        with_structural(slf, move |m| {
            m.set_time_step(
                time_step,
                (!solver_id.is_empty()).then_some(solver_id.as_str()),
            )
        })
    }
    /// `getElementMaterial(type, ghost_type=_not_ghost)`: a zero-copy
    /// writable view (`uint32`, D-047). Writes are seen by the next
    /// assembly (UD-508: every assembly compares the map with its inputs).
    #[pyo3(name = "getElementMaterial", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_element_material<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (t, g) = (r#type.0, ghost_type.single()?);
        model_of(slf)?.view::<u32>(slf.py(), ModelKey::ElementMaterial(t, g))
    }
    #[pyo3(name = "getMaterialByElement")]
    fn get_material_by_element(
        slf: &Bound<'_, Self>,
        element: &Bound<'_, PyElement>,
    ) -> PyResult<PyStructuralMaterial> {
        let el = element.get().element;
        with_structural(slf, move |m| {
            Ok(PyStructuralMaterial::of(m.material_by_element(el)?))
        })
    }
    #[pyo3(name = "addMaterial", signature = (mat, name = String::new()))]
    fn add_material(
        slf: &Bound<'_, Self>,
        mat: PyRef<'_, PyStructuralMaterial>,
        name: String,
    ) -> PyResult<usize> {
        let m = mat.to_rust();
        with_structural(slf, move |s| s.add_material(m, &name))
    }
    /// `getMaterial(index | name)`: a copy.
    #[pyo3(name = "getMaterial")]
    fn get_material(
        slf: &Bound<'_, Self>,
        material_index: &Bound<'_, PyAny>,
    ) -> PyResult<PyStructuralMaterial> {
        if let Ok(n) = material_index.extract::<String>() {
            return with_structural(slf, move |m| {
                Ok(PyStructuralMaterial::of(m.material_by_name(&n)?))
            });
        }
        let i: usize = material_index.extract()?;
        with_structural(slf, move |m| Ok(PyStructuralMaterial::of(m.material(i)?)))
    }
    #[pyo3(name = "getNbMaterials")]
    fn get_nb_materials(slf: &Bound<'_, Self>) -> PyResult<usize> {
        with_structural(slf, |m| Ok(m.nb_materials()))
    }
    #[pyo3(name = "getKineticEnergy")]
    fn get_kinetic_energy(slf: &Bound<'_, Self>) -> PyResult<Real> {
        with_structural(slf, |m| m.kinetic_energy())
    }
    #[pyo3(name = "getPotentialEnergy")]
    fn get_potential_energy(slf: &Bound<'_, Self>) -> PyResult<Real> {
        with_structural(slf, |m| m.potential_energy())
    }
    #[pyo3(name = "getEnergy")]
    fn get_energy(slf: &Bound<'_, Self>, energy_id: String) -> PyResult<Real> {
        with_structural(slf, move |m| m.energy(&energy_id))
    }
    #[pyo3(name = "getLumpedMass")]
    fn get_lumped_mass<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let p = with_structural(slf, |m| Ok(parts(m.lumped_mass()?)))?;
        copy_parts(slf.py(), p)
    }
    #[pyo3(name = "getMass")]
    fn get_mass<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let p = with_structural(slf, |m| Ok(parts(m.mass()?)))?;
        copy_parts(slf.py(), p)
    }
    #[pyo3(name = "assembleLumpedMassMatrix")]
    fn assemble_lumped_mass_matrix(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_structural(slf, |m| m.assemble_lumped_mass_matrix())
    }
    #[pyo3(name = "hasLumpedMass")]
    fn has_lumped_mass(slf: &Bound<'_, Self>) -> PyResult<bool> {
        with_structural(slf, |m| Ok(m.has_lumped_mass()))
    }
}

// --- phase field ----------------------------------------------------------------------------

/// `PhaseField` (PY-092): a phase-field law.
#[pyclass(extends = PyClPhaseField, subclass, frozen, name = "PhaseField", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyPhaseField;

fn pf_internal<'py>(
    slf: &Bound<'py, PyPhaseField>,
    name: &str,
    ty: Option<EtArg>,
    ghost: GhostArg,
) -> PyResult<Bound<'py, PyAny>> {
    let h = slf.as_super().as_super().get();
    if let super::laws::HandlerRef::PhaseFieldLaw(l) = &h.handler
        && let Some(t) = ty
    {
        return l.real_view(slf.py(), name, t.0, ghost.single()?);
    }
    let law = match &h.handler {
        super::laws::HandlerRef::Other(law) => law.clone(),
        super::laws::HandlerRef::PhaseFieldLaw(l) if ty.is_some() => {
            l.other().map_err(|e| super::runtime::to_py(slf.py(), e))?
        }
        super::laws::HandlerRef::PhaseFieldLaw(_) => {
            let h: &Bound<'py, PyConstitutiveLawInternalHandler> = slf.as_super().as_super();
            return h.call_method1("getInternalReal", (name,));
        }
        _ => return Err(PyTypeError::new_err("not a phase-field law")),
    };
    match ty {
        Some(t) => law.model.view::<f64>(
            slf.py(),
            ModelKey::Internal {
                handler: Handler::PhaseField,
                law: law.index,
                name: name.to_owned(),
                ty: t.0,
                ghost: ghost.single()?,
            },
        ),
        None => {
            let h: &Bound<'py, PyConstitutiveLawInternalHandler> = slf.as_super().as_super();
            h.call_method1("getInternalReal", (name,))
        }
    }
}

/// The phase-field link of a Python `PhaseField` subclass object (`None`
/// for a law of a model).
pub(crate) fn pf_link_of(pf: &Bound<'_, PyPhaseField>) -> Option<Arc<super::pf_laws::PfLink>> {
    match &pf.as_super().as_super().get().handler {
        super::laws::HandlerRef::PhaseFieldLaw(l) => Some(Arc::clone(l)),
        _ => None,
    }
}

#[pymethods]
impl PyPhaseField {
    /// `PhaseField(model, id)` (Python subclasses: `super().__init__(model,
    /// id)` inside a `PhaseFieldFactory` allocator, D-078).
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyClassInitializer<Self> {
        let link = Arc::new(super::pf_laws::PfLink::default());
        PyClassInitializer::from(PyParameterRegistry::external(Arc::new(
            super::pf_laws::PfRegistry(Arc::clone(&link)),
        )))
        .add_subclass(super::parser::PyParsable)
        .add_subclass(PyConstitutiveLawInternalHandler {
            handler: super::laws::HandlerRef::PhaseFieldLaw(link),
        })
        .add_subclass(PyClPhaseField)
        .add_subclass(Self)
    }

    /// `PhaseField.__init__(model, id)`: a pending law of `model`.
    #[pyo3(name = "__init__", signature = (model, id, *_args, **_kwargs))]
    fn init(
        slf: &Bound<'_, Self>,
        model: &Bound<'_, PyAny>,
        id: String,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let m = model_of(model.cast::<PyPhaseFieldModel>().map_err(|_| {
            PyTypeError::new_err("PhaseField(model, id): model must be a PhaseFieldModel")
        })?)?;
        if let Some(l) = pf_link_of(slf) {
            l.init_pending(&m, &id);
        }
        Ok(())
    }
    /// `getDamage(el_type, ghost_type=_not_ghost)` (view) or `getDamage()`
    /// (the internal).
    #[pyo3(name = "getDamage", signature = (el_type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_damage<'py>(
        slf: &Bound<'py, Self>,
        el_type: Option<EtArg>,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        pf_internal(slf, "damage", el_type, ghost_type)
    }
    /// `getStrain(el_type, ghost_type=_not_ghost)` / `getStrain()`.
    #[pyo3(name = "getStrain", signature = (el_type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_strain<'py>(
        slf: &Bound<'py, Self>,
        el_type: Option<EtArg>,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        pf_internal(slf, "strain", el_type, ghost_type)
    }
    /// `getEnergy(energy_id)` / `getEnergy(energy_id, element)` (the model's
    /// energy of this law's elements: the port computes energies per model).
    #[pyo3(name = "getEnergy", signature = (energy_id, element = None))]
    fn get_energy(
        slf: &Bound<'_, Self>,
        energy_id: String,
        element: Option<PyRef<'_, PyElement>>,
    ) -> PyResult<Real> {
        let h = slf.as_super().as_super().get();
        let law = match &h.handler {
            super::laws::HandlerRef::Other(law) => law.clone(),
            super::laws::HandlerRef::PhaseFieldLaw(l) => {
                l.other().map_err(|e| super::runtime::to_py(slf.py(), e))?
            }
            _ => return Err(PyTypeError::new_err("not a phase-field law")),
        };
        let el = element.map(|e| e.element);
        law.model.with_any(slf.py(), move |a, p| {
            let m = a.phase_field(p)?;
            match el {
                Some(e) => m.energy_of_element(&energy_id, e),
                None => m.energy(&energy_id),
            }
        })
    }
}

/// `PhaseFieldFactory` (PY-092): Python allocators need a runtime
/// phase-field type (proposal), `registerAllocator` raises.
#[pyclass(frozen, name = "PhaseFieldFactory", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyPhaseFieldFactory;

#[pymethods]
impl PyPhaseFieldFactory {
    #[staticmethod]
    #[pyo3(name = "getInstance")]
    fn get_instance() -> Self {
        Self
    }
    /// `registerAllocator(id, func)`: `func(dim, energy_split, model, id)`
    /// must return a new `PhaseField` object (a Python subclass,
    /// D-078); an id registered already raises, as upstream's factory.
    #[pyo3(name = "registerAllocator")]
    fn register_allocator(&self, py: Python<'_>, id: String, func: Py<PyAny>) -> PyResult<()> {
        super::pf_laws::register_allocator(py, id, func)
    }
}

/// `PhaseFieldModel(mesh, spatial_dimension=_all_dimensions,
/// id="phase_field_model", dof_manager=None,
/// model_type=ModelType.phase_field_model)` (PY-091).
#[pyclass(extends = PyClhPhaseField, frozen, name = "PhaseFieldModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyPhaseFieldModel;

fn with_pf<R: Send>(
    slf: &Bound<'_, PyPhaseFieldModel>,
    f: impl FnOnce(&mut akantu::phase_field::PhaseFieldModel) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    model_of(slf)?.with_any(slf.py(), move |a, p| f(a.phase_field(p)?))
}

#[pymethods]
impl PyPhaseFieldModel {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "phase_field_model".to_owned(), dof_manager = None, model_type = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        dof_manager: Option<&Bound<'_, PyAny>>,
        model_type: Option<&Bound<'_, ModelType>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        super::solver::refuse_user_dof_manager(dof_manager)?;
        let _ = model_type;
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let m = new_handle(py, mesh, move |sh, parser| {
            Ok(AnyModel::PhaseField(Box::new(
                akantu::phase_field::PhaseFieldModel::with_parser(sh, dim, &id, parser)?,
            )))
        })?;
        Ok(base(m).add_subclass(PyClhPhaseField).add_subclass(Self))
    }
    /// `initFull(options)` / `initFull(_analysis_method)`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        let m = model_of(slf)?;
        let h = Arc::clone(&m.h);
        m.with_any(slf.py(), move |a, p| {
            let pfm = a.phase_field(p)?;
            // allocators registered after the model was created
            super::pf_laws::install_python_phasefields(pfm, &h, p);
            pfm.init_full(&o)
        })
    }
    #[pyo3(name = "applyDirichletBC")]
    fn apply_dirichlet_bc_deprecated(slf: &Bound<'_, Self>) -> PyResult<()> {
        PyErr::warn(
            slf.py(),
            &slf.py()
                .get_type::<pyo3::exceptions::PyDeprecationWarning>(),
            c"applyDirichletBC() is deprecated, use applyBC instead",
            1,
        )
    }
    #[pyo3(name = "applyBC")]
    fn apply_bc(
        slf: &Bound<'_, Self>,
        func: &Bound<'_, PyAny>,
        element_group: String,
    ) -> PyResult<()> {
        super::bc::apply_bc(slf.py(), &model_of(slf)?, func, element_group)
    }
    #[pyo3(name = "setTimeStep", signature = (time_step, solver_id = String::new()))]
    fn set_time_step(slf: &Bound<'_, Self>, time_step: Real, solver_id: String) -> PyResult<()> {
        with_pf(slf, move |m| {
            m.set_time_step(
                time_step,
                (!solver_id.is_empty()).then_some(solver_id.as_str()),
            )
        })
    }
    /// `getEnergy(energy_id="dissipated")` or `getEnergy(group_id)`
    /// (a group name: the dissipated energy of the group).
    #[pyo3(name = "getEnergy", signature = (energy_id = "dissipated".to_owned()))]
    fn get_energy(slf: &Bound<'_, Self>, energy_id: String) -> PyResult<Real> {
        with_pf(slf, move |m| match m.energy(&energy_id) {
            Ok(e) => Ok(e),
            Err(_) => m.energy_of_group("dissipated", &energy_id),
        })
    }
    #[pyo3(name = "assembleStiffnessMatrix")]
    fn assemble_stiffness_matrix(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_pf(slf, |m| m.assemble_stiffness_matrix())
    }
    #[pyo3(name = "assembleInternalForces")]
    fn assemble_internal_forces(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_pf(slf, |m| m.assemble_internal_forces())
    }
    #[pyo3(name = "savePreviousState")]
    fn save_previous_state(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_pf(slf, |m| m.save_previous_state())
    }
    #[pyo3(name = "getDamage")]
    fn get_damage<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("damage"))
    }
    /// `getInternalForce()`: a zero-copy writable view (D-047, D-078),
    /// as upstream's `def_function_nocopy`.
    #[pyo3(name = "getInternalForce")]
    fn get_internal_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal("internal_force"))
    }
    #[pyo3(name = "getBlockedDOFs")]
    fn get_blocked_dofs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        model_of(slf)?.view::<bool>(slf.py(), ModelKey::BlockedDofs)
    }
    #[pyo3(name = "getPhaseField")]
    fn get_phase_field<'py>(
        slf: &Bound<'py, Self>,
        phase_field_id: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        law_object(
            slf.py(),
            &model_of(slf)?,
            Handler::PhaseField,
            phase_field_id,
        )
    }
    /// `setUpstreamCompat(upstream=True)`: turn on every upstream-compatibility
    /// switch of this phase-field model, so that a script reproduces the
    /// reference: UD-377, UD-378, UD-379, UD-380, UD-385, UD-387, UD-388
    /// (`PhaseFieldUpstreamCompat::upstream()`).
    ///
    /// `False` restores the corrected defaults. Not in upstream's module (port
    /// extension, D-003 category B / D-078); `getUpstreamCompat()` reads it
    /// back. The fine-grained switches stay in Rust.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        set_upstream_compat(slf.py(), &model_of(slf)?, upstream)
    }
    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch that `setUpstreamCompat(True)` turns on is on, `False`
    /// otherwise (the corrected defaults after construction or
    /// `setUpstreamCompat(False)`).
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        get_upstream_compat(slf.py(), &model_of(slf)?)
    }
    #[pyo3(name = "getPhaseFieldIndex")]
    fn get_phase_field_index(slf: &Bound<'_, Self>, name: String) -> PyResult<usize> {
        with_pf(slf, move |m| m.phase_field_index(&name))
    }
    #[pyo3(name = "setPhaseFieldSelector")]
    fn set_phase_field_selector(
        slf: &Bound<'_, Self>,
        selector: &Bound<'_, PyConstitutiveLawSelector>,
    ) -> PyResult<()> {
        let chain = super::laws::selector_chain(selector)?;
        model_of(slf)?.with_ops(slf.py(), move |o| {
            o.set_handler_selector(Handler::PhaseField, chain)
        })
    }
}

/// `getUpstreamCompat()`: whether every switch `setUpstreamCompat(True)`
/// turns on (see [`set_upstream_compat`]) is on for the model and its
/// parts.
fn get_upstream_compat(py: Python<'_>, model: &ModelRef) -> PyResult<bool> {
    use akantu::contact::ContactUpstreamCompat as Contact;
    use akantu::couplers::CouplerSolidPhaseFieldCompat as PfCoupler;
    use akantu::couplers::SolidContactCompat as Coupler;
    use akantu::phase_field::PhaseFieldUpstreamCompat as Phase;
    let all_contact = Contact::all();
    model.with_any(py, move |a, part| {
        Ok(match (a, part) {
            (AnyModel::SolidContact(c), Part::Root) => {
                c.upstream_compat() == Coupler::upstream()
                    && c.contact.upstream_compat() == all_contact
            }
            (AnyModel::SolidCohesiveContact(c), Part::Root) => {
                c.upstream_compat() == Coupler::all() && c.contact.upstream_compat() == all_contact
            }
            (AnyModel::SolidPhaseField(c), Part::Root) => {
                c.compat == PfCoupler::upstream() && c.phase.upstream_compat() == Phase::upstream()
            }
            (AnyModel::Contact(c), Part::Root) => c.upstream_compat() == all_contact,
            (
                a @ (AnyModel::SolidContact(_) | AnyModel::SolidCohesiveContact(_)),
                Part::Contact,
            ) => a.contact(Part::Contact)?.upstream_compat() == all_contact,
            (a, part) => a.phase_field(part)?.upstream_compat() == Phase::upstream(),
        })
    })
}

/// Port extension `setUpstreamCompat(upstream)` (not in upstream's module;
/// D-003 category B): `True` turns on every upstream-compatibility switch
/// of the model and of its parts, so that a script reproduces the reference
/// (contact: `ContactUpstreamCompat::all()`, UD-002/008/363…365/368/369/
/// 371/372; solid–contact couplers: UD-374, UD-1291, and UD-772 for the
/// cohesive one; phase field: `PhaseFieldUpstreamCompat::upstream()`, UD-377…380,
/// UD-385, UD-387 B, UD-388; solid–phase-field coupler: also UD-382/619/620);
/// `False` restores the corrected defaults.
fn set_upstream_compat(py: Python<'_>, model: &ModelRef, upstream: bool) -> PyResult<()> {
    use akantu::contact::ContactUpstreamCompat as Contact;
    use akantu::couplers::SolidContactCompat as Coupler;
    use akantu::phase_field::PhaseFieldUpstreamCompat as Phase;
    let contact = if upstream {
        Contact::all()
    } else {
        Contact::default()
    };
    model.with_any(py, move |a, part| match (a, part) {
        (AnyModel::SolidContact(c), Part::Root) => {
            c.set_upstream_compat(if upstream {
                Coupler::upstream()
            } else {
                Coupler::default()
            })?;
            c.contact.set_upstream_compat(contact);
            Ok(())
        }
        (AnyModel::SolidCohesiveContact(c), Part::Root) => {
            c.set_upstream_compat(if upstream {
                Coupler::all()
            } else {
                Coupler::default()
            });
            c.contact.set_upstream_compat(contact);
            Ok(())
        }
        (AnyModel::SolidPhaseField(c), Part::Root) => {
            c.set_upstream_compat(upstream);
            Ok(())
        }
        (AnyModel::Contact(c), Part::Root) => {
            c.set_upstream_compat(contact);
            Ok(())
        }
        (a @ (AnyModel::SolidContact(_) | AnyModel::SolidCohesiveContact(_)), Part::Contact) => {
            a.contact(Part::Contact)?.set_upstream_compat(contact);
            Ok(())
        }
        (a, part) => {
            a.phase_field(part)?.set_upstream_compat(if upstream {
                Phase::upstream()
            } else {
                Phase::default()
            });
            Ok(())
        }
    })
}

/// `CouplerSolidPhaseField(mesh, spatial_dimension=_all_dimensions,
/// id="coupler_solid_phasefield", model_type=…)` (PY-093).
#[pyclass(extends = PyModel, frozen, name = "CouplerSolidPhaseField", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyCouplerSolidPhaseField;

fn with_spf<R: Send>(
    slf: &Bound<'_, PyCouplerSolidPhaseField>,
    f: impl FnOnce(&mut akantu::couplers::CouplerSolidPhaseField) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    model_of(slf)?.with_any(slf.py(), move |a, _| f(a.solid_phase_field()?))
}

#[pymethods]
impl PyCouplerSolidPhaseField {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "coupler_solid_phasefield".to_owned(), model_type = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        model_type: Option<&Bound<'_, ModelType>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let _ = model_type;
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let m = new_handle(py, mesh, move |sh, parser| {
            Ok(AnyModel::SolidPhaseField(Box::new(
                akantu::couplers::CouplerSolidPhaseField::with_parser(sh, dim, &id, parser)?,
            )))
        })?;
        let h = Arc::clone(&m.h);
        native(py, move || {
            h.with(|a| {
                super::laws::install_python_materials(a.solid(Part::Solid)?, &h, Part::Solid);
                Ok(())
            })
        })?;
        Ok(base(m).add_subclass(Self))
    }
    /// `initFull(_analysis_method)` (the coupler's default solvers).
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        let m = model_of(slf)?;
        let h = Arc::clone(&m.h);
        m.with_any(slf.py(), move |a, _| {
            super::pf_laws::install_python_phasefields(
                a.phase_field(Part::PhaseField)?,
                &h,
                Part::PhaseField,
            );
            a.solid_phase_field()?.init_full(&o)
        })
    }
    /// `setUpstreamCompat(upstream=True)`: turn on every upstream-compatibility
    /// switch of the coupler (UD-382, UD-619, UD-620) and of its phase-field
    /// model (UD-377, UD-378, UD-379, UD-380, UD-385, UD-387, UD-388), so that
    /// a script reproduces the reference.
    ///
    /// `False` restores the corrected defaults. Not in upstream's module (port
    /// extension, D-003 category B / D-078); `getUpstreamCompat()` reads it
    /// back. The fine-grained switches stay in Rust.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        set_upstream_compat(slf.py(), &model_of(slf)?, upstream)
    }
    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch that `setUpstreamCompat(True)` turns on is on, `False`
    /// otherwise (the corrected defaults after construction or
    /// `setUpstreamCompat(False)`).
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        get_upstream_compat(slf.py(), &model_of(slf)?)
    }
    /// `solve(solid_solver_id, phase_solver_id)`.
    #[pyo3(signature = (solid_solver_id = String::new(), phase_solver_id = String::new()))]
    fn solve(
        slf: &Bound<'_, Self>,
        solid_solver_id: String,
        phase_solver_id: String,
    ) -> PyResult<()> {
        with_spf(slf, move |c| {
            c.solve(
                (!solid_solver_id.is_empty()).then_some(solid_solver_id.as_str()),
                (!phase_solver_id.is_empty()).then_some(phase_solver_id.as_str()),
            )
        })
    }
    #[pyo3(name = "getSolidMechanicsModel")]
    fn get_solid_mechanics_model<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        super::handle::model_object(slf.py(), &model_of(slf)?.with_part(Part::Solid))
    }
    #[pyo3(name = "getPhaseFieldModel")]
    fn get_phase_field_model<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        super::handle::model_object(slf.py(), &model_of(slf)?.with_part(Part::PhaseField))
    }
    #[pyo3(name = "computeStrainOnQuadPoints", signature = (ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn compute_strain_on_quad_points(slf: &Bound<'_, Self>, ghost_type: GhostArg) -> PyResult<()> {
        let g = ghost_type.single()?;
        with_spf(slf, move |c| c.compute_strain_on_quad_points(g))
    }
    #[pyo3(name = "computeDamageOnQuadPoints", signature = (ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn compute_damage_on_quad_points(slf: &Bound<'_, Self>, ghost_type: GhostArg) -> PyResult<()> {
        let g = ghost_type.single()?;
        with_spf(slf, move |c| c.compute_damage_on_quad_points(g))
    }
}

/// `PhaseFieldElementFilter(model, max_damage)` (PY-093): a clustering
/// filter (the port's `createClusters` uses the default filter only).
#[pyclass(extends = PyClusteringFilter, frozen, name = "PhaseFieldElementFilter", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyPhaseFieldElementFilter {
    #[allow(dead_code)] // kept as upstream's filter state
    max_damage: Real,
}

#[pymethods]
impl PyPhaseFieldElementFilter {
    #[new]
    fn new(model: &Bound<'_, PyPhaseFieldModel>, max_damage: Real) -> PyClassInitializer<Self> {
        let _ = model;
        PyClassInitializer::from(PyClusteringFilter).add_subclass(Self { max_damage })
    }
}

// --- GeometryUtils ---------------------------------------------------------------------------------

/// `GeometryUtils` (PY-088): static helpers of the contact geometry
/// (`akantu::contact::geometry`). Nodal coordinates are taken from
/// `positions` (an array of the mesh's nodes) at the element's nodes.
/// `normal(…, outward=True)` orients with the barycentres of the given
/// `positions` (UD-371 correction; upstream uses the mesh nodes, the same
/// when `positions` are the mesh nodes); `contravariantBasis` is
/// `T (TᵀT)⁻¹` (UD-366; identical to upstream's for a 2-D basis).
#[pyclass(frozen, name = "GeometryUtils", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyGeometryUtils;

/// Run `f` on the contact view of `mesh` (a mesh, or its facet mesh with the
/// mesh as parent) and the selected mesh (native section).
fn with_geometry_view<R: Send>(
    py: Python<'_>,
    mesh: &Bound<'_, PyMesh>,
    f: impl FnOnce(&akantu::contact::MeshView<'_>, &akantu::mesh::Mesh) -> akantu::core::Result<R>
    + Send,
) -> PyResult<R> {
    let m = PyMesh::mesh_ref(mesh);
    native(py, move || {
        let g = m.h.shared.read()?;
        match m.part {
            akantu::mesh::MeshPart::Main => f(&akantu::contact::MeshView::new(&g), &g),
            akantu::mesh::MeshPart::Facets => {
                let facets = g.mesh_facets().ok_or_else(|| {
                    Error::not_found("facet mesh", "No facet mesh is defined yet!")
                })?;
                f(&akantu::contact::MeshView::facets(facets, &g), facets)
            }
        }
    })
}

/// A float vector argument.
fn vector_arg(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Real>> {
    Ok(super::arrays::input_f64(obj)?.0)
}

/// Column-major `rows × cols` values as a `(rows, cols)` NumPy array.
fn matrix_out(
    py: Python<'_>,
    values: Vec<Real>,
    rows: usize,
    cols: usize,
) -> PyResult<Bound<'_, PyAny>> {
    // column-major storage = the row-major layout of the transpose
    let t = owned_numpy(py, values, cols, rows, Layout::Rows)?;
    t.call_method0("transpose")?.call_method0("copy")
}

#[pymethods]
impl PyGeometryUtils {
    /// `normal(mesh, positions, element, outward=True)`.
    #[staticmethod]
    #[pyo3(signature = (mesh, positions, element, outward = true))]
    fn normal<'py>(
        py: Python<'py>,
        mesh: &Bound<'py, PyMesh>,
        positions: &Bound<'py, PyAny>,
        element: &Bound<'py, PyElement>,
        outward: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (values, shape) = super::arrays::input_f64(positions)?;
        let el = element.get().element;
        let n = with_geometry_view(py, mesh, move |view, m| {
            let dim = view.dim();
            let pos = Array::from_vec(values, shape.get(1).copied().unwrap_or(dim))?;
            let coords = m.extract_nodal_values_from_element(&pos, el)?;
            let frame = outward.then_some(akantu::contact::geometry::OutwardFrame::Current(&pos));
            let n = akantu::contact::geometry::normal_from_coords(view, &coords, el, frame)?;
            Ok(n[..dim].to_vec())
        })?;
        let len = n.len();
        owned_numpy(py, n, len, 1, Layout::Rows)?.call_method0("ravel")
    }

    /// `covariantBasis(mesh, positions, element, normal,
    /// natural_projection)` → `(dim, natural_dim)` matrix.
    #[staticmethod]
    #[pyo3(name = "covariantBasis", signature = (mesh, positions, element, normal, natural_projection))]
    fn covariant_basis<'py>(
        py: Python<'py>,
        mesh: &Bound<'py, PyMesh>,
        positions: &Bound<'py, PyAny>,
        element: &Bound<'py, PyElement>,
        normal: &Bound<'py, PyAny>,
        natural_projection: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (values, shape) = super::arrays::input_f64(positions)?;
        let (normal, xi) = (vector_arg(normal)?, vector_arg(natural_projection)?);
        let el = element.get().element;
        let (out, dim, sd) = with_geometry_view(py, mesh, move |view, m| {
            let dim = view.dim();
            let pos = Array::from_vec(values, shape.get(1).copied().unwrap_or(dim))?;
            let coords = m.extract_nodal_values_from_element(&pos, el)?;
            let sd = el.ty().natural_space_dimension();
            let mut out = vec![0.0; dim * sd];
            akantu::contact::geometry::covariant_basis_with_normal(
                &coords,
                dim,
                el.ty(),
                &normal,
                &xi,
                &mut out,
            )?;
            Ok((out, dim, sd))
        })?;
        matrix_out(py, out, dim, sd)
    }

    /// `curvature(mesh, positions, element, natural_coord)` → `(dim,
    /// natural_dim²)` matrix.
    #[staticmethod]
    fn curvature<'py>(
        py: Python<'py>,
        mesh: &Bound<'py, PyMesh>,
        positions: &Bound<'py, PyAny>,
        element: &Bound<'py, PyElement>,
        natural_coord: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (values, shape) = super::arrays::input_f64(positions)?;
        let xi = vector_arg(natural_coord)?;
        let el = element.get().element;
        let (out, dim, cols) = with_geometry_view(py, mesh, move |view, m| {
            let dim = view.dim();
            let pos = Array::from_vec(values, shape.get(1).copied().unwrap_or(dim))?;
            let coords = m.extract_nodal_values_from_element(&pos, el)?;
            let sd = el.ty().natural_space_dimension();
            let mut out = vec![0.0; dim * sd * sd];
            akantu::contact::geometry::curvature(&coords, dim, el.ty(), &xi, &mut out)?;
            Ok((out, dim, sd * sd))
        })?;
        matrix_out(py, out, dim, cols)
    }

    /// `contravariantBasis(covariant_basis)` (a `(dim, sd)` matrix or a
    /// vector, i.e. one column) → `(dim, sd)` matrix.
    #[staticmethod]
    #[pyo3(name = "contravariantBasis", signature = (covariant_basis))]
    fn contravariant_basis<'py>(
        py: Python<'py>,
        covariant_basis: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let np = py.import("numpy")?;
        let a = np.call_method1("asarray", (covariant_basis, np.getattr("float64")?))?;
        let shape: Vec<usize> = a.getattr("shape")?.extract()?;
        let (dim, sd) = match shape.as_slice() {
            [d] => (*d, 1),
            [d, s] => (*d, *s),
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "contravariantBasis: a vector or a matrix",
                ));
            }
        };
        // column-major values
        let t: Vec<Real> = a
            .call_method0("transpose")?
            .call_method0("ravel")?
            .call_method0("tolist")?
            .extract()?;
        let mut out = vec![0.0; dim * sd];
        akantu::contact::geometry::contravariant_basis(&t, dim, sd, &mut out)
            .map_err(|e| super::runtime::to_py(py, e))?;
        // upstream returns a Matrix (a vector input is one column)
        matrix_out(py, out, dim, sd)
    }

    /// `realProjection(mesh, positions, slave, element, normal)`: the
    /// projection of `slave` on the plane of the element's first node with
    /// normal `normal`.
    #[staticmethod]
    #[pyo3(name = "realProjection", signature = (mesh, positions, slave, element, normal))]
    fn real_projection<'py>(
        py: Python<'py>,
        mesh: &Bound<'py, PyMesh>,
        positions: &Bound<'py, PyAny>,
        slave: &Bound<'py, PyAny>,
        element: &Bound<'py, PyElement>,
        normal: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (values, shape) = super::arrays::input_f64(positions)?;
        let (slave, normal) = (vector_arg(slave)?, vector_arg(normal)?);
        let el = element.get().element;
        let out = with_geometry_view(py, mesh, move |view, m| {
            let dim = view.dim();
            if slave.len() != dim || normal.len() != dim {
                return Err(Error::invalid(format!(
                    "realProjection: slave and normal need {dim} components"
                )));
            }
            let pos = Array::from_vec(values, shape.get(1).copied().unwrap_or(dim))?;
            let coords = m.extract_nodal_values_from_element(&pos, el)?;
            let mut out = vec![0.0; dim];
            akantu::contact::geometry::real_projection_on_plane(&coords, &slave, &normal, &mut out);
            Ok(out)
        })?;
        let len = out.len();
        owned_numpy(py, out, len, 1, Layout::Rows)?.call_method0("ravel")
    }

    /// `isBoundaryElement(mesh, element)`.
    #[staticmethod]
    #[pyo3(name = "isBoundaryElement")]
    fn is_boundary_element(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        element: &Bound<'_, PyElement>,
    ) -> PyResult<bool> {
        let el = element.get().element;
        with_geometry_view(py, mesh, move |view, _| {
            akantu::contact::geometry::is_boundary_element(view, el)
        })
    }
}

// --- contact -------------------------------------------------------------------------------------

/// Surface selector kinds.
#[derive(Clone, Copy, Debug)]
enum SurfaceKind {
    Base,
    Physical,
    Cohesive,
    All,
}

/// `SurfaceSelector(mesh)` and subclasses (PY-086).
#[pyclass(
    subclass,
    frozen,
    name = "SurfaceSelector",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PySurfaceSelector {
    kind: SurfaceKind,
}

#[pymethods]
impl PySurfaceSelector {
    #[new]
    fn new(mesh: &Bound<'_, PyMesh>) -> Self {
        let _ = mesh;
        Self {
            kind: SurfaceKind::Base,
        }
    }
}

macro_rules! surface_class {
    ($rust:ident, $name:literal, $kind:expr) => {
        #[doc = concat!("`", $name, "(mesh)` (PY-086).")]
        #[pyclass(extends = PySurfaceSelector, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;

        #[pymethods]
        impl $rust {
            #[new]
            fn new(mesh: &Bound<'_, PyMesh>) -> PyClassInitializer<Self> {
                let _ = mesh;
                PyClassInitializer::from(PySurfaceSelector { kind: $kind }).add_subclass(Self)
            }
        }
    };
}

surface_class!(
    PyPhysicalSurfaceSelector,
    "PhysicalSurfaceSelector",
    SurfaceKind::Physical
);
surface_class!(
    PyCohesiveSurfaceSelector,
    "CohesiveSurfaceSelector",
    SurfaceKind::Cohesive
);
surface_class!(PyAllSurfaceSelector, "AllSurfaceSelector", SurfaceKind::All);

/// The surface selector of `kind` on `mesh` with `parser` (upstream's
/// selector constructors read the first `contact_detector` section).
fn build_surface_selector(
    kind: SurfaceKind,
    mesh: &akantu::mesh::Mesh,
    parser: &akantu::parser::Parser,
) -> akantu::core::Result<Box<dyn akantu::contact::selector::SurfaceSelector>> {
    Ok(match kind {
        SurfaceKind::Physical => Box::new(akantu::contact::selector::PhysicalSurfaceSelector::new(
            mesh, parser,
        )?),
        SurfaceKind::Cohesive => {
            Box::new(akantu::contact::selector::CohesiveSurfaceSelector::new())
        }
        SurfaceKind::All => {
            let fm = mesh
                .mesh_facets()
                .ok_or_else(|| Error::invalid("AllSurfaceSelector needs the facet mesh"))?;
            Box::new(akantu::contact::selector::AllSurfaceSelector::new(
                fm, parser,
            )?)
        }
        SurfaceKind::Base => {
            return Err(Error::invalid("the base SurfaceSelector selects nothing"));
        }
    })
}

/// A stand-alone `ContactDetector(mesh, id)` (PY-086): the
/// detector built from the mesh's node coordinates and the first
/// `contact_detector` section of the static parser, and the Python mesh it
/// was built on (upstream keeps a reference to the mesh).
#[derive(Debug)]
pub(crate) struct StandaloneDetector {
    mesh: super::mesh::MeshRef,
    detector: std::sync::Mutex<akantu::contact::ContactDetector>,
}

/// What a Python `ContactDetector` designates.
#[derive(Debug)]
enum DetectorOf {
    /// The detector of a contact model (`getContactDetector()`).
    Model(ModelRef),
    /// A stand-alone detector.
    Standalone(Arc<StandaloneDetector>),
}

/// `ContactDetector` (PY-086): a contact model's detector, or a
/// stand-alone one.
#[pyclass(frozen, name = "ContactDetector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyContactDetector {
    of: DetectorOf,
}

impl PyContactDetector {
    /// The detector of the contact model `model`.
    fn of_model(model: ModelRef) -> Self {
        Self {
            of: DetectorOf::Model(model),
        }
    }

    /// The stand-alone detector, if this is one (tests, port extension).
    pub(crate) fn standalone(&self) -> Option<&Arc<StandaloneDetector>> {
        match &self.of {
            DetectorOf::Standalone(d) => Some(d),
            DetectorOf::Model(_) => None,
        }
    }
}

#[pymethods]
impl PyContactDetector {
    /// Upstream's two constructors (`py_contact_mechanics_model.cc:79-84`):
    ///
    /// * `ContactDetector(mesh, id="contact_detector")`: the detector of
    ///   the mesh's node coordinates, parameters from the first
    ///   `contact_detector` section of the static parser
    ///   (`contact_detector.cc:28-50`). Without such a section the port
    ///   raises `akantu.Exception` (upstream dereferences the end iterator
    ///   of the empty section range: undefined behaviour that happens to
    ///   raise "parameter type has not been found", UD-1171).
    /// * `ContactDetector(mesh, positions, id="contact_detector")`:
    ///   **refused** with `akantu.Exception` (UD-1170, category A): upstream
    ///   takes the NumPy array by value as an `Array<Real>` that shares the
    ///   array's buffer and frees it twice (abort, oracle probe).
    #[new]
    #[pyo3(signature = (mesh, *args, **kwargs))]
    fn new(
        mesh: &Bound<'_, PyMesh>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Self> {
        let py = mesh.py();
        let positions_form = kwargs.is_some_and(|k| k.contains("positions").unwrap_or(false))
            || args
                .get_item(0)
                .is_ok_and(|a| !a.is_instance_of::<PyString>());
        if positions_form {
            return Err(super::errors::exception(
                py,
                "ContactDetector(mesh, positions, id) is refused (UD-1170): upstream's binding \
                 frees the positions array twice (abort); use ContactDetector(mesh, id) or the \
                 contact model's getContactDetector()"
                    .into(),
            ));
        }
        if args.len() > 1 {
            return Err(PyTypeError::new_err(
                "ContactDetector(mesh, id=\"contact_detector\"): too many arguments",
            ));
        }
        let id: String = match (
            args.get_item(0).ok(),
            kwargs.and_then(|k| k.get_item("id").ok().flatten()),
        ) {
            (Some(_), Some(_)) => {
                return Err(PyTypeError::new_err(
                    "ContactDetector(): got multiple values for argument 'id'",
                ));
            }
            (Some(a), None) | (None, Some(a)) => a.extract()?,
            (None, None) => "contact_detector".to_owned(),
        };
        if let Some(k) = kwargs
            && let Some((key, _)) = k
                .iter()
                .find(|(key, _)| key.extract::<String>().map_or(true, |s| s != "id"))
        {
            return Err(PyTypeError::new_err(format!(
                "ContactDetector(): unexpected keyword argument {key}"
            )));
        }
        let mesh_ref = PyMesh::mesh_ref(mesh);
        let detector = mesh_ref.read(py, move |m| {
            let parser = akantu::runtime::global()
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .parser
                .clone();
            let nodes = m.nodes();
            akantu::contact::ContactDetector::with_positions(
                m.spatial_dimension(),
                &nodes,
                &parser,
                id,
            )
            .map_err(|e| match e {
                Error::NotFound { .. } => Error::invalid(format!(
                    "ContactDetector(mesh, id): the static parser has no contact_detector \
                     section ({e}; UD-1171)"
                )),
                other => other,
            })
        })?;
        Ok(Self {
            of: DetectorOf::Standalone(Arc::new(StandaloneDetector {
                mesh: mesh_ref,
                detector: std::sync::Mutex::new(detector),
            })),
        })
    }

    /// `setSurfaceSelector(selector)`. The selector's surface becomes the
    /// mesh group `contact_surface` (upstream's selector constructors
    /// create it): written by the contact model alone
    /// on its mesh, through the coupler for a coupler's contact model; for
    /// a stand-alone detector on a mesh no model uses yet (with a model on
    /// the mesh the group is not written: the mesh is the model's).
    #[pyo3(name = "setSurfaceSelector")]
    fn set_surface_selector(
        &self,
        py: Python<'_>,
        selector: &Bound<'_, PySurfaceSelector>,
    ) -> PyResult<()> {
        let kind = selector.get().kind;
        let model = match &self.of {
            DetectorOf::Model(m) => m.clone(),
            DetectorOf::Standalone(d) => {
                let d = Arc::clone(d);
                return native(py, move || d.set_surface_selector(kind));
            }
        };
        model.with_any(py, move |a, p| {
            let sel: Box<dyn akantu::contact::selector::SurfaceSelector> = {
                let c = a.contact(p)?;
                let mesh_h = c.base.mesh.clone();
                let mesh = mesh_h.read()?;
                let parser = c.base.parser.clone();
                build_surface_selector(kind, &mesh, &parser)?
            };
            match a {
                AnyModel::SolidContact(m) => m.set_surface_selector(sel),
                AnyModel::SolidCohesiveContact(m) => m.set_surface_selector(sel),
                other => other.contact(p)?.set_surface_selector(sel),
            }
        })
    }
}

impl StandaloneDetector {
    /// `setSurfaceSelector` of a stand-alone detector (native section).
    fn set_surface_selector(&self, kind: SurfaceKind) -> akantu::core::Result<()> {
        let h = &self.mesh.h;
        let parser = akantu::runtime::global()
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .parser
            .clone();
        let (sel, dim) = {
            let mesh = h.shared.read()?;
            (
                build_surface_selector(kind, &mesh, &parser)?,
                mesh.spatial_dimension(),
            )
        };
        if h.shared.consumer_names()?.is_empty() {
            let part = sel.part();
            let surface = sel.contact_surface();
            h.shared.mutate(
                &mut [],
                |tx| {
                    akantu::contact::selector::write_contact_surface_group(
                        tx.groups_mut(part)?,
                        surface,
                        dim.saturating_sub(1),
                    )
                },
                None,
            )?;
            h.refresh_exports()?;
        }
        self.detector
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_surface_selector(sel);
        Ok(())
    }

    /// Detection parameters `(type, projection_tolerance, max_iterations,
    /// extension_tolerance)` and the number of positions (tests).
    pub(crate) fn summary(&self) -> (String, f64, i64, f64, usize, bool) {
        let d = self.detector.lock().unwrap_or_else(PoisonError::into_inner);
        (
            format!("{:?}", d.detection_type()),
            d.projection_tolerance(),
            d.max_iterations(),
            d.extension_tolerance(),
            d.positions().len(),
            d.surface_selector().is_some(),
        )
    }
}

/// `ContactElement()` (PY-087).
#[pyclass(name = "ContactElement", module = "akantu.py11_akantu")]
#[derive(Clone, Debug)]
pub(crate) struct PyContactElement {
    master: PyElement,
    #[pyo3(get, set)]
    slave: i64,
}

impl Default for PyContactElement {
    fn default() -> Self {
        Self {
            master: PyElement {
                element: akantu::core::Element::NULL,
            },
            slave: -1,
        }
    }
}

#[pymethods]
impl PyContactElement {
    #[new]
    fn new() -> Self {
        Self::default()
    }
    #[getter]
    fn master(&self) -> PyElement {
        self.master
    }
    #[setter]
    fn set_master(&mut self, master: &Bound<'_, PyElement>) {
        self.master = *master.get();
    }
    fn __repr__(&self) -> String {
        format!("{{master: {}, slave: {}}}", self.master.element, self.slave)
    }
    fn __eq__(&self, other: &Self) -> bool {
        self.master.element == other.master.element && self.slave == other.slave
    }
}

/// `ContactElementsView` (PY-087): a snapshot of the contact elements.
#[pyclass(frozen, name = "ContactElementsView", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyContactElementsView {
    elements: Vec<PyContactElement>,
}

#[pymethods]
impl PyContactElementsView {
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let l = pyo3::types::PyList::new(py, self.elements.clone())?;
        l.call_method0("__iter__")
    }
    fn __size__(&self) -> usize {
        self.elements.len()
    }
    fn __contains__(&self, element: &PyContactElement) -> bool {
        self.elements.iter().any(|e| e.__eq__(element))
    }
    fn __getitem__(&self, i: usize) -> PyResult<PyContactElement> {
        self.elements.get(i).cloned().ok_or_else(|| {
            pyo3::exceptions::PyIndexError::new_err("contact element index out of range")
        })
    }
}

/// `ContactMechanicsModel(mesh, spatial_dimension=_all_dimensions,
/// id="contact_mechanics_model", dof_manager=None, model_type=…)` (PY-087).
#[pyclass(extends = PyModel, frozen, name = "ContactMechanicsModel", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyContactMechanicsModel;

fn with_contact<R: Send>(
    slf: &Bound<'_, PyContactMechanicsModel>,
    f: impl FnOnce(&mut akantu::contact::ContactMechanicsModel) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    model_of(slf)?.with_any(slf.py(), move |a, p| f(a.contact(p)?))
}

/// A contact nodal field upstream returns by reference: a zero-copy
/// writable view (D-047, D-078). The per-search fields are
/// refilled in place by every search, so a view taken once follows them
/// (as upstream's reference); a resize to another node count (cohesive
/// insertion) leaves it stale but safe (UD-085).
fn contact_field<'py>(
    slf: &Bound<'py, PyContactMechanicsModel>,
    name: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    model_of(slf)?.view::<f64>(slf.py(), ModelKey::Nodal(name))
}

#[pymethods]
impl PyContactMechanicsModel {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "contact_mechanics_model".to_owned(), dof_manager = None, model_type = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        dof_manager: Option<&Bound<'_, PyAny>>,
        model_type: Option<&Bound<'_, ModelType>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        super::solver::refuse_user_dof_manager(dof_manager)?;
        let _ = model_type;
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let m = new_handle(py, mesh, move |sh, parser| {
            Ok(AnyModel::Contact(Box::new(
                akantu::contact::ContactMechanicsModel::with_parser(sh, dim, &id, parser)?,
            )))
        })?;
        Ok(base(m).add_subclass(Self))
    }
    /// `initFull(options)` / `initFull(_analysis_method)`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        with_contact(slf, move |c| c.init_full(o.analysis_method))
    }
    fn search(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_contact(slf, |c| c.search())
    }
    /// `setUpstreamCompat(upstream=True)`: turn on every upstream-compatibility
    /// switch of this contact model, so that a script reproduces the reference:
    /// UD-002, UD-008, UD-363, UD-364, UD-365, UD-368, UD-369, UD-371, UD-372
    /// (`ContactUpstreamCompat::all()`).
    ///
    /// `False` restores the corrected defaults. Not in upstream's module (port
    /// extension, D-003 category B / D-078); `getUpstreamCompat()` reads it
    /// back. The fine-grained switches stay in Rust.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        set_upstream_compat(slf.py(), &model_of(slf)?, upstream)
    }
    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch that `setUpstreamCompat(True)` turns on is on, `False`
    /// otherwise (the corrected defaults after construction or
    /// `setUpstreamCompat(False)`).
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        get_upstream_compat(slf.py(), &model_of(slf)?)
    }
    #[pyo3(name = "assembleInternalForces")]
    fn assemble_internal_forces(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_contact(slf, |c| c.assemble_internal_forces())
    }
    /// `assembleStiffnessMatrix()`: needs the coupler's DOF manager (a
    /// stand-alone contact model has none).
    #[pyo3(name = "assembleStiffnessMatrix")]
    fn assemble_stiffness_matrix(slf: &Bound<'_, Self>) -> PyResult<()> {
        Err(super::errors::exception(
            slf.py(),
            "a stand-alone ContactMechanicsModel has no DOF manager: assemble through its coupler"
                .into(),
        ))
    }
    #[pyo3(name = "getExternalForce")]
    fn get_external_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "external_force")
    }
    #[pyo3(name = "getNormalForce")]
    fn get_normal_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "normal_force")
    }
    #[pyo3(name = "getTangentialForce")]
    fn get_tangential_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "tangential_force")
    }
    #[pyo3(name = "getInternalForce")]
    fn get_internal_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "internal_force")
    }
    #[pyo3(name = "getGaps")]
    fn get_gaps<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "gaps")
    }
    #[pyo3(name = "getNormals")]
    fn get_normals<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "normals")
    }
    #[pyo3(name = "getNodalArea")]
    fn get_nodal_area<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        contact_field(slf, "nodal_area")
    }
    #[pyo3(name = "getContactDetector")]
    fn get_contact_detector(slf: &Bound<'_, Self>) -> PyResult<PyContactDetector> {
        Ok(PyContactDetector::of_model(model_of(slf)?))
    }
    #[pyo3(name = "getContactElements")]
    fn get_contact_elements(slf: &Bound<'_, Self>) -> PyResult<PyContactElementsView> {
        let els = with_contact(slf, |c| {
            Ok(c.contact_elements()
                .iter()
                .map(|e| PyContactElement {
                    master: PyElement { element: e.master },
                    slave: i64::from(e.slave),
                })
                .collect::<Vec<_>>())
        })?;
        Ok(PyContactElementsView { elements: els })
    }
}

// --- couplers ---------------------------------------------------------------------------------------

macro_rules! coupler_impl {
    ($rust:ident, { $($extra:tt)* }) => {
        #[pymethods]
        impl $rust {
        $($extra)*
        #[pyo3(name = "applyBC")]
        fn apply_bc(slf: &Bound<'_, Self>, func: &Bound<'_, PyAny>, element_group: String) -> PyResult<()> {
            super::bc::apply_bc(slf.py(), &model_of(slf)?, func, element_group)
        }
        #[pyo3(name = "getContactMechanicsModel")]
        fn get_contact_mechanics_model<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            super::handle::model_object(slf.py(), &model_of(slf)?.with_part(Part::Contact))
        }
        #[pyo3(name = "getDisplacement")]
        fn get_displacement<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            model_of(slf)?.with_part(Part::Solid).view::<f64>(slf.py(), ModelKey::Nodal("displacement"))
        }
        #[pyo3(name = "getVelocity")]
        fn get_velocity<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            model_of(slf)?.with_part(Part::Solid).view::<f64>(slf.py(), ModelKey::Nodal("velocity"))
        }
        #[pyo3(name = "getAcceleration")]
        fn get_acceleration<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            model_of(slf)?.with_part(Part::Solid).view::<f64>(slf.py(), ModelKey::Nodal("acceleration"))
        }
        #[pyo3(name = "getMass")]
        fn get_mass<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            model_of(slf)?.with_part(Part::Solid).view::<f64>(slf.py(), ModelKey::Nodal("mass"))
        }
        /// `getContactForce()`: the solid's internal force, as upstream
        /// (UD-623).
        #[pyo3(name = "getContactForce")]
        fn get_contact_force<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
            model_of(slf)?.with_part(Part::Solid).view::<f64>(slf.py(), ModelKey::Nodal("internal_force"))
        }
        #[pyo3(name = "getContactDetector")]
        fn get_contact_detector(slf: &Bound<'_, Self>) -> PyResult<PyContactDetector> {
            Ok(PyContactDetector::of_model(model_of(slf)?.with_part(Part::Contact)))
        }
        #[pyo3(name = "setTimeStep", signature = (time_step, solver_id = String::new()))]
        fn set_time_step(slf: &Bound<'_, Self>, time_step: Real, solver_id: String) -> PyResult<()> {
            model_of(slf)?.with_ops(slf.py(), move |m| m.set_time_step(time_step, Some(&solver_id)))
        }
        }
    };
}

fn new_coupler(
    py: Python<'_>,
    mesh: &Bound<'_, PyMesh>,
    spatial_dimension: i64,
    id: String,
    dof_manager: Option<&Bound<'_, PyAny>>,
    cohesive: bool,
) -> PyResult<ModelRef> {
    super::solver::refuse_user_dof_manager(dof_manager)?;
    let dim = super::mesh::dim_filter(spatial_dimension)?;
    let m = new_handle(py, mesh, move |sh, parser| {
        Ok(if cohesive {
            AnyModel::SolidCohesiveContact(Box::new(
                akantu::couplers::CouplerSolidCohesiveContact::with_parser(sh, dim, &id, parser)?,
            ))
        } else {
            AnyModel::SolidContact(Box::new(
                akantu::couplers::CouplerSolidContact::with_parser(sh, dim, &id, parser)?,
            ))
        })
    })?;
    let h = Arc::clone(&m.h);
    native(py, move || {
        h.with(|a| {
            super::laws::install_python_materials(a.solid(Part::Solid)?, &h, Part::Solid);
            Ok(())
        })
    })?;
    Ok(m)
}

/// `CouplerSolidContact(mesh, spatial_dimension=_all_dimensions,
/// id="coupler_solid_contact", dof_manager=None)` (PY-089).
#[pyclass(extends = PyModel, frozen, name = "CouplerSolidContact", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyCouplerSolidContact;

coupler_impl!(PyCouplerSolidContact, {
    /// `setUpstreamCompat(upstream=True)`: turn on every upstream-compatibility
    /// switch of the coupler (UD-374, UD-1291) and of its contact model (UD-002,
    /// UD-008, UD-363, UD-364, UD-365, UD-368, UD-369, UD-371, UD-372), so that
    /// a script reproduces the reference.
    ///
    /// `False` restores the corrected defaults. Not in upstream's module (port
    /// extension, D-003 category B / D-078); `getUpstreamCompat()` reads it
    /// back. The fine-grained switches stay in Rust.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        set_upstream_compat(slf.py(), &model_of(slf)?, upstream)
    }
    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch that `setUpstreamCompat(True)` turns on is on, `False`
    /// otherwise (the corrected defaults after construction or
    /// `setUpstreamCompat(False)`).
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        get_upstream_compat(slf.py(), &model_of(slf)?)
    }
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "coupler_solid_contact".to_owned(), dof_manager = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        dof_manager: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        Ok(base(new_coupler(
            py,
            mesh,
            spatial_dimension,
            id,
            dof_manager,
            false,
        )?)
        .add_subclass(Self))
    }
    #[pyo3(name = "getSolidMechanicsModel")]
    fn get_solid_mechanics_model<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        super::handle::model_object(slf.py(), &model_of(slf)?.with_part(Part::Solid))
    }
    /// `initFull(_analysis_method=_explicit_lumped_mass)`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        let m = model_of(slf)?;
        let h = Arc::clone(&m.h);
        m.with_any(slf.py(), move |a, _| {
            super::laws::install_python_materials(a.solid(Part::Solid)?, &h, Part::Solid);
            a.solid_contact()?.init_full(&o)
        })
    }
    #[pyo3(name = "getStableTimeStep")]
    fn get_stable_time_step(slf: &Bound<'_, Self>) -> PyResult<Real> {
        model_of(slf)?.with_any(slf.py(), |a, _| a.solid_contact()?.stable_time_step())
    }
});

/// `CouplerSolidCohesiveContact(mesh, spatial_dimension=_all_dimensions,
/// id="coupler_solid_contact", dof_manager=None)` (PY-089).
#[pyclass(extends = PyModel, frozen, name = "CouplerSolidCohesiveContact", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyCouplerSolidCohesiveContact;

coupler_impl!(PyCouplerSolidCohesiveContact, {
    /// `setUpstreamCompat(upstream=True)`: turn on every upstream-compatibility
    /// switch of the coupler (UD-374, UD-772, UD-1291) and of its contact model
    /// (UD-002, UD-008, UD-363, UD-364, UD-365, UD-368, UD-369, UD-371,
    /// UD-372), so that a script reproduces the reference.
    ///
    /// `False` restores the corrected defaults. Not in upstream's module (port
    /// extension, D-003 category B / D-078); `getUpstreamCompat()` reads it
    /// back. The fine-grained switches stay in Rust.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        set_upstream_compat(slf.py(), &model_of(slf)?, upstream)
    }
    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch that `setUpstreamCompat(True)` turns on is on, `False`
    /// otherwise (the corrected defaults after construction or
    /// `setUpstreamCompat(False)`).
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        get_upstream_compat(slf.py(), &model_of(slf)?)
    }
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "coupler_solid_contact".to_owned(), dof_manager = None))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
        dof_manager: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        Ok(base(new_coupler(
            py,
            mesh,
            spatial_dimension,
            id,
            dof_manager,
            true,
        )?)
        .add_subclass(Self))
    }
    #[pyo3(name = "getSolidMechanicsModelCohesive")]
    fn get_solid_mechanics_model_cohesive<'py>(
        slf: &Bound<'py, Self>,
    ) -> PyResult<Bound<'py, PyAny>> {
        super::handle::model_object(slf.py(), &model_of(slf)?.with_part(Part::Solid))
    }
    /// `initFull(_analysis_method=_explicit_lumped_mass, _is_extrinsic=False)`.
    #[pyo3(name = "initFull", signature = (_analysis_method = None, _is_extrinsic = false))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
        _is_extrinsic: bool,
    ) -> PyResult<()> {
        let o = options_of(_analysis_method)?;
        let m = model_of(slf)?;
        let h = Arc::clone(&m.h);
        m.with_any(slf.py(), move |a, _| {
            super::laws::install_python_materials(a.solid(Part::Solid)?, &h, Part::Solid);
            a.solid_cohesive_contact()?.init_full(&o, _is_extrinsic)
        })
    }
    #[pyo3(name = "checkCohesiveStress")]
    fn check_cohesive_stress(slf: &Bound<'_, Self>) -> PyResult<usize> {
        model_of(slf)?.with_any(slf.py(), |a, _| {
            a.solid_cohesive_contact()?.check_cohesive_stress()
        })
    }
    #[pyo3(name = "getStableTimeStep")]
    fn get_stable_time_step(slf: &Bound<'_, Self>) -> PyResult<Real> {
        model_of(slf)?.with_any(slf.py(), |a, _| {
            a.solid_cohesive_contact()?.stable_time_step()
        })
    }
});

/// `register_heat_transfer_model`.
pub(crate) fn register_heat_transfer(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_class_as::<PyClDiffusion>(m, "ConstitutiveLawakantu::DiffusionModel")?;
    m.add_class::<PyDiffusionLaw>()?;
    m.add_class::<PyHeatDiffusion1D>()?;
    m.add_class::<PyHeatDiffusion2D>()?;
    m.add_class::<PyHeatDiffusion3D>()?;
    add_class_as::<PyClhDiffusion>(
        m,
        "ConstitutiveLawsHandlerakantu::DiffusionLawakantu::Model",
    )?;
    m.add_class::<PyDiffusionModel>()?;
    m.add_class::<PyHeatTransferModel>()?;
    Ok(())
}

/// `register_structural_mechanics_model`.
pub(crate) fn register_structural(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStructuralMaterial>()?;
    m.add_class::<PyStructuralMechanicsModel>()?;
    Ok(())
}

/// `register_contact_mechanics_model` and `register_model_couplers`.
pub(crate) fn register_contact(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyContactDetector>()?;
    m.add_class::<PySurfaceSelector>()?;
    m.add_class::<PyGeometryUtils>()?;
    m.add_class::<PyPhysicalSurfaceSelector>()?;
    m.add_class::<PyCohesiveSurfaceSelector>()?;
    m.add_class::<PyAllSurfaceSelector>()?;
    m.add("ContactMechanicsModelOptions", m.getattr("ModelOptions")?)?;
    m.add_class::<PyContactElementsView>()?;
    m.add_class::<PyContactMechanicsModel>()?;
    m.add_class::<PyContactElement>()?;
    m.add_class::<PyCouplerSolidContact>()?;
    m.add_class::<PyCouplerSolidCohesiveContact>()?;
    Ok(())
}

/// `register_phase_field_model`, `register_phasefield`,
/// `register_phase_field_coupler`, `register_phase_field_element_filter`.
pub(crate) fn register_phase_field(m: &Bound<'_, PyModule>) -> PyResult<()> {
    add_class_as::<PyClhPhaseField>(m, "ConstitutiveLawsHandlerakantu::PhaseFieldakantu::Model")?;
    m.add("PhaseFieldModelOptions", m.getattr("ModelOptions")?)?;
    m.add_class::<PyPhaseFieldModel>()?;
    add_class_as::<PyClPhaseField>(m, "ConstitutiveLawakantu::PhaseFieldModel")?;
    m.add_class::<PyPhaseField>()?;
    m.add_class::<PyPhaseFieldFactory>()?;
    m.add_class::<PyCouplerSolidPhaseField>()?;
    m.add_class::<PyPhaseFieldElementFilter>()?;
    Ok(())
}
