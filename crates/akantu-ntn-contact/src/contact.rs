// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/{ntn_base_contact.hh, ntn_base_contact.cc,
// ntn_base_contact_inline_impl.hh, ntn_contact.hh, ntn_contact.cc,
// ntrf_contact.hh, ntrf_contact.cc, mIIasym_contact.hh, mIIasym_contact.cc}.

//! Node-to-node, node-to-rigid-flat and mode-II antisymmetric contact
//! (upstream `NTNBaseContact`, `NTNContact`, `NTRFContact`,
//! `MIIASYMContact`; TASN-001…003).
//!
//! The upstream class hierarchy is one struct, [`NtnBaseContact`], with a
//! [`ContactVariant`] for the variant-specific data (master nodes of the
//! node-to-node pairs, reference point and normal of the rigid plane); the
//! virtual methods dispatch on it.
//!
//! **Algorithm** (Kammer's traction-at-split-node method, explicit
//! dynamics): with the impedance `Z` of every contact node, the predicted
//! normal gap `g = g_n + Δt v_n + Δt² a_new − ½ Δt² a_old` (relative normal
//! fields, `a_new` the free acceleration `(f_int + f_ext)/m` of the current
//! residual) closes iff `g ≤ 0`, and then the contact pressure is
//! `p = Z g / (2Δt) · n`; it is assembled with the lumped boundary areas
//! into the residual (master `+A_m p`, slave `−A_s p`).
//!
//! Every floating-point expression follows the upstream evaluation order
//! (sequential Eigen reductions, no contraction) for bitwise reproduction.
//!
//! **Distributed meshes** (MPI-051/052): upstream's
//! `initParallel` (the element synchronizer filtered on the contact nodes)
//! and its `_cf_nodal` / `_cf_incr` exchanges are in [`crate::parallel`];
//! without `init_parallel` the exchanges, hence a contact step, are refused
//! (UD-904, category A). The counts upstream all-reduces
//! ([`NtnBaseContact::nb_nodes_in_contact`]) are collective there.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::Mesh;
use akantu_mesh::utils::{NodeToElements, build_node_to_elements};
use akantu_model::model::{ReduceOperation, make_consistent_for_periodicity};

use crate::TasnUpstreamCompat;
use crate::state::{ModelState, check_len};
use crate::sync_array::{SyncChoice, SyncJournal, SynchronizedArray};

/// Per-type element lists (upstream `ElementTypeMapArray<Idx>` of the
/// boundary elements of the contact, `slave_elements` / `master_elements`).
#[derive(Clone, Debug, Default)]
pub struct ElementBlocks {
    blocks: BTreeMap<(ElementType, GhostType), Vec<Idx>>,
    members: HashSet<Element>,
}

impl ElementBlocks {
    /// Elements of `(ty, ghost)` in insertion order.
    #[must_use]
    pub fn of(&self, ty: ElementType, ghost: GhostType) -> &[Idx] {
        self.blocks.get(&(ty, ghost)).map_or(&[], Vec::as_slice)
    }

    /// Every element, block by block.
    pub fn iter(&self) -> impl Iterator<Item = Element> + '_ {
        self.blocks
            .iter()
            .flat_map(|(&(ty, ghost), v)| v.iter().map(move |&e| Element::new(ty, e, ghost)))
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// No element.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    fn contains(&self, el: Element) -> bool {
        self.members.contains(&el)
    }

    fn push(&mut self, el: Element) {
        if self.members.insert(el) {
            self.blocks
                .entry((el.ty(), el.ghost_type()))
                .or_default()
                .push(el.index());
        }
    }
}

/// Variant-specific data of a contact.
// one value per contact object: boxing the node-to-node payload would only add
// an indirection
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum ContactVariant {
    /// `NTNContact`: slave/master node pairs.
    NodeToNode {
        /// Master node of each pair (upstream `masters`).
        masters: SynchronizedArray<Idx>,
        /// Lumped boundary area of each master node.
        lumped_boundary_masters: SynchronizedArray<Real>,
        /// Boundary elements whose nodes are all master nodes.
        master_elements: ElementBlocks,
    },
    /// `NTRFContact`: contact against a rigid flat plane.
    RigidFlat {
        /// A point of the plane.
        reference_point: Vec<Real>,
        /// Outward unit normal of the plane.
        normal: Vec<Real>,
    },
    /// `MIIASYMContact`: rigid-flat contact for a mode-II antisymmetric
    /// setup (half impedance, doubled relative fields, pressure computed only
    /// on request in equilibrium).
    ModeIIAntisymmetric {
        /// A point of the plane.
        reference_point: Vec<Real>,
        /// Outward unit normal of the plane.
        normal: Vec<Real>,
    },
}

/// Which variant (upstream class) a contact is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactKind {
    /// `NTNContact`.
    NodeToNode,
    /// `NTRFContact`.
    RigidFlat,
    /// `MIIASYMContact`.
    ModeIIAntisymmetric,
}

/// Upstream `NTNBaseContact` and its subclasses.
#[derive(Debug)]
pub struct NtnBaseContact {
    id: String,
    spatial_dimension: usize,
    slaves: SynchronizedArray<Idx>,
    normals: SynchronizedArray<Real>,
    contact_pressure: SynchronizedArray<Real>,
    is_in_contact: SynchronizedArray<bool>,
    lumped_boundary_slaves: SynchronizedArray<Real>,
    impedance: SynchronizedArray<Real>,
    global_contact_pressure: Array<Real>,
    global_contact_pressure_old: Array<Real>,
    /// Upstream `contact_surfaces` (a set of group pointers, only tested for
    /// emptiness): the names of the groups added.
    contact_surfaces: BTreeSet<String>,
    slave_elements: ElementBlocks,
    node_to_elements: NodeToElements,
    boundary_fe: FeEngine,
    variant: ContactVariant,
    journal: SyncJournal,
    compat: TasnUpstreamCompat,
    /// Upstream `synchronizer`: the filtered element synchronizer of
    /// `initParallel` (`None` until [`NtnBaseContact::init_parallel`]).
    synchronizer: Option<akantu_comm::Synchronizer<Element>>,
    /// `init_parallel` ran on a mesh distributed over more than one rank:
    /// the contact exchanges before reading ghost data, so the read-only
    /// [`NtnBaseContact::update_normals`] and further node pairs are
    /// refused.
    exchanging: bool,
    /// The surface pairs added with [`NtnBaseContact::add_surface_pair`],
    /// in call order: the per-rank pairing that `init_parallel` checks
    /// against the pairing of the whole mesh on a distributed mesh
    /// (UD-1105, D-097 (2)).
    surface_pairings: Vec<SurfacePairing>,
}

/// One [`NtnBaseContact::add_surface_pair`] call (arguments of upstream
/// `pairInterfaceNodes`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfacePairing {
    pub(crate) slave: String,
    pub(crate) master: String,
    pub(crate) surface_normal_dir: usize,
    pub(crate) keep_searching: bool,
}

fn nan_array<T: crate::sync_array::SyncValue>(
    nb_component: usize,
    id: &str,
    name: &str,
    default: T,
) -> Result<SynchronizedArray<T>> {
    SynchronizedArray::new(nb_component, format!("{id}:{name}"), default, name)
}

/// First index of every value (upstream `Array::find` returns the first
/// occurrence).
fn first_index_map(nodes: &[Idx]) -> HashMap<Idx, usize> {
    let mut m = HashMap::with_capacity(nodes.len());
    for (i, &n) in nodes.iter().enumerate() {
        m.entry(n).or_insert(i);
    }
    m
}

fn ux(n: Idx) -> Result<usize> {
    usize::try_from(n).map_err(|_| Error::InvalidIndex {
        what: "node",
        value: i64::from(n),
    })
}

/// Sequential dot product (Eigen redux order without vectorization).
#[inline]
pub(crate) fn dot(a: &[Real], b: &[Real]) -> Real {
    let mut s = a[0] * b[0];
    for i in 1..a.len() {
        s += a[i] * b[i];
    }
    s
}

/// Eigen `norm()` = `sqrt(squaredNorm())`, sequential.
#[inline]
pub(crate) fn norm(a: &[Real]) -> Real {
    dot(a, a).sqrt()
}

/// Eigen `normalize()`: divide by the norm when the squared norm is
/// positive, leave unchanged otherwise.
#[inline]
pub(crate) fn normalize(a: &mut [Real]) {
    let z = dot(a, a);
    if z > 0.0 {
        let s = z.sqrt();
        for v in a.iter_mut() {
            *v /= s;
        }
    }
}

impl NtnBaseContact {
    fn with_variant(
        mesh: &Mesh,
        spatial_dimension: usize,
        id: &str,
        variant: ContactVariant,
        compat: TasnUpstreamCompat,
    ) -> Result<Self> {
        if spatial_dimension == 0 || spatial_dimension > 3 {
            return Err(Error::invalid(format!(
                "NTNBaseContact: invalid spatial dimension {spatial_dimension}"
            )));
        }
        let dim = spatial_dimension;
        let nan = Real::NAN;
        let boundary_fe = FeEngine::with_precomputation(
            mesh,
            Some(dim - 1),
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            format!("{id}:boundary_fem"),
        )?;
        let node_to_elements = build_node_to_elements(mesh, Some(dim - 1))?;
        Ok(Self {
            id: id.to_owned(),
            spatial_dimension,
            // upstream default: quiet_NaN of UInt = 0
            slaves: nan_array(1, id, "slaves", 0)?,
            normals: nan_array(dim, id, "normals", nan)?,
            contact_pressure: nan_array(dim, id, "contact_pressure", nan)?,
            is_in_contact: nan_array(1, id, "is_in_contact", false)?,
            lumped_boundary_slaves: nan_array(1, id, "lumped_boundary_slaves", nan)?,
            impedance: nan_array(1, id, "impedance", nan)?,
            global_contact_pressure: Array::zeros(mesh.nb_nodes(), dim),
            global_contact_pressure_old: Array::zeros(0, dim),
            contact_surfaces: BTreeSet::new(),
            slave_elements: ElementBlocks::default(),
            node_to_elements,
            boundary_fe,
            variant,
            journal: SyncJournal::default(),
            compat,
            synchronizer: None,
            exchanging: false,
            surface_pairings: Vec::new(),
        })
    }

    /// Upstream `NTNContact(model, id)`.
    ///
    /// # Errors
    /// Distributed mesh, invalid dimension, or boundary FE engine failure.
    pub fn new_ntn(
        mesh: &Mesh,
        spatial_dimension: usize,
        id: &str,
        compat: TasnUpstreamCompat,
    ) -> Result<Self> {
        let variant = ContactVariant::NodeToNode {
            masters: nan_array(1, id, "masters", 0)?,
            lumped_boundary_masters: nan_array(1, id, "lumped_boundary_masters", Real::NAN)?,
            master_elements: ElementBlocks::default(),
        };
        Self::with_variant(mesh, spatial_dimension, id, variant, compat)
    }

    /// Upstream `NTRFContact(model, id)` (reference point and normal zero
    /// until set, as upstream's `Vector<Real>(dim)`).
    ///
    /// # Errors
    /// See [`NtnBaseContact::new_ntn`].
    pub fn new_ntrf(
        mesh: &Mesh,
        spatial_dimension: usize,
        id: &str,
        compat: TasnUpstreamCompat,
    ) -> Result<Self> {
        let variant = ContactVariant::RigidFlat {
            reference_point: vec![0.0; spatial_dimension],
            normal: vec![0.0; spatial_dimension],
        };
        Self::with_variant(mesh, spatial_dimension, id, variant, compat)
    }

    /// Upstream `MIIASYMContact(model, id)`.
    ///
    /// # Errors
    /// See [`NtnBaseContact::new_ntn`].
    pub fn new_miiasym(
        mesh: &Mesh,
        spatial_dimension: usize,
        id: &str,
        compat: TasnUpstreamCompat,
    ) -> Result<Self> {
        let variant = ContactVariant::ModeIIAntisymmetric {
            reference_point: vec![0.0; spatial_dimension],
            normal: vec![0.0; spatial_dimension],
        };
        Self::with_variant(mesh, spatial_dimension, id, variant, compat)
    }

    // ---- accessors ------------------------------------------------------

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Spatial dimension.
    #[must_use]
    pub fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }

    /// Variant.
    #[must_use]
    pub fn kind(&self) -> ContactKind {
        match self.variant {
            ContactVariant::NodeToNode { .. } => ContactKind::NodeToNode,
            ContactVariant::RigidFlat { .. } => ContactKind::RigidFlat,
            ContactVariant::ModeIIAntisymmetric { .. } => ContactKind::ModeIIAntisymmetric,
        }
    }

    /// Variant data.
    #[must_use]
    pub fn variant(&self) -> &ContactVariant {
        &self.variant
    }

    /// Upstream `isNTNContact()` (`false` for the rigid-flat variants).
    #[must_use]
    pub fn is_ntn_contact(&self) -> bool {
        self.kind() == ContactKind::NodeToNode
    }

    /// Upstream-defect switches of this contact.
    #[must_use]
    pub fn compat(&self) -> TasnUpstreamCompat {
        self.compat
    }

    /// Slave (contact) nodes.
    #[must_use]
    pub fn slaves(&self) -> &SynchronizedArray<Idx> {
        &self.slaves
    }

    /// Normal of every contact node.
    #[must_use]
    pub fn normals(&self) -> &SynchronizedArray<Real> {
        &self.normals
    }

    /// Contact pressure (vector) of every contact node.
    #[must_use]
    pub fn contact_pressure(&self) -> &SynchronizedArray<Real> {
        &self.contact_pressure
    }

    /// Whether every contact node is in contact.
    #[must_use]
    pub fn is_in_contact(&self) -> &SynchronizedArray<bool> {
        &self.is_in_contact
    }

    /// Lumped boundary area of every slave node.
    #[must_use]
    pub fn lumped_boundary_slaves(&self) -> &SynchronizedArray<Real> {
        &self.lumped_boundary_slaves
    }

    /// Upstream: set the lumped boundary areas yourself when no surface was
    /// added (the warning of `internalUpdateLumpedBoundary`).
    pub fn lumped_boundary_slaves_mut(&mut self) -> &mut SynchronizedArray<Real> {
        &mut self.lumped_boundary_slaves
    }

    /// Impedance of every contact node.
    #[must_use]
    pub fn impedance(&self) -> &SynchronizedArray<Real> {
        &self.impedance
    }

    /// Global (per mesh node) contact force (upstream
    /// `getGlobalContactPressure`).
    #[must_use]
    pub fn global_contact_pressure(&self) -> &Array<Real> {
        &self.global_contact_pressure
    }

    /// Mutable global contact force.
    pub fn global_contact_pressure_mut(&mut self) -> &mut Array<Real> {
        &mut self.global_contact_pressure
    }

    /// Saved global contact force (upstream `getGlobalContactPressureOld`).
    #[must_use]
    pub fn global_contact_pressure_old(&self) -> &Array<Real> {
        &self.global_contact_pressure_old
    }

    /// Boundary elements of the slave nodes.
    #[must_use]
    pub fn slave_elements(&self) -> &ElementBlocks {
        &self.slave_elements
    }

    /// Master nodes (node-to-node contact only).
    #[must_use]
    pub fn masters(&self) -> Option<&SynchronizedArray<Idx>> {
        match &self.variant {
            ContactVariant::NodeToNode { masters, .. } => Some(masters),
            _ => None,
        }
    }

    /// Lumped boundary areas of the master nodes (node-to-node only).
    #[must_use]
    pub fn lumped_boundary_masters(&self) -> Option<&SynchronizedArray<Real>> {
        match &self.variant {
            ContactVariant::NodeToNode {
                lumped_boundary_masters,
                ..
            } => Some(lumped_boundary_masters),
            _ => None,
        }
    }

    /// Boundary elements of the master nodes (node-to-node only).
    #[must_use]
    pub fn master_elements(&self) -> Option<&ElementBlocks> {
        match &self.variant {
            ContactVariant::NodeToNode {
                master_elements, ..
            } => Some(master_elements),
            _ => None,
        }
    }

    /// Names of the surfaces added (upstream `contact_surfaces`).
    #[must_use]
    pub fn contact_surfaces(&self) -> &BTreeSet<String> {
        &self.contact_surfaces
    }

    /// The boundary FE engine (upstream `model.getFEEngineBoundary()`).
    #[must_use]
    pub fn boundary_fe(&self) -> &FeEngine {
        &self.boundary_fe
    }

    /// Journal of the synchronised node-list changes (replayed on the
    /// friction arrays).
    #[must_use]
    pub fn journal(&self) -> &SyncJournal {
        &self.journal
    }

    /// Number of contact nodes (upstream `getNbContactNodes`).
    #[must_use]
    pub fn nb_contact_nodes(&self) -> usize {
        self.slaves.len()
    }

    /// Upstream `getNodeIndex(node)`: index of `node` among the slaves, or
    /// (node-to-node) the larger of its slave and master indices.
    #[must_use]
    pub fn node_index(&self, node: Idx) -> Option<usize> {
        let s = self.slaves.find(node);
        match &self.variant {
            ContactVariant::NodeToNode { masters, .. } => {
                let m = masters.find(node);
                match (s, m) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                }
            }
            _ => s,
        }
    }

    /// Node pairs `(slave, master)` (upstream `NTNContact::getNodePairs`).
    ///
    /// # Errors
    /// Not a node-to-node contact.
    pub fn node_pairs(&self) -> Result<Vec<(Idx, Idx)>> {
        let masters = self.masters().ok_or_else(|| {
            Error::invalid("getNodePairs: only a node-to-node contact has node pairs")
        })?;
        Ok(self
            .slaves
            .as_slice()
            .iter()
            .zip(masters.as_slice())
            .map(|(&s, &m)| (s, m))
            .collect())
    }

    /// Upstream `getNbNodesInContact()`: contact nodes in contact that are
    /// local or master nodes and not periodic slaves, summed over the ranks
    /// of a distributed mesh (upstream `allReduce(_sum)`; collective there).
    ///
    /// # Errors
    /// A slave node is not a mesh node (agreed on every rank of a
    /// distributed mesh); communication errors.
    pub fn nb_nodes_in_contact(&self, mesh: &Mesh) -> Result<usize> {
        let local = (|| {
            let mut nb = 0;
            for (n, &s) in self.slaves.as_slice().iter().enumerate() {
                if mesh.is_local_or_master_node(s)?
                    && !mesh.is_periodic_slave(s)?
                    && self.is_in_contact.at(n)
                {
                    nb += 1;
                }
            }
            Ok(nb)
        })();
        crate::parallel::sum_count(mesh, local, "nb_nodes_in_contact")
    }

    /// The filtered synchronizer (crate-internal access).
    pub(crate) fn synchronizer_ref(&self) -> Option<&akantu_comm::Synchronizer<Element>> {
        self.synchronizer.as_ref()
    }

    /// Install the filtered synchronizer; `exchanging`: the mesh is
    /// distributed over more than one rank.
    pub(crate) fn set_synchronizer(
        &mut self,
        sync: Option<akantu_comm::Synchronizer<Element>>,
        exchanging: bool,
    ) {
        self.exchanging = exchanging && sync.is_some();
        self.synchronizer = sync;
    }

    /// The surface pairs added so far (crate-internal: the pairing check
    /// of `init_parallel`).
    pub(crate) fn surface_pairings(&self) -> &[SurfacePairing] {
        &self.surface_pairings
    }

    /// Refuse new contact nodes after `init_parallel` on a mesh distributed
    /// over more than one rank, before anything changes: the filtered
    /// synchronizer is built from the contact nodes known at
    /// `init_parallel` (upstream `filterScheme`), so a node added later
    /// lies outside it and its ghost data stays stale; a surface pair would
    /// also escape the pairing check of `init_parallel` (UD-1105, D-097
    /// (2)), and adding pairs updates the normals from a read-only state,
    /// which cannot make upstream's `_cf_nodal` exchange.
    /// Applies to every addition: the pair additions, the low-level
    /// [`Self::add_split_node`] they use and the rigid-flat
    /// [`Self::add_surface`] / [`Self::add_nodes`].
    /// The same outcome on every rank (`init_parallel` is collective), and
    /// recoverable (nothing changed).
    fn refuse_after_init_parallel(&self, what: &str) -> Result<()> {
        if self.exchanging {
            return Err(Error::Unsupported(format!(
                "{what} after init_parallel() on a distributed mesh: the contact synchronizer is \
                 filtered with the contact nodes known at init_parallel, the per-rank pairing is \
                 checked there (UD-1105) and the normals of new pairs need the _cf_nodal \
                 exchange; add every contact node before init_parallel"
            )));
        }
        Ok(())
    }

    // ---- node registration ---------------------------------------------

    /// Upstream `addSplitNode(node, master)`: append a contact node (and its
    /// master for a node-to-node contact; ignored otherwise) with NaN normal,
    /// pressure, impedance and lumped boundary, not in contact. The change is
    /// propagated by [`Self::sync_arrays`] (called by the `add_*` helpers).
    ///
    /// # Errors
    /// Unsynchronised erasures are pending; called after
    /// [`NtnBaseContact::init_parallel`] on a mesh distributed over more
    /// than one rank (`Error::Unsupported`, nothing changed).
    pub fn add_split_node(&mut self, node: Idx, master: Idx) -> Result<()> {
        self.refuse_after_init_parallel("addSplitNode")?;
        let dim = self.spatial_dimension;
        self.slaves.push_value(node)?;
        self.is_in_contact.push_value(false)?;
        self.contact_pressure.push_value(Real::NAN)?;
        self.impedance.push_value(Real::NAN)?;
        self.lumped_boundary_slaves.push_value(Real::NAN)?;
        self.normals.push_tuple(&vec![Real::NAN; dim])?;
        if let ContactVariant::NodeToNode {
            masters,
            lumped_boundary_masters,
            ..
        } = &mut self.variant
        {
            masters.push_value(master)?;
            lumped_boundary_masters.push_value(Real::NAN)?;
        }
        Ok(())
    }

    /// Upstream `syncArrays(choice)`: synchronise every array of the contact
    /// and record the node-list change for the friction arrays.
    ///
    /// # Errors
    /// Array synchronisation failure.
    pub fn sync_arrays(&mut self, choice: SyncChoice) -> Result<()> {
        if let Some(record) = self.slaves.sync_elements(choice, &mut [])? {
            self.journal.push(record);
        }
        self.normals.sync_elements(choice, &mut [])?;
        self.is_in_contact.sync_elements(choice, &mut [])?;
        self.contact_pressure.sync_elements(choice, &mut [])?;
        self.lumped_boundary_slaves.sync_elements(choice, &mut [])?;
        self.impedance.sync_elements(choice, &mut [])?;
        if let ContactVariant::NodeToNode {
            masters,
            lumped_boundary_masters,
            ..
        } = &mut self.variant
        {
            masters.sync_elements(choice, &mut [])?;
            lumped_boundary_masters.sync_elements(choice, &mut [])?;
        }
        Ok(())
    }

    /// Upstream `NTNContact::pairInterfaceNodes(slave, master, dir, mesh,
    /// pairs)`: pairs of nodes of the two groups whose coordinates projected
    /// on the plane normal to `surface_normal_dir` are closer than a tenth of
    /// the smallest distance between two projected slave nodes.
    ///
    /// **UD-392 (B):** upstream's `continue` after a match does not stop the
    /// search, so a slave matching several masters appears in several pairs;
    /// the port stops at the first match (`keep_searching` = compat).
    /// **UD-393 (A):** for a 1-D mesh upstream never stores the node indices
    /// (assigned inside the empty projection loop) and pairs node 0 with node
    /// 0; the port stores the actual nodes.
    ///
    /// # Errors
    /// Unknown group, or `surface_normal_dir >= dim`.
    pub fn pair_interface_nodes(
        mesh: &Mesh,
        slave_group: &str,
        master_group: &str,
        surface_normal_dir: usize,
        keep_searching: bool,
    ) -> Result<Vec<(Idx, Idx)>> {
        let dim = mesh.spatial_dimension();
        if surface_normal_dir >= dim {
            return Err(Error::invalid(format!(
                "Mesh is of {dim} dimensions and cannot have direction {surface_normal_dir} for surface normal"
            )));
        }
        let offset: Vec<usize> = (0..dim).filter(|&i| i != surface_normal_dir).collect();
        let pd = dim - 1;
        let coords = mesh.nodes();
        let groups = mesh.groups();
        let project = |name: &str| -> Result<(Vec<Real>, Vec<Idx>)> {
            let g = groups.element_group(name)?;
            let nodes = g.nodes().as_slice().to_vec();
            let mut proj = Vec::with_capacity(nodes.len() * pd);
            for &n in &nodes {
                let x = coords.get_tuple(ux(n)?)?;
                for &o in &offset {
                    proj.push(x[o]);
                }
            }
            Ok((proj, nodes))
        };
        let (ps, slave_nodes) = project(slave_group)?;
        let (pm, master_nodes) = project(master_group)?;
        let ns = slave_nodes.len();
        let nm = master_nodes.len();
        // smallest squared distance between two projected slave nodes
        let mut min_dist = Real::MAX;
        for i in 0..ns {
            for j in i + 1..ns {
                let mut dist = 0.0;
                for d in 0..pd {
                    dist += (ps[i * pd + d] - ps[j * pd + d]) * (ps[i * pd + d] - ps[j * pd + d]);
                }
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }
        let min_dist = min_dist.sqrt();
        let local_tol = 0.1 * min_dist;
        let mut pairs = Vec::new();
        for i in 0..ns {
            for j in 0..nm {
                let mut dist = 0.0;
                for d in 0..pd {
                    dist += (ps[i * pd + d] - pm[j * pd + d]) * (ps[i * pd + d] - pm[j * pd + d]);
                }
                let dist = dist.sqrt();
                if dist < local_tol {
                    pairs.push((slave_nodes[i], master_nodes[j]));
                    if !keep_searching {
                        break;
                    }
                }
            }
        }
        Ok(pairs)
    }

    /// Upstream `NTNContact::addSurfacePair(slave, master, dir)`: pair the
    /// nodes of the two groups, drop pairs with a periodic slave node, add
    /// them ([`Self::add_node_pairs`]).
    ///
    /// On a distributed mesh each rank pairs the nodes it holds (local or
    /// ghost), as upstream (D-097 (2)); [`NtnBaseContact::init_parallel`]
    /// refuses a partition on which that differs from the pairing of the
    /// whole mesh (UD-1105).
    ///
    /// # Errors
    /// Not a node-to-node contact, unknown group, called after
    /// [`NtnBaseContact::init_parallel`] on a distributed mesh, or a
    /// failure of [`Self::add_node_pairs`].
    pub fn add_surface_pair(
        &mut self,
        state: &ModelState<'_>,
        slave: &str,
        master: &str,
        surface_normal_dir: usize,
    ) -> Result<()> {
        if !self.is_ntn_contact() {
            return Err(Error::invalid(
                "addSurfacePair: only a node-to-node contact pairs surfaces",
            ));
        }
        self.refuse_after_init_parallel("addSurfacePair")?;
        let mesh = state.mesh;
        let groups = mesh.groups();
        groups.element_group(slave)?;
        groups.element_group(master)?;
        self.contact_surfaces.insert(slave.to_owned());
        self.contact_surfaces.insert(master.to_owned());
        let pairing = SurfacePairing {
            slave: slave.to_owned(),
            master: master.to_owned(),
            surface_normal_dir,
            keep_searching: self.compat.pairing_keeps_searching,
        };
        let kept = Self::surface_pairs(mesh, &pairing)?;
        self.surface_pairings.push(pairing);
        self.add_node_pairs(state, &kept)
    }

    /// The pairs [`NtnBaseContact::add_surface_pair`] adds for `pairing` on
    /// this rank: [`NtnBaseContact::pair_interface_nodes`] without the pairs
    /// holding a periodic slave node.
    pub(crate) fn surface_pairs(mesh: &Mesh, pairing: &SurfacePairing) -> Result<Vec<(Idx, Idx)>> {
        let pairs = Self::pair_interface_nodes(
            mesh,
            &pairing.slave,
            &pairing.master,
            pairing.surface_normal_dir,
            pairing.keep_searching,
        )?;
        let mut kept = Vec::with_capacity(pairs.len());
        for (s, m) in pairs {
            if !mesh.is_periodic_slave(s)? && !mesh.is_periodic_slave(m)? {
                kept.push((s, m));
            }
        }
        Ok(kept)
    }

    /// Upstream `NTNContact::addNodePairs(pairs)`: add every pair, collect
    /// the boundary elements of the slave and master nodes, update the
    /// normals, lumped boundaries and impedances, synchronise.
    ///
    /// # Errors
    /// Not a node-to-node contact, called after
    /// [`NtnBaseContact::init_parallel`] on a distributed mesh, or an update
    /// fails.
    pub fn add_node_pairs(&mut self, state: &ModelState<'_>, pairs: &[(Idx, Idx)]) -> Result<()> {
        if !self.is_ntn_contact() {
            return Err(Error::invalid(
                "addNodePairs: only a node-to-node contact has node pairs",
            ));
        }
        self.refuse_after_init_parallel("addNodePairs")?;
        for &(s, m) in pairs {
            self.add_split_node(s, m)?;
        }
        let mesh = state.mesh;
        let slave_nodes = self.slaves.as_slice().to_vec();
        Self::find_boundary_elements(
            mesh,
            &self.node_to_elements,
            &slave_nodes,
            &mut self.slave_elements,
        )?;
        if let ContactVariant::NodeToNode {
            masters,
            master_elements,
            ..
        } = &mut self.variant
        {
            let master_nodes = masters.as_slice().to_vec();
            Self::find_boundary_elements(
                mesh,
                &self.node_to_elements,
                &master_nodes,
                master_elements,
            )?;
        }
        self.update_internal_data(state)?;
        self.sync_arrays(SyncChoice::Added)
    }

    /// Upstream `NTRFContact::addSurface(surf)`: add the nodes of the group
    /// (periodic slaves excluded) as contact nodes, collect their boundary
    /// elements, update, synchronise. A missing group is ignored (upstream
    /// catches the exception: "other procs might have it").
    ///
    /// # Errors
    /// A node-to-node contact, called after
    /// [`NtnBaseContact::init_parallel`] on a mesh distributed over more
    /// than one rank (before anything changes, on every rank, also where
    /// the group is missing), or an update fails.
    pub fn add_surface(&mut self, state: &ModelState<'_>, surf: &str) -> Result<()> {
        if self.is_ntn_contact() {
            return Err(Error::invalid(
                "addSurface: only a rigid-flat contact adds single surfaces",
            ));
        }
        self.refuse_after_init_parallel("addSurface")?;
        let mesh = state.mesh;
        match mesh.groups().element_group(surf) {
            Ok(g) => {
                self.contact_surfaces.insert(surf.to_owned());
                for &node in g.nodes().as_slice() {
                    if !mesh.is_periodic_slave(node)? {
                        self.add_split_node(node, 0)?;
                    }
                }
            }
            Err(_) => log::info!(
                "NTRFContact addSurface did not find subboundary {surf} and ignored it. Other procs might have it :)"
            ),
        }
        self.finish_rigid_flat_addition(state)
    }

    /// Upstream `NTRFContact::addNodes(nodes)`: every value of `nodes` (all
    /// components) becomes a contact node.
    ///
    /// # Errors
    /// A node-to-node contact, called after
    /// [`NtnBaseContact::init_parallel`] on a mesh distributed over more
    /// than one rank (before anything changes, on every rank, also with an
    /// empty list), or an update fails.
    pub fn add_nodes(&mut self, state: &ModelState<'_>, nodes: &[Idx]) -> Result<()> {
        if self.is_ntn_contact() {
            return Err(Error::invalid(
                "addNodes: only a rigid-flat contact adds single nodes",
            ));
        }
        self.refuse_after_init_parallel("addNodes")?;
        for &n in nodes {
            self.add_split_node(n, 0)?;
        }
        self.finish_rigid_flat_addition(state)
    }

    fn finish_rigid_flat_addition(&mut self, state: &ModelState<'_>) -> Result<()> {
        let slave_nodes = self.slaves.as_slice().to_vec();
        Self::find_boundary_elements(
            state.mesh,
            &self.node_to_elements,
            &slave_nodes,
            &mut self.slave_elements,
        )?;
        self.update_internal_data(state)?;
        self.sync_arrays(SyncChoice::Added)
    }

    /// Upstream `findBoundaryElements(interface_nodes, elements)`: add the
    /// boundary elements (dimension `dim − 1`) around the interface nodes
    /// whose nodes are all interface nodes, once, in node then CSR order.
    fn find_boundary_elements(
        mesh: &Mesh,
        node_to_elements: &NodeToElements,
        interface_nodes: &[Idx],
        elements: &mut ElementBlocks,
    ) -> Result<()> {
        let set: HashSet<Idx> = interface_nodes.iter().copied().collect();
        for &node in interface_nodes {
            let Some(row) = node_to_elements.get_row(ux(node)?) else {
                return Err(Error::OutOfRange {
                    what: "node",
                    index: ux(node)?,
                    len: node_to_elements.nb_rows(),
                });
            };
            for &el in row {
                let conn = mesh.element_connectivity(el)?;
                let mut nb_found = 0;
                for nn in &conn {
                    if set.contains(nn) {
                        nb_found += 1;
                    } else {
                        break;
                    }
                }
                if nb_found == conn.len() && !elements.contains(el) {
                    elements.push(el);
                }
            }
        }
        Ok(())
    }

    // ---- internal data -------------------------------------------------

    /// Upstream `updateInternalData()`: normals, lumped boundary, impedance.
    /// A node-to-node contact after [`NtnBaseContact::init_parallel`] on a
    /// distributed mesh needs [`NtnBaseContact::update_internal_data_in`]
    /// (the normals exchange first).
    ///
    /// # Errors
    /// See the three updates.
    pub fn update_internal_data(&mut self, state: &ModelState<'_>) -> Result<()> {
        self.update_normals(state)?;
        self.update_lumped_boundary(state.mesh)?;
        self.update_impedance(state)
    }

    /// Upstream `updateInternalData()` inside a running collective
    /// operation of `dofs` (a solver callback): normals with upstream's
    /// `_cf_nodal` exchange ([`NtnBaseContact::update_normals_in`]),
    /// lumped boundary, impedance.
    ///
    /// # Errors
    /// See the three updates.
    pub fn update_internal_data_in(
        &mut self,
        physics: &mut akantu_solid::model::SolidMechanicsPhysics,
        dofs: &mut akantu_solver::DofManager,
    ) -> Result<()> {
        self.update_normals_in(physics, dofs)?;
        let mesh_h = physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.update_lumped_boundary(&mesh)?;
        let state = ModelState::of_parts(&mesh, physics, dofs)?;
        self.update_impedance(&state)
    }

    /// [`NtnBaseContact::update_internal_data_in`] as its own collective
    /// operation on `model` (upstream `updateInternalData()` called by a
    /// user program).
    ///
    /// # Errors
    /// See [`NtnBaseContact::update_internal_data_in`].
    pub fn update_internal_data_of(
        &mut self,
        model: &mut akantu_solid::SolidMechanicsModel,
    ) -> Result<()> {
        let physics = &mut model.physics;
        model
            .solver
            .dof_manager
            .collective("ntn:update_internal_data", |dm| {
                self.update_internal_data_in(physics, dm)
            })
    }

    /// Upstream `updateNormals()` on a model inside a running collective
    /// operation of `dofs`: a node-to-node contact after
    /// [`NtnBaseContact::init_parallel`] first exchanges `_cf_nodal`
    /// (displacement, current position, velocity at the nodes of the
    /// filtered elements), as upstream's `updateNormals` starts with
    /// `synchronize(_cf_nodal)` (`ntn_contact.cc:239-240`),
    /// then computes the normals ([`NtnBaseContact::update_normals`]).
    /// Before `init_parallel` (no synchronizer: upstream's exchange does
    /// nothing) and on one rank nothing is exchanged.
    ///
    /// # Errors
    /// Exchange failure (agreed by the enclosing operation), FE failure or
    /// inconsistent displacement.
    pub fn update_normals_in(
        &mut self,
        physics: &mut akantu_solid::model::SolidMechanicsPhysics,
        dofs: &mut akantu_solver::DofManager,
    ) -> Result<()> {
        if self.is_ntn_contact() && self.synchronizer.is_some() {
            self.synchronize_nodal_in(
                akantu_core::enums::SynchronizationTag::CfNodal,
                physics,
                dofs,
            )?;
        }
        let mesh_h = physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let state = ModelState::of_parts(&mesh, physics, dofs)?;
        self.compute_normals(&state)
    }

    /// [`NtnBaseContact::update_normals_in`] as its own collective
    /// operation on `model` (upstream `updateNormals()` called by a user
    /// program).
    ///
    /// # Errors
    /// See [`NtnBaseContact::update_normals_in`].
    pub fn update_normals_of(
        &mut self,
        model: &mut akantu_solid::SolidMechanicsModel,
    ) -> Result<()> {
        let physics = &mut model.physics;
        model
            .solver
            .dof_manager
            .collective("ntn:update_normals", |dm| {
                self.update_normals_in(physics, dm)
            })
    }

    /// Upstream `updateNormals()` from a read-only state. Node-to-node: the
    /// normalised sum of the quadrature-point normals (current positions)
    /// of the master boundary elements around each master node (upstream
    /// notes this is meant for elements with one quadrature point). Rigid
    /// flat: the plane normal.
    ///
    /// A node-to-node contact after [`NtnBaseContact::init_parallel`] on a
    /// mesh distributed over more than one rank is refused: upstream's
    /// `updateNormals` exchanges `_cf_nodal` first, which a read-only state
    /// cannot do; use [`NtnBaseContact::update_normals_of`] /
    /// [`NtnBaseContact::update_normals_in`] (the same outcome
    /// on every rank).
    ///
    /// # Errors
    /// The refusal above, FE failure or inconsistent displacement.
    pub fn update_normals(&mut self, state: &ModelState<'_>) -> Result<()> {
        if self.is_ntn_contact() && self.exchanging {
            return Err(Error::Unsupported(
                "update_normals(&ModelState) on a distributed mesh after init_parallel(): \
                 upstream's updateNormals exchanges _cf_nodal first, which a read-only state \
                 cannot do; use NtnBaseContact::update_normals_of (or update_normals_in inside \
                 a solver callback)"
                    .into(),
            ));
        }
        self.compute_normals(state)
    }

    /// The normals of [`NtnBaseContact::update_normals`] from `state`, with
    /// no exchange.
    fn compute_normals(&mut self, state: &ModelState<'_>) -> Result<()> {
        match &self.variant {
            ContactVariant::RigidFlat { normal, .. }
            | ContactVariant::ModeIIAntisymmetric { normal, .. } => {
                let normal = normal.clone();
                return self.normals.set_tuples(&normal);
            }
            ContactVariant::NodeToNode { .. } => {}
        }
        let dim = self.spatial_dimension;
        self.normals.zero();
        let mesh = state.mesh;
        let cur_pos = state.current_position()?;
        let cur_view = akantu_core::array::ArrayView::new(&cur_pos, dim)?;
        let ContactVariant::NodeToNode {
            masters,
            master_elements,
            ..
        } = &self.variant
        else {
            return Ok(());
        };
        let index = first_index_map(masters.as_slice());
        let normals = self.normals.as_mut_slice();
        let mut quad_normals = Array::new(dim);
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = mesh
                .element_types(TypeFilter::new().dim(dim - 1).ghost(ghost))
                .collect();
            for ty in types {
                let elements = master_elements.of(ty, ghost);
                if mesh.nb_element(ty, ghost) == 0 {
                    continue;
                }
                self.boundary_fe.compute_normals_for(
                    mesh,
                    cur_view,
                    mesh,
                    &mut quad_normals,
                    ty,
                    ghost,
                )?;
                let nq = self.boundary_fe.nb_integration_points(ty, ghost)?;
                let conn = mesh.connectivity(ty, ghost)?;
                for &element in elements {
                    let e = ux(element)?;
                    for &node in conn.get_tuple(e)? {
                        let ni = *index.get(&node).ok_or_else(|| {
                            Error::invalid(format!("Could not find node {node} in the array!"))
                        })?;
                        for q in 0..nq {
                            let qn = quad_normals.get_tuple(e * nq + q)?;
                            for d in 0..dim {
                                normals[ni * dim + d] += qn[d];
                            }
                        }
                    }
                }
            }
        }
        for n in normals.chunks_exact_mut(dim) {
            normalize(n);
        }
        Ok(())
    }

    /// Upstream `updateLumpedBoundary()`: per slave (and master) node, the
    /// sum over its boundary elements of `∫ N_node` (boundary FE engine).
    ///
    /// # Errors
    /// FE failure, or a boundary element node that is not a contact node.
    pub fn update_lumped_boundary(&mut self, mesh: &Mesh) -> Result<()> {
        if self.contact_surfaces.is_empty() {
            log::warn!(
                "No surfaces in ntn base contact. You have to define the lumped boundary by yourself."
            );
        }
        let slave_nodes = self.slaves.as_slice().to_vec();
        Self::internal_update_lumped_boundary(
            mesh,
            &self.boundary_fe,
            self.spatial_dimension,
            &slave_nodes,
            &self.slave_elements,
            &mut self.lumped_boundary_slaves,
        )?;
        if let ContactVariant::NodeToNode {
            masters,
            lumped_boundary_masters,
            master_elements,
        } = &mut self.variant
        {
            let master_nodes = masters.as_slice().to_vec();
            Self::internal_update_lumped_boundary(
                mesh,
                &self.boundary_fe,
                self.spatial_dimension,
                &master_nodes,
                master_elements,
                lumped_boundary_masters,
            )?;
        }
        Ok(())
    }

    fn internal_update_lumped_boundary(
        mesh: &Mesh,
        fe: &FeEngine,
        dim: usize,
        nodes: &[Idx],
        elements: &ElementBlocks,
        boundary: &mut SynchronizedArray<Real>,
    ) -> Result<()> {
        boundary.zero();
        let index = first_index_map(nodes);
        let values = boundary.as_mut_slice();
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = mesh
                .element_types(TypeFilter::new().dim(dim - 1).ghost(ghost))
                .collect();
            for ty in types {
                if mesh.nb_element(ty, ghost) == 0 {
                    continue;
                }
                let nnpe = ty.nb_nodes_per_element();
                let conn = mesh.connectivity(ty, ghost)?;
                let shapes = fe.shapes(ty, ghost)?;
                let mut area = Array::new(nnpe);
                fe.integrate(shapes.view(), &mut area, nnpe, ty, ghost, None)?;
                for &elem in elements.of(ty, ghost) {
                    let e = ux(elem)?;
                    let nodes_e = conn.get_tuple(e)?;
                    let a = area.get_tuple(e)?;
                    for q in 0..nnpe {
                        let node = nodes_e[q];
                        let ni = *index.get(&node).ok_or_else(|| {
                            Error::invalid(format!("Could not find node {node} in the array!"))
                        })?;
                        values[ni] += a[q];
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `updateImpedance()`. Node-to-node:
    /// `Z = 2/Δt / (A_m/m_m + A_s/m_s)`; rigid flat: `Z = 2/Δt / (A_s/m_s)`;
    /// mode-II antisymmetric: half the rigid-flat value (`m` = first
    /// component of the lumped mass). Upstream's time-step check
    /// (`delta_t != NAN`) is always true, so a zero time step gives infinite
    /// impedances, as upstream.
    ///
    /// # Errors
    /// The lumped mass is not assembled.
    pub fn update_impedance(&mut self, state: &ModelState<'_>) -> Result<()> {
        let delta_t = state.time_step;
        let dim = state.spatial_dimension;
        state.check_nodal(
            "updateImpedance: lumped mass (assemble it first)",
            &state.mass,
        )?;
        let mass = |n: Idx| -> Result<Real> { Ok(state.mass[ux(n)? * dim]) };
        let nb = self.slaves.len();
        match &self.variant {
            ContactVariant::NodeToNode {
                masters,
                lumped_boundary_masters,
                ..
            } => {
                for i in 0..nb {
                    let m = masters.at(i);
                    let s = self.slaves.at(i);
                    let lbm = lumped_boundary_masters.at(i);
                    let lbs = self.lumped_boundary_slaves.at(i);
                    let imp = 2.0 / delta_t / (lbm / mass(m)? + lbs / mass(s)?);
                    self.impedance.set_at(i, imp);
                }
            }
            ContactVariant::RigidFlat { .. } | ContactVariant::ModeIIAntisymmetric { .. } => {
                let half = matches!(self.variant, ContactVariant::ModeIIAntisymmetric { .. });
                for i in 0..nb {
                    let slave = self.slaves.at(i);
                    let mut imp = self.lumped_boundary_slaves.at(i) / mass(slave)?;
                    imp = 2.0 / delta_t / imp;
                    self.impedance.set_at(i, imp);
                }
                if half {
                    for v in self.impedance.as_mut_slice() {
                        *v *= 0.5;
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `NTRFContact::setReferencePoint(x, y, z)` (the first `dim`
    /// coordinates are used).
    ///
    /// # Errors
    /// Not a rigid-flat contact.
    pub fn set_reference_point(&mut self, x: Real, y: Real, z: Real) -> Result<()> {
        let dim = self.spatial_dimension;
        match &mut self.variant {
            ContactVariant::RigidFlat {
                reference_point, ..
            }
            | ContactVariant::ModeIIAntisymmetric {
                reference_point, ..
            } => {
                let c = [x, y, z];
                reference_point.copy_from_slice(&c[..dim]);
                Ok(())
            }
            ContactVariant::NodeToNode { .. } => Err(Error::invalid(
                "setReferencePoint: only a rigid-flat contact has a reference point",
            )),
        }
    }

    /// Upstream `NTRFContact::setNormal(x, y, z)`: normalised, then copied to
    /// every contact node.
    ///
    /// # Errors
    /// Not a rigid-flat contact.
    pub fn set_normal(&mut self, x: Real, y: Real, z: Real) -> Result<()> {
        let dim = self.spatial_dimension;
        let n = match &mut self.variant {
            ContactVariant::RigidFlat { normal, .. }
            | ContactVariant::ModeIIAntisymmetric { normal, .. } => {
                let c = [x, y, z];
                normal.copy_from_slice(&c[..dim]);
                normalize(normal);
                normal.clone()
            }
            ContactVariant::NodeToNode { .. } => {
                return Err(Error::invalid(
                    "setNormal: only a rigid-flat contact has a plane normal",
                ));
            }
        };
        self.normals.set_tuples(&n)
    }

    /// Reference point of the rigid plane (rigid-flat variants).
    #[must_use]
    pub fn reference_point(&self) -> Option<&[Real]> {
        match &self.variant {
            ContactVariant::RigidFlat {
                reference_point, ..
            }
            | ContactVariant::ModeIIAntisymmetric {
                reference_point, ..
            } => Some(reference_point),
            ContactVariant::NodeToNode { .. } => None,
        }
    }

    /// Normal of the rigid plane (rigid-flat variants).
    #[must_use]
    pub fn plane_normal(&self) -> Option<&[Real]> {
        match &self.variant {
            ContactVariant::RigidFlat { normal, .. }
            | ContactVariant::ModeIIAntisymmetric { normal, .. } => Some(normal),
            ContactVariant::NodeToNode { .. } => None,
        }
    }

    // ---- relative fields -----------------------------------------------

    fn check_field(&self, field: &[Real], mesh_nodes: usize) -> Result<()> {
        check_len(
            "NTN relative field (nb_nodes x dim)",
            field,
            mesh_nodes * self.spatial_dimension,
        )?;
        self.check_nodes(mesh_nodes)
    }

    /// Every contact node (slave and master) is a node of the mesh: a node
    /// added with [`Self::add_split_node`] outside `0..nb_nodes` is an error
    /// of the computations that read it (upstream: out-of-range access),
    /// never a panic.
    fn check_nodes(&self, mesh_nodes: usize) -> Result<()> {
        let bad = |n: Idx| usize::try_from(n).map_or(true, |k| k >= mesh_nodes);
        let masters = match &self.variant {
            ContactVariant::NodeToNode { masters, .. } => masters.as_slice(),
            _ => &[],
        };
        if let Some(&n) = self
            .slaves
            .as_slice()
            .iter()
            .chain(masters.iter())
            .find(|&&n| bad(n))
        {
            return Err(Error::InvalidIndex {
                what: "contact node",
                value: i64::from(n),
            });
        }
        Ok(())
    }

    /// Upstream `computeRelativeNormalField(field)`: per contact node the
    /// normal component of the relative field (slave − master for
    /// node-to-node, the absolute field for rigid flat, doubled for the
    /// mode-II antisymmetric variant). `field` has `nb_nodes × dim` values.
    ///
    /// # Errors
    /// Wrong field length or invalid node.
    pub fn compute_relative_normal_field(
        &self,
        field: &[Real],
        nb_nodes: usize,
    ) -> Result<Vec<Real>> {
        self.check_field(field, nb_nodes)?;
        let dim = self.spatial_dimension;
        let nb = self.slaves.len();
        let mut out = Vec::with_capacity(nb);
        let normals = self.normals.as_slice();
        match &self.variant {
            ContactVariant::NodeToNode { masters, .. } => {
                let mut rfv = [0.0; 3];
                for n in 0..nb {
                    let s = ux(self.slaves.at(n))?;
                    let m = ux(masters.at(n))?;
                    for d in 0..dim {
                        rfv[d] = field[s * dim + d] - field[m * dim + d];
                    }
                    out.push(dot(&rfv[..dim], &normals[n * dim..(n + 1) * dim]));
                }
            }
            ContactVariant::RigidFlat { .. } | ContactVariant::ModeIIAntisymmetric { .. } => {
                let twice = matches!(self.variant, ContactVariant::ModeIIAntisymmetric { .. });
                for n in 0..nb {
                    let node = ux(self.slaves.at(n))?;
                    let v = dot(
                        &field[node * dim..(node + 1) * dim],
                        &normals[n * dim..(n + 1) * dim],
                    );
                    out.push(v);
                }
                if twice {
                    for v in &mut out {
                        *v *= 2.0;
                    }
                }
            }
        }
        Ok(out)
    }

    /// Upstream `computeRelativeTangentialField(field)`: per contact node the
    /// relative field minus its normal projection (`nb_contact × dim`
    /// values; doubled for the mode-II antisymmetric variant).
    ///
    /// # Errors
    /// Wrong field length or invalid node.
    pub fn compute_relative_tangential_field(
        &self,
        field: &[Real],
        nb_nodes: usize,
    ) -> Result<Vec<Real>> {
        self.check_field(field, nb_nodes)?;
        let dim = self.spatial_dimension;
        let nb = self.slaves.len();
        let mut out = vec![0.0; nb * dim];
        let normals = self.normals.as_slice();
        match &self.variant {
            ContactVariant::NodeToNode { masters, .. } => {
                let mut rfv = [0.0; 3];
                for n in 0..nb {
                    let s = ux(self.slaves.at(n))?;
                    let m = ux(masters.at(n))?;
                    let nv = &normals[n * dim..(n + 1) * dim];
                    for d in 0..dim {
                        rfv[d] = field[s * dim + d] - field[m * dim + d];
                    }
                    let dn = dot(&rfv[..dim], nv);
                    for d in 0..dim {
                        out[n * dim + d] = rfv[d] - nv[d] * dn;
                    }
                }
            }
            ContactVariant::RigidFlat { .. } | ContactVariant::ModeIIAntisymmetric { .. } => {
                let twice = matches!(self.variant, ContactVariant::ModeIIAntisymmetric { .. });
                for n in 0..nb {
                    let node = ux(self.slaves.at(n))?;
                    let nv = &normals[n * dim..(n + 1) * dim];
                    let f = &field[node * dim..(node + 1) * dim];
                    let dn = dot(f, nv);
                    for d in 0..dim {
                        out[n * dim + d] = f[d] - nv[d] * dn;
                    }
                }
                if twice {
                    for v in &mut out {
                        *v *= 2.0;
                    }
                }
            }
        }
        Ok(out)
    }

    /// Upstream `computeNormalGap()`: node-to-node, the relative normal field
    /// of the current positions; rigid flat (both variants), `(x − x_ref)·n`
    /// (not doubled for the mode-II antisymmetric variant, as upstream).
    ///
    /// # Errors
    /// Inconsistent displacement or invalid node.
    pub fn compute_normal_gap(&self, state: &ModelState<'_>) -> Result<Vec<Real>> {
        let cur_pos = state.current_position()?;
        let nb_nodes = state.nb_nodes();
        match &self.variant {
            ContactVariant::NodeToNode { .. } => {
                self.compute_relative_normal_field(&cur_pos, nb_nodes)
            }
            ContactVariant::RigidFlat {
                reference_point, ..
            }
            | ContactVariant::ModeIIAntisymmetric {
                reference_point, ..
            } => {
                self.check_field(&cur_pos, nb_nodes)?;
                let dim = self.spatial_dimension;
                let normals = self.normals.as_slice();
                let mut gap_v = [0.0; 3];
                let mut out = Vec::with_capacity(self.slaves.len());
                for n in 0..self.slaves.len() {
                    let node = ux(self.slaves.at(n))?;
                    for d in 0..dim {
                        gap_v[d] = cur_pos[node * dim + d] - reference_point[d];
                    }
                    out.push(dot(&gap_v[..dim], &normals[n * dim..(n + 1) * dim]));
                }
                Ok(out)
            }
        }
    }

    /// Upstream `computeAcceleration(acceleration)`: the free acceleration
    /// `f_m2a (f_int + f_ext) / m` per DOF (blocked DOFs not considered).
    ///
    /// # Errors
    /// Forces or mass not allocated.
    pub fn compute_acceleration(state: &ModelState<'_>) -> Result<Vec<Real>> {
        state.check_nodal("computeAcceleration: internal force", &state.internal_force)?;
        state.check_nodal("computeAcceleration: external force", &state.external_force)?;
        state.check_nodal("computeAcceleration: lumped mass", &state.mass)?;
        let f_m2a = state.f_m2a;
        Ok(state
            .internal_force
            .iter()
            .zip(state.external_force.iter())
            .zip(state.mass.iter())
            .map(|((fi, fe), m)| f_m2a * (fi + fe) / m)
            .collect())
    }

    // ---- contact pressure ----------------------------------------------

    /// Upstream `computeContactPressure()` (a no-op for the mode-II
    /// antisymmetric variant, see
    /// [`Self::compute_contact_pressure_in_equilibrium`]).
    ///
    /// # Errors
    /// Non-positive time step (upstream assertion), missing fields.
    pub fn compute_contact_pressure(&mut self, state: &ModelState<'_>) -> Result<()> {
        if self.kind() == ContactKind::ModeIIAntisymmetric {
            return Ok(());
        }
        self.base_compute_contact_pressure(state)
    }

    /// Upstream `MIIASYMContact::computeContactPressureInEquilibrium()`: the
    /// rigid-flat pressure computation with the doubled relative fields.
    ///
    /// # Errors
    /// Not a mode-II antisymmetric contact, or see
    /// [`Self::compute_contact_pressure`].
    pub fn compute_contact_pressure_in_equilibrium(
        &mut self,
        state: &ModelState<'_>,
    ) -> Result<()> {
        if self.kind() != ContactKind::ModeIIAntisymmetric {
            return Err(Error::invalid(
                "computeContactPressureInEquilibrium: only a MIIASYM contact",
            ));
        }
        self.base_compute_contact_pressure(state)
    }

    fn base_compute_contact_pressure(&mut self, state: &ModelState<'_>) -> Result<()> {
        // UD-904 (category A): upstream's default path reads stale ghost data
        self.refuse_without_init_parallel(state.mesh)?;
        let dim = self.spatial_dimension;
        let delta_t = state.time_step;
        if delta_t.is_nan() || delta_t <= 0.0 {
            return Err(Error::invalid(
                "Cannot compute contact pressure if no time step is set",
            ));
        }
        let nb_nodes = state.nb_nodes();
        state.check_nodal("computeContactPressure: velocity", &state.velocity)?;
        state.check_nodal("computeContactPressure: acceleration", &state.acceleration)?;
        let acceleration = Self::compute_acceleration(state)?;
        let r_disp = self.compute_normal_gap(state)?;
        let r_velo = self.compute_relative_normal_field(&state.velocity, nb_nodes)?;
        let r_acce = self.compute_relative_normal_field(&acceleration, nb_nodes)?;
        let r_old_acce = self.compute_relative_normal_field(&state.acceleration, nb_nodes)?;
        let nb = self.slaves.len();
        for n in 0..nb {
            let g = r_disp[n] + delta_t * r_velo[n] + delta_t * delta_t * r_acce[n]
                - delta_t * delta_t * r_old_acce[n] / 2.0;
            let in_contact = g <= 0.0;
            self.is_in_contact.set_at(n, in_contact);
            let imp = self.impedance.at(n);
            let normal: [Real; 3] = {
                let mut a = [0.0; 3];
                a[..dim].copy_from_slice(self.normals.tuple(n));
                a
            };
            let cp = self.contact_pressure.tuple_mut(n);
            if in_contact {
                let s = imp * g / (2.0 * delta_t);
                for d in 0..dim {
                    cp[d] = s * normal[d];
                }
            } else {
                cp.fill(0.0);
            }
        }
        Ok(())
    }

    /// Upstream `assembleGlobalContactPressure()`: node-to-node, master
    /// `A_m p`, slave `−A_s p` (assigned, not accumulated; then made
    /// consistent for periodicity); rigid flat, slave `−A_s p` only.
    ///
    /// # Errors
    /// Invalid node or wrong global array size.
    pub fn assemble_global_contact_pressure(&mut self, mesh: &Mesh) -> Result<()> {
        let dim = self.spatial_dimension;
        check_len(
            "assembleGlobalContactPressure: global array",
            self.global_contact_pressure.as_slice(),
            mesh.nb_nodes() * dim,
        )?;
        self.global_contact_pressure.zero();
        let nb = self.slaves.len();
        let gcp = self.global_contact_pressure.as_mut_slice();
        match &self.variant {
            ContactVariant::NodeToNode {
                masters,
                lumped_boundary_masters,
                ..
            } => {
                for n in 0..nb {
                    let m = ux(masters.at(n))?;
                    let s = ux(self.slaves.at(n))?;
                    let lbm = lumped_boundary_masters.at(n);
                    let lbs = self.lumped_boundary_slaves.at(n);
                    let cp = self.contact_pressure.tuple(n);
                    for d in 0..dim {
                        gcp[m * dim + d] = lbm * cp[d];
                    }
                    for d in 0..dim {
                        gcp[s * dim + d] = -lbs * cp[d];
                    }
                }
                make_consistent_for_periodicity(
                    mesh,
                    &mut self.global_contact_pressure,
                    ReduceOperation::Id,
                    |a, _| a,
                )?;
            }
            ContactVariant::RigidFlat { .. } | ContactVariant::ModeIIAntisymmetric { .. } => {
                for n in 0..nb {
                    let s = ux(self.slaves.at(n))?;
                    let lbs = self.lumped_boundary_slaves.at(n);
                    let cp = self.contact_pressure.tuple(n);
                    for d in 0..dim {
                        gcp[s * dim + d] = -lbs * cp[d];
                    }
                }
            }
        }
        Ok(())
    }

    /// Upstream `savePreviousState()`: copy of the global contact force.
    pub fn save_previous_state(&mut self) {
        self.global_contact_pressure_old = self.global_contact_pressure.clone();
    }

    /// Upstream `restorePreviousState()`.
    pub fn restore_previous_state(&mut self) {
        self.global_contact_pressure = self.global_contact_pressure_old.clone();
    }

    // ---- restart ---------------------------------------------------------

    /// Upstream `dumpRestart(file_name)`: one restart file per array.
    ///
    /// # Errors
    /// Unsynchronised arrays or I/O failure.
    pub fn dump_restart(&self, file_name: &str) -> Result<()> {
        self.slaves.dump_restart_file(file_name)?;
        self.normals.dump_restart_file(file_name)?;
        self.is_in_contact.dump_restart_file(file_name)?;
        self.contact_pressure.dump_restart_file(file_name)?;
        self.lumped_boundary_slaves.dump_restart_file(file_name)?;
        self.impedance.dump_restart_file(file_name)?;
        if let ContactVariant::NodeToNode {
            masters,
            lumped_boundary_masters,
            ..
        } = &self.variant
        {
            masters.dump_restart_file(file_name)?;
            lumped_boundary_masters.dump_restart_file(file_name)?;
        }
        Ok(())
    }

    /// Upstream `readRestart(file_name)` (UD-394: values restored unless
    /// [`TasnUpstreamCompat::restart_read_ignores_values`]).
    ///
    /// # Errors
    /// Unsynchronised arrays, unreadable or inconsistent files.
    pub fn read_restart(&mut self, file_name: &str) -> Result<()> {
        let ig = self.compat.restart_read_ignores_values;
        self.slaves.read_restart_file(file_name, ig)?;
        self.normals.read_restart_file(file_name, ig)?;
        self.is_in_contact.read_restart_file(file_name, ig)?;
        self.contact_pressure.read_restart_file(file_name, ig)?;
        self.lumped_boundary_slaves
            .read_restart_file(file_name, ig)?;
        self.impedance.read_restart_file(file_name, ig)?;
        if let ContactVariant::NodeToNode {
            masters,
            lumped_boundary_masters,
            ..
        } = &mut self.variant
        {
            masters.read_restart_file(file_name, ig)?;
            lumped_boundary_masters.read_restart_file(file_name, ig)?;
        }
        Ok(())
    }

    /// Nodal arrays of the contact by dump-field name (upstream
    /// `addDumpFieldToDumper` names of `NTNBaseContact` / `NTNContact`):
    /// `normal`, `contact_pressure`, `lumped_boundary_slave`, `impedance`,
    /// `lumped_boundary_master` (per contact node, `nb_component` values).
    #[must_use]
    pub fn contact_field(&self, field_id: &str) -> Option<(&[Real], usize)> {
        match field_id {
            "normal" => Some((self.normals.as_slice(), self.spatial_dimension)),
            "contact_pressure" => Some((self.contact_pressure.as_slice(), self.spatial_dimension)),
            "lumped_boundary_slave" => Some((self.lumped_boundary_slaves.as_slice(), 1)),
            "impedance" => Some((self.impedance.as_slice(), 1)),
            "lumped_boundary_master" => self.lumped_boundary_masters().map(|a| (a.as_slice(), 1)),
            _ => None,
        }
    }
}

impl std::fmt::Display for NtnBaseContact {
    /// Upstream `printself` (simplified layout).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.kind() {
            ContactKind::NodeToNode => "NTNContact",
            ContactKind::RigidFlat => "NTRFContact",
            ContactKind::ModeIIAntisymmetric => "MIIASYMContact",
        };
        writeln!(f, "{name} [")?;
        writeln!(f, " + id            : {}", self.id)?;
        writeln!(f, " + slaves        : {:?}", self.slaves.as_slice())?;
        writeln!(f, " + normals       : {:?}", self.normals.as_slice())?;
        writeln!(
            f,
            " + contact_pressure : {:?}",
            self.contact_pressure.as_slice()
        )?;
        if let Some(m) = self.masters() {
            writeln!(f, " + masters       : {:?}", m.as_slice())?;
        }
        writeln!(f, "]")
    }
}
