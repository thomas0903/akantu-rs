// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/element_group.cc (constructor:
// default ParaView dumper), src/mesh/node_group.cc (constructor: default
// ParaView dumper and its "positions" field), src/mesh/group_manager.hh,
// src/mesh/group_manager_inline_impl.hh (createElementalField,
// createElementalFilteredField, createNodalField, createStridedNodalField),
// src/mesh/mesh.cc (Mesh::getGroupDumper, Mesh::createFieldFromAttachedData,
// Mesh::getNbDataPerElem), src/mesh/group_manager.cc (renameGroup).

//! Default dumpers of element and node groups (MESH-022, MESH-023), the
//! group-aware field factories (MESH-027) and `Mesh::getGroupDumper`.
//!
//! Upstream every `ElementGroup` and `NodeGroup` is `Dumpable`; its
//! constructor registers a default ParaView dumper named `paraview_<name>`
//! writing files named `<name>`:
//!
//! * element group: the filtered mesh of the group's elements and nodes over
//!   **all dimensions** (`addDumpFilteredMesh(mesh, elements, nodes,
//!   _all_dimensions)`, not-ghost, every kind);
//! * node group: no mesh, the field `positions` restricted to the group's
//!   nodes (a point set: one poly-vertex cell).
//!
//! The port keeps meshes free of I/O (the mesh crate does not depend on the
//! dumpers): [`MeshDumpers`] owns the dumpers upstream would attach to the
//! mesh and its groups (ADR "group-dumper registry"):
//!
//! * **One registry per mesh**, shared by every model on it: the registry of
//!   a mesh domain is [`SharedMeshDumpers::of`] (an attachment of the
//!   [`SharedMesh`], the same object through every handle). Upstream's
//!   `Model::dumpGroup`, `addDumpGroupField*` and `Mesh::getGroupDumper` all
//!   reach the group's own dumpers, so two models on one mesh write one
//!   `paraview_<group>` series; a model must not keep group dumpers of its
//!   own. [`MeshDumpers::new`] is for a mesh outside a domain.
//! * **Groups by identity, not by name**: the dumpers of a group are keyed
//!   by its object identity ([`akantu_mesh::group::ElementGroup::uid`],
//!   [`akantu_mesh::group::NodeGroup::uid`]). A group's default dumper is
//!   created the first time it is asked for (identical to upstream's
//!   constructor-registered one as long as the group keeps its identity); a
//!   destroyed group's dumpers are dropped, and a group re-created under the
//!   same name starts a new series with fresh defaults (upstream: a new
//!   group object registers a new `paraview_<name>`).
//! * **One rename entry point**: renaming goes through the mesh only
//!   (`GroupManager::rename_element_group` / `rename_node_group`, in place:
//!   UD-671). Every access to the registry first reconciles it with the
//!   mesh ([`MeshDumpers::sync`]): a group seen under a new name keeps its
//!   dumpers (dumper and file names stay the old ones, as upstream, whose
//!   dumpers refer to the group object), and every selection and field of
//!   the registry's dumpers naming the old group name the new one.
//!   [`rename_group_in`] re-targets a [`Dumpable`] outside the registry
//!   (not needed by the port's own models: group-restricted fields belong
//!   to the group's dumpers, as upstream).
//!
//! **Field factories.** Upstream `createElementalField` /
//! `createNodalField` / `createStridedNodalField` return dumper fields that
//! read a given array, either over the whole mesh (group name `"all"`) or
//! restricted to an element group. Here they return [`FieldDescriptor`]s
//! (`docs/ARCHITECTURE.md`): `"all"` gives `group = None`, any other name the
//! group, which must exist (upstream throws). The elemental factories check
//! that the group's dimension equals the requested one; upstream executes a
//! bare `throw;` there, which terminates the process (UD-051, category A:
//! the port returns an error). Nodal fields restricted to a group use the
//! node group of the **element** group of that name (a node group alone is
//! not accepted, as upstream's `getElementGroup`). The dimension and kind
//! arguments only serve the group check: the written element types are the
//! dumper's selection (upstream: the field's own filter), which coincide
//! for the registrations upstream makes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use akantu_core::{ElementKind, Error, GhostType, Result};
use akantu_mesh::group::ALL_DIMENSIONS;
use akantu_mesh::type_map_init::nb_data_per_elem_shared;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_mesh::{MeshDataTypeCode, MeshDataValues};

use super::{
    Dumpable, Dumper, FieldDescriptor, FieldSource, FieldSupport, FieldValueType, MeshFilter,
    MeshSelection, Padding,
};

/// Upstream's whole-mesh group name.
pub const ALL_GROUP: &str = "all";

/// Name of the default dumper of a group (`"paraview_" + name`).
#[must_use]
pub fn group_dumper_name(group: &str) -> String {
    format!("paraview_{group}")
}

/// The dumpers upstream's `ElementGroup` constructor registers: a default
/// ParaView dumper `paraview_<group>` (file name `<group>`) on the group's
/// elements and nodes, all dimensions, not-ghost, every kind.
#[must_use]
pub fn element_group_dumpable(group: &str) -> Dumpable {
    let mut d = Dumpable::new();
    let name = group_dumper_name(group);
    d.register_paraview(&name, Some(group), true);
    // a fresh ParaView dumper accepts its first selection
    let _ = d.add_dump_mesh_to_dumper(
        &name,
        MeshSelection::new(None, GhostType::NotGhost, ElementKind::NotDefined)
            .filtered(MeshFilter::ElementGroup(group.to_owned())),
    );
    d
}

/// The dumpers upstream's `NodeGroup` constructor registers: a default
/// ParaView dumper `paraview_<group>` (file name `<group>`) writing the
/// group's nodes as a point set with the field `positions`.
#[must_use]
pub fn node_group_dumpable(group: &str) -> Dumpable {
    let mut d = Dumpable::new();
    let name = group_dumper_name(group);
    d.register_paraview(&name, Some(group), true);
    let _ = d.add_dump_mesh_to_dumper(
        &name,
        MeshSelection::new(None, GhostType::NotGhost, ElementKind::NotDefined)
            .filtered(MeshFilter::NodeGroup(group.to_owned())),
    );
    d
}

/// Make every dumper of `dumpable` follow a renamed group: mesh selections
/// on group `old` (an element group if `element`, else a node group) and
/// field descriptors restricted to element group `old` name `new`
/// afterwards (upstream dumpers refer to the group object, not its name).
/// [`MeshDumpers`] does this by itself for its own dumpers; this is for a
/// [`Dumpable`] outside the registry.
pub fn rename_group_in(dumpable: &mut Dumpable, element: bool, old: &str, new: &str) {
    rename_groups_in(
        dumpable,
        element,
        &BTreeMap::from([(old.to_owned(), new.to_owned())]),
    );
}

/// [`rename_group_in`] for several `old → new` renames applied
/// simultaneously (a swap of two names is one call).
pub fn rename_groups_in(
    dumpable: &mut Dumpable,
    element: bool,
    renames: &BTreeMap<String, String>,
) {
    if renames.is_empty() {
        return;
    }
    let names: Vec<String> = dumpable.dumper_names().map(str::to_owned).collect();
    for n in names {
        if let Ok(d) = dumpable.dumper_mut(&n) {
            d.rename_groups(element, renames);
        }
    }
}

/// The dumpers upstream attaches to a mesh (`Mesh` is `Dumpable`) and to
/// each of its element and node groups, keyed by group identity (module
/// docs). One registry per mesh: for a mesh in a [`SharedMesh`] domain use
/// the domain's registry, [`SharedMeshDumpers::of`].
#[derive(Debug, Default)]
pub struct MeshDumpers {
    mesh: Dumpable,
    /// Dumpers of element groups, by group identity.
    element_groups: BTreeMap<u64, Dumpable>,
    /// Dumpers of node groups, by group identity.
    node_groups: BTreeMap<u64, Dumpable>,
    /// Name of every element group of the mesh at the last [`Self::sync`],
    /// by identity (renames are detected against it).
    element_names: BTreeMap<u64, String>,
    /// Same for node groups.
    node_names: BTreeMap<u64, String>,
}

/// `old → new` for every group of `known` that `live` holds under another
/// name.
fn renames(
    known: &BTreeMap<u64, String>,
    live: &BTreeMap<u64, String>,
) -> BTreeMap<String, String> {
    known
        .iter()
        .filter_map(|(uid, old)| {
            live.get(uid)
                .filter(|new| *new != old)
                .map(|new| (old.clone(), new.clone()))
        })
        .collect()
}

impl MeshDumpers {
    /// No dumper yet (upstream `Mesh` registers none by default).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconcile the registry with the groups of `mesh` (done by every
    /// accessor below): drop the dumpers of destroyed (or replaced) groups,
    /// and for each group renamed since the last reconciliation re-target
    /// every selection and field of the registry's dumpers from its old
    /// name to its new one (simultaneously, so swaps are followed).
    pub fn sync(&mut self, mesh: &Mesh) {
        let groups = mesh.groups();
        let live_elements: BTreeMap<u64, String> = groups
            .element_groups()
            .map(|g| (g.uid(), g.name().to_owned()))
            .collect();
        let live_nodes: BTreeMap<u64, String> = groups
            .node_groups()
            .map(|g| (g.uid(), g.name().to_owned()))
            .collect();
        let element_renames = renames(&self.element_names, &live_elements);
        let node_renames = renames(&self.node_names, &live_nodes);
        self.element_groups
            .retain(|uid, _| live_elements.contains_key(uid));
        self.node_groups
            .retain(|uid, _| live_nodes.contains_key(uid));
        for (element, map) in [(true, &element_renames), (false, &node_renames)] {
            if map.is_empty() {
                continue;
            }
            rename_groups_in(&mut self.mesh, element, map);
            for d in self.element_groups.values_mut() {
                rename_groups_in(d, element, map);
            }
            for d in self.node_groups.values_mut() {
                rename_groups_in(d, element, map);
            }
        }
        self.element_names = live_elements;
        self.node_names = live_nodes;
    }

    /// The mesh's own dumpers (after [`Self::sync`]).
    pub fn mesh(&mut self, mesh: &Mesh) -> &mut Dumpable {
        self.sync(mesh);
        &mut self.mesh
    }

    /// The dumpers of element group `name` (after [`Self::sync`]; created
    /// with upstream's default dumper on first access to this group
    /// object).
    ///
    /// # Errors
    /// `mesh` has no element group `name`.
    pub fn element_group(&mut self, mesh: &Mesh, name: &str) -> Result<&mut Dumpable> {
        self.sync(mesh);
        let uid = mesh.groups().element_group(name)?.uid();
        Ok(self
            .element_groups
            .entry(uid)
            .or_insert_with(|| element_group_dumpable(name)))
    }

    /// The dumpers of node group `name` (after [`Self::sync`]; created with
    /// upstream's default dumper on first access to this group object).
    ///
    /// # Errors
    /// `mesh` has no node group `name`.
    pub fn node_group(&mut self, mesh: &Mesh, name: &str) -> Result<&mut Dumpable> {
        self.sync(mesh);
        let uid = mesh.groups().node_group(name)?.uid();
        Ok(self
            .node_groups
            .entry(uid)
            .or_insert_with(|| node_group_dumpable(name)))
    }

    /// Upstream `Mesh::getGroupDumper(dumper_name, group_name)`: the mesh's
    /// dumper for `"all"`, else the dumper of the element group. Upstream
    /// dereferences a null group for an unknown name (`element_groups[name]`
    /// inserts an empty `unique_ptr`; UD-672, category A): an error here.
    ///
    /// # Errors
    /// Unknown group or dumper.
    pub fn group_dumper(
        &mut self,
        mesh: &Mesh,
        dumper_name: &str,
        group_name: &str,
    ) -> Result<&mut dyn Dumper> {
        if group_name == ALL_GROUP {
            return self.mesh(mesh).dumper_mut(dumper_name);
        }
        self.element_group(mesh, group_name)?
            .dumper_mut(dumper_name)
    }
}

/// The group-dumper registry of a mesh domain (ADR condition 1):
/// one [`MeshDumpers`] per [`SharedMesh`], shared by every model on
/// that mesh (an attachment of the domain, dropped with it). Lock it per
/// operation, after taking the mesh's read guard:
///
/// ```
/// use akantu_io::dumper::group::SharedMeshDumpers;
/// use akantu_mesh::{Mesh, SharedMesh};
///
/// let shared = SharedMesh::new(Mesh::new(2, "m")?);
/// let model_a = SharedMeshDumpers::of(&shared)?;
/// let model_b = SharedMeshDumpers::of(&shared.clone())?; // e.g. a second model
/// assert!(model_a.ptr_eq(&model_b));
/// let mesh = shared.read()?;
/// let mut dumpers = model_a.lock()?;
/// dumpers.mesh(&mesh).register_paraview("all", None, true);
/// assert!(model_b.lock().is_err()); // never waits: in use
/// # Ok::<(), akantu_core::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct SharedMeshDumpers(Arc<RegistrySlot>);

/// Private attachment type: only this module can reach the domain's
/// registry.
#[derive(Debug, Default)]
struct RegistrySlot(Mutex<MeshDumpers>);

impl SharedMeshDumpers {
    /// The registry of `mesh`'s domain (created empty on first request; the
    /// same object for every handle on the domain).
    ///
    /// # Errors
    /// The domain's attachment lock was poisoned.
    pub fn of(mesh: &SharedMesh) -> Result<Self> {
        mesh.attachment(RegistrySlot::default).map(Self)
    }

    /// Exclusive access for one operation. Never waits (as the mesh domain,
    /// `docs/ARCHITECTURE.md`): a registry already locked — by a re-entrant call
    /// or another thread — is an error.
    ///
    /// # Errors
    /// The registry is in use, or its lock was poisoned by a panic.
    pub fn lock(&self) -> Result<MutexGuard<'_, MeshDumpers>> {
        match self.0.0.try_lock() {
            Ok(g) => Ok(g),
            Err(TryLockError::WouldBlock) => Err(Error::Mesh(
                "the group-dumper registry of this mesh is in use (re-entrant or concurrent access)"
                    .into(),
            )),
            Err(TryLockError::Poisoned(_)) => Err(Error::Mesh(
                "the group-dumper registry of this mesh was poisoned by a panic".into(),
            )),
        }
    }

    /// `true` if both handles are the same registry (same domain).
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// `"all"` → `None`; another name → `Some(name)` if `mesh` has that element
/// group (upstream `getElementGroup` throws otherwise).
fn element_group_of(mesh: &Mesh, group: &str) -> Result<Option<String>> {
    if group == ALL_GROUP {
        return Ok(None);
    }
    mesh.groups().element_group(group)?;
    Ok(Some(group.to_owned()))
}

/// Element group restriction with upstream's dimension check
/// (`createElementalFilteredField`: `group.getDimension() ==
/// spatial_dimension`, else a bare `throw;`, UD-051).
fn checked_elemental_group(
    mesh: &Mesh,
    group: &str,
    spatial_dimension: Option<usize>,
) -> Result<Option<String>> {
    if group == ALL_GROUP {
        return Ok(None);
    }
    let g = mesh.groups().element_group(group)?;
    let requested = spatial_dimension.unwrap_or(ALL_DIMENSIONS);
    if g.dimension != requested {
        let show = |d: usize| {
            if d == ALL_DIMENSIONS {
                "_all_dimensions".to_owned()
            } else {
                d.to_string()
            }
        };
        return Err(Error::InvalidArgument(format!(
            "elemental dump field on group '{group}': the group has dimension {} but dimension {} \
             was requested (upstream terminates, UD-051)",
            show(g.dimension),
            show(requested)
        )));
    }
    Ok(Some(group.to_owned()))
}

fn padding_of(padding_size: usize) -> Result<Padding> {
    match padding_size {
        0 => Ok(Padding::None),
        3 => Ok(Padding::To3D),
        n => Err(Error::Unsupported(format!(
            "nodal dump field padding to {n} components (only 0 and 3, the sizes upstream's \
             callers use, are supported)"
        ))),
    }
}

/// Upstream `GroupManager::createNodalField(field, group_name,
/// padding_size)`: the nodal `source` (all components) over the whole mesh
/// (`"all"`) or the nodes of element group `group`, padded to
/// `padding_size` components (0: none; 3: [`Padding::To3D`]).
///
/// # Errors
/// Unknown element group; a padding size other than 0 or 3.
pub fn create_nodal_field(
    mesh: &Mesh,
    name: &str,
    source: FieldSource,
    value_type: FieldValueType,
    group: &str,
    padding_size: usize,
) -> Result<FieldDescriptor> {
    create_strided_nodal_field(mesh, name, source, value_type, group, 0, 0, padding_size)
}

/// Upstream `GroupManager::createStridedNodalField(field, group_name, size,
/// stride, padding_size)`: components `stride .. stride + size` of the
/// nodal `source` (`size = 0`, `stride = 0`: all components).
///
/// # Errors
/// As [`create_nodal_field`]; `size = 0` with `stride > 0` (upstream takes
/// the remaining `nb_component − stride` components, which a descriptor
/// cannot know before dump time: give `size` explicitly).
#[allow(clippy::too_many_arguments)] // upstream's argument list plus name, source and type
pub fn create_strided_nodal_field(
    mesh: &Mesh,
    name: &str,
    source: FieldSource,
    value_type: FieldValueType,
    group: &str,
    size: usize,
    stride: usize,
    padding_size: usize,
) -> Result<FieldDescriptor> {
    let group = element_group_of(mesh, group)?;
    let padding = padding_of(padding_size)?;
    let source = match (size, stride) {
        (0, 0) => source,
        (0, _) => {
            return Err(Error::Unsupported(format!(
                "strided nodal dump field '{name}': size 0 with stride {stride} (give the number \
                 of components)"
            )));
        }
        (count, first) => FieldSource::Components {
            source: Box::new(source),
            first,
            count,
        },
    };
    Ok(FieldDescriptor {
        name: name.to_owned(),
        source,
        support: FieldSupport::Nodal,
        value_type,
        group,
        padding,
        homogenize: false,
    })
}

/// Upstream `GroupManager::createElementalField(field, group_name,
/// spatial_dimension, kind, nb_data_per_elem)` (all three overloads:
/// elemental, generic elemental and internal-material fields): the
/// elemental or quadrature-point `source` over the whole mesh (`"all"`) or
/// restricted to element group `group`, whose dimension must equal
/// `spatial_dimension` (`None`: `_all_dimensions`).
///
/// # Errors
/// Unknown group; group dimension ≠ `spatial_dimension` (UD-051); a nodal
/// `support`.
#[allow(clippy::too_many_arguments)] // upstream's argument list plus name, source, support, type
pub fn create_elemental_field(
    mesh: &Mesh,
    name: &str,
    source: FieldSource,
    support: FieldSupport,
    value_type: FieldValueType,
    group: &str,
    spatial_dimension: Option<usize>,
    kind: ElementKind,
    homogenize: bool,
) -> Result<FieldDescriptor> {
    let _ = kind; // selects element types upstream; the dumper's selection does here
    if support == FieldSupport::Nodal {
        return Err(Error::InvalidArgument(format!(
            "elemental dump field '{name}' with nodal support"
        )));
    }
    let group = checked_elemental_group(mesh, group, spatial_dimension)?;
    Ok(FieldDescriptor {
        name: name.to_owned(),
        source,
        support,
        value_type,
        group,
        padding: Padding::None,
        homogenize,
    })
}

/// Upstream `Mesh::createFieldFromAttachedData<T>(field_id, group_name,
/// element_kind)` with `T` given by `value_type` (upstream instantiates
/// `Real` and `Int`): the mesh data `field_id` as a quadrature-point field
/// (`nb_data_per_elem` values per element, `getNbDataPerElem`), restricted
/// to element group `group` (dimension = the mesh's) unless `"all"`.
/// `Ok(None)` when the mesh has no data `field_id` of type `T` (upstream
/// returns a null field).
///
/// # Errors
/// A type of the data without an element of that type in the mesh (upstream
/// divides by zero, UD-667); unknown group; group dimension ≠ mesh
/// dimension (UD-051); a `value_type` other than `Real`/`Int`.
pub fn create_field_from_attached_data(
    mesh: &Mesh,
    field_id: &str,
    value_type: FieldValueType,
    group: &str,
    kind: ElementKind,
) -> Result<Option<FieldDescriptor>> {
    let wanted = match value_type {
        FieldValueType::Real => MeshDataTypeCode::Real,
        FieldValueType::Int => MeshDataTypeCode::Int,
        other => {
            return Err(Error::Unsupported(format!(
                "createFieldFromAttachedData<{other:?}> (upstream instantiates Real and Int)"
            )));
        }
    };
    let Ok(values) = mesh.data().elemental(field_id) else {
        return Ok(None);
    };
    if values.type_code() != wanted {
        return Ok(None);
    }
    match values {
        MeshDataValues::Int(a) => {
            nb_data_per_elem_shared(mesh, a)?;
        }
        MeshDataValues::Real(a) => {
            nb_data_per_elem_shared(mesh, a)?;
        }
        _ => return Ok(None),
    }
    create_elemental_field(
        mesh,
        field_id,
        FieldSource::Mesh(field_id.to_owned()),
        FieldSupport::QuadraturePoint,
        value_type,
        group,
        Some(mesh.spatial_dimension()),
        kind,
        false,
    )
    .map(Some)
}
