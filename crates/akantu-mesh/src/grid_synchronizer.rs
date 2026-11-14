// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/synchronizer/grid_synchronizer.hh,
// grid_synchronizer.cc, grid_synchronizer_tmpl.hh; src/common/aka_bbox.hh
// (setLowerBounds/setUpperBounds, intersects, intersection, allGather).

//! Grid synchronizer (upstream `GridSynchronizer`, MPI-021).
//!
//! Builds an **additional** ghost layer from a spatial grid of the local
//! elements (non-local neighbourhoods, upstream `NeighborhoodBase`): every
//! rank whose grid box, grown by one cell spacing on each side, intersects
//! this rank's grown box receives the local elements stored in the cells
//! of the intersection, with their global connectivities; unknown nodes
//! are requested from the sender and created as pure ghosts, unknown
//! elements as ghost elements. The returned synchronizer's schemes are:
//! send toward `p` = the local elements of those cells (set order, upstream
//! `std::set<Element>`); receive from `p` = the ghost elements received
//! from `p` (existing ghosts found by connectivity, new ones appended), in
//! message order.
//!
//! # Protocol (ARCHITECTURE.md, "Agreement across ranks")
//!
//! [`plan_grid_synchronizer_in`] reads the mesh and the grid and computes
//! everything that must change, in phases of the caller's scope:
//! `grid:boxes` (all-gather of the `3·dim` box values of every rank, a
//! bounded payload, ARCHITECTURE.md, "Agreement across ranks"), `grid:conn` (point-to-point connectivities to
//! every intersecting rank — the relation is symmetric, so every rank
//! knows whom it receives from; local post-processing agreed),
//! `grid:ask` (requested global node ids) and `grid:coords` (their
//! coordinates and owner ranks). The mesh is changed only afterwards, by
//! [`apply_grid_plan`] (set-up, no consumers) or inside a mesh mutation
//! ([`create_grid_synchronizer_shared`]), so a rank-specific failure leaves
//! every mesh unchanged.
//!
//! # Deviations (`docs/UPSTREAM_DEFECTS.md`)
//!
//! * UD-252 (B): upstream records no owner rank for the new pure ghost
//!   nodes (`getNodePrank` = −1, unlike every other pure ghost); the port
//!   sends the owner with the coordinates. `GridSynchronizerOptions::
//!   upstream_node_pranks` reproduces upstream.
//! * UD-253 (A): upstream reads the connectivity of a sent element from
//!   the not-ghost block whatever its ghost type (a ghost element in the
//!   grid sends another element's nodes, or reads out of bounds); refused
//!   here.
//! * Upstream leaves the mesh data of the new ghost elements short; the
//!   port extends the adjacency datasets (`element_to_subelement`,
//!   `subelement_to_element`, `mesh_to_mesh_facet`, parent and facet mesh)
//!   with empty / NULL rows (ARCHITECTURE adjacency contract) and
//!   mirrors the new nodes into the facet mesh (shared node set).
//!
//! # Bounding boxes (UD-255 withdrawn)
//!
//! Upstream's `mesh.sendEvent(new_nodes)` resolves to the specialisation
//! `Mesh::sendEvent<NewNodesEvent>` (`mesh.cc:114-116`), whose first
//! statement is `computeBoundingBox()`: the local box over **every** node,
//! the new pure ghosts included, then `bbox_local.allSum(comm)` (a
//! collective) for the global box — on every rank, after the coordinates
//! were received and before the node synchronizer reacts to the event.
//! [`plan_grid_synchronizer_in`] computes both boxes in that position (phase
//! `grid:bbox`, Min/Max reduction, exact) and [`apply_grid_plan`] installs
//! them; the mutation path ([`create_grid_synchronizer_shared`]) installs
//! the same boxes by stage 3 of `SharedMesh::mutate`. The global box does
//! not change (the new nodes are copies of other ranks' nodes); the local
//! one grows by the new ghost layer. The facet mesh's box is not touched by
//! the set-up path (upstream's facet mesh does not receive the event); the
//! mutation's stage 3 recomputes it (mutation contract).
//!
//! # Grid
//!
//! The grid is [`akantu_core::spatial_grid::SpatialGrid`] (D-048: the
//! port's only grid), which implements [`GridCells`] here (trait crate).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use akantu_comm::phase::Phases;
use akantu_comm::{Communications, Communicator, Synchronizer, Tag};
use akantu_core::{
    Element, ElementType, Error, GhostType, GlobalIdx, Idx, NodeFlag, Rank, Real, Result, try_idx,
    try_to_usize,
};

use akantu_core::bbox::BBox;
use akantu_core::spatial_grid::{CellId, SpatialGrid};

use crate::events::{MeshEvent, MeshEvents};
use crate::mesh::Mesh;
use crate::shared::{ConsumerToken, MeshPart, SharedMesh};
use crate::synchronizers::ElementSynchronizer;
use crate::upstream_bbox::FlaggedBox;

/// What the grid synchronizer needs from a spatial grid (upstream
/// `SpatialGrid<E>` accessors and its `friend` access to `getCellID`;
/// implemented by [`akantu_core::spatial_grid::SpatialGrid`]`<Element>`,
/// D-048).
pub trait GridCells {
    /// Spatial dimension.
    fn dimension(&self) -> usize;
    /// Lower corner of the non-empty cells (`+Real::MAX` if none).
    fn lower_bounds(&self) -> &[Real];
    /// Upper corner of the non-empty cells (`−Real::MAX` if none).
    fn upper_bounds(&self) -> &[Real];
    /// Cell spacing per direction.
    fn spacing(&self) -> &[Real];
    /// Cell index of `position` in `direction` (upstream `getCellID(Real,
    /// Int)`).
    ///
    /// # Errors
    /// A direction outside the grid, or a position whose cell index is not
    /// finite or not representable (upstream: undefined conversion, UD-270).
    fn cell_index(&self, position: Real, direction: usize) -> Result<Idx>;
    /// Visit the elements of every non-empty cell whose indices lie in the
    /// closed range `[first, last]` (any order; duplicates allowed).
    ///
    /// # Errors
    /// Ranges of another dimension than the grid's, or indices that are not
    /// cell indices of the grid.
    fn elements_in_cell_range(
        &self,
        first: &[Idx],
        last: &[Idx],
        visit: &mut dyn FnMut(Element),
    ) -> Result<()>;
}

/// D-048: the core grid is the grid synchronizer's grid (the trait lives
/// here, so does the impl: orphan rule). Upstream visits the cells of the
/// range with nested loops and `getCell`, which is
/// [`SpatialGrid::cells_in_range`]'s sequence.
impl GridCells for SpatialGrid<Element> {
    fn dimension(&self) -> usize {
        SpatialGrid::dimension(self)
    }

    fn lower_bounds(&self) -> &[Real] {
        SpatialGrid::lower_bounds(self)
    }

    fn upper_bounds(&self) -> &[Real] {
        SpatialGrid::upper_bounds(self)
    }

    fn spacing(&self) -> &[Real] {
        SpatialGrid::spacing(self)
    }

    fn cell_index(&self, position: Real, direction: usize) -> Result<Idx> {
        SpatialGrid::cell_index(self, position, direction)
    }

    fn elements_in_cell_range(
        &self,
        first: &[Idx],
        last: &[Idx],
        visit: &mut dyn FnMut(Element),
    ) -> Result<()> {
        let first = CellId::new(first.to_vec())?;
        let last = CellId::new(last.to_vec())?;
        for (_, items) in self.cells_in_range(&first, &last)? {
            for &e in items {
                visit(e);
            }
        }
        Ok(())
    }
}

/// Options of the grid synchronizer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GridSynchronizerOptions {
    /// UD-252 compatibility: leave the owner rank of the new pure ghost
    /// nodes unset (upstream). Default: the owner sent by the element's
    /// rank.
    pub upstream_node_pranks: bool,
}

/// A pure ghost node created by the grid synchronizer.
#[derive(Clone, Debug, PartialEq)]
pub struct GridGhostNode {
    /// Global id.
    pub global_id: GlobalIdx,
    /// Coordinates.
    pub position: Vec<Real>,
    /// Owner rank (`None` with [`GridSynchronizerOptions::upstream_node_pranks`]).
    pub owner: Option<Rank>,
}

/// Everything [`plan_grid_synchronizer_in`] decided (no mesh change yet).
#[derive(Clone, Debug)]
pub struct GridGhostPlan {
    /// Revision of the mesh the plan was computed from.
    pub mesh_revision: u64,
    /// New pure ghost nodes, appended in this order (local ids from the
    /// current node count on).
    pub new_nodes: Vec<GridGhostNode>,
    /// New ghost elements, appended in this order (types in any order;
    /// each element's local index is the block length at its turn).
    pub new_elements: Vec<(ElementType, Vec<Idx>)>,
    /// Ghost counter of every received ghost element, final values per
    /// `(type, ghost index)` (existing ones incremented, new ones 1 plus
    /// later repetitions).
    pub counters: BTreeMap<Element, Idx>,
    /// Schemes of the new synchronizer.
    pub schemes: Communications<Element>,
    /// Bounding boxes after the new nodes, `(local, global)` (upstream
    /// `sendEvent<NewNodesEvent>` → `computeBoundingBox`; module
    /// documentation); `None` on one rank (upstream returns before).
    pub bounding_boxes: Option<(BBox, BBox)>,
}

/// Coordinates and owner ranks of the nodes a rank requested.
type NodeReply = (Rank, Vec<Real>, Vec<Rank>);

const CONN_TAG: Tag = Tag(29_200);
const ASK_TAG: Tag = Tag(29_201);
const COORD_TAG: Tag = Tag(29_202);
const OWNER_TAG: Tag = Tag(29_203);

/// Wire code of an element type (its upstream enum value).
fn type_code(ty: ElementType) -> GlobalIdx {
    GlobalIdx::from(ty as u8)
}

fn type_of_code(code: GlobalIdx) -> Result<ElementType> {
    ElementType::ALL
        .iter()
        .copied()
        .find(|&t| type_code(t) == code && t != ElementType::NotDefined)
        .ok_or_else(|| {
            Error::Communication(format!("grid synchronizer: unknown element type {code}"))
        })
}

/// Local part before the box exchange.
fn my_box(mesh: &Mesh, grid: &dyn GridCells) -> Result<FlaggedBox> {
    let dim = mesh.spatial_dimension();
    if grid.dimension() != dim
        || grid.lower_bounds().len() != dim
        || grid.upper_bounds().len() != dim
        || grid.spacing().len() != dim
    {
        return Err(Error::DimensionMismatch {
            context: "grid synchronizer (grid dimension = mesh dimension)",
            expected: dim,
            got: grid.dimension(),
        });
    }
    let sp = grid.spacing();
    Ok(FlaggedBox {
        lower: grid
            .lower_bounds()
            .iter()
            .zip(sp)
            .map(|(l, h)| l - h)
            .collect(),
        upper: grid
            .upper_bounds()
            .iter()
            .zip(sp)
            .map(|(u, h)| u + h)
            .collect(),
        // setLowerBounds / setUpperBounds mark the box non-empty
        empty: false,
    })
}

/// Elements to send toward every intersecting rank and the encoded
/// connectivity messages (`[type code, nb values, global connectivities…]`
/// per type in type order).
struct SendPlan {
    intersecting: Vec<Rank>,
    send: BTreeMap<Rank, Vec<Element>>,
    messages: Vec<(Rank, Vec<GlobalIdx>)>,
}

fn plan_sends(
    mesh: &Mesh,
    grid: &dyn GridCells,
    mine: &FlaggedBox,
    boxes: &[FlaggedBox],
) -> Result<SendPlan> {
    let rank = mesh.communicator().rank();
    let dim = mesh.spatial_dimension();
    let mut intersecting = Vec::new();
    let mut send = BTreeMap::new();
    let mut messages = Vec::new();
    for (p, other) in boxes.iter().enumerate() {
        let p = Rank::try_from(p).map_err(|_| Error::InvalidArgument("rank".into()))?;
        if p == rank {
            continue;
        }
        let Some(inter) = mine.intersection(other) else {
            continue;
        };
        intersecting.push(p);
        let first = (0..dim)
            .map(|s| grid.cell_index(inter.lower[s], s))
            .collect::<Result<Vec<Idx>>>()?;
        let last = (0..dim)
            .map(|s| grid.cell_index(inter.upper[s], s))
            .collect::<Result<Vec<Idx>>>()?;
        let mut to_send: BTreeSet<Element> = BTreeSet::new();
        grid.elements_in_cell_range(&first, &last, &mut |e| {
            to_send.insert(e);
        })?;
        let mut per_type: BTreeMap<ElementType, Vec<GlobalIdx>> = BTreeMap::new();
        for &e in &to_send {
            if e.ghost_type() != GhostType::NotGhost {
                // UD-253: upstream reads the not-ghost element of the same
                // index (`mesh.getConnectivity(type)`)
                return Err(Error::InvalidArgument(format!(
                    "grid synchronizer: the grid holds the ghost element {e} in a cell sent to \
                     rank {p} (only local elements can be sent)"
                )));
            }
            let conn = mesh.element_connectivity(e)?;
            per_type
                .entry(e.ty())
                .or_default()
                .extend(conn.iter().map(|&n| mesh.node_global_id(n)));
        }
        let mut msg = Vec::new();
        for (ty, values) in per_type {
            msg.push(type_code(ty));
            msg.push(GlobalIdx::try_from(values.len()).map_err(|_| {
                Error::InvalidArgument("grid synchronizer: message too long".into())
            })?);
            msg.extend(values);
        }
        send.insert(p, to_send.into_iter().collect());
        messages.push((p, msg));
    }
    Ok(SendPlan {
        intersecting,
        send,
        messages,
    })
}

/// Receive-side processing of the connectivity messages (rank order).
struct RecvPlan {
    recv: BTreeMap<Rank, Vec<Element>>,
    new_nodes: Vec<GlobalIdx>,
    asks: Vec<(Rank, Vec<GlobalIdx>)>,
    new_elements: Vec<(ElementType, Vec<Idx>)>,
    counters: BTreeMap<Element, Idx>,
}

fn process_connectivities(mesh: &Mesh, received: Vec<(Rank, Vec<GlobalIdx>)>) -> Result<RecvPlan> {
    let nb_global = mesh.nb_global_nodes();
    // first local node of every global id (upstream `find`)
    let mut local_of: HashMap<GlobalIdx, Idx> = HashMap::new();
    for n in (0..mesh.nb_nodes()).rev() {
        let n = try_idx(n)?;
        local_of.insert(mesh.node_global_id(n), n);
    }
    let mut next_node = mesh.nb_nodes();
    // ghost connectivities of every type: first index of each tuple
    let mut ghost_index: HashMap<ElementType, HashMap<Vec<Idx>, usize>> = HashMap::new();
    let mut ghost_len: HashMap<ElementType, usize> = HashMap::new();
    let mut plan = RecvPlan {
        recv: BTreeMap::new(),
        new_nodes: Vec::new(),
        asks: Vec::new(),
        new_elements: Vec::new(),
        counters: BTreeMap::new(),
    };
    for (p, msg) in received {
        let scheme = plan.recv.entry(p).or_default();
        let mut ask = Vec::new();
        let mut i = 0usize;
        while i < msg.len() {
            let (Some(&code), Some(&nv)) = (msg.get(i), msg.get(i + 1)) else {
                return Err(Error::Communication(format!(
                    "grid synchronizer: truncated message from rank {p}"
                )));
            };
            let ty = type_of_code(code)?;
            let nnpe = ty.nb_nodes_per_element();
            let nv = usize::try_from(nv)
                .ok()
                .filter(|&v| nnpe > 0 && v % nnpe == 0)
                .ok_or_else(|| {
                    Error::Communication(format!("grid synchronizer: bad size from rank {p}"))
                })?;
            let start = i + 2;
            let end = start
                .checked_add(nv)
                .filter(|&e| e <= msg.len())
                .ok_or_else(|| {
                    Error::Communication(format!("grid synchronizer: short message from rank {p}"))
                })?;
            let index = ghost_index.entry(ty).or_insert_with(|| {
                let mut m = HashMap::new();
                if let Ok(c) = mesh.connectivity(ty, GhostType::Ghost) {
                    for (k, t) in c.tuples().enumerate() {
                        m.entry(t.to_vec()).or_insert(k);
                    }
                }
                m
            });
            let len = ghost_len
                .entry(ty)
                .or_insert_with(|| mesh.nb_element(ty, GhostType::Ghost));
            for gconn in msg[start..end].chunks_exact(nnpe) {
                let mut conn = Vec::with_capacity(nnpe);
                let mut asked = 0usize;
                for &gn in gconn {
                    if usize::try_from(gn).map_or(true, |g| g >= nb_global) {
                        return Err(Error::Communication(format!(
                            "grid synchronizer: global node {gn} from rank {p} outside 0..{nb_global}"
                        )));
                    }
                    let ln = match local_of.get(&gn) {
                        Some(&l) => l,
                        None => {
                            let l = try_idx(next_node)?;
                            next_node += 1;
                            local_of.insert(gn, l);
                            plan.new_nodes.push(gn);
                            ask.push(gn);
                            asked += 1;
                            l
                        }
                    };
                    conn.push(ln);
                }
                let existing = if asked == 0 {
                    index.get(&conn).copied()
                } else {
                    None
                };
                let element = match existing {
                    Some(k) => Element::try_new(ty, k, GhostType::Ghost)?,
                    None => {
                        let k = *len;
                        *len += 1;
                        index.entry(conn.clone()).or_insert(k);
                        plan.new_elements.push((ty, conn));
                        Element::try_new(ty, k, GhostType::Ghost)?
                    }
                };
                let counter = plan.counters.entry(element).or_insert_with(|| {
                    if existing.is_some() {
                        mesh.ghosts_counters()
                            .get(ty, GhostType::Ghost)
                            .ok()
                            .and_then(|c| c.as_slice().get(element.idx()).copied())
                            .unwrap_or(0)
                    } else {
                        0
                    }
                });
                *counter += 1;
                scheme.push(element);
            }
            i = end;
        }
        plan.asks.push((p, ask));
    }
    Ok(plan)
}

/// Upstream `GridSynchronizer::createGridSynchronizer(grid)`
/// (`grid_synchronizer.cc:36-460`) without the mesh changes: phases of the
/// caller's scope `ph` (see the module documentation). One rank: an empty
/// plan (upstream returns before any communication).
pub fn plan_grid_synchronizer_in(
    ph: &mut Phases,
    mesh: &Mesh,
    grid: &dyn GridCells,
    options: GridSynchronizerOptions,
) -> Result<GridGhostPlan> {
    let comm = mesh.communicator().clone();
    let size = comm.size();
    let dim = mesh.spatial_dimension();
    let empty = GridGhostPlan {
        mesh_revision: mesh.revision(),
        new_nodes: Vec::new(),
        new_elements: Vec::new(),
        counters: BTreeMap::new(),
        schemes: Communications::default(),
        bounding_boxes: None,
    };
    if size == 1 {
        ph.check("grid")?;
        return Ok(empty);
    }
    let prepared = (|| -> Result<Vec<Real>> {
        if !mesh.is_distributed() {
            return Err(Error::Mesh(format!(
                "grid synchronizer: mesh '{}' is not distributed",
                mesh.id()
            )));
        }
        let mut v = Vec::with_capacity(3 * dim);
        my_box(mesh, grid)?.pack(&mut v);
        Ok(v)
    })();
    let gathered = ph.run_collective("grid:boxes", prepared, |c, v| c.all_gather(&v))?;
    let boxes = gathered
        .chunks_exact(3 * dim)
        .map(|b| FlaggedBox::unpack(b, dim))
        .collect::<Result<Vec<FlaggedBox>>>()?;
    let mine = my_box(mesh, grid)?;
    // connectivities toward / from every intersecting rank
    let sends = plan_sends(mesh, grid, &mine, &boxes);
    let (intersecting, send_schemes, recv) =
        ph.run("grid:conn", sends, |c: &Communicator, plan| {
            let mut reqs = Vec::with_capacity(plan.messages.len());
            for (p, msg) in plan.messages {
                reqs.push(c.isend(msg, p, CONN_TAG)?);
            }
            let mut received = Vec::with_capacity(plan.intersecting.len());
            for &p in &plan.intersecting {
                received.push((p, c.receive_vec::<GlobalIdx>(p, CONN_TAG)?));
            }
            for r in reqs {
                r.wait()?;
            }
            Ok(process_connectivities(mesh, received)
                .map(|recv| (plan.intersecting, plan.send, recv)))
        })?;
    // node requests (upstream ASK_NODES_TAG; `-1` terminator not sent)
    let asks = recv.asks.clone();
    let replies = ph.run(
        "grid:ask",
        Ok(asks),
        |c: &Communicator, asks: Vec<(Rank, Vec<GlobalIdx>)>| {
            let mut reqs = Vec::with_capacity(asks.len());
            for (p, a) in asks {
                reqs.push(c.isend(a, p, ASK_TAG)?);
            }
            let mut asked = Vec::with_capacity(intersecting.len());
            for &p in &intersecting {
                asked.push((p, c.receive_vec::<GlobalIdx>(p, ASK_TAG)?));
            }
            for r in reqs {
                r.wait()?;
            }
            // replies: coordinates and owner ranks of the asked nodes
            let reply = (|| -> Result<Vec<NodeReply>> {
                let mut local_of: HashMap<GlobalIdx, Idx> = HashMap::new();
                for n in (0..mesh.nb_nodes()).rev() {
                    let n = try_idx(n)?;
                    local_of.insert(mesh.node_global_id(n), n);
                }
                let me = mesh.communicator().rank();
                let mut out = Vec::with_capacity(asked.len());
                for (p, gs) in asked {
                    let mut x = Vec::with_capacity(gs.len() * dim);
                    let mut owners = Vec::with_capacity(gs.len());
                    for g in gs {
                        let l = *local_of.get(&g).ok_or_else(|| {
                            Error::Mesh(format!(
                                "grid synchronizer: the node {g} requested by rank {p} was not \
                                 found locally"
                            ))
                        })?;
                        x.extend_from_slice(mesh.nodes().tuple(try_to_usize(l)?));
                        let f = mesh.node_flag(l);
                        owners.push(if f.is_local_or_master() {
                            me
                        } else {
                            mesh.node_prank(l)
                        });
                    }
                    out.push((p, x, owners));
                }
                Ok(out)
            })();
            Ok(reply)
        },
    )?;
    let expected: BTreeMap<Rank, usize> = recv.asks.iter().map(|(p, a)| (*p, a.len())).collect();
    let (coords, owners) = ph.run(
        "grid:coords",
        Ok(replies),
        |c: &Communicator, replies: Vec<NodeReply>| {
            let mut reqs_x = Vec::with_capacity(replies.len());
            let mut reqs_o = Vec::with_capacity(replies.len());
            for (p, x, o) in replies {
                reqs_x.push(c.isend(x, p, COORD_TAG)?);
                reqs_o.push(c.isend(o, p, OWNER_TAG)?);
            }
            let mut xs = Vec::new();
            let mut os = Vec::new();
            for &p in &intersecting {
                let x = c.receive_vec::<Real>(p, COORD_TAG)?;
                let o = c.receive_vec::<Rank>(p, OWNER_TAG)?;
                xs.push((p, x));
                os.push((p, o));
            }
            for r in reqs_x {
                r.wait()?;
            }
            for r in reqs_o {
                r.wait()?;
            }
            // lengths validated (post-processing)
            for ((p, x), (_, o)) in xs.iter().zip(&os) {
                let n = expected.get(p).copied().unwrap_or(0);
                if x.len() != n * dim || o.len() != n {
                    return Ok(Err(Error::Communication(format!(
                        "grid synchronizer: rank {p} sent {} coordinates / {} owners for {n} nodes",
                        x.len(),
                        o.len()
                    ))));
                }
            }
            Ok(Ok((xs, os)))
        },
    )?;
    // new nodes in creation order (rank order, then request order)
    let mut new_nodes = Vec::with_capacity(recv.new_nodes.len());
    let mut it = recv.new_nodes.iter();
    for ((_, x), (_, o)) in coords.iter().zip(&owners) {
        for (k, owner) in o.iter().enumerate() {
            let g = *it
                .next()
                .ok_or_else(|| Error::Mesh("grid synchronizer: node bookkeeping".into()))?;
            new_nodes.push(GridGhostNode {
                global_id: g,
                position: x[k * dim..(k + 1) * dim].to_vec(),
                owner: (!options.upstream_node_pranks).then_some(*owner),
            });
        }
    }
    // upstream `mesh.sendEvent(new_nodes)` → `computeBoundingBox()`: local
    // box over every node incl. the new pure ghosts, then the collective
    // Min/Max for the global box, before the node synchronizer
    // reacts to the event
    let bounding_boxes = boxes_with_new_nodes_in(
        ph,
        "grid:bbox",
        mesh,
        new_nodes.iter().map(|n| n.position.as_slice()),
    )?;
    let schemes = Communications {
        send: send_schemes,
        recv: recv.recv,
    };
    Ok(GridGhostPlan {
        mesh_revision: mesh.revision(),
        new_nodes,
        new_elements: recv.new_elements,
        counters: recv.counters,
        schemes,
        bounding_boxes: Some(bounding_boxes),
    })
}

/// Upstream `Mesh::computeBoundingBox()` on a distributed mesh as a phase
/// of `ph` (`mesh.cc:415-434`), for the mesh's nodes followed by `new`
/// (nodes about to be appended): the local box over every node (pure ghosts
/// included), then one Min/Max reduction (`BBox::allSum`: the lower corner
/// and the negated upper corner reduced with `min`, exact) for the global
/// box. Returns `(local, global)`.
pub(crate) fn boxes_with_new_nodes_in<'a>(
    ph: &mut Phases,
    name: &str,
    mesh: &Mesh,
    new: impl Iterator<Item = &'a [Real]>,
) -> Result<(BBox, BBox)> {
    let mut local = mesh.local_bounding_box();
    for x in new {
        local.add_point(x);
    }
    let (mut lower, mut upper) = (local.lower.clone(), local.upper.clone());
    ph.collective(name, |c| {
        crate::mesh::all_sum_bounds(c, &mut lower, &mut upper)
    })?;
    Ok((local, BBox { lower, upper }))
}

/// Apply a plan to a mesh being set up (no consumers): new pure ghost
/// nodes (mirrored in the facet mesh, if any), new ghost elements, ghost
/// counters; returns the grid synchronizer and the events (`NodesAdded`,
/// then `ElementsAdded`, as upstream). Refused (nothing changed) if the
/// mesh changed since the plan.
pub fn apply_grid_plan(
    mesh: &mut Mesh,
    plan: GridGhostPlan,
    id: impl Into<String>,
) -> Result<(ElementSynchronizer, MeshEvents)> {
    if plan.mesh_revision != mesh.revision() {
        return Err(Error::Mesh(format!(
            "grid synchronizer: the plan was computed for revision {} of '{}' (now {})",
            plan.mesh_revision,
            mesh.id(),
            mesh.revision()
        )));
    }
    let mut sync = Synchronizer::new(id, mesh.communicator().clone());
    sync.communications = plan.schemes;
    let dim = mesh.spatial_dimension();
    if let Some((local, global)) = &plan.bounding_boxes
        && (local.lower.len() != dim
            || local.upper.len() != dim
            || global.lower.len() != dim
            || global.upper.len() != dim)
    {
        return Err(Error::Mesh(
            "grid synchronizer: bounding box dimension".into(),
        ));
    }
    if plan.new_nodes.is_empty() && plan.new_elements.is_empty() && plan.counters.is_empty() {
        // upstream recomputes the boxes on every rank even without new
        // nodes (the event is sent unconditionally): same values
        if let Some((local, global)) = plan.bounding_boxes {
            mesh.bbox_local = local;
            mesh.bbox = global;
        }
        return Ok((sync, MeshEvents::none()));
    }
    // (otherwise both events, possibly one of them empty, as upstream)
    // validate before changing anything
    let n0 = mesh.nb_nodes();
    let total = n0 + plan.new_nodes.len();
    for n in &plan.new_nodes {
        if n.position.len() != dim {
            return Err(Error::Mesh("grid synchronizer: node dimension".into()));
        }
    }
    for (ty, conn) in &plan.new_elements {
        if conn.len() != ty.nb_nodes_per_element()
            || conn
                .iter()
                .any(|&c| usize::try_from(c).map_or(true, |c| c >= total))
        {
            return Err(Error::Mesh(
                "grid synchronizer: element connectivity".into(),
            ));
        }
    }
    if mesh.nodes_global_ids.is_none() {
        return Err(Error::Mesh(
            "grid synchronizer: the mesh has no global ids".into(),
        ));
    }
    // a wrongly typed adjacency dataset would make the row extension fail
    // half-way: refused before any change
    for m in std::iter::once(&*mesh).chain(mesh.mesh_facets.as_deref()) {
        use crate::data::MeshDataTypeCode as C;
        for (name, want) in [
            (crate::utils::ELEMENT_TO_SUBELEMENT, C::ElementList),
            (crate::utils::SUBELEMENT_TO_ELEMENT, C::Element),
            (crate::utils::MESH_TO_MESH_FACET, C::Element),
        ] {
            if let Ok(code) = m.data().type_code(name)
                && code != want
            {
                return Err(Error::Mesh(format!(
                    "grid synchronizer: mesh data '{name}' of '{}' has type {code:?}",
                    m.id()
                )));
            }
        }
    }
    // nodes (parent and facet mesh share the node set; the facet mesh holds
    // its own copy of the arrays)
    let mut added_nodes = Vec::with_capacity(plan.new_nodes.len());
    for node in &plan.new_nodes {
        let l = try_idx(mesh.nodes.len())?;
        mesh.nodes.borrow_mut().try_push_tuple(&node.position)?;
        mesh.nodes_flags.push_value(NodeFlag::PURE_GHOST);
        if let Some(g) = &mut mesh.nodes_global_ids {
            g.push_value(node.global_id);
        }
        if let Some(o) = node.owner {
            mesh.nodes_prank.insert(l, o);
        }
        if let Some(f) = mesh.mesh_facets.as_deref_mut() {
            f.nodes.borrow_mut().try_push_tuple(&node.position)?;
            f.nodes_flags.push_value(NodeFlag::PURE_GHOST);
            if let Some(g) = &mut f.nodes_global_ids {
                g.push_value(node.global_id);
            }
            if let Some(o) = node.owner {
                f.nodes_prank.insert(l, o);
            }
        }
        added_nodes.push(l);
    }
    // elements
    let mut added_elements = Vec::with_capacity(plan.new_elements.len());
    for (ty, conn) in &plan.new_elements {
        let c = mesh
            .connectivities
            .as_map_mut()
            .get_or_insert_with(*ty, GhostType::Ghost, || {
                akantu_core::Array::with_id(ty.nb_nodes_per_element(), format!("connectivity:{ty}"))
                    .into()
            });
        let k = c.len();
        c.borrow_mut().try_push_tuple(conn)?;
        added_elements.push(Element::try_new(*ty, k, GhostType::Ghost)?);
    }
    // adjacency rows of the new ghost elements (parent and facet mesh)
    let types: BTreeSet<ElementType> = plan.new_elements.iter().map(|(t, _)| *t).collect();
    for ty in types {
        let len = mesh.nb_element(ty, GhostType::Ghost);
        extend_adjacency_rows(&mut mesh.data, ty, len)?;
        if let Some(f) = mesh.mesh_facets.as_deref_mut() {
            extend_adjacency_rows(&mut f.data, ty, len)?;
        }
    }
    // ghost counters
    for (e, &v) in &plan.counters {
        let counters = &mut mesh.ghosts_counters;
        if !counters.exists(e.ty(), GhostType::Ghost) {
            counters.alloc(0, 1, e.ty(), GhostType::Ghost, 1)?;
        }
        let a = counters.get_mut(e.ty(), GhostType::Ghost)?;
        if a.len() <= e.idx() {
            a.try_resize(e.idx() + 1, 1)?;
        }
        a.as_mut_slice()[e.idx()] = v;
    }
    // bounding boxes: computed by the plan over the final node set
    if let Some((local, global)) = plan.bounding_boxes {
        mesh.bbox_local = local;
        mesh.bbox = global;
    }
    mesh.revision += 1;
    let mut ev = MeshEvents::none();
    ev.push(MeshEvent::NodesAdded {
        nodes: added_nodes,
        old_nodes: None,
        origin: "GridSynchronizer".into(),
        extension: None,
    });
    ev.push(MeshEvent::ElementsAdded {
        elements: added_elements,
        constitutive_law: None,
        origin: "GridSynchronizer".into(),
        extension: None,
    });
    Ok((sync, ev))
}

/// Adjacency contract (ARCHITECTURE): every `element_to_subelement`,
/// `subelement_to_element` and `mesh_to_mesh_facet` block of `(ty, Ghost)`
/// has one row per element; new ghost elements get empty / NULL rows (their
/// facets are unknown; upstream leaves the mesh data short).
fn extend_adjacency_rows(
    data: &mut crate::data::MeshData,
    ty: ElementType,
    len: usize,
) -> Result<()> {
    use crate::utils::{ELEMENT_TO_SUBELEMENT, MESH_TO_MESH_FACET, SUBELEMENT_TO_ELEMENT};
    if let Ok(e2s) = data.element_lists_mut(ELEMENT_TO_SUBELEMENT)
        && let Ok(a) = e2s.get_mut(ty, GhostType::Ghost)
        && a.len() < len
    {
        a.try_resize(len, Vec::new())?;
    }
    for name in [SUBELEMENT_TO_ELEMENT, MESH_TO_MESH_FACET] {
        if let Ok(m) = data.element_map_mut(name)
            && let Ok(a) = m.get_mut(ty, GhostType::Ghost)
            && a.len() < len
        {
            a.try_resize(len, Element::NULL)?;
        }
    }
    Ok(())
}

/// Upstream `GridSynchronizer(mesh, grid, id)` on a mesh being set up (no
/// consumers; use `SharedMesh::setup`): plan and node-scheme rebuild in an
/// own scope (settled), then the local application. Collective.
///
/// Upstream sends its `NewNodesEvent` on every rank, and the mesh's node
/// synchronizer rebuilds its schemes on it (`NodeSynchronizer::onNodesAdded`:
/// receive schemes in local node order, send schemes in the order the slave
/// ranks list their nodes) — even when no node was added; so does the port
/// on more than one rank ([`crate::synchronizers::rebuild_node_schemes_in`]).
/// The new nodes are pure ghosts, which are in no node scheme, so the
/// rebuild is computed before the mesh changes.
pub fn create_grid_synchronizer(
    mesh: &mut Mesh,
    grid: &dyn GridCells,
    id: impl Into<String>,
    options: GridSynchronizerOptions,
) -> Result<(ElementSynchronizer, MeshEvents)> {
    let comm = mesh.communicator().clone();
    let mut ph = Phases::new(&comm, format!("grid_synchronizer:{}", mesh.id()));
    let r = (|| {
        let plan = plan_grid_synchronizer_in(&mut ph, mesh, grid, options)?;
        let node_sync = rebuilt_node_synchronizer_in(&mut ph, mesh)?;
        Ok((plan, node_sync))
    })();
    let (plan, node_sync) = ph.settle(r)?;
    drop(ph);
    let out = apply_grid_plan(mesh, plan, id)?;
    if let Some(ns) = node_sync {
        mesh.synchronizers.node = Some(ns);
    }
    Ok(out)
}

/// The mesh's node synchronizer with the schemes `onNodesAdded` rebuilds
/// (`None` on one rank or without a node synchronizer).
fn rebuilt_node_synchronizer_in(
    ph: &mut Phases,
    mesh: &Mesh,
) -> Result<Option<crate::synchronizers::NodeSynchronizer>> {
    if mesh.communicator().size() == 1 {
        return Ok(None);
    }
    let Some(ns) = mesh.synchronizers().node.as_ref() else {
        return Ok(None);
    };
    let mut ns = ns.clone();
    crate::synchronizers::rebuild_node_schemes_in(ph, mesh, &mut ns)?;
    Ok(Some(ns))
}

/// Upstream `GridSynchronizer(mesh, grid, id)` on a mesh shared with
/// consumers: the plan is computed collectively from a read of the mesh,
/// then applied by a mesh mutation (`NodesAdded`, `ElementsAdded`
/// dispatched to `handlers`; local bounding box recomputed by the
/// mutation). Collective.
pub fn create_grid_synchronizer_shared(
    shared: &SharedMesh,
    handlers: &mut [(&ConsumerToken, &mut dyn crate::events::MeshEventHandler)],
    grid: &dyn GridCells,
    id: impl Into<String>,
    options: GridSynchronizerOptions,
) -> Result<(ElementSynchronizer, crate::mutation::CommittedMutation)> {
    let (plan, comm, mesh_id) = {
        let g = shared.read()?;
        let comm = g.communicator().clone();
        let mut ph = Phases::new(&comm, format!("grid_synchronizer:{}", g.id()));
        let r = plan_grid_synchronizer_in(&mut ph, &g, grid, options);
        (ph.settle(r)?, comm, g.id().to_owned())
    };
    let _ = mesh_id;
    let mut sync = Synchronizer::new(id, comm);
    sync.communications = plan.schemes.clone();
    /// The node synchronizer reacts to the node addition (upstream
    /// `NodeSynchronizer::onNodesAdded`, on every rank).
    struct Hook;
    impl crate::mutation::DistributedHook for Hook {
        fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
            Ok(())
        }
        fn apply(
            &mut self,
            tx: &mut crate::mutation::MeshTransaction,
            _: &[(MeshPart, MeshEvent)],
            ph: &mut Phases,
        ) -> Result<()> {
            if let Some(ns) = rebuilt_node_synchronizer_in(ph, tx.mesh())? {
                tx.synchronizers_mut(MeshPart::Main)?.node = Some(ns);
            }
            Ok(())
        }
    }
    let committed = shared.mutate(
        handlers,
        |tx| {
            if tx.mesh().revision() != plan.mesh_revision {
                return Err(Error::Mesh(
                    "grid synchronizer: the mesh changed since the plan".into(),
                ));
            }
            let mut nodes = Vec::with_capacity(plan.new_nodes.len());
            for node in &plan.new_nodes {
                let l = tx.push_node(MeshPart::Main, &node.position)?;
                tx.set_node_flag(MeshPart::Main, l, NodeFlag::PURE_GHOST)?;
                tx.set_node_global_id(MeshPart::Main, l, node.global_id)?;
                if let Some(o) = node.owner {
                    tx.set_node_prank(MeshPart::Main, l, Some(o))?;
                }
                nodes.push(l);
            }
            let mut elements = Vec::with_capacity(plan.new_elements.len());
            for (ty, conn) in &plan.new_elements {
                elements.push(tx.push_element(MeshPart::Main, *ty, GhostType::Ghost, conn)?);
            }
            let types: BTreeSet<ElementType> = plan.new_elements.iter().map(|(t, _)| *t).collect();
            for ty in types {
                let len = tx.mesh().nb_element(ty, GhostType::Ghost);
                extend_adjacency_rows(tx.data_mut(MeshPart::Main)?, ty, len)?;
                if tx.mesh().mesh_facets().is_some() {
                    extend_adjacency_rows(tx.data_mut(MeshPart::Facets)?, ty, len)?;
                }
            }
            let counters = tx.ghosts_counters_mut(MeshPart::Main)?;
            for (e, &v) in &plan.counters {
                if !counters.exists(e.ty(), GhostType::Ghost) {
                    counters.alloc(0, 1, e.ty(), GhostType::Ghost, 1)?;
                }
                let a = counters.get_mut(e.ty(), GhostType::Ghost)?;
                if a.len() <= e.idx() {
                    a.try_resize(e.idx() + 1, 1)?;
                }
                a.as_mut_slice()[e.idx()] = v;
            }
            // both events on every rank, possibly empty (upstream sends
            // them unconditionally; identical schedules on every rank)
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes,
                    old_nodes: None,
                    origin: "GridSynchronizer".into(),
                    extension: None,
                },
            );
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements,
                    constitutive_law: None,
                    origin: "GridSynchronizer".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        Some(&mut Hook),
    )?;
    Ok((sync, committed))
}
