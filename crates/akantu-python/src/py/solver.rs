// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_solver.cc (SparseMatrix,
// SparseMatrixAIJ, SolverVector, SparseSolver*, TermToAssemble,
// TermsToAssemble), python/py_dof_manager.cc (DOFManager, NonLinearSolver*,
// TimeStepSolver, the PySolverCallback / PyInterceptSolverCallback
// trampolines), python/py_integration_scheme.cc (IntegrationScheme,
// SolutionType, NewmarkBeta family).

//! The solver layer (PY-028, PY-044…047, PY-058, PY-060, PY-063, PY-064,
//! PY-090; group 1).
//!
//! **Objects by reference.** Upstream returns references to the objects a
//! model owns (`getDOFManager()`, `getMatrix("K")`, `getResidual()`,
//! `getIntegrationScheme(dof)`). The port returns small handles naming the
//! owner (the model object and the matrix / scheme name); every method
//! reaches the live object in a native section, so a handle always sees the
//! current state (a matrix re-created by the model is the same `"K"`).
//!
//! **Solver callbacks** (`model.solveStep(callback)`). A Python
//! `InterceptSolverCallback` subclass becomes the solve's
//! [`InterceptCallback`]; a plain `SolverCallback` subclass replaces the
//! model's callback. Every method the Python class overrides is called back
//! through the runtime trampoline ([`super::runtime::callback`]: refused
//! while a native guard is alive; a Python exception travels through native
//! code and is re-raised at `solveStep`). During such a call the model is
//! held by the running solve; its solver-callback interface
//! (`self.model.assembleMatrix(id)`, `assembleResidual()`,
//! `getMatrixType(id)`, …) and its DOF manager (`getDOFManager()`, the
//! matrices, the residual) reach the running solve through a phase-scoped
//! exclusive handle ([`crate::scoped::with_solver_access`]), valid only for
//! that call; array getters return views of the live arrays. Methods the
//! Python class does not override run natively (upstream: the C++ base
//! method).

use std::sync::{Arc, Mutex, PoisonError};

use akantu::core::enums::{IntegrationSchemeType as RsSchemeType, MatrixType as RsMatrixType};
use akantu::core::enums::{NonLinearSolverType as RsNlsType, SparseSolverType as RsSparseType};
use akantu::core::{Array, Error, GlobalIdx, Idx, Real, Result};
use akantu::linalg::terms::{TermToAssemble, TermsToAssemble};
use akantu::parser::param::{ParamValue, ParameterRegistry};
use akantu::solver::callback::{Delegate, InterceptCallback};
use akantu::solver::{DofManager, SolverCallback};
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};

use super::arrays::{input_f64, owned_numpy};
use super::enums::{EnumMeta, MatrixType};
use super::handle::{ModelKey, ModelRef};
use super::parser::{PyParameterRegistry, PyParsable, RegistryAccess};
use super::runtime::{IntoPy, callback, lending_callback, native};
use crate::foreign_view::numpy_export::Layout;
use crate::scoped::{SolverAccess, with_solver_access};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // only short, non-panicking critical sections
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `MatrixType` enum object of a Rust matrix type.
pub(crate) fn matrix_type_obj(py: Python<'_>, t: RsMatrixType) -> PyResult<Bound<'_, PyAny>> {
    let v = match t {
        RsMatrixType::Unsymmetric => 0,
        RsMatrixType::Symmetric => 1,
        // upstream `_mt_not_defined` is not bound; pybind11 prints `???`
        RsMatrixType::NotDefined => 2,
    };
    Ok(MatrixType::instance(py, v)?.into_any())
}

fn matrix_type_of(obj: &Bound<'_, PyAny>) -> PyResult<RsMatrixType> {
    let v: i64 = obj.call_method0("__int__")?.extract()?;
    Ok(match v {
        0 => RsMatrixType::Unsymmetric,
        1 => RsMatrixType::Symmetric,
        _ => RsMatrixType::NotDefined,
    })
}

// ---- solve accesses ------------------------------------------------------------

/// The model's own callback and its DOF manager (outside a solve).
struct CallbackAccess<'a> {
    cb: &'a mut dyn SolverCallback,
    dofs: &'a mut DofManager,
}

impl SolverAccess for CallbackAccess<'_> {
    fn matrix_type(&self, matrix_id: &str) -> Result<RsMatrixType> {
        Ok(self.cb.matrix_type(matrix_id))
    }
    fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.cb.assemble_matrix(self.dofs, matrix_id)
    }
    fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.cb.assemble_lumped_matrix(self.dofs, matrix_id)
    }
    fn assemble_residual(&mut self) -> Result<()> {
        self.cb.assemble_residual(self.dofs)
    }
    fn predictor(&mut self) -> Result<()> {
        self.cb.predictor(self.dofs)
    }
    fn corrector(&mut self) -> Result<()> {
        self.cb.corrector(self.dofs)
    }
    fn before_solve_step(&mut self) -> Result<()> {
        self.cb.before_solve_step(self.dofs)
    }
    fn after_solve_step(&mut self, converged: bool) -> Result<()> {
        self.cb.after_solve_step(self.dofs, converged)
    }
    fn dofs(&mut self) -> Result<&mut DofManager> {
        Ok(self.dofs)
    }
}

/// The model callback of a running intercepted solve (`super()`).
struct DelegateAccess<'a, 'b>(&'a mut Delegate<'b>);

impl SolverAccess for DelegateAccess<'_, '_> {
    fn matrix_type(&self, matrix_id: &str) -> Result<RsMatrixType> {
        Ok(self.0.matrix_type(matrix_id))
    }
    fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.0.assemble_matrix(matrix_id)
    }
    fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.0.assemble_lumped_matrix(matrix_id)
    }
    fn assemble_residual(&mut self) -> Result<()> {
        self.0.assemble_residual()
    }
    fn predictor(&mut self) -> Result<()> {
        self.0.predictor()
    }
    fn corrector(&mut self) -> Result<()> {
        self.0.corrector()
    }
    fn before_solve_step(&mut self) -> Result<()> {
        self.0.before_solve_step()
    }
    fn after_solve_step(&mut self, converged: bool) -> Result<()> {
        self.0.after_solve_step(converged)
    }
    fn dofs(&mut self) -> Result<&mut DofManager> {
        Ok(self.0.dofs())
    }
}

/// `getMatrixType` only (the model callback is lent shared).
struct TypeAccess<'a>(&'a dyn SolverCallback);

fn type_only(what: &str) -> Error {
    Error::invalid(format!(
        "{what} is not available inside getMatrixType (the solver only lends the model's \
         matrix types there)"
    ))
}

impl SolverAccess for TypeAccess<'_> {
    fn matrix_type(&self, matrix_id: &str) -> Result<RsMatrixType> {
        Ok(self.0.matrix_type(matrix_id))
    }
    fn assemble_matrix(&mut self, _matrix_id: &str) -> Result<()> {
        Err(type_only("assembleMatrix"))
    }
    fn assemble_lumped_matrix(&mut self, _matrix_id: &str) -> Result<()> {
        Err(type_only("assembleLumpedMatrix"))
    }
    fn assemble_residual(&mut self) -> Result<()> {
        Err(type_only("assembleResidual"))
    }
    fn predictor(&mut self) -> Result<()> {
        Err(type_only("predictor"))
    }
    fn corrector(&mut self) -> Result<()> {
        Err(type_only("corrector"))
    }
    fn before_solve_step(&mut self) -> Result<()> {
        Err(type_only("beforeSolveStep"))
    }
    fn after_solve_step(&mut self, _converged: bool) -> Result<()> {
        Err(type_only("afterSolveStep"))
    }
    fn dofs(&mut self) -> Result<&mut DofManager> {
        Err(type_only("the DOF manager"))
    }
}

/// A model constructor's `dof_manager` argument (D-078):
/// upstream's binding cannot use one either — pybind11 raises
/// `RuntimeError` ("Unable to cast from non-held to held instance") for the
/// `DOFManager` a model returns, the only kind Python can obtain — so the
/// port refuses it with the same exception type; any other object is an
/// incompatible argument (`TypeError`), `None` is the default.
pub(crate) fn refuse_user_dof_manager(dof_manager: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
    match dof_manager {
        None => Ok(()),
        Some(d) if d.is_none() => Ok(()),
        Some(d) if d.is_instance_of::<PyDofManager>() => Err(PyRuntimeError::new_err(
            "Unable to cast from non-held to held instance (T& to Holder<T>): a user-provided \
             DOFManager cannot be used by a model (upstream's binding raises this error too; \
             D-078)",
        )),
        Some(_) => Err(PyTypeError::new_err(
            "incompatible constructor arguments: dof_manager must be a DOFManager or None",
        )),
    }
}

/// Run `f` on the solver-callback interface of `model`: the running
/// solve's access inside a Python callback of a solve of this model,
/// otherwise the model's own callback (native section).
pub(crate) fn on_model_callback<R: Send>(
    py: Python<'_>,
    model: &ModelRef,
    f: impl FnOnce(&mut dyn SolverAccess) -> Result<R> + Send,
) -> PyResult<R> {
    if let Some((acc, cx)) = model.active_access(py).py_err(py)? {
        return native(py, move || acc.with_mut(cx, |a| f(a))?);
    }
    model.with_ops(py, move |ops| {
        let mut f = Some(f);
        let mut out = None;
        ops.with_callback(&mut |cb, dofs| {
            let f = f
                .take()
                .ok_or_else(|| Error::invalid("internal error: callback reused"))?;
            out = Some(f(&mut CallbackAccess { cb, dofs })?);
            Ok(())
        })?;
        out.ok_or_else(|| Error::invalid("internal error: model callback not run"))
    })
}

/// Run `f` on the DOF manager of `model` (the running solve's inside a
/// Python callback of a solve of this model).
pub(crate) fn with_dofs<R: Send>(
    py: Python<'_>,
    model: &ModelRef,
    f: impl FnOnce(&mut DofManager) -> Result<R> + Send,
) -> PyResult<R> {
    if let Some((acc, cx)) = model.active_access(py).py_err(py)? {
        return native(py, move || acc.with_mut(cx, |a| f(a.dofs()?))?);
    }
    model.with_ops(py, move |ops| f(&mut ops.model_solver()?.dof_manager))
}

// ---- IntegrationScheme ------------------------------------------------------------

/// `IntegrationScheme.SolutionType` (PY-028; nested enum, members exported
/// into `IntegrationScheme`).
#[pyclass(frozen, name = "SolutionType", module = "akantu.py11_akantu")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PySolutionType {
    pub(crate) value: i64,
}

const SOLUTION_TYPE_MEMBERS: &[(&str, i64)] = &[
    ("_not_defined", -1),
    ("_displacement", 0),
    ("_temperature", 0),
    ("_damage", 0),
    ("_velocity", 1),
    ("_temperature_rate", 1),
    ("_acceleration", 2),
];

fn solution_type_name(v: i64) -> &'static str {
    SOLUTION_TYPE_MEMBERS
        .iter()
        .find(|(_, x)| *x == v)
        .map_or("???", |(n, _)| n)
}

#[pymethods]
impl PySolutionType {
    #[new]
    fn new(value: i64) -> Self {
        Self { value }
    }
    fn __int__(&self) -> i64 {
        self.value
    }
    fn __index__(&self) -> i64 {
        self.value
    }
    fn __hash__(&self) -> isize {
        isize::try_from(self.value).unwrap_or(isize::MAX)
    }
    fn __repr__(&self) -> String {
        format!(
            "<SolutionType.{}: {}>",
            solution_type_name(self.value),
            self.value
        )
    }
    fn __str__(&self) -> String {
        format!("SolutionType.{}", solution_type_name(self.value))
    }
    #[getter]
    fn name(&self) -> &'static str {
        solution_type_name(self.value)
    }
    #[getter]
    fn value(&self) -> i64 {
        self.value
    }
    /// pybind11 plain-enum equality (convertible to `int`).
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .call_method0("__int__")
            .and_then(|v| v.extract::<i64>())
            .is_ok_and(|v| v == self.value)
    }
    fn __ne__(&self, other: &Bound<'_, PyAny>) -> bool {
        !self.__eq__(other)
    }
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> (Bound<'py, PyType>, (i64,)) {
        (slf.get_type(), (slf.get().value,))
    }
}

/// What a Python scheme object designates.
#[derive(Debug)]
enum SchemeRef {
    /// A scheme of a time-step solver of a model.
    Solver {
        model: ModelRef,
        solver_id: String,
        dof_id: String,
    },
    /// A scheme constructed from Python (`NewmarkBeta(dof_manager, id,
    /// alpha, beta)`): only its parameters are reachable.
    Own(Mutex<akantu::solver::NewmarkBeta>),
}

/// `IntegrationScheme` (PY-058).
#[pyclass(
    subclass,
    frozen,
    name = "IntegrationScheme",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyIntegrationScheme {
    target: SchemeRef,
}

impl PyIntegrationScheme {
    fn with_params<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut dyn ParameterRegistry) -> Result<R> + Send,
    ) -> PyResult<R> {
        match &self.target {
            SchemeRef::Own(s) => {
                let mut s = lock(s);
                f(&mut *s).map_err(|e| super::runtime::to_py(py, e))
            }
            SchemeRef::Solver {
                model,
                solver_id,
                dof_id,
            } => {
                let (sid, dof) = (solver_id.clone(), dof_id.clone());
                model.with_ops(py, move |ops| {
                    let ms = ops.model_solver()?;
                    let tss = ms.solver_mut((!sid.is_empty()).then_some(sid.as_str()))?;
                    f(tss.integration_scheme_mut(&dof)?)
                })
            }
        }
    }

    fn real(&self, py: Python<'_>, name: &'static str) -> PyResult<Real> {
        self.with_params(py, move |p| p.get_param(name)?.as_real())
    }

    fn set_real(&self, py: Python<'_>, name: &'static str, v: Real) -> PyResult<()> {
        self.with_params(py, move |p| p.set_param(name, ParamValue::Real(v)))
    }
}

/// `IntegrationScheme2ndOrder`.
#[pyclass(extends = PyIntegrationScheme, subclass, frozen, name = "IntegrationScheme2ndOrder", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyIntegrationScheme2ndOrder;

/// `NewmarkBeta(dof_manager, id, alpha, beta)` with `alpha`, `beta`
/// properties.
#[pyclass(extends = PyIntegrationScheme2ndOrder, subclass, frozen, name = "NewmarkBeta", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNewmarkBeta;

#[pymethods]
impl PyNewmarkBeta {
    /// Upstream constructor: a scheme for the DOF family `id` of
    /// `dof_manager` (not attached to any solver, as upstream).
    #[new]
    #[pyo3(signature = (dof_manager, id, alpha, beta))]
    fn new(
        dof_manager: &Bound<'_, PyDofManager>,
        id: String,
        alpha: Real,
        beta: Real,
    ) -> PyClassInitializer<Self> {
        let _ = dof_manager;
        PyClassInitializer::from(PyIntegrationScheme {
            target: SchemeRef::Own(Mutex::new(akantu::solver::NewmarkBeta::new(
                &id, alpha, beta,
            ))),
        })
        .add_subclass(PyIntegrationScheme2ndOrder)
        .add_subclass(Self)
    }

    #[getter]
    fn alpha(slf: &Bound<'_, Self>) -> PyResult<Real> {
        slf.as_super().as_super().get().real(slf.py(), "alpha")
    }

    #[setter]
    fn set_alpha(slf: &Bound<'_, Self>, v: Real) -> PyResult<()> {
        slf.as_super()
            .as_super()
            .get()
            .set_real(slf.py(), "alpha", v)
    }

    #[getter]
    fn beta(slf: &Bound<'_, Self>) -> PyResult<Real> {
        slf.as_super().as_super().get().real(slf.py(), "beta")
    }

    #[setter]
    fn set_beta(slf: &Bound<'_, Self>, v: Real) -> PyResult<()> {
        slf.as_super()
            .as_super()
            .get()
            .set_real(slf.py(), "beta", v)
    }
}

macro_rules! newmark_subclass {
    ($rust:ident, $name:literal) => {
        #[doc = concat!("`", $name, "` (a `NewmarkBeta`).")]
        #[pyclass(extends = PyNewmarkBeta, subclass, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;
    };
}

newmark_subclass!(PyCentralDifference, "CentralDifference");
newmark_subclass!(PyTrapezoidalRule2, "TrapezoidalRule2");
newmark_subclass!(PyFoxGoodwin, "FoxGoodwin");
newmark_subclass!(PyLinearAceleration, "LinearAceleration");

/// The Python object of the scheme of `dof_id` in solver `solver_id`: the
/// most derived bound class (pybind11 polymorphic return).
pub(crate) fn scheme_object<'py>(
    py: Python<'py>,
    model: ModelRef,
    solver_id: String,
    dof_id: String,
) -> PyResult<Bound<'py, PyAny>> {
    let (sid, dof) = (solver_id.clone(), dof_id.clone());
    let kind = model.with_ops(py, move |ops| {
        let tss = ops
            .model_solver()?
            .solver((!sid.is_empty()).then_some(sid.as_str()))?;
        Ok(tss.integration_scheme(&dof)?.kind())
    })?;
    let base = PyClassInitializer::from(PyIntegrationScheme {
        target: SchemeRef::Solver {
            model,
            solver_id,
            dof_id,
        },
    });
    Ok(match kind {
        RsSchemeType::CentralDifference => Bound::new(
            py,
            base_nb(base_2nd(base)).add_subclass(PyCentralDifference),
        )?
        .into_any(),
        RsSchemeType::TrapezoidalRule2 => {
            Bound::new(py, base_nb(base_2nd(base)).add_subclass(PyTrapezoidalRule2))?.into_any()
        }
        RsSchemeType::FoxGoodwin => {
            Bound::new(py, base_nb(base_2nd(base)).add_subclass(PyFoxGoodwin))?.into_any()
        }
        RsSchemeType::LinearAcceleration => Bound::new(
            py,
            base_nb(base_2nd(base)).add_subclass(PyLinearAceleration),
        )?
        .into_any(),
        RsSchemeType::NewmarkBeta => Bound::new(py, base_nb(base_2nd(base)))?.into_any(),
        // first-order and pseudo-time schemes: `IntegrationScheme1stOrder`
        // and its subclasses are not bound upstream
        _ => Bound::new(py, base)?.into_any(),
    })
}

fn base_2nd(
    b: PyClassInitializer<PyIntegrationScheme>,
) -> PyClassInitializer<PyIntegrationScheme2ndOrder> {
    b.add_subclass(PyIntegrationScheme2ndOrder)
}

fn base_nb(
    b: PyClassInitializer<PyIntegrationScheme2ndOrder>,
) -> PyClassInitializer<PyNewmarkBeta> {
    b.add_subclass(PyNewmarkBeta)
}

// ---- SparseMatrix ----------------------------------------------------------------------

/// `SparseMatrix` (PY-044): a matrix of a DOF manager, by name.
#[pyclass(subclass, frozen, name = "SparseMatrix", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySparseMatrix {
    model: ModelRef,
    name: String,
}

impl PySparseMatrix {
    fn dofs<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut DofManager, &str) -> Result<R> + Send,
    ) -> PyResult<R> {
        let name = self.name.clone();
        with_dofs(py, &self.model, move |d| f(d, &name))
    }
}

fn kwarg<'py>(kwargs: Option<&Bound<'py, PyDict>>, k: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    Ok(match kwargs {
        Some(d) => d.get_item(k)?,
        None => None,
    })
}

fn gidx(py: Python<'_>, i: i64) -> PyResult<GlobalIdx> {
    if i < 0 {
        return Err(super::errors::exception(
            py,
            format!("negative matrix index {i}"),
        ));
    }
    Ok(i)
}

#[pymethods]
impl PySparseMatrix {
    #[pyo3(name = "getMatrixType")]
    fn get_matrix_type<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let t = self.dofs(py, |d, n| Ok(d.matrix(n)?.matrix_type()))?;
        matrix_type_obj(py, t)
    }

    fn size(&self, py: Python<'_>) -> PyResult<GlobalIdx> {
        self.dofs(py, |d, n| Ok(d.matrix(n)?.size()))
    }

    fn zero(&self, py: Python<'_>) -> PyResult<()> {
        self.dofs(py, |d, n| d.zero_matrix(n))
    }

    #[pyo3(name = "saveProfile")]
    fn save_profile(&self, py: Python<'_>, filename: std::path::PathBuf) -> PyResult<()> {
        self.dofs(py, move |d, n| d.matrix(n)?.save_profile(&filename))
    }

    #[pyo3(name = "saveMatrix")]
    fn save_matrix(&self, py: Python<'_>, filename: std::path::PathBuf) -> PyResult<()> {
        self.dofs(py, move |d, n| {
            d.matrix(n)?.save_matrix(&filename).map(drop)
        })
    }

    /// `add(i, j)` (profile entry), `add(i, j, value)`, `add(A, alpha=1.)`.
    #[pyo3(signature = (*args, **kwargs))]
    fn add<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<()> {
        let kw = |k: &str| kwarg(kwargs, k);
        let first = match args.get_item(0) {
            Ok(a) => Some(a),
            Err(_) => kw("A")?,
        };
        if let Some(a) = first.as_ref()
            && let Ok(other) = a.cast::<PySparseMatrix>()
        {
            let alpha: Real = match args.get_item(1) {
                Ok(v) => v.extract()?,
                Err(_) => kw("alpha")?
                    .map(|v| v.extract())
                    .transpose()?
                    .unwrap_or(1.0),
            };
            let o = other.get();
            if !Arc::ptr_eq(&o.model.h, &self.model.h) || o.model.part != self.model.part {
                return Err(super::errors::exception(
                    py,
                    "SparseMatrix.add(A): A must be a matrix of the same DOF manager".into(),
                ));
            }
            let src = o.name.clone();
            return self.dofs(py, move |d, n| d.add_matrix(n, &src, alpha));
        }
        match args.len() {
            2 => {
                let (i, j): (i64, i64) =
                    (args.get_item(0)?.extract()?, args.get_item(1)?.extract()?);
                let (i, j) = (gidx(py, i)?, gidx(py, j)?);
                self.dofs(py, move |d, n| {
                    d.matrix_mut(n)?.add_to_profile(i, j).map(drop)
                })
            }
            3 => {
                let (i, j): (i64, i64) =
                    (args.get_item(0)?.extract()?, args.get_item(1)?.extract()?);
                let v: Real = args.get_item(2)?.extract()?;
                let (i, j) = (gidx(py, i)?, gidx(py, j)?);
                self.dofs(py, move |d, n| d.matrix_mut(n)?.add(i, j, v))
            }
            _ => Err(PyTypeError::new_err(
                "add(): expected (i, j), (i, j, value) or (A, alpha=1.)",
            )),
        }
    }

    #[pyo3(name = "isFinite")]
    fn is_finite(&self, py: Python<'_>) -> PyResult<bool> {
        self.dofs(py, |d, n| Ok(d.matrix(n)?.is_finite()))
    }

    /// `getRelease()`: the value release.
    #[pyo3(name = "getRelease")]
    fn get_release(&self, py: Python<'_>) -> PyResult<u64> {
        self.dofs(py, |d, n| d.matrix_release(n))
    }

    fn __call__(&self, py: Python<'_>, i: i64, j: i64) -> PyResult<Real> {
        let (i, j) = (gidx(py, i)?, gidx(py, j)?);
        self.dofs(py, move |d, n| Ok(d.matrix(n)?.get(i, j)))
    }
}

/// `SparseMatrixAIJ` (PY-044): `getIRN`/`getJCN` (1-based, as upstream),
/// `getA` — read-only copies (upstream: references to the matrix arrays).
#[pyclass(extends = PySparseMatrix, subclass, frozen, name = "SparseMatrixAIJ", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySparseMatrixAij;

fn read_only(a: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    a.call_method1("setflags", (false,))?;
    Ok(a)
}

#[pymethods]
impl PySparseMatrixAij {
    #[pyo3(name = "getIRN")]
    fn get_irn<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let v = slf.as_super().get().dofs(py, |d, n| {
            let m = d.matrix(n)?;
            Ok(m.coo_one_based_i32(u64::MAX)?.0)
        })?;
        let rows = v.len();
        read_only(owned_numpy(py, v, rows, 1, Layout::Rows)?)
    }

    #[pyo3(name = "getJCN")]
    fn get_jcn<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let v = slf.as_super().get().dofs(py, |d, n| {
            let m = d.matrix(n)?;
            Ok(m.coo_one_based_i32(u64::MAX)?.1)
        })?;
        let rows = v.len();
        read_only(owned_numpy(py, v, rows, 1, Layout::Rows)?)
    }

    #[pyo3(name = "getA")]
    fn get_a<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let v = slf
            .as_super()
            .get()
            .dofs(py, |d, n| Ok(d.matrix(n)?.values().to_vec()))?;
        let rows = v.len();
        read_only(owned_numpy(py, v, rows, 1, Layout::Rows)?)
    }
}

/// `SparseMatrixPETSc` (registered as upstream's PETSc build does; the
/// port has no PETSc matrix backend, DOF-015: never returned).
#[pyclass(extends = PySparseMatrix, subclass, frozen, name = "SparseMatrixPETSc", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySparseMatrixPetsc;

/// The `SparseMatrixAIJ` object of matrix `name` of `model`'s DOF manager
/// (checked to exist).
pub(crate) fn matrix_object<'py>(
    py: Python<'py>,
    model: ModelRef,
    name: String,
) -> PyResult<Bound<'py, PySparseMatrixAij>> {
    let n = name.clone();
    with_dofs(py, &model, move |d| d.matrix(&n).map(drop))?;
    Bound::new(
        py,
        PyClassInitializer::from(PySparseMatrix { model, name }).add_subclass(PySparseMatrixAij),
    )
}

// ---- SolverVector -------------------------------------------------------------------------

/// `SolverVector` (PY-045): the residual of a DOF manager.
#[pyclass(frozen, name = "SolverVector", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySolverVector {
    model: ModelRef,
}

#[pymethods]
impl PySolverVector {
    /// `getValues()`: an `(n, 1)` zero-copy, writable NumPy view of the
    /// residual's values, as upstream (`py_solver.cc:85-92`,
    /// `reference_internal`, "Is not copied"): a view kept
    /// across later calls follows every assembly and zeroing of the
    /// residual, and writes land in the residual. Across a renumbering of
    /// the equations an earlier view follows UD-085's retained-view rule:
    /// attached while the residual grows within its
    /// capacity (it shows the old prefix), retired by a growth beyond it or
    /// any shrink, then stale but safe with the values it had (D-047;
    /// upstream's view would dangle).
    /// Inside a Python callback of a running solve the view is taken
    /// through the solve access the solve lent.
    #[pyo3(name = "getValues")]
    fn get_values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if let Some((acc, cx)) = self.model.active_access(py).py_err(py)? {
            let h = Arc::clone(&self.model.h);
            let part = self.model.part;
            let v = native(py, move || {
                acc.with_mut(cx, |a| h.export_residual_of(part, a.dofs()?))?
            })?;
            return super::exports::into_numpy(py, v, Layout::Rows);
        }
        self.model.view::<f64>(py, ModelKey::Residual)
    }

    #[pyo3(name = "isDistributed")]
    fn is_distributed(&self, py: Python<'_>) -> PyResult<bool> {
        with_dofs(py, &self.model, |d| Ok(d.residual().is_distributed()))
    }
}

// ---- SparseSolver -------------------------------------------------------------------------

/// The (parameter-less) registry of a sparse solver: the port's direct
/// solvers have no Python-tunable parameters.
#[derive(Debug)]
struct NoParams(&'static str);

impl RegistryAccess for NoParams {
    fn with_registry(
        &self,
        _prepared: &super::parser::Prepared,
        _f: &mut dyn FnMut(&mut dyn ParameterRegistry) -> Result<()>,
    ) -> Result<()> {
        Err(Error::Unsupported(format!(
            "parameters of the {} sparse solver",
            self.0
        )))
    }
}

/// `SparseSolver` (PY-046).
#[pyclass(extends = PyParsable, subclass, frozen, name = "SparseSolver", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PySparseSolver;

macro_rules! sparse_solver_class {
    ($rust:ident, $name:literal) => {
        #[doc = concat!("`", $name, "` (PY-046).")]
        #[pyclass(extends = PySparseSolver, subclass, frozen, name = $name, module = "akantu.py11_akantu")]
        #[derive(Debug)]
        pub(crate) struct $rust;
    };
}

sparse_solver_class!(PySparseSolverEigen, "SparseSolverEigen");
sparse_solver_class!(PySparseSolverMumps, "SparseSolverMumps");
sparse_solver_class!(PySparseSolverPetsc, "SparseSolverPETSc");

/// The sparse solver object of a kind.
pub(crate) fn sparse_solver_object(
    py: Python<'_>,
    kind: RsSparseType,
) -> PyResult<Bound<'_, PyAny>> {
    let label = match kind {
        RsSparseType::Mumps => "MUMPS",
        RsSparseType::Petsc => "PETSc",
        _ => "Eigen",
    };
    let base = PyClassInitializer::from(PyParameterRegistry::external(Arc::new(NoParams(label))))
        .add_subclass(PyParsable)
        .add_subclass(PySparseSolver);
    Ok(match kind {
        RsSparseType::Mumps => Bound::new(py, base.add_subclass(PySparseSolverMumps))?.into_any(),
        RsSparseType::Petsc => Bound::new(py, base.add_subclass(PySparseSolverPetsc))?.into_any(),
        _ => Bound::new(py, base.add_subclass(PySparseSolverEigen))?.into_any(),
    })
}

/// The non-linear solver kind of `model`'s solver `solver_id`.
pub(crate) fn non_linear_kind(
    py: Python<'_>,
    model: &ModelRef,
    solver_id: &str,
) -> PyResult<(RsNlsType, Option<RsSparseType>)> {
    let id = solver_id.to_owned();
    model.with_ops(py, move |ops| {
        let nls = ops
            .model_solver()?
            .non_linear_solver_mut((!id.is_empty()).then_some(id.as_str()))?;
        let kind = nls.kind();
        let sparse = nls
            .as_newton_raphson_mut()
            .and_then(|nr| nr.sparse_solver_kind());
        Ok((kind, sparse))
    })
}

// ---- TermsToAssemble ------------------------------------------------------------------

/// `TermToAssemble(i, j)` (PY-047): a standalone term (also what
/// `terms(i, j, val)` returns, a copy as upstream, UD-831).
#[pyclass(frozen, name = "TermToAssemble", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyTermToAssemble {
    local: Mutex<TermToAssemble>,
}

#[pymethods]
impl PyTermToAssemble {
    #[new]
    fn new(i: Idx, j: Idx) -> Self {
        Self {
            local: Mutex::new(TermToAssemble { i, j, value: 0.0 }),
        }
    }

    fn __iadd__(&self, value: Real) {
        lock(&self.local).value += value;
    }

    #[getter]
    fn i(&self) -> Idx {
        lock(&self.local).i
    }

    #[getter]
    fn j(&self) -> Idx {
        lock(&self.local).j
    }

    /// Current value (akantu-rs helper; upstream converts the term to
    /// `Real` in C++ only).
    fn __float__(&self) -> Real {
        lock(&self.local).value
    }
}

/// `TermsToAssemble(dof_id_m, dof_id_n)` (PY-047).
#[pyclass(frozen, name = "TermsToAssemble", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyTermsToAssemble {
    inner: Mutex<TermsToAssemble>,
}

impl PyTermsToAssemble {
    /// A copy of the terms (taken before a native section).
    pub(crate) fn snapshot(&self) -> TermsToAssemble {
        lock(&self.inner).clone()
    }
}

#[pymethods]
impl PyTermsToAssemble {
    #[new]
    fn new(dof_id_m: String, dof_id_n: String) -> Self {
        Self {
            inner: Mutex::new(TermsToAssemble::new(dof_id_m, dof_id_n)),
        }
    }

    #[pyo3(name = "getDOFIdM")]
    fn get_dof_id_m(&self) -> String {
        lock(&self.inner).dof_id_m.clone()
    }

    #[pyo3(name = "getDOFIdN")]
    fn get_dof_id_n(&self) -> String {
        lock(&self.inner).dof_id_n.clone()
    }

    /// `terms(i, j, val=0.)`: append a term `(i, j)` holding `val` and
    /// return a **copy** of it (UD-831, category C: upstream's lambda
    /// returns the term by value although it asks for
    /// `return_value_policy::reference`, so `+=` on the result does not
    /// change the container — reproduced).
    #[pyo3(signature = (i, j, val = 0.0))]
    fn __call__(&self, i: Idx, j: Idx, val: Real) -> PyTermToAssemble {
        lock(&self.inner).assign(i, j, val);
        PyTermToAssemble {
            local: Mutex::new(TermToAssemble { i, j, value: val }),
        }
    }

    /// Number of terms (akantu-rs helper).
    fn __len__(&self) -> usize {
        lock(&self.inner).len()
    }
}

// ---- DOFManager ------------------------------------------------------------------------

/// `DOFManager` (PY-060) of a model.
#[pyclass(frozen, name = "DOFManager", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDofManager {
    pub(crate) model: ModelRef,
}

/// A float input array with its component count (`(n,)` = one column).
fn input_array(obj: &Bound<'_, PyAny>) -> PyResult<Array<Real>> {
    let (values, shape) = input_f64(obj)?;
    let nbc = match shape.as_slice() {
        [_, c] => *c,
        _ => 1,
    };
    Array::from_vec(values, nbc.max(1)).map_err(|e| super::runtime::to_py(obj.py(), e))
}

#[pymethods]
impl PyDofManager {
    /// `getMatrix(name)`: the `SparseMatrixAIJ` of the DOF manager.
    #[pyo3(name = "getMatrix")]
    fn get_matrix<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PySparseMatrixAij>> {
        matrix_object(py, self.model.clone(), name)
    }

    /// `getNewMatrix(name, matrix_to_copy_id)`.
    #[pyo3(name = "getNewMatrix")]
    fn get_new_matrix<'py>(
        &self,
        py: Python<'py>,
        name: String,
        matrix_to_copy_id: String,
    ) -> PyResult<Bound<'py, PySparseMatrixAij>> {
        let n = name.clone();
        with_dofs(py, &self.model, move |d| {
            d.new_matrix_copy(&n, &matrix_to_copy_id)
        })?;
        matrix_object(py, self.model.clone(), name)
    }

    /// `getResidual()`.
    #[pyo3(name = "getResidual")]
    fn get_residual(&self) -> PySolverVector {
        PySolverVector {
            model: self.model.clone(),
        }
    }

    /// `getArrayPerDOFs(dof_id, global_vector, local_array)`: the family's
    /// entries of the vector written into `local_array` (a NumPy array of
    /// the family's shape).
    #[pyo3(name = "getArrayPerDOFs")]
    fn get_array_per_dofs(
        &self,
        py: Python<'_>,
        dof_id: String,
        global: &Bound<'_, PySolverVector>,
        local: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let shape: Vec<usize> = local.getattr("shape")?.extract()?;
        let nbc = match shape.as_slice() {
            [_, c] => *c,
            _ => 1,
        };
        // the values of `global_vector` (the residual of
        // the DOF manager it belongs to, which may be another model's; its
        // size must match this DOF manager's equations)
        let g = with_dofs(py, &global.get().model, |d| {
            Ok(d.residual().values().to_vec())
        })?;
        let (values, rows) = with_dofs(py, &self.model, move |d| {
            let mut out = Array::<Real>::zeros(0, nbc.max(1));
            d.get_array_per_dofs(&dof_id, &g, &mut out)?;
            Ok((out.as_slice().to_vec(), out.len()))
        })?;
        super::fe::write_out(local, values, rows, nbc)
    }

    #[pyo3(name = "hasMatrix", signature = (name))]
    fn has_matrix(&self, py: Python<'_>, name: String) -> PyResult<bool> {
        with_dofs(py, &self.model, move |d| Ok(d.has_matrix(&name)))
    }

    /// `assembleToResidual(dof_id, array_to_assemble, scale_factor=1.)`.
    #[pyo3(name = "assembleToResidual", signature = (dof_id, array_to_assemble, scale_factor = 1.0))]
    fn assemble_to_residual(
        &self,
        py: Python<'_>,
        dof_id: String,
        array_to_assemble: &Bound<'_, PyAny>,
        scale_factor: Real,
    ) -> PyResult<()> {
        let a = input_array(array_to_assemble)?;
        with_dofs(py, &self.model, move |d| {
            d.assemble_to_residual(&dof_id, a.view(), scale_factor)
        })
    }

    /// `assembleToLumpedMatrix(dof_id, array_to_assemble, lumped_mtx,
    /// scale_factor=1.)`.
    #[pyo3(name = "assembleToLumpedMatrix", signature = (dof_id, array_to_assemble, lumped_mtx, scale_factor = 1.0))]
    fn assemble_to_lumped_matrix(
        &self,
        py: Python<'_>,
        dof_id: String,
        array_to_assemble: &Bound<'_, PyAny>,
        lumped_mtx: String,
        scale_factor: Real,
    ) -> PyResult<()> {
        let a = input_array(array_to_assemble)?;
        with_dofs(py, &self.model, move |d| {
            d.assemble_to_lumped_matrix(&dof_id, a.view(), &lumped_mtx, scale_factor)
        })
    }

    /// `assemblePreassembledMatrix(matrix_id, terms)`.
    #[pyo3(name = "assemblePreassembledMatrix", signature = (matrix_id, terms))]
    fn assemble_preassembled_matrix(
        &self,
        py: Python<'_>,
        matrix_id: String,
        terms: &Bound<'_, PyTermsToAssemble>,
    ) -> PyResult<()> {
        let t = terms.get().snapshot();
        with_dofs(py, &self.model, move |d| {
            d.assemble_preassembled_matrix(&matrix_id, &t)
        })
    }

    #[pyo3(name = "zeroResidual")]
    fn zero_residual(&self, py: Python<'_>) -> PyResult<()> {
        with_dofs(py, &self.model, |d| {
            d.zero_residual();
            Ok(())
        })
    }
}

// ---- SolverCallback / InterceptSolverCallback ---------------------------------------------

/// `SolverCallback(dof_manager)` (PY-063): base of every model class and of
/// Python solver callbacks.
#[pyclass(
    subclass,
    frozen,
    name = "SolverCallback",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PySolverCallback {
    /// The DOF manager given to the constructor (Python subclasses).
    dofs: Mutex<Option<Py<PyDofManager>>>,
}

impl PySolverCallback {
    /// The base part of a model object.
    pub(crate) fn model_base() -> Self {
        Self {
            dofs: Mutex::new(None),
        }
    }
}

/// What the receiver of a `SolverCallback` method is.
enum Receiver {
    /// A model object (its own callback, or the running solve's access).
    Model(ModelRef),
    /// An `InterceptSolverCallback`: its wrapped callback.
    Intercept(Py<PyAny>),
    /// A plain Python `SolverCallback` subclass (upstream: pure virtual
    /// methods).
    Plain,
}

fn receiver(slf: &Bound<'_, PySolverCallback>) -> PyResult<Receiver> {
    if let Ok(m) = slf.as_any().cast::<super::model::PyModelSolver>() {
        return Ok(Receiver::Model(m.get().model.clone()));
    }
    if let Ok(i) = slf.as_any().cast::<PyInterceptSolverCallback>() {
        let t = lock(&i.get().target)
            .as_ref()
            .map(|t| t.clone_ref(slf.py()));
        return match t {
            Some(t) => Ok(Receiver::Intercept(t)),
            None => Err(super::errors::exception(
                slf.py(),
                "InterceptSolverCallback.__init__(solver_callback) was not called".into(),
            )),
        };
    }
    Ok(Receiver::Plain)
}

fn pure_virtual(py: Python<'_>, name: &str) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err({
        let _ = py;
        format!("Tried to call pure virtual function \"SolverCallback::{name}\"")
    })
}

#[pymethods]
impl PySolverCallback {
    /// `SolverCallback(dof_manager)`; subclasses may pass their own
    /// arguments (pybind11 `init_alias` semantics through `__init__`).
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Self::model_base()
    }

    /// `super().__init__(dof_manager)` from a Python subclass.
    #[pyo3(name = "__init__", signature = (*args, **_kwargs))]
    fn init(&self, args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
        if let Ok(a) = args.get_item(0)
            && let Ok(d) = a.cast::<PyDofManager>()
        {
            *lock(&self.dofs) = Some(d.clone().unbind());
        }
        Ok(())
    }

    /// `getMatrixType(matrix_id)`.
    #[pyo3(name = "getMatrixType")]
    fn get_matrix_type<'py>(
        slf: &Bound<'py, Self>,
        matrix_id: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => {
                let t = on_model_callback(py, &m, move |a| a.matrix_type(&matrix_id))?;
                matrix_type_obj(py, t)
            }
            Receiver::Intercept(t) => t.into_bound(py).call_method1("getMatrixType", (matrix_id,)),
            Receiver::Plain => Err(pure_virtual(py, "getMatrixType")),
        }
    }

    /// `assembleMatrix(matrix_id)`.
    #[pyo3(name = "assembleMatrix")]
    fn assemble_matrix(slf: &Bound<'_, Self>, matrix_id: String) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, move |a| a.assemble_matrix(&matrix_id)),
            Receiver::Intercept(t) => t
                .bind(py)
                .call_method1("assembleMatrix", (matrix_id,))
                .map(drop),
            Receiver::Plain => Err(pure_virtual(py, "assembleMatrix")),
        }
    }

    /// `assembleLumpedMatrix(matrix_id)`.
    #[pyo3(name = "assembleLumpedMatrix")]
    fn assemble_lumped_matrix(slf: &Bound<'_, Self>, matrix_id: String) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => {
                on_model_callback(py, &m, move |a| a.assemble_lumped_matrix(&matrix_id))
            }
            Receiver::Intercept(t) => t
                .bind(py)
                .call_method1("assembleLumpedMatrix", (matrix_id,))
                .map(drop),
            Receiver::Plain => Err(pure_virtual(py, "assembleLumpedMatrix")),
        }
    }

    /// `assembleResidual()`.
    #[pyo3(name = "assembleResidual")]
    fn assemble_residual(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, |a| a.assemble_residual()),
            Receiver::Intercept(t) => t.bind(py).call_method0("assembleResidual").map(drop),
            Receiver::Plain => Err(pure_virtual(py, "assembleResidual")),
        }
    }

    /// `predictor()`.
    #[pyo3(name = "predictor")]
    fn predictor(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, |a| a.predictor()),
            Receiver::Intercept(t) => t.bind(py).call_method0("predictor").map(drop),
            Receiver::Plain => Ok(()),
        }
    }

    /// `corrector()`.
    #[pyo3(name = "corrector")]
    fn corrector(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, |a| a.corrector()),
            Receiver::Intercept(t) => t.bind(py).call_method0("corrector").map(drop),
            Receiver::Plain => Ok(()),
        }
    }

    /// `beforeSolveStep()`.
    #[pyo3(name = "beforeSolveStep")]
    fn before_solve_step(slf: &Bound<'_, Self>) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, |a| a.before_solve_step()),
            Receiver::Intercept(t) => t.bind(py).call_method0("beforeSolveStep").map(drop),
            Receiver::Plain => Ok(()),
        }
    }

    /// `afterSolveStep(converged)`.
    #[pyo3(name = "afterSolveStep")]
    fn after_solve_step(slf: &Bound<'_, Self>, converged: bool) -> PyResult<()> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => on_model_callback(py, &m, move |a| a.after_solve_step(converged)),
            Receiver::Intercept(t) => t
                .bind(py)
                .call_method1("afterSolveStep", (converged,))
                .map(drop),
            Receiver::Plain => Ok(()),
        }
    }

    /// `dof_manager`: the model's DOF manager (models), the wrapped
    /// callback's (`InterceptSolverCallback`) or the constructor's.
    #[getter]
    fn dof_manager<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        match receiver(slf)? {
            Receiver::Model(m) => Ok(Bound::new(py, PyDofManager { model: m })?.into_any()),
            Receiver::Intercept(t) => t.into_bound(py).getattr("dof_manager"),
            Receiver::Plain => match lock(&slf.get().dofs).as_ref() {
                Some(d) => Ok(d.clone_ref(py).into_bound(py).into_any()),
                None => Ok(py.None().into_bound(py)),
            },
        }
    }
}

/// `InterceptSolverCallback(solver_callback)` (PY-064).
#[pyclass(extends = PySolverCallback, subclass, frozen, name = "InterceptSolverCallback", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyInterceptSolverCallback {
    target: Mutex<Option<Py<PyAny>>>,
}

#[pymethods]
impl PyInterceptSolverCallback {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PySolverCallback::model_base()).add_subclass(Self {
            target: Mutex::new(None),
        })
    }

    /// `super().__init__(solver_callback)`.
    #[pyo3(name = "__init__", signature = (solver_callback, *_args, **_kwargs))]
    fn init(
        &self,
        solver_callback: &Bound<'_, PyAny>,
        _args: &Bound<'_, PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        if !solver_callback.is_instance_of::<PySolverCallback>() {
            return Err(PyTypeError::new_err(
                "InterceptSolverCallback(solver_callback): expected a SolverCallback (e.g. a model)",
            ));
        }
        *lock(&self.target) = Some(solver_callback.clone().unbind());
        Ok(())
    }
}

/// Indices of the overridable callback methods.
const METHODS: [&str; 8] = [
    "getMatrixType",
    "assembleMatrix",
    "assembleLumpedMatrix",
    "assembleResidual",
    "predictor",
    "corrector",
    "beforeSolveStep",
    "afterSolveStep",
];
const M_TYPE: usize = 0;
const M_MATRIX: usize = 1;
const M_LUMPED: usize = 2;
const M_RESIDUAL: usize = 3;
const M_PREDICTOR: usize = 4;
const M_CORRECTOR: usize = 5;
const M_BEFORE: usize = 6;
const M_AFTER: usize = 7;

/// Which [`METHODS`] the class of `obj` overrides (compared with the
/// `SolverCallback` base methods).
fn overrides(obj: &Bound<'_, PyAny>) -> PyResult<[bool; 8]> {
    let py = obj.py();
    let base = py.get_type::<PySolverCallback>();
    let ty = obj.get_type();
    let mut out = [false; 8];
    for (k, name) in METHODS.iter().enumerate() {
        out[k] = !ty.getattr(*name)?.is(&base.getattr(*name)?);
    }
    Ok(out)
}

/// A Python exception carried out of an infallible callback
/// (`getMatrixType`), re-raised by `solveStep`.
type Pending = Arc<Mutex<Option<PyErr>>>;

/// The trampoline of a Python `InterceptSolverCallback`.
struct InterceptTrampoline {
    obj: Py<PyAny>,
    model: ModelRef,
    overrides: [bool; 8],
    /// The wrapped callback is the solved model (else non-overridden
    /// methods call the wrapped callback's Python methods, as upstream).
    wraps_model: bool,
    pending: Pending,
}

impl InterceptTrampoline {
    fn call_with<R>(
        &self,
        access: &mut dyn SolverAccess,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<R>,
    ) -> Result<R> {
        lending_callback(|py, cx| {
            with_solver_access(access, cx.key(), |h| {
                let _g = self.model.h.push_access(self.model.part, h, cx);
                f(self.obj.bind(py))
            })
        })
    }

    fn call(
        &self,
        d: &mut Delegate<'_>,
        f: impl FnOnce(&Bound<'_, PyAny>) -> PyResult<()>,
    ) -> Result<()> {
        self.call_with(&mut DelegateAccess(d), f)
    }

    /// A method the Python class does not override: the wrapped callback.
    fn forward(
        &self,
        d: &mut Delegate<'_>,
        method: usize,
        native_call: impl FnOnce(&mut Delegate<'_>) -> Result<()>,
        args: impl for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>>,
    ) -> Result<()> {
        if self.wraps_model {
            return native_call(d);
        }
        let name = METHODS[method];
        let target = callback(|py| {
            Ok(self
                .obj
                .bind(py)
                .cast::<PyInterceptSolverCallback>()?
                .get()
                .target
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
                .map(|t| t.clone_ref(py)))
        })?
        .ok_or_else(|| Error::invalid("InterceptSolverCallback without a wrapped callback"))?;
        self.call(d, |o| {
            let py = o.py();
            target.bind(py).call_method1(name, args(py)?).map(drop)
        })
    }
}

impl InterceptCallback for InterceptTrampoline {
    fn matrix_type(&self, model: &dyn SolverCallback, matrix_id: &str) -> RsMatrixType {
        if !self.overrides[M_TYPE] {
            return model.matrix_type(matrix_id);
        }
        let r = self.call_with(&mut TypeAccess(model), |o| {
            matrix_type_of(&o.call_method1("getMatrixType", (matrix_id,))?)
        });
        match r {
            Ok(t) => t,
            Err(e) => {
                let err = callback(|py| Ok(super::runtime::to_py(py, e)));
                if let Ok(err) = err {
                    lock(&self.pending).get_or_insert(err);
                }
                RsMatrixType::NotDefined
            }
        }
    }
    fn assemble_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        if !self.overrides[M_MATRIX] {
            return self.forward(
                model,
                M_MATRIX,
                |d| d.assemble_matrix(matrix_id),
                |py| PyTuple::new(py, [matrix_id]),
            );
        }
        self.call(model, |o| {
            o.call_method1("assembleMatrix", (matrix_id,)).map(drop)
        })
    }
    fn assemble_lumped_matrix(&mut self, model: &mut Delegate<'_>, matrix_id: &str) -> Result<()> {
        if !self.overrides[M_LUMPED] {
            return self.forward(
                model,
                M_LUMPED,
                |d| d.assemble_lumped_matrix(matrix_id),
                |py| PyTuple::new(py, [matrix_id]),
            );
        }
        self.call(model, |o| {
            o.call_method1("assembleLumpedMatrix", (matrix_id,))
                .map(drop)
        })
    }
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        if !self.overrides[M_RESIDUAL] {
            return self.forward(
                model,
                M_RESIDUAL,
                |d| d.assemble_residual(),
                |py| Ok(PyTuple::empty(py)),
            );
        }
        self.call(model, |o| o.call_method0("assembleResidual").map(drop))
    }
    fn predictor(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        if !self.overrides[M_PREDICTOR] {
            return self.forward(
                model,
                M_PREDICTOR,
                |d| d.predictor(),
                |py| Ok(PyTuple::empty(py)),
            );
        }
        self.call(model, |o| o.call_method0("predictor").map(drop))
    }
    fn corrector(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        if !self.overrides[M_CORRECTOR] {
            return self.forward(
                model,
                M_CORRECTOR,
                |d| d.corrector(),
                |py| Ok(PyTuple::empty(py)),
            );
        }
        self.call(model, |o| o.call_method0("corrector").map(drop))
    }
    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        if !self.overrides[M_BEFORE] {
            return self.forward(
                model,
                M_BEFORE,
                |d| d.before_solve_step(),
                |py| Ok(PyTuple::empty(py)),
            );
        }
        self.call(model, |o| o.call_method0("beforeSolveStep").map(drop))
    }
    fn after_solve_step(&mut self, model: &mut Delegate<'_>, converged: bool) -> Result<()> {
        if !self.overrides[M_AFTER] {
            return self.forward(
                model,
                M_AFTER,
                |d| d.after_solve_step(converged),
                |py| PyTuple::new(py, [converged]),
            );
        }
        self.call(model, |o| {
            o.call_method1("afterSolveStep", (converged,)).map(drop)
        })
    }
}

/// The trampoline of a plain Python `SolverCallback` replacing the model's
/// callback: Python methods get the model's own callback and the running
/// DOF manager as their access.
struct CallbackTrampoline<'a> {
    obj: Py<PyAny>,
    model: ModelRef,
    overrides: [bool; 8],
    pending: Pending,
    /// The model's own callback (not driven by this solve).
    inner: &'a mut dyn SolverCallback,
}

impl CallbackTrampoline<'_> {
    fn call(
        &mut self,
        dofs: &mut DofManager,
        method: usize,
        args: impl for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>>,
    ) -> Result<()> {
        let name = METHODS[method];
        let mut acc = CallbackAccess {
            cb: &mut *self.inner,
            dofs,
        };
        let (obj, model) = (&self.obj, &self.model);
        lending_callback(|py, cx| {
            with_solver_access(&mut acc, cx.key(), |h| {
                let _g = model.h.push_access(model.part, h, cx);
                obj.bind(py).call_method1(name, args(py)?).map(drop)
            })
        })
    }

    fn pure(&self, method: usize) -> Error {
        Error::invalid(format!(
            "Tried to call pure virtual function \"SolverCallback::{}\"",
            METHODS[method]
        ))
    }
}

impl SolverCallback for CallbackTrampoline<'_> {
    fn matrix_type(&self, matrix_id: &str) -> RsMatrixType {
        let r: Result<RsMatrixType> = if self.overrides[M_TYPE] {
            callback(|py| {
                matrix_type_of(
                    &self
                        .obj
                        .bind(py)
                        .call_method1("getMatrixType", (matrix_id,))?,
                )
            })
        } else {
            Err(self.pure(M_TYPE))
        };
        match r {
            Ok(t) => t,
            Err(e) => {
                if let Ok(err) = callback(|py| Ok(super::runtime::to_py(py, e))) {
                    lock(&self.pending).get_or_insert(err);
                }
                RsMatrixType::NotDefined
            }
        }
    }
    fn assemble_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if !self.overrides[M_MATRIX] {
            return Err(self.pure(M_MATRIX));
        }
        self.call(dofs, M_MATRIX, |py| PyTuple::new(py, [matrix_id]))
    }
    fn assemble_lumped_matrix(&mut self, dofs: &mut DofManager, matrix_id: &str) -> Result<()> {
        if !self.overrides[M_LUMPED] {
            return Err(self.pure(M_LUMPED));
        }
        self.call(dofs, M_LUMPED, |py| PyTuple::new(py, [matrix_id]))
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !self.overrides[M_RESIDUAL] {
            return Err(self.pure(M_RESIDUAL));
        }
        self.call(dofs, M_RESIDUAL, |py| Ok(PyTuple::empty(py)))
    }
    fn predictor(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !self.overrides[M_PREDICTOR] {
            return Ok(());
        }
        self.call(dofs, M_PREDICTOR, |py| Ok(PyTuple::empty(py)))
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !self.overrides[M_CORRECTOR] {
            return Ok(());
        }
        self.call(dofs, M_CORRECTOR, |py| Ok(PyTuple::empty(py)))
    }
    fn before_solve_step(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !self.overrides[M_BEFORE] {
            return Ok(());
        }
        self.call(dofs, M_BEFORE, |py| Ok(PyTuple::empty(py)))
    }
    fn after_solve_step(&mut self, dofs: &mut DofManager, converged: bool) -> Result<()> {
        if !self.overrides[M_AFTER] {
            return Ok(());
        }
        self.call(dofs, M_AFTER, |py| PyTuple::new(py, [converged]))
    }
}

/// `solveStep(callback, solver_id)` of `model` with a Python callback
/// object (`InterceptSolverCallback` or `SolverCallback` subclass, or a
/// model).
pub(crate) fn solve_step_with_callback(
    py: Python<'_>,
    model: &ModelRef,
    cb: &Bound<'_, PyAny>,
    solver_id: String,
) -> PyResult<()> {
    let id = move || solver_id;
    if let Ok(other) = cb.cast::<super::model::PyModelSolver>() {
        let o = &other.get().model;
        if Arc::ptr_eq(&o.h, &model.h) && o.part == model.part {
            let sid = id();
            return model.with_ops(py, move |ops| ops.solve_step(Some(&sid)));
        }
        return Err(super::errors::exception(
            py,
            "solveStep(callback): another model as the callback is not supported".into(),
        ));
    }
    let pending: Pending = Arc::new(Mutex::new(None));
    let ov = overrides(cb)?;
    if let Ok(icb) = cb.cast::<PyInterceptSolverCallback>() {
        let wraps_model = match lock(&icb.get().target).as_ref() {
            Some(t) => match t.bind(py).cast::<super::model::PyModelSolver>() {
                Ok(m) => {
                    Arc::ptr_eq(&m.get().model.h, &model.h) && m.get().model.part == model.part
                }
                Err(_) => false,
            },
            None => {
                return Err(super::errors::exception(
                    py,
                    "InterceptSolverCallback.__init__(solver_callback) was not called".into(),
                ));
            }
        };
        let mut user = InterceptTrampoline {
            obj: cb.clone().unbind(),
            model: model.clone(),
            overrides: ov,
            wraps_model,
            pending: Arc::clone(&pending),
        };
        let sid = id();
        let r = model.with_ops(py, move |ops| {
            ops.solve_step_intercepted(&mut user, Some(&sid))
        });
        if let Some(e) = lock(&pending).take() {
            return Err(e);
        }
        return r;
    }
    if cb.is_instance_of::<PySolverCallback>() {
        let mut obj = Some(cb.clone().unbind());
        let m = model.clone();
        let p = Arc::clone(&pending);
        let sid = id();
        let r = model.with_ops(py, move |ops| {
            ops.solve_step_replaced(Some(&sid), &mut |inner, solver, id| {
                let obj = obj
                    .take()
                    .ok_or_else(|| Error::invalid("internal error: solve callback reused"))?;
                let mut t = CallbackTrampoline {
                    obj,
                    model: m.clone(),
                    overrides: ov,
                    pending: Arc::clone(&p),
                    inner,
                };
                solver.solve_step(&mut t, id)
            })
        });
        if let Some(e) = lock(&pending).take() {
            return Err(e);
        }
        return r;
    }
    Err(PyTypeError::new_err(
        "solveStep(callback, solver_id=''): expected a SolverCallback",
    ))
}

/// Test support (`_test_common.callbackMatrixType`):
/// ask the trampoline of the Python callback `cb` (an
/// `InterceptSolverCallback` or a plain `SolverCallback` subclass) for the
/// type of `matrix_id` through the native `SolverCallback::matrix_type`, as
/// the solver does when it sets an integration scheme, and raise the Python
/// exception that the infallible method carried (`pending`), as
/// `solveStep` does. A solve does not ask a user callback for matrix types
/// (upstream and port both query them at `setIntegrationScheme`, with the
/// model as the callback), so this is the only way to exercise that carry.
pub(crate) fn callback_matrix_type<'py>(
    py: Python<'py>,
    model: &ModelRef,
    cb: &Bound<'py, PyAny>,
    matrix_id: String,
) -> PyResult<Bound<'py, PyAny>> {
    let pending: Pending = Arc::new(Mutex::new(None));
    let ov = overrides(cb)?;
    let obj = cb.clone().unbind();
    let intercept = cb.is_instance_of::<PyInterceptSolverCallback>();
    if !intercept && !cb.is_instance_of::<PySolverCallback>() {
        return Err(PyTypeError::new_err("expected a SolverCallback"));
    }
    let (m, p) = (model.clone(), Arc::clone(&pending));
    let t = model.with_ops(py, move |ops| {
        let mut obj = Some(obj);
        let mut out = None;
        ops.with_callback(&mut |inner, _dofs| {
            let obj = obj
                .take()
                .ok_or_else(|| Error::invalid("internal error: callback reused"))?;
            out = Some(if intercept {
                InterceptTrampoline {
                    obj,
                    model: m.clone(),
                    overrides: ov,
                    wraps_model: false,
                    pending: Arc::clone(&p),
                }
                .matrix_type(&*inner, &matrix_id)
            } else {
                CallbackTrampoline {
                    obj,
                    model: m.clone(),
                    overrides: ov,
                    pending: Arc::clone(&p),
                    inner,
                }
                .matrix_type(&matrix_id)
            });
            Ok(())
        })?;
        out.ok_or_else(|| Error::invalid("internal error: model callback not run"))
    })?;
    if let Some(e) = lock(&pending).take() {
        return Err(e);
    }
    matrix_type_obj(py, t)
}

/// Parse `solveStep(solver_id='')` / `solveStep(callback, solver_id='')`.
pub(crate) fn solve_step_args<'py>(
    args: &Bound<'py, PyTuple>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<(Option<Bound<'py, PyAny>>, String)> {
    let mut callback = None;
    let mut solver_id = String::new();
    let mut items: Vec<Bound<'py, PyAny>> = args.iter().collect();
    if let Some(first) = items.first()
        && !first.is_instance_of::<PyString>()
    {
        callback = Some(items.remove(0));
    }
    match items.as_slice() {
        [] => {}
        [s] => solver_id = s.extract()?,
        _ => {
            return Err(PyTypeError::new_err(
                "solveStep(): expected (solver_id='') or (callback, solver_id='')",
            ));
        }
    }
    if let Some(k) = kwargs {
        if let Some(c) = k.get_item("callback")? {
            callback = Some(c);
        }
        if let Some(s) = k.get_item("solver_id")? {
            solver_id = s.extract()?;
        }
    }
    Ok((callback, solver_id))
}

/// Register the solver-layer classes (`register_solvers`,
/// `register_integration_schemes`, `register_dof_manager` parts).
pub(crate) fn register_solvers(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySparseMatrix>()?;
    m.add_class::<PySparseMatrixAij>()?;
    m.add_class::<PySolverVector>()?;
    m.add_class::<PySparseSolver>()?;
    m.add_class::<PySparseSolverEigen>()?;
    m.add_class::<PySparseSolverMumps>()?;
    m.add_class::<PySparseSolverPetsc>()?;
    m.add_class::<PySparseMatrixPetsc>()?;
    m.add_class::<PyTermToAssemble>()?;
    m.add_class::<PyTermsToAssemble>()?;
    Ok(())
}

/// `register_integration_schemes`.
pub(crate) fn register_integration_schemes(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    m.add_class::<PyIntegrationScheme>()?;
    let scheme = py.get_type::<PyIntegrationScheme>();
    let st = py.get_type::<PySolutionType>();
    let members = PyDict::new(py);
    let mut doc = String::from("Members:\n\n");
    for (name, value) in SOLUTION_TYPE_MEMBERS {
        let inst = Bound::new(py, PySolutionType { value: *value })?;
        st.setattr(*name, &inst)?;
        members.set_item(*name, &inst)?;
        // `.export_values()` into the enclosing class scope
        scheme.setattr(*name, &inst)?;
        doc.push_str("  ");
        doc.push_str(name);
        doc.push_str("\n\n");
    }
    st.setattr("__members__", members)?;
    st.setattr("__doc__", doc.trim_end())?;
    st.setattr("__qualname__", "IntegrationScheme.SolutionType")?;
    scheme.setattr("SolutionType", st)?;
    m.add_class::<PyIntegrationScheme2ndOrder>()?;
    m.add_class::<PyNewmarkBeta>()?;
    m.add_class::<PyCentralDifference>()?;
    m.add_class::<PyTrapezoidalRule2>()?;
    m.add_class::<PyFoxGoodwin>()?;
    m.add_class::<PyLinearAceleration>()?;
    Ok(())
}

/// The DOF manager and solver-callback classes of `register_dof_manager`.
pub(crate) fn register_dof_manager(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDofManager>()?;
    m.add_class::<PySolverCallback>()?;
    m.add_class::<PyInterceptSolverCallback>()?;
    Ok(())
}
