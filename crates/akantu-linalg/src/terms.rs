// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/terms_to_assemble.hh,
// python/py_solver.cc (TermsToAssemble bindings).

//! Pre-assembled terms (upstream `TermsToAssemble`, SPM-013), used for
//! constraints, contact and user terms added directly to a matrix.
//!
//! Upstream's `operator()(i, j)` appends a term whose value starts at `0.`
//! and returns it for assignment (`terms(i, j) = v`) or accumulation
//! (`terms(i, j) += v`, which stores `0. + v`). Terms are never merged: the
//! assembly order is the insertion order.

use akantu_core::{Idx, Real};

/// One `(i, j, value)` term in *local DOF* numbering of a DOF family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermToAssemble {
    /// Row DOF.
    pub i: Idx,
    /// Column DOF.
    pub j: Idx,
    /// Value.
    pub value: Real,
}

impl TermToAssemble {
    /// Row DOF (upstream `i()`).
    #[must_use]
    pub fn i(&self) -> Idx {
        self.i
    }
    /// Column DOF (upstream `j()`).
    #[must_use]
    pub fn j(&self) -> Idx {
        self.j
    }
}

/// Terms for a pair of DOF families (upstream `TermsToAssemble(dof_id_m,
/// dof_id_n)`).
#[derive(Clone, Debug, Default)]
pub struct TermsToAssemble {
    /// Row DOF family.
    pub dof_id_m: String,
    /// Column DOF family.
    pub dof_id_n: String,
    /// Terms, in insertion order (assembly order is significant).
    pub terms: Vec<TermToAssemble>,
}

impl TermsToAssemble {
    /// New container.
    #[must_use]
    pub fn new(dof_id_m: impl Into<String>, dof_id_n: impl Into<String>) -> Self {
        Self {
            dof_id_m: dof_id_m.into(),
            dof_id_n: dof_id_n.into(),
            terms: Vec::new(),
        }
    }

    /// Row family (upstream `getDOFIdM`).
    #[must_use]
    pub fn dof_id_m(&self) -> &str {
        &self.dof_id_m
    }

    /// Column family (upstream `getDOFIdN`).
    #[must_use]
    pub fn dof_id_n(&self) -> &str {
        &self.dof_id_n
    }

    /// Append a term with value `0.` and return it (upstream
    /// `operator()(i, j)`).
    pub fn term(&mut self, i: Idx, j: Idx) -> &mut TermToAssemble {
        self.terms.push(TermToAssemble { i, j, value: 0.0 });
        let last = self.terms.len() - 1;
        &mut self.terms[last]
    }

    /// `terms(i, j) += value`: appends a term holding `0. + value` (terms
    /// are not merged; `0. + (-0.)` is `+0.` as upstream).
    pub fn add(&mut self, i: Idx, j: Idx, value: Real) {
        self.term(i, j).value += value;
    }

    /// `terms(i, j) = value` (C++ assignment, Python `terms(i, j, val)`).
    pub fn assign(&mut self, i: Idx, j: Idx, value: Real) {
        self.term(i, j).value = value;
    }

    /// Iterate in insertion order (upstream `begin()`/`end()`).
    pub fn iter(&self) -> std::slice::Iter<'_, TermToAssemble> {
        self.terms.iter()
    }

    /// Number of terms.
    #[must_use]
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// `true` if there is no term.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}

impl<'a> IntoIterator for &'a TermsToAssemble {
    type Item = &'a TermToAssemble;
    type IntoIter = std::slice::Iter<'a, TermToAssemble>;
    fn into_iter(self) -> Self::IntoIter {
        self.terms.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_keep_insertion_order_and_upstream_zero_start() {
        let mut t = TermsToAssemble::new("u", "u");
        t.add(1, 1, -0.0);
        t.assign(0, 1, -0.0);
        t.add(1, 1, 2.0);
        assert_eq!(t.len(), 3);
        assert_eq!(t.terms[0].value.to_bits(), 0, "0. + (-0.) == +0.");
        assert!(t.terms[1].value.is_sign_negative(), "assignment keeps -0.");
        let ij: Vec<_> = t.iter().map(|x| (x.i(), x.j())).collect();
        assert_eq!(ij, [(1, 1), (0, 1), (1, 1)]);
        assert_eq!((t.dof_id_m(), t.dof_id_n()), ("u", "u"));
    }
}
