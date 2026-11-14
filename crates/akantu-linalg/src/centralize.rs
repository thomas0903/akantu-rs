// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/solver_vector_distributed.cc
// (getGlobalVector: gather of the master values on rank 0; setGlobalVector:
// scatter from rank 0), src/synchronizer/dof_synchronizer.cc (gather /
// scatter of local-or-master DOFs) as used by the centralised direct
// solvers (SparseSolverMumps with ICNTL(20) = ICNTL(21) = 0, and the serial
// Eigen backend on rank 0).

//! Rank-0 centralisation of an AIJ system (SPM-011).
//!
//! Upstream gathers the **owned** (local-or-master) entries of the
//! right-hand side into a dense global vector on rank 0 and scatters the
//! solution back; values of shared (slave) copies are discarded by the
//! gather and filled by the slave synchronisation after the scatter. The
//! matrix of a centralised solver is the sum of the rank-partial AIJ
//! matrices (MUMPS sums duplicate distributed entries itself; for the
//! centralised Eigen/faer path the port sums them on rank 0 **in rank
//! order, then storage order**, a deterministic, documented choice).
//!
//! Every exchange runs as a phase of the caller's collective scope
//! ([`Phases`], `docs/ARCHITECTURE.md`): local validation is agreed
//! before any message, so an invalid input on one rank is reported on every
//! rank and nobody waits for a message that is never sent.
//!
//! **Capacity preflight (ARCHITECTURE).** Before any
//! payload is copied, gathered or allocated, every rank checks the backend's
//! [`BackendCapabilities`] (matrix *and* right-hand side centralised on rank
//! 0, every global equation representable, the rank's entries within
//! `max_local_nnz`) and the outcome is agreed; the per-rank counts are then
//! all-gathered and the root aggregate (`max_centralized_size`, total
//! entries with checked sums) is checked on identical data on every rank
//! and agreed again. Only then does rank 0 allocate its receive buffers,
//! sized from the agreed counts. Memory on rank 0 is
//! `O(global system size + global nnz)`, bounded by the capabilities.

use akantu_comm::communicator::Tag;
use akantu_comm::phase::{Phases, Severity};
use akantu_core::enums::MatrixType;
use akantu_core::{Error, GlobalIdx, Real, Result};

use crate::direct::{BackendCapabilities, centralized_capabilities};
use crate::layout::EquationLayout;
use crate::sparse::SparseMatrixAij;

const TAG_SIZES: Tag = Tag(0x4C00);
const TAG_OWNED_G: Tag = Tag(0x4C01);
const TAG_OWNED_V: Tag = Tag(0x4C02);
const TAG_IRN: Tag = Tag(0x4C03);
const TAG_JCN: Tag = Tag(0x4C04);
const TAG_A: Tag = Tag(0x4C05);
const TAG_SOLUTION: Tag = Tag(0x4C07);

/// A system gathered on rank 0 (`matrix`, `rhs` are `Some` on rank 0
/// only), with what is needed to scatter the solution back.
#[derive(Debug)]
pub struct CentralizedSystem {
    /// Global matrix (rank 0).
    pub matrix: Option<SparseMatrixAij>,
    /// Global right-hand side (rank 0).
    pub rhs: Option<Vec<Real>>,
    /// Owned global equations of each rank, in each rank's local order
    /// (rank 0).
    owned_per_rank: Vec<Vec<GlobalIdx>>,
    /// This rank's owned local equations and their global numbers.
    owned_local: OwnedPairs,
    /// `(profile release, value release, symmetric)` of every rank's
    /// partial matrix (rank 0): the change detector of a centralised
    /// solver (re-analyse when a profile release or the symmetry changed,
    /// refactorise when a value release changed).
    pub releases: Vec<(u64, u64, bool)>,
}

fn u64_of(n: usize) -> u64 {
    n as u64
}

fn usize_of(n: u64) -> Result<usize> {
    usize::try_from(n).map_err(|_| Error::invalid("received count exceeds the address space"))
}

/// Owned `(local equation, global equation)` pairs.
type OwnedPairs = Vec<(usize, GlobalIdx)>;

/// Owned `(local, global)` pairs of `layout` with the matching values of
/// `local` (validated against the system size).
fn owned_pairs(layout: &EquationLayout, local: &[Real]) -> Result<(OwnedPairs, Vec<Real>)> {
    if local.len() != layout.nb_local() {
        return Err(Error::DimensionMismatch {
            context: "centralize: local vector",
            expected: layout.nb_local(),
            got: local.len(),
        });
    }
    let n = layout.system_size();
    let mut pairs = Vec::with_capacity(layout.nb_owned());
    let mut vals = Vec::with_capacity(layout.nb_owned());
    for (l, &v) in local.iter().enumerate() {
        if layout.is_owned(l) {
            let g = layout
                .global(l)
                .filter(|&g| (0..n).contains(&g))
                .ok_or_else(|| {
                    Error::invalid(format!(
                        "owned local equation {l} has no valid global number"
                    ))
                })?;
            pairs.push((l, g));
            vals.push(v);
        }
    }
    Ok((pairs, vals))
}

/// Local part of the capacity preflight: distribution and per-rank limits
/// (no copy, no allocation).
fn local_preflight(
    layout: &EquationLayout,
    a: &SparseMatrixAij,
    caps: &BackendCapabilities,
) -> Result<u64> {
    if !caps.is_fully_centralized() {
        return Err(Error::Unsupported(format!(
            "centralised solve: the backend takes its matrix as {:?} and its right-hand side \
             as {:?}; the rank-0 gather needs both centralised on rank 0",
            caps.matrix, caps.rhs
        )));
    }
    let nnz = u64::try_from(a.nnz()).unwrap_or(u64::MAX);
    caps.check_local(layout.system_size(), nnz)?;
    if a.size() > layout.system_size() {
        return Err(Error::invalid(format!(
            "matrix '{}' order {} exceeds the system size {}",
            a.id(),
            a.size(),
            layout.system_size()
        )));
    }
    Ok(nnz)
}

/// Gather the owned rows of `rhs` and the rank-partial matrix `a` on rank
/// 0 (collective over the scope's communicator) for a backend with
/// capabilities `caps`.
///
/// `a` is the local matrix lookup: an error (e.g. a missing matrix) is
/// agreed like any other local validation failure. The capacity preflight
/// (module documentation) runs and is agreed before anything is copied or
/// gathered; a limit exceeded on any rank fails every rank. The matrix is
/// summed on rank 0 in rank order, then each rank's storage order (D-039).
pub fn gather_system(
    phases: &mut Phases,
    name: &str,
    layout: &EquationLayout,
    a: Result<&SparseMatrixAij>,
    rhs: &[Real],
    caps: &BackendCapabilities,
) -> Result<CentralizedSystem> {
    let (rank, size) = phases.rank_size();
    let system_size = layout.system_size();
    // Phase 1: local preflight (distribution, per-rank limits, vector
    // shapes) agreed, then the per-rank counts all-gathered.
    let (local, counts) = match a.and_then(|a| {
        let nnz = local_preflight(layout, a, caps)?;
        let (pairs, vals) = owned_pairs(layout, rhs)?;
        Ok((a, nnz, pairs, vals))
    }) {
        Ok(l) => {
            let c = [u64_of(l.2.len()), l.1];
            (Some(l), Ok(c))
        }
        Err(e) => (None, Err(e)),
    };
    let all = phases.run_collective(&format!("{name}:preflight"), counts, |c, counts| {
        c.all_gather(&counts)
    })?;
    let Some((a, _, pairs, vals)) = local else {
        return Err(Error::invalid(
            "centralize: agreed preflight without local data",
        ));
    };
    // Phase 2: root aggregate on identical data (checked sums), agreed.
    let aggregate = (|| -> Result<()> {
        if all.len() != 2 * size {
            return Err(Error::invalid("centralize: malformed preflight counts"));
        }
        let overflow = || Error::CapacityExceeded {
            what: "stored entries of the matrix centralised on rank 0",
            requested: u128::from(u64::MAX) + 1,
            limit: u128::from(u64::MAX),
        };
        let (mut owned, mut nnz) = (0u64, 0u64);
        for r in 0..size {
            owned = owned.checked_add(all[2 * r]).ok_or_else(overflow)?;
            nnz = nnz.checked_add(all[2 * r + 1]).ok_or_else(overflow)?;
            usize_of(all[2 * r])?;
            usize_of(all[2 * r + 1])?;
        }
        caps.check_centralized(system_size, nnz)?;
        usize_of(nnz)?;
        usize::try_from(system_size)
            .map_err(|_| Error::invalid("system size exceeds the address space"))?;
        if owned > u64::try_from(system_size).unwrap_or(0) {
            return Err(Error::invalid(format!(
                "the ranks own {owned} equations, the system has {system_size}"
            )));
        }
        Ok(())
    })();
    phases.agree(
        &format!("{name}:capacity"),
        aggregate,
        Severity::Recoverable,
    )?;
    // Phase 3: the gather (rank 0 sizes its buffers from the agreed counts).
    let (irn, jcn, av) = a.coo();
    let ty = u64::from(a.matrix_type() == MatrixType::Symmetric);
    let rel = (a.profile_release(), a.value_release());
    let id = a.id().to_owned();
    let gathered = phases.run(&format!("{name}:gather"), Ok(()), |comm, ()| {
        let owned_g: Vec<GlobalIdx> = pairs.iter().map(|p| p.1).collect();
        if rank != 0 {
            comm.send(&[rel.0, rel.1, ty], 0, TAG_SIZES)?;
            comm.send(&owned_g, 0, TAG_OWNED_G)?;
            comm.send(&vals, 0, TAG_OWNED_V)?;
            comm.send(irn, 0, TAG_IRN)?;
            comm.send(jcn, 0, TAG_JCN)?;
            comm.send(av, 0, TAG_A)?;
            return Ok(Ok(None));
        }
        // rank 0: receive in rank order (transport errors are fatal); the
        // counts were agreed and checked against the capabilities.
        let mut received = Vec::with_capacity(size.saturating_sub(1));
        let mut releases = vec![(rel.0, rel.1, ty == 1)];
        for r in 1..size {
            let src = r as i32;
            let (no, nz) = (all[2 * r] as usize, all[2 * r + 1] as usize);
            let mut head = [0u64; 3];
            comm.receive(&mut head, src, TAG_SIZES)?;
            let mut g = vec![0i64; no];
            let mut v = vec![0.0; no];
            let mut i = vec![0i64; nz];
            let mut j = vec![0i64; nz];
            let mut x = vec![0.0; nz];
            comm.receive(&mut g, src, TAG_OWNED_G)?;
            comm.receive(&mut v, src, TAG_OWNED_V)?;
            comm.receive(&mut i, src, TAG_IRN)?;
            comm.receive(&mut j, src, TAG_JCN)?;
            comm.receive(&mut x, src, TAG_A)?;
            releases.push((head[0], head[1], head[2] == 1));
            received.push((g, v, i, j, x, head[2]));
        }
        // post-processing on rank 0 (agreed afterwards)
        let assemble = || -> Result<(SparseMatrixAij, Vec<Real>, Vec<Vec<GlobalIdx>>)> {
            if received.iter().any(|p| p.5 != ty) {
                return Err(Error::invalid(
                    "ranks disagree on the effective symmetry of the matrix (agree the promotion first)",
                ));
            }
            let mtype = if ty == 1 {
                MatrixType::Symmetric
            } else {
                MatrixType::Unsymmetric
            };
            let n = usize::try_from(system_size)
                .map_err(|_| Error::invalid("system size exceeds the address space"))?;
            let mut m = SparseMatrixAij::new(id.clone(), system_size, mtype);
            let mut b = vec![0.0; n];
            let mut seen = vec![false; n];
            let mut owned_per_rank = Vec::with_capacity(size);
            let parts = std::iter::once((
                owned_g.as_slice(),
                vals.as_slice(),
                irn,
                jcn,
                av,
            ))
            .chain(received.iter().map(|(g, v, i, j, x, _)| {
                (g.as_slice(), v.as_slice(), i.as_slice(), j.as_slice(), x.as_slice())
            }));
            for (g, v, i, j, x) in parts {
                for (&gi, &vi) in g.iter().zip(v) {
                    let idx = usize::try_from(gi)
                        .ok()
                        .filter(|&k| k < n)
                        .ok_or_else(|| Error::invalid(format!("owned global equation {gi} outside the system")))?;
                    if seen[idx] {
                        return Err(Error::invalid(format!("global equation {gi} is owned by two ranks")));
                    }
                    seen[idx] = true;
                    b[idx] = vi;
                }
                for ((&ii, &jj), &xx) in i.iter().zip(j).zip(x) {
                    if ii >= system_size || jj >= system_size {
                        return Err(Error::invalid(format!(
                            "matrix entry ({ii}, {jj}) outside the system of order {system_size}"
                        )));
                    }
                    m.add(ii, jj, xx)?;
                }
                owned_per_rank.push(g.to_vec());
            }
            Ok((m, b, owned_per_rank))
        };
        Ok(assemble().map(|(m, b, o)| Some((m, b, o, releases))))
    })?;
    Ok(match gathered {
        Some((m, b, owned_per_rank, releases)) => CentralizedSystem {
            matrix: Some(m),
            rhs: Some(b),
            owned_per_rank,
            owned_local: pairs,
            releases,
        },
        None => CentralizedSystem {
            matrix: None,
            rhs: None,
            owned_per_rank: Vec::new(),
            owned_local: pairs,
            releases: Vec::new(),
        },
    })
}

/// Scatter the global solution `x` (rank 0, length = system size) back to
/// the owned rows of `solution` on every rank (upstream `setGlobalVector`
/// before the slave synchronisation, which stays the DOF manager's job).
/// `x_root` is the outcome of the rank-0 solve: its error is agreed before
/// any message.
pub fn scatter_solution(
    phases: &mut Phases,
    name: &str,
    sys: &CentralizedSystem,
    x_root: Result<Option<Vec<Real>>>,
    solution: &mut [Real],
) -> Result<()> {
    let (rank, size) = phases.rank_size();
    let prepared = x_root.and_then(|x| {
        if rank == 0 {
            let x = x.ok_or_else(|| Error::invalid("rank 0 has no solution to scatter"))?;
            for owned in &sys.owned_per_rank {
                for &g in owned {
                    if usize::try_from(g).map_or(true, |k| k >= x.len()) {
                        return Err(Error::invalid("solution shorter than the system"));
                    }
                }
            }
            Ok(Some(x))
        } else {
            Ok(None)
        }
    });
    let prepared = prepared.and_then(|x| {
        for &(l, _) in &sys.owned_local {
            if l >= solution.len() {
                return Err(Error::DimensionMismatch {
                    context: "centralize: solution vector",
                    expected: l + 1,
                    got: solution.len(),
                });
            }
        }
        Ok(x)
    });
    let mine = phases.run(&format!("{name}:scatter"), prepared, |comm, x| {
        if rank == 0 {
            let x = x.unwrap_or_default();
            for r in 1..size {
                let vals: Vec<Real> = sys.owned_per_rank[r]
                    .iter()
                    .map(|&g| x[g as usize])
                    .collect();
                comm.send(&vals, r as i32, TAG_SOLUTION)?;
            }
            Ok(Ok(sys.owned_per_rank[0]
                .iter()
                .map(|&g| x[g as usize])
                .collect::<Vec<_>>()))
        } else {
            let mut vals = vec![0.0; sys.owned_local.len()];
            comm.receive(&mut vals, 0, TAG_SOLUTION)?;
            Ok(Ok(vals))
        }
    })?;
    for (&(l, _), v) in sys.owned_local.iter().zip(mine) {
        solution[l] = v;
    }
    Ok(())
}

/// The right-hand-side part of [`gather_system`] for a backend that takes
/// its matrix distributed (MUMPS `ICNTL(18) = 3` with a centralised dense
/// RHS, `ICNTL(20) = ICNTL(21) = 0`, `sparse_solver_mumps.cc:140-182,272-280`):
/// the owned rows of `local` in a dense global vector on rank 0
/// (`CentralizedSystem::rhs`, `None` elsewhere, no matrix), with the map
/// [`scatter_solution`] needs. The vector capacity follows `caps`
/// (`max_global_index`, `max_centralized_size`), checked and agreed before
/// anything is gathered.
pub fn gather_rhs(
    phases: &mut Phases,
    name: &str,
    layout: &EquationLayout,
    local: &[Real],
    caps: &BackendCapabilities,
) -> Result<CentralizedSystem> {
    let empty = SparseMatrixAij::new("", 0, MatrixType::Unsymmetric);
    let vector_caps = BackendCapabilities {
        matrix: crate::direct::MatrixDistribution::CentralizedOnRoot,
        rhs: crate::direct::RhsDistribution::CentralizedOnRoot,
        max_global_index: caps.max_global_index,
        max_local_nnz: u64::MAX,
        max_centralized_size: caps.max_centralized_size,
    };
    gather_system(phases, name, layout, Ok(&empty), local, &vector_caps)
}

/// Upstream `SolverVectorDistributed::getGlobalVector` for one vector:
/// the owned values of every rank in a dense global vector on rank 0
/// (`None` elsewhere).
pub fn gather_owned_to_root(
    phases: &mut Phases,
    name: &str,
    layout: &EquationLayout,
    local: &[Real],
) -> Result<Option<Vec<Real>>> {
    let empty = SparseMatrixAij::new("", 0, MatrixType::Unsymmetric);
    let sys = gather_system(
        phases,
        name,
        layout,
        Ok(&empty),
        local,
        &centralized_capabilities(),
    )?;
    Ok(sys.rhs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::EquationOwnership::{Owned, PureGhost, Shared};
    use akantu_comm::communicator::run_local;
    use std::time::Duration;

    /// Two ranks sharing global equation 1 (owned by rank 0); rank 1 also
    /// has a pure ghost. The rank-partial matrices sum to the global
    /// operator on rank 0; the scatter fills the owned rows.
    #[test]
    fn two_rank_gather_and_scatter() {
        let out = run_local(2, Duration::from_secs(20), |comm| {
            let rank = comm.rank();
            let mut layout = EquationLayout::new();
            if rank == 0 {
                layout
                    .append_block(&[Owned, Owned], 0, 3, |_| None)
                    .unwrap();
            } else {
                layout
                    .append_block(&[Shared, Owned, PureGhost], 2, 3, |i| (i == 0).then_some(1))
                    .unwrap();
            }
            let mut a = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
            if rank == 0 {
                a.add(0, 0, 2.0).unwrap();
                a.add(0, 1, -1.0).unwrap();
                a.add(1, 1, 1.0).unwrap();
            } else {
                a.add(1, 1, 1.0).unwrap();
                a.add(2, 1, -1.0).unwrap();
                a.add(2, 2, 2.0).unwrap();
            }
            let rhs: Vec<f64> = if rank == 0 {
                vec![1.0, 0.0]
            } else {
                vec![99.0, 1.0, 7.0]
            };
            let mut ph = Phases::new(&comm, "solve");
            let sys = gather_system(
                &mut ph,
                "t",
                &layout,
                Ok(&a),
                &rhs,
                &centralized_capabilities(),
            )
            .unwrap();
            let x = if rank == 0 {
                assert_eq!(sys.releases.len(), 2);
                let m = sys.matrix.as_ref().unwrap();
                assert_eq!((m.get(1, 1), m.get(1, 2), m.nnz()), (2.0, -1.0, 5));
                assert_eq!(sys.rhs.as_deref(), Some(&[1.0, 0.0, 1.0][..]));
                // tridiag(-1, 2, -1) x = (1, 0, 1)  =>  x = (1, 1, 1)
                Ok(Some(vec![1.0, 1.0, 1.0]))
            } else {
                Ok(None)
            };
            let mut sol = vec![-5.0; layout.nb_local()];
            scatter_solution(&mut ph, "t", &sys, x, &mut sol).unwrap();
            sol
        });
        assert_eq!(out[0], [1.0, 1.0]);
        // shared row untouched (synchronised by the manager), owned row set,
        // pure ghost untouched
        assert_eq!(out[1], [-5.0, 1.0, -5.0]);
    }

    /// A local validation failure on one rank is agreed: every rank gets an
    /// error, nobody blocks.
    #[test]
    fn invalid_input_on_one_rank_fails_everywhere() {
        let out = run_local(2, Duration::from_secs(20), |comm| {
            let layout = EquationLayout::serial(2).unwrap();
            let a = SparseMatrixAij::new("K", 2, MatrixType::Unsymmetric);
            let rhs = if comm.rank() == 1 {
                vec![0.0; 1]
            } else {
                vec![0.0; 2]
            };
            let mut ph = Phases::new(&comm, "solve");
            gather_system(
                &mut ph,
                "t",
                &layout,
                Ok(&a),
                &rhs,
                &centralized_capabilities(),
            )
            .is_err()
        });
        assert_eq!(out, [true, true]);
        // a failed rank-0 solve is agreed before the scatter
        let out = run_local(2, Duration::from_secs(20), |comm| {
            let mut layout = EquationLayout::new();
            layout
                .append_block(&[Owned], comm.rank() as i64, 2, |_| None)
                .unwrap();
            let a = SparseMatrixAij::new("K", 2, MatrixType::Unsymmetric);
            let mut ph = Phases::new(&comm, "solve");
            let sys = gather_system(
                &mut ph,
                "t",
                &layout,
                Ok(&a),
                &[1.0],
                &centralized_capabilities(),
            )
            .unwrap();
            let x = if comm.rank() == 0 {
                Err(Error::LinearSolver("singular".into()))
            } else {
                Ok(None)
            };
            let mut sol = [0.0];
            scatter_solution(&mut ph, "t", &sys, x, &mut sol).is_err()
        });
        assert_eq!(out, [true, true]);
    }
}
