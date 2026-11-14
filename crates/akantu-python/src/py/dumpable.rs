// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_dumpable.cc (register_dumpable:
// registerDumperParaview, addDumpMesh(ToDumper), addDumpField(ToDumper),
// addDumpFieldExternal(ToDumper), dump overloads), src/io/dumper/dumpable.cc,
// src/io/dumper/dumpable_inline_impl.hh.

//! `Dumpable` (PY-048; group 3).
//!
//! Upstream `Mesh` and `ElementGroup` are `Dumpable`. The port keeps the
//! dumpers of a mesh domain and of its groups in the domain's registry
//! (`akantu::io::dumper::group::SharedMeshDumpers`, ADR "group-dumper
//! registry"): `mesh.registerDumperParaview(…)`, `group.dump()`, … act on
//! the same dumpers the models use for `dumpGroup`. `ElementGroup` derives
//! from the `Dumpable` class; `Mesh` (whose single base is `GroupManager`)
//! carries the methods itself.
//!
//! `addDumpFieldExternal(field_id, array)` registers a nodal field read from
//! the NumPy array **at dump time** (upstream wraps the array's buffer in a
//! proxy kept in a global vector whose reallocation leaves the earlier
//! proxies dangling, UD-835: the port keeps a reference to the array).
//! Upstream's `dumpers::Field` objects come from `dumper_module`, which
//! cannot be imported from the upstream package (PY-049): the overloads
//! taking one raise `TypeError`. `addDumpField(ToDumper)` of a mesh or a
//! group is upstream's base `Dumpable` method (`AKANTU_TO_IMPLEMENT`).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use akantu::core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu::io::dumper::group::SharedMeshDumpers;
use akantu::io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshSelection, Padding,
};
use akantu::mesh::Mesh;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use super::arrays::input_f64;
use super::mesh::{MeshRef, PyMesh};
use super::runtime::native;

/// `Dumpable` (base class of `ElementGroup`; `Mesh` carries the methods).
#[pyclass(subclass, frozen, name = "Dumpable", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyDumpable;

/// An external field registered from Python.
#[derive(Debug)]
pub(crate) struct ExternalField {
    /// Element group of the dumpable (`None`: the mesh).
    group: Option<String>,
    /// Dumper (`None`: the default dumper).
    dumper: Option<String>,
    /// Field name.
    field: String,
    /// The NumPy array (read at dump time).
    array: Py<PyAny>,
}

/// External fields of a mesh domain's dumpables.
#[derive(Debug, Default)]
pub(crate) struct ExternalFields(Mutex<Vec<ExternalField>>);

impl ExternalFields {
    fn lock(&self) -> MutexGuard<'_, Vec<ExternalField>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a `Dumpable` object designates.
#[derive(Clone, Debug)]
pub(crate) struct DumpTarget {
    pub(crate) mesh: MeshRef,
    /// Element group (`None`: the mesh).
    pub(crate) group: Option<String>,
}

impl DumpTarget {
    /// Run `f` on the target's dumpable (native section).
    fn with<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut Dumpable, &Mesh) -> Result<R> + Send,
    ) -> PyResult<R> {
        let (shared, group) = (self.mesh.h.shared.clone(), self.group.clone());
        native(py, move || {
            let reg = SharedMeshDumpers::of(&shared)?;
            let mut reg = reg.lock()?;
            let mesh = shared.read()?;
            let d = match &group {
                None => reg.mesh(&mesh),
                Some(g) => reg.element_group(&mesh, g)?,
            };
            f(d, &mesh)
        })
    }
}

/// Values of the external fields of one dump.
struct ExternalResolver(BTreeMap<String, (usize, Vec<Real>)>);

impl FieldResolver for ExternalResolver {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let (FieldSource::ModelNodal(name), None) = (&field.source, ty) else {
            return Err(Error::Unsupported(format!(
                "dump field '{}' of a mesh dumpable (only external nodal fields)",
                field.name
            )));
        };
        let (nbc, values) = self
            .0
            .get(name)
            .ok_or_else(|| Error::not_found("external dump field", name))?;
        Ok(DumpBuffer::Real {
            nb_component: *nbc,
            values: values.clone(),
        })
    }
}

fn selection(dimension: i64, ghost: GhostType, kind: ElementKind) -> PyResult<MeshSelection> {
    let dim = super::mesh::dim_filter(dimension)?;
    Ok(MeshSelection::new(dim, ghost, kind))
}

fn same_mesh(target: &DumpTarget, mesh: &Bound<'_, PyMesh>) -> PyResult<()> {
    let m = PyMesh::mesh_ref(mesh);
    if std::sync::Arc::ptr_eq(&m.h, &target.mesh.h) {
        Ok(())
    } else {
        Err(super::errors::exception(
            mesh.py(),
            "addDumpMesh: only the dumpable's own mesh can be registered".into(),
        ))
    }
}

/// `registerDumperParaview(dumper_name, file_name, is_default=False)`.
pub(crate) fn register_dumper_paraview(
    py: Python<'_>,
    target: &DumpTarget,
    dumper_name: String,
    file_name: String,
    is_default: bool,
) -> PyResult<()> {
    target.with(py, move |d, _| {
        d.register_paraview(&dumper_name, Some(&file_name), is_default);
        Ok(())
    })
}

/// `addDumpMesh(ToDumper)(…, mesh, dimension, ghost_type, element_kind)`.
pub(crate) fn add_dump_mesh(
    py: Python<'_>,
    target: &DumpTarget,
    dumper_name: Option<String>,
    mesh: &Bound<'_, PyMesh>,
    dimension: i64,
    ghost: GhostType,
    kind: ElementKind,
) -> PyResult<()> {
    same_mesh(target, mesh)?;
    let sel = selection(dimension, ghost, kind)?;
    target.with(py, move |d, _| match dumper_name {
        Some(n) => d.add_dump_mesh_to_dumper(&n, sel),
        None => d.add_dump_mesh(sel),
    })
}

/// `addDumpField(ToDumper)`: upstream's base `Dumpable` method.
pub(crate) fn add_dump_field(py: Python<'_>) -> PyResult<()> {
    Err(super::errors::exception(
        py,
        "addDumpField is not implemented for this Dumpable (upstream AKANTU_TO_IMPLEMENT)".into(),
    ))
}

/// `addDumpFieldExternal(ToDumper)(…, field_id, field)`.
pub(crate) fn add_dump_field_external(
    py: Python<'_>,
    target: &DumpTarget,
    externals: &ExternalFields,
    dumper_name: Option<String>,
    field_id: String,
    field: &Bound<'_, PyAny>,
) -> PyResult<()> {
    if dumper_name.is_some() || field.getattr("shape").is_err() {
        return Err(PyTypeError::new_err(
            "addDumpFieldExternal(ToDumper) with a dumpers::Field: the dumper module is not \
             part of the Python package (PY-049); pass a NumPy array",
        ));
    }
    let shape: Vec<usize> = field.getattr("shape")?.extract()?;
    let nbc = shape.get(1).copied().unwrap_or(1);
    let desc = FieldDescriptor {
        name: field_id.clone(),
        source: FieldSource::ModelNodal(field_id.clone()),
        support: FieldSupport::Nodal,
        value_type: FieldValueType::Real,
        group: None,
        padding: if nbc > 1 {
            Padding::To3D
        } else {
            Padding::None
        },
        homogenize: false,
    };
    let dn = dumper_name.clone();
    target.with(py, move |d, _| match dn {
        Some(n) => d.add_dump_field_to_dumper(&n, desc),
        None => d.add_dump_field(desc),
    })?;
    let mut ext = externals.lock();
    ext.retain(|e| !(e.group == target.group && e.dumper == dumper_name && e.field == field_id));
    ext.push(ExternalField {
        group: target.group.clone(),
        dumper: dumper_name,
        field: field_id,
        array: field.clone().unbind(),
    });
    Ok(())
}

/// `dump()`, `dump(step)`, `dump(time, step)`, `dump(dumper_name)`,
/// `dump(dumper_name, step)`, `dump(dumper_name, time, step)`.
pub(crate) fn dump(
    py: Python<'_>,
    target: &DumpTarget,
    externals: &ExternalFields,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let (dumper, time, step) = super::model::dump_args(args, kwargs)?;
    // the external arrays' current values, copied before the native section
    let mut values = BTreeMap::new();
    for e in externals.lock().iter().filter(|e| e.group == target.group) {
        let (v, shape) = input_f64(e.array.bind(py))?;
        let nbc = shape.get(1).copied().unwrap_or(1).max(1);
        values.insert(e.field.clone(), (nbc, v));
    }
    let resolver = ExternalResolver(values);
    target.with(py, move |d, mesh| match dumper {
        Some(n) => d.dump_named(&n, mesh, &resolver, time, step),
        None => d.dump(mesh, &resolver, time, step),
    })
}
