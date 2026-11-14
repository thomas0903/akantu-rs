// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: per-copy check of the Neumann loads (D-105 /
// D-108, UD-1280) in the cohesive driver ported from
// performance-testing-cohesive @ 483bace8 (cohesive_extrinsic.cc); the
// re-application is Akantu's Neumann `applyBC`
// (src/model/common/boundary_condition/boundary_condition_tmpl.hh) into a
// scratch array.

//! The per-copy check of the loads (`rust_load_check_*`, off by default).
//!
//! The inverted reproducer `tests/doubled_node_load_side.rs` checks, after a
//! forced band of insertions, that **every copy of every doubled node**
//! carries the external force a fresh application of the loads on the
//! post-insertion topology gives it. This module runs the same check inside
//! the benchmark loop, on the run's own crack pattern: at the chosen steps,
//! after the step's insertion and before its explicit solve (the external
//! force that step's solve uses), every rank
//!
//! 1. applies the run's loads again (compression on "top" / "bottom", the
//!    initial shear and every shear increment applied so far on the four
//!    sheared groups, in the driver's order) into a **scratch** nodal array
//!    with the model's own boundary FE engine
//!    ([`akantu_model::boundary_condition::apply_neumann`], the application
//!    without a record: the model's external force, its Neumann store and
//!    the store's view of the external force are only read);
//! 2. compares it with the model's external force at **every** node it
//!    holds (local, master, slave and pure ghost copies: both arrays are sums
//!    over the rank's own local and ghost boundary elements);
//! 3. reports the store's statistics (applications, records moved, moves
//!    from written nodes, foreign writes), the number of
//!    nodes holding a Neumann record, the number of doubled loaded nodes so
//!    far (copies created by the insertion, global id at least the initial
//!    number of global nodes, whose re-applied force is not zero: exactly
//!    the copies upstream's placement leaves without their load), and the
//!    largest per-copy difference relative to the largest nodal load with
//!    its global node id;
//! 4. counts the loaded boundary elements whose connectivity row differs
//!    from the row of their facet (`mesh_to_mesh_facet`),
//!    a topology check the re-application cannot make, since it shares the
//!    rows with the run: before the first insertion that doubles a node the
//!    rows are the mesh file's and many differ from their facets' by design
//!    (upstream copies the facet rows into the boundary rows only in that
//!    insertion's node event); from then on a non-zero count is a row the
//!    insertion did not copy (a stale row, e.g. under UD-1250's gate on a
//!    distributed mesh). Reported (`row_facet_mismatches`, and in
//!    `summary.json` the largest count after the first check with a new
//!    copy), not part of the verdict.
//!
//! The values are reduced over the ranks (one agreed failure flag first,
//! then a fixed sequence of reductions on every rank, D-106 (2)); rank 0
//! writes `load_check.csv` (one row per check), every rank
//! `load_check.r<rank>.csv`, both flushed after every check so that a run
//! stopped early keeps them; `summary.json` gets the input keys as written
//! and a `rust_load_check` object. A check **fails** when the largest
//! difference exceeds [`RTOL`] of the largest nodal load (or is not a
//! number); the run continues (the negative control, upstream's placement,
//! fails from its first doubled loaded node on and runs to the end). Without
//! the input keys nothing of this module runs and the driver's outputs are
//! unchanged.
//!
//! Before the first insertion that doubles a node the scratch array holds
//! the same sums in the same order as the external force: the two are equal
//! bit for bit at every node. From that insertion on they differ by rounding:
//! at a doubled node's copies by the rounding of the move (the record of an
//! element is the sum of its contributions over the applications, moved as
//! one value), and at nodes of boundary triangles whose connectivity rows the
//! insertion rewrote as cyclic rotations of the same nodes (the per-copy
//! check measured 1,839 of 2,798 rows at the first doubling of the h = 0.12 run;
//! normals and jacobians unchanged): with one quadrature point the shape
//! value `1 − ξ − η` at the centroid differs from `ξ` and `η` by an ulp, so a
//! re-application gives it to another node of each rotated triangle than the
//! applications made before the rotation. Hence a bound of rounding size.
//! The store's records follow such rows by node identity
//! (`akantu_model::neumann_store`): a move takes the share recorded for the
//! doubled node itself, whatever slot of the rewritten row it now occupies.
//! (Before the UD-1280 fix the records were kept by slot, so a move after a
//! rotation took the share of another node of the element: with the equal
//! shares of these triangles that was part of the same rounding.)

use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Write as _};
use std::path::Path;

use akantu_comm::{Communicator, ReduceOp, ValueLoc};
use akantu_core::{
    Array, Error, GhostType, GlobalIdx, Idx, NodeFlag, Real, Result, SharedArray, TypeFilter,
};
use akantu_mesh::Mesh;
use akantu_mesh::utils::MESH_TO_MESH_FACET;
use akantu_model::boundary_condition::{FromHigherDim, apply_neumann};
use akantu_model::neumann_store::NeumannStoreStats;
use akantu_solid::SolidMechanicsModel;

use crate::output::g;

/// Bound of the per-copy check, relative to the largest nodal load: D-105
/// (3)'s acceptance bound for the per-copy external force against a
/// re-application of the loads. Only rounding separates the two, and it
/// grows with the number of applications: after `A` applications of up to
/// `m` element contributions each, |f − f_re| ≲ 2·A·m·u·fmax plus the
/// move's two roundings, ≈ 3.6e-13 at the full-size run's step 3,200
/// (A = 134, m ≈ 12), so 1e-12 is the smallest
/// round bound that is safe a priori over the benchmark's history (the
/// reproducer `doubled_node_load_side`, two applications, holds C1F-1's
/// 1e-14). Measured ≤ 2.7e-16 in the reproducer and ≤ 1.9e-15 in the
/// benchmark runs (`docs/NUMERICAL_VALIDATION.md`), while a
/// misplaced load is a whole nodal share (0.34 of the largest nodal load at
/// the first one of the h = 0.12 run with upstream's placement). The files
/// carry the measured values, so a tighter reading can be made from them.
pub const RTOL: Real = 1e-12;

/// The input keys (`rust_load_check_every`, `rust_load_check_steps`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadCheckConfig {
    /// Check at every step that is a multiple of this (step 0, the state
    /// before the loop, included); 0 = no periodic check.
    pub every: usize,
    /// Further steps to check (sorted, distinct).
    pub steps: Vec<usize>,
    /// `rust_load_check_every` as written in the input (recorded in
    /// `summary.json`).
    pub every_raw: Option<String>,
    /// `rust_load_check_steps` as written in the input.
    pub steps_raw: Option<String>,
}

impl LoadCheckConfig {
    /// Whether any check is asked for.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.every > 0 || !self.steps.is_empty()
    }

    /// Whether `step` is checked (0 = after the static solve, before the
    /// loop).
    #[must_use]
    pub fn is_due(&self, step: usize) -> bool {
        (self.every > 0 && step.is_multiple_of(self.every))
            || self.steps.binary_search(&step).is_ok()
    }

    /// Parse the two keys (raw values of the user section, `None` when
    /// absent).
    ///
    /// # Errors
    /// A value that is not a step count / step list.
    pub fn parse(every: Option<String>, steps: Option<String>) -> Result<Self> {
        let every_n = match &every {
            Some(v) => v.trim().parse::<usize>().map_err(|_| {
                Error::invalid(format!(
                    "rust_load_check_every must be a step count (0 = off), not \"{v}\""
                ))
            })?,
            None => 0,
        };
        let list = match &steps {
            Some(v) => crate::diag::parse_steps_for("rust_load_check_steps", v)?,
            None => Vec::new(),
        };
        Ok(Self {
            every: every_n,
            steps: list,
            every_raw: every,
            steps_raw: steps,
        })
    }
}

/// One application of the run's load history: a traction and the groups it
/// is applied on.
pub type LoadApplication<'a> = (&'a FromHigherDim, &'a [&'a str]);

/// The node with the largest per-copy difference.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorstNode {
    /// The rank holding it (for a reduced check: the smallest such rank).
    pub rank: usize,
    /// Its global id.
    pub gid: GlobalIdx,
    /// Its flag on that rank.
    pub flag: NodeFlag,
    /// The model's external force there.
    pub f: [Real; 3],
    /// The re-applied loads there.
    pub reapplied: [Real; 3],
}

/// One rank's values at one check, before the reductions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RankCheck {
    /// Nodes of the rank (local, master, slave, pure ghost).
    pub nb_nodes: u64,
    /// Owned nodes (local or master).
    pub nb_owned: u64,
    /// Whether the model has a Neumann store.
    pub has_store: bool,
    /// The store's counters (zero without a store).
    pub stats: NeumannStoreStats,
    /// Nodes of the rank at which a recorded element contribution is not
    /// zero / of them owned.
    pub record_nodes: u64,
    /// See [`RankCheck::record_nodes`].
    pub record_nodes_owned: u64,
    /// Owned nodes whose re-applied force is not zero.
    pub loaded_owned: u64,
    /// Copies created by the insertion (global id ≥ the initial number of
    /// global nodes) held by the rank / of them owned.
    pub new_copies: u64,
    /// See [`RankCheck::new_copies`].
    pub new_copies_owned: u64,
    /// Of the new copies, those whose re-applied force is not zero (the
    /// doubled loaded nodes) / of them owned.
    pub doubled_loaded: u64,
    /// See [`RankCheck::doubled_loaded`].
    pub doubled_loaded_owned: u64,
    /// Nodes whose external force differs from the re-application (any
    /// component, `!=`) / of them owned.
    pub differing: u64,
    /// See [`RankCheck::differing`].
    pub differing_owned: u64,
    /// Largest |re-applied component| over the owned nodes.
    pub fmax_owned: Real,
    /// Largest |f − f_re| over every node of the rank (NaN counted as
    /// infinite) / over the owned nodes.
    pub diff: Real,
    /// See [`RankCheck::diff`].
    pub diff_owned: Real,
    /// The node of [`RankCheck::diff`].
    pub worst: Option<WorstNode>,
    /// Σ f and Σ f_re over the owned nodes.
    pub total_f: [Real; 3],
    /// See [`RankCheck::total_f`].
    pub total_re: [Real; 3],
    /// Loaded boundary elements of the rank (local and ghost, each once)
    /// whose connectivity row differs from their facet's row
    /// (module documentation).
    pub row_facet_mismatches: u64,
}

fn abs_diff(a: Real, b: Real) -> Real {
    let d = (a - b).abs();
    if d.is_nan() { Real::INFINITY } else { d }
}

fn count(v: usize) -> u64 {
    u64::try_from(v).unwrap_or(u64::MAX)
}

/// The re-application of `loads` on the current mesh into a zeroed scratch
/// array (the model's boundary FE engine; nothing of the model written).
///
/// # Errors
/// No boundary FE engine (no Neumann load applied yet), unknown group, FE
/// errors.
pub fn reapply(
    mesh: &Mesh,
    smm: &SolidMechanicsModel,
    loads: &[LoadApplication<'_>],
) -> Result<Vec<Real>> {
    let dim = mesh.spatial_dimension();
    let fe = smm.physics.base.fems.existing_boundary("")?;
    let mut scratch = SharedArray::new(Array::try_zeros(mesh.nb_nodes(), dim)?);
    for (traction, groups) in loads {
        for name in *groups {
            let group = mesh.groups().element_group(name)?;
            apply_neumann(*traction, mesh, group, fe, dim, &mut scratch)?;
        }
    }
    let out = scratch.read()?.as_slice().to_vec();
    Ok(out)
}

/// The loaded boundary elements of the rank (the elements of the load
/// groups, local and ghost, each counted once) whose connectivity row
/// differs from the row of their facet. Zero without a
/// facet mesh or without `mesh_to_mesh_facet`.
///
/// # Errors
/// Unknown group, mesh errors.
pub fn row_facet_mismatches(mesh: &Mesh, loads: &[LoadApplication<'_>]) -> Result<u64> {
    let Some(fm) = mesh.mesh_facets() else {
        return Ok(0);
    };
    let Ok(m2f) = mesh.data().element_map(MESH_TO_MESH_FACET) else {
        return Ok(0);
    };
    let mut seen = std::collections::HashSet::new();
    let mut mismatches = 0;
    for (_, groups) in loads {
        for name in *groups {
            let group = mesh.groups().element_group(name)?;
            for ghost in GhostType::ALL {
                for ty in group.element_types(ghost) {
                    let Ok(map) = m2f.get(ty, ghost) else {
                        continue;
                    };
                    let conn = mesh.connectivity(ty, ghost)?;
                    for &e in group.elements_of(ty, ghost).iter() {
                        let e = akantu_core::try_to_usize(e)?;
                        if !seen.insert((ty, ghost, e)) {
                            continue;
                        }
                        let Some(facet) = map.as_slice().get(e) else {
                            continue;
                        };
                        if facet.is_null() {
                            continue;
                        }
                        if *fm.element_connectivity(*facet)? != *conn.tuple(e) {
                            mismatches += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(mismatches)
}

/// One rank's part of a check (rank-local, no communication): see the module
/// documentation. `n0` is the number of global nodes before the first
/// insertion.
///
/// # Errors
/// See [`reapply`]; a store block of another size than the mesh's.
pub fn rank_check(
    mesh: &Mesh,
    smm: &SolidMechanicsModel,
    loads: &[LoadApplication<'_>],
    n0: usize,
) -> Result<RankCheck> {
    let dim = mesh.spatial_dimension();
    if dim != 3 {
        return Err(Error::invalid("the load check is written for 3-D runs"));
    }
    let f_re = reapply(mesh, smm, loads)?;
    let f_guard = smm.external_force()?;
    let f = f_guard.as_slice();
    let nb_nodes = mesh.nb_nodes();
    if f.len() != nb_nodes * dim || f_re.len() != f.len() {
        return Err(Error::DimensionMismatch {
            context: "load check: external force",
            expected: nb_nodes * dim,
            got: f.len(),
        });
    }
    // nodes holding a recorded (non-zero) contribution (the store
    // keeps the node of each entry)
    let mut recorded = vec![false; nb_nodes];
    let store = smm.neumann_store();
    if let Some(st) = store {
        let nb_dof = st.nb_dof();
        for ghost in GhostType::ALL {
            for ty in st.forces().element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: ghost,
                kind: None,
            }) {
                let nb = mesh.nb_element(ty, ghost);
                let block = st.forces().get(ty, ghost)?;
                let held = st.holders().get(ty, ghost)?;
                if block.len() != nb || held.len() != nb {
                    return Err(Error::invalid(format!(
                        "load check: the store's block ({ty}, {ghost:?}) has {} rows, the mesh {nb}",
                        block.len(),
                    )));
                }
                for (vals, &n) in block.as_slice().chunks_exact(nb_dof).zip(held.as_slice()) {
                    if vals.iter().any(|v| *v != 0.0) {
                        let n = akantu_core::try_to_usize(n)?;
                        if let Some(r) = recorded.get_mut(n) {
                            *r = true;
                        }
                    }
                }
            }
        }
    }
    let mut out = RankCheck {
        nb_nodes: count(nb_nodes),
        has_store: store.is_some(),
        stats: store.map(|s| s.stats()).unwrap_or_default(),
        row_facet_mismatches: row_facet_mismatches(mesh, loads)?,
        ..RankCheck::default()
    };
    let mut worst_node: Option<usize> = None;
    for (n, &rec) in recorded.iter().enumerate() {
        let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
        let flag = mesh.get_node_flag(ni)?;
        let owned = flag.is_local_or_master();
        let new = usize::try_from(mesh.node_global_id(ni)).is_ok_and(|gid| gid >= n0);
        let r = n * dim..(n + 1) * dim;
        let (fa, fb) = (&f[r.clone()], &f_re[r]);
        let loaded = fb.iter().any(|v| *v != 0.0);
        let differs = fa.iter().zip(fb).any(|(a, b)| a != b);
        let d = fa
            .iter()
            .zip(fb)
            .map(|(a, b)| abs_diff(*a, *b))
            .fold(0.0, Real::max);
        out.nb_owned += u64::from(owned);
        out.record_nodes += u64::from(rec);
        out.record_nodes_owned += u64::from(rec && owned);
        out.new_copies += u64::from(new);
        out.new_copies_owned += u64::from(new && owned);
        out.doubled_loaded += u64::from(new && loaded);
        out.doubled_loaded_owned += u64::from(new && loaded && owned);
        out.differing += u64::from(differs);
        out.differing_owned += u64::from(differs && owned);
        if d > out.diff {
            out.diff = d;
            worst_node = Some(n);
        }
        if owned {
            out.loaded_owned += u64::from(loaded);
            out.diff_owned = out.diff_owned.max(d);
            for k in 0..dim {
                out.fmax_owned = out.fmax_owned.max(fb[k].abs());
                out.total_f[k] += fa[k];
                out.total_re[k] += fb[k];
            }
        }
    }
    if let Some(n) = worst_node {
        let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
        let r = n * dim;
        out.worst = Some(WorstNode {
            rank: usize::try_from(mesh.communicator().rank()).unwrap_or(0),
            gid: mesh.node_global_id(ni),
            flag: mesh.get_node_flag(ni)?,
            f: [f[r], f[r + 1], f[r + 2]],
            reapplied: [f_re[r], f_re[r + 1], f_re[r + 2]],
        });
    }
    Ok(out)
}

/// The values of a check reduced over the ranks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlobalCheck {
    /// Number of ranks.
    pub ranks: usize,
    /// The largest nodal load (largest |re-applied component| over every
    /// owned node of every rank).
    pub fmax: Real,
    /// Sums over the ranks of the owned counts (each node counted once):
    /// loaded nodes, nodes with a record, new copies, doubled loaded nodes,
    /// nodes differing from the re-application.
    pub loaded: u64,
    /// See [`GlobalCheck::loaded`].
    pub record_nodes: u64,
    /// See [`GlobalCheck::loaded`].
    pub new_copies: u64,
    /// See [`GlobalCheck::loaded`].
    pub doubled_loaded: u64,
    /// See [`GlobalCheck::loaded`].
    pub differing_owned: u64,
    /// Sums over the ranks of the per-rank counts (every copy a rank holds:
    /// a shared node counts on each rank holding it): doubled loaded copies,
    /// copies differing from the re-application.
    pub doubled_loaded_copies: u64,
    /// See [`GlobalCheck::doubled_loaded_copies`].
    pub differing_copies: u64,
    /// Sums over the ranks of the store counters (a move is made on every
    /// rank that holds the element: local and ghost copies).
    pub moved: u64,
    /// See [`GlobalCheck::moved`].
    pub moved_from_written: u64,
    /// See [`GlobalCheck::moved`].
    pub foreign_writes: u64,
    /// See [`GlobalCheck::moved`].
    pub reset_nodes: u64,
    /// See [`GlobalCheck::moved`].
    pub forgotten: u64,
    /// Ranks with a Neumann store.
    pub ranks_with_store: u64,
    /// Smallest and largest number of recorded applications over the ranks.
    pub applications_min: u64,
    /// See [`GlobalCheck::applications_min`].
    pub applications_max: u64,
    /// max |f − f_re| / fmax over every node of every rank / over the owned
    /// nodes.
    pub max_rel: Real,
    /// See [`GlobalCheck::max_rel`].
    pub max_rel_owned: Real,
    /// The node of [`GlobalCheck::max_rel`] (`None` when every copy equals
    /// its re-application bit for bit).
    pub worst: Option<WorstNode>,
    /// Σ f and Σ f_re over the owned nodes of every rank.
    pub total_f: [Real; 3],
    /// See [`GlobalCheck::total_f`].
    pub total_re: [Real; 3],
    /// Sum over the ranks of [`RankCheck::row_facet_mismatches`] (a shared
    /// element counts on each rank holding it).
    pub row_facet_mismatches: u64,
}

impl GlobalCheck {
    /// Whether the check passes: every copy within [`RTOL`] of the largest
    /// nodal load (a NaN fails).
    #[must_use]
    pub fn passes(&self) -> bool {
        self.max_rel <= RTOL
    }

    /// max over the components of |Σ f − Σ f_re| / fmax.
    #[must_use]
    pub fn total_rel(&self) -> Real {
        let d = (0..3)
            .map(|k| abs_diff(self.total_f[k], self.total_re[k]))
            .fold(0.0, Real::max);
        rel(d, self.fmax)
    }
}

fn rel(d: Real, fmax: Real) -> Real {
    if d == 0.0 {
        0.0
    } else if fmax > 0.0 {
        d / fmax
    } else {
        Real::INFINITY
    }
}

/// `local`, agreed on every rank before the next collective (D-106 (2)):
/// an error on any rank is an error on all.
///
/// # Errors
/// The local error, or "failed on another rank".
pub fn agreed<T>(comm: &Communicator, local: Result<T>) -> Result<T> {
    let mut flag = [u64::from(local.is_err())];
    comm.all_reduce(&mut flag, ReduceOp::Max)?;
    match local {
        Err(e) => Err(e),
        Ok(_) if flag[0] != 0 => Err(Error::invalid(
            "load check: failed on another rank (see its error)",
        )),
        Ok(v) => Ok(v),
    }
}

/// Reduce one check over the ranks (collective: the same sequence of
/// reductions on every rank).
///
/// # Errors
/// Communication errors.
pub fn reduce(comm: &Communicator, r: &RankCheck) -> Result<GlobalCheck> {
    let ranks = comm.size();
    let mut mx = [r.fmax_owned, r.diff, r.diff_owned];
    comm.all_reduce(&mut mx, ReduceOp::Max)?;
    let [fmax, diff, diff_owned] = mx;
    let mut sums = [
        r.loaded_owned,
        r.record_nodes_owned,
        r.new_copies_owned,
        r.doubled_loaded_owned,
        r.differing_owned,
        r.doubled_loaded,
        r.differing,
        r.stats.moved_contributions,
        r.stats.moved_from_written_nodes,
        r.stats.foreign_writes,
        r.stats.reset_nodes,
        r.stats.forgotten_entries,
        u64::from(r.has_store),
        r.row_facet_mismatches,
    ];
    comm.all_reduce(&mut sums, ReduceOp::Sum)?;
    let mut amin = [r.stats.applications];
    comm.all_reduce(&mut amin, ReduceOp::Min)?;
    let mut amax = [r.stats.applications];
    comm.all_reduce(&mut amax, ReduceOp::Max)?;
    let mut totals = [
        r.total_f[0],
        r.total_f[1],
        r.total_f[2],
        r.total_re[0],
        r.total_re[1],
        r.total_re[2],
    ];
    comm.all_reduce(&mut totals, ReduceOp::Sum)?;
    // the worst node: its rank (MaxLoc, ties to the smallest rank), then its
    // values from that rank alone
    let rank = comm.rank();
    let mut loc = [ValueLoc {
        value: if r.worst.is_some() { r.diff } else { -1.0 },
        loc: rank,
    }];
    comm.all_reduce(&mut loc, ReduceOp::MaxLoc)?;
    let winner = loc[0].loc;
    let mine = winner == rank && r.worst.is_some();
    let (gid, flag, wf, wr) = match (mine, r.worst) {
        (true, Some(w)) => (w.gid, i64::from(w.flag.bits()), w.f, w.reapplied),
        _ => (-1, -1, [0.0; 3], [0.0; 3]),
    };
    let mut ids = [gid, flag];
    comm.all_reduce(&mut ids, ReduceOp::Max)?;
    let mut vals = [wf[0], wf[1], wf[2], wr[0], wr[1], wr[2]];
    comm.all_reduce(&mut vals, ReduceOp::Sum)?;
    let worst = (loc[0].value >= 0.0 && ids[0] >= 0).then(|| WorstNode {
        rank: usize::try_from(winner).unwrap_or(0),
        gid: ids[0],
        flag: NodeFlag(u8::try_from(ids[1]).unwrap_or(0)),
        f: [vals[0], vals[1], vals[2]],
        reapplied: [vals[3], vals[4], vals[5]],
    });
    Ok(GlobalCheck {
        ranks,
        fmax,
        loaded: sums[0],
        record_nodes: sums[1],
        new_copies: sums[2],
        doubled_loaded: sums[3],
        differing_owned: sums[4],
        doubled_loaded_copies: sums[5],
        differing_copies: sums[6],
        moved: sums[7],
        moved_from_written: sums[8],
        foreign_writes: sums[9],
        reset_nodes: sums[10],
        forgotten: sums[11],
        ranks_with_store: sums[12],
        row_facet_mismatches: sums[13],
        applications_min: amin[0],
        applications_max: amax[0],
        max_rel: rel(diff, fmax),
        max_rel_owned: rel(diff_owned, fmax),
        worst,
        total_f: [totals[0], totals[1], totals[2]],
        total_re: [totals[3], totals[4], totals[5]],
    })
}

/// A whole check on every rank (collective): [`rank_check`], agreed, then
/// [`reduce`]. Returns this rank's values and the reduced ones.
///
/// # Errors
/// The first rank's error (agreed), communication errors.
pub fn check(
    comm: &Communicator,
    mesh: &Mesh,
    smm: &SolidMechanicsModel,
    loads: &[LoadApplication<'_>],
    n0: usize,
) -> Result<(RankCheck, GlobalCheck)> {
    let local = agreed(comm, rank_check(mesh, smm, loads, n0))?;
    let global = reduce(comm, &local)?;
    Ok((local, global))
}

/// Short name of a node flag (`local`, `master`, `slave`, `ghost`).
#[must_use]
pub fn flag_name(f: NodeFlag) -> &'static str {
    if f.is_pure_ghost() {
        "ghost"
    } else if f.is_slave() {
        "slave"
    } else if f.is_master() {
        "master"
    } else {
        "local"
    }
}

/// What `summary.json` records of the checks (`rust_load_check`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadCheckSummary {
    /// `rust_load_check_every` as written.
    pub every_raw: Option<String>,
    /// `rust_load_check_steps` as written.
    pub steps_raw: Option<String>,
    /// Checks made.
    pub checks: usize,
    /// Checks that failed.
    pub failed_checks: usize,
    /// The largest per-copy difference over every check (relative to that
    /// check's largest nodal load), its step and global node id.
    pub max_rel: Real,
    /// See [`LoadCheckSummary::max_rel`].
    pub max_rel_step: Option<usize>,
    /// See [`LoadCheckSummary::max_rel`].
    pub max_rel_node: Option<GlobalIdx>,
    /// First checked step with a doubled loaded node.
    pub first_doubled_loaded_step: Option<usize>,
    /// First failed check.
    pub first_failed_step: Option<usize>,
    /// The last check's step and global counts.
    pub last_step: Option<usize>,
    /// See [`GlobalCheck::doubled_loaded`] (last check).
    pub doubled_loaded: u64,
    /// See [`GlobalCheck::moved`] (last check).
    pub moved: u64,
    /// See [`GlobalCheck::moved_from_written`] (last check).
    pub moved_from_written: u64,
    /// See [`GlobalCheck::foreign_writes`] (last check).
    pub foreign_writes: u64,
    /// The largest [`GlobalCheck::row_facet_mismatches`] over the checks
    /// from the first one with a new copy on (`None` before such a check):
    /// a loaded boundary row the insertion did not copy from its facet
    /// (informational, not part of the verdict).
    pub row_facet_mismatches_after_doubling: Option<u64>,
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_string(), |v| v.to_string())
}

impl LoadCheckSummary {
    /// Record one reduced check.
    pub fn record(&mut self, step: usize, gl: &GlobalCheck) {
        self.checks += 1;
        if !gl.passes() {
            self.failed_checks += 1;
            self.first_failed_step.get_or_insert(step);
        }
        if gl.doubled_loaded > 0 {
            self.first_doubled_loaded_step.get_or_insert(step);
        }
        let m = if gl.max_rel.is_nan() {
            Real::INFINITY
        } else {
            gl.max_rel
        };
        if self.max_rel_step.is_none() || m > self.max_rel {
            self.max_rel = m;
            self.max_rel_step = Some(step);
            self.max_rel_node = gl.worst.map(|w| w.gid);
        }
        if gl.new_copies > 0 || self.row_facet_mismatches_after_doubling.is_some() {
            let m = self.row_facet_mismatches_after_doubling.get_or_insert(0);
            *m = (*m).max(gl.row_facet_mismatches);
        }
        self.last_step = Some(step);
        self.doubled_loaded = gl.doubled_loaded;
        self.moved = gl.moved;
        self.moved_from_written = gl.moved_from_written;
        self.foreign_writes = gl.foreign_writes;
    }

    /// The verdict: `pass` when every check passed, `fail` when one failed,
    /// `none` when no check was made (keys given but no listed step reached).
    #[must_use]
    pub fn verdict(&self) -> &'static str {
        if self.checks == 0 {
            "none"
        } else if self.failed_checks == 0 {
            "pass"
        } else {
            "fail"
        }
    }

    /// The end-of-run line of the driver (rank 0, verbose).
    #[must_use]
    pub fn final_line_text(&self) -> String {
        let s = self;
        format!(
            "akantu-rs: load check: {} checks, {} failed; max |f - f_reapplied| / max|f| = {} at \
             step {} (node {}); {} doubled loaded nodes, {} records moved (sum over the ranks) at \
             the last check (step {}): {}",
            s.checks,
            s.failed_checks,
            g(s.max_rel, 3),
            opt(s.max_rel_step),
            opt(s.max_rel_node),
            s.doubled_loaded,
            s.moved,
            opt(s.last_step),
            match s.verdict() {
                "pass" => "pass",
                "none" => "no check made",
                _ => "FAIL",
            }
        )
    }

    /// The lines `summary.json` gets (each ending with `,\n`): the input
    /// keys as written (strings, so that a session comparing its settings
    /// with the summary finds them) and the `rust_load_check` object.
    #[must_use]
    pub fn json_lines(&self) -> String {
        let mut s = String::new();
        if let Some(v) = &self.every_raw {
            let _ = writeln!(s, "  \"rust_load_check_every\": \"{}\",", json_str(v));
        }
        if let Some(v) = &self.steps_raw {
            let _ = writeln!(s, "  \"rust_load_check_steps\": \"{}\",", json_str(v));
        }
        let max_rel = if self.max_rel.is_finite() {
            g(self.max_rel, 17)
        } else {
            "null".into()
        };
        let _ = writeln!(
            s,
            "  \"rust_load_check\": {{\"checks\": {}, \"failed_checks\": {}, \"tolerance\": {}, \
             \"max_rel_diff\": {max_rel}, \"max_rel_diff_step\": {}, \"max_rel_diff_node\": {}, \
             \"first_doubled_loaded_step\": {}, \"first_failed_step\": {}, \"last_step\": {}, \
             \"doubled_loaded_nodes\": {}, \"moved_contributions\": {}, \
             \"moved_from_written_nodes\": {}, \"foreign_writes\": {}, \
             \"row_facet_mismatches_after_doubling\": {}, \"verdict\": \"{}\"}},",
            self.checks,
            self.failed_checks,
            g(RTOL, 17),
            opt(self.max_rel_step),
            opt(self.max_rel_node),
            opt(self.first_doubled_loaded_step),
            opt(self.first_failed_step),
            opt(self.last_step),
            self.doubled_loaded,
            self.moved,
            self.moved_from_written,
            self.foreign_writes,
            opt(self.row_facet_mismatches_after_doubling),
            self.verdict()
        );
        s
    }
}

fn json_str(v: &str) -> String {
    v.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if (c as u32) < 0x20 => vec![' '],
            c => vec![c],
        })
        .collect()
}

/// Header of `load_check.r<rank>.csv`.
pub const RANK_HEADER: &str = "step, increments, nb_nodes, nb_owned, store, applications, \
     moved_contributions, moved_from_written_nodes, foreign_writes, reset_nodes, \
     forgotten_entries, record_nodes, record_nodes_owned, loaded_owned, new_copies, \
     new_copies_owned, doubled_loaded, doubled_loaded_owned, differing, differing_owned, \
     max_rel_diff, max_rel_diff_owned, worst_gid, worst_flag, worst_f_x, worst_f_y, worst_f_z, \
     worst_reapplied_x, worst_reapplied_y, worst_reapplied_z, row_facet_mismatches";

/// Header of `load_check.csv`.
pub const GLOBAL_HEADER: &str = "step, increments, ranks, fmax, loaded_nodes, record_nodes, \
     new_copies, doubled_loaded, doubled_loaded_copies, moved_contributions_sum, \
     moved_from_written_nodes_sum, foreign_writes_sum, reset_nodes_sum, forgotten_entries_sum, \
     ranks_with_store, applications_min, applications_max, differing_owned, differing_copies, \
     max_rel_diff, max_rel_diff_owned, worst_rank, worst_gid, worst_flag, worst_f_x, worst_f_y, \
     worst_f_z, worst_reapplied_x, worst_reapplied_y, worst_reapplied_z, total_f_x, total_f_y, \
     total_f_z, total_reapplied_x, total_reapplied_y, total_reapplied_z, total_rel_diff, \
     row_facet_mismatches, verdict";

/// One row of `load_check.r<rank>.csv` (`fmax` = the reduced largest nodal
/// load).
#[must_use]
pub fn rank_row(step: usize, increments: usize, r: &RankCheck, fmax: Real) -> String {
    let p = 17;
    let st = r.stats;
    let mut s = format!(
        "{step}, {increments}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, \
         {}, {}, {}",
        r.nb_nodes,
        r.nb_owned,
        u8::from(r.has_store),
        st.applications,
        st.moved_contributions,
        st.moved_from_written_nodes,
        st.foreign_writes,
        st.reset_nodes,
        st.forgotten_entries,
        r.record_nodes,
        r.record_nodes_owned,
        r.loaded_owned,
        r.new_copies,
        r.new_copies_owned,
        r.doubled_loaded,
        r.doubled_loaded_owned,
        r.differing,
        r.differing_owned,
        g(rel(r.diff, fmax), p),
        g(rel(r.diff_owned, fmax), p)
    );
    match r.worst {
        Some(WorstNode {
            gid,
            flag,
            f,
            reapplied: re,
            ..
        }) => {
            let _ = write!(
                s,
                ", {gid}, {}, {}, {}, {}, {}, {}, {}",
                flag_name(flag),
                g(f[0], p),
                g(f[1], p),
                g(f[2], p),
                g(re[0], p),
                g(re[1], p),
                g(re[2], p)
            );
        }
        None => s.push_str(", -1, -, 0, 0, 0, 0, 0, 0"),
    }
    let _ = write!(s, ", {}", r.row_facet_mismatches);
    s
}

/// One row of `load_check.csv`.
#[must_use]
pub fn global_row(step: usize, increments: usize, gl: &GlobalCheck) -> String {
    let p = 17;
    let mut s = format!(
        "{step}, {increments}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, \
         {}, {}",
        gl.ranks,
        g(gl.fmax, p),
        gl.loaded,
        gl.record_nodes,
        gl.new_copies,
        gl.doubled_loaded,
        gl.doubled_loaded_copies,
        gl.moved,
        gl.moved_from_written,
        gl.foreign_writes,
        gl.reset_nodes,
        gl.forgotten,
        gl.ranks_with_store,
        gl.applications_min,
        gl.applications_max,
        gl.differing_owned,
        gl.differing_copies,
        g(gl.max_rel, p),
        g(gl.max_rel_owned, p)
    );
    match gl.worst {
        Some(WorstNode {
            rank,
            gid,
            flag,
            f,
            reapplied: re,
        }) => {
            let _ = write!(
                s,
                ", {rank}, {gid}, {}, {}, {}, {}, {}, {}, {}",
                flag_name(flag),
                g(f[0], p),
                g(f[1], p),
                g(f[2], p),
                g(re[0], p),
                g(re[1], p),
                g(re[2], p)
            );
        }
        None => s.push_str(", -1, -1, -, 0, 0, 0, 0, 0, 0"),
    }
    let _ = write!(
        s,
        ", {}, {}, {}, {}, {}, {}, {}, {}, {}",
        g(gl.total_f[0], p),
        g(gl.total_f[1], p),
        g(gl.total_f[2], p),
        g(gl.total_re[0], p),
        g(gl.total_re[1], p),
        g(gl.total_re[2], p),
        g(gl.total_rel(), p),
        gl.row_facet_mismatches,
        if gl.passes() { "pass" } else { "FAIL" }
    );
    s
}

/// The driver's state of the checks: the output files and the summary.
#[derive(Debug)]
pub struct LoadCheck {
    rank_file: BufWriter<File>,
    global_file: Option<BufWriter<File>>,
    /// What `summary.json` records.
    pub summary: LoadCheckSummary,
    n0: usize,
    verbose: bool,
}

fn create(dir: &Path, name: &str) -> Result<BufWriter<File>> {
    let path = dir.join(name);
    File::create(&path)
        .map(BufWriter::new)
        .map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })
}

fn write_line(w: &mut BufWriter<File>, line: &str, name: &str) -> Result<()> {
    writeln!(w, "{line}")
        .and_then(|()| w.flush())
        .map_err(|source| Error::Io {
            path: name.to_string(),
            source,
        })
}

impl LoadCheck {
    /// Open the output files in `dir` (collective: a failure on one rank is
    /// agreed). `n0`: the number of global nodes before the first insertion.
    ///
    /// # Errors
    /// I/O errors (agreed).
    pub fn new(
        cfg: &LoadCheckConfig,
        comm: &Communicator,
        dir: &Path,
        n0: usize,
        verbose: bool,
    ) -> Result<Self> {
        let prank = comm.rank();
        let local = (|| -> Result<(BufWriter<File>, Option<BufWriter<File>>)> {
            let name = format!("load_check.r{prank}.csv");
            let mut rank_file = create(dir, &name)?;
            write_line(&mut rank_file, RANK_HEADER, &name)?;
            let global_file = if prank == 0 {
                let mut f = create(dir, "load_check.csv")?;
                write_line(&mut f, GLOBAL_HEADER, "load_check.csv")?;
                Some(f)
            } else {
                None
            };
            Ok((rank_file, global_file))
        })();
        let (rank_file, global_file) = agreed(comm, local)?;
        Ok(Self {
            rank_file,
            global_file,
            summary: LoadCheckSummary {
                every_raw: cfg.every_raw.clone(),
                steps_raw: cfg.steps_raw.clone(),
                ..LoadCheckSummary::default()
            },
            n0,
            verbose,
        })
    }

    /// One check at `step` (collective): computes, reduces, writes the rows
    /// and records the summary. Returns the reduced values.
    ///
    /// # Errors
    /// The check's errors (agreed), I/O errors.
    pub fn run(
        &mut self,
        step: usize,
        increments: usize,
        comm: &Communicator,
        mesh: &Mesh,
        smm: &SolidMechanicsModel,
        loads: &[LoadApplication<'_>],
    ) -> Result<GlobalCheck> {
        let (local, global) = check(comm, mesh, smm, loads, self.n0)?;
        let prank = comm.rank();
        let io = (|| -> Result<()> {
            write_line(
                &mut self.rank_file,
                &rank_row(step, increments, &local, global.fmax),
                "load_check.r<rank>.csv",
            )?;
            if let Some(f) = self.global_file.as_mut() {
                write_line(f, &global_row(step, increments, &global), "load_check.csv")?;
            }
            Ok(())
        })();
        agreed(comm, io)?;
        let was_failed = self.summary.first_failed_step.is_some();
        self.summary.record(step, &global);
        if self.verbose && prank == 0 && !was_failed && !global.passes() {
            println!(
                "akantu-rs: load check: step {step}: a copy's external force differs from the \
                 re-applied loads by {} of the largest nodal load (node {}; bound {})",
                g(global.max_rel, 3),
                opt(global.worst.map(|w| w.gid)),
                g(RTOL, 3)
            );
        }
        Ok(global)
    }

    /// The end-of-run line (rank 0, verbose):
    /// [`LoadCheckSummary::final_line_text`].
    #[must_use]
    pub fn final_line(&self) -> String {
        self.summary.final_line_text()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_parse_and_select_steps() {
        let off = LoadCheckConfig::parse(None, None).unwrap();
        assert!(!off.enabled() && !off.is_due(0) && !off.is_due(100));
        let every = LoadCheckConfig::parse(Some("10".into()), None).unwrap();
        assert!(every.enabled());
        assert!(every.is_due(0) && every.is_due(10) && every.is_due(3200));
        assert!(!every.is_due(5) && !every.is_due(3201));
        let list = LoadCheckConfig::parse(Some("0".into()), Some("683, 700:710:5".into())).unwrap();
        assert!(list.enabled());
        assert_eq!(list.steps, vec![683, 700, 705, 710]);
        assert!(list.is_due(705) && !list.is_due(0) && !list.is_due(701));
        assert_eq!(list.every_raw.as_deref(), Some("0"));
        assert!(LoadCheckConfig::parse(Some("-1".into()), None).is_err());
        assert!(LoadCheckConfig::parse(Some("x".into()), None).is_err());
        let e = LoadCheckConfig::parse(None, Some("5:1".into())).unwrap_err();
        assert!(e.to_string().contains("rust_load_check_steps"), "{e}");
    }

    #[test]
    fn summary_records_the_first_failure_and_the_largest_difference() {
        let mut s = LoadCheckSummary {
            every_raw: Some("1".into()),
            ..LoadCheckSummary::default()
        };
        let pass = GlobalCheck {
            max_rel: 2e-16,
            fmax: 1.0,
            ..GlobalCheck::default()
        };
        s.record(0, &pass);
        let loaded = GlobalCheck {
            doubled_loaded: 3,
            moved: 9,
            max_rel: 0.5,
            worst: Some(WorstNode {
                rank: 1,
                gid: 4242,
                flag: NodeFlag::SLAVE,
                f: [0.0; 3],
                reapplied: [1.0; 3],
            }),
            ..pass.clone()
        };
        s.record(7, &loaded);
        s.record(8, &pass);
        assert_eq!(s.checks, 3);
        assert_eq!(s.failed_checks, 1);
        assert_eq!(s.first_failed_step, Some(7));
        assert_eq!(s.first_doubled_loaded_step, Some(7));
        assert_eq!((s.max_rel_step, s.max_rel_node), (Some(7), Some(4242)));
        assert_eq!(s.last_step, Some(8));
        let j = s.json_lines();
        assert!(
            j.starts_with("  \"rust_load_check_every\": \"1\",\n"),
            "{j}"
        );
        assert!(j.contains("\"first_failed_step\": 7") && j.contains("\"verdict\": \"fail\""));
        assert!(j.ends_with("},\n"));
        // NaN fails and counts as the largest difference
        let nan = GlobalCheck {
            max_rel: Real::NAN,
            ..pass
        };
        assert!(!nan.passes());
        s.record(9, &nan);
        assert_eq!(s.max_rel_step, Some(9));
        assert!(s.json_lines().contains("\"max_rel_diff\": null"));
    }

    /// Keys given but no check made is not a pass; the
    /// row/facet count is kept from the first check with a new copy on.
    #[test]
    fn no_check_is_not_a_pass_and_mismatches_count_after_the_first_copy() {
        let mut s = LoadCheckSummary {
            every_raw: Some("0".into()),
            steps_raw: Some("9999".into()),
            ..LoadCheckSummary::default()
        };
        assert_eq!(s.verdict(), "none");
        assert!(s.json_lines().contains("\"verdict\": \"none\""));
        assert!(s.final_line_text().ends_with("no check made"));
        let before = GlobalCheck {
            fmax: 1.0,
            row_facet_mismatches: 1839,
            ..GlobalCheck::default()
        };
        s.record(0, &before);
        assert_eq!(s.row_facet_mismatches_after_doubling, None);
        let after = GlobalCheck {
            new_copies: 4,
            ..before.clone()
        };
        s.record(
            838,
            &GlobalCheck {
                row_facet_mismatches: 0,
                ..after.clone()
            },
        );
        assert_eq!(s.row_facet_mismatches_after_doubling, Some(0));
        s.record(
            839,
            &GlobalCheck {
                row_facet_mismatches: 2,
                new_copies: 0,
                ..after
            },
        );
        assert_eq!(s.row_facet_mismatches_after_doubling, Some(2));
        assert_eq!(s.verdict(), "pass");
        assert!(
            s.json_lines()
                .contains("\"row_facet_mismatches_after_doubling\": 2, \"verdict\": \"pass\"")
        );
    }

    #[test]
    fn rows_have_as_many_fields_as_their_headers() {
        let n = |s: &str| s.split(", ").count();
        let r = RankCheck::default();
        assert_eq!(n(&rank_row(0, 0, &r, 1.0)), n(RANK_HEADER));
        let w = WorstNode {
            rank: 2,
            gid: 5,
            flag: NodeFlag::MASTER,
            f: [1.0; 3],
            reapplied: [2.0; 3],
        };
        let r = RankCheck {
            worst: Some(w),
            ..r
        };
        assert_eq!(n(&rank_row(0, 0, &r, 1.0)), n(RANK_HEADER));
        let gl = GlobalCheck::default();
        assert_eq!(n(&global_row(0, 0, &gl)), n(GLOBAL_HEADER));
        let gl = GlobalCheck {
            worst: Some(WorstNode {
                flag: NodeFlag::PURE_GHOST,
                ..w
            }),
            ..gl
        };
        let row = global_row(0, 0, &gl);
        assert_eq!(n(&row), n(GLOBAL_HEADER));
        assert!(row.ends_with(", pass"), "{row}");
    }
}
