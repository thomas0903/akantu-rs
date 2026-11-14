// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_mesh.cc (register_mesh: Mesh,
// MeshData, PeriodicSlaves, MeshUtils, MeshAccessor),
// python/py_group_manager.cc (NodeGroup, ElementGroup, GroupManager,
// ClusteringFilter).

//! Mesh bindings (PY-050…055).
//!
//! A Python `Mesh` owns a [`SharedMesh`] (the mesh domain, D-019) behind an
//! `Arc`, shared with every model built on it (`keep_alive` of upstream).
//! Arrays returned by reference upstream (`getNodes`, `getConnectivity`,
//! group lists, mesh data) are **zero-copy views of the mesh's storage**
//! (D-047, [`super::exports`]); all are writable as upstream, except that
//! once a model uses the mesh the topology arrays (connectivities, group
//! lists) are read-only views and views of them fetched before go stale
//! (UD-532). Node coordinates stay writable: a write lands in the mesh at
//! once (upstream semantics: the model uses the new coordinates from its
//! next computation that reads them).
//!
//! **Modifying the mesh** (reading a file, `MeshAccessor`, `buildFacets`,
//! group creation, writes into node/connectivity views) is a set-up
//! operation of the domain: allowed while no model uses the mesh. Once a
//! model was created on it, such a call raises `akantu.Exception` (the
//! domain's topology changes go through `SharedMesh::mutate` with every
//! consumer's handler; upstream lets Python change a mesh under a running
//! model, UD-024).
//!
//! `Mesh` derives from `GroupManager` (as upstream); upstream's other bases
//! `Dumpable` and `MeshData` are flattened into `Mesh` (PyO3 has single
//! inheritance): `getElementalDataUInt`/`getElementalDataReal` are methods of
//! `Mesh`, and the `MeshData` class exists for `isinstance` checks only.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use akantu::core::enums::SpatialDirection;
use akantu::core::{Element, ElementType, Error, GhostType, Idx, Result, TypeFilter};
use akantu::mesh::group::ALL_DIMENSIONS;
use akantu::mesh::utils::NodeToElements;
use akantu::mesh::{ExportSlot, Mesh, MeshAccessor, MeshEvents, MeshExports, MeshPart, SharedMesh};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use super::common::PyElement;
use super::dumpable::{DumpTarget, PyDumpable};
use super::enums::{DirectionArg, EtArg, GhostArg, KindArg, MeshIoArg, element_type_obj};
use super::etma::{EtmaInt, EtmaReal, EtmaSource, EtmaUInt};
use super::exports::{ExportTable, into_numpy};
use super::runtime::{in_callback, native};
use crate::foreign_view::ForeignView;
use crate::foreign_view::numpy_export::{Exportable, Layout};

/// Upstream `_all_dimensions` (`std::numeric_limits<Int>::max()`).
pub(crate) const PY_ALL_DIMENSIONS: i64 = i32::MAX as i64;

/// A spatial-dimension filter argument (`_all_dimensions` = no filter).
pub(crate) fn dim_filter(d: i64) -> PyResult<Option<usize>> {
    if d == PY_ALL_DIMENSIONS {
        return Ok(None);
    }
    usize::try_from(d).map(Some).map_err(|_| {
        pyo3::exceptions::PyValueError::new_err(format!("invalid spatial dimension {d}"))
    })
}

/// A group dimension argument (`_all_dimensions` = [`ALL_DIMENSIONS`]).
fn group_dim(d: i64) -> PyResult<usize> {
    Ok(dim_filter(d)?.unwrap_or(ALL_DIMENSIONS))
}

// --- exports -------------------------------------------------------------------

/// Dtype of a mesh-data block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DataDtype {
    Real,
    Int,
    UInt,
    Bool,
}

/// Arrays of a mesh exported to Python (zero-copy views, D-047).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MeshKey {
    /// `getNodes()`.
    Nodes,
    /// `getConnectivity(type, ghost)`.
    Conn(ElementType, GhostType),
    /// `NodeGroup.getNodes()` (by node-group name).
    NodeGroup(String),
    /// `ElementGroup.getElements()(type, ghost)`.
    GroupElements(String, ElementType, GhostType),
    /// Mesh data `(name, type, ghost)`.
    Data(String, ElementType, GhostType, DataDtype),
}

impl MeshKey {
    /// Topology arrays become read-only views once a model uses the mesh
    /// (and earlier views go stale): UD-532.
    fn is_topology(&self) -> bool {
        matches!(
            self,
            Self::Conn(..) | Self::NodeGroup(_) | Self::GroupElements(..)
        )
    }
}

/// Element types of mesh arrays: the export slot of a key.
pub(crate) trait MeshValue: Exportable + Default + std::fmt::Debug {
    /// Dtype of mesh-data blocks of this type.
    const DATA: DataDtype;
    fn slot<'e>(e: &'e mut MeshExports<'_>, key: &MeshKey) -> Result<ExportSlot<'e, Self>>;
}

fn wrong_key(key: &MeshKey) -> Error {
    Error::invalid(format!(
        "internal error: {key:?} accessed with another dtype"
    ))
}

impl MeshValue for f64 {
    const DATA: DataDtype = DataDtype::Real;
    fn slot<'e>(e: &'e mut MeshExports<'_>, key: &MeshKey) -> Result<ExportSlot<'e, Self>> {
        match key {
            MeshKey::Nodes => Ok(e.nodes()),
            MeshKey::Data(n, t, g, DataDtype::Real) => e.data_real(n, *t, *g),
            _ => Err(wrong_key(key)),
        }
    }
}

impl MeshValue for i32 {
    const DATA: DataDtype = DataDtype::Int;
    fn slot<'e>(e: &'e mut MeshExports<'_>, key: &MeshKey) -> Result<ExportSlot<'e, Self>> {
        match key {
            MeshKey::Conn(t, g) => e.connectivity(*t, *g),
            MeshKey::NodeGroup(n) => e.node_group_nodes(n),
            MeshKey::GroupElements(n, t, g) => e.group_elements(n, *t, *g),
            MeshKey::Data(n, t, g, DataDtype::Int) => e.data_int(n, *t, *g),
            _ => Err(wrong_key(key)),
        }
    }
}

impl MeshValue for u32 {
    const DATA: DataDtype = DataDtype::UInt;
    fn slot<'e>(e: &'e mut MeshExports<'_>, key: &MeshKey) -> Result<ExportSlot<'e, Self>> {
        match key {
            MeshKey::Data(n, t, g, DataDtype::UInt) => e.data_uint(n, *t, *g),
            _ => Err(wrong_key(key)),
        }
    }
}

impl MeshValue for bool {
    const DATA: DataDtype = DataDtype::Bool;
    fn slot<'e>(e: &'e mut MeshExports<'_>, key: &MeshKey) -> Result<ExportSlot<'e, Self>> {
        match key {
            MeshKey::Data(n, t, g, DataDtype::Bool) => e.data_bool(n, *t, *g),
            _ => Err(wrong_key(key)),
        }
    }
}

/// The mesh of `part` in a domain guard.
pub(crate) fn select(mesh: &Mesh, part: MeshPart) -> Result<&Mesh> {
    match part {
        MeshPart::Main => Ok(mesh),
        MeshPart::Facets => mesh
            .mesh_facets()
            .ok_or_else(|| Error::not_found("facet mesh", "No facet mesh is defined yet!")),
    }
}

fn select_mut(mesh: &mut Mesh, part: MeshPart) -> Result<&mut Mesh> {
    match part {
        MeshPart::Main => Ok(mesh),
        MeshPart::Facets => mesh
            .mesh_facets_mut()
            .ok_or_else(|| Error::not_found("facet mesh", "No facet mesh is defined yet!")),
    }
}

/// `Mesh.distribute()` of the MPI build over the mesh's MPI communicator
/// (collective; refused, without any MPI call, on a thread the MPI thread
/// level does not allow): Scotch on rank 0 (upstream `distributeImpl` with
/// `MeshPartitionScotch` seeded with `RandomGenerator<Int>::seed()`), no
/// partitioner on one rank (upstream only creates the synchronizers). No
/// model uses the mesh yet (the set-up refuses otherwise), so the
/// distribution event has no receiver.
#[cfg(feature = "mpi")]
fn distribute_over_mpi(m: &mut Mesh) -> Result<()> {
    use akantu::mesh::distribution::{MeshPartitioner, distribute_with};
    let comm = m.communicator().clone();
    // A thread MPI's thread level does not allow (`funneled`: a Python
    // worker thread) is refused here, before the collective protocol
    // starts, so the communicator stays usable (a refusal inside the
    // protocol breaks it).
    comm.check_thread()?;
    let mut scotch = (comm.size() > 1 && comm.rank() == 0)
        .then(akantu::mesh::scotch::ScotchPartitioner::from_global_seed);
    let partitioner = scotch.as_mut().map(|p| p as &mut dyn MeshPartitioner);
    distribute_with(m, partitioner, 0).map(drop)
}

/// Export-table key of a mesh array (`(part slot, key)`).
type PartKey = (usize, MeshKey);

/// Binding-side owner of a mesh domain.
#[derive(Debug)]
pub(crate) struct MeshHandle {
    pub(crate) shared: SharedMesh,
    /// Export leases of the arrays Python fetched (bookkeeping, no values).
    exports: Mutex<ExportTable<PartKey>>,
    /// Cache of `fillNodesToElements` (upstream `nodes_to_elements`), per
    /// part, with the mesh revision it was computed at.
    n2e: Mutex<[Option<(u64, NodeToElements)>; 2]>,
    /// External dump fields registered from Python (`Dumpable`).
    pub(crate) externals: super::dumpable::ExternalFields,
}

fn part_slot(part: MeshPart) -> usize {
    match part {
        MeshPart::Main => 0,
        MeshPart::Facets => 1,
    }
}

fn slot_part(slot: usize) -> MeshPart {
    if slot == 0 {
        MeshPart::Main
    } else {
        MeshPart::Facets
    }
}

impl MeshHandle {
    pub(crate) fn new(mesh: Mesh) -> Arc<Self> {
        Arc::new(Self {
            shared: SharedMesh::new(mesh),
            exports: Mutex::new(ExportTable::default()),
            n2e: Mutex::new([None, None]),
            externals: super::dumpable::ExternalFields::default(),
        })
    }

    fn table(&self) -> MutexGuard<'_, ExportTable<PartKey>> {
        // only short, non-panicking critical sections
        self.exports.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `true` once a model consumes the mesh.
    fn has_model(&self) -> bool {
        self.shared
            .consumer_names()
            .is_ok_and(|names| !names.is_empty())
    }

    fn setup_raw<R>(&self, f: impl FnOnce(&mut Mesh) -> Result<R>) -> Result<R> {
        let mut out = None;
        self.shared
            .setup(|mesh| {
                out = Some(f(mesh)?);
                Ok(MeshEvents::none())
            })
            .map_err(|e| match e {
                Error::Mesh(msg) if msg.contains("without consumers") => Error::Mesh(format!(
                    "the mesh is used by a model: it cannot be modified from Python any more \
                     ({msg})"
                )),
                other => other,
            })?;
        out.ok_or_else(|| Error::invalid("internal error: mesh set-up produced no result"))
    }

    /// Run `f` on the mesh of `part` as a set-up operation (native
    /// section); export leases are refreshed afterwards.
    pub(crate) fn modify<R>(
        &self,
        part: MeshPart,
        f: impl FnOnce(&mut Mesh) -> Result<R>,
    ) -> Result<R> {
        let r = self.setup_raw(|mesh| f(select_mut(mesh, part)?));
        let s = self.refresh_exports();
        let r = r?;
        s?;
        Ok(r)
    }

    /// Run `f` on the mesh of `part` (read access; native section).
    pub(crate) fn read<R>(&self, part: MeshPart, f: impl FnOnce(&Mesh) -> Result<R>) -> Result<R> {
        let g = self.shared.read()?;
        select(&g, part).and_then(f)
    }

    /// A view of the array `key` of `part` (native section). Topology arrays
    /// are read-only views once a model uses the mesh. While the domain is
    /// held by a running call that called back into Python (a read guard
    /// is alive), the cached lease is returned.
    pub(crate) fn export<T: MeshValue>(
        &self,
        part: MeshPart,
        key: MeshKey,
    ) -> Result<ForeignView<T>> {
        let pk = (part_slot(part), key.clone());
        let r = self.shared.with_exports(|e| {
            let mut p = e.part(part)?;
            let mut slot = T::slot(&mut p, &key)?;
            self.table().view(pk.clone(), &mut slot)
        });
        let v = match r {
            Ok(v) => v,
            Err(e) => {
                if !in_callback() || !self.table().contains(&pk) {
                    return Err(e);
                }
                self.table().cached::<T>(&pk)?
            }
        };
        Ok(if key.is_topology() && self.has_model() {
            v.read_only()
        } else {
            v
        })
    }

    /// Re-lease every cached array whose lease no longer covers the live
    /// values (bookkeeping only); entries of arrays that no longer exist are
    /// dropped. Skipped while the domain is held by a running call.
    pub(crate) fn refresh_exports(&self) -> Result<()> {
        let keys = self.table().keys();
        if keys.is_empty() {
            return Ok(());
        }
        let r = self.shared.with_exports(|e| {
            let mut t = self.table();
            for pk in keys {
                let ok = match e.part(slot_part(pk.0)) {
                    Err(_) => false,
                    Ok(mut p) => {
                        let r = match &pk.1 {
                            MeshKey::Nodes | MeshKey::Data(.., DataDtype::Real) => {
                                f64::slot(&mut p, &pk.1)
                                    .and_then(|mut s| t.view(pk.clone(), &mut s).map(drop))
                            }
                            MeshKey::Data(.., DataDtype::UInt) => u32::slot(&mut p, &pk.1)
                                .and_then(|mut s| t.view(pk.clone(), &mut s).map(drop)),
                            MeshKey::Data(.., DataDtype::Bool) => bool::slot(&mut p, &pk.1)
                                .and_then(|mut s| t.view(pk.clone(), &mut s).map(drop)),
                            _ => i32::slot(&mut p, &pk.1)
                                .and_then(|mut s| t.view(pk.clone(), &mut s).map(drop)),
                        };
                        r.is_ok()
                    }
                };
                if !ok {
                    t.remove(&pk);
                }
            }
            Ok(())
        });
        match r {
            Err(_) if in_callback() => Ok(()),
            other => other,
        }
    }

    /// Before a model is built on the mesh: every node and element index in
    /// the topology arrays (which Python may have written into) is valid, so
    /// that no out-of-range index reaches native code.
    pub(crate) fn check_topology(&self) -> Result<()> {
        let g = self.shared.read()?;
        let nn = g.nb_nodes();
        let bad = |what: &str, v: Idx| {
            Error::invalid(format!(
                "invalid {what} {v} in the mesh (written from Python?): valid range [0, {nn})"
            ))
        };
        for (ty, ghost, c) in g.connectivities().as_map().iter_all() {
            for &n in c.read()?.as_slice() {
                if usize::try_from(n).map_or(true, |u| u >= nn) {
                    return Err(bad(&format!("node index of {ty}:{ghost}"), n));
                }
            }
        }
        for ng in g.groups().node_groups() {
            for &n in ng.nodes().as_slice() {
                if usize::try_from(n).map_or(true, |u| u >= nn) {
                    return Err(bad(&format!("node of group {}", ng.name()), n));
                }
            }
        }
        for eg in g.groups().element_groups() {
            for (ty, ghost, els) in eg.elements.as_map().iter_all() {
                let ne = g.nb_element(ty, ghost);
                for &e in els.read()?.as_slice() {
                    if usize::try_from(e).map_or(true, |u| u >= ne) {
                        return Err(Error::invalid(format!(
                            "invalid element {e} of {ty}:{ghost} in group {} (written from \
                             Python?): valid range [0, {ne})",
                            eg.name()
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// When a model starts using the mesh: views of topology arrays fetched
    /// before go stale (UD-532), later ones are read-only.
    pub(crate) fn detach_topology(&self) -> Result<()> {
        self.shared.with_exports(|e| e.detach_topology())
    }
}

/// A mesh of a domain (the mesh itself or its facet mesh).
#[derive(Clone, Debug)]
pub(crate) struct MeshRef {
    pub(crate) h: Arc<MeshHandle>,
    pub(crate) part: MeshPart,
}

impl MeshRef {
    pub(crate) fn read<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&Mesh) -> Result<R> + Send,
    ) -> PyResult<R> {
        let (h, part) = (Arc::clone(&self.h), self.part);
        native(py, move || h.read(part, f))
    }

    pub(crate) fn modify<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut Mesh) -> Result<R> + Send,
    ) -> PyResult<R> {
        let (h, part) = (Arc::clone(&self.h), self.part);
        native(py, move || h.modify(part, f))
    }

    pub(crate) fn view<'py, T: MeshValue>(
        &self,
        py: Python<'py>,
        key: MeshKey,
        layout: Layout,
    ) -> PyResult<Bound<'py, PyAny>> {
        let (h, part) = (Arc::clone(&self.h), self.part);
        let v = native(py, move || h.export::<T>(part, key))?;
        into_numpy(py, v, layout)
    }

    fn mesh_obj<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyMesh>> {
        PyMesh::wrap(py, self.clone())
    }
}

// --- NodeGroup -------------------------------------------------------------------

/// `NodeGroup` (PY-050): a node group of a mesh, by name.
#[pyclass(frozen, name = "NodeGroup", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyNodeGroup {
    pub(crate) mesh: MeshRef,
    pub(crate) name: String,
}

impl PyNodeGroup {
    fn nodes(&self, py: Python<'_>) -> PyResult<Vec<Idx>> {
        let name = self.name.clone();
        self.mesh
            .read(py, move |m| Ok(m.groups().node_group(&name)?.node_list()))
    }

    fn edit<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut akantu::mesh::NodeGroup) -> Result<R> + Send,
    ) -> PyResult<R> {
        let name = self.name.clone();
        self.mesh
            .modify(py, move |m| f(m.groups_mut().node_group_mut(&name)?))
    }
}

#[pymethods]
impl PyNodeGroup {
    /// `getNodes()`: `(n, 1)` int32 view of the group's node list
    /// (writable as upstream; read-only once a model uses the mesh, UD-532).
    #[pyo3(name = "getNodes")]
    fn get_nodes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.mesh
            .view::<i32>(py, MeshKey::NodeGroup(self.name.clone()), Layout::Rows)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        Ok(self.nodes(py)?.len())
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        PyList::new(py, self.nodes(py)?)?
            .into_any()
            .try_iter()
            .map(Bound::into_any)
    }

    fn __contains__(&self, py: Python<'_>, node: i64) -> PyResult<bool> {
        Ok(self.nodes(py)?.iter().any(|&n| i64::from(n) == node))
    }

    #[pyo3(name = "getName")]
    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        self.edit(py, |g| {
            g.clear();
            Ok(())
        })
    }

    fn empty(&self, py: Python<'_>) -> PyResult<bool> {
        Ok(self.nodes(py)?.is_empty())
    }

    /// `append(other)`: append the nodes of `other` (then optimize, as
    /// upstream).
    fn append(&self, py: Python<'_>, other: &Bound<'_, PyNodeGroup>) -> PyResult<()> {
        let src = other.get().nodes(py)?;
        let name = self.name.clone();
        self.edit(py, move |g| {
            let tmp = akantu::mesh::NodeGroup::with_nodes(name, src);
            g.append(&tmp);
            Ok(())
        })
    }

    /// `add(node, check_for_duplicate=True)` → the node.
    #[pyo3(signature = (node, check_for_duplicate = true))]
    fn add(&self, py: Python<'_>, node: Idx, check_for_duplicate: bool) -> PyResult<Idx> {
        self.edit(py, move |g| {
            let p = g.add(node, check_for_duplicate);
            Ok(g.nodes().as_slice()[p])
        })
    }

    /// `remove(node)`. Upstream binds `remove` to `NodeGroup::add` (UD-020,
    /// Report); the port removes the node.
    fn remove(&self, py: Python<'_>, node: Idx) -> PyResult<()> {
        self.edit(py, move |g| {
            g.remove(node);
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        format!("<NodeGroup '{}'>", self.name)
    }
}

// --- ElementGroup ----------------------------------------------------------------

/// `ElementGroup` (PY-051): an element group of a mesh, by name (a
/// `Dumpable`: its dumpers are the mesh domain's group dumpers, PY-048).
#[pyclass(extends = PyDumpable, frozen, name = "ElementGroup", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyElementGroup {
    pub(crate) mesh: MeshRef,
    pub(crate) name: String,
}

impl PyElementGroup {
    /// A new Python object of group `name` of `mesh`.
    pub(crate) fn object(py: Python<'_>, mesh: MeshRef, name: String) -> PyResult<Bound<'_, Self>> {
        Bound::new(
            py,
            PyClassInitializer::from(PyDumpable).add_subclass(Self { mesh, name }),
        )
    }

    fn target(&self) -> DumpTarget {
        DumpTarget {
            mesh: self.mesh.clone(),
            group: Some(self.name.clone()),
        }
    }

    fn edit<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut akantu::mesh::group::ElementGroupMut<'_>) -> Result<R> + Send,
    ) -> PyResult<R> {
        let name = self.name.clone();
        self.mesh.modify(py, move |m| {
            f(&mut m.groups_mut().element_group_mut(&name)?)
        })
    }
}

#[pymethods]
impl PyElementGroup {
    /// `registerDumperParaview(dumper_name, file_name, is_default=False)`
    /// (PY-048).
    #[pyo3(name = "registerDumperParaview", signature = (dumper_name, file_name, is_default = false))]
    fn register_dumper_paraview(
        &self,
        py: Python<'_>,
        dumper_name: String,
        file_name: String,
        is_default: bool,
    ) -> PyResult<()> {
        super::dumpable::register_dumper_paraview(
            py,
            &self.target(),
            dumper_name,
            file_name,
            is_default,
        )
    }

    /// `addDumpMeshToDumper(dumper_name, mesh, dimension,
    /// ghost_type=_not_ghost, element_kind=_ek_regular)`.
    #[pyo3(name = "addDumpMeshToDumper", signature = (dumper_name, mesh, dimension, ghost_type = GhostArg(Some(GhostType::NotGhost)), element_kind = KindArg(Some(akantu::core::ElementKind::Regular))))]
    fn add_dump_mesh_to_dumper(
        &self,
        py: Python<'_>,
        dumper_name: String,
        mesh: &Bound<'_, PyMesh>,
        dimension: i64,
        ghost_type: GhostArg,
        element_kind: KindArg,
    ) -> PyResult<()> {
        super::dumpable::add_dump_mesh(
            py,
            &self.target(),
            Some(dumper_name),
            mesh,
            dimension,
            ghost_type.single()?,
            element_kind
                .0
                .unwrap_or(akantu::core::ElementKind::NotDefined),
        )
    }

    /// `addDumpMesh(mesh, dimension, ghost_type=_not_ghost,
    /// element_kind=_ek_regular)`.
    #[pyo3(name = "addDumpMesh", signature = (mesh, dimension, ghost_type = GhostArg(Some(GhostType::NotGhost)), element_kind = KindArg(Some(akantu::core::ElementKind::Regular))))]
    fn add_dump_mesh(
        &self,
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        dimension: i64,
        ghost_type: GhostArg,
        element_kind: KindArg,
    ) -> PyResult<()> {
        super::dumpable::add_dump_mesh(
            py,
            &self.target(),
            None,
            mesh,
            dimension,
            ghost_type.single()?,
            element_kind
                .0
                .unwrap_or(akantu::core::ElementKind::NotDefined),
        )
    }

    /// `addDumpField(field_id)` (upstream base `Dumpable`: not
    /// implemented).
    #[pyo3(name = "addDumpField", signature = (field_id))]
    fn add_dump_field_dumpable(&self, py: Python<'_>, field_id: String) -> PyResult<()> {
        let _ = field_id;
        super::dumpable::add_dump_field(py)
    }

    /// `addDumpFieldToDumper(dumper_name, field_id)` (upstream base
    /// `Dumpable`: not implemented).
    #[pyo3(name = "addDumpFieldToDumper", signature = (dumper_name, field_id))]
    fn add_dump_field_to_dumper_dumpable(
        &self,
        py: Python<'_>,
        dumper_name: String,
        field_id: String,
    ) -> PyResult<()> {
        let _ = (dumper_name, field_id);
        super::dumpable::add_dump_field(py)
    }

    /// `addDumpFieldExternal(field_id, field)` (`field`: a NumPy array of
    /// nodal values, read at dump time).
    #[pyo3(name = "addDumpFieldExternal", signature = (field_id, field))]
    fn add_dump_field_external(
        &self,
        py: Python<'_>,
        field_id: String,
        field: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let t = self.target();
        super::dumpable::add_dump_field_external(py, &t, &t.mesh.h.externals, None, field_id, field)
    }

    /// `addDumpFieldExternalToDumper(dumper_name, field_id, field)`
    /// (upstream: a `dumpers::Field`, PY-049).
    #[pyo3(name = "addDumpFieldExternalToDumper", signature = (dumper_name, field_id, field))]
    fn add_dump_field_external_to_dumper(
        &self,
        py: Python<'_>,
        dumper_name: String,
        field_id: String,
        field: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let t = self.target();
        super::dumpable::add_dump_field_external(
            py,
            &t,
            &t.mesh.h.externals,
            Some(dumper_name),
            field_id,
            field,
        )
    }

    /// `dump()`, `dump(step)`, `dump(time, step)`, `dump(dumper_name)`,
    /// `dump(dumper_name, step)`, `dump(dumper_name, time, step)`.
    #[pyo3(signature = (*args, **kwargs))]
    fn dump(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let t = self.target();
        super::dumpable::dump(py, &t, &t.mesh.h.externals, args, kwargs)
    }

    /// `getNodeGroup()`: the node group this element group refers to.
    #[pyo3(name = "getNodeGroup")]
    fn get_node_group(&self, py: Python<'_>) -> PyResult<PyNodeGroup> {
        let name = self.name.clone();
        let ng = self.mesh.read(py, move |m| {
            Ok(m.groups()
                .element_group(&name)?
                .node_group
                .name()
                .to_owned())
        })?;
        Ok(PyNodeGroup {
            mesh: self.mesh.clone(),
            name: ng,
        })
    }

    #[pyo3(name = "getName")]
    fn get_name(&self) -> String {
        self.name.clone()
    }

    /// `getElements()`: `ElementTypeMapArrayInt` of the group's elements
    /// (read-only views).
    #[pyo3(name = "getElements")]
    fn get_elements(&self) -> EtmaInt {
        EtmaInt::from_source(EtmaSource::GroupElements(
            self.mesh.clone(),
            self.name.clone(),
        ))
    }

    /// Number of elements (both ghost types).
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let name = self.name.clone();
        self.mesh.read(py, move |m| {
            Ok(m.groups()
                .element_group(&name)?
                .group()
                .elements
                .total_len())
        })
    }

    fn clear(&self, py: Python<'_>) -> PyResult<()> {
        self.edit(py, |g| {
            g.clear();
            Ok(())
        })
    }

    fn empty(&self, py: Python<'_>) -> PyResult<bool> {
        let name = self.name.clone();
        self.mesh
            .read(py, move |m| Ok(m.groups().element_group(&name)?.is_empty()))
    }

    /// `append(other)`: append the elements and nodes of `other`.
    fn append(&self, py: Python<'_>, other: &Bound<'_, PyElementGroup>) -> PyResult<()> {
        let (name, other) = (self.name.clone(), other.get().name.clone());
        self.mesh.modify(py, move |m| {
            m.groups_mut().append_element_group(&name, &other)
        })
    }

    fn optimize(&self, py: Python<'_>) -> PyResult<()> {
        self.edit(py, |g| {
            g.optimize();
            Ok(())
        })
    }

    /// `add(element, add_nodes=False, check_for_duplicate=True)`.
    #[pyo3(signature = (element, add_nodes = false, check_for_duplicate = true))]
    fn add(
        &self,
        py: Python<'_>,
        element: &Bound<'_, PyElement>,
        add_nodes: bool,
        check_for_duplicate: bool,
    ) -> PyResult<()> {
        let el = element.get().element;
        let name = self.name.clone();
        self.mesh.modify(py, move |m| {
            m.element_group_add(&name, el, add_nodes, check_for_duplicate)
        })
    }

    /// `fillFromNodeGroup()`.
    #[pyo3(name = "fillFromNodeGroup")]
    fn fill_from_node_group(&self, py: Python<'_>) -> PyResult<()> {
        let name = self.name.clone();
        self.mesh
            .modify(py, move |m| m.fill_element_group_from_node_group(&name))
    }

    /// `addDimension(dimension)`.
    #[pyo3(name = "addDimension")]
    fn add_dimension(&self, py: Python<'_>, dimension: i64) -> PyResult<()> {
        let d = group_dim(dimension)?;
        self.edit(py, move |g| {
            g.add_dimension(d);
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        format!("<ElementGroup '{}'>", self.name)
    }
}

// --- GroupManager / Mesh ------------------------------------------------------------

/// `GroupManager` (PY-052), base class of `Mesh`.
#[pyclass(subclass, frozen, name = "GroupManager", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyGroupManager {
    pub(crate) mesh: MeshRef,
}

fn element_groups<'py>(
    mesh: &MeshRef,
    py: Python<'py>,
) -> PyResult<Vec<Bound<'py, PyElementGroup>>> {
    let names = mesh.read(py, |m| {
        Ok(m.groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect::<Vec<_>>())
    })?;
    names
        .into_iter()
        .map(|name| PyElementGroup::object(py, mesh.clone(), name))
        .collect()
}

fn node_groups(mesh: &MeshRef, py: Python<'_>) -> PyResult<Vec<PyNodeGroup>> {
    let names = mesh.read(py, |m| {
        Ok(m.groups()
            .node_groups()
            .map(|g| g.name().to_owned())
            .collect::<Vec<_>>())
    })?;
    Ok(names
        .into_iter()
        .map(|name| PyNodeGroup {
            mesh: mesh.clone(),
            name,
        })
        .collect())
}

#[pymethods]
impl PyGroupManager {
    #[pyo3(name = "getElementGroup")]
    fn get_element_group<'py>(
        &self,
        py: Python<'py>,
        name: String,
    ) -> PyResult<Bound<'py, PyElementGroup>> {
        let n = name.clone();
        self.mesh
            .read(py, move |m| m.groups().element_group(&n).map(|_| ()))?;
        PyElementGroup::object(py, self.mesh.clone(), name)
    }

    #[pyo3(name = "getNodeGroup")]
    fn get_node_group(&self, py: Python<'_>, name: String) -> PyResult<PyNodeGroup> {
        let n = name.clone();
        self.mesh
            .read(py, move |m| m.groups().node_group(&n).map(|_| ()))?;
        Ok(PyNodeGroup {
            mesh: self.mesh.clone(),
            name,
        })
    }

    #[pyo3(name = "iterateElementGroups")]
    fn iterate_element_groups<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Vec<Bound<'py, PyElementGroup>>> {
        element_groups(&self.mesh, py)
    }

    #[pyo3(name = "iterateNodeGroups")]
    fn iterate_node_groups(&self, py: Python<'_>) -> PyResult<Vec<PyNodeGroup>> {
        node_groups(&self.mesh, py)
    }

    #[pyo3(name = "elementGroups")]
    fn element_groups_<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyElementGroup>>> {
        element_groups(&self.mesh, py)
    }

    #[pyo3(name = "nodeGroups")]
    fn node_groups_(&self, py: Python<'_>) -> PyResult<Vec<PyNodeGroup>> {
        node_groups(&self.mesh, py)
    }

    /// `createNodeGroup(name, replace_group)` (upstream exposes both
    /// arguments positionally; `replace_group` defaults to `False` here).
    #[pyo3(name = "createNodeGroup", signature = (name, replace_group = false))]
    fn create_node_group(
        &self,
        py: Python<'_>,
        name: String,
        replace_group: bool,
    ) -> PyResult<PyNodeGroup> {
        let n = name.clone();
        self.mesh.modify(py, move |m| {
            m.groups_mut()
                .create_node_group_replace(&n, replace_group)
                .map(|_| ())
        })?;
        Ok(PyNodeGroup {
            mesh: self.mesh.clone(),
            name,
        })
    }

    /// `createElementGroup(id, spatial_dimension, replace_group=False)`.
    #[pyo3(name = "createElementGroup", signature = (id, spatial_dimension, replace_group = false))]
    fn create_element_group<'py>(
        &self,
        py: Python<'py>,
        id: String,
        spatial_dimension: i64,
        replace_group: bool,
    ) -> PyResult<Bound<'py, PyElementGroup>> {
        let d = group_dim(spatial_dimension)?;
        let n = id.clone();
        self.mesh.modify(py, move |m| {
            m.groups_mut()
                .create_element_group_replace(&n, d, replace_group)
                .map(|_| ())
        })?;
        PyElementGroup::object(py, self.mesh.clone(), id)
    }

    /// `createGroupsFromMeshDataUInt(dataset)`.
    #[pyo3(name = "createGroupsFromMeshDataUInt")]
    fn create_groups_from_mesh_data_uint(&self, py: Python<'_>, dataset: String) -> PyResult<()> {
        self.mesh
            .modify(py, move |m| m.create_groups_from_mesh_data(&dataset))
    }

    /// `createElementGroupFromNodeGroup(name, node_group,
    /// dimension=_all_dimensions)`.
    #[pyo3(name = "createElementGroupFromNodeGroup", signature = (name, node_group, dimension = PY_ALL_DIMENSIONS))]
    fn create_element_group_from_node_group(
        &self,
        py: Python<'_>,
        name: String,
        node_group: String,
        dimension: i64,
    ) -> PyResult<()> {
        let d = group_dim(dimension)?;
        self.mesh.modify(py, move |m| {
            m.create_element_group_from_node_group(&name, &node_group, d)
        })
    }

    /// `createBoundaryGroupFromGeometry()`.
    #[pyo3(name = "createBoundaryGroupFromGeometry")]
    fn create_boundary_group_from_geometry(&self, py: Python<'_>) -> PyResult<()> {
        self.mesh.modify(py, |m| {
            akantu::mesh::clusters::create_boundary_group_from_geometry(m).map(|_| ())
        })
    }

    /// `createClusters(element_dimension, cluster_name_prefix, filter)` →
    /// number of clusters. Only the default `ClusteringFilter` (accept
    /// every element) is supported (Python filters: later WP).
    #[pyo3(name = "createClusters", signature = (element_dimension, cluster_name_prefix = "cluster".to_owned(), filter = None))]
    fn create_clusters(
        &self,
        py: Python<'_>,
        element_dimension: i64,
        cluster_name_prefix: String,
        filter: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<usize> {
        if let Some(f) = filter
            && !f.is_instance_of::<PyClusteringFilter>()
        {
            return Err(PyTypeError::new_err("filter must be a ClusteringFilter"));
        }
        let d = usize::try_from(element_dimension)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid element dimension"))?;
        self.mesh.modify(py, move |m| {
            akantu::mesh::clusters::create_clusters(
                m,
                d,
                &cluster_name_prefix,
                &|_| true,
                akantu::mesh::clusters::ClusterFacets::Build,
            )
        })
    }
}

/// `GroupManager.ClusteringFilter` (default filter: every element).
#[pyclass(
    subclass,
    frozen,
    name = "ClusteringFilter",
    module = "akantu.py11_akantu"
)]
#[derive(Debug)]
pub(crate) struct PyClusteringFilter;

#[pymethods]
impl PyClusteringFilter {
    #[new]
    fn new() -> Self {
        Self
    }
}

/// `MeshData` (flattened into `Mesh`, see the module docs).
#[pyclass(subclass, frozen, name = "MeshData", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMeshData;

/// `PeriodicSlaves`: iterable of the slaves of a periodic master.
#[pyclass(frozen, name = "PeriodicSlaves", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyPeriodicSlaves {
    slaves: Vec<Idx>,
}

#[pymethods]
impl PyPeriodicSlaves {
    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        PyList::new(py, &self.slaves)?
            .into_any()
            .try_iter()
            .map(Bound::into_any)
    }
}

/// `Mesh(spatial_dimension, id="mesh")` (PY-053).
#[pyclass(extends = PyGroupManager, frozen, name = "Mesh", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMesh;

impl PyMesh {
    pub(crate) fn wrap(py: Python<'_>, mesh: MeshRef) -> PyResult<Bound<'_, Self>> {
        Bound::new(
            py,
            PyClassInitializer::from(PyGroupManager { mesh }).add_subclass(PyMesh),
        )
    }

    pub(crate) fn mesh_ref(slf: &Bound<'_, Self>) -> MeshRef {
        slf.as_super().get().mesh.clone()
    }
}

fn mref(slf: &Bound<'_, PyMesh>) -> MeshRef {
    PyMesh::mesh_ref(slf)
}

fn mesh_target(slf: &Bound<'_, PyMesh>) -> DumpTarget {
    DumpTarget {
        mesh: mref(slf),
        group: None,
    }
}

#[pymethods]
impl PyMesh {
    /// `registerDumperParaview(dumper_name, file_name, is_default=False)`
    /// (PY-048).
    #[pyo3(name = "registerDumperParaview", signature = (dumper_name, file_name, is_default = false))]
    fn register_dumper_paraview(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        dumper_name: String,
        file_name: String,
        is_default: bool,
    ) -> PyResult<()> {
        super::dumpable::register_dumper_paraview(
            py,
            &mesh_target(slf),
            dumper_name,
            file_name,
            is_default,
        )
    }

    /// `addDumpMeshToDumper(dumper_name, mesh, dimension,
    /// ghost_type=_not_ghost, element_kind=_ek_regular)`.
    #[pyo3(name = "addDumpMeshToDumper", signature = (dumper_name, mesh, dimension, ghost_type = GhostArg(Some(GhostType::NotGhost)), element_kind = KindArg(Some(akantu::core::ElementKind::Regular))))]
    fn add_dump_mesh_to_dumper(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        dumper_name: String,
        mesh: &Bound<'_, PyMesh>,
        dimension: i64,
        ghost_type: GhostArg,
        element_kind: KindArg,
    ) -> PyResult<()> {
        super::dumpable::add_dump_mesh(
            py,
            &mesh_target(slf),
            Some(dumper_name),
            mesh,
            dimension,
            ghost_type.single()?,
            element_kind
                .0
                .unwrap_or(akantu::core::ElementKind::NotDefined),
        )
    }

    /// `addDumpMesh(mesh, dimension, ghost_type=_not_ghost,
    /// element_kind=_ek_regular)`.
    #[pyo3(name = "addDumpMesh", signature = (mesh, dimension, ghost_type = GhostArg(Some(GhostType::NotGhost)), element_kind = KindArg(Some(akantu::core::ElementKind::Regular))))]
    fn add_dump_mesh(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        mesh: &Bound<'_, PyMesh>,
        dimension: i64,
        ghost_type: GhostArg,
        element_kind: KindArg,
    ) -> PyResult<()> {
        super::dumpable::add_dump_mesh(
            py,
            &mesh_target(slf),
            None,
            mesh,
            dimension,
            ghost_type.single()?,
            element_kind
                .0
                .unwrap_or(akantu::core::ElementKind::NotDefined),
        )
    }

    /// `addDumpField(field_id)` (upstream base `Dumpable`: not
    /// implemented).
    #[pyo3(name = "addDumpField", signature = (field_id))]
    fn add_dump_field_dumpable(
        _slf: &Bound<'_, Self>,
        py: Python<'_>,
        field_id: String,
    ) -> PyResult<()> {
        let _ = field_id;
        super::dumpable::add_dump_field(py)
    }

    /// `addDumpFieldToDumper(dumper_name, field_id)` (upstream base
    /// `Dumpable`: not implemented).
    #[pyo3(name = "addDumpFieldToDumper", signature = (dumper_name, field_id))]
    fn add_dump_field_to_dumper_dumpable(
        _slf: &Bound<'_, Self>,
        py: Python<'_>,
        dumper_name: String,
        field_id: String,
    ) -> PyResult<()> {
        let _ = (dumper_name, field_id);
        super::dumpable::add_dump_field(py)
    }

    /// `addDumpFieldExternal(field_id, field)` (`field`: a NumPy array of
    /// nodal values, read at dump time).
    #[pyo3(name = "addDumpFieldExternal", signature = (field_id, field))]
    fn add_dump_field_external(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        field_id: String,
        field: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let t = mesh_target(slf);
        super::dumpable::add_dump_field_external(py, &t, &t.mesh.h.externals, None, field_id, field)
    }

    /// `addDumpFieldExternalToDumper(dumper_name, field_id, field)`
    /// (upstream: a `dumpers::Field`, PY-049).
    #[pyo3(name = "addDumpFieldExternalToDumper", signature = (dumper_name, field_id, field))]
    fn add_dump_field_external_to_dumper(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        dumper_name: String,
        field_id: String,
        field: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let t = mesh_target(slf);
        super::dumpable::add_dump_field_external(
            py,
            &t,
            &t.mesh.h.externals,
            Some(dumper_name),
            field_id,
            field,
        )
    }

    /// `dump()`, `dump(step)`, `dump(time, step)`, `dump(dumper_name)`,
    /// `dump(dumper_name, step)`, `dump(dumper_name, time, step)`.
    #[pyo3(signature = (*args, **kwargs))]
    fn dump(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, pyo3::types::PyDict>>,
    ) -> PyResult<()> {
        let t = mesh_target(slf);
        super::dumpable::dump(py, &t, &t.mesh.h.externals, args, kwargs)
    }

    #[new]
    #[pyo3(signature = (spatial_dimension, id = "mesh".to_owned()))]
    fn new(
        py: Python<'_>,
        spatial_dimension: i64,
        id: String,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dim = usize::try_from(spatial_dimension)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid spatial dimension"))?;
        // upstream `Mesh(spatial_dimension, id)`: on the static communicator
        // (the MPI world in the MPI build; serial otherwise)
        let mesh = native(py, move || {
            let comm = akantu::runtime::global()
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .communicator
                .clone();
            Mesh::with_communicator(dim, id, comm)
        })?;
        Ok(PyClassInitializer::from(PyGroupManager {
            mesh: MeshRef {
                h: MeshHandle::new(mesh),
                part: MeshPart::Main,
            },
        })
        .add_subclass(PyMesh))
    }

    /// `read(filename, mesh_io_type=_miot_auto)`.
    #[pyo3(signature = (filename, mesh_io_type = None))]
    fn read(
        slf: &Bound<'_, Self>,
        filename: std::path::PathBuf,
        mesh_io_type: Option<MeshIoArg>,
    ) -> PyResult<()> {
        let fmt = mesh_io_type.map_or(akantu::core::enums::MeshIoType::Auto, |a| a.0);
        mref(slf).modify(slf.py(), move |m| {
            akantu::io::mesh_io::read_mesh(m, &filename, fmt)
        })
    }

    /// `getNodes()`: writable `(nb_nodes, dim)` float64 view.
    #[pyo3(name = "getNodes")]
    fn get_nodes<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        mref(slf).view::<f64>(slf.py(), MeshKey::Nodes, Layout::Rows)
    }

    #[pyo3(name = "getNbNodes")]
    fn get_nb_nodes(slf: &Bound<'_, Self>) -> PyResult<usize> {
        mref(slf).read(slf.py(), |m| Ok(m.nb_nodes()))
    }

    /// `getConnectivity(type)`: writable `(nb_element, nb_nodes_per_element)`
    /// int32 view of the not-ghost block (`ghost_type` is an akantu-rs
    /// extension).
    #[pyo3(name = "getConnectivity", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_connectivity<'py>(
        slf: &Bound<'py, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let g = ghost_type.single()?;
        mref(slf).view::<i32>(slf.py(), MeshKey::Conn(r#type.0, g), Layout::Rows)
    }

    /// `getConnectivities()`: `ElementTypeMapArrayInt` of the
    /// connectivities.
    #[pyo3(name = "getConnectivities")]
    fn get_connectivities(slf: &Bound<'_, Self>) -> EtmaInt {
        EtmaInt::from_source(EtmaSource::Connectivities(mref(slf)))
    }

    /// `addConnectivityType(type, ghost_type=_not_ghost)`.
    #[pyo3(name = "addConnectivityType", signature = (r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn add_connectivity_type(
        slf: &Bound<'_, Self>,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<()> {
        let g = ghost_type.single()?;
        let t = r#type.0;
        mref(slf).modify(slf.py(), move |m| {
            MeshAccessor::new(m).connectivity_mut(t, g);
            Ok(())
        })
    }

    /// `distribute()` (upstream `Mesh::distribute()` →
    /// `distributeImpl(getCommunicator())`). Sequential wheel: marks the
    /// mesh distributed over its single-rank communicator (upstream with one
    /// process). MPI build (D-101 (1)): the centralised
    /// distribution over the mesh's communicator, the world
    /// (`akantu::mesh::distribution::distribute_with`, collective), rank 0
    /// partitioning with Scotch seeded with the global seed as upstream's
    /// `MeshPartitionScotch` (`RandomGenerator::seed()`); rank 0 holds the
    /// read mesh, the other ranks an empty one (upstream's `plate.py`
    /// reads on rank 0 only; a non-empty mesh elsewhere is refused). Views
    /// of the mesh arrays taken before go stale (they keep the old values,
    /// D-047); fetch them again.
    fn distribute(slf: &Bound<'_, Self>) -> PyResult<()> {
        mref(slf).modify(slf.py(), |m| {
            if m.is_distributed() {
                return Err(Error::invalid("This mesh is already distribute"));
            }
            #[cfg(feature = "mpi")]
            if m.communicator().is_mpi() {
                return distribute_over_mpi(m);
            }
            if m.communicator().size() > 1 {
                return Err(Error::Unsupported(
                    "Mesh.distribute over several ranks needs the MPI build of the module".into(),
                ));
            }
            MeshAccessor::new(m).set_distributed();
            m.compute_bounding_box()
        })
    }

    /// `getGlobalNodesIds()` (akantu-rs extension; upstream C++
    /// `Mesh::getGlobalNodesIds()`, not bound in Python): the global id of
    /// every local node, as a list (the local index without global ids).
    #[pyo3(name = "getGlobalNodesIds")]
    fn get_global_nodes_ids(slf: &Bound<'_, Self>) -> PyResult<Vec<i64>> {
        mref(slf).read(slf.py(), |m| {
            let n = Idx::try_from(m.nb_nodes())
                .map_err(|_| Error::invalid("too many nodes for an index"))?;
            Ok((0..n).map(|i| m.node_global_id(i)).collect())
        })
    }

    /// `getNbGlobalNodes()` (akantu-rs extension; upstream C++
    /// `Mesh::getNbGlobalNodes()`).
    #[pyo3(name = "getNbGlobalNodes")]
    fn get_nb_global_nodes(slf: &Bound<'_, Self>) -> PyResult<usize> {
        mref(slf).read(slf.py(), |m| Ok(m.nb_global_nodes()))
    }

    #[pyo3(name = "isDistributed")]
    fn is_distributed(slf: &Bound<'_, Self>) -> PyResult<bool> {
        mref(slf).read(slf.py(), |m| Ok(m.is_distributed()))
    }

    /// `fillNodesToElements(dimension=_all_dimensions)`.
    #[pyo3(name = "fillNodesToElements", signature = (dimension = PY_ALL_DIMENSIONS))]
    fn fill_nodes_to_elements(slf: &Bound<'_, Self>, dimension: i64) -> PyResult<()> {
        let d = dim_filter(dimension)?;
        let r = mref(slf);
        let h = Arc::clone(&r.h);
        let part = r.part;
        r.read(slf.py(), move |m| {
            let map = akantu::mesh::utils::fill_nodes_to_elements(m, d)?;
            let rev = m.revision();
            h.n2e.lock().unwrap_or_else(PoisonError::into_inner)[part_slot(part)] =
                Some((rev, map));
            Ok(())
        })
    }

    /// `getAssociatedElements(node, list)`: append the elements of `node`
    /// (after `fillNodesToElements`) to `list`.
    #[pyo3(name = "getAssociatedElements")]
    fn get_associated_elements(
        slf: &Bound<'_, Self>,
        node: Idx,
        list: &Bound<'_, PyList>,
    ) -> PyResult<()> {
        let r = mref(slf);
        let h = Arc::clone(&r.h);
        let part = r.part;
        let elements = r.read(slf.py(), move |m| {
            let cache = h.n2e.lock().unwrap_or_else(PoisonError::into_inner);
            let Some((rev, map)) = cache[part_slot(part)].as_ref() else {
                return Err(Error::invalid(
                    "getAssociatedElements: call fillNodesToElements first",
                ));
            };
            if *rev != m.revision() {
                return Err(Error::invalid(
                    "getAssociatedElements: the mesh changed since fillNodesToElements",
                ));
            }
            let n = usize::try_from(node).map_err(|_| Error::OutOfRange {
                what: "node",
                index: 0,
                len: map.nb_rows(),
            })?;
            map.get_row(n)
                .map(<[Element]>::to_vec)
                .ok_or(Error::OutOfRange {
                    what: "node",
                    index: n,
                    len: map.nb_rows(),
                })
        })?;
        for e in elements {
            list.append(PyElement { element: e })?;
        }
        Ok(())
    }

    /// `makePeriodic(direction)`.
    #[pyo3(name = "makePeriodic")]
    fn make_periodic(slf: &Bound<'_, Self>, direction: DirectionArg) -> PyResult<()> {
        let dir = match direction.0 {
            0 => SpatialDirection::X,
            1 => SpatialDirection::Y,
            _ => SpatialDirection::Z,
        };
        mref(slf).modify(slf.py(), move |m| {
            akantu::mesh::periodic::make_periodic(m, dir, None).map(|_| ())
        })
    }

    /// `getNbElement(*, spatial_dimension=_all_dimensions,
    /// ghost_type=_not_ghost, kind=_ek_not_defined)` or
    /// `getNbElement(*, element_type, ghost_type=_not_ghost)`.
    #[pyo3(name = "getNbElement", signature = (*, element_type = None, spatial_dimension = PY_ALL_DIMENSIONS, ghost_type = GhostArg(Some(GhostType::NotGhost)), kind = KindArg(None)))]
    fn get_nb_element(
        slf: &Bound<'_, Self>,
        element_type: Option<EtArg>,
        spatial_dimension: i64,
        ghost_type: GhostArg,
        kind: KindArg,
    ) -> PyResult<usize> {
        let g = ghost_type.single()?;
        let d = dim_filter(spatial_dimension)?;
        mref(slf).read(slf.py(), move |m| {
            Ok(match element_type {
                Some(t) => m.nb_element(t.0, g),
                None => m.nb_element_filtered(TypeFilter {
                    spatial_dimension: d,
                    ghost_type: g,
                    kind: kind.0,
                }),
            })
        })
    }

    /// `Mesh.getSpatialDimension(type)` (static).
    #[staticmethod]
    #[pyo3(name = "getSpatialDimension")]
    fn get_spatial_dimension(r#type: EtArg) -> usize {
        r#type.0.spatial_dimension()
    }

    /// `getDataReal(name, type, ghost_type=_not_ghost)`: writable view.
    #[pyo3(name = "getDataReal", signature = (name, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn get_data_real<'py>(
        slf: &Bound<'py, Self>,
        name: String,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<Bound<'py, PyAny>> {
        let g = ghost_type.single()?;
        mref(slf).view::<f64>(
            slf.py(),
            MeshKey::Data(name, r#type.0, g, DataDtype::Real),
            Layout::Rows,
        )
    }

    /// `hasDataReal(name, type, ghost_type=_not_ghost)`.
    #[pyo3(name = "hasDataReal", signature = (name, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn has_data_real(
        slf: &Bound<'_, Self>,
        name: String,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<bool> {
        let g = ghost_type.single()?;
        let t = r#type.0;
        mref(slf).read(slf.py(), move |m| {
            Ok(m.data().reals(&name).is_ok_and(|d| d.exists(t, g)))
        })
    }

    /// `getElementalDataUInt(name)` (upstream `MeshData`).
    #[pyo3(name = "getElementalDataUInt")]
    fn get_elemental_data_uint(slf: &Bound<'_, Self>, name: String) -> PyResult<EtmaUInt> {
        let n = name.clone();
        mref(slf).read(slf.py(), move |m| m.data().uints(&n).map(|_| ()))?;
        Ok(EtmaUInt::from_source(EtmaSource::Data(mref(slf), name)))
    }

    /// `getElementalDataReal(name)` (upstream `MeshData`).
    #[pyo3(name = "getElementalDataReal")]
    fn get_elemental_data_real(slf: &Bound<'_, Self>, name: String) -> PyResult<EtmaReal> {
        let n = name.clone();
        mref(slf).read(slf.py(), move |m| m.data().reals(&n).map(|_| ()))?;
        Ok(EtmaReal::from_source(EtmaSource::Data(mref(slf), name)))
    }

    #[pyo3(name = "isPeriodic")]
    fn is_periodic(slf: &Bound<'_, Self>) -> PyResult<bool> {
        mref(slf).read(slf.py(), |m| Ok(m.is_periodic()))
    }

    /// `getPeriodicMaster(slave)` (upstream returns the node itself when it
    /// is not a slave).
    #[pyo3(name = "getPeriodicMaster")]
    fn get_periodic_master(slf: &Bound<'_, Self>, slave: Idx) -> PyResult<Idx> {
        mref(slf).read(slf.py(), move |m| {
            Ok(m.periodic_master(slave).unwrap_or(slave))
        })
    }

    #[pyo3(name = "getPeriodicSlaves")]
    fn get_periodic_slaves(slf: &Bound<'_, Self>, master: Idx) -> PyResult<PyPeriodicSlaves> {
        let slaves = mref(slf).read(slf.py(), move |m| Ok(m.periodic_slaves(master).to_vec()))?;
        Ok(PyPeriodicSlaves { slaves })
    }

    #[pyo3(name = "isPeriodicSlave")]
    fn is_periodic_slave(slf: &Bound<'_, Self>, node: Idx) -> PyResult<bool> {
        mref(slf).read(slf.py(), move |m| m.is_periodic_slave(node))
    }

    #[pyo3(name = "isPeriodicMaster")]
    fn is_periodic_master(slf: &Bound<'_, Self>, node: Idx) -> PyResult<bool> {
        mref(slf).read(slf.py(), move |m| m.is_periodic_master(node))
    }

    /// `getMeshFacets()`: the facet mesh (raises if none was built).
    #[pyo3(name = "getMeshFacets")]
    fn get_mesh_facets<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyMesh>> {
        let r = mref(slf);
        if r.part == MeshPart::Facets {
            return Err(super::errors::exception(
                slf.py(),
                "a facet mesh has no facet mesh".into(),
            ));
        }
        r.read(slf.py(), |m| select(m, MeshPart::Facets).map(|_| ()))?;
        MeshRef {
            h: r.h,
            part: MeshPart::Facets,
        }
        .mesh_obj(slf.py())
    }

    /// `initMeshFacets(id="mesh_facets")`: build the facet mesh once and
    /// return it.
    #[pyo3(name = "initMeshFacets", signature = (id = "mesh_facets".to_owned()))]
    fn init_mesh_facets<'py>(slf: &Bound<'py, Self>, id: String) -> PyResult<Bound<'py, PyMesh>> {
        let _ = id; // the facet mesh id is fixed (`mesh_facets`) by the port
        let r = mref(slf);
        if r.part == MeshPart::Facets {
            return Err(super::errors::exception(
                slf.py(),
                "initMeshFacets on a facet mesh".into(),
            ));
        }
        r.modify(slf.py(), |m| {
            akantu::mesh::utils::init_mesh_facets(m).map(|_| ())
        })?;
        MeshRef {
            h: r.h,
            part: MeshPart::Facets,
        }
        .mesh_obj(slf.py())
    }

    /// `getUpperBounds()` (copy, 1-D).
    #[pyo3(name = "getUpperBounds")]
    fn get_upper_bounds<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let v = mref(slf).read(slf.py(), |m| Ok(m.upper_bounds().to_vec()))?;
        super::arrays::vector_to_numpy(slf.py(), v)
    }

    /// `getLowerBounds()` (copy, 1-D).
    #[pyo3(name = "getLowerBounds")]
    fn get_lower_bounds<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyAny>> {
        let v = mref(slf).read(slf.py(), |m| Ok(m.lower_bounds().to_vec()))?;
        super::arrays::vector_to_numpy(slf.py(), v)
    }

    /// Spatial dimension of this mesh (akantu-rs extension; upstream only
    /// has the static per-type variant in Python).
    #[pyo3(name = "getDimension")]
    fn get_dimension(slf: &Bound<'_, Self>) -> PyResult<usize> {
        mref(slf).read(slf.py(), |m| Ok(m.spatial_dimension()))
    }

    #[pyo3(name = "getID")]
    fn get_id(slf: &Bound<'_, Self>) -> PyResult<String> {
        mref(slf).read(slf.py(), |m| Ok(m.id().to_owned()))
    }

    /// Element types present in the mesh (akantu-rs helper:
    /// `elementTypes(dim=_all_dimensions, ghost_type=_not_ghost,
    /// kind=_ek_not_defined)`).
    #[pyo3(name = "elementTypes", signature = (dim = PY_ALL_DIMENSIONS, ghost_type = GhostArg(Some(GhostType::NotGhost)), kind = KindArg(None)))]
    fn element_types<'py>(
        slf: &Bound<'py, Self>,
        dim: i64,
        ghost_type: GhostArg,
        kind: KindArg,
    ) -> PyResult<Vec<Bound<'py, PyAny>>> {
        let d = dim_filter(dim)?;
        let g = ghost_type.single()?;
        let types = mref(slf).read(slf.py(), move |m| {
            Ok(m.element_types(TypeFilter {
                spatial_dimension: d,
                ghost_type: g,
                kind: kind.0,
            })
            .collect::<Vec<_>>())
        })?;
        types
            .into_iter()
            .map(|t| Ok(element_type_obj(slf.py(), t)?.into_any()))
            .collect()
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let r = mref(slf);
        let (id, dim, n) = r.read(slf.py(), |m| {
            Ok((m.id().to_owned(), m.spatial_dimension(), m.nb_nodes()))
        })?;
        Ok(format!(
            "<Mesh '{id}' ({dim}D, {n} nodes{})>",
            if r.part == MeshPart::Facets {
                ", facets"
            } else {
                ""
            }
        ))
    }
}

// --- MeshAccessor / MeshUtils ---------------------------------------------------------

/// `MeshAccessor(mesh)` (PY-054).
#[pyclass(frozen, name = "MeshAccessor", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMeshAccessor {
    mesh: MeshRef,
}

#[pymethods]
impl PyMeshAccessor {
    #[new]
    fn new(mesh: &Bound<'_, PyMesh>) -> Self {
        Self { mesh: mref(mesh) }
    }

    /// `resizeConnectivity(new_size, type, ghost_type=_not_ghost)`.
    #[pyo3(name = "resizeConnectivity", signature = (new_size, r#type, ghost_type = GhostArg(Some(GhostType::NotGhost))))]
    fn resize_connectivity(
        &self,
        py: Python<'_>,
        new_size: usize,
        r#type: EtArg,
        ghost_type: GhostArg,
    ) -> PyResult<()> {
        let g = ghost_type.single()?;
        let t = r#type.0;
        self.mesh.modify(py, move |m| {
            // upstream `resizeConnectivity`: new entries are `-1`
            MeshAccessor::new(m)
                .connectivity_mut(t, g)
                .try_resize(new_size, -1)
        })
    }

    /// `resizeNodes(new_size)` (new nodes at the origin).
    #[pyo3(name = "resizeNodes")]
    fn resize_nodes(&self, py: Python<'_>, new_size: usize) -> PyResult<()> {
        self.mesh
            .modify(py, move |m| MeshAccessor::new(m).resize_nodes(new_size))
    }

    /// `makeReady()`: bounding box and consistency (upstream: sends the
    /// `NewNodes`/`NewElements` events and recomputes the bounding box).
    #[pyo3(name = "makeReady")]
    fn make_ready(&self, py: Python<'_>) -> PyResult<()> {
        self.mesh.modify(py, |m| {
            m.compute_bounding_box()?;
            m.check_consistency()
        })
    }
}

/// `MeshUtils` (PY-054).
#[pyclass(frozen, name = "MeshUtils", module = "akantu.py11_akantu")]
#[derive(Debug)]
pub(crate) struct PyMeshUtils;

#[pymethods]
impl PyMeshUtils {
    /// `MeshUtils.buildFacets(mesh)`.
    #[staticmethod]
    #[pyo3(name = "buildFacets")]
    fn build_facets(mesh: &Bound<'_, PyMesh>) -> PyResult<()> {
        mref(mesh).modify(mesh.py(), |m| {
            akantu::mesh::utils::build_facets(m).map(|_| ())
        })
    }
}

/// Register the mesh classes (`register_group_manager`, `register_mesh`).
pub(crate) fn register_group_manager(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyNodeGroup>()?;
    m.add_class::<PyElementGroup>()?;
    m.add_class::<PyGroupManager>()?;
    m.add_class::<PyClusteringFilter>()?;
    Ok(())
}

/// `register_mesh`.
pub(crate) fn register_mesh(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPeriodicSlaves>()?;
    m.add_class::<PyMeshData>()?;
    m.add_class::<PyMesh>()?;
    m.add_class::<PyMeshUtils>()?;
    m.add_class::<PyMeshAccessor>()?;
    super::etma::register(m)?;
    Ok(())
}
