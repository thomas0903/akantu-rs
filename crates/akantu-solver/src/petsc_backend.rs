// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
// {dof_manager_petsc.hh, dof_manager_petsc.cc} (DOFManagerPETSc: AO
// numbering, matrices, solver vectors, non-linear solver factory),
// src/solver/{sparse_matrix_petsc.*, solver_vector_petsc.*,
// sparse_solver_petsc.*} (SparseMatrixPETSc, SolverVectorPETSc,
// SparseSolverPETSc), src/model/common/non_linear_solver/
// {non_linear_solver_petsc.*, non_linear_solver_tao.*} (SNES / TAO solve
// sequences, callbacks).

//! The PETSc linear-system backend of the DOF manager (Cargo feature
//! `petsc`; DOF-015, SPM-008, SPM-012, SOLV-005, NLS-006,
//! NLS-007).
//!
//! **Licence.** The feature is opt-in and off by default; a build with it
//! links Debian's PETSc and its GPL-2+ dependencies and is distributed
//! under GPL-3.0-or-later as a whole (D-076 option A, `NOTICE.md`).
//!
//! # Design
//!
//! * **Assembly store.** Every matrix is kept as a rank-partial
//!   [`SparseMatrixAij`] in **unsymmetric (full) storage** — upstream
//!   `SparseMatrixPETSc::addValues` inserts whole elemental blocks, never
//!   one triangle — so all local operations keep the AIJ backend's
//!   semantics (profiles, `add_values`, pre-assembled terms, `zero`,
//!   Dirichlet rows with owner-only diagonals, `copy_profile`,
//!   `add_matrix`, value releases, the UD-003-corrected distributed
//!   product). The declared symmetry is tracked separately (effective type,
//!   UD-034 promotion proposal on an unsymmetric block, agreed by the
//!   manager) and drives PETSc's `MAT_SYMMETRIC` flags as upstream.
//! * **PETSc matrix.** At each collective native use — a KSP solve, the
//!   preparation of a SNES/TAO solve, a Jacobian update inside a native
//!   callback ([`LinearSystemBackend::sync_native`]) — the PETSc `Mat` is
//!   brought up to date inside an agreed native collective: application
//!   ordering recomputed after a numbering change (`AOCreateMemoryScalable`
//!   from the owned global equations: one map for rows, columns and
//!   vectors, the UD-006 fix), the matrix created with upstream's sequence
//!   (`MatSetFromOptions`, sizes `(owned, owned, N, N)`, `MatSetUp`,
//!   `MAT_SYMMETRIC`), its preallocation reset after a profile change
//!   (upstream `clearProfile`), then `MatZeroEntries` + `MatSetValues`
//!   (`ADD_VALUES`, one call per row) + `MatAssemblyBegin/End`. Every
//!   decision (renumber, create, reset, refill, symmetry flags) is reduced
//!   over the ranks first, so all ranks enter the same PETSc collectives.
//! * **Vectors.** PETSc's natural ordering puts this rank's owned
//!   equations, in local order, at its contiguous ownership range, so
//!   right-hand sides and solutions move between akantu's local-equation
//!   vectors and PETSc vectors without index maps.
//! * **KSP** (upstream `SparseSolverPETSc`): one per solved matrix, with a
//!   private options database ([`LinearSystemBackend::set_solver_option`],
//!   upstream `getSparseSolver().set(...)`); as upstream, `KSPSetFromOptions`
//!   runs only once an option was set (so command-line KSP options alone do
//!   not configure it, UD-425), defaults otherwise (GMRES, ILU(0) serial /
//!   block Jacobi + ILU(0) parallel, `rtol = 1e-5`), zero initial guess. A
//!   diverged solve is an `Error::LinearSolver` (UD-423; compat:
//!   upstream's silent return). The blocked rows of the right-hand side
//!   (reactions) are zeroed before the solve (UD-427; compat: kept, which
//!   makes an iterative KSP's tolerance relative to the reactions).
//! * **SNES / TAO** ([`PetscNativeDriver`]): per solve a solver object is
//!   created (upstream constructor sequence: `SNESNEWTONLS` / `TAOGPCG`,
//!   global options), the solver's private options pushed around
//!   `XSetFromOptions`, then `SNESSolve`/`TaoSolve` runs as an agreed
//!   native region whose callbacks go through
//!   [`crate::NativeSolveContext`] (residual, Jacobian, objective, all
//!   agreed collective operations of the solve's scope).
//!
//! # Not reproduced (documented differences)
//!
//! * Upstream writes `J_petsc` (Matrix Market) into the working directory
//!   at every SNES solve (debug leftover, UD-426): not done by default;
//!   the manager-wide compat switch `UpstreamCompat::petsc_write_j_petsc`
//!   writes it exactly as upstream (D-003 B).
//! * The PETSc `Mat` is filled from the assembly store at the native use
//!   instead of per elemental `MatSetValues` call: identical values in
//!   serial (same per-entry summation order), the off-process summation
//!   order of parallel runs differs (`docs/NUMERICAL_VALIDATION.md`).

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use akantu_comm::Communicator;
use akantu_comm::ReduceOp;
use akantu_comm::phase::{Severity, agreed_or_transport, digest_bytes};
use akantu_core::enums::{MatrixType, NonLinearSolverType, SparseSolverType};
use akantu_core::{Error, FatalKind, GlobalIdx, Id, Real, Result};
use akantu_linalg::petsc::{
    self, Ksp, KspOutcome, Matrix, NativeCallbacks, NativeComm, Options, PetscInt, PetscLock,
    Session, Snes, SnesOutcome, Tao, TaoOutcome, Vector,
};
use akantu_linalg::{EquationLayout, PartialSumReduction, SparseMatrixAij};

use crate::backend::{
    BackendContext, BackendKind, LinearSystemBackend, NativeNonLinearDriver, NativeReport,
    NativeRequest, NativeSolveContext,
};
use crate::dof_manager::UpstreamCompat;

/// PETSc's global options database (upstream `PetscOptionsSetValue(NULL,
/// …)`): see [`akantu_linalg::petsc::set_global_option`].
pub use akantu_linalg::petsc::{set_command_line, set_global_option};

/// Upstream's Jacobian dump of every SNES solve (UD-426, compat only).
const J_PETSC: &str = "J_petsc";

/// Rank on which the glue code of the next native callback panics (test
/// hook of the glue-panic regression; `-1` = never).
static GLUE_PANIC_RANK: AtomicI32 = AtomicI32::new(-1);

/// **Test hook** (`petsc_mpi_cases::ape01_*`):
/// the next native callback's glue code (`Callbacks::scatter`) panics on
/// rank `rank` (once; `-1` disables it). A panic there is a defect of the
/// back-end's internals that no model callback can cause (their panics are
/// agreed); the driver ends the job with
/// `TRANSPORT_ABORT_CODE` (`docs/ARCHITECTURE.md`). Not part of the API.
#[doc(hidden)]
pub fn inject_glue_panic(rank: i32) {
    GLUE_PANIC_RANK.store(rank, Ordering::SeqCst);
}

/// A PETSc error inside an agreed native collective: ranks may have
/// diverged inside PETSc, a transport-class failure (`docs/ARCHITECTURE.md`).
fn fatal(e: Error) -> Error {
    Error::fatal(FatalKind::Transport, e)
}

/// A global equation as a 32-bit `PetscInt` (checked narrowing).
fn to_petsc_int(g: GlobalIdx) -> Result<PetscInt> {
    PetscInt::try_from(g).map_err(|_| Error::CapacityExceeded {
        what: "global equation as a 32-bit PetscInt",
        requested: u128::try_from(g.max(0)).unwrap_or(u128::MAX),
        limit: u128::try_from(PetscInt::MAX).unwrap_or(0),
    })
}

fn size_of(g: GlobalIdx) -> Result<usize> {
    usize::try_from(g).map_err(|_| Error::invalid(format!("negative matrix order {g}")))
}

/// Digest of an equation layout (numbering changes are detected and the
/// renumbering decision reduced over the ranks).
fn layout_digest(layout: &EquationLayout) -> u64 {
    let mut h = digest_bytes(0xcbf2_9ce4_8422_2325, &layout.system_size().to_le_bytes());
    for (l, g) in layout.local_to_global().iter().enumerate() {
        h = digest_bytes(h, &g.to_le_bytes());
        h = digest_bytes(h, &[u8::from(layout.is_owned(l))]);
    }
    h
}

/// Owned local equations in local order (= PETSc's local ordering).
fn owned_locals(layout: &EquationLayout) -> Vec<usize> {
    (0..layout.nb_local())
        .filter(|&l| layout.is_owned(l))
        .collect()
}

/// PETSc numbering of the local equations (application ordering).
#[derive(Debug)]
struct Numbering {
    /// Bumped at every renumbering (plans keyed on it).
    epoch: u64,
    digest: u64,
    /// PETSc index of every local equation (`-1` for pure ghosts).
    petsc_of_local: Vec<PetscInt>,
}

/// Insertion plan of a matrix: its AIJ entries grouped by PETSc row.
#[derive(Debug)]
struct Plan {
    profile: u64,
    epoch: u64,
    /// `(PETSc row, start, end)` into `cols` / `perm`.
    rows: Vec<(PetscInt, usize, usize)>,
    cols: Vec<PetscInt>,
    /// AIJ entry index of each planned value.
    perm: Vec<usize>,
}

/// What the PETSc matrix currently holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Built {
    profile: u64,
    values: u64,
    epoch: u64,
}

#[derive(Debug)]
struct PetscMatrix {
    /// Rank-partial assembly store (full storage).
    aij: SparseMatrixAij,
    declared: MatrixType,
    effective: MatrixType,
    native: Option<Matrix>,
    built: Option<Built>,
    /// `MAT_SYMMETRIC = false` applied to `native`.
    marked_unsymmetric: bool,
    plan: Option<Plan>,
    /// Native solves holding a reference to `native` (a recreation would
    /// leave them with a stale matrix).
    holders: Arc<()>,
    /// Blocked local equations of the last `apply_boundary` (UD-427: their
    /// right-hand-side rows are zeroed before a KSP solve).
    blocked: Option<Vec<bool>>,
}

impl PetscMatrix {
    fn new(id: &str, ty: MatrixType, size: GlobalIdx) -> Self {
        Self {
            aij: SparseMatrixAij::new(id, size, MatrixType::Unsymmetric),
            declared: ty,
            effective: ty,
            native: None,
            built: None,
            marked_unsymmetric: false,
            plan: None,
            holders: Arc::new(()),
            blocked: None,
        }
    }
}

/// KSP of one matrix (upstream `SparseSolverPETSc`).
#[derive(Debug, Default)]
struct KspSlot {
    ksp: Option<Ksp>,
    /// Private options (`-name value`) in setting order.
    options: Vec<(String, String)>,
    /// Options not yet applied (`KSPSetFromOptions` at the next solve).
    dirty: bool,
    db: Option<Options>,
    last: Option<KspOutcome>,
}

/// Local decisions of a native update, reduced (max) over the ranks.
#[derive(Clone, Copy, Debug, Default)]
struct Flags {
    renumber: bool,
    create: bool,
    reset: bool,
    refill: bool,
    unsymmetric: bool,
    /// Recreate the cached KSP solve vectors (their creation is collective,
    /// so the decision must be common to every rank).
    vectors: bool,
}

impl Flags {
    fn to_words(self) -> [u8; 6] {
        [
            u8::from(self.renumber),
            u8::from(self.create),
            u8::from(self.reset),
            u8::from(self.refill),
            u8::from(self.unsymmetric),
            u8::from(self.vectors),
        ]
    }

    fn from_words(w: [u8; 6]) -> Self {
        Self {
            renumber: w[0] != 0,
            create: w[1] != 0,
            reset: w[2] != 0,
            refill: w[3] != 0,
            unsymmetric: w[4] != 0,
            vectors: w[5] != 0,
        }
    }
}

/// The PETSc backend (upstream `DOFManagerPETSc` storage and solvers).
#[derive(Debug)]
pub struct PetscBackend {
    comm: Communicator,
    session: Session,
    matrices: BTreeMap<Id, PetscMatrix>,
    numbering: Option<Numbering>,
    next_epoch: u64,
    ksp: BTreeMap<Id, KspSlot>,
    /// Cached solve vectors `(rhs, solution)` for `(owned, size)`.
    vectors: Option<(usize, usize, Vector, Vector)>,
    compat: UpstreamCompat,
}

impl PetscBackend {
    /// New backend over `comm` (an MPI communicator; the serial backend and
    /// the local world have no MPI handle: `Error::Unsupported`). Joins or
    /// starts the PETSc session (upstream `PETScSingleton`): **collective**
    /// over `comm`, outside any collective scope.
    pub fn new(comm: &Communicator) -> Result<Self> {
        if !comm.is_mpi() {
            return Err(Error::Unsupported(
                "the PETSc backend needs an MPI communicator (Communicator::mpi_world)".into(),
            ));
        }
        let session = Session::acquire(comm)?;
        Ok(Self {
            comm: comm.clone(),
            session,
            matrices: BTreeMap::new(),
            numbering: None,
            next_epoch: 1,
            ksp: BTreeMap::new(),
            vectors: None,
            compat: UpstreamCompat::default(),
        })
    }

    fn get(&self, id: &str) -> Result<&PetscMatrix> {
        self.matrices
            .get(id)
            .ok_or_else(|| Error::not_found("matrix", id))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut PetscMatrix> {
        self.matrices
            .get_mut(id)
            .ok_or_else(|| Error::not_found("matrix", id))
    }

    /// The last KSP outcome of matrix `id` (iterations, reason, residual
    /// norm; tests and diagnostics).
    #[must_use]
    pub fn last_ksp_outcome(&self, id: &str) -> Option<&KspOutcome> {
        self.ksp.get(id).and_then(|s| s.last.as_ref())
    }

    /// `(KSP type, PC type)` of the KSP of matrix `id` after a solve.
    pub fn ksp_types(&self, id: &str) -> Result<(String, String)> {
        self.ksp
            .get(id)
            .and_then(|s| s.ksp.as_ref())
            .ok_or_else(|| Error::not_found("PETSc KSP of matrix", id))?
            .types()
    }

    /// Values of the PETSc matrix `id` at owned rows × columns given as
    /// **akantu global equations** (tests and diagnostics; local).
    pub fn native_values(
        &self,
        id: &str,
        layout: &EquationLayout,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
    ) -> Result<Vec<Real>> {
        let m = self.get(id)?;
        let native = m
            .native
            .as_ref()
            .ok_or_else(|| Error::not_found("PETSc matrix", id))?;
        let numbering = self
            .numbering
            .as_ref()
            .ok_or_else(|| Error::invalid("no PETSc numbering yet"))?;
        let map = |g: &GlobalIdx| -> Result<PetscInt> {
            let l = layout.local(*g)?;
            Ok(numbering.petsc_of_local[l])
        };
        let r: Vec<PetscInt> = rows.iter().map(map).collect::<Result<_>>()?;
        let c: Vec<PetscInt> = cols.iter().map(map).collect::<Result<_>>()?;
        native.get_values(&r, &c)
    }

    /// PETSc index of every local equation (`-1` for pure ghosts) after the
    /// last native use (tests: the application ordering).
    #[must_use]
    pub fn petsc_numbering(&self) -> Option<&[PetscInt]> {
        self.numbering.as_ref().map(|n| n.petsc_of_local.as_slice())
    }

    /// Local decisions of a native update of matrix `id`.
    fn local_flags(&self, id: &str, layout: &EquationLayout) -> Result<Flags> {
        let m = self.get(id)?;
        let digest = layout_digest(layout);
        let renumber = self.numbering.as_ref().is_none_or(|n| n.digest != digest);
        let size = size_of(m.aij.size())?;
        let create = m
            .native
            .as_ref()
            .is_none_or(|n| n.n_owned() != layout.nb_owned() || n.size() != size);
        let epoch = self.numbering.as_ref().map_or(0, |n| n.epoch);
        let reset = match m.built {
            None => true,
            Some(b) => b.profile != m.aij.profile_release() || b.epoch != epoch || renumber,
        };
        let refill = reset || create || m.built.is_none_or(|b| b.values != m.aij.value_release());
        let unsymmetric = m.effective != MatrixType::Symmetric
            && m.declared == MatrixType::Symmetric
            && (!m.marked_unsymmetric || create);
        // Any rank whose ownership or numbering changed recreates the
        // solve vectors on every rank (reduced with the other flags).
        let vectors = renumber
            || !matches!(&self.vectors, Some((o, s, _, _)) if *o == layout.nb_owned() && *s == size);
        if create && Arc::strong_count(&m.holders) > 1 {
            return Err(Error::invalid(format!(
                "PETSc matrix '{id}' must be recreated (numbering change) while a native solver \
                 holds it"
            )));
        }
        if size != size_of(layout.system_size())? {
            return Err(Error::DimensionMismatch {
                context: "PetscBackend (matrix order vs system size)",
                expected: size_of(layout.system_size())?,
                got: size,
            });
        }
        Ok(Flags {
            renumber,
            create,
            reset,
            refill,
            unsymmetric,
            vectors,
        })
    }

    /// Bring matrix `id`'s PETSc copy up to date: local decisions, reduced
    /// over the ranks, then one native collective. Collective (inside the
    /// caller's scope, `ctx`).
    ///
    /// Locks: the PETSc lock is taken inside the agreed
    /// native collective (after the admission, holding the `Serialized`
    /// MPI lock), never across the agreements. An agreed refusal of the
    /// native collective (e.g. the `Serialized` admission rule) is an
    /// ordinary, recoverable error of the step.
    fn ensure_native(&mut self, id: &str, ctx: &mut BackendContext<'_>, name: &str) -> Result<()> {
        let layout = ctx.layout;
        let prepared = self.local_flags(id, layout);
        let own = self.comm.clone();
        ctx.phases.run(name, prepared, |c, flags| {
            let Some(flags) = agree_flags(c, &own, flags)? else {
                return Ok(Err(foreign_scope()));
            };
            let r = c.native_collective(name, Ok(()), |nc| {
                let _petsc = PetscLock::enter_native(&nc);
                self.build_native(&nc, id, layout, flags)
            });
            match agreed_or_transport(r)? {
                Ok(built) => built.map_err(fatal)?,
                Err(refused) => return Ok(Err(refused)),
            }
            Ok(Ok(()))
        })
    }

    /// The native update (inside the agreed native collective).
    fn build_native(
        &mut self,
        nc: &NativeComm<'_>,
        id: &str,
        layout: &EquationLayout,
        flags: Flags,
    ) -> Result<()> {
        if flags.renumber {
            self.renumber(nc, layout)?;
        }
        let epoch = self.numbering.as_ref().map_or(0, |n| n.epoch);
        let (comm, session) = (self.comm.clone(), self.session.clone());
        let numbering = self
            .numbering
            .as_ref()
            .ok_or_else(|| Error::invalid("PETSc numbering missing"))?;
        let m = self
            .matrices
            .get_mut(id)
            .ok_or_else(|| Error::not_found("matrix", id))?;
        let size = size_of(m.aij.size())?;
        if flags.create {
            // Upstream's constructor: MAT_SYMMETRIC for a declared
            // symmetric matrix (then cleared by an unsymmetric block).
            m.native = None;
            m.native = Some(Matrix::create(
                nc,
                &comm,
                &session,
                id,
                layout.nb_owned(),
                size,
                m.declared == MatrixType::Symmetric,
            )?);
            m.marked_unsymmetric = false;
        } else if flags.reset
            && let Some(native) = m.native.as_mut()
        {
            native.reset_preallocation(nc)?;
        }
        let native = m
            .native
            .as_mut()
            .ok_or_else(|| Error::invalid("PETSc matrix missing"))?;
        if flags.unsymmetric {
            native.set_unsymmetric(nc)?;
            m.marked_unsymmetric = true;
        }
        if flags.refill {
            let profile = m.aij.profile_release();
            let stale = m
                .plan
                .as_ref()
                .is_none_or(|p| p.profile != profile || p.epoch != epoch);
            if stale {
                m.plan = Some(plan(&m.aij, layout, &numbering.petsc_of_local, epoch)?);
            }
            let plan = m
                .plan
                .as_ref()
                .ok_or_else(|| Error::invalid("plan missing"))?;
            native.zero_entries(nc)?;
            let values = m.aij.values();
            let mut vals = Vec::new();
            for &(row, start, end) in &plan.rows {
                vals.clear();
                vals.extend(plan.perm[start..end].iter().map(|&k| values[k]));
                native.add_row(row, &plan.cols[start..end], &vals)?;
            }
            native.assemble(nc)?;
        }
        m.built = Some(Built {
            profile: m.aij.profile_release(),
            values: m.aij.value_release(),
            epoch,
        });
        Ok(())
    }

    /// Application ordering of the current layout (collective).
    fn renumber(&mut self, nc: &NativeComm<'_>, layout: &EquationLayout) -> Result<()> {
        let mut owned = Vec::with_capacity(layout.nb_owned());
        let mut idx = Vec::with_capacity(layout.nb_local());
        for l in 0..layout.nb_local() {
            match layout.global(l) {
                Some(g) => {
                    let p = to_petsc_int(g)?;
                    if layout.is_owned(l) {
                        owned.push(p);
                    }
                    idx.push(p);
                }
                None => idx.push(-1),
            }
        }
        petsc::ao_map(nc, &owned, &mut idx)?;
        let epoch = self.next_epoch;
        self.next_epoch += 1;
        self.numbering = Some(Numbering {
            epoch,
            digest: layout_digest(layout),
            petsc_of_local: idx,
        });
        // Every plan refers to the old numbering.
        for m in self.matrices.values_mut() {
            m.plan = None;
        }
        Ok(())
    }

    /// The KSP solve of matrix `id` (inside the agreed native collective):
    /// outer error = PETSc failure (fatal), inner = ordinary outcome.
    fn solve_native(
        &mut self,
        nc: &NativeComm<'_>,
        id: &str,
        layout: &EquationLayout,
        flags: Flags,
        rhs_owned: &[Real],
    ) -> Result<Result<Vec<Real>>> {
        self.build_native(nc, id, layout, flags)?;
        let size = size_of(layout.system_size())?;
        let n_owned = layout.nb_owned();
        let (comm, session) = (self.comm.clone(), self.session.clone());
        let slot = self.ksp.entry(id.into()).or_default();
        // A recreated matrix or recreated vectors may have other local
        // sizes (re-partitioning at constant global size): the KSP drops its
        // operators first (agreed decision, every rank resets).
        if (flags.create || flags.vectors)
            && let Some(ksp) = slot.ksp.as_mut()
        {
            ksp.reset(nc)?;
        }
        if slot.ksp.is_none() {
            slot.ksp = Some(Ksp::create(
                nc,
                &comm,
                &session,
                &format!("{id}:sparse_solver"),
            )?);
        }
        if slot.dirty {
            let mut db = Options::new(&comm, &session)?;
            for (k, v) in &slot.options {
                db.set(k, v)?;
            }
            if let Some(ksp) = slot.ksp.as_mut() {
                ksp.set_from_options(nc, Some(&db))?;
            }
            slot.db = Some(db);
            slot.dirty = false;
        }
        // The (collective) recreation is decided by the reduced flag.
        if flags.vectors || self.vectors.is_none() {
            self.vectors = None;
            let b = Vector::create(nc, &comm, &session, &format!("{id}:rhs"), n_owned, size)?;
            let x = Vector::create(
                nc,
                &comm,
                &session,
                &format!("{id}:solution"),
                n_owned,
                size,
            )?;
            self.vectors = Some((n_owned, size, b, x));
        }
        let Some((_, _, b, x)) = self.vectors.as_mut() else {
            return Err(Error::invalid("PETSc solve vectors missing"));
        };
        b.set_owned(rhs_owned)?;
        let a = self
            .matrices
            .get(id)
            .and_then(|m| m.native.as_ref())
            .ok_or_else(|| Error::not_found("PETSc matrix", id))?;
        let slot = self
            .ksp
            .get_mut(id)
            .ok_or_else(|| Error::not_found("PETSc KSP of matrix", id))?;
        let ksp = slot
            .ksp
            .as_mut()
            .ok_or_else(|| Error::not_found("PETSc KSP of matrix", id))?;
        let outcome = ksp.solve(nc, a, b, x)?;
        let mut sol = vec![0.0; n_owned];
        x.get_owned(&mut sol)?;
        let diverged = outcome.reason < 0;
        let message = format!(
            "PETSc KSP did not converge ({}, reason {}) after {} iterations, residual norm {:e}",
            outcome.reason_string, outcome.reason, outcome.iterations, outcome.residual_norm
        );
        slot.last = Some(outcome);
        if diverged && !self.compat.petsc_ignore_ksp_divergence {
            return Ok(Err(Error::LinearSolver(message)));
        }
        Ok(Ok(sol))
    }

    /// Native objects of a SNES/TAO solve (collective, inside the solve's
    /// scope): the PETSc `J` up to date, the solver created and configured,
    /// solution/residual vectors, a reference to `J`.
    fn prepare_native(
        &mut self,
        request: &NativeRequest,
        id: &str,
        ctx: &mut BackendContext<'_>,
    ) -> Result<NativeObjects> {
        self.ensure_native(id, ctx, &format!("petsc:native:matrix:{id}"))?;
        let layout = ctx.layout;
        let kind = request
            .kind
            .ok_or_else(|| Error::invalid("native solve without a solver kind"))?;
        let size = size_of(layout.system_size())?;
        let owned = owned_locals(layout);
        let n_owned = owned.len();
        // UD-426 compat (D-003 B): upstream writes `J_petsc` (Matrix Market,
        // working directory) at every SNES solve, right after assembling J.
        // PETSc's viewer opens the file on rank 0: checked there first and
        // agreed, so an unwritable directory fails every rank ordinarily
        // (upstream's solve throws) instead of failing inside the collective.
        let write_j = self.compat.petsc_write_j_petsc && kind == NonLinearSolverType::PetscSnes;
        let j_file = if write_j && self.comm.rank() == 0 {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(J_PETSC)
                .map(drop)
                .map_err(|source| Error::Io {
                    // UD-426 compat: the Jacobian dump of the SNES solve in
                    // the working directory.
                    path: J_PETSC.into(),
                    source,
                })
        } else {
            Ok(())
        };
        let bounds = match &request.bounds {
            Some((lo, hi)) if lo.len() != layout.nb_local() || hi.len() != layout.nb_local() => {
                Err(Error::DimensionMismatch {
                    context: "petsc_tao bounds (local equations)",
                    expected: layout.nb_local(),
                    got: lo.len().min(hi.len()),
                })
            }
            Some((lo, hi)) => Ok(Some((
                owned.iter().map(|&l| lo[l]).collect::<Vec<_>>(),
                owned.iter().map(|&l| hi[l]).collect::<Vec<_>>(),
            ))),
            None => Ok(None),
        };
        let prepared = j_file.and(bounds);
        let (comm, session) = (self.comm.clone(), self.session.clone());
        let m = self.get(id)?;
        let holders = Arc::clone(&m.holders);
        let jac = m.native.as_ref();
        let name = format!("petsc:native:objects:{}", request.solver_id);
        // The solver kind, its options and the aliasing switch steer
        // PETSc's control flow: agreed on every rank.
        ctx.phases.agree_digest(
            &format!("{name}:request"),
            Ok(()),
            request_digest(request),
            Severity::Recoverable,
        )?;
        let own = comm.clone();
        ctx.phases.run(&name, prepared, |c, bounds| {
            if agree_flags(c, &own, Flags::default())?.is_none() {
                return Ok(Err(foreign_scope()));
            }
            let r = c.native_collective(&name, Ok(()), |nc| -> Result<NativeObjects> {
                let _petsc = PetscLock::enter_native(&nc);
                let jac = jac.ok_or_else(|| Error::not_found("PETSc matrix", id))?;
                if write_j {
                    // Upstream `J.saveMatrix("J_petsc")` (collective).
                    jac.save_matrix_market(&nc, J_PETSC)?;
                }
                let mut db = Options::new(&comm, &session)?;
                for (k, v) in &request.options {
                    db.set(k, v)?;
                }
                let sname = request.solver_id.as_str();
                let solver = match kind {
                    NonLinearSolverType::PetscTao => {
                        let mut t = Tao::create(&nc, &comm, &session, sname)?;
                        t.set_from_options(&nc, Some(&db))?;
                        NativeSolver::Tao(t)
                    }
                    _ => {
                        let mut s = Snes::create(&nc, &comm, &session, sname)?;
                        s.set_from_options(&nc, Some(&db))?;
                        NativeSolver::Snes(s)
                    }
                };
                let x = Vector::create(&nc, &comm, &session, &format!("{sname}:x"), n_owned, size)?;
                let r =
                    Vector::create(&nc, &comm, &session, &format!("{sname}:rhs"), n_owned, size)?;
                // TAO: a second work vector for the objective's dot products
                // (with `r`: upstream's `VecDot(rhs, x)`, `VecDot(x, Jx)`).
                let w = match kind {
                    NonLinearSolverType::PetscTao => Some(Vector::create(
                        &nc,
                        &comm,
                        &session,
                        &format!("{sname}:Kx"),
                        n_owned,
                        size,
                    )?),
                    _ => None,
                };
                let jacobian = jac.new_reference()?;
                Ok(NativeObjects {
                    solver,
                    jacobian,
                    x,
                    r,
                    w,
                    owned: owned.clone(),
                    bounds,
                    _holder: Arc::clone(&holders),
                    _db: db,
                })
            });
            // An agreed refusal is an ordinary (inner) error.
            match agreed_or_transport(r)? {
                Ok(objects) => Ok(Ok(objects.map_err(fatal)?)),
                Err(refused) => Ok(Err(refused)),
            }
        })
    }
}

/// Reduce the local update decisions over the ranks (max) together with a
/// check that the scope's communicator `c` is the one the backend's PETSc
/// objects live on; `None` (identically on every rank)
/// if any rank was handed a foreign scope. Runs inside a communication step.
fn agree_flags(c: &Communicator, own: &Communicator, flags: Flags) -> Result<Option<Flags>> {
    let w = flags.to_words();
    let mut v = [
        w[0],
        w[1],
        w[2],
        w[3],
        w[4],
        w[5],
        u8::from(!c.is_same(own)),
    ];
    c.all_reduce(&mut v, ReduceOp::Max)?;
    if v[6] != 0 {
        return Ok(None);
    }
    Ok(Some(Flags::from_words([
        v[0], v[1], v[2], v[3], v[4], v[5],
    ])))
}

fn foreign_scope() -> Error {
    Error::invalid(
        "PetscBackend: the collective scope belongs to another communicator than the backend's \
         PETSc objects (create the DOF manager and its PETSc backend on the same communicator)",
    )
}

/// Digest of the settings of a native solve that steer PETSc's control flow.
fn request_digest(r: &NativeRequest) -> u64 {
    let mut h = digest_bytes(0xcbf2_9ce4_8422_2325, format!("{:?}", r.kind).as_bytes());
    for (k, v) in &r.options {
        h = digest_bytes(digest_bytes(h, k.as_bytes()), v.as_bytes());
    }
    digest_bytes(
        h,
        &[u8::from(r.upstream_aliasing), u8::from(r.bounds.is_some())],
    )
}

/// Insertion plan of `aij` (entries grouped by PETSc row, AIJ order inside
/// a row).
fn plan(
    aij: &SparseMatrixAij,
    layout: &EquationLayout,
    petsc_of_local: &[PetscInt],
    epoch: u64,
) -> Result<Plan> {
    let (irn, jcn, _) = aij.coo();
    let map = |g: GlobalIdx| -> Result<PetscInt> {
        let l = layout.local(g)?;
        let p = petsc_of_local
            .get(l)
            .copied()
            .ok_or_else(|| Error::invalid("local equation outside the PETSc numbering"))?;
        if p < 0 {
            return Err(Error::invalid(format!(
                "matrix entry on global equation {g} without a PETSc index"
            )));
        }
        Ok(p)
    };
    let mut entries: Vec<(PetscInt, PetscInt, usize)> = Vec::with_capacity(irn.len());
    for (k, (&i, &j)) in irn.iter().zip(jcn).enumerate() {
        entries.push((map(i)?, map(j)?, k));
    }
    // Stable: AIJ (insertion) order inside a row.
    entries.sort_by_key(|e| e.0);
    let mut rows = Vec::new();
    let mut cols = Vec::with_capacity(entries.len());
    let mut perm = Vec::with_capacity(entries.len());
    let mut start = 0;
    for (n, &(r, c, k)) in entries.iter().enumerate() {
        if n > 0 && entries[n - 1].0 != r {
            rows.push((entries[n - 1].0, start, n));
            start = n;
        }
        cols.push(c);
        perm.push(k);
    }
    if let Some(last) = entries.last() {
        rows.push((last.0, start, entries.len()));
    }
    Ok(Plan {
        profile: aij.profile_release(),
        epoch,
        rows,
        cols,
        perm,
    })
}

/// The `values_type`-independent orientation of an elemental block:
/// upstream (`MAT_ROW_ORIENTED` over a column-major block, UD-422) reads
/// `values` row-major.
fn row_oriented(rows: usize, cols: usize, values: &[Real]) -> Vec<Real> {
    let mut out = vec![0.0; values.len()];
    for i in 0..rows {
        for j in 0..cols {
            out[i + j * rows] = values[i * cols + j];
        }
    }
    out
}

impl LinearSystemBackend for PetscBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Petsc
    }
    fn new_matrix(&mut self, id: &str, ty: MatrixType, size: GlobalIdx) -> Result<()> {
        if self.matrices.contains_key(id) {
            return Err(Error::AlreadyExists {
                kind: "matrix",
                id: id.into(),
            });
        }
        self.matrices
            .insert(id.into(), PetscMatrix::new(id, ty, size));
        Ok(())
    }
    fn has_matrix(&self, id: &str) -> bool {
        self.matrices.contains_key(id)
    }
    fn matrix_ids(&self) -> Vec<Id> {
        self.matrices.keys().cloned().collect()
    }
    fn matrix_type(&self, id: &str) -> Result<MatrixType> {
        Ok(self.get(id)?.effective)
    }
    fn reset_profile(&mut self, id: &str, size: GlobalIdx) -> Result<()> {
        let m = self.get_mut(id)?;
        m.aij.reset_profile(size);
        m.plan = None;
        m.built = None;
        m.blocked = None;
        Ok(())
    }
    fn promote_to_unsymmetric(&mut self, id: &str) -> Result<()> {
        let m = self.get_mut(id)?;
        if m.declared == MatrixType::Symmetric {
            m.effective = MatrixType::Unsymmetric;
        }
        Ok(())
    }
    fn add_block_to_profile(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
    ) -> Result<()> {
        self.get_mut(id)?.aij.add_block_to_profile(rows, cols)
    }
    fn add_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        let transposed = self.compat.petsc_row_oriented_blocks;
        let m = self.get_mut(id)?;
        if values.len() != rows.len() * cols.len() {
            return Err(Error::DimensionMismatch {
                context: "PetscBackend::add_values",
                expected: rows.len() * cols.len(),
                got: values.len(),
            });
        }
        if transposed {
            let v = row_oriented(rows.len(), cols.len(), values);
            m.aij.add_values(rows, cols, &v, values_type)?;
        } else {
            m.aij.add_values(rows, cols, values, values_type)?;
        }
        // Upstream addValues: an unsymmetric block makes the matrix
        // unsymmetric (here a local proposal, agreed at finalisation).
        if m.effective == MatrixType::Symmetric && values_type != MatrixType::Symmetric {
            m.effective = MatrixType::Unsymmetric;
        }
        Ok(())
    }
    fn add_terms(&mut self, id: &str, terms: &[(GlobalIdx, GlobalIdx, Real)]) -> Result<()> {
        let m = self.get_mut(id)?;
        for &(i, j, _) in terms {
            if i < 0 || j < 0 || i == akantu_core::INVALID_GLOBAL_IDX {
                return Err(Error::InvalidArgument(format!(
                    "invalid global equation ({i}, {j}) in pre-assembled terms"
                )));
            }
        }
        for &(i, j, v) in terms {
            m.aij.add(i, j, v)?;
        }
        Ok(())
    }
    fn finalize_assembly(&mut self, id: &str, _ctx: &mut BackendContext<'_>) -> Result<()> {
        // The PETSc matrix is filled at its next native use (solve, native
        // solver preparation, `sync_native`).
        self.get(id).map(|_| ())
    }
    fn zero(&mut self, id: &str) -> Result<()> {
        self.get_mut(id)?.aij.zero();
        Ok(())
    }
    fn apply_boundary(
        &mut self,
        id: &str,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        let m = self.get_mut(id)?;
        m.aij.apply_boundary(layout, blocked, block_val)?;
        m.blocked = Some(blocked.to_vec());
        Ok(())
    }
    fn mat_vec(
        &self,
        id: &str,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        self.get(id)?
            .aij
            .mat_vec_mul(layout, reduction, x, y, alpha, beta)
    }
    /// KSP solve (upstream `SparseSolverPETSc::solve`): the private options
    /// digest agreed, the PETSc matrix brought up to date and the solve run
    /// in one agreed native collective; owned rows of `rhs` in, owned rows
    /// of `solution` out.
    fn solve(
        &mut self,
        id: &str,
        ctx: &mut BackendContext<'_>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        let layout = ctx.layout;
        // Every setting that steers the native solve is agreed:
        // the KSP's private options and the compat switches.
        let compat_bits = [
            u8::from(self.compat.petsc_ignore_ksp_divergence),
            u8::from(self.compat.petsc_ksp_rhs_with_reactions),
        ];
        let options_digest = digest_bytes(
            self.ksp.get(id).map_or(0, |s| {
                s.options.iter().fold(0xcbf2_9ce4_8422_2325, |h, (k, v)| {
                    digest_bytes(digest_bytes(h, k.as_bytes()), v.as_bytes())
                })
            }),
            &compat_bits,
        );
        let owned = owned_locals(layout);
        let local = if rhs.len() != layout.nb_local() || solution.len() != layout.nb_local() {
            Err(Error::DimensionMismatch {
                context: "PetscBackend::solve",
                expected: layout.nb_local(),
                got: rhs.len().min(solution.len()),
            })
        } else {
            Ok(())
        };
        ctx.phases.agree_digest(
            &format!("petsc:solve:{id}:options"),
            local,
            options_digest,
            Severity::Recoverable,
        )?;
        // UD-427: blocked rows of the right-hand side zeroed (the matrix
        // has unit diagonals there) unless upstream's reactions are kept.
        let zero_blocked = !self.compat.petsc_ksp_rhs_with_reactions;
        let blocked = self
            .matrices
            .get(id)
            .and_then(|m| m.blocked.as_ref())
            .filter(|b| zero_blocked && b.len() == layout.nb_local());
        let prepared = self.local_flags(id, layout).map(|f| {
            let rhs_owned: Vec<Real> = owned
                .iter()
                .map(|&l| {
                    if blocked.is_some_and(|b| b[l]) {
                        0.0
                    } else {
                        rhs[l]
                    }
                })
                .collect();
            (f, rhs_owned)
        });
        let name = format!("petsc:solve:{id}");
        let own = self.comm.clone();
        let x = ctx.phases.run(&name, prepared, |c, (flags, rhs_owned)| {
            let Some(flags) = agree_flags(c, &own, flags)? else {
                return Ok(Err(foreign_scope()));
            };
            let r = c.native_collective(&name, Ok(()), |nc| {
                let _petsc = PetscLock::enter_native(&nc);
                self.solve_native(&nc, id, layout, flags, &rhs_owned)
            });
            // An agreed refusal is an ordinary (inner) error; a PETSc
            // failure inside the collective is fatal.
            match agreed_or_transport(r)? {
                Ok(solved) => solved.map_err(fatal),
                Err(refused) => Ok(Err(refused)),
            }
        })?;
        for (k, &l) in owned.iter().enumerate() {
            solution[l] = x[k];
        }
        Ok(())
    }
    fn native_non_linear(&mut self, id: &str) -> Result<Box<dyn NativeNonLinearDriver>> {
        self.native_non_linear_with(
            id,
            &NativeRequest {
                kind: Some(NonLinearSolverType::PetscSnes),
                solver_id: format!("{id}:snes"),
                ..NativeRequest::default()
            },
        )
    }
    fn native_non_linear_with(
        &mut self,
        id: &str,
        request: &NativeRequest,
    ) -> Result<Box<dyn NativeNonLinearDriver>> {
        let kind = request
            .kind
            .ok_or_else(|| Error::invalid("native solve without a solver kind"))?;
        if !matches!(
            kind,
            NonLinearSolverType::PetscSnes | NonLinearSolverType::PetscTao
        ) {
            return Err(Error::invalid(format!(
                "the PETSc backend has no native {kind} solver"
            )));
        }
        self.get(id)?;
        Ok(Box::new(PetscNativeDriver {
            kind,
            request: request.clone(),
            report: None,
        }))
    }
    fn sync_native(&mut self, id: &str, ctx: &mut BackendContext<'_>) -> Result<()> {
        self.ensure_native(id, ctx, &format!("petsc:sync:{id}"))
    }
    /// The KSP of matrix `id` exists independently of the matrix (upstream:
    /// the Newton–Raphson solver's `SparseSolverPETSc` is created before
    /// `J`), so options may be set before `id` is created.
    fn set_solver_option(&mut self, id: &str, name: &str, value: &str) -> Result<()> {
        let slot = self.ksp.entry(id.into()).or_default();
        slot.options.push((name.to_owned(), value.to_owned()));
        slot.dirty = true;
        Ok(())
    }
    fn set_upstream_compat(&mut self, compat: &UpstreamCompat) {
        self.compat = *compat;
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
    fn aij(&self, id: &str) -> Result<&SparseMatrixAij> {
        Err(Error::Unsupported(format!(
            "COO access to the PETSc matrix '{id}' (upstream SparseMatrixPETSc has none)"
        )))
    }
    fn value_release(&self, id: &str) -> Result<u64> {
        Ok(self.get(id)?.aij.value_release())
    }
    fn copy_profile(&mut self, dst: &str, src: &str) -> Result<()> {
        if dst == src {
            let m = self.get_mut(dst)?;
            let copy = m.aij.clone();
            m.aij.copy_profile(&copy);
            return Ok(());
        }
        let src_m = self.get(src)?.aij.clone();
        self.get_mut(dst)?.aij.copy_profile(&src_m);
        Ok(())
    }
    fn add_matrix(&mut self, dst: &str, src: &str, alpha: Real) -> Result<()> {
        // `dst == src` (upstream `K.add(K, alpha)`):
        // adding a copy is `K ← (1 + α) K`
        let s = self.get(src)?.aij.clone();
        let src_unsym = self.get(src)?.effective != MatrixType::Symmetric;
        let d = self.get_mut(dst)?;
        d.aij.add_matrix(&s, alpha)?;
        // Upstream MatAXPY: the sum of an unsymmetric matrix is unsymmetric.
        if src_unsym && d.effective == MatrixType::Symmetric {
            d.effective = MatrixType::Unsymmetric;
        }
        Ok(())
    }
    fn copy_matrix(&mut self, id: &str, src: &str) -> Result<()> {
        if self.matrices.contains_key(id) {
            return Err(Error::AlreadyExists {
                kind: "matrix",
                id: id.into(),
            });
        }
        let s = self.get(src)?;
        let mut m = PetscMatrix::new(id, s.declared, s.aij.size());
        m.aij = SparseMatrixAij::copy_with_id(&s.aij, id);
        m.effective = s.effective;
        self.matrices.insert(id.into(), m);
        Ok(())
    }
    /// Every matrix of this backend is solved by PETSc's KSP.
    fn sparse_solver_kind(&self, id: &str) -> Option<SparseSolverType> {
        let _ = id;
        Some(SparseSolverType::Petsc)
    }
}

// ------------------------------------------------------------ native solves

#[derive(Debug)]
enum NativeSolver {
    Snes(Snes),
    Tao(Tao),
}

/// Native objects of one SNES/TAO solve (owned by the driver while the
/// solver runs; their destruction is collective, queued in SPMD order).
#[derive(Debug)]
struct NativeObjects {
    solver: NativeSolver,
    jacobian: Matrix,
    x: Vector,
    r: Vector,
    /// TAO: work vector of the objective's dot products (with `r`).
    w: Option<Vector>,
    /// Owned local equations in PETSc order.
    owned: Vec<usize>,
    /// TAO bounds on the increment, owned rows.
    bounds: Option<(Vec<Real>, Vec<Real>)>,
    _holder: Arc<()>,
    _db: Options,
}

/// SNES/TAO driver of the PETSc backend (upstream
/// `NonLinearSolverPETSc::solve` / `NonLinearSolverTAO::solve`, native part).
#[derive(Debug)]
pub struct PetscNativeDriver {
    kind: NonLinearSolverType,
    request: NativeRequest,
    report: Option<NativeReport>,
}

/// The callbacks of a native solve: owned rows ↔ local-equation vectors,
/// trampolines of the solve's context.
struct Callbacks<'c, 'a, 'n> {
    ctx: &'c mut NativeSolveContext<'a>,
    owned: &'c [usize],
    jacobian: &'c Matrix,
    nc: NativeComm<'n>,
    x: Vec<Real>,
    f: Vec<Real>,
    /// TAO: PETSc work vectors of the objective's dot products.
    work: Option<(&'c mut Vector, &'c mut Vector)>,
}

impl Callbacks<'_, '_, '_> {
    fn scatter(&mut self, x_owned: &[Real]) {
        let rank = self.ctx.communicator().rank();
        if GLUE_PANIC_RANK.load(Ordering::SeqCst) == rank
            && GLUE_PANIC_RANK
                .compare_exchange(rank, -1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            // The test hook (`inject_glue_panic`): a glue-code defect
            // on this rank only.
            std::panic::panic_any("akantu-rs: injected panic in the PETSc callback glue");
        }
        self.x.fill(0.0);
        for (k, &l) in self.owned.iter().enumerate() {
            self.x[l] = x_owned[k];
        }
    }

    /// Jacobian at `x` (form_jacobian, then the PETSc copy updated).
    fn assemble_jacobian(&mut self) -> bool {
        let x = std::mem::take(&mut self.x);
        let ok = self.ctx.form_jacobian(&x) == 0;
        self.x = x;
        ok && self.ctx.sync_native() == 0
    }
}

impl NativeCallbacks for Callbacks<'_, '_, '_> {
    fn residual(&mut self, x_owned: &[Real], f_owned: &mut [Real]) -> bool {
        self.scatter(x_owned);
        let x = std::mem::take(&mut self.x);
        let mut f = std::mem::take(&mut self.f);
        let ok = self.ctx.form_function(&x, &mut f) == 0;
        if ok {
            for (k, &l) in self.owned.iter().enumerate() {
                f_owned[k] = f[l];
            }
        }
        self.x = x;
        self.f = f;
        ok
    }

    fn jacobian(&mut self, x_owned: &[Real]) -> bool {
        self.scatter(x_owned);
        self.assemble_jacobian()
    }

    /// Upstream `NonLinearSolverTAO::computeObjectiveGradient`: `rhs = F(x)`
    /// (= −residual, blocked rows zero), `J` assembled, `grad = J x + rhs`,
    /// `obj = ½ xᵀ J x + rhsᵀ x`.
    fn objective(&mut self, x_owned: &[Real], grad: &mut [Real]) -> Option<Real> {
        self.scatter(x_owned);
        let x = std::mem::take(&mut self.x);
        let mut f = std::mem::take(&mut self.f);
        let ok = self.ctx.form_function(&x, &mut f) == 0;
        self.x = x;
        if !ok || !self.assemble_jacobian() {
            self.f = f;
            return None;
        }
        let n = x_owned.len();
        let mut jx = vec![0.0; n];
        // Collective MatMult on the PETSc J (every rank reaches it: the
        // previous statuses are agreed).
        let mult = self.jacobian.mult(&self.nc, x_owned, &mut jx);
        let mut jx_local = vec![0.0; self.x.len()];
        for (k, &l) in self.owned.iter().enumerate() {
            jx_local[l] = jx[k];
        }
        if mult.is_err() {
            self.f = f;
            return None;
        }
        let x = std::mem::take(&mut self.x);
        let (fx, xjx) = match self.work.as_mut() {
            // Upstream's `VecDot(rhs, x, &fx)` and `VecDot(x, Jx, &xJx)`:
            // PETSc's dot products (BLAS `ddot`, then PETSc's reduction),
            // so that the objective has upstream's rounding.
            Some((a, b)) => {
                let f_owned: Vec<Real> = self.owned.iter().map(|&l| f[l]).collect();
                let dots = a
                    .set_owned(&f_owned)
                    .and_then(|()| b.set_owned(x_owned))
                    .and_then(|()| a.dot(&self.nc, b))
                    .and_then(|fx| {
                        a.set_owned(&jx)?;
                        Ok((fx, b.dot(&self.nc, a)?))
                    });
                match dots {
                    Ok((fx, xjx)) => (Ok(fx), Ok(xjx)),
                    Err(_) => (Err(1), Err(1)),
                }
            }
            None => (
                self.ctx.owned_dot(&f, &x),
                self.ctx.owned_dot(&x, &jx_local),
            ),
        };
        self.x = x;
        for (k, &l) in self.owned.iter().enumerate() {
            grad[k] = jx[k] + f[l];
        }
        self.f = f;
        match (fx, xjx) {
            (Ok(fx), Ok(xjx)) => Some(0.5 * xjx + fx),
            _ => None,
        }
    }
}

/// Outcome of the native region.
enum Outcome {
    Snes(SnesOutcome),
    Tao(TaoOutcome),
}

impl NativeNonLinearDriver for PetscNativeDriver {
    fn solve(&mut self, ctx: &mut NativeSolveContext<'_>) -> Result<(usize, Real, bool)> {
        let id = ctx.matrix_id().to_owned();
        let request = self.request.clone();
        // 1. Native objects (agreed collective of the solve's scope).
        let mut objects = ctx.with_backend("petsc:native:prepare", |b, bctx| {
            let backend = b
                .as_any_mut()
                .and_then(|a| a.downcast_mut::<PetscBackend>())
                .ok_or_else(|| Error::invalid("PETSc native solve on another backend"))?;
            backend.prepare_native(&request, &id, bctx)
        })?;
        let n_local = ctx.nb_local();
        // 2. Admission of the native region, agreed on every rank.
        let comm = ctx.communicator().clone();
        let what = match self.kind {
            NonLinearSolverType::PetscTao => "TaoSolve",
            _ => "SNESSolve",
        };
        // The ticket's registration marks this thread from its admission on:
        // the agreement below and the region's own
        // native-policy reductions pass the admission rule. The PETSc lock
        // is taken inside the region (lock order MPI → PETSc).
        let ticket = comm.native_ticket(what);
        let verdict = ticket.verdict();
        ctx.agree(&format!("petsc:native:enter:{what}"), verdict)?;
        // 3. The native region: PETSc's own collectives and the agreed
        // callbacks run on this thread, in lock-step on every rank.
        let NativeObjects {
            solver,
            jacobian,
            x,
            r,
            w,
            owned,
            bounds,
            ..
        } = &mut objects;
        let jacobian: &Matrix = jacobian;
        let owned: &[usize] = owned;
        let aliased = self.request.upstream_aliasing;
        let n_owned = owned.len();
        // A panic caught by a trampoline during this solve.
        let _ = petsc::take_caught_panic();
        let region = comm.native_region(ticket, what, |nc| {
            let _petsc = PetscLock::enter_native(&nc);
            let mut cb = Callbacks {
                ctx: &mut *ctx,
                owned,
                jacobian,
                nc,
                x: vec![0.0; n_local],
                f: vec![0.0; n_local],
                work: None,
            };
            let outcome = match solver {
                NativeSolver::Snes(s) => s
                    .solve(&nc, jacobian, x, r, &mut cb, aliased)
                    .map(Outcome::Snes),
                NativeSolver::Tao(t) => {
                    cb.work = w.as_mut().map(|w| (&mut *r, w));
                    let b = bounds
                        .as_ref()
                        .map(|(lo, hi)| (lo.as_slice(), hi.as_slice()));
                    t.solve(&nc, jacobian, x, b, &mut cb, aliased)
                        .map(Outcome::Tao)
                }
            };
            // The final iterate, read while this thread holds the PETSc
            // lock (a local copy; no rank-local refusal after the region).
            outcome.and_then(|o| {
                let mut x_owned = vec![0.0; n_owned];
                x.get_owned(&mut x_owned).map(|()| (o, x_owned))
            })
        });
        let caught_panic = petsc::take_caught_panic();
        let (outcome, x_owned) = match region {
            Ok(Ok(o)) => o,
            // A callback failure (agreed) made PETSc return an error: the
            // stored error is re-raised by the caller.
            Ok(Err(e)) if ctx.has_error() => return Err(e),
            // A panic in the glue code of a trampoline on this rank
            // only (model callbacks are agreed): the other ranks may
            // be blocked in PETSc's collectives without a deadline, so the
            // job ends here (`docs/ARCHITECTURE.md`) instead of relying on the
            // application to apply the abort policy.
            Ok(Err(e)) if caught_panic => {
                let err = fatal(Error::invalid(format!(
                    "{what}: a panic in the PETSc callback glue code on rank {} ({e})",
                    comm.rank()
                )));
                // The detector reports the failure (the abort's own note
                // carries only the rank and the error code:
                // `abort_on_transport_fatal`).
                {
                    use std::io::Write as _;
                    let _ = writeln!(std::io::stderr(), "akantu: {err}");
                }
                comm.abort_on_transport_fatal(&err);
                return Err(err);
            }
            Ok(Err(e)) => return Err(fatal(e)),
            Err(e) => return Err(e),
        };
        // 4. Final increment (upstream: splitSolutionPerDOFs,
        // restoreLastConvergedStep, corrector).
        let mut x_local = vec![0.0; n_local];
        for (k, &l) in owned.iter().enumerate() {
            x_local[l] = x_owned[k];
        }
        let finished = ctx.finish(&x_local) == 0;
        let (iterations, error, converged, report) = match outcome {
            Outcome::Snes(o) => {
                let error = o.error();
                let first = o.history.first().copied().unwrap_or(f64::NAN);
                let report = NativeReport {
                    reason: o.reason,
                    reason_string: o.reason_string.clone(),
                    function_norm: o.function_norm,
                    solution_norm: o.solution_norm,
                    update_norm: o.update_norm,
                    atol: o.tolerances.0,
                    rtol: o.tolerances.1,
                    stol: o.tolerances.2,
                    divtol: o.tolerances.3,
                    max_iterations: o.limits.0,
                    relative_norm: o.function_norm / first,
                    objective: 0.0,
                };
                (o.iterations, error, o.reason >= 0, report)
            }
            Outcome::Tao(o) => {
                // The final gradient norm (what TAO compares with
                // gatol) is the solver's error; upstream's exception carries
                // reason, iterations and tolerances
                // (`TAONotConvergedException`, see
                // `NativeReport::tao_not_converged_message`).
                let report = NativeReport {
                    reason: o.reason,
                    reason_string: format!("TaoConvergedReason {}", o.reason),
                    function_norm: o.gradient_norm,
                    objective: o.objective,
                    atol: o.tolerances.0,
                    rtol: o.tolerances.1,
                    stol: o.tolerances.2,
                    max_iterations: o.max_iterations,
                    relative_norm: o.gradient_norm,
                    ..NativeReport::default()
                };
                (o.iterations, o.gradient_norm, o.reason >= 0, report)
            }
        };
        self.report = Some(report);
        let converged = converged && finished;
        Ok((iterations, error, converged))
    }

    fn report(&self) -> Option<NativeReport> {
        self.report.clone()
    }
}
