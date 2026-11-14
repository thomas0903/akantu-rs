// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_solid_mechanics_model_cohesive.cc
// (CohesiveElementInserter, SolidMechanicsModelCohesiveOptions,
// SolidMechanicsModelCohesive), python/py_material.cc (MaterialCohesive and
// the cohesive law classes), python/py_constitutive_law_selector.cc (the
// cohesive selectors), python/py_fragment_manager.cc (FragmentManager).

//! Cohesive bindings (PY-078, PY-080…082; group 4).
//!
//! `SolidMechanicsModelCohesive` derives from `SolidMechanicsModel` (the
//! inherited methods act on the cohesive model's solid part). Upstream
//! returns references to the inserter's facet flags and to the model's
//! facet stresses and tangents: the port returns read-only snapshot copies
//! (plain arrays of the cohesive extension, D-056 item 2). Opening and
//! tractions of a cohesive law are zero-copy views of the law internals.
//! A Python subclass of `MaterialCohesive` would need a runtime cohesive law
//! type in `akantu-cohesive`:
//! such an allocator raises.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use akantu::cohesive::selector::{
    MaterialCohesiveRulesSelector as RsRules, MeshDataMaterialCohesiveSelector as RsMeshData,
};
use akantu::cohesive::{FragmentManager, MaterialCohesive, SolidMechanicsModelCohesive};
use akantu::core::enums::AnalysisMethod as RsAnalysisMethod;
use akantu::core::{ElementType, Error, GhostType, Real, TypeFilter};
use akantu::model::ModelOptions;
use akantu::model::constitutive_law::ConstitutiveLawSelector;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use super::arrays::owned_numpy;
use super::enums::{DirectionArg, EtArg, GhostArg};
use super::etma::{EtmaBool, EtmaInt, EtmaProvider};
use super::handle::{AnyModel, ModelHandle, ModelRef, Part};
use super::laws::{LawLink, PyConstitutiveLawSelector, SelectorKind, install_python_materials};
use super::mesh::{PY_ALL_DIMENSIONS, PyGroupManager, PyMesh};
use super::model::{AnalysisArg, PyModel, PyModelOptions, PyModelSolver, PyModelSolverOptions};
use super::runtime::native;
use super::solid::{PyClhMaterial, PyMaterial, PySolidMechanicsModel};
use crate::foreign_view::numpy_export::{Exportable, Layout};

fn read_only(a: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    a.call_method1("setflags", (false,))?;
    Ok(a)
}

fn block_copy<'py, T: Exportable>(
    py: Python<'py>,
    values: Vec<T>,
    rows: usize,
    cols: usize,
) -> PyResult<Bound<'py, PyAny>> {
    read_only(owned_numpy(py, values, rows, cols, Layout::Rows)?)
}

// --- SolidMechanicsModelCohesiveOptions -------------------------------------------------

/// `SolidMechanicsModelCohesiveOptions(analysis_method=_explicit_lumped_mass,
/// solver_options=ModelSolverOptions(), is_extrinsic=False)` (PY-081).
#[pyclass(extends = PyModelOptions, frozen, name = "SolidMechanicsModelCohesiveOptions", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySolidMechanicsModelCohesiveOptions {
    pub(crate) is_extrinsic: bool,
}

#[pymethods]
impl PySolidMechanicsModelCohesiveOptions {
    #[new]
    #[pyo3(signature = (analysis_method = None, solver_options = None, is_extrinsic = false))]
    fn new(
        analysis_method: Option<AnalysisArg>,
        solver_options: Option<PyRef<'_, PyModelSolverOptions>>,
        is_extrinsic: bool,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyModelOptions {
            options: ModelOptions {
                analysis_method: analysis_method
                    .and_then(|a| a.0)
                    .unwrap_or(RsAnalysisMethod::ExplicitLumpedMass),
                solver_options: solver_options
                    .map(|s| s.options.clone())
                    .unwrap_or_default(),
            },
        })
        .add_subclass(Self { is_extrinsic })
    }
}

// --- SolidMechanicsModelCohesive ---------------------------------------------------------

/// `SolidMechanicsModelCohesive(mesh, spatial_dimension=_all_dimensions,
/// id="solid_mechanics_model")` (PY-081).
#[pyclass(extends = PySolidMechanicsModel, frozen, name = "SolidMechanicsModelCohesive", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySolidMechanicsModelCohesive;

/// The model handle of a cohesive model object.
fn cm(slf: &Bound<'_, PySolidMechanicsModelCohesive>) -> PyResult<ModelRef> {
    super::model::model_of(slf)
}

/// Run `f` on the cohesive model of `model`.
pub(crate) fn with_cohesive<R: Send>(
    py: Python<'_>,
    model: &ModelRef,
    f: impl FnOnce(&mut SolidMechanicsModelCohesive) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    model.with_any(py, move |m, part| f(m.cohesive(part)?))
}

impl PySolidMechanicsModelCohesive {
    /// A Python object of the cohesive model of `model`.
    pub(crate) fn wrap(py: Python<'_>, model: ModelRef) -> PyResult<Bound<'_, Self>> {
        Bound::new(
            py,
            PyModelSolver::init(model)
                .add_subclass(PyModel)
                .add_subclass(PyClhMaterial)
                .add_subclass(PySolidMechanicsModel)
                .add_subclass(Self),
        )
    }
}

#[pymethods]
impl PySolidMechanicsModelCohesive {
    #[new]
    #[pyo3(signature = (mesh, spatial_dimension = PY_ALL_DIMENSIONS, id = "solid_mechanics_model".to_owned()))]
    fn new(
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        spatial_dimension: i64,
        id: String,
    ) -> PyResult<PyClassInitializer<Self>> {
        let mesh = PyMesh::mesh_ref(mesh);
        let dim = super::mesh::dim_filter(spatial_dimension)?;
        let shared = mesh.h.shared.clone();
        let mh = Arc::clone(&mesh.h);
        let model = native(py, move || {
            mh.check_topology()?;
            let parser = akantu::runtime::global()
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .parser
                .clone();
            // the constructor builds the facet mesh (a mesh mutation)
            let m = SolidMechanicsModelCohesive::with_parser(shared, dim, &id, parser)?;
            mh.detach_topology()?;
            Ok(m)
        })?;
        let handle = ModelHandle::new(mesh, AnyModel::Cohesive(Box::new(model)));
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
            .add_subclass(PySolidMechanicsModel)
            .add_subclass(Self))
    }

    /// `setUpstreamCompat(upstream=True)` (port extension, D-078):
    /// `True` turns on every upstream-compatibility switch of
    /// the cohesive model — UD-034 (`K` kept symmetric, the upper triangle
    /// of the unsymmetric cohesive tangent; must be set before `initFull`)
    /// and the cohesive switches (`CohesiveUpstreamCompat::upstream()`:
    /// UD-028, UD-040, UD-217…219, UD-224, UD-241, the tangent switches
    /// and UD-1280, `doubled_node_keeps_external_force`: a doubled loaded
    /// node keeps upstream's load placement; read at `initFull`); `False`
    /// restores the corrected defaults. Not in upstream's module;
    /// `getUpstreamCompat()` reads it back.
    #[pyo3(name = "setUpstreamCompat", signature = (upstream = true))]
    fn set_upstream_compat(slf: &Bound<'_, Self>, upstream: bool) -> PyResult<()> {
        with_cohesive(slf.py(), &cm(slf)?, move |m| {
            use akantu::cohesive::CohesiveUpstreamCompat as C;
            use akantu::linalg::SymmetryPolicy as P;
            m.set_symmetry_policy(if upstream {
                P::UpstreamUpperSelection
            } else {
                P::PromoteOnUnsymmetric
            })?;
            m.ext.compat = if upstream {
                C::upstream()
            } else {
                C::default()
            };
            Ok(())
        })
    }

    /// `getUpstreamCompat()` (port extension, D-078): `True` when every
    /// switch `setUpstreamCompat(True)` turns on is on.
    #[pyo3(name = "getUpstreamCompat")]
    fn get_upstream_compat(slf: &Bound<'_, Self>) -> PyResult<bool> {
        with_cohesive(slf.py(), &cm(slf)?, |m| {
            Ok(
                m.symmetry_policy() == akantu::linalg::SymmetryPolicy::UpstreamUpperSelection
                    && m.ext.compat == akantu::cohesive::CohesiveUpstreamCompat::upstream(),
            )
        })
    }

    /// `initFull(_analysis_method=_explicit_lumped_mass,
    /// _is_extrinsic=False)` (also `initFull(SolidMechanicsModelCohesiveOptions)`).
    #[pyo3(name = "initFull", signature = (_analysis_method = None, _is_extrinsic = false))]
    fn init_full(
        slf: &Bound<'_, Self>,
        _analysis_method: Option<&Bound<'_, PyAny>>,
        _is_extrinsic: bool,
    ) -> PyResult<()> {
        let mut options = ModelOptions::default();
        let mut extrinsic = _is_extrinsic;
        if let Some(a) = _analysis_method {
            if let Ok(o) = a.cast::<PySolidMechanicsModelCohesiveOptions>() {
                extrinsic = o.get().is_extrinsic;
                options = o.as_super().get().options.clone();
            } else if let Ok(o) = a.cast::<PyModelOptions>() {
                options = o.get().options.clone();
            } else {
                let m: AnalysisArg = a.extract()?;
                options.analysis_method = m.0.unwrap_or(RsAnalysisMethod::ExplicitLumpedMass);
            }
        }
        let model = cm(slf)?;
        let h = Arc::clone(&model.h);
        let part = model.part;
        with_cohesive(slf.py(), &model, move |c| {
            let rt = akantu::runtime::global()
                .read()
                .unwrap_or_else(PoisonError::into_inner);
            c.smm.physics.base.parser = rt.parser.clone();
            drop(rt);
            install_python_materials(&mut c.smm, &h, part);
            c.init_full(&options, extrinsic)
        })
    }

    /// `checkCohesiveStress()` → number of inserted elements. With Python
    /// `MaterialCohesive` subclasses (runtime cohesive laws) the
    /// check runs in three phases: the facet stresses and the built-in
    /// laws' checks, then each Python law's `checkInsertion(False)` with
    /// the model released (its accessors work as outside the check; the
    /// insertion flags are writable views), then the insertion
    /// (`check_cohesive_stress_prepare` / `_finish`).
    #[pyo3(name = "checkCohesiveStress")]
    fn check_cohesive_stress(slf: &Bound<'_, Self>) -> PyResult<usize> {
        let py = slf.py();
        let model = cm(slf)?;
        let check = with_cohesive(py, &model, |m| m.check_cohesive_stress_prepare())?;
        if check.foreign_laws.is_empty() {
            return with_cohesive(py, &model, move |m| m.check_cohesive_stress_finish(check));
        }
        let ids = {
            let laws = check.foreign_laws.clone();
            with_cohesive(py, &model, move |m| {
                laws.iter()
                    .map(|&i| Ok(m.smm.physics.materials.law(i)?.law_id().to_owned()))
                    .collect::<akantu::core::Result<Vec<String>>>()
            })
        };
        let run = || -> PyResult<()> {
            for id in ids? {
                let obj = model.h.law_object(model.part, &id).ok_or_else(|| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "the Python object of the cohesive law '{id}' is gone"
                    ))
                })?;
                obj.bind(py).call_method1("checkInsertion", (false,))?;
            }
            Ok(())
        };
        match run() {
            Ok(()) => with_cohesive(py, &model, move |m| m.check_cohesive_stress_finish(check)),
            Err(e) => {
                with_cohesive(py, &model, move |m| m.abort_insertion_check(&check))?;
                Err(e)
            }
        }
    }

    /// `getElementInserter()`.
    #[pyo3(name = "getElementInserter")]
    fn get_element_inserter(slf: &Bound<'_, Self>) -> PyResult<PyCohesiveElementInserter> {
        Ok(PyCohesiveElementInserter { model: cm(slf)? })
    }

    /// `getStressOnFacets(type, ghost_type=_not_ghost)`: read-only copy.
    #[pyo3(name = "getStressOnFacets", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_stress_on_facets<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (ty, g) = (r#type.0, ghost_type.single()?);
        let (v, r, c) = with_cohesive(slf.py(), &cm(slf)?, move |m| {
            let a = m.ext.facet_stress.get(ty, g)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        block_copy(slf.py(), v, r, c)
    }

    /// `getTangents(type, ghost_type=_not_ghost)`: read-only copy.
    #[pyo3(name = "getTangents", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_tangents<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (ty, g) = (r#type.0, ghost_type.single()?);
        let (v, r, c) = with_cohesive(slf.py(), &cm(slf)?, move |m| {
            let a = m.ext.tangents.get(ty, g)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        block_copy(slf.py(), v, r, c)
    }

    /// `updateAutomaticInsertion()`.
    #[pyo3(name = "updateAutomaticInsertion")]
    fn update_automatic_insertion(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_cohesive(
            slf.py(),
            &cm(slf)?,
            SolidMechanicsModelCohesive::update_automatic_insertion,
        )
    }

    /// `assembleInternalForces()` (the cohesive model's: tractions first).
    #[pyo3(name = "assembleInternalForces")]
    fn assemble_internal_forces(slf: &Bound<'_, Self>) -> PyResult<()> {
        with_cohesive(
            slf.py(),
            &cm(slf)?,
            SolidMechanicsModelCohesive::assemble_internal_forces,
        )
    }

    /// `setMaterialSelector(selector)`: a cohesive selector (or a Python
    /// selector) heads the model's chain, followed by the default cohesive
    /// selector; another built-in selector replaces the chain.
    #[pyo3(name = "setMaterialSelector")]
    fn set_material_selector(
        slf: &Bound<'_, Self>,
        selector: &Bound<'_, PyConstitutiveLawSelector>,
    ) -> PyResult<()> {
        let py = slf.py();
        let model = cm(slf)?;
        model.h.set_selector_object(
            model.part,
            super::handle::Handler::Material,
            selector.clone().into_any().unbind(),
        );
        let base = py.get_type::<PyConstitutiveLawSelector>();
        let overridden = !selector
            .get_type()
            .getattr("__call__")?
            .is(&base.getattr("__call__")?);
        let kind = selector.get().kind.clone();
        if !overridden && !matches!(kind, SelectorKind::Cohesive(_)) {
            let chain = super::laws::selector_chain(selector)?;
            return with_cohesive(py, &model, move |c| {
                c.set_material_selector_chain(chain);
                Ok(())
            });
        }
        let chain = if overridden {
            Some(super::laws::selector_chain(selector)?)
        } else {
            None
        };
        with_cohesive(py, &model, move |c| {
            let def = c.default_cohesive_selector();
            let first: Box<dyn ConstitutiveLawSelector> = match kind {
                SelectorKind::Cohesive(CohesiveKind::Default) if !overridden => Box::new(def),
                SelectorKind::Cohesive(CohesiveKind::MeshData) if !overridden => {
                    Box::new(RsMeshData { default: def })
                }
                SelectorKind::Cohesive(CohesiveKind::Rules {
                    rules,
                    mesh_data_id,
                }) if !overridden => Box::new(RsRules {
                    rules,
                    mesh_data_id,
                    default: def,
                }),
                _ => {
                    // a Python selector: its trampoline (first of its chain)
                    let mut chain =
                        chain.ok_or_else(|| Error::invalid("internal error: selector chain"))?;
                    if chain.selectors.is_empty() {
                        return Err(Error::invalid("empty selector chain"));
                    }
                    chain.selectors.remove(0)
                }
            };
            c.set_material_selector(first);
            Ok(())
        })
    }
}

// --- CohesiveElementInserter -----------------------------------------------------------------

/// `CohesiveElementInserter` (PY-080) of a cohesive model.
#[pyclass(
    frozen,
    name = "CohesiveElementInserter",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyCohesiveElementInserter {
    model: ModelRef,
}

/// Snapshot copies of the inserter's facet flags.
#[derive(Debug)]
struct FacetFlags {
    model: ModelRef,
    check: bool,
}

impl FacetFlags {
    fn with_map<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&akantu::core::ElementTypeMapArray<bool>) -> akantu::core::Result<R> + Send,
    ) -> PyResult<R> {
        let check = self.check;
        with_cohesive(py, &self.model, move |m| {
            if check {
                f(m.ext.inserter.check_facets())
            } else {
                f(m.ext.inserter.insertion_facets())
            }
        })
    }
}

impl EtmaProvider<bool> for FacetFlags {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        self.with_map(py, |m| Ok(m.id().to_owned()))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        self.with_map(py, move |m| Ok(m.element_types(filter).collect()))
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, r, c) = self.with_map(py, move |m| {
            let a = m.get(ty, ghost)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        block_copy(py, v, r, c)
    }
}

impl PyCohesiveElementInserter {
    fn flags<'py>(
        &self,
        py: Python<'py>,
        check: bool,
        ty: Option<EtArg>,
        ghost: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let p = FacetFlags {
            model: self.model.clone(),
            check,
        };
        match ty {
            Some(t) => p.call(py, t.0, ghost.single()?),
            None => Ok(Bound::new(py, EtmaBool::from_provider(Arc::new(p)))?.into_any()),
        }
    }
}

#[pymethods]
impl PyCohesiveElementInserter {
    /// `setLimit(axis, first_limit, second_limit)`.
    #[pyo3(name = "setLimit")]
    fn set_limit(
        &self,
        py: Python<'_>,
        axis: DirectionArg,
        first: Real,
        second: Real,
    ) -> PyResult<()> {
        with_cohesive(py, &self.model, move |m| {
            m.ext.inserter.set_limit(axis.0, first, second)
        })
    }

    /// `getCheckFacets()` (map) or `getCheckFacets(type,
    /// ghost_type=_not_ghost)` (block): read-only copies.
    #[pyo3(name = "getCheckFacets", signature = (r#type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_check_facets<'py>(
        &self,
        py: Python<'py>,
        r#type: Option<EtArg>,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.flags(py, true, r#type, ghost_type)
    }

    /// `getInsertionFacets()` / `getInsertionFacets(type, ghost_type)`:
    /// read-only copies.
    #[pyo3(name = "getInsertionFacets", signature = (r#type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_insertion_facets<'py>(
        &self,
        py: Python<'py>,
        r#type: Option<EtArg>,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        // inside the Python laws' insertion checks the flags are staged as
        // exportable storage: a zero-copy writable view
        if let Some(t) = &r#type {
            let staged = with_cohesive(py, &self.model, |m| {
                Ok(m.ext.staged_insertion_facets.is_some())
            })?;
            if staged {
                let g = ghost_type.single()?;
                return self
                    .model
                    .view::<bool>(py, super::handle::ModelKey::InsertionFacets(t.0, g));
            }
        }
        self.flags(py, false, r#type, ghost_type)
    }

    #[pyo3(name = "addPhysicalSurface")]
    fn add_physical_surface(&self, py: Python<'_>, name: String) -> PyResult<()> {
        with_cohesive(py, &self.model, move |m| {
            m.ext.inserter.add_physical_surface(name);
            Ok(())
        })
    }

    #[pyo3(name = "addPhysicalVolume")]
    fn add_physical_volume(&self, py: Python<'_>, name: String) -> PyResult<()> {
        with_cohesive(py, &self.model, move |m| {
            m.ext.inserter.add_physical_volume(name);
            Ok(())
        })
    }
}

// --- MaterialCohesive --------------------------------------------------------------------

/// `MaterialCohesive` (PY-078): a cohesive law of a cohesive model.
#[pyclass(extends = PyMaterial, subclass, frozen, name = "MaterialCohesive", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMaterialCohesive;

fn cohesive_link(slf: &Bound<'_, PyMaterialCohesive>) -> PyResult<Arc<LawLink>> {
    super::solid::link_of(slf.as_super())
        .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("not a law object"))
}

/// Run `f` on the cohesive law of `link`.
fn with_cohesive_law<R: Send>(
    py: Python<'_>,
    link: &Arc<LawLink>,
    f: impl FnOnce(&MaterialCohesive) -> akantu::core::Result<R> + Send,
) -> PyResult<R> {
    native(py, || {
        link.with_law(|smm, i| {
            let law = smm.material(i)?;
            let c = law
                .as_any()
                .and_then(|a| a.downcast_ref::<MaterialCohesive>())
                .ok_or_else(|| Error::invalid("this law is not a cohesive law"))?;
            f(c)
        })
    })
}

/// The facet filter of a cohesive law (read-only copies).
#[derive(Debug)]
struct FacetFilter(Arc<LawLink>);

impl EtmaProvider<i32> for FacetFilter {
    fn id(&self, py: Python<'_>) -> PyResult<String> {
        with_cohesive_law(py, &self.0, |c| Ok(c.core.facet_filter.id().to_owned()))
    }
    fn types(&self, py: Python<'_>, filter: TypeFilter) -> PyResult<Vec<ElementType>> {
        with_cohesive_law(py, &self.0, move |c| {
            Ok(c.core.facet_filter.element_types(filter).collect())
        })
    }
    fn call<'py>(
        &self,
        py: Python<'py>,
        ty: ElementType,
        ghost: GhostType,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, r, c) = with_cohesive_law(py, &self.0, move |c| {
            let a = c.core.facet_filter.get(ty, ghost)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        block_copy(py, v, r, c)
    }
}

#[pymethods]
impl PyMaterialCohesive {
    /// `MaterialCohesive(model, id)` (Python subclasses:
    /// `super().__init__(model, id)`): the object becomes a runtime cohesive
    /// law when a factory allocator returns it (D-078;
    /// `akantu::cohesive::laws::foreign`).
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyClassInitializer<Self> {
        super::solid::material_init(super::laws::LawLink::uninit()).add_subclass(Self)
    }

    /// `getFacetFilter()` (map) or `getFacetFilter(type,
    /// ghost_type=_not_ghost)` (block): read-only copies.
    #[pyo3(name = "getFacetFilter", signature = (r#type = None, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_facet_filter<'py>(
        slf: &Bound<'py, Self>,
        r#type: Option<EtArg>,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let p = FacetFilter(cohesive_link(slf)?);
        match r#type {
            Some(t) => p.call(slf.py(), t.0, ghost_type.single()?),
            None => Ok(Bound::new(slf.py(), EtmaInt::from_provider(Arc::new(p)))?.into_any()),
        }
    }

    /// `getOpening(type, ghost_type=_not_ghost)`: view of the law's opening.
    #[pyo3(name = "getOpening", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_opening<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        cohesive_link(slf)?.real_view(slf.py(), "opening", r#type.0, ghost_type.single()?)
    }

    /// `getTraction(type, ghost_type=_not_ghost)`: view of the law's
    /// tractions.
    #[pyo3(name = "getTraction", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_traction<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        cohesive_link(slf)?.real_view(slf.py(), "tractions", r#type.0, ghost_type.single()?)
    }
}

macro_rules! cohesive_law_class {
    ($rust:ident, $name:literal) => {
        #[doc = concat!("`", $name, "` (PY-078).")]
        #[pyclass(extends = PyMaterialCohesive, subclass, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;
    };
}

cohesive_law_class!(PyMaterialCohesiveLinear2D, "MaterialCohesiveLinear2D");
cohesive_law_class!(PyMaterialCohesiveLinear3D, "MaterialCohesiveLinear3D");
cohesive_law_class!(
    PyMaterialCohesiveLinearFriction2D,
    "MaterialCohesiveLinearFriction2D"
);
cohesive_law_class!(
    PyMaterialCohesiveLinearFriction3D,
    "MaterialCohesiveLinearFriction3D"
);

/// The Python object of a cohesive law (`None` for other laws).
pub(crate) fn cohesive_material_object<'py>(
    py: Python<'py>,
    init: PyClassInitializer<PyMaterial>,
    ty: &str,
    dim: usize,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    if !ty.starts_with("cohesive") {
        return Ok(None);
    }
    let c = init.add_subclass(PyMaterialCohesive);
    Ok(Some(match (ty, dim) {
        ("cohesive_linear", 2) => {
            Bound::new(py, c.add_subclass(PyMaterialCohesiveLinear2D))?.into_any()
        }
        ("cohesive_linear", 3) => {
            Bound::new(py, c.add_subclass(PyMaterialCohesiveLinear3D))?.into_any()
        }
        ("cohesive_linear_friction", 2) => {
            Bound::new(py, c.add_subclass(PyMaterialCohesiveLinearFriction2D))?.into_any()
        }
        ("cohesive_linear_friction", 3) => {
            Bound::new(py, c.add_subclass(PyMaterialCohesiveLinearFriction3D))?.into_any()
        }
        _ => Bound::new(py, c)?.into_any(),
    }))
}

// --- cohesive selectors ------------------------------------------------------------------

/// The cohesive selector kinds.
#[derive(Clone, Debug)]
pub(crate) enum CohesiveKind {
    /// `DefaultMaterialCohesiveSelector(model)`.
    Default,
    /// `MeshDataMaterialCohesiveSelector(model)`.
    MeshData,
    /// `MaterialCohesiveRulesSelector(model, rules, mesh_data_id)`.
    Rules {
        rules: BTreeMap<(String, String), String>,
        mesh_data_id: String,
    },
}

/// `DefaultMaterialCohesiveSelector(model)` (PY-070).
#[pyclass(extends = PyConstitutiveLawSelector, subclass, frozen, name = "DefaultMaterialCohesiveSelector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDefaultMaterialCohesiveSelector;

#[pymethods]
impl PyDefaultMaterialCohesiveSelector {
    #[new]
    fn new(model: &Bound<'_, PySolidMechanicsModelCohesive>) -> PyClassInitializer<Self> {
        let _ = model;
        PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::Cohesive(
            CohesiveKind::Default,
        )))
        .add_subclass(Self)
    }
}

/// `MeshDataMaterialCohesiveSelector(model)` (PY-070).
#[pyclass(extends = PyConstitutiveLawSelector, subclass, frozen, name = "MeshDataMaterialCohesiveSelector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMeshDataMaterialCohesiveSelector;

#[pymethods]
impl PyMeshDataMaterialCohesiveSelector {
    #[new]
    fn new(model: &Bound<'_, PySolidMechanicsModelCohesive>) -> PyClassInitializer<Self> {
        let _ = model;
        PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::Cohesive(
            CohesiveKind::MeshData,
        )))
        .add_subclass(Self)
    }
}

/// `MaterialCohesiveRulesSelector(model, rules, mesh_data_id="physical_names")`
/// (PY-070): `rules` maps `(name, name)` pairs of the two sides' mesh data
/// to a cohesive law name.
#[pyclass(extends = PyConstitutiveLawSelector, subclass, frozen, name = "MaterialCohesiveRulesSelector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMaterialCohesiveRulesSelector;

#[pymethods]
impl PyMaterialCohesiveRulesSelector {
    #[new]
    #[pyo3(signature = (model, rules, mesh_data_id = "physical_names".to_owned()))]
    fn new(
        model: &Bound<'_, PySolidMechanicsModelCohesive>,
        rules: BTreeMap<(String, String), String>,
        mesh_data_id: String,
    ) -> PyClassInitializer<Self> {
        let _ = model;
        PyClassInitializer::from(PyConstitutiveLawSelector::of_kind(SelectorKind::Cohesive(
            CohesiveKind::Rules {
                rules,
                mesh_data_id,
            },
        )))
        .add_subclass(Self)
    }
}

// --- FragmentManager ----------------------------------------------------------------------

/// `FragmentManager(model, dump_data=True, ID="fragment_manager")` (PY-082).
/// Its `GroupManager` base acts on the model's mesh groups (upstream: the
/// fragment manager's own group container holding `fragment_<i>`); the
/// computed data are returned as copies.
#[pyclass(extends = PyGroupManager, frozen, name = "FragmentManager", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFragmentManager {
    model: ModelRef,
    fm: Mutex<FragmentManager>,
}

impl PyFragmentManager {
    fn run<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(
            &mut FragmentManager,
            &mut SolidMechanicsModelCohesive,
        ) -> akantu::core::Result<R>
        + Send,
    ) -> PyResult<R> {
        let fm = &self.fm;
        with_cohesive(py, &self.model, move |m| {
            let mut fm = fm.lock().unwrap_or_else(PoisonError::into_inner);
            f(&mut fm, m)
        })
    }

    fn rows<'py>(
        &self,
        py: Python<'py>,
        pick: fn(&FragmentManager) -> &Vec<Real>,
        nbc: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (v, n) = {
            let fm = self.fm.lock().unwrap_or_else(PoisonError::into_inner);
            (pick(&fm).clone(), fm.nb_fragment())
        };
        let cols = if n == 0 { nbc } else { v.len() / n.max(1) };
        block_copy(py, v, n, cols)
    }
}

#[pymethods]
impl PyFragmentManager {
    #[new]
    #[pyo3(signature = (model, dump_data = true, ID = "fragment_manager".to_owned()))]
    #[allow(non_snake_case)] // upstream keyword argument name
    fn new(
        py: Python<'_>,
        model: &Bound<'_, PySolidMechanicsModelCohesive>,
        dump_data: bool,
        ID: String,
    ) -> PyResult<PyClassInitializer<Self>> {
        let _ = (dump_data, ID);
        let m = cm(model)?;
        let fm = with_cohesive(py, &m, |c| FragmentManager::new(c))?;
        Ok(PyClassInitializer::from(PyGroupManager {
            mesh: m.h.mesh.clone(),
        })
        .add_subclass(Self {
            model: m,
            fm: Mutex::new(fm),
        }))
    }

    #[pyo3(name = "buildFragments", signature = (damage_limit = 1.0))]
    fn build_fragments(&self, py: Python<'_>, damage_limit: Real) -> PyResult<()> {
        self.run(py, move |fm, m| fm.build_fragments(m, damage_limit))
    }

    #[pyo3(name = "computeAllData", signature = (damage_limit = 1.0))]
    fn compute_all_data(&self, py: Python<'_>, damage_limit: Real) -> PyResult<()> {
        self.run(py, move |fm, m| fm.compute_all_data(m, damage_limit))
    }

    #[pyo3(name = "computeCenterOfMass")]
    fn compute_center_of_mass(&self, py: Python<'_>) -> PyResult<()> {
        self.run(py, |fm, m| fm.compute_center_of_mass(m))
    }

    #[pyo3(name = "computeVelocity")]
    fn compute_velocity(&self, py: Python<'_>) -> PyResult<()> {
        self.run(py, |fm, m| fm.compute_velocity(m))
    }

    #[pyo3(name = "computeInertiaMoments")]
    fn compute_inertia_moments(&self, py: Python<'_>) -> PyResult<()> {
        self.run(py, |fm, m| fm.compute_inertia_moments(m))
    }

    #[pyo3(name = "computeNbElementsPerFragment")]
    fn compute_nb_elements_per_fragment(&self, py: Python<'_>) -> PyResult<()> {
        self.run(py, |fm, _| fm.compute_nb_elements_per_fragment())
    }

    #[pyo3(name = "getNbFragment")]
    fn get_nb_fragment(&self) -> usize {
        self.fm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .nb_fragment()
    }

    #[pyo3(name = "getMass")]
    fn get_mass<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.rows(py, |f| &f.mass, 1)
    }

    #[pyo3(name = "getVelocity")]
    fn get_velocity<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.rows(py, |f| &f.velocity, 1)
    }

    #[pyo3(name = "getMomentsOfInertia")]
    fn get_moments_of_inertia<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.rows(py, |f| &f.inertia_moments, 1)
    }

    #[pyo3(name = "getPrincipalDirections")]
    fn get_principal_directions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.rows(py, |f| &f.principal_directions, 1)
    }

    #[pyo3(name = "getNbElementsPerFragment")]
    fn get_nb_elements_per_fragment<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v: Vec<u32> = self
            .fm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .nb_elements_per_fragment
            .iter()
            .map(|&n| u32::try_from(n).unwrap_or(u32::MAX))
            .collect();
        let n = v.len();
        block_copy(py, v, n, 1)
    }
}

/// Register the cohesive classes (`register_solid_mechanics_model_cohesive`,
/// `register_fragment_manager`, the cohesive parts of `register_material`
/// and `register_constitutive_law_selector`).
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCohesiveElementInserter>()?;
    m.add_class::<PySolidMechanicsModelCohesiveOptions>()?;
    m.add_class::<PySolidMechanicsModelCohesive>()?;
    m.add_class::<PyFragmentManager>()?;
    Ok(())
}

/// Cohesive law classes (`register_material`).
pub(crate) fn register_materials(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMaterialCohesive>()?;
    m.add_class::<PyMaterialCohesiveLinear2D>()?;
    m.add_class::<PyMaterialCohesiveLinear3D>()?;
    m.add_class::<PyMaterialCohesiveLinearFriction2D>()?;
    m.add_class::<PyMaterialCohesiveLinearFriction3D>()?;
    Ok(())
}

/// Cohesive selectors (`register_constitutive_law_selector`).
pub(crate) fn register_selectors(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDefaultMaterialCohesiveSelector>()?;
    m.add_class::<PyMeshDataMaterialCohesiveSelector>()?;
    m.add_class::<PyMaterialCohesiveRulesSelector>()?;
    Ok(())
}
