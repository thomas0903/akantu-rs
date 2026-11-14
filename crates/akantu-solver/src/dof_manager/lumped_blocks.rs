// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Extends the lumped
// matrices of src/model/common/dof_manager/dof_manager.cc /
// dof_manager_default.cc (getNewLumpedMatrix, assembleToLumpedMatrix,
// assembleLumpedMatMulVectToResidual, NonLinearSolverLumped::solveLumped)
// with symmetric 3×3 couplings; no upstream counterpart (D-059,
// exact lumped rotary inertia of 3-D beams).

//! Block-lumped matrices (D-059, ARCHITECTURE).
//!
//! A lumped matrix is one value per local equation (upstream). A lumped
//! matrix may in addition carry **symmetric 3×3 couplings** over three
//! consecutive components of every tuple of one DOF family: block `b`
//! couples the local equations `e_b = (e₀, e₁, e₂)` of the components
//! `first .. first + 3` of tuple `b`, and the matrix restricted to them is
//!
//! ```text
//! ⎡ a[e₀]  o₀₁    o₀₂  ⎤
//! ⎢ o₀₁    a[e₁]  o₁₂  ⎥      a = the lumped vector (diagonal),
//! ⎣ o₀₂    o₁₂    a[e₂]⎦      o = the stored off-diagonal entries.
//! ```
//!
//! The diagonal stays in the lumped vector, so every scalar consumer
//! ([`DofManager::lumped_matrix`], [`DofManager::lumped_matrix_per_dofs`])
//! sees the diagonal and the couplings cannot disagree with it. Storage is
//! contiguous (struct of arrays: 3 equations and 3 off-diagonal values per
//! block), built at assembly; the time step allocates nothing.
//!
//! * product ([`DofManager::assemble_lumped_mat_mul_vect_to_residual`] and
//!   the family variant, used by the integration schemes' `r −= M·ü` /
//!   `r −= M·u̇`): the diagonal loop over every local equation as without
//!   couplings, then per block `r[eᵢ] += Σ_{j≠i} oᵢⱼ·x[eⱼ]` (`j` ascending);
//! * solve ([`DofManager::solve_lumped`]): the diagonal rule
//!   `x = α·(b / a)` over every local equation as without couplings, then
//!   per block with at least two **free** components (blocked flags of the
//!   last [`DofManager::update_global_blocked_dofs`] snapshot) the free
//!   sub-block `M_FF y = b_F` is solved by `LDLᵀ` and `x_F = α·y`; blocked
//!   components keep the diagonal value (the correctors skip blocked DOFs,
//!   as upstream's lumped solve computes them and the correctors skip
//!   them). With one free component the 1×1 free block is the diagonal
//!   entry, so the diagonal rule is already the exact solve. A non-positive
//!   or non-finite pivot is refused with `Error::Numerical` (the free
//!   sub-block of a mass must be positive definite);
//! * [`DofManager::zero_lumped_matrix`] drops the couplings of that matrix
//!   (the next assembly adds them again) and a numbering change drops every
//!   coupling (their equations are stale).
//!
//! A lumped matrix without couplings behaves exactly as before (same
//! operations in the same order: bitwise). Couplings are serial and
//! non-periodic: registering them on a distributed manager or a family
//! with periodic slaves is refused (`Error::Unsupported`); one coupling
//! structure (family and component range) per lumped matrix.

use akantu_core::array::ArrayView;
use akantu_core::{Error, Id, Real, Result};

use super::{DofManager, check_len};

/// Symmetric 3×3 couplings of a block-lumped matrix (module docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LumpedBlockCouplings {
    family: Id,
    first_component: usize,
    /// Local equations of the three coupled DOFs of each block (3 per
    /// block, tuple order of the family).
    equations: Vec<usize>,
    /// Off-diagonal entries `(0,1), (0,2), (1,2)` of each block.
    off_diagonal: Vec<Real>,
    /// Largest local equation (bounds check of the products in `O(1)`).
    max_equation: usize,
}

impl LumpedBlockCouplings {
    /// The DOF family whose tuples carry the blocks.
    #[must_use]
    pub fn family(&self) -> &str {
        &self.family
    }

    /// First of the three coupled components.
    #[must_use]
    pub fn first_component(&self) -> usize {
        self.first_component
    }

    /// Number of blocks (= tuples of the family).
    #[must_use]
    pub fn nb_blocks(&self) -> usize {
        self.equations.len() / 3
    }

    /// Local equations of the coupled DOFs (3 per block).
    #[must_use]
    pub fn equations(&self) -> &[usize] {
        &self.equations
    }

    /// Off-diagonal entries `(0,1), (0,2), (1,2)` (3 per block).
    #[must_use]
    pub fn off_diagonal(&self) -> &[Real] {
        &self.off_diagonal
    }

    /// `r[eᵢ] += Σ_{j≠i} oᵢⱼ·x[eⱼ]` for every block (the couplings' part of
    /// `r += A·x`; the caller added the diagonal part).
    fn add_coupling_product(&self, r: &mut [Real], x: &[Real]) -> Result<()> {
        if self.equations.is_empty() {
            return Ok(());
        }
        if self.max_equation >= r.len() || self.max_equation >= x.len() {
            return Err(Error::invalid(
                "lumped block couplings: equation outside the local system (stale couplings)",
            ));
        }
        for (e, o) in self
            .equations
            .as_chunks::<3>()
            .0
            .iter()
            .zip(self.off_diagonal.as_chunks::<3>().0)
        {
            let (x0, x1, x2) = (x[e[0]], x[e[1]], x[e[2]]);
            r[e[0]] += o[0] * x1;
            r[e[0]] += o[1] * x2;
            r[e[1]] += o[0] * x0;
            r[e[1]] += o[2] * x2;
            r[e[2]] += o[1] * x0;
            r[e[2]] += o[2] * x1;
        }
        Ok(())
    }

    /// Overwrite `x` on the free components of every block with at least
    /// two free components by `α·M_FF⁻¹ b_F` (module docs).
    fn solve_free_blocks(
        &self,
        a: &[Real],
        b: &[Real],
        blocked: &[bool],
        alpha: Real,
        x: &mut [Real],
    ) -> Result<()> {
        if self.equations.is_empty() {
            return Ok(());
        }
        if self.max_equation >= a.len() || self.max_equation >= b.len() {
            return Err(Error::invalid(
                "lumped block couplings: equation outside the local system (stale couplings)",
            ));
        }
        let is_blocked = |e: usize| blocked.get(e).copied().unwrap_or(false);
        for (e, o) in self
            .equations
            .as_chunks::<3>()
            .0
            .iter()
            .zip(self.off_diagonal.as_chunks::<3>().0)
        {
            let mut free = [0usize; 3];
            let mut k = 0;
            for (i, &eq) in e.iter().enumerate() {
                if !is_blocked(eq) {
                    free[k] = i;
                    k += 1;
                }
            }
            if k < 2 {
                continue;
            }
            let mut m = [[0.0; 3]; 3];
            let mut rhs = [0.0; 3];
            for p in 0..k {
                for q in 0..k {
                    m[p][q] = block_entry(a, e, o, free[p], free[q]);
                }
                rhs[p] = b[e[free[p]]];
            }
            let y = ldlt_solve(&m, &rhs, k).ok_or_else(|| {
                Error::Numerical(format!(
                    "lumped block of local equations {e:?}: the free sub-block is not positive \
                     definite"
                ))
            })?;
            for p in 0..k {
                x[e[free[p]]] = alpha * y[p];
            }
        }
        Ok(())
    }
}

/// Entry `(i, j)` of one block: the diagonal from the lumped vector, the
/// off-diagonal from the couplings.
fn block_entry(a: &[Real], e: &[usize], o: &[Real], i: usize, j: usize) -> Real {
    match (i.min(j), i.max(j)) {
        (p, q) if p == q => a[e[p]],
        (0, 1) => o[0],
        (0, 2) => o[1],
        _ => o[2],
    }
}

/// Solve the symmetric `k × k` system `m y = rhs` (`k ≤ 3`, lower triangle
/// read) by `LDLᵀ`; `None` when a pivot is not positive and finite.
fn ldlt_solve(m: &[[Real; 3]; 3], rhs: &[Real; 3], k: usize) -> Option<[Real; 3]> {
    let mut l = [[0.0; 3]; 3];
    let mut d = [0.0; 3];
    for i in 0..k {
        for j in 0..i {
            let mut s = m[i][j];
            for p in 0..j {
                s -= l[i][p] * d[p] * l[j][p];
            }
            l[i][j] = s / d[j];
        }
        let mut s = m[i][i];
        for p in 0..i {
            s -= l[i][p] * d[p] * l[i][p];
        }
        if !(s > 0.0 && s.is_finite()) {
            return None;
        }
        d[i] = s;
    }
    let mut y = [0.0; 3];
    for i in 0..k {
        let mut s = rhs[i];
        for p in 0..i {
            s -= l[i][p] * y[p];
        }
        y[i] = s;
    }
    for i in 0..k {
        y[i] /= d[i];
    }
    for i in (0..k).rev() {
        let mut s = y[i];
        for p in i + 1..k {
            s -= l[p][i] * y[p];
        }
        y[i] = s;
    }
    Some(y)
}

impl DofManager {
    /// Add `scale · off_diagonal` to the symmetric 3×3 couplings of lumped
    /// matrix `lumped_id` over components `first_component ..
    /// first_component + 3` of every tuple of family `dof_id` (D-059,
    /// module docs). `off_diagonal` has one tuple of 3 values `(0,1), (0,2),
    /// (1,2)` per tuple of the family. The diagonal of the blocks is the
    /// lumped vector itself ([`DofManager::assemble_to_lumped_matrix`]).
    ///
    /// # Errors
    /// * a poisoned manager; an unknown lumped matrix or family;
    /// * `first_component + 3` above the family's component count, or an
    ///   `off_diagonal` of another shape (`Error::DimensionMismatch`);
    /// * a distributed manager, a family with periodic slaves or a DOF
    ///   without a local equation (`Error::Unsupported`: the couplings are
    ///   serial and non-periodic);
    /// * couplings of another family or component range already on the
    ///   matrix (`Error::Unsupported`: one structure per lumped matrix).
    pub fn assemble_lumped_block_couplings(
        &mut self,
        dof_id: &str,
        lumped_id: &str,
        first_component: usize,
        off_diagonal: ArrayView<'_, Real>,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        if !self.lumped.contains_key(lumped_id) {
            return Err(Error::not_found(
                "lumped matrix",
                self.lumped_matrix_registry_id(lumped_id),
            ));
        }
        if self.communicator.size() > 1 || self.numbered_distributed == Some(true) {
            return Err(Error::Unsupported(
                "lumped block couplings on a distributed DOF manager (the couplings are serial)"
                    .into(),
            ));
        }
        let f = self.family(dof_id)?;
        let nbc = f.dofs.nb_component();
        let nb_tuples = f.dofs.len();
        if first_component.checked_add(3).is_none_or(|end| end > nbc) {
            return Err(Error::DimensionMismatch {
                context: "lumped block couplings: components of the family",
                expected: nbc,
                got: first_component.saturating_add(3),
            });
        }
        if off_diagonal.nb_component() != 3 {
            return Err(Error::DimensionMismatch {
                context: "lumped block couplings: off-diagonal entries per block",
                expected: 3,
                got: off_diagonal.nb_component(),
            });
        }
        check_len(
            "lumped block couplings: one block per tuple of the family",
            off_diagonal.len(),
            nb_tuples,
        )?;
        if f.periodic_slave.iter().any(|&s| s) {
            return Err(Error::Unsupported(
                "lumped block couplings on a family with periodic slaves".into(),
            ));
        }
        let nb_local = self.layout.nb_local();
        let mut equations = Vec::with_capacity(nb_tuples.saturating_mul(3));
        for t in 0..nb_tuples {
            for i in 0..3 {
                let k = t * nbc + first_component + i;
                let eq = f
                    .local_equation_numbers
                    .get(k)
                    .and_then(|&e| usize::try_from(e).ok())
                    .filter(|&e| e < nb_local)
                    .ok_or_else(|| {
                        Error::Unsupported(format!(
                            "lumped block couplings: DOF {k} of family {dof_id} has no local \
                             equation"
                        ))
                    })?;
                equations.push(eq);
            }
        }
        let max_equation = equations.iter().copied().max().unwrap_or(0);
        let values = off_diagonal.as_slice();
        if let Some(c) = self.lumped_blocks.get_mut(lumped_id) {
            if c.family != dof_id
                || c.first_component != first_component
                || c.equations != equations
            {
                return Err(Error::Unsupported(format!(
                    "lumped matrix {lumped_id} already has block couplings on family {} \
                     components {}..{} (one structure per lumped matrix)",
                    c.family,
                    c.first_component,
                    c.first_component + 3
                )));
            }
            for (o, &v) in c.off_diagonal.iter_mut().zip(values) {
                *o += scale * v;
            }
            return Ok(());
        }
        let off: Vec<Real> = values.iter().map(|&v| scale * v).collect();
        self.lumped_blocks.insert(
            lumped_id.into(),
            LumpedBlockCouplings {
                family: dof_id.into(),
                first_component,
                equations,
                off_diagonal: off,
                max_equation,
            },
        );
        Ok(())
    }

    /// The symmetric 3×3 couplings of lumped matrix `lumped_id`, if any
    /// (module docs).
    #[must_use]
    pub fn lumped_block_couplings(&self, lumped_id: &str) -> Option<&LumpedBlockCouplings> {
        self.lumped_blocks.get(lumped_id)
    }

    /// The couplings' part of `r += A·x` (after the diagonal part).
    pub(super) fn add_lumped_coupling_product(&mut self, a_id: &str, x: &[Real]) -> Result<()> {
        match self.lumped_blocks.get(a_id) {
            Some(c) => c.add_coupling_product(self.residual.values_mut().as_mut_slice(), x),
            None => Ok(()),
        }
    }

    /// The free-block solves of [`DofManager::solve_lumped`] (after the
    /// diagonal rule wrote every local equation of `self.solution`).
    pub(super) fn solve_lumped_free_blocks(&mut self, lumped_id: &str, alpha: Real) -> Result<()> {
        let (Some(c), Some(a)) = (
            self.lumped_blocks.get(lumped_id),
            self.lumped.get(lumped_id),
        ) else {
            return Ok(());
        };
        let residual = self.residual.values();
        let r = c.solve_free_blocks(
            &a.values,
            &residual,
            &self.global_blocked,
            alpha,
            &mut self.solution,
        );
        drop(residual);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ldlt_matches_the_closed_form_inverse() {
        // 2×2: [[4, 1], [1, 3]]⁻¹ [1, 2] = [1/11, 7/11]
        let m = [[4.0, 1.0, 0.0], [1.0, 3.0, 0.0], [0.0; 3]];
        let y = ldlt_solve(&m, &[1.0, 2.0, 0.0], 2).unwrap();
        assert!((y[0] - 1.0 / 11.0).abs() <= 2.0 * f64::EPSILON);
        assert!((y[1] - 7.0 / 11.0).abs() <= 2.0 * f64::EPSILON);
        // indefinite and singular blocks are refused
        let m = [[1.0, 2.0, 0.0], [2.0, 1.0, 0.0], [0.0; 3]];
        assert!(ldlt_solve(&m, &[1.0, 1.0, 0.0], 2).is_none());
        let m = [[0.0; 3]; 3];
        assert!(ldlt_solve(&m, &[1.0; 3], 3).is_none());
    }

    /// A diagonal block solves exactly like the diagonal rule `b / a` (the
    /// values make `b·(1/a) ≠ b/a` for the first and third entries).
    #[test]
    fn diagonal_blocks_solve_exactly_like_the_diagonal_rule() {
        let m = [[3.0, 0.0, 0.0], [0.0, 7.0, 0.0], [0.0, 0.0, 3.0]];
        let b = [7.0, 2.0, 10.0];
        let y = ldlt_solve(&m, &b, 3).unwrap();
        for i in 0..3 {
            assert_eq!(y[i], b[i] / m[i][i]);
        }
    }
}
