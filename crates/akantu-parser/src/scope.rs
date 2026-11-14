// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parser.hh
// (ParserSection::getParameter / hasParameter with ParserParameterSearchCxt,
// the parent_section back-pointers).
// Copyright (©) 2013-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Variable scopes: a section plus the chain of its ancestors.
//!
//! Upstream sections and parameters keep raw back-pointers to their parent
//! section; algebraic expressions look variables up in the current section,
//! then in the parents (`_ppsc_current_and_parent_scope`), and evaluate the
//! found parameter **in the section that holds it**. In Rust every
//! [`ParserSection`] carries an immutable, reference-counted snapshot of its
//! ancestors' parameters ([`ScopeNode`]), rebuilt by
//! [`ParserSection::relink`] (done automatically by the parser and by
//! [`ParserSection::add_sub_section`]).

use std::sync::Arc;

use crate::section::{ParserParameter, ParserSection};

/// Search context of a parameter lookup (upstream
/// `ParserParameterSearchCxt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SearchScope {
    /// `_ppsc_current_scope`: only the section itself (upstream default).
    #[default]
    Current,
    /// `_ppsc_parent_scope`: only the ancestors. Upstream dereferences an
    /// uninitialised iterator in this mode (undefined behaviour, UD
    /// candidate); the port implements the evident intent: search the
    /// parent, then its ancestors.
    Parent,
    /// `_ppsc_current_and_parent_scope`: the section, then its ancestors.
    CurrentAndParent,
}

/// Immutable snapshot of an ancestor section's parameters.
#[derive(Clone, PartialEq)]
pub(crate) struct ScopeNode {
    pub(crate) parameters: Vec<ParserParameter>,
    pub(crate) parent: Option<Arc<ScopeNode>>,
}

impl Drop for ScopeNode {
    /// Releases the ancestor chain iteratively (a deep chain would otherwise
    /// be dropped by native recursion).
    fn drop(&mut self) {
        let mut next = self.parent.take();
        while let Some(arc) = next {
            next = Arc::into_inner(arc).and_then(|mut node| node.parent.take());
        }
    }
}

impl std::fmt::Debug for ScopeNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopeNode")
            .field("nb_parameters", &self.parameters.len())
            .field("has_parent", &self.parent.is_some())
            .finish()
    }
}

/// A scope in which variables are resolved.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ScopeRef<'a> {
    /// A live section (its own parameters + its parent snapshot chain).
    Section(&'a ParserSection),
    /// An ancestor snapshot.
    Node(&'a ScopeNode),
}

impl<'a> ScopeRef<'a> {
    fn parameters(self) -> &'a [ParserParameter] {
        match self {
            ScopeRef::Section(s) => &s.parameters,
            ScopeRef::Node(n) => &n.parameters,
        }
    }

    fn parent(self) -> Option<ScopeRef<'a>> {
        match self {
            ScopeRef::Section(s) => s.parent_scope().map(ScopeRef::Node),
            ScopeRef::Node(n) => n.parent.as_deref().map(ScopeRef::Node),
        }
    }

    fn find_local(self, name: &[u8]) -> Option<&'a ParserParameter> {
        self.parameters().iter().find(|p| p.name.as_bytes() == name)
    }

    /// Upstream `getParameter(name, ctx)`: the parameter and the scope that
    /// holds it (in which its value must be evaluated).
    pub(crate) fn lookup(
        self,
        name: &[u8],
        search: SearchScope,
    ) -> Option<(&'a ParserParameter, ScopeRef<'a>)> {
        match search {
            SearchScope::Current => self.find_local(name).map(|p| (p, self)),
            SearchScope::CurrentAndParent => {
                let mut scope = Some(self);
                while let Some(s) = scope {
                    if let Some(p) = s.find_local(name) {
                        return Some((p, s));
                    }
                    scope = s.parent();
                }
                None
            }
            SearchScope::Parent => self
                .parent()
                .and_then(|p| p.lookup(name, SearchScope::CurrentAndParent)),
        }
    }
}
