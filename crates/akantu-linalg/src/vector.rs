// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/solver_vector.hh,
// solver_vector_default.hh, solver_vector_default_tmpl.hh,
// solver_vector_distributed.hh, solver_vector_distributed.cc.

//! Solver vectors (residual, solution, lumped matrices; SPM-009…011).
//!
//! Upstream has an abstract `SolverVector` with an array-backed default
//! (`SolverVectorDefault`, local system size) and a distributed variant
//! (`SolverVectorDistributed`) whose global view is gathered on rank 0. The
//! port has one concrete type over the **local equations** of the rank
//! ([`EquationLayout`] gives their global numbers and ownership) with a
//! `distributed` flag; the gather/scatter of the distributed variant is in
//! [`crate::centralize`] (it communicates through the DOF manager's phase
//! scope) and the PETSc vector is a backend-native object.
//!
//! Release counter (upstream `release_`, starts at 0): `resize`, `set` /
//! `zero` and `operator+` increment it, `copy` copies the source's.

use std::path::{Path, PathBuf};

use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, ReduceOp};
use akantu_core::{Error, GlobalIdx, Rank, Real, Result};

use crate::layout::EquationLayout;
use crate::matrix_market;

/// A vector over the local equations of this rank (upstream
/// `SolverVectorDefault` / `SolverVectorDistributed`).
#[derive(Clone, Debug, Default)]
pub struct SolverVector {
    /// Values of the local equations.
    pub values: Vec<Real>,
    /// Global equation number of each local equation (empty in serial;
    /// informational — the [`EquationLayout`] is authoritative).
    pub local_to_global: Vec<GlobalIdx>,
    id: String,
    release: u64,
    distributed: bool,
}

impl SolverVector {
    /// Zero vector of `n` local equations.
    #[must_use]
    pub fn zeros(n: usize) -> Self {
        Self {
            values: vec![0.0; n],
            ..Self::default()
        }
    }

    /// Empty vector named `id` (upstream constructor; call
    /// [`SolverVector::resize`] to size it). `distributed` selects the
    /// `SolverVectorDistributed` behaviour (gather on rank 0, rank-suffixed
    /// files, collective `isFinite`).
    #[must_use]
    pub fn new(id: impl Into<String>, distributed: bool) -> Self {
        Self {
            id: id.into(),
            distributed,
            ..Self::default()
        }
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Release counter (upstream `release()`).
    #[must_use]
    pub fn release(&self) -> u64 {
        self.release
    }

    fn bump(&mut self) {
        self.release = self.release.wrapping_add(1);
    }

    /// Resize to `local_size` local equations, new entries zero (upstream
    /// `resize()` to the local system size).
    pub fn resize(&mut self, local_size: usize) {
        self.values.resize(local_size, 0.0);
        self.bump();
    }

    /// Set every entry to `val` (upstream `set`).
    pub fn set(&mut self, val: Real) {
        self.values.fill(val);
        self.bump();
    }

    /// Set every entry to zero (upstream `zero`).
    pub fn zero(&mut self) {
        self.set(0.0);
    }

    /// Number of local equations (upstream `localSize`).
    #[must_use]
    pub fn local_size(&self) -> usize {
        self.values.len()
    }

    /// Global system size (upstream `size()`).
    #[must_use]
    pub fn size(layout: &EquationLayout) -> GlobalIdx {
        layout.system_size()
    }

    /// `self += y` entry by entry (upstream `operator+`, which modifies and
    /// returns `*this`); lengths must match.
    pub fn add_assign(&mut self, y: &SolverVector) -> Result<()> {
        if y.values.len() != self.values.len() {
            return Err(Error::DimensionMismatch {
                context: "SolverVector::operator+",
                expected: self.values.len(),
                got: y.values.len(),
            });
        }
        for (a, b) in self.values.iter_mut().zip(&y.values) {
            *a += *b;
        }
        self.bump();
        Ok(())
    }

    /// Copy the values and the release of `y` (upstream `copy`).
    pub fn copy_from(&mut self, y: &SolverVector) {
        self.values.clone_from(&y.values);
        self.release = y.release;
    }

    /// `true` if every local entry is finite (upstream default `isFinite`).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.values.iter().all(|v| v.is_finite())
    }

    /// Upstream `SolverVectorDistributed::isFinite`: local test followed by
    /// a logical-and all-reduce (collective) — as one agreed step of the
    /// caller's collective scope `phases` (`docs/ARCHITECTURE.md`;
    /// the form to use inside a DOF-manager or model operation). A serial
    /// (non-distributed) vector does not communicate.
    pub fn is_finite_in(&self, phases: &mut Phases, name: &str) -> Result<bool> {
        let local = self.is_finite();
        if !self.distributed || phases.rank_size().1 == 1 {
            return Ok(local);
        }
        phases.run_collective(name, Ok(u8::from(local)), |c, flag| {
            let mut flag = [flag];
            c.all_reduce(&mut flag, ReduceOp::LogicalAnd)?;
            Ok(flag[0] != 0)
        })
    }

    /// Standalone [`SolverVector::is_finite_in`]: runs in its own settled
    /// collective scope on `comm` (refused — locally, without communicating
    /// — while another scope is live on `comm`; use
    /// [`SolverVector::is_finite_in`] there).
    pub fn is_finite_collective(&self, comm: &Communicator) -> Result<bool> {
        if !self.distributed || comm.is_serial() {
            return Ok(self.is_finite());
        }
        let mut ph = Phases::new(comm, "SolverVector::isFinite");
        let r = self.is_finite_in(&mut ph, "is_finite");
        ph.settle(r)
    }

    /// `true` for the distributed variant (upstream `isDistributed`).
    #[must_use]
    pub fn is_distributed(&self) -> bool {
        self.distributed
    }

    /// Select the distributed variant (upstream: the DOF manager allocates
    /// a `SolverVectorDistributed` or a `SolverVectorDefault`; the port's
    /// DOF manager sets the kind of its lumped matrices when it learns the
    /// mesh's distribution). Values and release are
    /// unchanged. The flag selects the collective [`SolverVector::is_finite_in`]
    /// and the rank-suffixed file names of [`SolverVector::save_vector`]:
    /// set it identically on every rank of the communicator.
    pub fn set_distributed(&mut self, distributed: bool) {
        self.distributed = distributed;
    }

    /// Euclidean norm over the equations flagged in `owned` (local or
    /// master equations): squares summed in local order from `0.0`, then a
    /// sum all-reduce over ranks and a square root — the pattern of
    /// upstream's residual norm (`non_linear_solver_newton_raphson.cc:
    /// 243-253`).
    ///
    /// Collective step of the caller's scope `phases`:
    /// the local validation (mask length) is **agreed** before the
    /// all-reduce, so an invalid input on one rank fails the step on every
    /// rank promptly (that rank keeps its own error, the others report a
    /// failure on another rank) and nobody waits in an unmatched reduction.
    pub fn norm_in(&self, phases: &mut Phases, name: &str, owned: &[bool]) -> Result<Real> {
        let prepared = if owned.len() == self.values.len() {
            let mut s = 0.0;
            for (&v, &o) in self.values.iter().zip(owned) {
                if o {
                    s += v * v;
                }
            }
            Ok(s)
        } else {
            Err(Error::DimensionMismatch {
                context: "SolverVector::norm",
                expected: self.values.len(),
                got: owned.len(),
            })
        };
        // the reduction is issued on every communicator, also of one rank
        // (an identity there): a checked reduction must issue its
        // collective on every rank (`Phases::run_collective`)
        phases.run_collective(name, prepared, |c, s| {
            let mut s = [s];
            c.all_reduce(&mut s, ReduceOp::Sum)?;
            Ok(s[0].sqrt())
        })
    }

    /// Standalone [`SolverVector::norm_in`] on `comm`: one settled
    /// collective scope of its own (agreed validation, then the reduction;
    /// a fatal or broken scope is returned as `Error::Fatal`). On a serial
    /// communicator it computes locally without a scope. Otherwise refused
    /// locally, without communicating, while another scope is live on
    /// `comm` — inside a DOF-manager operation use
    /// [`SolverVector::norm_in`] with the operation's scope.
    pub fn norm(&self, owned: &[bool], comm: &Communicator) -> Result<Real> {
        // Like `is_finite_collective`, a serial
        // communicator does not communicate and needs no scope (so it is
        // not refused while another scope is live): validate, then compute
        // locally with the same arithmetic as `norm_in`.
        if comm.is_serial() {
            if owned.len() != self.values.len() {
                return Err(Error::DimensionMismatch {
                    context: "SolverVector::norm",
                    expected: self.values.len(),
                    got: owned.len(),
                });
            }
            let mut s = 0.0;
            for (&v, &o) in self.values.iter().zip(owned) {
                if o {
                    s += v * v;
                }
            }
            return Ok(s.sqrt());
        }
        let mut ph = Phases::new(comm, "SolverVector::norm");
        let r = self.norm_in(&mut ph, "norm", owned);
        ph.settle(r)
    }

    /// Text of upstream `saveVector` with the default output policy (17
    /// significant digits, round-trip exact; UD-061): header
    /// `coordinate real general`, `N 1 count`, then `g+1 1 v` for every
    /// owned local equation in local order.
    pub fn matrix_market_text(&self, layout: &EquationLayout) -> Result<String> {
        self.matrix_market_text_with(layout, &matrix_market::MatrixMarketOptions::default())
    }

    /// Text of `saveVector` under `options`
    /// ([`matrix_market::MatrixMarketOptions::upstream_precision`]: the
    /// bytes upstream writes, precision 15, lossy).
    pub fn matrix_market_text_with(
        &self,
        layout: &EquationLayout,
        options: &matrix_market::MatrixMarketOptions,
    ) -> Result<String> {
        use std::fmt::Write as _;
        if self.values.len() != layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "SolverVector::saveVector",
                expected: layout.nb_local(),
                got: self.values.len(),
            });
        }
        let count = (0..self.values.len())
            .filter(|&l| layout.is_owned(l))
            .count();
        let mut out = String::with_capacity(64 + 32 * count);
        out.push_str("%%MatrixMarket matrix coordinate real general\n");
        let _ = writeln!(out, "{} 1 {}", layout.system_size(), count);
        for (l, &v) in self.values.iter().enumerate() {
            if layout.is_owned(l) {
                let g = layout
                    .global(l)
                    .ok_or_else(|| Error::invalid("owned equation without a global number"))?;
                let _ = writeln!(out, "{} 1 {}", g + 1, options.format(v));
            }
        }
        Ok(out)
    }

    /// Upstream `saveVector(filename)` on rank `rank` of `nb_proc` (default
    /// output policy): the distributed variant suffixes `_rank-<rank>` with
    /// several processes, then `.mtx` is added when there is no extension.
    /// Returns the path.
    pub fn save_vector(
        &self,
        path: &Path,
        layout: &EquationLayout,
        rank: Rank,
        nb_proc: usize,
    ) -> Result<PathBuf> {
        self.save_vector_with(
            path,
            layout,
            rank,
            nb_proc,
            &matrix_market::MatrixMarketOptions::default(),
        )
    }

    /// [`SolverVector::save_vector`] under `options`.
    pub fn save_vector_with(
        &self,
        path: &Path,
        layout: &EquationLayout,
        rank: Rank,
        nb_proc: usize,
        options: &matrix_market::MatrixMarketOptions,
    ) -> Result<PathBuf> {
        let procs = if self.distributed { nb_proc } else { 1 };
        let file = matrix_market::vector_file_name(path, rank, procs);
        matrix_market::write_text(&file, &self.matrix_market_text_with(layout, options)?)?;
        Ok(file)
    }

    /// Serial global view (upstream `SolverVectorDefault::getGlobalVector`,
    /// the identity): the vector in **global** equation order, built from
    /// the owned equations (non-owned ones stay 0). The distributed gather
    /// is [`crate::centralize::gather_owned_to_root`].
    pub fn global_vector(&self, layout: &EquationLayout) -> Result<Vec<Real>> {
        if self.values.len() != layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "SolverVector::getGlobalVector",
                expected: layout.nb_local(),
                got: self.values.len(),
            });
        }
        let n = usize::try_from(layout.system_size())
            .map_err(|_| Error::invalid("system size exceeds the address space"))?;
        let mut g = vec![0.0; n];
        for (l, &v) in self.values.iter().enumerate() {
            if layout.is_owned(l)
                && let Some(gi) = layout.global(l)
            {
                let gi = usize::try_from(gi)
                    .ok()
                    .filter(|&u| u < n)
                    .ok_or_else(|| Error::invalid("owned equation outside the system"))?;
                g[gi] = v;
            }
        }
        Ok(g)
    }

    /// Serial `setGlobalVector(solution)`: every local equation with a
    /// global number takes `solution[global]` (owned and shared alike, as
    /// upstream's scatter followed by the slave synchronisation).
    pub fn set_global_vector(&mut self, layout: &EquationLayout, solution: &[Real]) -> Result<()> {
        if self.values.len() != layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "SolverVector::setGlobalVector",
                expected: layout.nb_local(),
                got: self.values.len(),
            });
        }
        let mut staged = Vec::with_capacity(self.values.len());
        for l in 0..self.values.len() {
            staged.push(match layout.global(l) {
                Some(g) => Some(
                    *usize::try_from(g)
                        .ok()
                        .and_then(|u| solution.get(u))
                        .ok_or_else(|| Error::invalid("solution shorter than the system"))?,
                ),
                None => None,
            });
        }
        for (v, s) in self.values.iter_mut().zip(staged) {
            if let Some(s) = s {
                *v = s;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::EquationOwnership;

    #[test]
    fn releases_follow_upstream() {
        let mut v = SolverVector::new("r", false);
        assert_eq!(v.release(), 0);
        v.resize(3);
        v.set(2.0);
        let mut w = SolverVector::zeros(3);
        w.add_assign(&v).unwrap();
        assert_eq!((v.release(), w.release()), (2, 1));
        w.copy_from(&v);
        assert_eq!((w.release(), w.values.as_slice()), (2, [2.0; 3].as_slice()));
        assert!(w.add_assign(&SolverVector::zeros(2)).is_err());
        assert!(w.is_finite());
        w.values[1] = f64::NAN;
        assert!(!w.is_finite());
        assert!(!w.is_finite_collective(&Communicator::serial()).unwrap());
    }

    #[test]
    fn matrix_market_and_global_views() {
        // Local 0 owned (global 1), local 1 shared (global 0), local 2 pure ghost.
        let mut layout = EquationLayout::new();
        layout
            .append_block(
                &[
                    EquationOwnership::Owned,
                    EquationOwnership::Shared,
                    EquationOwnership::PureGhost,
                ],
                1,
                2,
                |i| (i == 1).then_some(0),
            )
            .unwrap();
        let mut v = SolverVector::zeros(3);
        v.values = vec![0.5, 7.0, 9.0];
        assert_eq!(
            v.matrix_market_text(&layout).unwrap(),
            "%%MatrixMarket matrix coordinate real general\n2 1 1\n2 1 0.5\n"
        );
        assert_eq!(v.global_vector(&layout).unwrap(), [0.0, 0.5]);
        v.set_global_vector(&layout, &[3.0, 4.0]).unwrap();
        assert_eq!(v.values, [4.0, 3.0, 9.0]);
        assert!(v.set_global_vector(&layout, &[1.0]).is_err());
        assert_eq!(v.values, [4.0, 3.0, 9.0]);
        let owned = [true, false, false];
        assert_eq!(v.norm(&owned, &Communicator::serial()).unwrap(), 4.0);
    }
}
