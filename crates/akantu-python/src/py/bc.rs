// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_boundary_conditions.cc
// (BCFunctor, DirichletFunctor/NeumannFunctor with PYBIND11_OVERRIDE
// trampolines, FixedValue, IncrementValue, Increment, FromHigherDim,
// FromSameDim, FreeBoundary).

//! Boundary-condition functors (PY-069).
//!
//! Built-in functors run natively. A Python subclass overriding
//! `__call__` is called back, through the runtime trampoline
//! ([`super::runtime::callback`]), once per node (Dirichlet:
//! `__call__(node, flags, primal, coord)`) or quadrature point (Neumann:
//! `__call__(quad_point, dual, coord, normals)`). For a Dirichlet group the
//! runtime is entered once and `__call__` runs for every node in order.
//!
//! akantu-rs extension: a Dirichlet functor that defines
//! `callNodes(nodes, flags, primal, coords)` is called once per group
//! instead, with NumPy copies of the group's rows (`nodes` int32 `(n,)`,
//! `flags` bool and `primal` float64 `(n, c)`, `coords` read-only float64
//! `(n, dim)`); `flags` and `primal` are written back to the model after
//! the call returns (nothing is written if it raises).
//!
//! Dirichlet arguments are, as upstream (`VectorProxy` row views through
//! pybind11's Eigen caster), **views of the live rows**: `flags` and
//! `primal` alias the node's tuple of the model's blocked DOFs and
//! displacement (writable), `coord` the node's coordinates (read-only).
//! Writes land immediately, reads see the values written for earlier nodes,
//! and the functor may read and write any other model array through views
//! it holds (no native guard is alive during the call, ARCHITECTURE.md, "Shared storage and its aliasing rules"). Neumann
//! arguments are rows of temporaries upstream too (`dual` of the local
//! per-point array, `coord`/`normals` of interpolated data): small NumPy
//! copies, `dual` copied back after the call. A Python exception raised in
//! `__call__` propagates out of `applyBC` with its original type and
//! traceback.

use akantu::core::array_shared::SharedArray;
use akantu::core::enums::SpatialDirection;
use akantu::core::{Idx, Real, Result};
use akantu::fe::integration_point::IntegrationPoint;
use akantu::mesh::Mesh;
use akantu::model::boundary_condition::{
    DirichletFunctor, FixedValue, FreeBoundary, FromHigherDim, FromSameDim, Increment,
    IncrementValue, NeumannFunctor, apply_rows_by_copy, apply_shared_per_node,
};
use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;

use super::arrays::input_matrix;
use super::common::PyElement;
use super::enums::DirectionArg;
use super::runtime::{callback, runtime};
use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::{Layout, to_numpy};

fn axis(d: DirectionArg) -> SpatialDirection {
    match d.0 {
        0 => SpatialDirection::X,
        1 => SpatialDirection::Y,
        _ => SpatialDirection::Z,
    }
}

/// `BCFunctor`.
#[pyclass(subclass, frozen, name = "BCFunctor", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyBcFunctor;

#[derive(Clone, Debug)]
pub(crate) enum BuiltinDirichlet {
    Base,
    FixedValue(FixedValue),
    IncrementValue(IncrementValue),
    Increment(Increment),
}

/// `DirichletFunctor()` / `DirichletFunctor(axis)`: base class of
/// Dirichlet boundary conditions applied with `model.applyBC(functor,
/// group)`.
///
/// A subclass defines `__call__(node, flags, primal, coord)`, called once
/// per node of the group with views of the node's rows (`flags` and
/// `primal` writable, `coord` read-only).
///
/// Optional (akantu-rs, absent upstream): `callNodes(nodes, flags, primal,
/// coords)`, called once per group instead, and used instead of `__call__`
/// when both exist. It receives copies of the group's rows (`nodes` int32
/// `(n,)`, `flags` bool and `primal` float64 `(n, nb_dof)`, `coords`
/// read-only float64 `(n, dim)`); `flags` and `primal` are written back to
/// the model after it returns, in node order (a node listed twice keeps its
/// last row); nothing is written if it raises. It is not called for a group
/// without nodes.
///
/// Example:
///
/// ```python
/// class Ramp(aka.DirichletFunctor):
///     def callNodes(self, nodes, flags, primal, coords):
///         primal[:, 0] = 1e-3 * coords[:, 0]
///         flags[:, 0] = True
///
/// model.applyBC(Ramp(), "left")
/// ```
#[pyclass(extends = PyBcFunctor, subclass, frozen, name = "DirichletFunctor", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDirichletFunctor {
    builtin: BuiltinDirichlet,
}

#[pymethods]
impl PyDirichletFunctor {
    /// `DirichletFunctor()` / `DirichletFunctor(axis)`; Python subclasses
    /// may pass their own constructor arguments (pybind11 semantics).
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyClassInitializer<Self> {
        PyClassInitializer::from(PyBcFunctor).add_subclass(Self {
            builtin: BuiltinDirichlet::Base,
        })
    }

    /// `super().__init__(…)` from a Python subclass (arguments accepted and
    /// ignored: the base functor has no state used by the port).
    #[pyo3(name = "__init__", signature = (*_args, **_kwargs))]
    fn init(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) {
    }
}

fn dirichlet_init(b: BuiltinDirichlet) -> PyClassInitializer<PyDirichletFunctor> {
    PyClassInitializer::from(PyBcFunctor).add_subclass(PyDirichletFunctor { builtin: b })
}

/// `FixedValue(value, axis)`.
#[pyclass(extends = PyDirichletFunctor, subclass, frozen, name = "FixedValue", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFixedValue;

#[pymethods]
impl PyFixedValue {
    #[new]
    fn new(value: Real, axis: DirectionArg) -> PyClassInitializer<Self> {
        dirichlet_init(BuiltinDirichlet::FixedValue(FixedValue {
            value,
            axis: self::axis(axis),
        }))
        .add_subclass(Self)
    }
}

/// `IncrementValue(value, axis)`.
#[pyclass(extends = PyDirichletFunctor, subclass, frozen, name = "IncrementValue", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyIncrementValue;

#[pymethods]
impl PyIncrementValue {
    #[new]
    fn new(value: Real, axis: DirectionArg) -> PyClassInitializer<Self> {
        dirichlet_init(BuiltinDirichlet::IncrementValue(IncrementValue {
            value,
            axis: self::axis(axis),
        }))
        .add_subclass(Self)
    }
}

/// `Increment(value)` (vector).
#[pyclass(extends = PyDirichletFunctor, subclass, frozen, name = "Increment", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyIncrement;

#[pymethods]
impl PyIncrement {
    #[new]
    fn new(value: &Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let (v, _) = super::arrays::input_f64(value)?;
        Ok(dirichlet_init(BuiltinDirichlet::Increment(Increment { value: v })).add_subclass(Self))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum BuiltinNeumann {
    Base,
    FromHigherDim(FromHigherDim),
    FromSameDim(FromSameDim),
    FreeBoundary,
}

/// `NeumannFunctor()`.
#[pyclass(extends = PyBcFunctor, subclass, frozen, name = "NeumannFunctor", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNeumannFunctor {
    builtin: BuiltinNeumann,
}

#[pymethods]
impl PyNeumannFunctor {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyClassInitializer<Self> {
        neumann_init(BuiltinNeumann::Base)
    }

    /// `super().__init__(…)` from a Python subclass.
    #[pyo3(name = "__init__", signature = (*_args, **_kwargs))]
    fn init(
        &self,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) {
    }
}

fn neumann_init(b: BuiltinNeumann) -> PyClassInitializer<PyNeumannFunctor> {
    PyClassInitializer::from(PyBcFunctor).add_subclass(PyNeumannFunctor { builtin: b })
}

/// `FromHigherDim(matrix)` (`FromStress` in `akantu/__init__.py`).
#[pyclass(extends = PyNeumannFunctor, subclass, frozen, name = "FromHigherDim", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFromHigherDim;

#[pymethods]
impl PyFromHigherDim {
    #[new]
    fn new(matrix: &Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let m = input_matrix(matrix)?;
        Ok(neumann_init(BuiltinNeumann::FromHigherDim(FromHigherDim::new(m))).add_subclass(Self))
    }
}

/// `FromSameDim(vector)` (`FromTraction` in `akantu/__init__.py`).
#[pyclass(extends = PyNeumannFunctor, subclass, frozen, name = "FromSameDim", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFromSameDim;

#[pymethods]
impl PyFromSameDim {
    #[new]
    fn new(vector: &Bound<'_, PyAny>) -> PyResult<PyClassInitializer<Self>> {
        let (v, _) = super::arrays::input_f64(vector)?;
        Ok(neumann_init(BuiltinNeumann::FromSameDim(FromSameDim::new(v))).add_subclass(Self))
    }
}

/// `FreeBoundary()`.
#[pyclass(extends = PyNeumannFunctor, subclass, frozen, name = "FreeBoundary", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFreeBoundary;

#[pymethods]
impl PyFreeBoundary {
    #[new]
    fn new() -> PyClassInitializer<Self> {
        neumann_init(BuiltinNeumann::FreeBoundary).add_subclass(Self)
    }
}

/// `IntegrationPoint` passed to `NeumannFunctor.__call__` (opaque upstream;
/// the getters are akantu-rs extensions).
#[pyclass(frozen, name = "IntegrationPoint", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyIntegrationPoint {
    point: IntegrationPoint,
}

#[pymethods]
impl PyIntegrationPoint {
    /// The element (akantu-rs extension).
    #[getter]
    fn element(&self) -> PyElement {
        PyElement {
            element: self.point.element,
        }
    }

    /// Index of the point in its element (akantu-rs extension).
    #[getter]
    fn num_point(&self) -> Idx {
        self.point.num_point
    }

    /// Index of the point in its `(type, ghost)` block (akantu-rs
    /// extension).
    #[getter]
    fn global_num(&self) -> Idx {
        self.point.global_num
    }
}

/// A read-only 1-D NumPy copy.
fn read_only<'py>(py: Python<'py>, v: &[Real]) -> PyResult<Bound<'py, PyArray1<Real>>> {
    let a = PyArray1::from_slice(py, v);
    a.call_method1("setflags", (false,))?;
    Ok(a)
}

// --- adapters --------------------------------------------------------------------

/// A Dirichlet functor ready for `applyBC` (moved into the native section).
#[derive(Debug)]
pub(crate) enum DirichletAdapter {
    Builtin(BuiltinDirichlet),
    /// A Python `__call__`, with a view of the mesh node coordinates for
    /// the `coord` row views (set inside the native section).
    /// `vectorised`: the object has a `callNodes` method, called once per
    /// group instead of `__call__` per node.
    Python {
        obj: Py<PyAny>,
        coords: Option<ForeignView<Real>>,
        vectorised: bool,
    },
}

impl DirichletAdapter {
    /// Provide the coordinates view for Python functors.
    pub(crate) fn set_coords(&mut self, view: ForeignView<Real>) {
        if let Self::Python { coords, .. } = self {
            *coords = Some(view.read_only());
        }
    }
}

/// A Neumann functor ready for `applyBC`.
#[derive(Debug)]
pub(crate) enum NeumannAdapter {
    Builtin(BuiltinNeumann),
    Python(Py<PyAny>),
}

/// Name of the optional group method of Python Dirichlet functors
/// (akantu-rs extension): `callNodes(nodes, flags, primal, coords)`.
const CALL_NODES: &str = "callNodes";

/// The adapter of a `DirichletFunctor` object: the Python `__call__` if a
/// subclass defines one, else the built-in functor.
pub(crate) fn dirichlet_adapter(obj: &Bound<'_, PyAny>) -> PyResult<DirichletAdapter> {
    let vectorised = obj.hasattr(CALL_NODES)?;
    if obj.is_callable() || vectorised {
        return Ok(DirichletAdapter::Python {
            obj: obj.clone().unbind(),
            coords: None,
            vectorised,
        });
    }
    let f = obj.cast::<PyDirichletFunctor>()?;
    Ok(DirichletAdapter::Builtin(f.get().builtin.clone()))
}

/// The adapter of a `NeumannFunctor` object.
pub(crate) fn neumann_adapter(obj: &Bound<'_, PyAny>) -> PyResult<NeumannAdapter> {
    if obj.is_callable() {
        return Ok(NeumannAdapter::Python(obj.clone().unbind()));
    }
    let f = obj.cast::<PyNeumannFunctor>()?;
    Ok(NeumannAdapter::Builtin(f.get().builtin.clone()))
}

fn not_implemented(what: &str) -> akantu::core::Error {
    akantu::core::Error::NotImplemented(match what {
        "dirichlet" => "DirichletFunctor.__call__ (define it in a subclass)",
        _ => "Tried to call pure virtual function \"NeumannFunctor::__call__\"",
    })
}

impl DirichletFunctor for DirichletAdapter {
    fn apply(
        &self,
        node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()> {
        match self {
            Self::Builtin(BuiltinDirichlet::Base) => Err(not_implemented("dirichlet")),
            Self::Builtin(BuiltinDirichlet::FixedValue(f)) => f.apply(node, flags, primal, coord),
            Self::Builtin(BuiltinDirichlet::IncrementValue(f)) => {
                f.apply(node, flags, primal, coord)
            }
            Self::Builtin(BuiltinDirichlet::Increment(f)) => f.apply(node, flags, primal, coord),
            // only reached when called on copies (not by `applyBC`, which
            // uses `apply_shared`): the copies are copied back
            Self::Python { obj, .. } => {
                let (f, p) = callback(|py| {
                    let fl = PyArray1::from_slice(py, flags);
                    let pr = PyArray1::from_slice(py, primal);
                    let co = read_only(py, coord)?;
                    obj.call1(py, (node, &fl, &pr, &co))?;
                    Ok((fl.to_vec()?, pr.to_vec()?))
                })?;
                copy_back(flags, &f)?;
                copy_back(primal, &p)
            }
        }
    }

    /// Python functors call back into Python; built-in ones take the native
    /// path of `apply_dirichlet` (guards once per group).
    fn calls_foreign(&self) -> bool {
        matches!(self, Self::Python { .. })
    }

    /// Python functors get views of the live rows (upstream `VectorProxy`
    /// row views); built-in functors work on copies (bitwise the same).
    fn apply_shared(
        &self,
        node: Idx,
        row: usize,
        blocked: &mut SharedArray<bool>,
        primal: &mut SharedArray<Real>,
        coord: &[Real],
    ) -> Result<()> {
        let Self::Python { obj, coords, .. } = self else {
            return apply_rows_by_copy(self, node, row, blocked, primal, coord);
        };
        let rt = runtime();
        let fv = ForeignView::new(blocked, rt, true)?.rows(row..row + 1)?;
        let pv = ForeignView::new(primal, rt, true)?.rows(row..row + 1)?;
        let cv = match coords {
            Some(c) => Some(c.rows(row..row + 1)?),
            None => None,
        };
        callback(|py| {
            let fl = to_numpy(py, fv, Layout::Tuple)?;
            let pr = to_numpy(py, pv, Layout::Tuple)?;
            let co = match cv {
                Some(v) => to_numpy(py, v, Layout::Tuple)?,
                None => read_only(py, coord)?.into_any(),
            };
            obj.call1(py, (node, fl, pr, co))?;
            Ok(())
        })
    }

    /// Python functors: the arrays are exported once and Python is entered
    /// once for the group; `__call__` is still called once per node, in
    /// order, with views of the node's live rows (as
    /// [`DirichletFunctor::apply_shared`] passes them), and the first
    /// exception stops the loop.
    fn apply_shared_nodes(
        &self,
        nodes: &[Idx],
        mesh: &Mesh,
        blocked: &mut SharedArray<bool>,
        primal: &mut SharedArray<Real>,
    ) -> Result<()> {
        let Self::Python {
            obj,
            coords,
            vectorised,
        } = self
        else {
            return apply_shared_per_node(self, nodes, mesh, blocked, primal);
        };
        if *vectorised {
            return call_nodes(obj, nodes, mesh, blocked, primal);
        }
        let Some(coords) = coords else {
            return apply_shared_per_node(self, nodes, mesh, blocked, primal);
        };
        if nodes.is_empty() {
            return Ok(());
        }
        let rt = runtime();
        let fv = ForeignView::new(blocked, rt, true)?;
        let pv = ForeignView::new(primal, rt, true)?;
        akantu::core::array_shared::export_all::callback_site("DirichletFunctor::apply_shared");
        callback(|py| {
            let to_py = |e| super::runtime::to_py(py, e);
            for &node in nodes {
                let row = akantu::core::try_to_usize(node).map_err(to_py)?;
                let rows = row..row + 1;
                let fl = to_numpy(py, fv.rows(rows.clone()).map_err(to_py)?, Layout::Tuple)?;
                let pr = to_numpy(py, pv.rows(rows.clone()).map_err(to_py)?, Layout::Tuple)?;
                let co = to_numpy(py, coords.rows(rows).map_err(to_py)?, Layout::Tuple)?;
                obj.call1(py, (node, fl, pr, co))?;
            }
            Ok(())
        })
    }
}

/// `callNodes(nodes, flags, primal, coords)` of a Python Dirichlet functor
/// (akantu-rs extension): one call for the group with NumPy copies of the
/// group's rows (`nodes` int32 `(n,)`, `flags` bool `(n, c)`, `primal`
/// float64 `(n, c)`, `coords` read-only float64 `(n, dim)`); the rows of
/// `flags` and `primal` are written back to the model in node order after
/// the call (a node listed twice keeps its last row).
fn call_nodes(
    obj: &Py<PyAny>,
    nodes: &[Idx],
    mesh: &Mesh,
    blocked: &mut SharedArray<bool>,
    primal: &mut SharedArray<Real>,
) -> Result<()> {
    // as `__call__`, which is never entered for a group without nodes
    if nodes.is_empty() {
        return Ok(());
    }
    let dim = mesh.spatial_dimension();
    let rows: Vec<usize> = nodes
        .iter()
        .map(|&n| akantu::core::try_to_usize(n))
        .collect::<Result<_>>()?;
    let n = rows.len();
    let (c, flags, values) = {
        let b = blocked.read()?;
        let p = primal.read()?;
        let c = p.nb_component();
        if b.nb_component() != c {
            return Err(akantu::core::Error::DimensionMismatch {
                context: "DirichletFunctor.callNodes: blocked flags vs primal components",
                expected: c,
                got: b.nb_component(),
            });
        }
        let mut flags = Vec::with_capacity(n * c);
        let mut values = Vec::with_capacity(n * c);
        for &r in &rows {
            flags.extend_from_slice(b.get_tuple(r)?);
            values.extend_from_slice(p.get_tuple(r)?);
        }
        (c, flags, values)
    };
    let mut coords = Vec::with_capacity(n * dim);
    for &r in &rows {
        coords.extend_from_slice(&mesh.nodes().as_slice()[r * dim..(r + 1) * dim]);
    }
    akantu::core::array_shared::export_all::callback_site("DirichletFunctor::callNodes");
    let (flags, values) = callback(|py| {
        let nd = PyArray1::from_slice(py, nodes);
        let fl = PyArray1::from_vec(py, flags).reshape([n, c])?;
        let pr = PyArray1::from_vec(py, values).reshape([n, c])?;
        let co = PyArray1::from_vec(py, coords).reshape([n, dim])?;
        co.call_method1("setflags", (false,))?;
        obj.call_method1(py, CALL_NODES, (nd, &fl, &pr, co))?;
        Ok((fl.to_vec()?, pr.to_vec()?))
    })?;
    if flags.len() != n * c || values.len() != n * c {
        return Err(akantu::core::Error::DimensionMismatch {
            context: "DirichletFunctor.callNodes output",
            expected: n * c,
            got: flags.len().min(values.len()),
        });
    }
    let mut b = blocked.write()?;
    for (k, &r) in rows.iter().enumerate() {
        b.get_tuple_mut(r)?
            .copy_from_slice(&flags[k * c..(k + 1) * c]);
    }
    drop(b);
    let mut p = primal.write()?;
    for (k, &r) in rows.iter().enumerate() {
        p.get_tuple_mut(r)?
            .copy_from_slice(&values[k * c..(k + 1) * c]);
    }
    Ok(())
}

impl NeumannFunctor for NeumannAdapter {
    fn calls_foreign(&self) -> bool {
        matches!(self, Self::Python(_))
    }

    fn apply(
        &self,
        quad_point: &IntegrationPoint,
        dual: &mut [Real],
        coord: &[Real],
        normals: &[Real],
    ) -> Result<()> {
        match self {
            Self::Builtin(BuiltinNeumann::Base) => Err(not_implemented("neumann")),
            Self::Builtin(BuiltinNeumann::FromHigherDim(f)) => {
                f.apply(quad_point, dual, coord, normals)
            }
            Self::Builtin(BuiltinNeumann::FromSameDim(f)) => {
                f.apply(quad_point, dual, coord, normals)
            }
            Self::Builtin(BuiltinNeumann::FreeBoundary) => {
                FreeBoundary.apply(quad_point, dual, coord, normals)
            }
            Self::Python(obj) => {
                let point = *quad_point;
                let d = callback(|py| {
                    let du = PyArray1::from_slice(py, dual);
                    let co = read_only(py, coord)?;
                    let no = read_only(py, normals)?;
                    obj.call1(py, (PyIntegrationPoint { point }, &du, &co, &no))?;
                    du.to_vec().map_err(PyErr::from)
                })?;
                copy_back(dual, &d)
            }
        }
    }
}

fn copy_back<T: Copy>(dst: &mut [T], src: &[T]) -> Result<()> {
    if dst.len() != src.len() {
        return Err(akantu::core::Error::DimensionMismatch {
            context: "boundary-condition functor output",
            expected: dst.len(),
            got: src.len(),
        });
    }
    dst.copy_from_slice(src);
    Ok(())
}

/// Register the functor classes (`register_boundary_conditions`).
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyIntegrationPoint>()?;
    m.add_class::<PyBcFunctor>()?;
    m.add_class::<PyDirichletFunctor>()?;
    m.add_class::<PyNeumannFunctor>()?;
    m.add_class::<PyFixedValue>()?;
    m.add_class::<PyIncrementValue>()?;
    m.add_class::<PyIncrement>()?;
    m.add_class::<PyFromHigherDim>()?;
    m.add_class::<PyFromSameDim>()?;
    m.add_class::<PyFreeBoundary>()?;
    Ok(())
}

/// `applyBC(functor, element_group)` of any model with a
/// `DirichletFunctor` or a `NeumannFunctor` (built-in or Python subclass).
pub(crate) fn apply_bc(
    py: Python<'_>,
    model: &super::handle::ModelRef,
    func: &Bound<'_, PyAny>,
    element_group: String,
) -> PyResult<()> {
    if func.is_instance_of::<PyDirichletFunctor>() {
        let mut f = dirichlet_adapter(func)?;
        let h = std::sync::Arc::clone(&model.h);
        let part = model.part;
        return super::runtime::native(py, move || {
            if let DirichletAdapter::Python { .. } = f {
                // `coord` arguments are read-only views of the node rows
                f.set_coords(
                    h.mesh
                        .h
                        .export::<Real>(h.mesh.part, super::mesh::MeshKey::Nodes)?,
                );
            }
            h.with(|m| m.ops(part)?.apply_dirichlet(&f, &element_group))
        });
    }
    if func.is_instance_of::<PyNeumannFunctor>() {
        let f = neumann_adapter(func)?;
        return model.with_ops(py, move |m| m.apply_neumann(&f, &element_group));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "applyBC: expected a DirichletFunctor or a NeumannFunctor",
    ))
}
