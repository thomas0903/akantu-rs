// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/{dof_manager.hh,
// dof_manager.cc, dof_manager_inline_impl.hh, dof_manager_default.*,
// dof_manager_petsc.*}, src/synchronizer/dof_synchronizer.*.

//! The DOF manager (upstream `DOFManager` / `DOFManagerDefault`).
//!
//! Owns, per DOF family (`"displacement"`, `"temperature"`, `"damage"`,
//! `"lagrange_multiplier"`, …): the DOF values and their time derivatives,
//! increment, previous values, blocked flags, the local equation numbers;
//! and globally: the [`EquationLayout`] (local ↔ global equations with
//! ownership), the residual, the solution, lumped matrices and — through a
//! [`LinearSystemBackend`] — the sparse matrices and the linear solve.
//!
//! # Numbering (normative, `dof_manager.cc:768-870`)
//!
//! * Each **registration** and each **node addition** numbers a new block:
//!   nodes in local order (registration) or event order (addition),
//!   components innermost. Periodic slaves get no equation and alias their
//!   master's local equation; every other DOF gets the next local equation.
//! * Owned (local-or-master node) DOFs get consecutive global numbers from
//!   `next_global + exclusive_scan(nb_new_owned)`; the persistent offset
//!   `next_global` then advances by the all-reduced count — numbering is per
//!   block, not one rank-contiguous range.
//! * Distributed slave DOFs receive their owner's global number through the
//!   node synchronizer (`_ask_nodes`); pure-ghost DOFs have a local equation
//!   but **no** global equation (they never enter matrices or global
//!   vectors).
//! * Registration order of families defines the initial numbering; on node
//!   addition families are updated in **lexicographic id order** (upstream
//!   iterates `std::map<ID, DOFData>`), reproduced here by the `BTreeMap`.
//!
//! # Checked sizes and collective protocol
//!
//! Every growth operation (registration, node addition) is one
//! [`Phases`] scope: a **local preflight** validates all user-controlled
//! inputs (node indices, group support, family names) and all aggregate
//! capacities — new DOFs of each family and the total of existing plus new
//! **local equations**, which are stored as [`Idx`] (D-012) — and reserves
//! the memory of every growing array, before any mutation or collective;
//! the preflight is agreed, then the numbering collectives run (exclusive
//! scan, all-reduce, `_ask_nodes` exchange) as agreed phases; the arrays
//! are committed last. A recoverable (agreed) failure restores the layout
//! and the families exactly; a transport failure poisons the manager.
//! Assembly entry points validate terms, filters and connectivities before
//! indexing (no panic on user input).
//!
//! # One scope per collective operation
//!
//! Every collective method runs through [`DofManager::collective`]: nested
//! calls (from solver callbacks, native trampolines or other methods) join
//! the outermost scope, local validation is agreed before dependent
//! communication, and every call ends with an agreement, so its result is
//! identical on every rank. Solver callbacks run as [`DofManager::step`]s
//! and communicate only through [`DofManager::phases`]. A transport failure
//! or an agreed fatal failure (promotion, reset or commit after an
//! irreversible change) poisons the manager — on every rank for the agreed
//! case — and returns `Error::Fatal`; so does a callback, body or native
//! driver returning an `Error::Fatal` itself (agreed fatal on every
//! rank, or — transport kind — the abort path without another agreement).
//! Between its own uses the running operation's scope is parked in the
//! manager, which therefore stays `Send`; every step resumes it on the
//! calling thread, so a scoped worker continues the same operation.
//!
//! # Assembly (normative)
//!
//! Elemental matrices are profiled once per `(matrix, family, type, ghost)`
//! with the full local loop, then added through the backend
//! (`assembleElementalMatricesToMatrix_`, `dof_manager_inline_impl.hh:
//! 233-305`). Element equations without a global number (pure ghosts) are
//! mapped past the matrix order and skipped by the `c_irn < size` rule.
//! Whenever the numbering or the connectivity changes, **every** matrix
//! profile is reset (upstream `resizeGlobalArrays` → `clearProfile`).
//!
//! # Symmetry (UD-034 default)
//!
//! [`DofManager::finalize_matrix`] agrees the effective symmetry of a
//! matrix on all ranks (a rank that promoted locally makes every rank
//! promote, including zero-work ranks) and propagates the promotion to the
//! matrices declared as derived from it ([`DofManager::new_derived_matrix`],
//! e.g. `J` from `K`, `M`, `C`); backends re-analyse on the new type.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use akantu_comm::phase::{ParkedScope, Phases, Severity};
use akantu_comm::{CommunicationBuffer, Communicator, DataAccessor, ReduceOp, Synchronizer};
use akantu_core::array::ArrayView;
use akantu_core::array_shared::{ArrayMut, ArrayRef, SharedArray};
use akantu_core::enums::{DofSupportType, EventPriority, MatrixType, SynchronizationTag};
use akantu_core::{
    Array, ElementType, Error, FatalKind, GhostType, GlobalIdx, INVALID_IDX, Id, Idx,
    MAX_LOCAL_COUNT, NodeFlag, Real, Result,
};
use akantu_linalg::direct::SparseSolverCompat;
use akantu_linalg::{
    EquationLayout, EquationOwnership, NoReduction, PartialSumReduction, SolverVector,
    SparseMatrixAij, TermsToAssemble,
};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};

use crate::backend::{AijBackend, BackendContext, BackendKind, LinearSystemBackend};

mod assembly;
mod blocked;
mod lumped_blocks;
mod node_removal;
mod residual;

pub use residual::ResidualVector;

pub use assembly::FamilyArray;
pub use lumped_blocks::LumpedBlockCouplings;

/// Vector whose norm a convergence test measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvergenceVector {
    /// The residual (`convergence_type = residual`).
    Residual,
    /// The solution of the last linear solve (`solution`,
    /// `residual_mass_wgh`).
    Solution,
}

/// Upstream-compatibility switches of the solver stack sharing one DOF
/// manager (D-003 category B defects found). The
/// default (`false` everywhere) is the corrected behaviour; differential
/// replays against the unpatched oracle set the switch of the defect they
/// reproduce. Read by the time-step solvers, integration schemes and
/// Newton–Raphson solvers working with this manager.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpstreamCompat {
    /// **UD-193**: integration schemes skip rebuilding the
    /// shared `"J"` when their own source matrices (`K`, `M`, `C`) and the
    /// blocked set are unchanged, even if another time-step solver rebuilt
    /// `J` in between (upstream: stale Jacobian after a solver switch).
    /// Default: a time-step solver rebuilds `J` whenever `J` changed since
    /// its own last assembly.
    pub stale_shared_jacobian: bool,
    /// **UD-194**: `convergence_type = residual_mass_wgh` takes
    /// upstream's never-implemented branch (solution norm without residual
    /// refreshes). Default: refused with `Error::NotImplemented`.
    pub residual_mass_wgh_unrefreshed: bool,
    /// **UD-195**: the generalized trapezoidal Jacobian reads
    /// `K` even when its coefficient `e = αΔt` is zero (forward Euler with a
    /// consistent mass, temperature-rate solution: upstream fails with
    /// "matrix K does not exist"). Default: `K` skipped when `e == 0`, as
    /// upstream's `NewmarkBeta` does.
    pub generalized_trapezoidal_reads_k_when_e_is_zero: bool,
    /// **UD-287**: the generalized trapezoidal predictor keeps the
    /// rate `u̇ⁿ` (`generalized_trapezoidal.cc:40-60`) while the correctors
    /// add `d·δ` to it, so for `α > 0` the scheme solves
    /// `u̇ⁿ⁺¹ = u̇ⁿ + (uⁿ⁺¹ − u_pred)/(αΔt)` instead of
    /// `uⁿ⁺¹ = uⁿ + Δt((1 − α)u̇ⁿ + α u̇ⁿ⁺¹)`: backward Euler freezes after
    /// its first step under constant loads (`(M/Δt + K)(uⁿ⁺¹ − uⁿ) = 0`),
    /// trapezoidal rule 1 is not Crank–Nicolson. Default: for `α ≠ 0` the
    /// predictor sets the rate of the free DOFs to `0` (the correctors then
    /// build `u̇ⁿ⁺¹` from the step's increment, the textbook scheme);
    /// forward Euler (`α = 0`, where both forms agree) is unchanged.
    pub generalized_trapezoidal_keeps_rate_in_predictor: bool,
    /// **UD-216**: every MUMPS solver of the manager
    /// reproduces upstream's workspace retry, whose outer solve uses the
    /// previous solution as right-hand side (`A^-(k+1) b` after `k`
    /// factorisation retries). Default: every solve starts from the
    /// original right-hand side. Applied before every solve to the
    /// installed sparse solver, also after a reallocation
    /// (`SparseSolverCompat`, D-041 (4)).
    pub mumps_retry_with_solution_rhs: bool,
    /// **UD-422**: the PETSc backend inserts every elemental
    /// block the way upstream `SparseMatrixPETSc::addValues` does — the
    /// column-major block read as row-major (`MAT_ROW_ORIENTED`), i.e.
    /// transposed for square blocks. Default: blocks inserted as given.
    /// Ignored by the AIJ backend.
    pub petsc_row_oriented_blocks: bool,
    /// **UD-423**: a PETSc KSP solve that diverges (negative
    /// `KSPConvergedReason`) returns its last iterate silently, as upstream
    /// `SparseSolverPETSc::solve`. Default: `Error::LinearSolver`.
    pub petsc_ignore_ksp_divergence: bool,
    /// **UD-427**: the right-hand side of a PETSc KSP solve keeps
    /// the residual at blocked (Dirichlet) rows — the reactions — as
    /// upstream, so an iterative KSP's relative tolerance applies to a norm
    /// dominated by the reactions and Newton–Raphson stalls at about
    /// `ksp_rtol × |reaction|`. Default: the blocked rows of the right-hand
    /// side are zeroed (the matrix has unit diagonals there, so only the
    /// blocked rows of the solution change, which the correctors skip).
    pub petsc_ksp_rhs_with_reactions: bool,
    /// **UD-424**: real values set on a PETSc solver
    /// (`NonLinearSolver::set_param`) become options with upstream's
    /// `std::stringstream` formatting (6 significant digits). Default:
    /// round-trip digits.
    pub petsc_six_digit_real_options: bool,
    /// **UD-428**: the PETSc SNES/TAO solvers alias their iterate
    /// with the DOF manager's solution vector, into which upstream's
    /// corrector copies every trial point (and SNES's function vector with
    /// the residual, overwritten by every evaluation): a rejected
    /// line-search step corrupts SNES's iterate (upstream diverges with
    /// `DIVERGED_LINE_SEARCH` on the cubic bar). Default: the solver owns
    /// its vectors.
    pub petsc_aliased_solver_vectors: bool,
    /// **UD-426** (D-003 B): every PETSc
    /// SNES solve of the manager writes the Jacobian to `J_petsc` in the
    /// working directory (Matrix Market through a PETSc viewer), exactly as
    /// upstream `NonLinearSolverPETSc::solve` does after assembling `J` and
    /// before `SNESSolve`. Default: no file (upstream's debug leftover: I/O
    /// proportional to the matrix at every solve, concurrent jobs in one
    /// directory overwrite each other). An unwritable working directory
    /// makes the solve fail on every rank (upstream's solve throws).
    pub petsc_write_j_petsc: bool,
    /// **UD-1022** (decision D-088, D-003 A with
    /// a compat switch): a bounded `petsc_tao` solve whose bounds exclude
    /// the zero increment on some owned row (`lower > 0` or `upper < 0`)
    /// runs as upstream's does. Upstream's objective
    /// (`NonLinearSolverTAO::computeObjectiveGradient`) is exact only at
    /// `δ = 0`, and GPCG evaluates it once, at its start point projected
    /// onto the bounds, `median(lower, 0, upper) = δ₀ ≠ 0`: the solve then
    /// returns the obstacle solution of the shifted load `f − K δ₀` (a
    /// silent error, 32 % on the toy bar of `petsc_tao_ud1022`). Default:
    /// such a solve is refused before anything runs, with
    /// `Error::Unsupported` naming UD-1022 on every rank (an agreed,
    /// recoverable error: the model's state is unchanged). Upstream's own
    /// use (the phase field, `0 ≤ δ ≤ 1 − d`) always contains `δ = 0`.
    pub tao_bounds_excluding_zero_increment: bool,
}

impl UpstreamCompat {
    /// The sparse-solver part of the switches (`akantu_linalg`).
    #[must_use]
    pub fn sparse_solver(&self) -> SparseSolverCompat {
        SparseSolverCompat {
            mumps_retry_with_solution_rhs: self.mumps_retry_with_solution_rhs,
        }
    }
}

/// Storage of one DOF family (upstream `DOFManager::DOFData`).
#[derive(Debug)]
pub struct DofData {
    /// Nodal or generic.
    pub support: DofSupportType,
    /// Element group whose nodes support the family (`None` = whole mesh,
    /// upstream `group_support == "__mesh__"`).
    pub group_support: Option<Id>,
    /// Values (`nb_dofs` tuples of `nb_component`). Exportable (D-047):
    /// element access through guards (`read`/`write`, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub dofs: SharedArray<Real>,
    /// Time derivatives by order (1 = velocity, 2 = acceleration).
    pub derivatives: BTreeMap<usize, SharedArray<Real>>,
    /// Increment of the last solve.
    pub increment: Option<SharedArray<Real>>,
    /// Values at the previous step.
    pub previous: Option<SharedArray<Real>>,
    /// Blocked (Dirichlet) flags.
    pub blocked: Option<SharedArray<bool>>,
    /// Solution of the last linear solve split on this family.
    pub solution: Array<Real>,
    /// Local equation of each DOF (tuple-major), stored as [`Idx`] (D-012);
    /// periodic slaves alias their master's equation.
    pub local_equation_numbers: Vec<Idx>,
    /// Supporting node of each DOF (nodal families; upstream
    /// `associated_nodes`).
    pub associated_nodes: Vec<Idx>,
    /// Per DOF: the global id of its supporting node when that node is
    /// local or master, else 0, as a `Real` (what upstream's
    /// `saveAssociatedNodes` assembles, `dof_manager.cc:1014-1034`),
    /// recorded when the DOF is numbered: the Newton solver's trace dump
    /// (`nodes.mtx`, DOF-017) has no mesh. Recorded again whenever the
    /// manager renumbers: node additions number their new DOFs, and a node
    /// removal renumbers every family afresh on the current mesh
    /// (`dof_manager/node_removal.rs`).
    pub associated_node_gids: Vec<Real>,
    /// Tuple of the DOF array that holds each supporting node (identity for
    /// whole-mesh families).
    pub node_to_tuple: Option<HashMap<Idx, usize>>,
    /// Per DOF: the supporting node is a periodic slave (its equation
    /// aliases the master's; upstream `isPeriodicSlave(node)` in
    /// `assembleToGlobalArray`). Empty when no DOF is a periodic slave.
    pub periodic_slave: Vec<bool>,
}

impl DofData {
    fn eq(&self, k: usize) -> Result<usize> {
        let e = *self
            .local_equation_numbers
            .get(k)
            .ok_or(Error::OutOfRange {
                what: "dof",
                index: k,
                len: self.local_equation_numbers.len(),
            })?;
        akantu_core::try_to_usize(e)
    }

    /// Exportable real arrays of the family (values, derivatives,
    /// increment, previous; the solution is not exported).
    fn shared_arrays_mut(&mut self) -> Vec<&mut SharedArray<Real>> {
        let mut v: Vec<&mut SharedArray<Real>> = vec![&mut self.dofs];
        v.extend(self.derivatives.values_mut());
        v.extend(
            [&mut self.increment, &mut self.previous]
                .into_iter()
                .flatten(),
        );
        v
    }
}

/// Owner of all DOF families of one solver.
#[derive(Debug)]
pub struct DofManager {
    id: Id,
    communicator: Communicator,
    families: BTreeMap<Id, DofData>,
    /// Registration order of families.
    order: Vec<Id>,
    layout: EquationLayout,
    /// The residual (exportable values, PY-045).
    residual: ResidualVector,
    /// Solution of the last linear solve (local equations).
    solution: Vec<Real>,
    lumped: BTreeMap<Id, SolverVector>,
    /// Symmetric 3×3 couplings of block-lumped matrices (D-059; see
    /// [`LumpedBlockCouplings`]), keyed by lumped matrix.
    lumped_blocks: BTreeMap<Id, LumpedBlockCouplings>,
    backend: Box<dyn LinearSystemBackend>,
    profiled: BTreeSet<(Id, Id, ElementType, GhostType)>,
    /// Declared type of every matrix.
    declared: BTreeMap<Id, MatrixType>,
    /// `source → derived` matrices (promotion propagation).
    derived: BTreeMap<Id, Vec<Id>>,
    /// Node synchronizer of the mesh (cached at registration and on mesh
    /// events) for owner → copy synchronisation of solutions.
    node_sync: Option<Synchronizer<Idx>>,
    /// Periodic node synchronizer of a distributed periodic mesh (cached
    /// with `node_sync`): owner → copy synchronisation of the periodic
    /// masters' solution values (upstream `getArrayPerDOFs` →
    /// `makeConsistentForPeriodicity(_id)` = `synchronizeArrayPBC`).
    periodic_sync: Option<Synchronizer<Idx>>,
    /// Set after a transport failure or an agreed fatal failure inside a
    /// collective operation; every later use is refused.
    poisoned: Option<(FatalKind, String)>,
    /// The collective scope of the running operation.
    active: ScopeSlot,
    /// Node flag of each local equation (upstream `dofs_flag`; `NORMAL` for
    /// generic DOFs).
    dofs_flag: Vec<NodeFlag>,
    /// The mesh of the last numbering was periodic (upstream
    /// `mesh->isPeriodic()` in `assembleToGlobalArray`).
    mesh_periodic: bool,
    /// Scratch vector over local equations (upstream `data_cache`).
    data_cache: Vec<Real>,
    /// How the per-equation loops of the integration schemes run (D-123):
    /// serial until the owning model sets its executor.
    exec: akantu_core::exec::Exec,
    /// Inverse equation maps of the threaded scatter, per family.
    eq_inverse: std::sync::Mutex<BTreeMap<Id, assembly::EqInverse>>,
    /// Bumped wherever a family's local equation numbers change
    /// (`number_block`, `restore`, `commit_renumbering`): the inverse maps
    /// of the threaded scatter are valid for one generation.
    numbering_generation: u64,
    /// Blocked local equations, sorted and unique (upstream
    /// `global_blocked_dofs_indexes`), snapshot of the last
    /// [`DofManager::update_global_blocked_dofs`].
    blocked_indexes: Vec<usize>,
    /// Upstream `previous_global_blocked_dofs_indexes`.
    previous_blocked_indexes: Vec<usize>,
    /// Upstream `global_blocked_dofs_indexes_release`.
    blocked_release: u64,
    /// Upstream `previous_global_blocked_dofs_indexes_release`.
    previous_blocked_release: u64,
    /// Blocked flag per local equation (upstream `global_blocked_dofs`).
    global_blocked: Vec<bool>,
    /// Value release of the matrix at the last `apply_boundary` (upstream
    /// `jacobian_release`, one counter shared by every matrix).
    jacobian_release: u64,
    /// Distribution state of the mesh at the first nodal registration
    /// (upstream: the residual is a `SolverVectorDistributed` iff the mesh
    /// was distributed when the manager was built); it sets the kind of
    /// the manager's vectors ([`DofManager::vectors_distributed`]).
    numbered_distributed: Option<bool>,
    /// Upstream-compatibility switches (default: corrected behaviour).
    compat: UpstreamCompat,
}

/// Collective scope slot of a [`DofManager`].
///
/// Between its own uses the running operation's scope is **parked**
/// ([`ParkedScope`], `Send`): the manager stays `Send` (idle owners can
/// move between threads; a running operation can be continued by a scoped
/// worker that borrows the manager), and every use resumes the scope on
/// the calling thread — the checked transfer that keeps the communicator's
/// admission identity on the thread actually communicating. While parked,
/// the scope stays registered: independent scopes and raw operations on
/// the communicator are refused on every thread.
#[derive(Debug, Default)]
enum ScopeSlot {
    /// No collective operation is running.
    #[default]
    Idle,
    /// A collective operation is running: nested collective methods and
    /// solver callbacks communicate through this scope.
    Active(ParkedScope),
    /// The scope is resumed by one step of the manager (an internal step,
    /// an agreement, or a callback holding [`DofManager::phases`]).
    Lent,
}

/// The running operation's scope, resumed on the current thread for a
/// communicating solver callback ([`DofManager::phases`]); parked back into
/// the manager when dropped. `!Send`: it stays on the resuming thread.
#[derive(Debug)]
pub struct ScopeGuard<'a> {
    slot: &'a mut ScopeSlot,
    phases: Option<Phases>,
}

impl std::ops::Deref for ScopeGuard<'_> {
    type Target = Phases;
    fn deref(&self) -> &Phases {
        self.phases
            .as_ref()
            .unwrap_or_else(|| unreachable!("the scope is held until drop"))
    }
}

impl std::ops::DerefMut for ScopeGuard<'_> {
    fn deref_mut(&mut self) -> &mut Phases {
        self.phases
            .as_mut()
            .unwrap_or_else(|| unreachable!("the scope is held until drop"))
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        if let Some(ph) = self.phases.take() {
            *self.slot = ScopeSlot::Active(ph.park());
        }
    }
}

fn to_global(n: usize) -> Result<GlobalIdx> {
    GlobalIdx::try_from(n).map_err(|_| Error::invalid("count exceeds the global index range"))
}

/// Local shape check of a local-equation vector (agreed by the caller).
fn check_len(context: &'static str, got: usize, expected: usize) -> Result<()> {
    if got == expected {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        })
    }
}

fn to_u64(n: usize) -> Result<u64> {
    u64::try_from(n).map_err(|_| Error::invalid("count exceeds 64 bits"))
}

/// Total local equations after adding `added` to `existing` must stay
/// representable as [`Idx`] (D-012).
fn check_local_equations(existing: usize, added: usize) -> Result<usize> {
    let total = existing.checked_add(added).ok_or(Error::CapacityExceeded {
        what: "local equations",
        requested: existing as u128 + added as u128,
        limit: MAX_LOCAL_COUNT as u128,
    })?;
    if total > MAX_LOCAL_COUNT {
        return Err(Error::CapacityExceeded {
            what: "local equations",
            requested: total as u128,
            limit: MAX_LOCAL_COUNT as u128,
        });
    }
    Ok(total)
}

fn check_node(mesh: &Mesh, n: Idx) -> Result<usize> {
    let u = akantu_core::try_to_usize(n)?;
    if u < mesh.nb_nodes() {
        Ok(u)
    } else {
        Err(Error::OutOfRange {
            what: "node",
            index: u,
            len: mesh.nb_nodes(),
        })
    }
}

/// Locally computed numbering of one block (no collective, no mutation).
#[derive(Debug, Default)]
struct BlockPlan {
    ownership: Vec<EquationOwnership>,
    /// Node flag of each new local equation (upstream `dofs_flag`).
    flags: Vec<NodeFlag>,
    nb_owned: u64,
    /// Local equation of each new DOF (absolute local numbers).
    local_eqs: Vec<Idx>,
    associated: Vec<Idx>,
    /// Per new DOF: global id of the node if local or master, else 0
    /// ([`DofData::associated_node_gids`]).
    associated_gids: Vec<Real>,
    /// Per new DOF: periodic slave node.
    periodic_slave: Vec<bool>,
}

/// Plan the numbering of `nodes` for a family with existing data `data`
/// (or none), local equations starting at `first_local` (upstream
/// `updateDOFsData` without its collectives). Validates node indices and
/// periodic masters.
fn plan_block(
    mesh: &Mesh,
    data: Option<&DofData>,
    nodes: &[Idx],
    nbc: usize,
    first_local: usize,
) -> Result<BlockPlan> {
    let mut plan = BlockPlan::default();
    for &n in nodes {
        check_node(mesh, n)?;
        if mesh.periodic_master(n).is_some() {
            continue;
        }
        let f = mesh.node_flag(n);
        let o = if f.is_local_or_master() {
            EquationOwnership::Owned
        } else if f.is_pure_ghost() {
            EquationOwnership::PureGhost
        } else {
            EquationOwnership::Shared
        };
        plan.ownership.extend(std::iter::repeat_n(o, nbc));
        plan.flags.extend(std::iter::repeat_n(f, nbc));
    }
    check_local_equations(first_local, plan.ownership.len())?;
    plan.nb_owned = to_u64(
        plan.ownership
            .iter()
            .filter(|o| **o == EquationOwnership::Owned)
            .count(),
    )?;
    let mut next_local = first_local;
    let mut masters: HashMap<(Idx, usize), Idx> = HashMap::new();
    for &n in nodes {
        // upstream `mesh->isLocalOrMasterNode(n) ? getNodeGlobalId(n) : 0`
        #[allow(clippy::cast_precision_loss)] // upstream `Real(gid)`
        let gid = if mesh.node_flag(n).is_local_or_master() {
            mesh.node_global_id(n) as Real
        } else {
            0.0
        };
        for c in 0..nbc {
            plan.associated.push(n);
            plan.associated_gids.push(gid);
            plan.periodic_slave.push(mesh.periodic_master(n).is_some());
            if mesh.periodic_master(n).is_some() {
                plan.local_eqs.push(INVALID_IDX); // fixed below
                continue;
            }
            let eq = akantu_core::try_idx(next_local)?;
            if !mesh.periodic_slaves(n).is_empty() {
                masters.insert((n, c), eq);
            }
            plan.local_eqs.push(eq);
            next_local += 1;
        }
    }
    // Periodic slaves alias their master's equation. Upstream looks only in
    // this block's masters and silently uses 0 otherwise; the port also
    // searches the family's existing DOFs and fails if absent.
    for k in 0..plan.associated.len() {
        let n = plan.associated[k];
        if let Some(m) = mesh.periodic_master(n) {
            let c = k % nbc;
            let eq = match masters.get(&(m, c)) {
                Some(&e) => e,
                None => data
                    .and_then(|d| {
                        d.associated_nodes
                            .iter()
                            .zip(&d.local_equation_numbers)
                            .skip(c)
                            .step_by(nbc)
                            .find(|(node, _)| **node == m)
                            .map(|(_, &e)| e)
                    })
                    .ok_or_else(|| {
                        Error::Mesh(format!("periodic master {m} of node {n} has no DOF"))
                    })?,
            };
            plan.local_eqs[k] = eq;
        }
    }
    Ok(plan)
}

/// Collective part of one block: global offsets (exclusive scan,
/// all-reduce), layout append, `_ask_nodes`. Every step is agreed or a
/// matched collective of `ph`.
#[allow(clippy::too_many_arguments)] // one argument per upstream input of `updateDOFsData`
fn number_block(
    ph: &mut Phases,
    layout: &mut EquationLayout,
    flags: &mut Vec<NodeFlag>,
    data: &mut DofData,
    plan: BlockPlan,
    nbc: usize,
    sync: Option<&Synchronizer<Idx>>,
    periodic: Option<&Synchronizer<Idx>>,
) -> Result<()> {
    let (offset, total) = ph.collective("first_dof_ids", |c| {
        let offset = c.exclusive_scan_sum_u64(plan.nb_owned)?;
        let mut total = [plan.nb_owned];
        c.all_reduce(&mut total, ReduceOp::Sum)?;
        Ok((offset, total[0]))
    })?;
    // Identical inputs on every rank except `offset`: an overflow is
    // detected consistently, but agree it anyway (no silent divergence).
    let first = usize::try_from(offset)
        .map_err(|_| Error::invalid("offset"))
        .and_then(to_global)
        .and_then(|o| {
            layout
                .next_global()
                .checked_add(o)
                .ok_or_else(|| Error::invalid("global equation overflow"))
        });
    let total = usize::try_from(total).map_err(|_| Error::invalid("total"));
    let appended = match (first, total) {
        (Ok(first), Ok(total)) => layout
            .append_block(&plan.ownership, first, total, |_| None)
            .map(|_| ()),
        (Err(e), _) | (_, Err(e)) => Err(e),
    };
    ph.agree("append_block", appended, Severity::Recoverable)?;
    data.local_equation_numbers
        .extend_from_slice(&plan.local_eqs);
    data.associated_nodes.extend_from_slice(&plan.associated);
    data.associated_node_gids
        .extend_from_slice(&plan.associated_gids);
    if plan.periodic_slave.iter().any(|&b| b) || !data.periodic_slave.is_empty() {
        let old = data.local_equation_numbers.len() - plan.local_eqs.len();
        data.periodic_slave.resize(old, false);
        data.periodic_slave.extend_from_slice(&plan.periodic_slave);
    }
    flags.extend_from_slice(&plan.flags);
    // Periodic master copies this rank does not own (parallel slaves and
    // pure ghosts appended by the distributed makePeriodic): the owners send
    // their global numbers through the periodic synchronizer first (upstream
    // `_giu_global_conn`, `dof_manager.cc:860-866`; local periodic slaves
    // alias these equations).
    if let Some(periodic) = periodic {
        let mut acc = GlobalNumbers {
            data: &*data,
            layout: &mut *layout,
            nbc,
        };
        periodic.synchronize_in(ph, &mut acc, SynchronizationTag::GiuGlobalConn)?;
    }
    // Shared DOFs: owners send global numbers (upstream `_ask_nodes`).
    if let Some(sync) = sync {
        let mut acc = GlobalNumbers { data, layout, nbc };
        sync.synchronize_in(ph, &mut acc, SynchronizationTag::AskNodes)?;
    }
    Ok(())
}

/// Saved state for the rollback of an agreed growth failure.
struct GrowthBackup {
    layout: EquationLayout,
    flags_len: usize,
    /// `(family, old dof count, old equation count)`.
    lens: Vec<(Id, usize, usize)>,
}

/// Upstream `CommunicatorEventHandler::onCommunicatorFinalize` of the
/// sparse solvers (`sparse_solver.cc:38,70`): the backend's solvers release
/// their internal data here, before the fields drop — the communicator
/// handle (declared before the backend) may be the rank's last one, whose
/// drop runs the communicator's finalisation.
impl Drop for DofManager {
    fn drop(&mut self) {
        self.backend.release_solvers();
    }
}

impl DofManager {
    /// Empty manager with the default AIJ backend.
    #[must_use]
    pub fn new(id: impl Into<Id>, communicator: Communicator) -> Self {
        Self::with_backend(id, communicator, Box::new(AijBackend::default()))
    }

    /// Upstream `DOFManagerFactory::allocate(dof_manager_type, mesh, id)`
    /// (`ModelSolver::initDOFManager`): `"default"` → the AIJ backend,
    /// `"petsc"` → the PETSc backend (feature `petsc`; collective over an
    /// MPI communicator, see `PetscBackend::new`). Other names, or
    /// `"petsc"` without the feature, are refused like an unregistered
    /// allocator upstream.
    pub fn of_type(
        dof_manager_type: &str,
        id: impl Into<Id>,
        communicator: Communicator,
    ) -> Result<Self> {
        match dof_manager_type {
            "default" => Ok(Self::new(id, communicator)),
            #[cfg(feature = "petsc")]
            "petsc" => {
                let backend = crate::petsc_backend::PetscBackend::new(&communicator)?;
                Ok(Self::with_backend(id, communicator, Box::new(backend)))
            }
            other => Err(Error::invalid(format!(
                "no DOF manager of type '{other}' is registered (available: default{})",
                if cfg!(feature = "petsc") {
                    ", petsc"
                } else {
                    "; petsc needs the `petsc` feature"
                }
            ))),
        }
    }

    /// Empty manager with a given backend (PETSc, mock, …).
    #[must_use]
    pub fn with_backend(
        id: impl Into<Id>,
        communicator: Communicator,
        backend: Box<dyn LinearSystemBackend>,
    ) -> Self {
        // upstream `DOFManagerPETSc` allocates `SolverVectorPETSc`s, whose
        // `isDistributed()` is always `true` (`solver_vector_petsc.hh:119`)
        let mut residual = ResidualVector::default();
        residual.set_distributed(backend.kind() == BackendKind::Petsc);
        Self {
            id: id.into(),
            communicator,
            families: BTreeMap::new(),
            order: Vec::new(),
            layout: EquationLayout::new(),
            residual,
            solution: Vec::new(),
            lumped: BTreeMap::new(),
            lumped_blocks: BTreeMap::new(),
            backend,
            profiled: BTreeSet::new(),
            declared: BTreeMap::new(),
            derived: BTreeMap::new(),
            node_sync: None,
            periodic_sync: None,
            poisoned: None,
            active: ScopeSlot::Idle,
            dofs_flag: Vec::new(),
            mesh_periodic: false,
            data_cache: Vec::new(),
            blocked_indexes: Vec::new(),
            previous_blocked_indexes: Vec::new(),
            blocked_release: 0,
            previous_blocked_release: 0,
            global_blocked: Vec::new(),
            jacobian_release: 0,
            numbered_distributed: None,
            compat: UpstreamCompat::default(),
            exec: akantu_core::exec::Exec::serial(),
            eq_inverse: std::sync::Mutex::default(),
            numbering_generation: 0,
        }
    }

    /// How the per-equation loops (integration-scheme updates) run.
    #[must_use]
    pub fn exec(&self) -> &akantu_core::exec::Exec {
        &self.exec
    }

    /// Set how the per-equation loops run (the owning model's executor,
    /// D-123); every result is the same for every thread count in
    /// [`akantu_core::exec::Precision::Bitwise`].
    pub fn set_exec(&mut self, exec: akantu_core::exec::Exec) {
        self.exec = exec;
    }

    /// Upstream-compatibility switches of the solvers working with this
    /// manager (default: every corrected behaviour).
    #[must_use]
    pub fn upstream_compat(&self) -> UpstreamCompat {
        self.compat
    }

    /// Select upstream-compatibility switches (differential replays; set
    /// identically on every rank, before the solves it should affect).
    pub fn set_upstream_compat(&mut self, compat: UpstreamCompat) {
        self.compat = compat;
        self.backend.set_upstream_compat(&compat);
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Communicator.
    #[must_use]
    pub fn communicator(&self) -> &Communicator {
        &self.communicator
    }

    /// Local ↔ global equation layout.
    #[must_use]
    pub fn layout(&self) -> &EquationLayout {
        &self.layout
    }

    /// Backend.
    #[must_use]
    pub fn backend(&self) -> &dyn LinearSystemBackend {
        self.backend.as_ref()
    }

    /// Mutable backend (solver configuration, native drivers); refused on
    /// a poisoned manager.
    pub fn backend_mut(&mut self) -> Result<&mut dyn LinearSystemBackend> {
        self.check_poison()?;
        Ok(self.backend.as_mut())
    }

    /// Poisoning reason after a transport failure or an agreed fatal
    /// failure, if any.
    #[must_use]
    pub fn poisoned(&self) -> Option<&str> {
        self.poisoned.as_ref().map(|(_, p)| p.as_str())
    }

    /// Class of the poisoning, if any (the runtime's abort signal).
    #[must_use]
    pub fn poison_kind(&self) -> Option<FatalKind> {
        self.poisoned.as_ref().map(|(k, _)| *k)
    }

    /// Local check, never communicating: a manager poisoned by an agreed
    /// fatal failure is poisoned on every rank; one poisoned by a
    /// transport failure must not communicate again (its runtime aborts).
    fn check_poison(&self) -> Result<()> {
        match &self.poisoned {
            Some((kind, p)) => Err(Error::fatal(
                *kind,
                Error::Communication(format!("DOF manager '{}' poisoned: {p}", self.id)),
            )),
            None => Ok(()),
        }
    }

    /// Record the poisoning; the first reason is kept, the kind only ever
    /// becomes more urgent (a manager poisoned agreed-fatal
    /// that later observes a transport-fatal result reports `Transport`).
    fn poison(&mut self, kind: FatalKind, reason: String) {
        match &mut self.poisoned {
            None => self.poisoned = Some((kind, reason)),
            Some((k, _)) => *k = k.most_urgent(kind),
        }
    }

    /// Owner/consumer exit of a scope step: a broken
    /// scope or an agreed fatal failure poisons the manager (identically on
    /// every rank for the agreed case) and the error becomes
    /// [`Error::Fatal`] ([`Phases::settle`]). The kind comes from the
    /// scope's state, never from the error alone: an agreed-kind fatal
    /// error the scope did not agree (a refusal before any communication)
    /// is passed on unchanged to the enclosing owner, which agrees it.
    fn settle<R>(&mut self, ph: &Phases, what: &str, r: Result<R>) -> Result<R> {
        let r = ph.settle(r);
        if let (Err(e), Some(kind)) = (&r, ph.fatal_kind()) {
            self.poison(kind, format!("{what}: {e}"));
        }
        r
    }

    fn scope(&self, what: &str) -> Phases {
        Phases::new(&self.communicator, format!("dof:{}:{what}", self.id))
    }

    /// Resume the running operation's scope on the current thread (the
    /// slot is `Lent` until [`DofManager::check_in`]).
    fn check_out(&mut self, what: &str) -> Result<Phases> {
        match std::mem::replace(&mut self.active, ScopeSlot::Lent) {
            ScopeSlot::Active(parked) => Ok(parked.resume()),
            ScopeSlot::Lent => Err(Error::invalid(format!(
                "DOF manager '{}': '{what}' re-entered from an internal step",
                self.id
            ))),
            ScopeSlot::Idle => {
                self.active = ScopeSlot::Idle;
                Err(Error::invalid(format!(
                    "DOF manager '{}': '{what}' outside a collective operation",
                    self.id
                )))
            }
        }
    }

    /// Park the scope back into the manager.
    fn check_in(&mut self, ph: Phases) {
        self.active = ScopeSlot::Active(ph.park());
    }

    /// The error of a step started on an already failed scope (nothing is
    /// communicated).
    fn failed_step(&mut self, name: &str) -> Result<Error> {
        let mut ph = self.check_out(name)?;
        let e = ph
            .check(name)
            .err()
            .unwrap_or_else(|| Error::invalid("failed scope"));
        self.check_in(ph);
        Ok(e)
    }

    /// Run `f` as **one collective operation** of this manager.
    ///
    /// * The first (outermost) call opens the manager's phase scope; every
    ///   collective method of the manager called inside `f` (directly or
    ///   from a solver callback) **joins** it instead of opening an
    ///   independent scope, and communicating callbacks communicate through
    ///   it ([`DofManager::phases`]).
    /// * Every call — outermost or joined — ends with an agreement on `f`'s
    ///   outcome, so **the result of every collective method is common
    ///   knowledge**: identical success/failure on every rank, also when
    ///   only one rank's validation or callback failed. Callers may branch
    ///   on it without diverging (native drivers branch on trampoline
    ///   status codes).
    /// * Once an agreement failed, the scope is failed on every rank: later
    ///   agreements and collectives return at once without communicating.
    /// * A transport failure or an agreed fatal failure poisons the manager
    ///   ([`Error::Fatal`]); a callback or body returning an
    ///   `Error::Fatal` is agreed as fatal (every rank's manager is
    ///   poisoned, every rank returns `Error::Fatal { kind: Agreed }`) or,
    ///   for the transport kind, breaks the scope without communicating.
    /// * Refused locally, without communicating, if another collective
    ///   scope is live on the communicator (a mesh mutation, another
    ///   manager's operation — on any thread): such code must use the scope
    ///   it was handed (`*_phased` methods).
    /// * The scope is thread-confined while in use and parked between uses:
    ///   a scoped worker borrowing the manager during `f`
    ///   continues the same operation (each step resumes the scope on the
    ///   worker), never an independent one.
    pub fn collective<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut DofManager) -> Result<R>,
    ) -> Result<R> {
        self.check_poison()?;
        let owner = match &self.active {
            ScopeSlot::Idle => true,
            ScopeSlot::Active(parked) if parked.is_failed() => {
                // The operation already failed on every rank (or this rank
                // observed a transport failure): nothing more runs.
                return Err(self.failed_step(name)?);
            }
            ScopeSlot::Active(_) => false,
            ScopeSlot::Lent => {
                return Err(Error::invalid(format!(
                    "DOF manager '{}': collective operation '{name}' re-entered from an internal step",
                    self.id
                )));
            }
        };
        if owner {
            let mut ph = self.scope(name);
            if let Err(e) = ph.check("open") {
                // Refused (another live scope) or born broken (the
                // communicator observed a transport failure): nothing is
                // communicated.
                return self.settle(&ph, name, Err(e));
            }
            self.check_in(ph);
        }
        let r = f(self);
        let mut ph = match std::mem::replace(&mut self.active, ScopeSlot::Lent) {
            ScopeSlot::Active(parked) => parked.resume(),
            other => {
                self.active = other;
                return Err(Error::invalid(format!(
                    "DOF manager '{}': collective scope lost in '{name}'",
                    self.id
                )));
            }
        };
        let r = ph.agreed(&format!("{name}:done"), r, Severity::Recoverable);
        let r = self.settle(&ph, name, r);
        if owner {
            self.active = ScopeSlot::Idle;
            drop(ph);
        } else {
            self.check_in(ph);
        }
        r
    }

    /// [`DofManager::phases`] together with the storage of the DOF family
    /// `dof_id` (`None` if not registered), borrowed disjointly: a callback
    /// whose code reads or writes the DOF arrays around its exchanges (the
    /// non-local cycle of the solid model, the split-node contact's
    /// `_cf_nodal` / `_cf_incr` exchanges) holds both at once.
    ///
    /// # Errors
    /// As [`DofManager::phases`].
    pub fn phases_with_family(
        &mut self,
        dof_id: &str,
    ) -> Result<(ScopeGuard<'_>, Option<&mut DofData>)> {
        match &self.active {
            ScopeSlot::Active(_) => {
                let ph = self.check_out("phases")?;
                Ok((
                    ScopeGuard {
                        slot: &mut self.active,
                        phases: Some(ph),
                    },
                    self.families.get_mut(dof_id),
                ))
            }
            ScopeSlot::Lent => Err(Error::invalid(format!(
                "DOF manager '{}': the collective scope is already in use by a step",
                self.id
            ))),
            ScopeSlot::Idle => Err(Error::Communication(format!(
                "DOF manager '{}': no collective operation is running (callbacks communicate \
                 only inside DofManager::collective)",
                self.id
            ))),
        }
    }

    /// The scope of the running collective operation, for **communicating
    /// solver callbacks** (the explicit enclosing-scope protocol, like
    /// `on_mesh_event_phased` for mesh consumers): a callback communicates
    /// only through it (`sync.slave_reduction_in(&mut *dofs.phases()?, …)`,
    /// `Phases::run`), never through an independent scope or a raw
    /// communicator call (both are refused while the scope is live). The
    /// guard resumes the scope on the calling thread and parks it back when
    /// dropped.
    pub fn phases(&mut self) -> Result<ScopeGuard<'_>> {
        match &self.active {
            ScopeSlot::Active(_) => {
                let ph = self.check_out("phases")?;
                Ok(ScopeGuard {
                    slot: &mut self.active,
                    phases: Some(ph),
                })
            }
            ScopeSlot::Lent => Err(Error::invalid(format!(
                "DOF manager '{}': the collective scope is already in use by a step",
                self.id
            ))),
            ScopeSlot::Idle => Err(Error::Communication(format!(
                "DOF manager '{}': no collective operation is running (callbacks communicate \
                 only inside DofManager::collective)",
                self.id
            ))),
        }
    }

    /// One step of the running collective operation — typically one solver
    /// callback — whose outcome is agreed on every rank afterwards (skipped
    /// without communication if the operation already failed). Must be
    /// called inside [`DofManager::collective`]. A callback returning an
    /// `Error::Fatal` is agreed as fatal on every rank (or, for the
    /// transport kind, breaks the scope without communicating) and poisons
    /// the manager.
    pub fn step<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut DofManager) -> Result<R>,
    ) -> Result<R> {
        match &self.active {
            ScopeSlot::Active(parked) if parked.is_failed() => {
                // Nothing more runs once the operation failed.
                return Err(self.failed_step(name)?);
            }
            ScopeSlot::Active(_) => {}
            _ => {
                return Err(Error::invalid(format!(
                    "DOF manager '{}': step '{name}' outside a collective operation",
                    self.id
                )));
            }
        }
        // a callback may be foreign code: checked in the test-only
        // export-all mode (no native guard alive here)
        akantu_core::array_shared::export_all::callback_site(name);
        let r = f(self);
        let mut ph = self.check_out(name)?;
        let r = ph.agreed(name, r, Severity::Recoverable);
        let r = self.settle(&ph, name, r);
        self.check_in(ph);
        r
    }

    /// Agree an **already computed** outcome of the running collective
    /// operation (e.g. a native driver's result): the rank's own error is
    /// kept even if the operation already failed (then nothing is
    /// communicated). Must be called inside [`DofManager::collective`]. A
    /// typed fatal outcome follows the same rule as [`DofManager::step`].
    pub fn agree_outcome<R>(&mut self, name: &str, outcome: Result<R>) -> Result<R> {
        let mut ph = self.check_out(name)?;
        let r = ph.agreed(name, outcome, Severity::Recoverable);
        let r = self.settle(&ph, name, r);
        self.check_in(ph);
        r
    }

    /// Lend the active scope to one internal step (no callback may run
    /// inside `f`).
    fn lend<R>(&mut self, f: impl FnOnce(&mut Self, &mut Phases) -> Result<R>) -> Result<R> {
        let mut ph = match self.check_out("internal step") {
            Ok(ph) => ph,
            Err(_) => {
                return Err(Error::invalid(
                    "internal step outside a collective operation",
                ));
            }
        };
        let r = f(self, &mut ph);
        self.check_in(ph);
        r
    }

    /// Register a nodal DOF family on all mesh nodes (upstream
    /// `registerDOFs(id, array, _dst_nodal)`). Collective.
    pub fn register_nodal_dofs(
        &mut self,
        mesh: &Mesh,
        dof_id: &str,
        nb_component: usize,
    ) -> Result<()> {
        let nodes = (0..mesh.nb_nodes()).map(akantu_core::try_idx).collect();
        self.register_nodal_impl(mesh, dof_id, nb_component, nodes, None)
    }

    /// Register a nodal DOF family supported by the nodes of an element
    /// group (upstream `registerDOFs(id, array, group)`). Collective.
    pub fn register_nodal_dofs_on_group(
        &mut self,
        mesh: &Mesh,
        dof_id: &str,
        nb_component: usize,
        group: &str,
    ) -> Result<()> {
        let nodes = mesh
            .groups()
            .element_group(group)
            .map(|g| g.nodes().as_slice().to_vec());
        self.register_nodal_impl(mesh, dof_id, nb_component, nodes, Some(group.into()))
    }

    fn register_nodal_impl(
        &mut self,
        mesh: &Mesh,
        dof_id: &str,
        nb_component: usize,
        nodes: Result<Vec<Idx>>,
        group: Option<Id>,
    ) -> Result<()> {
        self.collective(&format!("register:{dof_id}"), |dm| {
            dm.lend(|dm, ph| dm.register_nodal_in(ph, mesh, dof_id, nb_component, nodes, group))
        })
    }

    fn register_nodal_in(
        &mut self,
        ph: &mut Phases,
        mesh: &Mesh,
        dof_id: &str,
        nb_component: usize,
        nodes: Result<Vec<Idx>>,
        group: Option<Id>,
    ) -> Result<()> {
        // ---- local preflight: names, inputs, capacities, allocations -----
        let pre = (|| -> Result<(DofData, BlockPlan)> {
            if self.families.contains_key(dof_id) {
                return Err(Error::AlreadyExists {
                    kind: "dof family",
                    id: dof_id.into(),
                });
            }
            let nodes = nodes?;
            akantu_core::checked_local_dofs(nodes.len(), nb_component)?;
            let plan = plan_block(mesh, None, &nodes, nb_component, self.layout.nb_local())?;
            let node_to_tuple = group
                .as_ref()
                .map(|_| nodes.iter().enumerate().map(|(t, &n)| (n, t)).collect());
            let data = DofData {
                support: DofSupportType::Nodal,
                group_support: group.clone(),
                dofs: Array::try_zeros(nodes.len(), nb_component)?.into(),
                derivatives: BTreeMap::new(),
                increment: None,
                previous: None,
                blocked: None,
                solution: Array::try_zeros(nodes.len(), nb_component)?,
                local_equation_numbers: Vec::new(),
                associated_nodes: Vec::new(),
                associated_node_gids: Vec::new(),
                node_to_tuple,
                periodic_slave: Vec::new(),
            };
            Ok((data, plan))
        })();
        let (mut data, plan) = ph.agreed("preflight", pre, Severity::Recoverable)?;
        // ---- collectives (agreed) -----------------------------------------
        let backup = self.layout.clone();
        let flags_len = self.dofs_flag.len();
        let sync = mesh.synchronizers().node.clone();
        let periodic = periodic_numbering_sync(mesh);
        // the equation numbering changes (inverse maps of the threaded
        // scatter are rebuilt)
        self.numbering_generation = self.numbering_generation.wrapping_add(1);
        let r = number_block(
            ph,
            &mut self.layout,
            &mut self.dofs_flag,
            &mut data,
            plan,
            nb_component,
            sync.as_ref(),
            periodic.as_ref(),
        );
        if let Err(e) = r {
            // Agreed failure: exact restore. Transport failure: the owner
            // exit poisons the manager.
            self.layout = backup;
            self.dofs_flag.truncate(flags_len);
            return Err(e);
        }
        self.node_sync = sync;
        self.periodic_sync = periodic;
        self.mesh_periodic = mesh.is_periodic();
        if self.numbered_distributed.is_none() {
            self.numbered_distributed = Some(mesh.is_distributed());
            self.sync_vector_kinds();
        }
        self.families.insert(dof_id.into(), data);
        self.order.push(dof_id.into());
        // The numbering is committed: a failed matrix reset is fatal
        // (agreed, the manager is poisoned on every rank).
        let reset = self.resize_global_arrays();
        ph.agree("reset", reset, Severity::Fatal)
    }

    /// Register a generic DOF family (upstream `_dst_generic`): every DOF is
    /// owned by the registering rank. Collective.
    pub fn register_generic_dofs(
        &mut self,
        dof_id: &str,
        nb_dofs: usize,
        nb_component: usize,
    ) -> Result<()> {
        self.collective(&format!("register:{dof_id}"), |dm| {
            dm.lend(|dm, ph| dm.register_generic_in(ph, dof_id, nb_dofs, nb_component))
        })
    }

    fn register_generic_in(
        &mut self,
        ph: &mut Phases,
        dof_id: &str,
        nb_dofs: usize,
        nb_component: usize,
    ) -> Result<()> {
        let first_local = self.layout.nb_local();
        let pre = (|| -> Result<(DofData, BlockPlan)> {
            if self.families.contains_key(dof_id) {
                return Err(Error::AlreadyExists {
                    kind: "dof family",
                    id: dof_id.into(),
                });
            }
            let n = akantu_core::checked_local_dofs(nb_dofs, nb_component)?;
            check_local_equations(first_local, n)?;
            let local_eqs = (first_local..first_local + n)
                .map(akantu_core::try_idx)
                .collect::<Result<Vec<_>>>()?;
            let plan = BlockPlan {
                ownership: vec![EquationOwnership::Owned; n],
                flags: vec![NodeFlag::NORMAL; n],
                nb_owned: to_u64(n)?,
                local_eqs,
                associated: Vec::new(),
                associated_gids: Vec::new(),
                periodic_slave: Vec::new(),
            };
            let data = DofData {
                support: DofSupportType::Generic,
                group_support: None,
                dofs: Array::try_zeros(nb_dofs, nb_component)?.into(),
                derivatives: BTreeMap::new(),
                increment: None,
                previous: None,
                blocked: None,
                solution: Array::try_zeros(nb_dofs, nb_component)?,
                local_equation_numbers: Vec::new(),
                associated_nodes: Vec::new(),
                associated_node_gids: Vec::new(),
                node_to_tuple: None,
                periodic_slave: Vec::new(),
            };
            Ok((data, plan))
        })();
        let (mut data, plan) = ph.agreed("preflight", pre, Severity::Recoverable)?;
        let backup = self.layout.clone();
        let flags_len = self.dofs_flag.len();
        self.numbering_generation = self.numbering_generation.wrapping_add(1);
        if let Err(e) = number_block(
            ph,
            &mut self.layout,
            &mut self.dofs_flag,
            &mut data,
            plan,
            nb_component,
            None,
            None,
        ) {
            self.layout = backup;
            self.dofs_flag.truncate(flags_len);
            return Err(e);
        }
        self.families.insert(dof_id.into(), data);
        self.order.push(dof_id.into());
        let reset = self.resize_global_arrays();
        ph.agree("reset", reset, Severity::Fatal)
    }

    /// Allocate the derivative of order `order ≥ 1` (upstream
    /// `registerDOFsDerivative`; the port owns the arrays, zero-filled).
    /// Registering an order twice is an error, as upstream.
    pub fn register_dofs_derivative(&mut self, dof_id: &str, order: usize) -> Result<()> {
        self.check_poison()?;
        if order == 0 {
            return Err(Error::invalid(
                "the derivative of order 0 is the DOF array itself",
            ));
        }
        let f = self.family_mut(dof_id)?;
        let (n, c) = (f.dofs.len(), f.dofs.nb_component());
        match f.derivatives.entry(order) {
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(Array::try_zeros(n, c)?.into());
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => Err(Error::AlreadyExists {
                kind: "dof derivative",
                id: format!("{dof_id}:{order}"),
            }),
        }
    }

    /// Allocate blocked flags (upstream `registerBlockedDOFs`); twice is
    /// an error, as upstream.
    pub fn register_blocked_dofs(&mut self, dof_id: &str) -> Result<()> {
        self.check_poison()?;
        let f = self.family_mut(dof_id)?;
        let (n, c) = (f.dofs.len(), f.dofs.nb_component());
        if f.blocked.is_some() {
            return Err(Error::AlreadyExists {
                kind: "blocked dofs",
                id: dof_id.into(),
            });
        }
        f.blocked = Some(Array::try_filled(n, c, false)?.into());
        Ok(())
    }

    /// Allocate the previous-step values (upstream `registerDOFsPrevious`);
    /// twice is an error, as upstream.
    pub fn register_dofs_previous(&mut self, dof_id: &str) -> Result<()> {
        self.check_poison()?;
        let f = self.family_mut(dof_id)?;
        let (n, c) = (f.dofs.len(), f.dofs.nb_component());
        if f.previous.is_some() {
            return Err(Error::AlreadyExists {
                kind: "previous dofs",
                id: dof_id.into(),
            });
        }
        f.previous = Some(Array::try_zeros(n, c)?.into());
        Ok(())
    }

    /// Allocate the increment (upstream `registerDOFsIncrement`); twice is
    /// an error, as upstream.
    pub fn register_dofs_increment(&mut self, dof_id: &str) -> Result<()> {
        self.check_poison()?;
        let f = self.family_mut(dof_id)?;
        let (n, c) = (f.dofs.len(), f.dofs.nb_component());
        if f.increment.is_some() {
            return Err(Error::AlreadyExists {
                kind: "dofs increment",
                id: dof_id.into(),
            });
        }
        f.increment = Some(Array::try_zeros(n, c)?.into());
        Ok(())
    }

    /// `true` if the family exists (upstream `hasDOFs`).
    #[must_use]
    pub fn has_dofs(&self, dof_id: &str) -> bool {
        self.families.contains_key(dof_id)
    }

    /// Support type of a family (upstream `getSupportType`).
    pub fn support_type(&self, dof_id: &str) -> Result<DofSupportType> {
        Ok(self.family(dof_id)?.support)
    }

    /// `true` if the derivative of order `order` exists (upstream
    /// `hasDOFsDerivatives`, without its off-by-one: upstream answers
    /// `order < derivatives.size()`, i.e. `false` for the highest
    /// registered order).
    pub fn has_dofs_derivative(&self, dof_id: &str, order: usize) -> Result<bool> {
        Ok(order == 0 || self.family(dof_id)?.derivatives.contains_key(&order))
    }

    /// `true` if blocked flags exist (upstream `hasBlockedDOFs`).
    pub fn has_blocked_dofs(&self, dof_id: &str) -> Result<bool> {
        Ok(self.family(dof_id)?.blocked.is_some())
    }

    /// `true` if an increment array exists (upstream `hasDOFsIncrement`).
    pub fn has_dofs_increment(&self, dof_id: &str) -> Result<bool> {
        Ok(self.family(dof_id)?.increment.is_some())
    }

    /// `true` if a previous-values array exists (upstream
    /// `hasPreviousDOFs`).
    pub fn has_previous_dofs(&self, dof_id: &str) -> Result<bool> {
        Ok(self.family(dof_id)?.previous.is_some())
    }

    /// Blocked flags (upstream `getBlockedDOFs`; a guard, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub fn blocked_dofs(&self, dof_id: &str) -> Result<ArrayRef<'_, bool>> {
        self.family(dof_id)?
            .blocked
            .as_ref()
            .ok_or_else(|| Error::not_found("blocked dofs", dof_id))?
            .read()
    }

    /// Mutable blocked flags.
    pub fn blocked_dofs_mut(&mut self, dof_id: &str) -> Result<ArrayMut<'_, bool>> {
        self.family_mut(dof_id)?
            .blocked
            .as_mut()
            .ok_or_else(|| Error::not_found("blocked dofs", dof_id))?
            .write()
    }

    /// Previous-step values (upstream `getPreviousDOFs`).
    pub fn previous_dofs(&self, dof_id: &str) -> Result<ArrayRef<'_, Real>> {
        self.family(dof_id)?
            .previous
            .as_ref()
            .ok_or_else(|| Error::not_found("previous dofs", dof_id))?
            .read()
    }

    /// Increment (upstream `getDOFsIncrement`).
    pub fn dofs_increment(&self, dof_id: &str) -> Result<ArrayRef<'_, Real>> {
        self.family(dof_id)?
            .increment
            .as_ref()
            .ok_or_else(|| Error::not_found("dofs increment", dof_id))?
            .read()
    }

    /// Copy the DOF values into the previous-step array (upstream
    /// `savePreviousDOFs`).
    pub fn save_previous_dofs(&mut self, dof_id: &str) -> Result<()> {
        self.check_poison()?;
        let f = self.family_mut(dof_id)?;
        let mut prev = f
            .previous
            .as_mut()
            .ok_or_else(|| Error::not_found("previous dofs", dof_id))?
            .write()?;
        let dofs = f.dofs.read()?;
        // A user may have resized `previous` through
        // `family_mut`; a length mismatch is an error, not a slice panic.
        if prev.as_slice().len() != dofs.as_slice().len() {
            return Err(Error::DimensionMismatch {
                context: "save_previous_dofs: previous dofs vs dofs",
                expected: dofs.as_slice().len(),
                got: prev.as_slice().len(),
            });
        }
        prev.as_mut_slice().copy_from_slice(dofs.as_slice());
        Ok(())
    }

    /// Solution of the last solve split on a family (upstream
    /// `getSolution(dof_id)`).
    pub fn dofs_solution(&self, dof_id: &str) -> Result<&Array<Real>> {
        Ok(&self.family(dof_id)?.solution)
    }

    /// Family storage.
    pub fn family(&self, dof_id: &str) -> Result<&DofData> {
        self.families
            .get(dof_id)
            .ok_or_else(|| Error::not_found("dof family", dof_id))
    }

    /// Mutable family storage.
    pub fn family_mut(&mut self, dof_id: &str) -> Result<&mut DofData> {
        self.families
            .get_mut(dof_id)
            .ok_or_else(|| Error::not_found("dof family", dof_id))
    }

    /// DOF values (upstream `getDOFs`; a guard, ARCHITECTURE.md, "Shared storage and its aliasing rules").
    pub fn dofs(&self, dof_id: &str) -> Result<ArrayRef<'_, Real>> {
        self.family(dof_id)?.dofs.read()
    }

    /// Mutable DOF values.
    pub fn dofs_mut(&mut self, dof_id: &str) -> Result<ArrayMut<'_, Real>> {
        self.family_mut(dof_id)?.dofs.write()
    }

    /// Derivative of order `order` (upstream `getDOFsDerivatives`; order 0
    /// is the DOF array).
    pub fn dofs_derivative(&self, dof_id: &str, order: usize) -> Result<ArrayRef<'_, Real>> {
        self.dofs_derivative_shared(dof_id, order)?.read()
    }

    /// Mutable derivative of order `order` (order 0 is the DOF array).
    pub fn dofs_derivative_mut(
        &mut self,
        dof_id: &str,
        order: usize,
    ) -> Result<ArrayMut<'_, Real>> {
        self.dofs_derivative_shared_mut(dof_id, order)?.write()
    }

    /// The exportable array of the derivative of order `order` (order 0 is
    /// the DOF array).
    pub fn dofs_derivative_shared(&self, dof_id: &str, order: usize) -> Result<&SharedArray<Real>> {
        let f = self.family(dof_id)?;
        if order == 0 {
            return Ok(&f.dofs);
        }
        f.derivatives
            .get(&order)
            .ok_or_else(|| Error::not_found("dof derivative", format!("{dof_id}:{order}")))
    }

    /// Mutable exportable array of the derivative of order `order`.
    pub fn dofs_derivative_shared_mut(
        &mut self,
        dof_id: &str,
        order: usize,
    ) -> Result<&mut SharedArray<Real>> {
        let f = self.family_mut(dof_id)?;
        if order == 0 {
            return Ok(&mut f.dofs);
        }
        f.derivatives
            .get_mut(&order)
            .ok_or_else(|| Error::not_found("dof derivative", format!("{dof_id}:{order}")))
    }

    /// Local equation numbers of a family ([`Idx`], D-012).
    pub fn local_equation_numbers(&self, dof_id: &str) -> Result<&[Idx]> {
        Ok(&self.family(dof_id)?.local_equation_numbers)
    }

    /// Global equation of each DOF of a family (`None` for pure ghosts).
    pub fn global_equation_numbers(&self, dof_id: &str) -> Result<Vec<Option<GlobalIdx>>> {
        self.local_equation_numbers(dof_id)?
            .iter()
            .map(|&l| Ok(self.layout.global(akantu_core::try_to_usize(l)?)))
            .collect()
    }

    /// Names of the registered families in registration order.
    #[must_use]
    pub fn dof_ids(&self) -> &[Id] {
        &self.order
    }

    /// Global system size.
    #[must_use]
    pub fn system_size(&self) -> GlobalIdx {
        self.layout.system_size()
    }

    /// Local system size (owned, shared and pure-ghost equations).
    #[must_use]
    pub fn local_system_size(&self) -> usize {
        self.layout.nb_local()
    }

    /// Local system size restricted to owned equations.
    #[must_use]
    pub fn pure_local_system_size(&self) -> usize {
        self.layout.nb_owned()
    }

    /// Residual vector (local equations; its values are exportable
    /// storage reached through guards, [`ResidualVector`]).
    #[must_use]
    pub fn residual(&self) -> &ResidualVector {
        &self.residual
    }

    /// The residual's exportable storage (the Python bindings' zero-copy
    /// `SolverVector.getValues()`, PY-045).
    pub fn residual_shared_mut(&mut self) -> &mut SharedArray<Real> {
        self.residual.shared_mut()
    }

    /// Solution of the last linear solve (local equations; shared rows
    /// synchronised from their owners, pure-ghost rows zero). Its kind
    /// (upstream `getSolution().isDistributed()`) is
    /// [`DofManager::vectors_distributed`].
    #[must_use]
    pub fn solution(&self) -> &[Real] {
        &self.solution
    }

    /// `true` when the manager's vectors (residual, solution, data cache
    /// and lumped matrices) are of upstream's distributed kind:
    /// upstream `DOFManagerDefault(mesh)` allocates
    /// `SolverVectorDistributed`s iff the mesh is distributed
    /// (`dof_manager_default.cc:52-73, 157-163`; upstream marks a mesh
    /// distributed by `distribute()` even on one process), and
    /// `DOFManagerPETSc` always allocates `SolverVectorPETSc`s, which are
    /// distributed (`dof_manager_petsc.cc:287-289, 446-448`). The port's
    /// manager binds to the mesh at its first nodal registration (parity
    /// row DOF-013), so the default manager's vectors are of the
    /// non-distributed kind until then and for a manager with generic DOFs
    /// only. The kind selects the collective finiteness check and the
    /// rank-suffixed file names of the vector dumps
    /// ([`SolverVector::is_finite_in`], [`SolverVector::save_vector`]).
    #[must_use]
    pub fn vectors_distributed(&self) -> bool {
        self.backend.kind() == BackendKind::Petsc || self.numbered_distributed == Some(true)
    }

    /// Give the residual and every lumped matrix the manager's kind
    /// ([`DofManager::vectors_distributed`]); identical on every rank (the
    /// mesh's distribution state is).
    fn sync_vector_kinds(&mut self) {
        let distributed = self.vectors_distributed();
        self.residual.set_distributed(distributed);
        for v in self.lumped.values_mut() {
            v.set_distributed(distributed);
        }
    }

    /// Upstream `resizeGlobalArrays`: global vectors follow the local
    /// system size and **every** matrix loses its profile and values
    /// (`clearProfile`), so no obsolete coupling or first-insertion order
    /// survives a numbering change; profiles are rebuilt lazily by
    /// the next assembly.
    fn resize_global_arrays(&mut self) -> Result<()> {
        let n = self.layout.nb_local();
        self.residual.resize(n);
        self.solution.resize(n, 0.0);
        self.data_cache.resize(n, 0.0);
        for v in self.lumped.values_mut() {
            v.values.resize(n, 0.0);
        }
        // block couplings name local equations: stale after a renumbering
        self.lumped_blocks.clear();
        self.reset_all_profiles()
    }

    fn reset_all_profiles(&mut self) -> Result<()> {
        let size = self.layout.system_size();
        for id in self.backend.matrix_ids() {
            self.backend.reset_profile(&id, size)?;
        }
        self.profiled.clear();
        Ok(())
    }

    /// Create a matrix (upstream `getNewMatrix(id, type)`).
    pub fn new_matrix(&mut self, matrix_id: &str, matrix_type: MatrixType) -> Result<()> {
        self.check_poison()?;
        self.backend
            .new_matrix(matrix_id, matrix_type, self.layout.system_size())?;
        self.declared.insert(matrix_id.into(), matrix_type);
        Ok(())
    }

    /// Create a matrix derived from `sources` (upstream `J` from `K`, `M`,
    /// `C`, `TimeStepSolver::assembleMatrix`): its type is the common
    /// (minimum) effective type of the sources, and an agreed promotion of
    /// any source is propagated to it.
    pub fn new_derived_matrix(&mut self, matrix_id: &str, sources: &[&str]) -> Result<()> {
        self.check_poison()?;
        let mut ty = MatrixType::NotDefined;
        for s in sources {
            ty = ty.min(self.backend.matrix_type(s)?);
        }
        if ty == MatrixType::NotDefined {
            return Err(Error::invalid("a derived matrix needs at least one source"));
        }
        self.new_matrix(matrix_id, ty)?;
        for s in sources {
            self.derived
                .entry((*s).into())
                .or_default()
                .push(matrix_id.into());
        }
        Ok(())
    }

    /// COO matrix (Python `getMatrix`; AIJ backend only).
    pub fn matrix(&self, matrix_id: &str) -> Result<&SparseMatrixAij> {
        self.backend.aij(matrix_id)
    }

    /// Effective symmetry of a matrix.
    pub fn matrix_type(&self, matrix_id: &str) -> Result<MatrixType> {
        self.backend.matrix_type(matrix_id)
    }

    /// Create a lumped matrix (upstream `getNewLumpedMatrix`).
    pub fn new_lumped_matrix(&mut self, matrix_id: &str) -> Result<&mut SolverVector> {
        self.check_poison()?;
        if self.lumped.contains_key(matrix_id) {
            return Err(Error::AlreadyExists {
                kind: "lumped matrix",
                id: matrix_id.into(),
            });
        }
        // upstream `getNewLumpedMatrix`: a vector of the manager's kind
        let mut v = SolverVector::zeros(self.layout.nb_local());
        v.set_distributed(self.vectors_distributed());
        Ok(self.lumped.entry(matrix_id.into()).or_insert(v))
    }

    /// Tuple of node `n` in family `f` (checked).
    fn tuple_of(f: &DofData, n: Idx) -> Result<usize> {
        let t = match &f.node_to_tuple {
            None => akantu_core::try_to_usize(n)?,
            Some(m) => *m
                .get(&n)
                .ok_or_else(|| Error::Mesh(format!("node {n} does not support this family")))?,
        };
        if t >= f.dofs.len() {
            return Err(Error::OutOfRange {
                what: "dof node",
                index: t,
                len: f.dofs.len(),
            });
        }
        Ok(t)
    }

    /// Global equations of one element (pure ghosts mapped past the matrix
    /// order so the `< size` rule skips them).
    fn element_equations(&self, f: &DofData, conn: &[Idx], out: &mut Vec<GlobalIdx>) -> Result<()> {
        let nbc = f.dofs.nb_component();
        out.clear();
        for &n in conn {
            let t = Self::tuple_of(f, n)?;
            for c in 0..nbc {
                let l = f.eq(t * nbc + c)?;
                out.push(self.layout.global(l).unwrap_or(GlobalIdx::MAX));
            }
        }
        Ok(())
    }

    /// Element list of an assembly (filter, group or all), validated
    /// against the connectivity length.
    fn element_list(
        mesh: &Mesh,
        f: &DofData,
        filter: Option<&[Idx]>,
        ty: ElementType,
        ghost: GhostType,
        nb: usize,
    ) -> Result<Vec<usize>> {
        let raw: Vec<Idx> = match (filter, &f.group_support) {
            (Some(fl), _) => fl.to_vec(),
            (None, Some(g)) => mesh
                .groups()
                .element_group(g)?
                .elements_of(ty, ghost)
                .to_vec(),
            (None, None) => return Ok((0..nb).collect()),
        };
        raw.into_iter()
            .map(|e| {
                let u = akantu_core::try_to_usize(e)?;
                if u < nb {
                    Ok(u)
                } else {
                    Err(Error::OutOfRange {
                        what: "element",
                        index: u,
                        len: nb,
                    })
                }
            })
            .collect()
    }

    /// Assemble per-element matrices (upstream
    /// `assembleElementalMatricesToMatrix`); `elementary` holds one
    /// column-major `(nnodes·nbc)²` matrix per element of the filter (or of
    /// the family's group, or of all elements). Inputs (filter, group
    /// elements, connectivity nodes, sizes) are validated before any
    /// backend change.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn assemble_elemental_matrices_to_matrix(
        &mut self,
        mesh: &Mesh,
        matrix_id: &str,
        dof_id: &str,
        elementary: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        elemental_matrix_type: MatrixType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_poison()?;
        let f = self
            .families
            .get(dof_id)
            .ok_or_else(|| Error::not_found("dof family", dof_id))?;
        if !self.backend.has_matrix(matrix_id) {
            return Err(Error::not_found("matrix", matrix_id));
        }
        // Zero-work ranks (no element of this type) participate with an
        // empty contribution.
        if mesh.nb_element(ty, ghost) == 0 && elementary.is_empty() {
            return Ok(());
        }
        let conn = mesh.connectivity(ty, ghost)?;
        let nnodes = conn.nb_component();
        let elements = Self::element_list(mesh, f, filter, ty, ghost, conn.len())?;
        let size_mat = nnodes
            .checked_mul(f.dofs.nb_component())
            .ok_or_else(|| Error::invalid("elemental matrix size overflow"))?;
        let expected_comp = size_mat
            .checked_mul(size_mat)
            .ok_or_else(|| Error::invalid("elemental matrix size overflow"))?;
        if elementary.len() != elements.len() || elementary.nb_component() != expected_comp {
            return Err(Error::DimensionMismatch {
                context: "assemble_elemental_matrices_to_matrix",
                expected: elements.len(),
                got: elementary.len(),
            });
        }
        // Validate every element's equations before touching the backend.
        let mut eqs = Vec::with_capacity(size_mat);
        let key = (matrix_id.to_owned(), dof_id.to_owned(), ty, ghost);
        let profile_elems: Vec<usize> = if self.profiled.contains(&key) {
            Vec::new()
        } else {
            Self::element_list(mesh, f, None, ty, ghost, conn.len())?
        };
        for &e in elements.iter().chain(&profile_elems) {
            self.element_equations(f, conn.tuple(e), &mut eqs)?;
        }
        // addToProfile once per (matrix, family, type, ghost), all elements
        // of the connectivity/group in order.
        for &e in &profile_elems {
            self.element_equations(f, conn.tuple(e), &mut eqs)?;
            self.backend.add_block_to_profile(matrix_id, &eqs, &eqs)?;
        }
        if !profile_elems.is_empty() || !self.profiled.contains(&key) {
            self.profiled.insert(key);
        }
        let f = &self.families[dof_id];
        for (k, &e) in elements.iter().enumerate() {
            self.element_equations(f, conn.tuple(e), &mut eqs)?;
            self.backend.add_values(
                matrix_id,
                &eqs,
                &eqs,
                elementary.tuple(k),
                elemental_matrix_type,
            )?;
        }
        Ok(())
    }

    /// Zero a matrix, keeping its profile (upstream `zeroMatrix`).
    pub fn backend_zero(&mut self, matrix_id: &str) -> Result<()> {
        self.check_poison()?;
        self.backend.zero(matrix_id)
    }

    /// Collective end of an assembly phase of `matrix_id` (called by the
    /// time-step solver after `assemble_matrix`): agrees the effective
    /// symmetry on every rank and propagates a promotion to the derived
    /// matrices, then finalises the backend storage (PETSc
    /// `MatAssemblyEnd`). A promotion failure after other ranks (or other
    /// matrices) were promoted is **fatal**: agreed with
    /// [`Severity::Fatal`], the manager is poisoned on every rank and the
    /// error is [`Error::Fatal`] (backends promote atomically per
    /// matrix, but the set of matrices across ranks cannot be rolled back).
    pub fn finalize_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.collective(&format!("finalize:{matrix_id}"), |dm| {
            dm.lend(|dm, ph| dm.finalize_in(ph, matrix_id))
        })
    }

    fn finalize_in(&mut self, ph: &mut Phases, matrix_id: &str) -> Result<()> {
        let local = self.backend.matrix_type(matrix_id).map(|t| {
            t == MatrixType::Unsymmetric
                && self.declared.get(matrix_id) == Some(&MatrixType::Symmetric)
        });
        let promote = ph.run("symmetry", local, |c, promoted| {
            let mut v = [u8::from(promoted)];
            c.all_reduce(&mut v, ReduceOp::Max)?;
            Ok(Ok(v[0] == 1))
        })?;
        if promote {
            let mut todo = vec![matrix_id.to_owned()];
            let mut seen = BTreeSet::new();
            let mut promoted = Ok(());
            while let Some(id) = todo.pop() {
                if !seen.insert(id.clone()) {
                    continue;
                }
                if let Err(e) = self.backend.promote_to_unsymmetric(&id) {
                    promoted = Err(e);
                    break;
                }
                todo.extend(self.derived.get(&id).into_iter().flatten().cloned());
            }
            ph.agree("promote", promoted, Severity::Fatal)?;
        }
        let mut ctx = BackendContext {
            layout: &self.layout,
            phases: ph,
        };
        self.backend.finalize_assembly(matrix_id, &mut ctx)
    }

    /// Assemble pre-assembled terms (upstream `assemblePreassembledMatrix`):
    /// local DOF indices of families `m`/`n` → global equations. Every term
    /// is validated (negative or out-of-range DOF, pure-ghost DOF) before
    /// the backend is touched.
    pub fn assemble_preassembled_matrix(
        &mut self,
        matrix_id: &str,
        terms: &TermsToAssemble,
    ) -> Result<()> {
        self.check_poison()?;
        let fm = self.family(&terms.dof_id_m)?;
        let fn_ = self.family(&terms.dof_id_n)?;
        let mut out = Vec::with_capacity(terms.terms.len());
        for t in &terms.terms {
            let li = fm.eq(akantu_core::try_to_usize(t.i)?)?;
            let lj = fn_.eq(akantu_core::try_to_usize(t.j)?)?;
            match (self.layout.global(li), self.layout.global(lj)) {
                (Some(gi), Some(gj)) => out.push((gi, gj, t.value)),
                _ => return Err(Error::invalid("pre-assembled term on a pure-ghost DOF")),
            }
        }
        if !self.backend.has_matrix(matrix_id) {
            return Err(Error::not_found("matrix", matrix_id));
        }
        self.backend.add_terms(matrix_id, &out)
    }

    /// `y = beta·y + alpha·A·x` over local equations with the distributed
    /// reduction of this manager (UD-003-corrected). Collective: lengths
    /// and the matrix are agreed first, and the reduction runs as phases of
    /// the same scope, so a backend failing locally before it reaches the
    /// reduction is reported on every rank.
    pub fn mat_vec(
        &mut self,
        mesh: &Mesh,
        matrix_id: &str,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        self.collective(&format!("mat_vec:{matrix_id}"), |dm| {
            let n = dm.layout.nb_local();
            let pre = check_len("x", x.len(), n)
                .and_then(|()| check_len("y", y.len(), n))
                .and_then(|()| dm.check_matrix(matrix_id));
            dm.lend(|dm, ph| {
                ph.agreed("prepare", pre, Severity::Recoverable)?;
                let red = DofReduction {
                    dm,
                    sync: mesh.synchronizers().node.as_ref(),
                    periodic: dm.periodic_sync.as_ref(),
                    phases: RefCell::new(ph),
                };
                dm.backend
                    .mat_vec(matrix_id, &dm.layout, &red, x, y, alpha, beta)
            })
        })
    }

    /// Solve `A x = rhs` through the backend (collective):
    /// `rhs`/`x` over local equations, owned rows of `rhs` complete; on
    /// return the owned rows of `x` hold the solution and the shared rows
    /// are synchronised from their owners (pure-ghost rows are zero). Vector
    /// lengths and the matrix are validated locally and agreed before the
    /// backend communicates.
    pub fn solve_linear_system(
        &mut self,
        matrix_id: &str,
        rhs: &[Real],
        x: &mut [Real],
    ) -> Result<()> {
        self.collective(&format!("solve:{matrix_id}"), |dm| {
            let n = dm.layout.nb_local();
            let pre = check_len("rhs", rhs.len(), n)
                .and_then(|()| check_len("solution", x.len(), n))
                .and_then(|()| dm.check_matrix(matrix_id));
            dm.lend(|dm, ph| {
                ph.agreed("prepare", pre, Severity::Recoverable)?;
                x.fill(0.0);
                // D-041 (4): the manager-wide switches reach the solver of
                // this solve (whatever allocated it).
                dm.backend
                    .set_sparse_solver_compat(dm.compat.sparse_solver());
                let mut ctx = BackendContext {
                    layout: &dm.layout,
                    phases: &mut *ph,
                };
                dm.backend.solve(matrix_id, &mut ctx, rhs, x)?;
                dm.synchronize_shared_rows(ph, x)
            })
        })
    }

    fn check_matrix(&self, matrix_id: &str) -> Result<()> {
        if self.backend.has_matrix(matrix_id) {
            Ok(())
        } else {
            Err(Error::not_found("matrix", matrix_id))
        }
    }

    /// Solve with the residual as right-hand side into the manager's
    /// solution, then split it per family (upstream `solve` +
    /// `splitSolutionPerDOFs`). Collective.
    pub fn solve(&mut self, matrix_id: &str) -> Result<()> {
        self.collective(&format!("solve_residual:{matrix_id}"), |dm| {
            let rhs = dm.residual.values().to_vec();
            let mut x = std::mem::take(&mut dm.solution);
            x.resize(dm.layout.nb_local(), 0.0);
            let r = dm.solve_linear_system(matrix_id, &rhs, &mut x);
            dm.solution = x;
            r?;
            let sol = dm.solution.clone();
            dm.split_solution_per_dofs(&sol)
        })
    }

    /// Set the solution from a trial vector whose **owned** rows are
    /// meaningful (native solvers): shared rows are synchronised from their
    /// owners, then the solution is split per family. Collective; the
    /// length is validated locally and agreed before any exchange.
    pub fn set_solution(&mut self, x: &[Real]) -> Result<()> {
        self.collective("set_solution", |dm| {
            let pre = check_len("set_solution", x.len(), dm.layout.nb_local()).map(|()| {
                x.iter()
                    .enumerate()
                    .map(|(l, &xi)| if dm.layout.is_owned(l) { xi } else { 0.0 })
                    .collect::<Vec<Real>>()
            });
            let v = dm.lend(|dm, ph| {
                let mut v = ph.agreed("prepare", pre, Severity::Recoverable)?;
                dm.synchronize_shared_rows(ph, &mut v)?;
                Ok(v)
            })?;
            dm.solution = v;
            let sol = dm.solution.clone();
            dm.split_solution_per_dofs(&sol)
        })
    }

    /// Owner → copies for shared rows of a local-equation vector (one
    /// agreed exchange phase of `ph`); on a distributed periodic mesh also
    /// owner → copies of the periodic masters through the periodic node
    /// synchronizer (upstream's `makeConsistentForPeriodicity(_id)` at the
    /// end of `getArrayPerDOFs`; the local periodic slaves alias their
    /// master's equation, so the split per family gives them the value).
    fn synchronize_shared_rows(&self, ph: &mut Phases, v: &mut [Real]) -> Result<()> {
        if self.node_sync.is_none() && self.periodic_sync.is_none() {
            return Ok(());
        }
        let eqs = self.node_equations();
        if let Some(sync) = &self.node_sync {
            let mut acc = NodeVector::copy(&eqs, &mut *v);
            sync.synchronize_in(ph, &mut acc, SynchronizationTag::SolverSolution)?;
        }
        if let Some(periodic) = &self.periodic_sync {
            let mut acc = NodeVector::copy(&eqs, v);
            periodic.synchronize_in(ph, &mut acc, SynchronizationTag::SolverSolution)?;
        }
        Ok(())
    }

    /// Local equations of every node (infallible: local equations are
    /// non-negative by construction, D-012).
    fn node_equations(&self) -> HashMap<Idx, Vec<usize>> {
        let mut eqs: HashMap<Idx, Vec<usize>> = HashMap::new();
        for id in &self.order {
            let f = &self.families[id];
            for (&n, &e) in f.associated_nodes.iter().zip(&f.local_equation_numbers) {
                if let Ok(e) = usize::try_from(e) {
                    eqs.entry(n).or_default().push(e);
                }
            }
        }
        eqs
    }

    /// Euclidean norm over **owned** rows of a local-equation vector,
    /// reduced in rank order (collective, deterministic). The length is
    /// validated locally and agreed before the reduction.
    pub fn owned_norm(&mut self, v: &[Real]) -> Result<Real> {
        self.collective("norm", |dm| {
            let pre = check_len("owned_norm", v.len(), dm.layout.nb_local()).map(|()| {
                [v.iter()
                    .enumerate()
                    .filter(|(l, _)| dm.layout.is_owned(*l))
                    .map(|(_, x)| x * x)
                    .sum::<Real>()]
            });
            dm.lend(|_, ph| {
                ph.run_collective("sum", pre, |c, mut s| {
                    c.all_reduce(&mut s, ReduceOp::Sum)?;
                    Ok(s[0].sqrt())
                })
            })
        })
    }

    /// Rank-ordered dot product of two local-equation vectors over the
    /// owned rows (collective; lengths agreed first). Used by the TAO
    /// objective.
    pub fn owned_dot(&mut self, a: &[Real], b: &[Real]) -> Result<Real> {
        self.collective("dot", |dm| {
            let n = dm.layout.nb_local();
            let pre = check_len("owned_dot (a)", a.len(), n)
                .and_then(|()| check_len("owned_dot (b)", b.len(), n))
                .map(|()| {
                    [a.iter()
                        .zip(b)
                        .enumerate()
                        .filter(|(l, _)| dm.layout.is_owned(*l))
                        .map(|(_, (x, y))| x * y)
                        .sum::<Real>()]
                });
            dm.lend(|_, ph| {
                ph.run_collective("sum", pre, |c, mut s| {
                    c.all_reduce(&mut s, ReduceOp::Sum)?;
                    Ok(s[0])
                })
            })
        })
    }

    /// Number of owned rows `l` of two local-equation vectors where
    /// `pred(a[l], b[l])` holds, summed over the ranks (collective; lengths
    /// agreed first; joins a running operation). The UD-1022
    /// check of a bounded TAO solve.
    pub(crate) fn owned_count(
        &mut self,
        name: &str,
        a: &[Real],
        b: &[Real],
        pred: impl Fn(Real, Real) -> bool,
    ) -> Result<u64> {
        self.collective(name, |dm| {
            let n = dm.layout.nb_local();
            let pre = check_len("owned_count (a)", a.len(), n)
                .and_then(|()| check_len("owned_count (b)", b.len(), n))
                .map(|()| {
                    let n = a
                        .iter()
                        .zip(b)
                        .enumerate()
                        .filter(|&(l, (&x, &y))| dm.layout.is_owned(l) && pred(x, y))
                        .count();
                    [u64::try_from(n).unwrap_or(u64::MAX)]
                });
            dm.lend(|_, ph| {
                ph.run_collective("sum", pre, |c, mut s| {
                    c.all_reduce(&mut s, ReduceOp::Sum)?;
                    Ok(s[0])
                })
            })
        })
    }

    /// One collective operation handing the backend and a
    /// [`BackendContext`] of this manager's scope to `f` (native drivers);
    /// joins a running operation, outcome agreed.
    pub(crate) fn backend_collective<R>(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut dyn LinearSystemBackend, &mut BackendContext<'_>) -> Result<R>,
    ) -> Result<R> {
        self.collective(name, |dm| {
            dm.lend(|dm, ph| {
                let mut ctx = BackendContext {
                    layout: &dm.layout,
                    phases: ph,
                };
                f(dm.backend.as_mut(), &mut ctx)
            })
        })
    }

    /// `true` while a collective operation of this manager is running
    /// (solver callbacks then run as [`DofManager::step`]s).
    #[must_use]
    pub fn in_collective(&self) -> bool {
        !matches!(self.active, ScopeSlot::Idle)
    }

    /// Convergence norm of the Newton–Raphson solvers (upstream
    /// `NonLinearSolverNewtonRaphson::testConvergence`): `sqrt(Σ v²)` over
    /// the local equations that are not blocked in the last snapshot and
    /// are local-or-master (`isLocalOrMasterDOF`), accumulated in local
    /// order from `0.0`, all-reduced (rank order) before the square root.
    /// Collective; the result is identical on every rank.
    pub fn convergence_norm(&mut self, of: ConvergenceVector) -> Result<Real> {
        self.collective("convergence_norm", |dm| {
            let blocked = dm.blocked_snapshot();
            // the residual's guard ends before `lend` needs `dm`
            let pre = {
                let residual;
                let v: &[Real] = match of {
                    ConvergenceVector::Residual => {
                        residual = dm.residual.values();
                        &residual
                    }
                    ConvergenceVector::Solution => &dm.solution,
                };
                check_len("convergence_norm", v.len(), dm.layout.nb_local()).map(|()| {
                    let mut norm = 0.0;
                    for (n, &x) in v.iter().enumerate() {
                        let local = dm
                            .dofs_flag
                            .get(n)
                            .is_some_and(|f| f.bits() & NodeFlag::LOCAL_MASTER_MASK.bits() == 0);
                        if !blocked[n] && local {
                            norm += x * x;
                        }
                    }
                    [norm]
                })
            };
            dm.lend(|_, ph| {
                ph.run_collective("sum", pre, |c, mut s| {
                    c.all_reduce(&mut s, ReduceOp::Sum)?;
                    Ok(s[0].sqrt())
                })
            })
        })
    }

    /// Split a local solution vector into the families' `solution` arrays
    /// (upstream `splitSolutionPerDOFs`).
    pub fn split_solution_per_dofs(&mut self, x: &[Real]) -> Result<()> {
        self.check_poison()?;
        if x.len() != self.layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "split_solution_per_dofs",
                expected: self.layout.nb_local(),
                got: x.len(),
            });
        }
        let exec = &self.exec;
        for f in self.families.values_mut() {
            // a gather: every DOF reads its own equation (D-123)
            let n = (f.solution.as_slice().len()).min(f.local_equation_numbers.len());
            let items = (
                akantu_core::exec::Tuples::new(&mut f.solution.as_mut_slice()[..n], 1),
                akantu_core::exec::TuplesRef::new(&f.local_equation_numbers[..n], 1),
            );
            exec.try_for_each(items, 262_144, |_, (mut s, eqs)| {
                gather_part(s.as_mut_slice(), eqs.as_slice(), x)
            })?;
        }
        Ok(())
    }

    /// Mutable residual values (local equations): an exclusive guard of
    /// the exportable storage (`as_mut_slice()` for the flat values).
    pub fn residual_mut(&mut self) -> ArrayMut<'_, Real> {
        self.residual.values_mut()
    }

    /// Zero the residual.
    pub fn zero_residual(&mut self) {
        self.residual.zero();
    }

    /// Blocked flags per local equation (upstream `global_blocked_dofs`).
    pub fn blocked_equations(&self) -> Vec<bool> {
        let mut b = vec![false; self.layout.nb_local()];
        for f in self.families.values() {
            if let Some(bl) = &f.blocked {
                let bl = bl.borrow();
                for (&eq, &is) in f.local_equation_numbers.iter().zip(bl.as_slice()) {
                    if let Some(slot) = usize::try_from(eq).ok().and_then(|e| b.get_mut(e)) {
                        *slot |= is;
                    }
                }
            }
        }
        b
    }

    /// Add DOFs for nodes appended to the mesh (upstream
    /// `DOFManager::onNodesAdded`): families in lexicographic id order,
    /// group families restricted to their group's nodes; DOF arrays grow with
    /// zeros (values of copied nodes are set by the model afterwards,
    /// upstream `SolidMechanicsModelCohesive::onNodesAdded`). Collective;
    /// see [`DofManager::on_nodes_added_phased`].
    pub fn on_nodes_added(&mut self, mesh: &Mesh, nodes: &[Idx]) -> Result<()> {
        self.collective("nodes_added", |dm| {
            dm.lend(|dm, ph| dm.on_nodes_added_in(mesh, nodes, ph))
        })
    }

    /// [`DofManager::on_nodes_added`] as phases of an enclosing collective
    /// operation (a mesh mutation): local preflight of every family
    /// (validated nodes, per-family and aggregate capacities, reserved
    /// memory) agreed before any numbering collective; an agreed failure
    /// leaves every family and the layout unchanged. A
    /// failure after the numbering was committed (array commit, matrix
    /// reset) is agreed as fatal and poisons the manager on every rank; a
    /// transport failure poisons it locally.
    pub fn on_nodes_added_phased(
        &mut self,
        mesh: &Mesh,
        nodes: &[Idx],
        ph: &mut Phases,
    ) -> Result<()> {
        self.check_poison()?;
        let r = self.on_nodes_added_in(mesh, nodes, ph);
        self.settle(ph, "nodes_added", r)
    }

    fn on_nodes_added_in(&mut self, mesh: &Mesh, nodes: &[Idx], ph: &mut Phases) -> Result<()> {
        // ---- local preflight ------------------------------------------------
        let pre = self.plan_nodes_added(mesh, nodes);
        let plans = ph.agreed("dof:nodes_added:preflight", pre, Severity::Recoverable)?;
        // ---- collectives per family (lexicographic), then commit ----------
        let backup = GrowthBackup {
            layout: self.layout.clone(),
            flags_len: self.dofs_flag.len(),
            lens: plans
                .iter()
                .filter_map(|(id, _, _)| {
                    self.families
                        .get(id)
                        .map(|f| (id.clone(), f.dofs.len(), f.local_equation_numbers.len()))
                })
                .collect(),
        };
        let sync = mesh.synchronizers().node.clone();
        let periodic = periodic_numbering_sync(mesh);
        let mut numbered = Vec::new();
        self.numbering_generation = self.numbering_generation.wrapping_add(1);
        let r = (|| -> Result<()> {
            for (id, selected, plan) in plans {
                let data = self
                    .families
                    .get_mut(&id)
                    .ok_or_else(|| Error::not_found("dof family", id.clone()))?;
                let nbc = data.dofs.nb_component();
                number_block(
                    ph,
                    &mut self.layout,
                    &mut self.dofs_flag,
                    data,
                    plan,
                    nbc,
                    sync.as_ref(),
                    periodic.as_ref(),
                )?;
                numbered.push((id, selected));
            }
            Ok(())
        })();
        if let Err(e) = r {
            self.restore(backup);
            return Err(e);
        }
        // Commit DOF arrays (memory reserved in the preflight), then reset
        // every matrix: failures here are fatal (agreed).
        let commit = (|| -> Result<()> {
            for (id, selected) in numbered {
                if let Some(data) = self.families.get_mut(&id) {
                    let old = data.dofs.len();
                    let new_len = old + selected.len();
                    if let Some(map) = &mut data.node_to_tuple {
                        for (k, &n) in selected.iter().enumerate() {
                            map.insert(n, old + k);
                        }
                    }
                    data.solution.try_resize(new_len, 0.0)?;
                    for a in data.shared_arrays_mut() {
                        a.write()?.try_resize(new_len, 0.0)?;
                    }
                    if let Some(b) = &mut data.blocked {
                        b.write()?.try_resize(new_len, false)?;
                    }
                }
            }
            self.node_sync = sync;
            self.periodic_sync = periodic;
            self.mesh_periodic = mesh.is_periodic();
            self.resize_global_arrays()
        })();
        ph.agree("dof:nodes_added:commit", commit, Severity::Fatal)
    }

    /// Local preflight of a node addition: per family (lexicographic) the
    /// selected nodes and the numbering plan; validates node indices and
    /// group support, checks per-family and aggregate capacities and
    /// reserves the growth of every array (no observable change).
    fn plan_nodes_added(
        &mut self,
        mesh: &Mesh,
        nodes: &[Idx],
    ) -> Result<Vec<(Id, Vec<Idx>, BlockPlan)>> {
        self.check_poison()?;
        for &n in nodes {
            check_node(mesh, n)?;
        }
        let mut first_local = self.layout.nb_local();
        let mut plans = Vec::new();
        for (id, data) in &self.families {
            if data.support != DofSupportType::Nodal {
                continue;
            }
            let selected: Vec<Idx> = match &data.group_support {
                None => nodes.to_vec(),
                Some(g) => {
                    let grp: std::collections::HashSet<Idx> = mesh
                        .groups()
                        .element_group(g)?
                        .nodes()
                        .iter()
                        .copied()
                        .collect();
                    nodes.iter().copied().filter(|n| grp.contains(n)).collect()
                }
            };
            let nbc = data.dofs.nb_component();
            let new_len = data
                .dofs
                .len()
                .checked_add(selected.len())
                .ok_or_else(|| Error::invalid("dof count overflow"))?;
            akantu_core::checked_local_dofs(new_len, nbc)?;
            let plan = plan_block(mesh, Some(data), &selected, nbc, first_local)?;
            first_local = check_local_equations(first_local, plan.ownership.len())?;
            plans.push((id.clone(), selected, plan));
        }
        // Reserve every growing array (allocation failures are recoverable
        // here, impossible at commit).
        for (id, selected, plan) in &plans {
            if let Some(data) = self.families.get_mut(id) {
                data.solution.try_reserve(selected.len())?;
                for a in data.shared_arrays_mut() {
                    a.write()?.try_reserve(selected.len())?;
                }
                if let Some(b) = &mut data.blocked {
                    b.write()?.try_reserve(selected.len())?;
                }
                data.local_equation_numbers
                    .try_reserve(plan.local_eqs.len())
                    .map_err(|_| Error::invalid("cannot reserve local equations"))?;
                data.associated_nodes
                    .try_reserve(plan.associated.len())
                    .map_err(|_| Error::invalid("cannot reserve associated nodes"))?;
                data.associated_node_gids
                    .try_reserve(plan.associated_gids.len())
                    .map_err(|_| Error::invalid("cannot reserve associated nodes"))?;
            }
        }
        Ok(plans)
    }

    fn restore(&mut self, backup: GrowthBackup) {
        self.numbering_generation = self.numbering_generation.wrapping_add(1);
        self.layout = backup.layout;
        self.dofs_flag.truncate(backup.flags_len);
        for (id, _, eqs) in backup.lens {
            if let Some(f) = self.families.get_mut(&id) {
                f.local_equation_numbers.truncate(eqs);
                f.associated_nodes.truncate(eqs);
                f.associated_node_gids.truncate(eqs);
                if f.periodic_slave.len() > eqs {
                    f.periodic_slave.truncate(eqs);
                }
            }
        }
    }
}

/// `s[k] = x[eq[k]]` of one part (no-alias parameters, not inlined into the
/// part closure).
#[inline(never)]
fn gather_part(s: &mut [Real], eqs: &[Idx], x: &[Real]) -> Result<()> {
    for (s, &eq) in s.iter_mut().zip(eqs) {
        let e = akantu_core::try_to_usize(eq)?;
        // the error is built only on failure (`ok_or` built and dropped one
        // per equation)
        let Some(&v) = x.get(e) else {
            return Err(Error::OutOfRange {
                what: "local equation",
                index: e,
                len: x.len(),
            });
        };
        *s = v;
    }
    Ok(())
}

impl MeshEventHandler for DofManager {
    fn event_priority(&self) -> EventPriority {
        EventPriority::DOF_MANAGER
    }

    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.collective("mesh_event", |dm| {
            dm.lend(|dm, ph| dm.mesh_event_in(mesh, event, ph))
        })
    }

    /// The dispatch refusals that do not need communication, before the
    /// mesh commit: a poisoned manager refuses
    /// every event (its dispatch would fail on its poison), and a manager
    /// whose DOFs were numbered on the undistributed mesh refuses
    /// `MeshIsDistributed` (upstream `onMeshIsDistributed`). Node events
    /// whose numbering would read the final mesh at indices a later node
    /// event of the same mutation renumbers are refused (SMM-015,
    /// `node_removal` module docs, composition); the renumbering itself and
    /// profile resets cannot be refused.
    fn check_mesh_event(&self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        self.check_poison()?;
        match event {
            MeshEvent::MeshIsDistributed { .. } => self.distribution_consistency(mesh),
            MeshEvent::NodesAdded { .. } | MeshEvent::NodesRemoved { .. } => {
                self.check_node_event(mesh, event)
            }
            _ => Ok(()),
        }
    }

    /// Node additions and removals renumber (collective, phase-structured);
    /// any connectivity change resets every matrix profile, agreed as fatal
    /// (a manager whose matrices were reset on some ranks only is
    /// unusable: poisoned on every rank).
    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &MeshEvent,
        _priority: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        self.check_poison()?;
        let r = self.mesh_event_in(mesh, event, phases);
        self.settle(phases, "mesh_event", r)
    }
}

impl DofManager {
    fn mesh_event_in(&mut self, mesh: &Mesh, event: &MeshEvent, ph: &mut Phases) -> Result<()> {
        match event {
            MeshEvent::NodesAdded { nodes, .. } => self.on_nodes_added_in(mesh, nodes, ph),
            // SMM-015 / UD-209: upstream's `onNodesRemoved` is empty; the
            // port renumbers (`node_removal`).
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                self.on_nodes_removed_in(mesh, new_numbering, ph)
            }
            MeshEvent::ElementsAdded { .. }
            | MeshEvent::ElementsRemoved { .. }
            | MeshEvent::ElementsChanged { .. } => {
                let r = self.reset_all_profiles();
                ph.agree("dof:reset_profiles", r, Severity::Fatal)
            }
            // Upstream `onMeshIsDistributed`: a manager whose DOFs were
            // numbered on the undistributed mesh cannot follow the
            // distribution (`test_dof_mesh_distribute`). The port binds the
            // manager to the mesh at its first nodal registration (upstream:
            // at construction), so a manager without nodal DOFs accepts it.
            MeshEvent::MeshIsDistributed { .. } => {
                let r = self.distribution_consistency(mesh);
                ph.agree("dof:mesh_is_distributed", r, Severity::Recoverable)
            }
            _ => Ok(()),
        }
    }

    /// Upstream `onMeshIsDistributed`'s refusal (shared by the dispatch and
    /// the capability check).
    fn distribution_consistency(&self, mesh: &Mesh) -> Result<()> {
        match self.numbered_distributed {
            Some(false) if mesh.is_distributed() => Err(Error::invalid(format!(
                "There is an inconsistency about the distribution state of the `DOFManager` \
                 '{}'. It seams that the `Mesh` is distributed, but the `DOFManager`'s residual \
                 is not distributed",
                self.id
            ))),
            _ => Ok(()),
        }
    }
}

/// The periodic node synchronizer that carries the global numbers of the
/// periodic masters (upstream `getPeriodicNodeSynchronizer()` when the mesh
/// is distributed and periodic).
fn periodic_numbering_sync(mesh: &Mesh) -> Option<Synchronizer<Idx>> {
    if mesh.is_periodic() && mesh.is_distributed() {
        mesh.synchronizers().periodic_node.clone()
    } else {
        None
    }
}

/// `_ask_nodes` / `_giu_global_conn` accessor: owners pack the global
/// numbers of a node's DOFs, slaves unpack them (upstream
/// `GlobalDOFInfoDataAccessor`).
struct GlobalNumbers<'a> {
    data: &'a DofData,
    layout: &'a mut EquationLayout,
    nbc: usize,
}

impl GlobalNumbers<'_> {
    fn equations_of(&self, node: Idx) -> impl Iterator<Item = usize> + '_ {
        self.data
            .associated_nodes
            .iter()
            .zip(&self.data.local_equation_numbers)
            .filter(move |(n, _)| **n == node)
            .filter_map(|(_, &e)| usize::try_from(e).ok())
    }
}

impl DataAccessor<Idx> for GlobalNumbers<'_> {
    fn nb_data(&self, nodes: &[Idx], _: SynchronizationTag) -> usize {
        nodes.len() * self.nbc * std::mem::size_of::<GlobalIdx>()
    }
    fn pack_data(
        &self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &n in nodes {
            for e in self.equations_of(n) {
                buf.pack(
                    &self
                        .layout
                        .global(e)
                        .unwrap_or(akantu_core::INVALID_GLOBAL_IDX),
                );
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        tag: SynchronizationTag,
    ) -> Result<()> {
        for &n in nodes {
            let eqs: Vec<usize> = self.equations_of(n).collect();
            for e in eqs {
                let g: GlobalIdx = buf.unpack()?;
                if tag == SynchronizationTag::GiuGlobalConn {
                    self.layout.set_periodic_master_global(e, g)?;
                } else {
                    self.layout.set_shared_global(e, g)?;
                }
            }
        }
        Ok(())
    }
}

/// Distributed reduction of rank-partial vectors (upstream
/// `DOFSynchronizer::reduceSynchronizeArray<AddOperation>`, applied to the
/// partial rows instead of upstream's zero temporary: UD-003), as phases of
/// the calling operation's scope.
///
/// Channels (UD-872): a shared row reaches its owner through
/// the **node** synchronizer when its copy is a parallel slave, and through
/// the **periodic** node synchronizer when its copy is a pure-ghost
/// periodic master copy (turned `Shared` by the distributed periodic
/// numbering, a rank holds such a master only because local
/// periodic slaves alias its equation, so it computes a partial row there).
/// Upstream's DOF synchronizer is built from the node synchronizer's
/// schemes only (`dof_synchronizer.cc:64-117`), so those rows would be lost
/// even with UD-003 corrected. A periodic master copy that is a parallel
/// slave is in both synchronizers' schemes: it contributes through the
/// node synchronizer only (its periodic contribution is `0.0`), so nothing
/// is counted twice. Steps, each in rank order (D-039): pure-ghost periodic
/// copies → owner (sum), parallel slaves → owner (sum), owner → parallel
/// slaves, owner → every periodic master copy. The owner's row is
/// `((β·y + own partial) + periodic copies) + node copies`.
struct DofReduction<'a, 'p> {
    dm: &'a DofManager,
    sync: Option<&'a Synchronizer<Idx>>,
    /// Periodic node synchronizer of a distributed periodic mesh (`None`
    /// otherwise: the product is then exactly the node-only reduction).
    periodic: Option<&'a Synchronizer<Idx>>,
    phases: RefCell<&'p mut Phases>,
}

/// Local-equation vector seen per node (the equations of every family at
/// that node, in registration order: identical on every rank).
struct NodeVector<'a, 'e> {
    eqs: &'e HashMap<Idx, Vec<usize>>,
    v: &'a mut [f64],
    add: bool,
    /// Nodes whose values are packed as `0.0` (the parallel-slave copies in
    /// the periodic reduction: their partial rows travel through the node
    /// synchronizer).
    zero: Option<&'e HashSet<Idx>>,
}

impl<'a, 'e> NodeVector<'a, 'e> {
    /// Owner → copies (the receiver overwrites).
    fn copy(eqs: &'e HashMap<Idx, Vec<usize>>, v: &'a mut [f64]) -> Self {
        Self {
            eqs,
            v,
            add: false,
            zero: None,
        }
    }

    /// Copies → owner (the receiver adds).
    fn sum(eqs: &'e HashMap<Idx, Vec<usize>>, v: &'a mut [f64]) -> Self {
        Self {
            eqs,
            v,
            add: true,
            zero: None,
        }
    }
}

impl DataAccessor<Idx> for NodeVector<'_, '_> {
    fn nb_data(&self, nodes: &[Idx], _: SynchronizationTag) -> usize {
        nodes
            .iter()
            .map(|n| self.eqs.get(n).map_or(0, Vec::len))
            .sum::<usize>()
            * 8
    }
    fn pack_data(
        &self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for n in nodes {
            let zero = self.zero.is_some_and(|z| z.contains(n));
            for &e in self.eqs.get(n).into_iter().flatten() {
                buf.pack(&if zero { 0.0 } else { self.v[e] });
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        buf: &mut CommunicationBuffer,
        nodes: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for n in nodes {
            for &e in self.eqs.get(n).into_iter().flatten() {
                let x: f64 = buf.unpack()?;
                if self.add {
                    self.v[e] += x;
                } else {
                    self.v[e] = x;
                }
            }
        }
        Ok(())
    }
}

/// The nodes this rank receives through `sync` (its parallel-slave copies
/// for the node synchronizer).
fn received_nodes(sync: Option<&Synchronizer<Idx>>) -> HashSet<Idx> {
    sync.map(|s| s.communications.recv.values().flatten().copied().collect())
        .unwrap_or_default()
}

impl PartialSumReduction for DofReduction<'_, '_> {
    fn is_distributed(&self) -> bool {
        self.sync.is_some() || self.periodic.is_some()
    }
    fn reduce_and_synchronize(&self, local: &mut [f64]) -> Result<()> {
        if !self.is_distributed() {
            return NoReduction.reduce_and_synchronize(local);
        }
        let eqs = self.dm.node_equations();
        let mut ph = self
            .phases
            .try_borrow_mut()
            .map_err(|_| Error::invalid("re-entrant distributed reduction"))?;
        // Pure-ghost periodic master copies → owner; the copies
        // that are parallel slaves send 0.0 (node channel below).
        if let Some(periodic) = self.periodic {
            let via_node = received_nodes(self.sync);
            let mut acc = NodeVector {
                zero: Some(&via_node),
                ..NodeVector::sum(&eqs, &mut *local)
            };
            periodic.slave_reduction_in(&mut ph, &mut acc, SynchronizationTag::SmmRes)?;
        }
        if let Some(sync) = self.sync {
            sync.slave_reduction_in(
                &mut ph,
                &mut NodeVector::sum(&eqs, &mut *local),
                SynchronizationTag::SmmRes,
            )?;
            sync.synchronize_in(
                &mut ph,
                &mut NodeVector::copy(&eqs, &mut *local),
                SynchronizationTag::SmmRes,
            )?;
        }
        // Owner → every periodic master copy (pure ghost or parallel slave:
        // the latter already holds the owner's value, rewritten unchanged).
        if let Some(periodic) = self.periodic {
            periodic.synchronize_in(
                &mut ph,
                &mut NodeVector::copy(&eqs, local),
                SynchronizationTag::SmmRes,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_and_lookup() {
        let mut d = DofManager::new("dm", Communicator::serial());
        d.new_matrix("K", MatrixType::Symmetric).unwrap();
        assert!(d.new_matrix("K", MatrixType::Symmetric).is_err());
        assert_eq!(d.matrix("K").unwrap().id(), "K");
        assert!(d.matrix("M").is_err());
        d.new_lumped_matrix("M").unwrap();
        assert!(d.dofs("displacement").is_err());
        assert!(d.dof_ids().is_empty());
        d.zero_residual();
        assert_eq!(d.system_size(), 0);
    }

    /// Aggregate local-equation capacity (existing + new) is
    /// checked before any mutation, without allocating a huge mesh.
    #[test]
    fn synthetic_aggregate_overflow_is_rejected() {
        assert!(check_local_equations(MAX_LOCAL_COUNT - 2, 2).is_ok());
        assert!(matches!(
            check_local_equations(MAX_LOCAL_COUNT - 1, 2),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(check_local_equations(usize::MAX, 1).is_err());
        let mut mesh = Mesh::new(1, "m").unwrap();
        {
            let mut a = akantu_mesh::MeshAccessor::new(&mut mesh);
            a.push_node(&[0.0]).unwrap();
            a.push_node(&[1.0]).unwrap();
        }
        // Two nodes × 1 DOF on top of MAX_LOCAL_COUNT − 1 equations.
        let err = plan_block(&mesh, None, &[0, 1], 1, MAX_LOCAL_COUNT - 1).unwrap_err();
        assert!(matches!(err, Error::CapacityExceeded { .. }), "{err:?}");
        assert!(plan_block(&mesh, None, &[0, 1], 1, MAX_LOCAL_COUNT - 2).is_ok());
        // Registration preflight: per-family DOF product overflow.
        let mut d = DofManager::new("dm", Communicator::serial());
        assert!(matches!(
            d.register_generic_dofs("huge", MAX_LOCAL_COUNT, 2),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(d.dof_ids().is_empty());
    }

    #[test]
    fn generic_dofs_are_owned() {
        let mut d = DofManager::new("dm", Communicator::serial());
        d.register_generic_dofs("lambda", 2, 1).unwrap();
        assert_eq!(
            d.global_equation_numbers("lambda").unwrap(),
            vec![Some(0), Some(1)]
        );
        assert!(d.register_generic_dofs("lambda", 1, 1).is_err());
    }
}
