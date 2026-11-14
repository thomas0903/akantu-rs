// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract stand-ins derived from
// src/model/common/dof_manager/dof_manager_petsc.{hh,cc} (AO numbering,
// :167-235), src/solver/sparse_matrix_petsc.cc:262-268 (MatSetValues),
// src/solver/sparse_solver_mumps.cc:140-285 (distributed assembled matrix,
// centralised RHS/solution), src/model/common/non_linear_solver/
// non_linear_solver_petsc.cc:100-200 (SNES callbacks). Test support only.
#![allow(dead_code)] // each test binary uses a subset of the helpers

//! Test support for the solver contract tests:
//! a native backend stand-in, a dense direct
//! solver for the AIJ backend, 1D bar meshes (serial and 2 ranks) and a
//! non-linear spring physics.

pub mod drivers;
pub mod my_model;
pub mod oracle;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use akantu_comm::{Communicator, Synchronizer, Tag};
use akantu_core::enums::{MatrixType, SparseSolverType, SynchronizationTag};
use akantu_core::{
    Array, ElementType, Error, FatalKind, GhostType, GlobalIdx, Id, Idx, NodeFlag, Real, Result,
};
use akantu_linalg::direct::{
    BackendCapabilities, MatrixDistribution, RhsDistribution, SparseDirectSolver,
};
use akantu_linalg::{EquationLayout, PartialSumReduction, SparseMatrixAij};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::{
    BackendContext, BackendKind, DofManager, LinearSystemBackend, NativeNonLinearDriver,
    NativeSolveContext, SolverCallback,
};

pub type Log = Arc<Mutex<Vec<String>>>;

pub const SEG: ElementType = ElementType::Segment2;

/// Dense LU with partial pivoting (column-major), test stand-in for a
/// direct solver.
pub fn dense_solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Result<Vec<f64>> {
    for k in 0..n {
        let p = (k..n)
            .max_by(|&i, &j| a[i + k * n].abs().total_cmp(&a[j + k * n].abs()))
            .unwrap_or(k);
        if a[p + k * n] == 0.0 {
            return Err(Error::LinearSolver("singular matrix".into()));
        }
        if p != k {
            for j in 0..n {
                a.swap(k + j * n, p + j * n);
            }
            b.swap(k, p);
        }
        for i in k + 1..n {
            let f = a[i + k * n] / a[k + k * n];
            for j in k..n {
                a[i + j * n] -= f * a[k + j * n];
            }
            b[i] -= f * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in i + 1..n {
            s -= a[i + j * n] * x[j];
        }
        x[i] = s / a[i + i * n];
    }
    Ok(x)
}

// ------------------------------------------------------------- native mock

#[derive(Debug, Clone)]
struct NativeMat {
    ty: MatrixType,
    size: usize,
    /// Rank-partial entries in application ordering (AO) coordinates.
    entries: BTreeMap<(usize, usize), Real>,
    /// Generation: bumped by reset/promotion (re-analysis needed).
    generation: u64,
    analysed: Option<(u64, MatrixType)>,
}

/// "Native" backend: rank-partial storage in an application ordering
/// `ao(g) = n − 1 − g` applied to rows, columns **and** vectors (UD-006),
/// MUMPS-shaped solve (distributed assembled entries + centralised
/// right-hand side gathered to rank 0 through the phase scope, solution
/// scattered back by global equation), a native Newton driver, and no COO
/// access.
#[derive(Debug, Default)]
pub struct MockNative {
    pub log: Log,
    mats: BTreeMap<Id, NativeMat>,
    /// Injected failures (collective-failure tests): `promote_to_unsymmetric`
    /// fails atomically (before any change), `reset_profile` fails,
    /// `native_non_linear` refuses to hand out a driver, `mat_vec` fails
    /// before reaching the reduction.
    pub fail_promote: bool,
    pub fail_reset: bool,
    pub fail_driver: bool,
    pub fail_mat_vec: bool,
    /// The native driver returns an `Error::Fatal` of this kind as its
    /// outcome (a native backend reporting an unusable state).
    pub fatal_outcome: Option<FatalKind>,
    /// The native driver first evaluates `F` once (a trampoline, which may
    /// fail through an injected callback error), then returns this error
    /// as its outcome — ordinary (`Some(None)`) or `Error::Fatal` of the
    /// given kind (an unusable state detected while tearing down a
    /// failed solve).
    pub late_outcome: Option<Option<FatalKind>>,
}

impl MockNative {
    pub fn new(log: Log) -> Self {
        Self {
            log,
            ..Self::default()
        }
    }

    fn ao(n: usize, g: GlobalIdx) -> Result<usize> {
        let g = usize::try_from(g).map_err(|_| Error::invalid("negative global"))?;
        if g >= n {
            return Err(Error::invalid(format!("global {g} outside order {n}")));
        }
        Ok(n - 1 - g)
    }

    fn mat(&mut self, id: &str) -> Result<&mut NativeMat> {
        self.mats
            .get_mut(id)
            .ok_or_else(|| Error::not_found("native matrix", id))
    }

    fn note(&self, s: String) {
        self.log.lock().unwrap().push(s);
    }
}

impl LinearSystemBackend for MockNative {
    fn kind(&self) -> BackendKind {
        BackendKind::Other("mock-native")
    }
    fn new_matrix(&mut self, id: &str, ty: MatrixType, size: GlobalIdx) -> Result<()> {
        self.mats.insert(
            id.into(),
            NativeMat {
                ty,
                size: usize::try_from(size).unwrap(),
                entries: BTreeMap::new(),
                generation: 0,
                analysed: None,
            },
        );
        Ok(())
    }
    fn has_matrix(&self, id: &str) -> bool {
        self.mats.contains_key(id)
    }
    fn matrix_ids(&self) -> Vec<Id> {
        self.mats.keys().cloned().collect()
    }
    fn matrix_type(&self, id: &str) -> Result<MatrixType> {
        self.mats
            .get(id)
            .map(|m| m.ty)
            .ok_or_else(|| Error::not_found("native matrix", id))
    }
    fn reset_profile(&mut self, id: &str, size: GlobalIdx) -> Result<()> {
        if self.fail_reset {
            return Err(Error::Backend {
                backend: "mock-native",
                code: 73,
                message: format!("injected reset failure on {id}"),
            });
        }
        let m = self.mat(id)?;
        m.size = usize::try_from(size).unwrap();
        m.entries.clear();
        m.generation += 1;
        self.note(format!("reset {id} {size}"));
        Ok(())
    }
    fn promote_to_unsymmetric(&mut self, id: &str) -> Result<()> {
        if self.fail_promote {
            // Atomic: rejected before any change (the backend contract).
            return Err(Error::Backend {
                backend: "mock-native",
                code: 63,
                message: format!("injected promotion failure on {id}"),
            });
        }
        let m = self.mat(id)?;
        if m.ty != MatrixType::Unsymmetric {
            m.ty = MatrixType::Unsymmetric;
            m.generation += 1;
            self.note(format!("promote {id}"));
        }
        Ok(())
    }
    fn add_block_to_profile(&mut self, _: &str, _: &[GlobalIdx], _: &[GlobalIdx]) -> Result<()> {
        Ok(()) // native preallocation
    }
    fn add_values(
        &mut self,
        id: &str,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        v: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        let m = self.mat(id)?;
        let n = m.size;
        // Validate first (atomic), skip equations past the order (pure
        // ghosts), local promotion proposal as the AIJ backend.
        let mut staged = Vec::new();
        for (j, &gj) in cols.iter().enumerate() {
            for (i, &gi) in rows.iter().enumerate() {
                if gi >= m.size as GlobalIdx || gj >= m.size as GlobalIdx {
                    continue;
                }
                staged.push(((Self::ao(n, gi)?, Self::ao(n, gj)?), v[i + j * rows.len()]));
            }
        }
        if m.ty == MatrixType::Symmetric && values_type != MatrixType::Symmetric {
            m.ty = MatrixType::Unsymmetric;
            m.generation += 1;
        }
        for (k, x) in staged {
            *m.entries.entry(k).or_insert(0.0) += x;
        }
        let msg = format!("set {id} {rows:?}x{cols:?}");
        self.note(msg);
        Ok(())
    }
    fn add_terms(&mut self, id: &str, terms: &[(GlobalIdx, GlobalIdx, Real)]) -> Result<()> {
        let m = self.mat(id)?;
        let n = m.size;
        let staged: Vec<_> = terms
            .iter()
            .map(|&(i, j, v)| Ok(((Self::ao(n, i)?, Self::ao(n, j)?), v)))
            .collect::<Result<_>>()?;
        for (k, v) in staged {
            *m.entries.entry(k).or_insert(0.0) += v;
        }
        Ok(())
    }
    fn finalize_assembly(&mut self, id: &str, ctx: &mut BackendContext<'_>) -> Result<()> {
        // Collective like MatAssemblyEnd: agree that every rank holds the
        // same order (a numbering mismatch is detected collectively).
        let size = self.mat(id)?.size;
        ctx.phases.run(
            &format!("native:assembly_end:{id}"),
            Ok(size as u64),
            |c, n| {
                let mut v = [n, !n];
                c.all_reduce(&mut v, akantu_comm::ReduceOp::Max)?;
                Ok(if v[0] == !v[1] {
                    Ok(())
                } else {
                    Err(Error::invalid("ranks disagree on the matrix order"))
                })
            },
        )?;
        self.note(format!("assembly_end {id}"));
        Ok(())
    }
    fn zero(&mut self, id: &str) -> Result<()> {
        for v in self.mat(id)?.entries.values_mut() {
            *v = 0.0;
        }
        Ok(())
    }
    fn apply_boundary(
        &mut self,
        id: &str,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        let m = self.mat(id)?;
        let n = m.size;
        let mut blocked_ao = std::collections::BTreeSet::new();
        let mut owned_diag = Vec::new();
        for (l, &b) in blocked.iter().enumerate() {
            if let (true, Some(g)) = (b, layout.global(l)) {
                let a = Self::ao(n, g)?;
                blocked_ao.insert(a);
                if layout.is_owned(l) {
                    owned_diag.push(a);
                }
            }
        }
        for (&(i, j), v) in &mut m.entries {
            if blocked_ao.contains(&i) || blocked_ao.contains(&j) {
                *v = 0.0;
            }
        }
        for a in owned_diag {
            m.entries.insert((a, a), block_val);
        }
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
        if self.fail_mat_vec {
            // A local backend failure **before** the collective reduction.
            return Err(Error::Backend {
                backend: "mock-native",
                code: 91,
                message: "injected mat_vec failure".into(),
            });
        }
        let m = self
            .mats
            .get(id)
            .ok_or_else(|| Error::not_found("native matrix", id))?;
        let n = m.size;
        // Rank-partial product over the local entries (AO → local), then
        // the manager's reduction (sum onto owners, copies synchronised).
        let mut local_of = vec![None; n];
        for l in 0..layout.nb_local() {
            if let Some(g) = layout.global(l) {
                local_of[Self::ao(n, g)?] = Some(l);
            }
        }
        let mut tmp = vec![0.0; layout.nb_local()];
        for (&(i, j), &a) in &m.entries {
            if let (Some(li), Some(lj)) = (local_of[i], local_of[j]) {
                tmp[li] += alpha * a * x[lj];
            }
        }
        if reduction.is_distributed() {
            reduction.reduce_and_synchronize(&mut tmp)?;
        }
        for (yi, t) in y.iter_mut().zip(tmp) {
            *yi = beta * *yi + t;
        }
        Ok(())
    }
    fn solve(
        &mut self,
        id: &str,
        ctx: &mut BackendContext<'_>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        let layout = ctx.layout;
        let (rank, size) = ctx.phases.rank_size();
        // Prepared locally: rank-partial entries (AO) and owned RHS rows.
        // (entries, owned rhs rows, order, type, re-analysis needed)
        type Prepared = (Vec<f64>, Vec<f64>, usize, MatrixType, bool);
        let prepared = (|| -> Result<Prepared> {
            let m = self
                .mats
                .get(id)
                .ok_or_else(|| Error::not_found("native matrix", id))?;
            if rhs.len() != layout.nb_local() {
                return Err(Error::invalid("rhs length"));
            }
            let entries: Vec<f64> = m
                .entries
                .iter()
                .flat_map(|(&(i, j), &v)| [i as f64, j as f64, v])
                .collect();
            let mut owned = Vec::new();
            for (l, &r) in rhs.iter().enumerate() {
                if layout.is_owned(l)
                    && let Some(g) = layout.global(l)
                {
                    owned.extend([Self::ao(m.size, g)? as f64, r]);
                }
            }
            let reanalyse = m.analysed != Some((m.generation, m.ty));
            Ok((entries, owned, m.size, m.ty, reanalyse))
        })();
        let (sym, reanalyse) = match &prepared {
            Ok((_, _, _, ty, re)) => (if *ty == MatrixType::Symmetric { 2 } else { 0 }, *re),
            Err(_) => (0, false),
        };
        let mine = ctx.phases.run(
            &format!("native:solve:{id}"),
            prepared,
            |c, (entries, owned, n, _, _)| {
                let counts = c.all_gather(&[entries.len() as u64, owned.len() as u64])?;
                if rank == 0 {
                    let mut all_entries = vec![entries];
                    let mut all_owned = vec![owned];
                    for r in 1..size {
                        let src = r as i32;
                        let e = c.irecv::<f64>(counts[2 * r] as usize, src, Tag(77))?;
                        let o = c.irecv::<f64>(counts[2 * r + 1] as usize, src, Tag(78))?;
                        all_entries.push(e.wait()?);
                        all_owned.push(o.wait()?);
                    }
                    // Root: sum distributed entries, solve in AO order.
                    let mut a = vec![0.0; n * n];
                    for e in &all_entries {
                        for t in e.chunks(3) {
                            a[t[0] as usize + t[1] as usize * n] += t[2];
                        }
                    }
                    let mut b = vec![0.0; n];
                    for o in &all_owned {
                        for t in o.chunks(2) {
                            b[t[0] as usize] = t[1];
                        }
                    }
                    let x = match dense_solve(a, b, n) {
                        Ok(x) => x,
                        Err(e) => {
                            // Every rank still receives its (empty) reply.
                            for r in 1..size {
                                let len = counts[2 * r + 1] as usize / 2;
                                c.isend(vec![f64::NAN; len], r as i32, Tag(79))?.wait()?;
                            }
                            return Ok(Err(e));
                        }
                    };
                    let mut sends = Vec::new();
                    for (r, o) in all_owned.iter().enumerate().skip(1) {
                        let v: Vec<f64> = o.chunks(2).map(|t| x[t[0] as usize]).collect();
                        sends.push(c.isend(v, r as i32, Tag(79))?);
                    }
                    for s in sends {
                        s.wait()?;
                    }
                    Ok(Ok(all_owned[0]
                        .chunks(2)
                        .map(|t| (t[0] as usize, x[t[0] as usize]))
                        .collect::<Vec<_>>()))
                } else {
                    let s1 = c.isend(entries, 0, Tag(77))?;
                    let reply = c.irecv::<f64>(owned.len() / 2, 0, Tag(79))?;
                    let s2 = c.isend(owned.clone(), 0, Tag(78))?;
                    s1.wait()?;
                    s2.wait()?;
                    let x = reply.wait()?;
                    if x.iter().any(|v| v.is_nan()) {
                        return Ok(Err(Error::LinearSolver("root solve failed".into())));
                    }
                    Ok(Ok(owned
                        .chunks(2)
                        .zip(x)
                        .map(|(t, v)| (t[0] as usize, v))
                        .collect()))
                }
            },
        )?;
        let m = self.mat(id)?;
        let n = m.size;
        if reanalyse {
            m.analysed = Some((m.generation, m.ty));
        }
        for (a, v) in mine {
            let g = (n - 1 - a) as GlobalIdx;
            solution[layout.local(g)?] = v;
        }
        if reanalyse {
            self.note(format!("analysis {id} sym={sym}"));
        }
        self.note(format!("solve {id}"));
        Ok(())
    }
    fn native_non_linear(&mut self, id: &str) -> Result<Box<dyn NativeNonLinearDriver>> {
        if self.fail_driver {
            return Err(Error::Backend {
                backend: "mock-native",
                code: 85,
                message: "injected SNES creation failure".into(),
            });
        }
        self.note(format!("snes {id}"));
        Ok(Box::new(MockSnes {
            tolerance: 1e-12,
            max_iterations: 20,
            fatal_outcome: self.fatal_outcome,
            late_outcome: self.late_outcome,
        }))
    }
    fn aij(&self, _: &str) -> Result<&SparseMatrixAij> {
        Err(Error::Unsupported("COO access on a native backend".into()))
    }
}

/// Native Newton (SNES stand-in): trial solution `x` in local equations,
/// `F(x)` and `J(x)` through the context trampolines, linear solves with
/// the backend-owned matrix, convergence on the owned-row norm of `F`.
#[derive(Debug)]
pub struct MockSnes {
    tolerance: f64,
    max_iterations: usize,
    fatal_outcome: Option<FatalKind>,
    late_outcome: Option<Option<FatalKind>>,
}

impl NativeNonLinearDriver for MockSnes {
    fn solve(&mut self, ctx: &mut NativeSolveContext<'_>) -> Result<(usize, Real, bool)> {
        if let Some(kind) = self.fatal_outcome {
            return Err(Error::fatal(
                kind,
                Error::Backend {
                    backend: "mock-native",
                    code: 76,
                    message: "injected fatal driver outcome".into(),
                },
            ));
        }
        let n = ctx.nb_local();
        let mut x = vec![0.0; n];
        let mut f = vec![0.0; n];
        let mut dx = vec![0.0; n];
        if let Some(kind) = self.late_outcome {
            let _ = ctx.form_function(&x, &mut f);
            let e = Error::Backend {
                backend: "mock-native",
                code: 77,
                message: "injected late driver outcome".into(),
            };
            return Err(match kind {
                Some(k) => Error::fatal(k, e),
                None => e,
            });
        }
        for it in 0..=self.max_iterations {
            if ctx.form_function(&x, &mut f) != 0 {
                return Ok((it, f64::NAN, false));
            }
            let norm = match ctx.owned_norm(&f) {
                Ok(v) => v,
                Err(_) => return Ok((it, f64::NAN, false)),
            };
            if norm < self.tolerance {
                return Ok((it, norm, true));
            }
            if ctx.form_jacobian(&x) != 0 {
                return Ok((it, norm, false));
            }
            // J dx = -F  (F = -r already, so solve J dx = r = -F)
            let rhs: Vec<f64> = f.iter().map(|v| -v).collect();
            if ctx.linear_solve(&rhs, &mut dx) != 0 {
                return Ok((it, norm, false));
            }
            for (xi, d) in x.iter_mut().zip(&dx) {
                *xi += d;
            }
        }
        Ok((self.max_iterations, f64::NAN, false))
    }
}

// --------------------------------------------------------- AIJ direct solver

/// Dense test solver behind the AIJ backend (records analyses).
#[derive(Debug)]
pub struct DenseDirect {
    pub log: Log,
    a: Vec<f64>,
    n: usize,
}

impl DenseDirect {
    pub fn new(log: Log) -> Self {
        Self {
            log,
            a: Vec::new(),
            n: 0,
        }
    }
}

impl SparseDirectSolver for DenseDirect {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Eigen
    }
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            matrix: MatrixDistribution::CentralizedOnRoot,
            rhs: RhsDistribution::CentralizedOnRoot,
            max_global_index: akantu_core::MAX_GLOBAL_IDX,
            max_local_nnz: u64::MAX,
            max_centralized_size: Some(1000),
        }
    }
    fn analysis(&mut self, a: &SparseMatrixAij) -> Result<()> {
        self.log.lock().unwrap().push(format!(
            "analysis {} sym={}",
            a.id(),
            akantu_linalg::direct::mumps_symmetry(a)
        ));
        Ok(())
    }
    fn factorize(&mut self, a: &SparseMatrixAij) -> Result<()> {
        let n = usize::try_from(a.size()).unwrap();
        let sym = a.matrix_type() == MatrixType::Symmetric;
        let mut d = vec![0.0; n * n];
        let (irn, jcn, v) = a.coo();
        for ((&i, &j), &x) in irn.iter().zip(jcn).zip(v) {
            let (i, j) = (i as usize, j as usize);
            d[i + j * n] += x;
            if sym && i != j {
                d[j + i * n] += x;
            }
        }
        self.a = d;
        self.n = n;
        self.log
            .lock()
            .unwrap()
            .push(format!("factorize {}", a.id()));
        Ok(())
    }
    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        let s = dense_solve(self.a.clone(), b.to_vec(), self.n)?;
        x.copy_from_slice(&s);
        Ok(())
    }
}

// ------------------------------------------------------------------ meshes

/// Serial bar: nodes at x = 0..n_el, `n_el` segments.
pub fn serial_bar(n_el: usize) -> Mesh {
    let mut mesh = Mesh::new(1, "bar").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for i in 0..=n_el {
        acc.push_node(&[i as f64]).unwrap();
    }
    for i in 0..n_el {
        let i = i as Idx;
        acc.connectivity_mut(SEG, GhostType::NotGhost)
            .push_tuple(&[i, i + 1]);
    }
    mesh
}

/// Two-rank bar g0..g4: rank 0 holds g0 g1 g2(master) with [g0,g1]
/// [g1,g2]; rank 1 holds g2(slave) g3 g4 with [g2,g3] [g3,g4]. Node
/// synchronizer: rank 0 sends g2 to rank 1.
pub fn two_rank_bar(comm: &Communicator) -> Mesh {
    let mut mesh = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
    let r = comm.rank();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let (xs, flags) = if r == 0 {
            (
                [0.0, 1.0, 2.0],
                [NodeFlag::NORMAL, NodeFlag::NORMAL, NodeFlag::MASTER],
            )
        } else {
            (
                [2.0, 3.0, 4.0],
                [NodeFlag::SLAVE, NodeFlag::NORMAL, NodeFlag::NORMAL],
            )
        };
        for (x, f) in xs.iter().zip(flags) {
            let n = acc.push_node(&[*x]).unwrap();
            acc.nodes_flags_mut().as_mut_slice()[n as usize] = f;
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
        drop(c);
        let mut sync = Synchronizer::new("nodes", comm.clone());
        if r == 0 {
            sync.communications.send.insert(1, vec![2]);
        } else {
            sync.communications.recv.insert(0, vec![0]);
            acc.set_node_prank(0, 0);
        }
        acc.synchronizers_mut().node = Some(sync);
        acc.set_distributed();
    }
    mesh
}

// ----------------------------------------------------------------- physics

/// Non-linear springs `N(ε) = k (ε + ε³)` on unit segments, node at x = 0
/// blocked, force `P` at x = `x_load`. With `k = 1`, `P = 2` the exact
/// solution is `ε = 1`, i.e. `u = x`.
pub struct Springs {
    pub mesh: Mesh,
    pub x_load: f64,
    pub load: f64,
    /// `true`: the corrector adds the solution (increment) to `u`
    /// (akantu Newton); `false`: the solution is the total `u` (SNES).
    pub incremental: bool,
    pub fail_residual_at: Option<usize>,
    pub residual_calls: usize,
    /// Injected failures (rank-specific tests).
    pub fail_corrector: bool,
    pub fail_assemble_matrix: bool,
    pub fail_before_solve_step: bool,
    pub fail_predictor: bool,
    pub fail_after_solve_step: bool,
    /// Old (defective) style: the residual reduction opens its own scope.
    pub independent_scope: bool,
    /// Injected callback failures are `Error::Fatal` of this kind (a
    /// callback propagating an unusable dependent owner's error).
    pub fatal: Option<FatalKind>,
}

impl Springs {
    pub fn new(mesh: Mesh, x_load: f64, incremental: bool) -> Self {
        Self {
            mesh,
            x_load,
            load: 2.0,
            incremental,
            fail_residual_at: None,
            residual_calls: 0,
            fail_corrector: false,
            fail_assemble_matrix: false,
            fail_before_solve_step: false,
            fail_predictor: false,
            fail_after_solve_step: false,
            fatal: None,
            independent_scope: false,
        }
    }

    /// An injected failure, typed fatal when [`Springs::fatal`] is set.
    fn injected(&self, e: Error) -> Error {
        match self.fatal {
            Some(kind) => Error::fatal(kind, e),
            None => e,
        }
    }

    pub fn setup(&self, dm: &mut DofManager) {
        dm.register_nodal_dofs(&self.mesh, "u", 1).unwrap();
        dm.register_blocked_dofs("u").unwrap();
        for n in 0..self.mesh.nb_nodes() {
            if self.mesh.nodes().as_slice()[n] == 0.0 {
                dm.family_mut("u")
                    .unwrap()
                    .blocked
                    .as_mut()
                    .unwrap()
                    .borrow_mut()
                    .as_mut_slice()[n] = true;
            }
        }
    }

    fn strains(&self, dm: &DofManager) -> Vec<f64> {
        let u = dm.dofs("u").unwrap();
        let u = u.as_slice();
        let c = self.mesh.connectivity(SEG, GhostType::NotGhost).unwrap();
        (0..c.len())
            .map(|e| {
                let t = c.tuple(e);
                u[t[1] as usize] - u[t[0] as usize]
            })
            .collect()
    }
}

/// Slave → master sum of a nodal array over the node synchronizer.
struct NodalSum<'a>(&'a mut [f64]);

impl akantu_comm::DataAccessor<Idx> for NodalSum<'_> {
    fn nb_data(&self, n: &[Idx], _: SynchronizationTag) -> usize {
        n.len() * 8
    }
    fn pack_data(
        &self,
        b: &mut akantu_comm::CommunicationBuffer,
        n: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &i in n {
            b.pack(&self.0[i as usize]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut akantu_comm::CommunicationBuffer,
        n: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &i in n {
            self.0[i as usize] += b.unpack::<f64>()?;
        }
        Ok(())
    }
}

impl SolverCallback for Springs {
    fn matrix_type(&self, _: &str) -> MatrixType {
        MatrixType::Symmetric
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        if self.fail_assemble_matrix {
            return Err(self.injected(Error::Numerical("injected assemble_matrix failure".into())));
        }
        if !dm.backend().has_matrix(id) {
            dm.new_matrix(id, MatrixType::Symmetric)?;
        }
        dm.backend_zero(id)?;
        let mut ke = Array::new(4);
        for eps in self.strains(dm) {
            let k = 1.0 + 3.0 * eps * eps;
            ke.push_tuple(&[k, -k, -k, k]);
        }
        dm.assemble_elemental_matrices_to_matrix(
            &self.mesh,
            id,
            "u",
            ke.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        self.residual_calls += 1;
        if Some(self.residual_calls) == self.fail_residual_at {
            return Err(self.injected(Error::Numerical("injected residual failure".into())));
        }
        let nn = self.mesh.nb_nodes();
        let mut r = vec![0.0; nn];
        let c = self.mesh.connectivity(SEG, GhostType::NotGhost)?;
        for (e, eps) in self.strains(dm).into_iter().enumerate() {
            let force = eps + eps * eps * eps;
            let t = c.tuple(e);
            r[t[0] as usize] += force; // r = f_ext - f_int
            r[t[1] as usize] -= force;
        }
        if let Some(sync) = &self.mesh.synchronizers().node {
            // A communicating callback: through the enclosing scope of the
            // running collective operation, never an independent
            // one (`sync.slave_reduction` would be refused here).
            if self.independent_scope {
                sync.slave_reduction(&mut NodalSum(&mut r), SynchronizationTag::SmmRes)?;
            } else {
                sync.slave_reduction_in(
                    &mut *dm.phases()?,
                    &mut NodalSum(&mut r),
                    SynchronizationTag::SmmRes,
                )?;
            }
        }
        for (n, rn) in r.iter_mut().enumerate() {
            if self.mesh.nodes().as_slice()[n] == self.x_load
                && self.mesh.node_flag(n as Idx).is_local_or_master()
            {
                *rn += self.load;
            }
        }
        let arr = Array::from_vec(r, 1)?;
        dm.assemble_to_residual("u", arr.view(), 1.0)
    }
    fn corrector(&mut self, dm: &mut DofManager) -> Result<()> {
        if self.fail_corrector {
            return Err(self.injected(Error::Numerical("injected corrector failure".into())));
        }
        let sol = dm.family("u")?.solution.as_slice().to_vec();
        let mut u = dm.dofs_mut("u")?;
        let u = u.as_mut_slice();
        for (ui, s) in u.iter_mut().zip(sol) {
            if self.incremental {
                *ui += s;
            } else {
                *ui = s;
            }
        }
        Ok(())
    }
    fn before_solve_step(&mut self, _: &mut DofManager) -> Result<()> {
        if self.fail_before_solve_step {
            return Err(self.injected(Error::User("injected before_solve_step failure".into())));
        }
        Ok(())
    }
    fn predictor(&mut self, _: &mut DofManager) -> Result<()> {
        if self.fail_predictor {
            return Err(self.injected(Error::User("injected predictor failure".into())));
        }
        Ok(())
    }
    fn after_solve_step(&mut self, _: &mut DofManager, _: bool) -> Result<()> {
        if self.fail_after_solve_step {
            return Err(self.injected(Error::User("injected after_solve_step failure".into())));
        }
        Ok(())
    }
}

/// akantu-side Newton–Raphson over any backend: residual, owned norm
/// (blocked rows masked), Jacobian, `DofManager::solve`, corrector. Each
/// iteration is one collective operation of the DOF manager and every
/// callback outcome is agreed (the protocol the Newton solver follows),
/// so the convergence decision is identical on every rank.
pub fn newton(dm: &mut DofManager, p: &mut Springs) -> Result<usize> {
    for it in 0..30 {
        let converged = dm.collective("newton:iteration", |dm| {
            dm.zero_residual();
            dm.step("assemble_residual", |d| p.assemble_residual(d))?;
            let blocked = dm.blocked_equations();
            let masked: Vec<f64> = dm
                .residual()
                .values()
                .iter()
                .zip(&blocked)
                .map(|(&r, &b)| if b { 0.0 } else { r })
                .collect();
            if dm.owned_norm(&masked)? < 1e-12 {
                return Ok(true);
            }
            dm.step("assemble_matrix", |d| p.assemble_matrix(d, "J"))?;
            dm.finalize_matrix("J")?;
            dm.apply_boundary("J")?;
            dm.residual_mut().as_mut_slice().copy_from_slice(&masked);
            dm.solve("J")?;
            dm.step("corrector", |d| p.corrector(d))?;
            Ok(false)
        })?;
        if converged {
            return Ok(it);
        }
    }
    Err(Error::invalid("no convergence"))
}
