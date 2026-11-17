// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_model.cc (ModelSolverOptions,
// ModelOptions, ModelSolver, Model), python/py_dof_manager.cc:185-212
// (NonLinearSolver.set, NonLinearSolverNewtonRaphson.getSparseSolver,
// NonLinearSolverLumped, TimeStepSolver.getIntegrationScheme).

//! Model base classes (PY-061, PY-062, PY-065…067).
//!
//! Class hierarchy (single inheritance): `SolverCallback` ← `ModelSolver` ←
//! `Model` ← the model classes (`SolidMechanicsModel`, …). Upstream's
//! `Parsable` base of `ModelSolver` and the `ConstitutiveLawsHandler…`
//! level are flattened (PyO3 has no multiple inheritance). Every model
//! object is a [`ModelRef`] (see [`super::handle`]); the methods here work
//! on any model through [`super::model_ops::ModelOps`].

use std::sync::Arc;

use akantu::core::enums::{
    AnalysisMethod as RsAnalysisMethod, IntegrationSchemeType as RsIntegrationSchemeType,
    NonLinearSolverType as RsNonLinearSolverType, SparseSolverType as RsSparseSolverType,
    TimeStepSolverType as RsTimeStepSolverType,
};
use akantu::core::{Real, Result};
use akantu::model::ModelOptions;
use akantu::parser::param::{ParamValue, ParameterRegistry};
use akantu::solver::integration_scheme::SolutionType;
use akantu::solver::model_solver::ModelSolverOptions;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use super::enums::{
    AnalysisMethod, IntegrationSchemeType, NamedEnumArg, NonLinearSolverType, SparseSolverType,
    TimeStepSolverType, enum_member_name,
};
use super::handle::{AnyModel, ModelRef, Part};
use super::parser::{PyParameterRegistry, PyParsable, RegistryAccess};
use super::runtime::IntoPy;
use super::solver::{
    PyDofManager, PySolverCallback, non_linear_kind, on_model_callback, scheme_object,
    solve_step_args, solve_step_with_callback, sparse_solver_object,
};

// --- options ---------------------------------------------------------------------

pub(crate) type AnalysisArg = NamedEnumArg<AnalysisMethod, RsAnalysisMethod>;

/// `ModelSolverOptions(...)` (PY-065): `ModelSolverOptions(time_step_solver_type=TimeStepSolverType.not_defined,
/// non_linear_solver_type=NonLinearSolverType.auto,
/// sparse_solver_type=SparseSolverType.auto, integration_scheme_type={},
/// solution_type={}, sparse_solver_ordering=None)`; `sparse_solver_ordering`
/// is the akantu-rs option of the `eigen` solver (D-125): `"colamd"`,
/// `"nested_dissection"` or `"auto"` (`None`: the solver's default).
#[pyclass(frozen, name = "ModelSolverOptions", module = "akantu.py11_akantu")]
#[derive(Clone, Debug)]
pub(crate) struct PyModelSolverOptions {
    pub(crate) options: ModelSolverOptions,
}

pub(crate) fn solution_type(obj: &Bound<'_, PyAny>) -> PyResult<SolutionType> {
    let name = match enum_member_name(obj)? {
        Some(n) => n,
        None => obj.extract::<String>()?,
    };
    Ok(match name.trim_start_matches('_') {
        "displacement" | "temperature" | "damage" => SolutionType::Displacement,
        "velocity" | "temperature_rate" => SolutionType::Velocity,
        "acceleration" => SolutionType::Acceleration,
        "not_defined" => SolutionType::NotDefined,
        other => {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "invalid solution type {other}"
            )));
        }
    })
}

#[pymethods]
impl PyModelSolverOptions {
    #[new]
    #[pyo3(signature = (time_step_solver_type = None, non_linear_solver_type = None, sparse_solver_type = None, integration_scheme_type = None, solution_type = None, sparse_solver_ordering = None))]
    fn new(
        time_step_solver_type: Option<NamedEnumArg<TimeStepSolverType, RsTimeStepSolverType>>,
        non_linear_solver_type: Option<NamedEnumArg<NonLinearSolverType, RsNonLinearSolverType>>,
        sparse_solver_type: Option<NamedEnumArg<SparseSolverType, RsSparseSolverType>>,
        integration_scheme_type: Option<&Bound<'_, PyDict>>,
        solution_type: Option<&Bound<'_, PyDict>>,
        sparse_solver_ordering: Option<String>,
    ) -> PyResult<Self> {
        let ordering = sparse_solver_ordering
            .map(|o| o.parse::<akantu::linalg::eigen_sparse_lu::SparseOrdering>())
            .transpose()
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let mut options = ModelSolverOptions {
            time_step_solver_type: time_step_solver_type.and_then(|a| a.0),
            non_linear_solver_type: non_linear_solver_type.and_then(|a| a.0),
            sparse_solver_type: sparse_solver_type.and_then(|a| a.0),
            sparse_solver_ordering: ordering,
            ..ModelSolverOptions::default()
        };
        if let Some(d) = integration_scheme_type {
            for (k, v) in d.iter() {
                let t: NamedEnumArg<IntegrationSchemeType, RsIntegrationSchemeType> =
                    v.extract()?;
                let t = t.0.ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("invalid integration scheme type")
                })?;
                options
                    .integration_scheme_type
                    .insert(k.extract::<String>()?, t);
            }
        }
        if let Some(d) = solution_type {
            for (k, v) in d.iter() {
                options
                    .solution_type
                    .insert(k.extract::<String>()?, self::solution_type(&v)?);
            }
        }
        Ok(Self { options })
    }
}

/// `ModelOptions(analysis_method=_explicit_lumped_mass,
/// solver_options=ModelSolverOptions())`; `SolidMechanicsModelOptions` is an
/// alias (as upstream).
#[pyclass(subclass, frozen, name = "ModelOptions", module = "akantu.py11_akantu")]
#[derive(Clone, Debug)]
pub(crate) struct PyModelOptions {
    pub(crate) options: ModelOptions,
}

#[pymethods]
impl PyModelOptions {
    #[new]
    #[pyo3(signature = (analysis_method = None, solver_options = None))]
    fn new(
        analysis_method: Option<AnalysisArg>,
        solver_options: Option<PyRef<'_, PyModelSolverOptions>>,
    ) -> Self {
        Self {
            options: ModelOptions {
                analysis_method: analysis_method
                    .and_then(|a| a.0)
                    .unwrap_or(RsAnalysisMethod::ExplicitLumpedMass),
                solver_options: solver_options
                    .map(|s| s.options.clone())
                    .unwrap_or_default(),
            },
        }
    }
}

// --- NonLinearSolver / TimeStepSolver ------------------------------------------------------

#[derive(Debug)]
struct SolverRegistry(ModelRef, String);

impl RegistryAccess for SolverRegistry {
    fn with_registry(
        &self,
        _prepared: &super::parser::Prepared,
        f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        let id = self.1.clone();
        let part = self.0.part;
        self.0.h.with(|m| {
            let id = (!id.is_empty()).then_some(id.as_str());
            let s = m.ops(part)?.model_solver()?.non_linear_solver_mut(id)?;
            f(s)
        })
    }
}

/// `NonLinearSolver` (PY-061).
#[pyclass(extends = PyParsable, subclass, frozen, name = "NonLinearSolver", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNonLinearSolver {
    model: ModelRef,
    id: String,
}

#[pymethods]
impl PyNonLinearSolver {
    /// `set(id, value)`: a real (`max_iterations` converted to an integer,
    /// as upstream) or a `SolveConvergenceCriteria`.
    fn set(slf: &Bound<'_, Self>, id: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let reg = slf.as_super().as_super().get();
        let v = if let Some(name) = enum_member_name(value)? {
            ParamValue::Enum(name)
        } else {
            let x: f64 = value.extract()?;
            if id == "max_iterations" {
                // upstream `int(val)` (truncation)
                #[allow(clippy::cast_possible_truncation)] // C++ `int(double)` semantics
                ParamValue::Int(i64::from(x as i32))
            } else {
                ParamValue::Real(x)
            }
        };
        reg.set_value(slf.py(), id, v)
    }
}

/// `NonLinearSolverNewtonRaphson` (PY-061).
#[pyclass(extends = PyNonLinearSolver, subclass, frozen, name = "NonLinearSolverNewtonRaphson", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNonLinearSolverNewtonRaphson;

#[pymethods]
impl PyNonLinearSolverNewtonRaphson {
    /// `getSparseSolver()`: the sparse solver object of the kind this
    /// solver uses (`SparseSolverEigen`, `SparseSolverMumps`).
    #[pyo3(name = "getSparseSolver")]
    fn get_sparse_solver<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let base = slf.as_super().get();
        let (_, sparse) = non_linear_kind(py, &base.model, &base.id)?;
        sparse_solver_object(py, sparse.unwrap_or(RsSparseSolverType::Eigen))
    }
}

/// `NonLinearSolverLumped` (PY-061).
#[pyclass(extends = PyNonLinearSolver, subclass, frozen, name = "NonLinearSolverLumped", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNonLinearSolverLumped;

/// `TimeStepSolver` (PY-062).
#[pyclass(frozen, name = "TimeStepSolver", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyTimeStepSolver {
    model: ModelRef,
    id: String,
}

#[pymethods]
impl PyTimeStepSolver {
    /// `getIntegrationScheme(dof_id)`: the scheme object (most derived
    /// bound class).
    #[pyo3(name = "getIntegrationScheme")]
    fn get_integration_scheme<'py>(
        &self,
        py: Python<'py>,
        dof_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        scheme_object(py, self.model.clone(), self.id.clone(), dof_id)
    }

    /// Time step of this solver (akantu-rs helper).
    #[pyo3(name = "getTimeStep")]
    fn get_time_step(&self, py: Python<'_>) -> PyResult<Real> {
        let id = self.id.clone();
        self.model
            .with_ops(py, move |m| m.time_step(Some(id.as_str())))
    }
}

// --- ModelSolver / Model ----------------------------------------------------------------------

/// `ModelSolver` (PY-066).
#[pyclass(extends = PySolverCallback, subclass, frozen, name = "ModelSolver", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyModelSolver {
    pub(crate) model: ModelRef,
}

impl PyModelSolver {
    /// The base initializer of a model object.
    pub(crate) fn init(model: ModelRef) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PySolverCallback::model_base()).add_subclass(Self { model })
    }

    /// The model's `Parsable` base (upstream `ModelSolver : Parsable`; a
    /// registry of type `_model` named after the model, no parameter until
    /// Python registers one).
    fn parsable(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let model = self.model.clone();
        self.model
            .h
            .parsable(py, self.model.part, || model.with_ops(py, |m| Ok(m.id())))
    }
}

fn sid(id: &str) -> Option<&str> {
    (!id.is_empty()).then_some(id)
}

#[pymethods]
impl PyModelSolver {
    /// `get` of the model's `Parsable` base.
    #[pyo3(name = "get", signature = (*args, **kwargs))]
    fn parsable_get<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "get", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `set` of the model's `Parsable` base.
    #[pyo3(name = "set", signature = (*args, **kwargs))]
    fn parsable_set<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "set", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getReal` of the model's `Parsable` base.
    #[pyo3(name = "getReal", signature = (*args, **kwargs))]
    fn parsable_get_real<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "getReal", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `setReal` of the model's `Parsable` base.
    #[pyo3(name = "setReal", signature = (*args, **kwargs))]
    fn parsable_set_real<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "setReal", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getBool` of the model's `Parsable` base.
    #[pyo3(name = "getBool", signature = (*args, **kwargs))]
    fn parsable_get_bool<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "getBool", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `setBool` of the model's `Parsable` base.
    #[pyo3(name = "setBool", signature = (*args, **kwargs))]
    fn parsable_set_bool<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "setBool", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getInt` of the model's `Parsable` base.
    #[pyo3(name = "getInt", signature = (*args, **kwargs))]
    fn parsable_get_int<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "getInt", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `setInt` of the model's `Parsable` base.
    #[pyo3(name = "setInt", signature = (*args, **kwargs))]
    fn parsable_set_int<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "setInt", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getString` of the model's `Parsable` base.
    #[pyo3(name = "getString", signature = (*args, **kwargs))]
    fn parsable_get_string<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "getString", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `setString` of the model's `Parsable` base.
    #[pyo3(name = "setString", signature = (*args, **kwargs))]
    fn parsable_set_string<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "setString", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getMatrix` of the model's `Parsable` base.
    #[pyo3(name = "getMatrix", signature = (*args, **kwargs))]
    fn parsable_get_matrix<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "getMatrix", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `setRandom` of the model's `Parsable` base.
    #[pyo3(name = "setRandom", signature = (*args, **kwargs))]
    fn parsable_set_random<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "setRandom", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `parseSection` of the model's `Parsable` base.
    #[pyo3(name = "parseSection", signature = (*args, **kwargs))]
    fn parsable_parse_section<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "parseSection", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `registerParamReal` of the model's `Parsable` base.
    #[pyo3(name = "registerParamReal", signature = (*args, **kwargs))]
    fn parsable_register_param_real<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.parsable(py)?
            .call_method(py, "registerParamReal", args, kwargs)
            .map(|o| o.into_bound(py))
    }

    /// `getNonLinearSolver(solver_id="")`: `NonLinearSolverNewtonRaphson`
    /// or `NonLinearSolverLumped` (pybind11 returns the most derived bound
    /// class).
    #[pyo3(name = "getNonLinearSolver", signature = (solver_id = String::new()))]
    fn get_non_linear_solver<'py>(
        &self,
        py: Python<'py>,
        solver_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (kind, _) = non_linear_kind(py, &self.model, &solver_id)?;
        let reg = PyParameterRegistry::external(Arc::new(SolverRegistry(
            self.model.clone(),
            solver_id.clone(),
        )));
        let base = PyClassInitializer::from(reg)
            .add_subclass(PyParsable)
            .add_subclass(PyNonLinearSolver {
                model: self.model.clone(),
                id: solver_id,
            });
        Ok(match kind {
            RsNonLinearSolverType::NewtonRaphson
            | RsNonLinearSolverType::NewtonRaphsonModified
            | RsNonLinearSolverType::Linear => {
                Bound::new(py, base.add_subclass(PyNonLinearSolverNewtonRaphson))?.into_any()
            }
            RsNonLinearSolverType::Lumped => {
                Bound::new(py, base.add_subclass(PyNonLinearSolverLumped))?.into_any()
            }
            _ => Bound::new(py, base)?.into_any(),
        })
    }

    /// `getTimeStepSolver(solver_id="")`.
    #[pyo3(name = "getTimeStepSolver", signature = (solver_id = String::new()))]
    fn get_time_step_solver(
        &self,
        py: Python<'_>,
        solver_id: String,
    ) -> PyResult<PyTimeStepSolver> {
        let id = solver_id.clone();
        self.model
            .with_ops(py, move |m| m.model_solver()?.solver(sid(&id)).map(|_| ()))?;
        Ok(PyTimeStepSolver {
            model: self.model.clone(),
            id: solver_id,
        })
    }

    /// `solveStep(solver_id="")` or `solveStep(callback, solver_id="")`
    /// with a Python `SolverCallback` / `InterceptSolverCallback`.
    #[pyo3(name = "solveStep", signature = (*args, **kwargs))]
    fn solve_step(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let (callback, id) = solve_step_args(args, kwargs)?;
        match callback {
            Some(cb) => solve_step_with_callback(py, &self.model, &cb, id),
            None => self.model.with_ops(py, move |m| m.solve_step(sid(&id))),
        }
    }
}

/// `Model` (PY-067).
#[pyclass(extends = PyModelSolver, subclass, frozen, name = "Model", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyModel;

/// The model handle of any model object.
pub(crate) fn model_of<T: pyo3::PyClass>(slf: &Bound<'_, T>) -> PyResult<ModelRef> {
    let any = slf.as_any();
    let ms = any.cast::<PyModelSolver>()?;
    Ok(ms.get().model.clone())
}

pub(crate) fn dump_args(
    args: &Bound<'_, pyo3::types::PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<(Option<String>, Option<Real>, Option<usize>)> {
    let get = |name: &str| -> PyResult<Option<Bound<'_, PyAny>>> {
        Ok(match kwargs {
            Some(k) => k.get_item(name)?,
            None => None,
        })
    };
    let mut items: Vec<Bound<'_, PyAny>> = args.iter().collect();
    let mut dumper = None;
    if let Some(first) = items.first()
        && first.is_instance_of::<PyString>()
    {
        dumper = Some(first.extract::<String>()?);
        items.remove(0);
    }
    if let Some(d) = get("dumper_name")? {
        dumper = Some(d.extract()?);
    }
    let (mut time, mut step) = (None, None);
    match items.as_slice() {
        [] => {}
        [s] => step = Some(s.extract()?),
        [t, s] => {
            time = Some(t.extract()?);
            step = Some(s.extract()?);
        }
        _ => return Err(PyTypeError::new_err("dump(): too many arguments")),
    }
    if let Some(t) = get("time")? {
        time = Some(t.extract()?);
    }
    if let Some(s) = get("step")? {
        step = Some(s.extract()?);
    }
    Ok((dumper, time, step))
}

#[pymethods]
impl PyModel {
    /// `getFEEngine(name="")`.
    #[pyo3(name = "getFEEngine", signature = (name = String::new()))]
    fn get_fe_engine(slf: &Bound<'_, Self>, name: String) -> PyResult<super::fe::PyFeEngine> {
        let model = model_of(slf)?;
        let n = name.clone();
        let lent = model
            .h
            .active_services(slf.py(), model.part)
            .py_err(slf.py())?;
        if let Some((services, cx)) = lent {
            // inside a Python law callback of this model: its services
            super::runtime::native(slf.py(), move || {
                services.with(cx, |sv| sv.fe_engine(&n).map(drop))?
            })?;
        } else {
            model.with_ops(slf.py(), move |m| {
                m.with_fe_engine(&n, false, &mut |_, _| Ok(()))
            })?;
        }
        Ok(super::fe::PyFeEngine {
            model,
            name,
            boundary: false,
        })
    }

    /// `getFEEngineBoundary(name="")`: the boundary engine (created on first
    /// use). Upstream binds this name to `Model::getFEEngine` and returns
    /// the regular engine (UD-533, category B: corrected).
    #[pyo3(name = "getFEEngineBoundary", signature = (name = String::new()))]
    fn get_fe_engine_boundary(
        slf: &Bound<'_, Self>,
        name: String,
    ) -> PyResult<super::fe::PyFeEngine> {
        let model = model_of(slf)?;
        let n = name.clone();
        model.with_ops(slf.py(), move |m| {
            m.with_fe_engine(&n, true, &mut |_, _| Ok(()))
        })?;
        Ok(super::fe::PyFeEngine {
            model,
            name,
            boundary: true,
        })
    }

    /// `setBaseName(name)` (the mesh's default dumper upstream: for the
    /// solid part of a `CouplerSolidContact` the coupler's series).
    #[pyo3(name = "setBaseName")]
    fn set_base_name(slf: &Bound<'_, Self>, name: String) -> PyResult<()> {
        model_of(slf)?.with_any(slf.py(), move |a, part| match (a, part) {
            (AnyModel::SolidContact(c), Part::Solid) => c.set_base_name(&name),
            (a, part) => a.ops(part)?.set_base_name(&name),
        })
    }

    /// `setDirectory(directory)` (default dumper; routed as `setBaseName`).
    #[pyo3(name = "setDirectory")]
    fn set_directory(slf: &Bound<'_, Self>, directory: std::path::PathBuf) -> PyResult<()> {
        model_of(slf)?.with_any(slf.py(), move |a, part| match (a, part) {
            (AnyModel::SolidContact(c), Part::Solid) => c.set_directory(&directory),
            (a, part) => a.ops(part)?.set_directory(&directory),
        })
    }

    /// `addDumpField(field_id)`: the field this model creates, on the
    /// mesh's default dumper upstream (for the solid part of a
    /// `CouplerSolidContact` the coupler's series).
    #[pyo3(name = "addDumpField")]
    fn add_dump_field(slf: &Bound<'_, Self>, field_id: String) -> PyResult<()> {
        model_of(slf)?.with_any(slf.py(), move |a, part| match (a, part) {
            (AnyModel::SolidContact(c), Part::Solid) => c.solid_add_dump_field(&field_id, false),
            (a, part) => a.ops(part)?.add_dump_field(&field_id),
        })
    }

    /// `addDumpFieldVector(field_id)` (padding; routed as `addDumpField`).
    #[pyo3(name = "addDumpFieldVector")]
    fn add_dump_field_vector(slf: &Bound<'_, Self>, field_id: String) -> PyResult<()> {
        model_of(slf)?.with_any(slf.py(), move |a, part| match (a, part) {
            (AnyModel::SolidContact(c), Part::Solid) => c.solid_add_dump_field(&field_id, true),
            (a, part) => a.ops(part)?.add_dump_field_vector(&field_id),
        })
    }

    #[pyo3(name = "setBaseNameToDumper")]
    fn set_base_name_to_dumper(
        slf: &Bound<'_, Self>,
        dumper_name: String,
        basename: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.set_base_name_to_dumper(&dumper_name, &basename)
        })
    }

    #[pyo3(name = "addDumpFieldVectorToDumper")]
    fn add_dump_field_vector_to_dumper(
        slf: &Bound<'_, Self>,
        dumper_name: String,
        field_id: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.add_dump_field_vector_to_dumper(&dumper_name, &field_id)
        })
    }

    #[pyo3(name = "addDumpFieldToDumper")]
    fn add_dump_field_to_dumper(
        slf: &Bound<'_, Self>,
        dumper_name: String,
        field_id: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.add_dump_field_to_dumper(&dumper_name, &field_id)
        })
    }

    /// `dump()`, `dump(step)`, `dump(time, step)`, `dump(dumper_name)`,
    /// `dump(dumper_name, step)`, `dump(dumper_name, time, step)`.
    #[pyo3(signature = (*args, **kwargs))]
    fn dump(
        slf: &Bound<'_, Self>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let (dumper, time, step) = dump_args(args, kwargs)?;
        // the parts of a coupler whose dump is upstream's `mesh.dump()` of
        // the mesh's default dumper, which the coupler fills: the contact
        // part of a `CouplerSolidContact` (`dump_contact`, TEST-215), its
        // solid part (the solid's dump event, then the coupler's series),
        // the cohesive solid of a
        // `CouplerSolidCohesiveContact` (`dump_solid`)
        model_of(slf)?.with_any(slf.py(), move |a, part| match (a, part) {
            (AnyModel::SolidContact(c), Part::Contact) => {
                c.dump_contact(dumper.as_deref(), time, step)
            }
            (AnyModel::SolidContact(c), Part::Solid) => match dumper.as_deref() {
                None => c.dump(time, step),
                Some(d) => c.dump_named(d, time, step),
            },
            (AnyModel::SolidCohesiveContact(c), Part::Solid) => {
                c.dump_solid(dumper.as_deref(), time, step)
            }
            (a, part) => {
                let m = a.ops(part)?;
                match dumper {
                    Some(d) => m.dump_named(&d, time, step),
                    None => m.dump(time, step),
                }
            }
        })
    }

    /// `dumpGroup(group_name)`.
    #[pyo3(name = "dumpGroup", signature = (group_name))]
    fn dump_group(slf: &Bound<'_, Self>, group_name: String) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| m.dump_group(&group_name))
    }

    /// `setGroupDirectory(directory, group_name)`.
    #[pyo3(name = "setGroupDirectory", signature = (directory, group_name))]
    fn set_group_directory(
        slf: &Bound<'_, Self>,
        directory: std::path::PathBuf,
        group_name: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.set_group_directory(&directory, &group_name)
        })
    }

    /// `setGroupBaseName(basename, group_name)`.
    #[pyo3(name = "setGroupBaseName", signature = (basename, group_name))]
    fn set_group_base_name(
        slf: &Bound<'_, Self>,
        basename: String,
        group_name: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.set_group_base_name(&basename, &group_name)
        })
    }

    /// `addDumpGroupField(field_id, group_name)`.
    #[pyo3(name = "addDumpGroupField", signature = (field_id, group_name))]
    fn add_dump_group_field(
        slf: &Bound<'_, Self>,
        field_id: String,
        group_name: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.add_dump_group_field(&field_id, &group_name)
        })
    }

    /// `addDumpGroupFieldVector(field_id, group_name)`.
    #[pyo3(name = "addDumpGroupFieldVector", signature = (field_id, group_name))]
    fn add_dump_group_field_vector(
        slf: &Bound<'_, Self>,
        field_id: String,
        group_name: String,
    ) -> PyResult<()> {
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.add_dump_group_field_vector(&field_id, &group_name)
        })
    }

    /// `initNewSolver(analysis_method, option=ModelSolverOptions())`
    /// (non-default options: not supported by the port's `initNewSolver`).
    #[pyo3(name = "initNewSolver", signature = (analysis_method, option = None))]
    fn init_new_solver(
        slf: &Bound<'_, Self>,
        analysis_method: AnalysisArg,
        option: Option<PyRef<'_, PyModelSolverOptions>>,
    ) -> PyResult<()> {
        if option.is_some_and(|o| {
            format!("{:?}", o.options) != format!("{:?}", ModelSolverOptions::default())
        }) {
            return Err(super::errors::exception(
                slf.py(),
                "initNewSolver with non-default solver options is not supported (use getNewSolver)"
                    .into(),
            ));
        }
        let method = analysis_method
            .0
            .ok_or_else(|| PyTypeError::new_err("invalid analysis method"))?;
        model_of(slf)?.with_ops(slf.py(), move |m| m.init_new_solver(method))
    }

    /// `getNewSolver(id, solver_options=ModelSolverOptions())`.
    #[pyo3(name = "getNewSolver", signature = (id, solver_options = None))]
    fn get_new_solver(
        slf: &Bound<'_, Self>,
        id: String,
        solver_options: Option<PyRef<'_, PyModelSolverOptions>>,
    ) -> PyResult<()> {
        let o = solver_options
            .map(|o| o.options.clone())
            .unwrap_or_default();
        model_of(slf)?.with_ops(slf.py(), move |m| m.get_new_solver(&id, &o))
    }

    /// `setIntegrationScheme(id, primal, scheme_type,
    /// solution_type=_not_defined)`.
    #[pyo3(name = "setIntegrationScheme", signature = (id, primal, scheme_type, solution_type = None))]
    fn set_integration_scheme(
        slf: &Bound<'_, Self>,
        id: String,
        primal: String,
        scheme_type: NamedEnumArg<IntegrationSchemeType, RsIntegrationSchemeType>,
        solution_type: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let st = solution_type
            .map(self::solution_type)
            .transpose()?
            .unwrap_or_default();
        let ty = scheme_type
            .0
            .ok_or_else(|| PyTypeError::new_err("invalid integration scheme type"))?;
        model_of(slf)?.with_ops(slf.py(), move |m| {
            m.set_integration_scheme(&id, &primal, ty, st)
        })
    }

    /// `getDOFManager()`.
    #[pyo3(name = "getDOFManager")]
    fn get_dof_manager(slf: &Bound<'_, Self>) -> PyResult<PyDofManager> {
        let model = model_of(slf)?;
        // inside a Python callback of a solve of this model: the running
        // solve's DOF manager
        super::solver::with_dofs(slf.py(), &model, |_| Ok(()))?;
        Ok(PyDofManager { model })
    }

    /// `assembleMatrix(matrix_id)` (the model's solver callback).
    #[pyo3(name = "assembleMatrix")]
    fn assemble_matrix(slf: &Bound<'_, Self>, matrix_id: String) -> PyResult<()> {
        let model = model_of(slf)?;
        on_model_callback(slf.py(), &model, move |a| a.assemble_matrix(&matrix_id))
    }
}

/// Register `register_model` classes.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyModelSolverOptions>()?;
    m.add_class::<PyModelOptions>()?;
    m.add_class::<PyModelSolver>()?;
    m.add_class::<PyModel>()?;
    Ok(())
}

/// Register the non-linear and time-step solver classes
/// (`register_dof_manager` part).
pub(crate) fn register_solvers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNonLinearSolver>()?;
    m.add_class::<PyNonLinearSolverNewtonRaphson>()?;
    m.add_class::<PyNonLinearSolverLumped>()?;
    m.add_class::<PyTimeStepSolver>()?;
    Ok(())
}
