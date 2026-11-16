// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); no upstream
// counterpart (upstream laws write their internals in place).

//! Scratch blocks a constitutive law keeps between steps: a law computes
//! the new values of a `(type, ghost)` block into them and commits them with
//! [`akantu_model::InternalField::commit_overwrite`], which exchanges the
//! allocations, so a step allocates nothing and copies nothing, and the
//! law's internals are unchanged if the computation fails.

use akantu_core::{Array, Real, Result};

/// `K` scratch arrays.
#[derive(Clone, Debug)]
pub(crate) struct ScratchBlocks<const K: usize> {
    blocks: [Array<Real>; K],
}

impl<const K: usize> Default for ScratchBlocks<K> {
    fn default() -> Self {
        Self {
            blocks: std::array::from_fn(|_| Array::new(1)),
        }
    }
}

impl<const K: usize> ScratchBlocks<K> {
    /// The arrays, each sized to `shape[i] = (tuples, components)`; the
    /// values are unspecified (the caller writes every value it commits).
    ///
    /// # Errors
    /// Allocation failure ([`akantu_core::Error::CapacityExceeded`]).
    pub(crate) fn sized(&mut self, shape: [(usize, usize); K]) -> Result<&mut [Array<Real>; K]> {
        for (a, (nq, nbc)) in self.blocks.iter_mut().zip(shape) {
            if a.nb_component() != nbc {
                *a = Array::new(nbc);
            }
            a.try_resize(nq, 0.0)?;
        }
        Ok(&mut self.blocks)
    }

    /// Array `i` (as left by the last commit or [`Self::sized`]).
    pub(crate) fn get_mut(&mut self, i: usize) -> &mut Array<Real> {
        &mut self.blocks[i]
    }
}
