// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_fe_engine.cc (FEEngine:
// getNbIntegrationPoints, initShapeFunctions, integrate,
// gradientOnIntegrationPoints, interpolateOnIntegrationPoints, computeBtD,
// computeBtDB, computeNtb, computeNtbN, computeIntegrationPointsCoordinates,
// assembleFieldLumped, assembleFieldMatrix, getElementInradius,
// getNormalsOnIntegrationPoints, getShapes, getShapesDerivatives, getMesh),
// python/py_model.cc (Model.getFEEngine / getFEEngineBoundary),
// src/fe_engine/fe_engine_template_tmpl.hh (ElementTypeMapArray overloads
// of integrate / interpolateOnIntegrationPoints),
// src/fe_engine/fe_engine_template_tmpl_field.hh (field assembly).

//! `FEEngine` of a model (PY-057).
//!
//! Inputs (`f`, `u`, `filter_elements`) are copied before the native
//! section; outputs given as NumPy arrays (`intf`, `nablauq`, `uq`) are
//! filled after it by a NumPy assignment, so they must have exactly the
//! number of values the operation produces (upstream writes through a proxy
//! of the NumPy buffer and checks sizes only in debug builds). An akantu view
//! passed as an output must still be attached (stale views raise, ARCHITECTURE.md, "Shared storage and its aliasing rules").
//! `getShapes`/`getShapesDerivatives`/`getNormalsOnIntegrationPoints` return
//! read-only copies (upstream: references to the engine's `const` arrays).
//!
//! `ElementTypeMapArray` overloads (`integrate(f, intf, filter_elements)`,
//! `interpolateOnIntegrationPoints(u, uq, filter_elements)`) loop over the
//! blocks of `f` (resp. `uq`) of the engine's element kind, both ghost
//! types, as upstream; a map created from Python receives new blocks (any
//! size, upstream resizes), a map owned by akantu (a law internal) is
//! written in place and must have the right size.
//!
//! `assembleFieldLumped(field_funct, lumped_matrix, dof_manager, type,
//! ghost_type)` and `assembleFieldMatrix(field_funct, matrix_id, dof_id,
//! dof_manager, type, ghost_type)` call `field_funct(matrix, element)` once
//! per element, in element order, **before** the native section (no akantu
//! array is borrowed while Python runs; an exception propagates as is),
//! with a zeroed, writable `nb_dof × nb_points` Fortran-ordered NumPy
//! matrix (upstream: an Eigen map of a column-major `Matrix<Real>`), read
//! back after the call (fill it in place); `nb_points` is the engine's rule
//! for the lumped version and the mass rule (order `2·degree`) for the
//! matrix version, as upstream. The lumped
//! version scatter-adds into the NumPy `lumped_matrix` through the mesh of
//! the engine (upstream: the DOF manager's mesh, the same for a model's own
//! DOF manager); the matrix version assembles symmetric elemental matrices
//! into `matrix_id` of the given DOF manager.

use std::collections::BTreeMap;

use akantu::core::array::ArrayView;
use akantu::core::enums::MatrixType;
use akantu::core::{
    Array, Element, ElementKind, ElementType, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu::fe::engine::FeEngine;
use akantu::mesh::Mesh;
use pyo3::prelude::*;
use pyo3::types::PyEllipsis;

use super::arrays::{input_f64, owned_numpy};
use super::common::PyElement;
use super::enums::{EtArg, GhostArg};
use super::etma::{EtmaInt, EtmaReal};
use super::handle::ModelRef;
use super::mesh::PyMesh;
use super::runtime::IntoPy;
use super::solver::{PyDofManager, with_dofs};
use crate::foreign_view::numpy_export::Layout;

/// `FEEngine` (by model and engine name).
#[pyclass(frozen, name = "FEEngine", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyFeEngine {
    pub(crate) model: ModelRef,
    pub(crate) name: String,
    pub(crate) boundary: bool,
}

impl PyFeEngine {
    fn with<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&FeEngine, &Mesh) -> Result<R> + Send,
    ) -> PyResult<R> {
        let (name, boundary) = (self.name.clone(), self.boundary);
        let lent = if boundary {
            None
        } else {
            self.model
                .h
                .active_services(py, self.model.part)
                .py_err(py)?
        };
        if let Some((services, cx)) = lent {
            // inside a Python law callback of this model (the model is held
            // by the running operation): its phase-scoped services
            return super::runtime::native(py, move || {
                services.with(cx, |sv| f(sv.fe_engine(&name)?, sv.mesh()))?
            });
        }
        self.model.with_ops(py, move |m| {
            let mut f = Some(f);
            let mut out = None;
            m.with_fe_engine(&name, boundary, &mut |fe, mesh| {
                let f = f.take().ok_or_else(|| {
                    akantu::core::Error::invalid("internal error: FE callback reused")
                })?;
                out = Some(f(fe, mesh)?);
                Ok(())
            })?;
            out.ok_or_else(|| akantu::core::Error::invalid("internal error: FE callback not run"))
        })
    }
}

/// `(values, rows, cols)` of a float input (`(n,)` = one column).
fn input_2d(obj: &Bound<'_, PyAny>, nb_component: Option<usize>) -> PyResult<(Vec<Real>, usize)> {
    let (values, shape) = input_f64(obj)?;
    let nbc = match (shape.as_slice(), nb_component) {
        (_, Some(c)) => c,
        ([_, c], None) => *c,
        _ => 1,
    };
    if nbc > 0 && values.len() % nbc != 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "array of {} values is not a multiple of {nbc} components",
            values.len()
        )));
    }
    Ok((values, nbc))
}

fn input_filter(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<Vec<Idx>>> {
    let Some(obj) = obj else { return Ok(None) };
    if obj.is_none() {
        return Ok(None);
    }
    let np = obj.py().import("numpy")?;
    let a = np.call_method1("ascontiguousarray", (obj, np.getattr("int32")?))?;
    let v: Vec<Idx> = a.call_method0("ravel")?.call_method0("tolist")?.extract()?;
    // upstream maps "no filter" to an empty array
    Ok((!v.is_empty()).then_some(v))
}

/// Write `values` (`rows × cols`, row-major) into the NumPy output `out`.
pub(crate) fn write_out(
    out: &Bound<'_, PyAny>,
    values: Vec<Real>,
    rows: usize,
    cols: usize,
) -> PyResult<()> {
    let py = out.py();
    let mut b = out.clone();
    loop {
        if type_name(&b)? == "_ArrayLease" {
            if b.call_method0("is_stale")?.extract::<bool>()? {
                return Err(super::errors::exception(
                    py,
                    "stale array view passed as an output: the array was resized, replaced or destroyed; fetch it again"
                        .into(),
                ));
            }
            break;
        }
        match b.getattr("base") {
            Ok(base) if !base.is_none() => b = base,
            _ => break,
        }
    }
    let size: usize = out.getattr("size")?.extract()?;
    if size != values.len() {
        return Err(super::errors::exception(
            py,
            format!(
                "output array has {size} values, the operation produces {} ({rows} x {cols})",
                values.len()
            ),
        ));
    }
    let res = owned_numpy(py, values, rows, cols, Layout::Rows)?;
    let res = res.call_method1("reshape", (out.getattr("shape")?,))?;
    out.set_item(PyEllipsis::get(py), res)
}

/// Blocks of an `ElementTypeMapArrayReal` of element kind `kind` (copied):
/// `(type, ghost, values, nb_component)`.
type RealBlocks = Vec<(ElementType, GhostType, Vec<Real>, usize)>;

fn etma_real_blocks(m: &Bound<'_, EtmaReal>, kind: ElementKind) -> PyResult<RealBlocks> {
    let py = m.py();
    let p = &m.get().provider;
    let mut out = Vec::new();
    for g in GhostType::ALL {
        let tf = TypeFilter {
            spatial_dimension: None,
            ghost_type: g,
            kind: Some(kind),
        };
        for t in p.types(py, tf)? {
            let a = p.call(py, t, g)?;
            let (values, nbc) = input_2d(&a, None)?;
            out.push((t, g, values, nbc));
        }
    }
    Ok(out)
}

/// Blocks of an element filter map, by `(type, ghost)`.
type FilterBlocks = BTreeMap<(ElementType, GhostType), Vec<Idx>>;

/// Blocks of an `ElementTypeMapArrayInt` filter (copied).
fn etma_filter_blocks(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<FilterBlocks>> {
    let Some(obj) = obj.filter(|f| !f.is_none()) else {
        return Ok(None);
    };
    let fmap = obj.cast::<EtmaInt>().map_err(|_| {
        pyo3::exceptions::PyTypeError::new_err("filter_elements must be an ElementTypeMapArrayInt")
    })?;
    let py = obj.py();
    let p = &fmap.get().provider;
    let mut blocks = BTreeMap::new();
    for g in GhostType::ALL {
        let tf = TypeFilter {
            spatial_dimension: None,
            ghost_type: g,
            kind: None,
        };
        for t in p.types(py, tf)? {
            let a = p.call(py, t, g)?;
            let v: Vec<Idx> = a.call_method0("ravel")?.call_method0("tolist")?.extract()?;
            blocks.insert((t, g), v);
        }
    }
    Ok(Some(blocks))
}

/// The block `(t, g)` of a filter map (upstream `(*filter_elements)(type,
/// ghost_type)`, which throws when it is absent); an empty block is "no
/// filter", as upstream's `empty_filter` comparison.
fn filter_block(
    filter: Option<&FilterBlocks>,
    t: ElementType,
    g: GhostType,
) -> Result<Option<Vec<Idx>>> {
    match filter {
        None => Ok(None),
        Some(f) => {
            let b = f.get(&(t, g)).ok_or_else(|| {
                akantu::core::Error::not_found("filter_elements block", format!("{t}:{g:?}"))
            })?;
            Ok((!b.is_empty()).then(|| b.clone()))
        }
    }
}

/// Write blocks into an `ElementTypeMapArrayReal`: new blocks of a map
/// created from Python, in place into the blocks of a map owned by akantu.
fn etma_write(
    py: Python<'_>,
    m: &Bound<'_, EtmaReal>,
    blocks: Vec<(ElementType, GhostType, Array<Real>)>,
) -> PyResult<()> {
    if let Some(own) = m.get().provider.owned().cloned() {
        return super::runtime::native(py, move || {
            let mut o = own.try_borrow_mut()?;
            for (t, g, a) in blocks {
                o.put(t, g, a)?;
            }
            Ok(())
        });
    }
    let p = &m.get().provider;
    for (t, g, a) in blocks {
        let view = p.call(py, t, g)?;
        let rows = a.len();
        let cols = a.nb_component();
        write_out(&view, a.as_slice().to_vec(), rows, cols)?;
    }
    Ok(())
}

/// The values of `field_funct(matrix, element)` for the `nb_element`
/// elements of `(ty, ghost)`, in element order (upstream `fillField`): one
/// zeroed Fortran-ordered `(nb_dof, nq)` NumPy matrix per element, read back
/// column-major after the call. Evaluated before the native section, so no
/// akantu array is borrowed while Python runs; an exception of the functor
/// propagates as is.
fn eval_field_funct(
    py: Python<'_>,
    func: &Bound<'_, PyAny>,
    nb_dof: usize,
    nq: usize,
    nb_element: usize,
    ty: ElementType,
    ghost: GhostType,
) -> PyResult<Vec<Real>> {
    let np = py.import("numpy")?;
    let order = pyo3::types::PyDict::new(py);
    order.set_item("order", "F")?;
    let f64t = np.getattr("float64")?;
    let mut field = Vec::with_capacity(nb_element * nb_dof * nq);
    for e in 0..nb_element {
        let element =
            Element::try_new(ty, e, ghost).map_err(|err| super::runtime::to_py(py, err))?;
        let mat = np.call_method("zeros", ((nb_dof, nq),), Some(&order))?;
        func.call1((mat.clone(), PyElement { element }))?;
        let flat = np.call_method1("asarray", (mat, &f64t))?;
        let v: Vec<Real> = flat
            .call_method("ravel", (), Some(&order))?
            .call_method0("tolist")?
            .extract()?;
        if v.len() != nb_dof * nq {
            return Err(super::errors::exception(
                py,
                format!("field_funct: the matrix must keep its shape ({nb_dof}, {nq})"),
            ));
        }
        field.extend_from_slice(&v);
    }
    Ok(field)
}

/// A [`akantu::fe::engine::FieldFunctor`] reading the values of
/// [`eval_field_funct`].
fn replay_field(field: &[Real], m: &mut [Real], e: Element) -> Result<()> {
    let n = m.len();
    let start = e.index() as usize * n;
    let src = field.get(start..start + n).ok_or_else(|| {
        akantu::core::Error::invalid("internal error: field functor values out of range")
    })?;
    m.copy_from_slice(src);
    Ok(())
}

/// Blocks `(type, ghost, nb_element · nb_quadrature_points, nb_component)`
/// of an `ElementTypeMapArray` initialised from an engine (upstream
/// `FEEngineElementTypeMapArrayInitializer`: the engine's types of its
/// element dimension and of `kind`, both ghost types unless one is given).
pub(crate) fn quadrature_blocks(
    py: Python<'_>,
    fe: &PyFeEngine,
    ghost: Option<GhostType>,
    nb_component: usize,
    kind: Option<ElementKind>,
) -> PyResult<Vec<(ElementType, GhostType, usize, usize)>> {
    fe.with(py, move |fe, mesh| {
        let ghosts: Vec<GhostType> = match ghost {
            Some(g) => vec![g],
            None => GhostType::ALL.to_vec(),
        };
        let mut out = Vec::new();
        for g in ghosts {
            let types: Vec<ElementType> = fe
                .element_types(mesh, g)
                .filter(|t| kind.is_none_or(|k| t.kind() == k))
                .collect();
            for t in types {
                let ne = mesh.nb_element(t, g);
                let rows = if ne == 0 {
                    0
                } else {
                    ne * fe.nb_integration_points(t, g)?
                };
                out.push((t, g, rows, nb_component));
            }
        }
        Ok(out)
    })
}

fn type_name(o: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(o.get_type().name()?.to_cow()?.into_owned())
}

#[pymethods]
impl PyFeEngine {
    /// `getNbIntegrationPoints(type, ghost_type=_not_ghost)`.
    #[pyo3(name = "getNbIntegrationPoints", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_nb_integration_points(
        &self,
        py: Python<'_>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<usize> {
        let (t, g) = (r#type.0, ghost_type.single()?);
        self.with(py, move |fe, _| fe.nb_integration_points(t, g))
    }

    /// `initShapeFunctions(ghost_type=_not_ghost)`: the model's engines are
    /// initialised at construction (and by mesh events); no-op.
    #[pyo3(name = "initShapeFunctions", signature = (ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn init_shape_functions(&self, ghost_type: GhostArg) -> PyResult<()> {
        ghost_type.single().map(|_| ())
    }

    /// `integrate(f, intf, nb_degree_of_freedom, type, ghost_type=_not_ghost,
    /// filter_elements=None)` (writes `intf`) or `integrate(f, type,
    /// ghost_type=_not_ghost, filter_elements=None)` → float.
    #[pyo3(signature = (f, *args, **kwargs))]
    fn integrate<'py>(
        &self,
        py: Python<'py>,
        f: &Bound<'py, PyAny>,
        args: &Bound<'py, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'py, pyo3::types::PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let kw = |n: &str| -> PyResult<Option<Bound<'py, PyAny>>> {
            Ok(match kwargs {
                Some(k) => k.get_item(n)?,
                None => None,
            })
        };
        let first_is_type = args.len() > 0 && args.get_item(0)?.extract::<EtArg>().is_ok()
            || kw("type")?.is_some() && kw("intf")?.is_none();
        if first_is_type {
            // integrate(f, type, ghost_type, filter_elements) -> Real
            let t: EtArg = match args.len() {
                0 => kw("type")?
                    .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("missing type"))?
                    .extract()?,
                _ => args.get_item(0)?.extract()?,
            };
            let g: GhostArg = match (args.len() > 1, kw("ghost_type")?) {
                (true, _) => args.get_item(1)?.extract()?,
                (false, Some(g)) => g.extract()?,
                (false, None) => GhostArg(Some(GhostType::NotGhost)),
            };
            let filt = if args.len() > 2 {
                Some(args.get_item(2)?)
            } else {
                kw("filter_elements")?
            };
            let filter = input_filter(filt.as_ref())?;
            let (values, nbc) = input_2d(f, None)?;
            let (t, g) = (t.0, g.single()?);
            let r = self.with(py, move |fe, _| {
                let v = ArrayView::new(&values, nbc)?;
                fe.integrate_scalar(v, t, g, filter.as_deref())
            })?;
            return Ok(pyo3::types::PyFloat::new(py, r).into_any());
        }
        // integrate(f, intf, nb_degree_of_freedom, type, ghost_type, filter)
        let get = |i: usize, n: &str| -> PyResult<Option<Bound<'py, PyAny>>> {
            if args.len() > i {
                Ok(Some(args.get_item(i)?))
            } else {
                kw(n)
            }
        };
        let intf = get(0, "intf")?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("missing intf"))?;
        if let Ok(intf) = intf.cast::<EtmaReal>() {
            // integrate(f: ElementTypeMapArrayReal, intf, filter_elements=None)
            let f = f.cast::<EtmaReal>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "integrate(f, intf: ElementTypeMapArrayReal, …): f must be an \
                     ElementTypeMapArrayReal",
                )
            })?;
            let kind = self.with(py, |fe, _| Ok(fe.kind()))?;
            let blocks = etma_real_blocks(f, kind)?;
            let filter = etma_filter_blocks(get(1, "filter_elements")?.as_ref())?;
            let out = self.with(py, move |fe, _| {
                let mut out = Vec::new();
                for (t, g, values, nbc) in blocks {
                    let filt = filter_block(filter.as_ref(), t, g)?;
                    let v = ArrayView::new(&values, nbc)?;
                    let mut o = Array::new(nbc);
                    fe.integrate(v, &mut o, nbc, t, g, filt.as_deref())?;
                    out.push((t, g, o));
                }
                Ok(out)
            })?;
            etma_write(py, intf, out)?;
            return Ok(py.None().into_bound(py));
        }
        let nb_dof: usize = get(1, "nb_degree_of_freedom")?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("missing nb_degree_of_freedom"))?
            .extract()?;
        let t: EtArg = get(2, "type")?
            .ok_or_else(|| pyo3::exceptions::PyTypeError::new_err("missing type"))?
            .extract()?;
        let g: GhostArg = match get(3, "ghost_type")? {
            Some(g) => g.extract()?,
            None => GhostArg(Some(GhostType::NotGhost)),
        };
        let filter = input_filter(get(4, "filter_elements")?.as_ref())?;
        let (values, _) = input_2d(f, Some(nb_dof))?;
        let (t, g) = (t.0, g.single()?);
        let (out, rows) = self.with(py, move |fe, _| {
            let v = ArrayView::new(&values, nb_dof)?;
            let mut out = Array::new(nb_dof);
            fe.integrate(v, &mut out, nb_dof, t, g, filter.as_deref())?;
            let rows = out.len();
            Ok((out.into_vec(), rows))
        })?;
        write_out(&intf, out, rows, nb_dof)?;
        Ok(py.None().into_bound(py))
    }

    /// `gradientOnIntegrationPoints(u, nablauq, nb_degree_of_freedom, type,
    /// ghost_type=_not_ghost, filter_elements=None)`.
    #[pyo3(name = "gradientOnIntegrationPoints", signature = (u, nablauq, nb_degree_of_freedom, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(clippy::too_many_arguments)] // upstream signature
    fn gradient_on_integration_points(
        &self,
        py: Python<'_>,
        u: &Bound<'_, PyAny>,
        nablauq: &Bound<'_, PyAny>,
        nb_degree_of_freedom: usize,
        r#type: EtArg,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let filter = input_filter(filter_elements)?;
        let (values, _) = input_2d(u, Some(nb_degree_of_freedom))?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (out, rows, cols) = self.with(py, move |fe, mesh| {
            let v = ArrayView::new(&values, nb_degree_of_freedom)?;
            let cols = nb_degree_of_freedom * fe.element_dimension();
            let mut out = Array::new(cols);
            fe.gradient_on_integration_points(
                mesh,
                v,
                &mut out,
                nb_degree_of_freedom,
                t,
                g,
                filter.as_deref(),
            )?;
            let rows = out.len();
            Ok((out.into_vec(), rows, cols))
        })?;
        write_out(nablauq, out, rows, cols)
    }

    /// `interpolateOnIntegrationPoints(u, uq, nb_degree_of_freedom, type,
    /// ghost_type=_not_ghost, filter_elements=None)`.
    #[pyo3(name = "interpolateOnIntegrationPoints", signature = (u, uq, nb_degree_of_freedom = None, r#type = None, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(clippy::too_many_arguments)] // upstream signature
    fn interpolate_on_integration_points(
        &self,
        py: Python<'_>,
        u: &Bound<'_, PyAny>,
        uq: &Bound<'_, PyAny>,
        nb_degree_of_freedom: Option<&Bound<'_, PyAny>>,
        r#type: Option<EtArg>,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if let Ok(uq) = uq.cast::<EtmaReal>() {
            // interpolateOnIntegrationPoints(u, uq: ElementTypeMapArrayReal,
            // filter_elements=None); the filter may come third positionally
            let filt = match (filter_elements, nb_degree_of_freedom) {
                (Some(f), None) => Some(f.clone()),
                (None, Some(f)) => Some(f.clone()),
                (None, None) => None,
                (Some(_), Some(_)) => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "interpolateOnIntegrationPoints: filter_elements given twice",
                    ));
                }
            };
            if r#type.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "interpolateOnIntegrationPoints(u, uq: ElementTypeMapArrayReal, \
                     filter_elements=None)",
                ));
            }
            let kind = self.with(py, |fe, _| Ok(fe.kind()))?;
            let targets: Vec<(ElementType, GhostType, usize)> = etma_real_blocks(uq, kind)?
                .into_iter()
                .map(|(t, g, _, nbc)| (t, g, nbc))
                .collect();
            let filter = etma_filter_blocks(filt.as_ref())?;
            let (values, _) = input_2d(u, None)?;
            let out = self.with(py, move |fe, mesh| {
                let mut out = Vec::new();
                for (t, g, nbc) in targets {
                    let f = filter_block(filter.as_ref(), t, g)?;
                    let v = ArrayView::new(&values, nbc)?;
                    let mut o = Array::new(nbc);
                    fe.interpolate_on_integration_points(mesh, v, &mut o, nbc, t, g, f.as_deref())?;
                    out.push((t, g, o));
                }
                Ok(out)
            })?;
            return etma_write(py, uq, out);
        }
        let nb_degree_of_freedom = nb_degree_of_freedom
            .map(|n| n.extract::<usize>())
            .transpose()?;
        let (Some(nb_dof), Some(t)) = (nb_degree_of_freedom, r#type) else {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "interpolateOnIntegrationPoints(u, uq, nb_degree_of_freedom, type, \
                 ghost_type=_not_ghost, filter_elements=None)",
            ));
        };
        let filter = input_filter(filter_elements)?;
        let (values, _) = input_2d(u, Some(nb_dof))?;
        let (t, g) = (t.0, ghost_type.single()?);
        let (out, rows) = self.with(py, move |fe, mesh| {
            let v = ArrayView::new(&values, nb_dof)?;
            let mut out = Array::new(nb_dof);
            fe.interpolate_on_integration_points(
                mesh,
                v,
                &mut out,
                nb_dof,
                t,
                g,
                filter.as_deref(),
            )?;
            let rows = out.len();
            Ok((out.into_vec(), rows))
        })?;
        write_out(uq, out, rows, nb_dof)
    }

    /// `computeBtD(Ds, BtDs, type, ghost_type=_not_ghost,
    /// filter_elements=None)`.
    #[pyo3(name = "computeBtD", signature = (Ds, BtDs, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(non_snake_case)] // upstream argument names
    fn compute_btd(
        &self,
        py: Python<'_>,
        Ds: &Bound<'_, PyAny>,
        BtDs: &Bound<'_, PyAny>,
        r#type: EtArg,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let filter = input_filter(filter_elements)?;
        let (values, nbc) = input_2d(Ds, None)?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (out, rows, cols) = self.with(py, move |fe, _| {
            let mut out = Array::new(1);
            fe.compute_btd(
                ArrayView::new(&values, nbc)?,
                &mut out,
                t,
                g,
                filter.as_deref(),
            )?;
            let (rows, cols) = (out.len(), out.nb_component());
            Ok((out.into_vec(), rows, cols))
        })?;
        write_out(BtDs, out, rows, cols)
    }

    /// `computeBtDB(Ds, BtDBs, order_d, type, ghost_type=_not_ghost,
    /// filter_elements=None)`.
    #[pyo3(name = "computeBtDB", signature = (Ds, BtDBs, order_d, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(non_snake_case, clippy::too_many_arguments)] // upstream signature
    fn compute_btdb(
        &self,
        py: Python<'_>,
        Ds: &Bound<'_, PyAny>,
        BtDBs: &Bound<'_, PyAny>,
        order_d: usize,
        r#type: EtArg,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let filter = input_filter(filter_elements)?;
        let (values, nbc) = input_2d(Ds, None)?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (out, rows, cols) = self.with(py, move |fe, _| {
            let mut out = Array::new(1);
            fe.compute_btdb(
                ArrayView::new(&values, nbc)?,
                &mut out,
                order_d,
                t,
                g,
                filter.as_deref(),
            )?;
            let (rows, cols) = (out.len(), out.nb_component());
            Ok((out.into_vec(), rows, cols))
        })?;
        write_out(BtDBs, out, rows, cols)
    }

    /// `computeNtb(bs, Ntbs, type, ghost_type=_not_ghost,
    /// filter_elements=None)`.
    #[pyo3(name = "computeNtb", signature = (bs, Ntbs, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(non_snake_case)] // upstream argument names
    fn compute_ntb(
        &self,
        py: Python<'_>,
        bs: &Bound<'_, PyAny>,
        Ntbs: &Bound<'_, PyAny>,
        r#type: EtArg,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let filter = input_filter(filter_elements)?;
        let (values, nbc) = input_2d(bs, None)?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (out, rows, cols) = self.with(py, move |fe, _| {
            let mut out = Array::new(1);
            fe.compute_ntb(
                ArrayView::new(&values, nbc)?,
                &mut out,
                t,
                g,
                filter.as_deref(),
            )?;
            let (rows, cols) = (out.len(), out.nb_component());
            Ok((out.into_vec(), rows, cols))
        })?;
        write_out(Ntbs, out, rows, cols)
    }

    /// `computeNtbN(bs, NtbNs, type, ghost_type=_not_ghost,
    /// filter_elements=None)`.
    #[pyo3(name = "computeNtbN", signature = (bs, NtbNs, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), filter_elements = None))]
    #[allow(non_snake_case)] // upstream argument names
    fn compute_ntbn(
        &self,
        py: Python<'_>,
        bs: &Bound<'_, PyAny>,
        NtbNs: &Bound<'_, PyAny>,
        r#type: EtArg,
        ghost_type: GhostArg,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let filter = input_filter(filter_elements)?;
        let (values, nbc) = input_2d(bs, None)?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (out, rows, cols) = self.with(py, move |fe, _| {
            let mut out = Array::new(1);
            fe.compute_ntbn(
                ArrayView::new(&values, nbc)?,
                &mut out,
                t,
                g,
                filter.as_deref(),
            )?;
            let (rows, cols) = (out.len(), out.nb_component());
            Ok((out.into_vec(), rows, cols))
        })?;
        write_out(NtbNs, out, rows, cols)
    }

    /// `assembleFieldLumped(field_funct, lumped_matrix, dof_manager, type,
    /// ghost_type=_not_ghost)`: scatter-adds into the NumPy `lumped_matrix`
    /// (`nb_dof` = its number of columns).
    #[pyo3(name = "assembleFieldLumped", signature = (field_funct, lumped_matrix, dof_manager, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn assemble_field_lumped(
        &self,
        py: Python<'_>,
        field_funct: &Bound<'_, PyAny>,
        lumped_matrix: &Bound<'_, PyAny>,
        dof_manager: &Bound<'_, PyDofManager>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<()> {
        let _ = dof_manager;
        let (values, nb_dof) = input_2d(lumped_matrix, None)?;
        let (t, g) = (r#type.0, ghost_type.single()?);
        // upstream: getNbIntegrationPoints(type) (not-ghost rule)
        let (ne, nq) = self.with(py, move |fe, mesh| {
            Ok((
                mesh.nb_element(t, g),
                fe.nb_integration_points(t, GhostType::NotGhost)?,
            ))
        })?;
        let field = eval_field_funct(py, field_funct, nb_dof, nq, ne, t, g)?;
        let (out, rows) = self.with(py, move |fe, mesh| {
            let mut funct = |m: &mut [Real], e: Element| replay_field(&field, m, e);
            let elementary = fe.lumped_field_elemental(mesh, &mut funct, nb_dof, t, g)?;
            let mut out = Array::from_vec(values, nb_dof)?;
            akantu::solver::DofManager::assemble_elemental_array_local_array(
                mesh,
                elementary.view(),
                &mut out,
                t,
                g,
                1.0,
                None,
            )?;
            let rows = out.len();
            Ok((out.into_vec(), rows))
        })?;
        write_out(lumped_matrix, out, rows, nb_dof)
    }

    /// `assembleFieldMatrix(field_funct, matrix_id, dof_id, dof_manager,
    /// type, ghost_type=_not_ghost)`: symmetric elemental matrices
    /// `∫ Nᵀ diag(f) N` assembled into `matrix_id` (`nb_dof` = the
    /// components of the DOF family `dof_id`).
    #[pyo3(name = "assembleFieldMatrix", signature = (field_funct, matrix_id, dof_id, dof_manager, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    #[allow(clippy::too_many_arguments)] // upstream signature
    fn assemble_field_matrix(
        &self,
        py: Python<'_>,
        field_funct: &Bound<'_, PyAny>,
        matrix_id: String,
        dof_id: String,
        dof_manager: &Bound<'_, PyDofManager>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<()> {
        let dm = dof_manager.get().model.clone();
        let (t, g) = (r#type.0, ghost_type.single()?);
        let id = dof_id.clone();
        let nb_dof = with_dofs(py, &dm, move |d| Ok(d.dofs(&id)?.nb_component()))?;
        // the mass rule (order 2·degree unless configured)
        let (ne, nq) = self.with(py, move |fe, mesh| {
            let order = fe.integration_orders().mass_order(t);
            Ok((
                mesh.nb_element(t, g),
                akantu::fe::gauss_rule(t, order)?.nb_points(),
            ))
        })?;
        let field = eval_field_funct(py, field_funct, nb_dof, nq, ne, t, g)?;
        let elementary = self.with(py, move |fe, mesh| {
            let mut funct = |m: &mut [Real], e: Element| replay_field(&field, m, e);
            fe.field_matrix_elemental(mesh, &mut funct, nb_dof, t, g)
        })?;
        let mesh = dm.h.mesh.clone();
        with_dofs(py, &dm, move |d| {
            mesh.h.read(mesh.part, |m| {
                d.assemble_elemental_matrices_to_matrix(
                    m,
                    &matrix_id,
                    &dof_id,
                    elementary.view(),
                    t,
                    g,
                    MatrixType::Symmetric,
                    None,
                )
            })
        })
    }

    /// `computeIntegrationPointsCoordinates(coordinates,
    /// filter_elements=None)`: fills the blocks of `coordinates` (an
    /// `ElementTypeMapArrayReal` created from Python, or a law internal such
    /// as `registerInternalReal("quad_coordinates", dim)`), one per element
    /// type of the engine or of `filter_elements` (both ghost types; a
    /// filter map such as a law's `getElementFilter()` snapshot restricts
    /// the elements).
    #[pyo3(name = "computeIntegrationPointsCoordinates", signature = (coordinates, filter_elements = None))]
    fn compute_integration_points_coordinates(
        &self,
        py: Python<'_>,
        coordinates: &Bound<'_, EtmaReal>,
        filter_elements: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        // the filter's blocks are copied before the native section
        let mut filter: Option<Vec<(ElementType, GhostType, Vec<Idx>)>> = None;
        if let Some(fobj) = filter_elements.filter(|f| !f.is_none()) {
            let fmap = fobj.cast::<super::etma::EtmaInt>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err(
                    "filter_elements must be an ElementTypeMapArrayInt",
                )
            })?;
            let p = &fmap.get().provider;
            let mut blocks = Vec::new();
            for g in GhostType::ALL {
                let tf = akantu::core::TypeFilter {
                    spatial_dimension: None,
                    ghost_type: g,
                    kind: None,
                };
                for t in p.types(py, tf)? {
                    let a = p.call(py, t, g)?;
                    let v: Vec<Idx> = a.call_method0("ravel")?.call_method0("tolist")?.extract()?;
                    blocks.push((t, g, v));
                }
            }
            filter = Some(blocks);
        }
        let blocks = self.with(py, move |fe, mesh| {
            let mut out: Vec<(ElementType, GhostType, Array<Real>)> = Vec::new();
            match filter {
                None => {
                    for g in GhostType::ALL {
                        let types: Vec<ElementType> = fe.element_types(mesh, g).collect();
                        for t in types {
                            let mut a = Array::new(mesh.spatial_dimension());
                            fe.integration_points_coordinates(mesh, &mut a, t, g, None)?;
                            out.push((t, g, a));
                        }
                    }
                }
                Some(blocks) => {
                    for (t, g, f) in blocks {
                        let mut a = Array::new(mesh.spatial_dimension());
                        fe.integration_points_coordinates(mesh, &mut a, t, g, Some(&f))?;
                        out.push((t, g, a));
                    }
                }
            }
            Ok(out)
        })?;
        if let Some(own) = coordinates.get().provider.owned().cloned() {
            return super::runtime::native(py, move || {
                let mut o = own.try_borrow_mut()?;
                for (t, g, a) in blocks {
                    o.put(t, g, a)?;
                }
                Ok(())
            });
        }
        // a map owned by akantu (a law internal): write into its blocks
        let p = &coordinates.get().provider;
        for (t, g, a) in blocks {
            let view = p.call(py, t, g)?;
            let rows = a.len();
            let cols = a.nb_component();
            write_out(&view, a.as_slice().to_vec(), rows, cols)?;
        }
        Ok(())
    }

    /// `getElementInradius(element)`.
    #[pyo3(name = "getElementInradius")]
    fn get_element_inradius(
        &self,
        py: Python<'_>,
        element: &Bound<'_, PyElement>,
    ) -> PyResult<Real> {
        let el = element.get().element;
        self.with(py, move |fe, mesh| fe.element_inradius(mesh, el))
    }

    /// `getNormalsOnIntegrationPoints(type, ghost_type=_not_ghost)`
    /// (read-only copy; boundary engines).
    #[pyo3(name = "getNormalsOnIntegrationPoints", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_normals_on_integration_points<'py>(
        &self,
        py: Python<'py>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (v, r, c) = self.with(py, move |fe, _| {
            let a = fe.normals_on_integration_points(t, g)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        read_only(owned_numpy(py, v, r, c, Layout::Rows)?)
    }

    /// `getShapes(type, ghost_type=_not_ghost, id=0)` (read-only copy).
    #[pyo3(name = "getShapes", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), id = 0))]
    fn get_shapes<'py>(
        &self,
        py: Python<'py>,
        r#type: EtArg,
        ghost_type: GhostArg,
        id: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = id; // single interpolation per type
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (v, r, c) = self.with(py, move |fe, _| {
            let a = fe.shapes(t, g)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        read_only(owned_numpy(py, v, r, c, Layout::Rows)?)
    }

    /// `getShapesDerivatives(type, ghost_type=_not_ghost, id=0)` (read-only
    /// copy; per point a column-major `dim × nb_shapes` block).
    #[pyo3(name = "getShapesDerivatives", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost)), id = 0))]
    fn get_shapes_derivatives<'py>(
        &self,
        py: Python<'py>,
        r#type: EtArg,
        ghost_type: GhostArg,
        id: i64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = id;
        let (t, g) = (r#type.0, ghost_type.single()?);
        let (v, r, c) = self.with(py, move |fe, _| {
            let a = fe.shapes_derivatives(t, g)?;
            Ok((a.as_slice().to_vec(), a.len(), a.nb_component()))
        })?;
        read_only(owned_numpy(py, v, r, c, Layout::Rows)?)
    }

    /// `getMesh()`: the model's mesh.
    #[pyo3(name = "getMesh")]
    fn get_mesh<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyMesh>> {
        PyMesh::wrap(py, self.model.h.mesh.clone())
    }
}

fn read_only(a: Bound<'_, PyAny>) -> PyResult<Bound<'_, PyAny>> {
    a.call_method1("setflags", (false,))?;
    Ok(a)
}

/// Register `FEEngine`.
pub(crate) fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyFeEngine>()
}
