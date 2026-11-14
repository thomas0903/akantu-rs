// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/dof_manager.cc
// (updateGlobalBlockedDofs :920-969, applyBoundary :972-988, registries
// :426-582, saveAssociatedNodes :1014-1034), dof_manager_inline_impl.hh
// (classification predicates :165-206), dof_manager_default.cc
// (getNewMatrix, getNewLumpedMatrix).

//! Blocked-DOF snapshot and release tracking (DOF-005), Dirichlet treatment
//! of a matrix (DOF-006), matrix and lumped-matrix registries (DOF-012),
//! DOF classification predicates (DOF-016) and debug output (DOF-017).

use std::path::{Path, PathBuf};

use akantu_core::enums::MatrixType;
use akantu_core::{Error, GlobalIdx, NodeFlag, Real, Result};
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_linalg::{SolverVector, SparseMatrixAij};
use akantu_mesh::Mesh;

use super::DofManager;

impl DofManager {
    /// Upstream `updateGlobalBlockedDofs` (called at the start of every
    /// non-linear solve): the local equations of blocked DOFs (periodic
    /// slaves skipped), sorted and de-duplicated, become the blocked
    /// snapshot used by [`DofManager::apply_boundary`] and the convergence
    /// norms; the blocked release is incremented when the set changed.
    /// Local (no communication).
    pub fn update_global_blocked_dofs(&mut self) {
        let local = self.layout.nb_local();
        self.previous_blocked_indexes
            .clone_from(&self.blocked_indexes);
        self.blocked_indexes.clear();
        self.previous_blocked_release = self.blocked_release;
        for f in self.families.values() {
            let Some(bl) = &f.blocked else { continue };
            let bl = bl.borrow();
            for (&eq, &is) in f.local_equation_numbers.iter().zip(bl.as_slice()) {
                if is
                    && eq != -1
                    && let Ok(e) = usize::try_from(eq)
                {
                    self.blocked_indexes.push(e);
                }
            }
        }
        self.blocked_indexes.sort_unstable();
        self.blocked_indexes.dedup();
        let are_equal = self.blocked_indexes == self.previous_blocked_indexes;
        if self.blocked_indexes.is_empty() {
            self.global_blocked.clear();
            self.global_blocked.resize(local, false);
        }
        if are_equal {
            return;
        }
        self.blocked_release = self.blocked_release.wrapping_add(1);
        self.global_blocked.clear();
        self.global_blocked.resize(local, false);
        for &e in &self.blocked_indexes {
            if let Some(b) = self.global_blocked.get_mut(e) {
                *b = true;
            }
        }
    }

    /// Upstream `hasBlockedDOFsChanged`: the blocked release differs from
    /// the one recorded at the last `apply_boundary` (or the previous
    /// update).
    #[must_use]
    pub fn has_blocked_dofs_changed(&self) -> bool {
        self.blocked_release != self.previous_blocked_release
    }

    /// Blocked flag per local equation of the last snapshot (upstream
    /// `getGlobalBlockedDOFs`); entries past the snapshot's length (local
    /// equations added since) read as `false`, where upstream reads out of
    /// bounds.
    #[must_use]
    pub fn global_blocked_dofs(&self) -> &[bool] {
        &self.global_blocked
    }

    /// Blocked local equations of the last snapshot (upstream
    /// `getGlobalBlockedDOFsIndexes`).
    #[must_use]
    pub fn global_blocked_dofs_indexes(&self) -> &[usize] {
        &self.blocked_indexes
    }

    /// Blocked release (upstream `global_blocked_dofs_indexes_release`).
    #[must_use]
    pub fn blocked_release(&self) -> u64 {
        self.blocked_release
    }

    /// The snapshot padded to the local system size. A manager whose
    /// snapshot was never taken takes it first (upstream reads an empty
    /// array out of bounds when `applyBoundary` precedes the first
    /// `updateGlobalBlockedDofs`).
    pub(crate) fn blocked_snapshot(&mut self) -> Vec<bool> {
        let n = self.layout.nb_local();
        if self.global_blocked.is_empty() && n > 0 {
            self.update_global_blocked_dofs();
        }
        let mut b = self.global_blocked.clone();
        b.resize(n, false);
        b
    }

    /// Dirichlet treatment of `matrix_id` (upstream `applyBoundary`): if the
    /// matrix's value release equals the one recorded at the last call, the
    /// rows/columns are re-applied only when the blocked set changed;
    /// otherwise they are always applied (entries touching a blocked
    /// equation zeroed, owned diagonals set to 1). The release recorded is
    /// shared by every matrix, as upstream. Backends without releases always
    /// apply.
    pub fn apply_boundary(&mut self, matrix_id: &str) -> Result<()> {
        self.check_poison()?;
        let blocked = self.blocked_snapshot();
        let release = match self.backend.value_release(matrix_id) {
            Ok(r) => Some(r),
            Err(Error::Unsupported(_)) => None,
            Err(e) => return Err(e),
        };
        match release {
            Some(r) if r == self.jacobian_release => {
                if self.has_blocked_dofs_changed() {
                    self.backend
                        .apply_boundary(matrix_id, &self.layout, &blocked, 1.0)?;
                }
                self.previous_blocked_indexes
                    .clone_from(&self.blocked_indexes);
            }
            _ => self
                .backend
                .apply_boundary(matrix_id, &self.layout, &blocked, 1.0)?,
        }
        if let Some(r) = release {
            self.jacobian_release = self.backend.value_release(matrix_id).unwrap_or(r);
        }
        self.previous_blocked_release = self.blocked_release;
        Ok(())
    }

    // ------------------------------------------------------ predicates

    /// Node flag of a local equation (upstream `getDOFFlag`).
    pub fn dof_flag(&self, local: usize) -> Result<NodeFlag> {
        self.dofs_flag.get(local).copied().ok_or(Error::OutOfRange {
            what: "local equation",
            index: local,
            len: self.dofs_flag.len(),
        })
    }

    /// Upstream `isLocalOrMasterDOF`: `(flag & 0xCC) == 0`.
    pub fn is_local_or_master_dof(&self, local: usize) -> Result<bool> {
        Ok(self.dof_flag(local)?.bits() & NodeFlag::LOCAL_MASTER_MASK.bits() == 0)
    }

    /// Upstream `isSlaveDOF`: `(flag & 0x0F) == 0x05`.
    pub fn is_slave_dof(&self, local: usize) -> Result<bool> {
        Ok(self.dof_flag(local)?.bits() & NodeFlag::SHARED_MASK.bits() == NodeFlag::SLAVE.bits())
    }

    /// Upstream `isPureGhostDOF`: `(flag & 0x0F) == 0x09`.
    pub fn is_pure_ghost_dof(&self, local: usize) -> Result<bool> {
        Ok(self.dof_flag(local)?.bits() & NodeFlag::SHARED_MASK.bits()
            == NodeFlag::PURE_GHOST.bits())
    }

    /// Upstream `localToGlobalEquationNumber` (`None` for pure ghosts,
    /// upstream `-1`).
    #[must_use]
    pub fn local_to_global_equation_number(&self, local: usize) -> Option<GlobalIdx> {
        self.layout.global(local)
    }

    /// Upstream `globalToLocalEquationNumber`.
    pub fn global_to_local_equation_number(&self, global: GlobalIdx) -> Result<usize> {
        self.layout.local(global)
    }

    /// Upstream `hasGlobalEquationNumber`.
    #[must_use]
    pub fn has_global_equation_number(&self, global: GlobalIdx) -> bool {
        self.layout.local(global).is_ok()
    }

    // ------------------------------------------------------ registries

    /// Upstream registry name of a matrix (`<id>:mtx:<name>`).
    #[must_use]
    pub fn matrix_registry_id(&self, name: &str) -> String {
        format!("{}:mtx:{name}", self.id)
    }

    /// Upstream registry name of a lumped matrix (`<id>:lumped_mtx:<name>`).
    #[must_use]
    pub fn lumped_matrix_registry_id(&self, name: &str) -> String {
        format!("{}:lumped_mtx:{name}", self.id)
    }

    /// `true` if matrix `name` exists (upstream `hasMatrix`).
    #[must_use]
    pub fn has_matrix(&self, name: &str) -> bool {
        self.backend.has_matrix(name)
    }

    /// New matrix `name` as a copy of `src` (upstream
    /// `getNewMatrix(id, matrix_to_copy_id)`).
    pub fn new_matrix_copy(&mut self, name: &str, src: &str) -> Result<()> {
        self.check_poison()?;
        let ty = self
            .declared
            .get(src)
            .copied()
            .ok_or_else(|| Error::not_found("matrix", self.matrix_registry_id(src)))?;
        self.backend.copy_matrix(name, src)?;
        self.declared.insert(name.into(), ty);
        Ok(())
    }

    /// Mutable COO access (AIJ backend; upstream `getMatrix` used for
    /// direct `add`/`operator*=`).
    pub fn matrix_mut(&mut self, name: &str) -> Result<&mut SparseMatrixAij> {
        self.check_poison()?;
        self.backend.aij_mut(name)
    }

    /// Upstream `getMatrix(name)`'s check: an unknown matrix is an error
    /// naming its registry id (`The matrix <id>:mtx:<name> does not exists
    /// in <id>`).
    pub(crate) fn require_matrix(&self, name: &str) -> Result<()> {
        if self.backend.has_matrix(name) {
            Ok(())
        } else {
            Err(Error::not_found(
                "matrix",
                format!(
                    "{} (does not exists in {})",
                    self.matrix_registry_id(name),
                    self.id
                ),
            ))
        }
    }

    /// Value release of a matrix (upstream `getRelease`).
    pub fn matrix_release(&self, name: &str) -> Result<u64> {
        self.require_matrix(name)?;
        self.backend.value_release(name)
    }

    /// `dst.copyProfile(src)` (used by the integration schemes for `J`).
    pub fn copy_matrix_profile(&mut self, dst: &str, src: &str) -> Result<()> {
        self.check_poison()?;
        self.require_matrix(dst)?;
        self.require_matrix(src)?;
        self.backend.copy_profile(dst, src)
    }

    /// `dst += alpha · src` (upstream `SparseMatrix::add(B, alpha)`).
    pub fn add_matrix(&mut self, dst: &str, src: &str, alpha: Real) -> Result<()> {
        self.check_poison()?;
        self.require_matrix(dst)?;
        self.require_matrix(src)?;
        self.backend.add_matrix(dst, src, alpha)
    }

    /// Zero a matrix keeping its profile (upstream `zeroMatrix`).
    pub fn zero_matrix(&mut self, name: &str) -> Result<()> {
        self.backend_zero(name)
    }

    /// `true` if lumped matrix `name` exists (upstream `hasLumpedMatrix`).
    #[must_use]
    pub fn has_lumped_matrix(&self, name: &str) -> bool {
        self.lumped.contains_key(name)
    }

    /// Lumped matrix (upstream `getLumpedMatrix`), over local equations.
    pub fn lumped_matrix(&self, name: &str) -> Result<&SolverVector> {
        self.lumped
            .get(name)
            .ok_or_else(|| Error::not_found("lumped matrix", self.lumped_matrix_registry_id(name)))
    }

    /// Mutable lumped matrix.
    pub fn lumped_matrix_mut(&mut self, name: &str) -> Result<&mut SolverVector> {
        let id = self.lumped_matrix_registry_id(name);
        self.lumped
            .get_mut(name)
            .ok_or_else(|| Error::not_found("lumped matrix", id))
    }

    /// Zero a lumped matrix (upstream `zeroLumpedMatrix`); its block
    /// couplings, if any, are dropped (D-059: the next assembly adds them
    /// again, see [`super::LumpedBlockCouplings`]).
    pub fn zero_lumped_matrix(&mut self, name: &str) -> Result<()> {
        self.check_poison()?;
        self.lumped_matrix_mut(name)?.zero();
        self.lumped_blocks.remove(name);
        Ok(())
    }

    /// Mutable residual vector (upstream `getResidual`; its values are
    /// exportable storage reached through [`super::ResidualVector::values_mut`]).
    pub fn residual_vector_mut(&mut self) -> &mut super::ResidualVector {
        &mut self.residual
    }

    /// Upstream `saveAssociatedNodes`: a lumped vector `"nodes"` holding the
    /// global id of each nodal DOF's node (0 for non-owned nodes), saved in
    /// Matrix Market format to `path` (upstream `nodes.mtx`; on a
    /// distributed mesh with several processes `nodes_rank-<r>.mtx` per
    /// rank, the lumped vector being of the manager's kind,
    /// [`DofManager::vectors_distributed`]).
    pub fn save_associated_nodes(&mut self, mesh: &Mesh, path: &Path) -> Result<()> {
        self.save_associated_nodes_with(path, &MatrixMarketOptions::default(), |f| {
            #[allow(clippy::cast_precision_loss)] // upstream `Real(gid)`
            f.associated_nodes
                .iter()
                .map(|&n| {
                    if mesh.node_flag(n).is_local_or_master() {
                        mesh.node_global_id(n) as Real
                    } else {
                        0.0
                    }
                })
                .collect()
        })
        .map(|_| ())
    }

    /// [`DofManager::save_associated_nodes`] without a mesh: the node
    /// global ids recorded when the DOFs were numbered
    /// ([`super::DofData::associated_node_gids`]); used by the Newton
    /// solver's `dblTrace` dump (DOF-017), which has no mesh. Values written
    /// under `options`; returns the path written (`_rank-<r>` suffix on a
    /// distributed mesh with several processes, as upstream's distributed
    /// `saveVector`).
    ///
    /// # Errors
    /// Assembly or I/O errors.
    pub fn save_associated_nodes_numbered(
        &mut self,
        path: &Path,
        options: &MatrixMarketOptions,
    ) -> Result<PathBuf> {
        self.save_associated_nodes_with(path, options, |f| f.associated_node_gids.clone())
    }

    fn save_associated_nodes_with(
        &mut self,
        path: &Path,
        options: &MatrixMarketOptions,
        gids: impl Fn(&super::DofData) -> Vec<Real>,
    ) -> Result<PathBuf> {
        if !self.has_lumped_matrix("nodes") {
            self.new_lumped_matrix("nodes")?;
        }
        let ids: Vec<akantu_core::Id> = self.families.keys().cloned().collect();
        for id in ids {
            let f = &self.families[&id];
            if f.support != akantu_core::enums::DofSupportType::Nodal {
                continue;
            }
            let g = gids(f);
            let mut target = std::mem::take(&mut self.lumped_matrix_mut("nodes")?.values);
            let r = Self::assemble_to_global(
                &self.families[&id],
                self.mesh_periodic,
                &g,
                &mut target,
                1.0,
            );
            self.lumped_matrix_mut("nodes")?.values = target;
            r?;
        }
        let layout = self.layout.clone();
        let (rank, size) = (self.communicator.rank(), self.communicator.size());
        self.lumped_matrix("nodes")?
            .save_vector_with(path, &layout, rank, size, options)
    }

    /// Declared type of a matrix (the type it was created with).
    #[must_use]
    pub fn declared_matrix_type(&self, name: &str) -> Option<MatrixType> {
        self.declared.get(name).copied()
    }
}
