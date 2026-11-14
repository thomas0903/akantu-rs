// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/mesh_iterators.hh
// (MeshElementsByTypes, element_range, for_each_element),
// src/mesh/mesh_filter.hh (FilterFunctor, NodeFilterFunctor,
// ElementFilterFunctor).

//! Mesh iteration helpers (MESH-031) and filter functors (MESH-032).
//!
//! Upstream's iterator adaptors (`zip`, `enumerate`, `arange`, … — ITER-001
//! to ITER-007) are Rust's iterator combinators; only the mesh-specific
//! helpers with observable defaults are ported here.

use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, Result, TypeFilter,
};

use crate::mesh::Mesh;

/// Elements grouped into contiguous `(type, ghost)` ranges (upstream
/// `MeshElementsByTypes`: the list is sorted with `Element::operator<`,
/// then split into ranges of equal ghost type and type).
#[derive(Clone, Debug, Default)]
pub struct MeshElementsByTypes {
    elements: Vec<Element>,
}

/// One `(type, ghost)` range of [`MeshElementsByTypes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshElementsRange {
    /// Element type.
    pub ty: ElementType,
    /// Ghost type.
    pub ghost_type: GhostType,
    /// Element indices, sorted.
    pub elements: Vec<Idx>,
}

impl MeshElementsByTypes {
    /// Sorted copy of `elements`. `Element::NULL` entries are dropped
    /// (upstream's range search does not terminate on them).
    #[must_use]
    pub fn new(elements: &[Element]) -> Self {
        let mut elements: Vec<Element> =
            elements.iter().copied().filter(|e| !e.is_null()).collect();
        elements.sort_unstable();
        Self { elements }
    }

    /// The ranges, in `(ghost, type)` order.
    #[must_use]
    pub fn ranges(&self) -> Vec<MeshElementsRange> {
        let mut out: Vec<MeshElementsRange> = Vec::new();
        for e in &self.elements {
            match out.last_mut() {
                Some(r) if r.ty == e.ty() && r.ghost_type == e.ghost_type() => {
                    r.elements.push(e.index())
                }
                _ => out.push(MeshElementsRange {
                    ty: e.ty(),
                    ghost_type: e.ghost_type(),
                    elements: vec![e.index()],
                }),
            }
        }
        out
    }
}

/// Elements `start..stop` of `(ty, ghost)` (upstream `element_range`).
pub fn element_range(
    start: usize,
    stop: usize,
    ty: ElementType,
    ghost: GhostType,
) -> impl Iterator<Item = Result<Element>> {
    (start..stop).map(move |e| Element::try_new(ty, e, ghost))
}

/// Options of [`for_each_element`] (upstream named arguments).
#[derive(Clone, Copy, Debug, Default)]
pub struct ElementIteration<'a> {
    /// `_spatial_dimension` (default: the **mesh** dimension; `Some(None)`
    /// is not representable upstream — use [`ElementIteration::all_dimensions`]).
    pub spatial_dimension: Option<Option<usize>>,
    /// `_ghost_type` (default `_casper`: both, `NotGhost` first).
    pub ghost_type: Option<GhostType>,
    /// `_element_kind` (default `_ek_not_defined`: all kinds).
    pub kind: Option<ElementKind>,
    /// `_element_filter`: iterate the listed indices of the filter's types
    /// instead of every element.
    pub element_filter: Option<&'a ElementTypeMapArray<Idx>>,
}

impl ElementIteration<'_> {
    /// Iterate every dimension (`_spatial_dimension = _all_dimensions`).
    #[must_use]
    pub fn all_dimensions(mut self) -> Self {
        self.spatial_dimension = Some(None);
        self
    }
}

/// Upstream `for_each_element(mesh, function, named args…)`: for each ghost
/// type (both by default, `NotGhost` first), each type of the requested
/// dimension (default: mesh dimension) and kind (default: all) present in
/// the mesh — or in the element filter if given — every element (or every
/// filtered index), in order. Stops at the first error of `f`.
pub fn for_each_element(
    mesh: &Mesh,
    options: ElementIteration<'_>,
    mut f: impl FnMut(Element) -> Result<()>,
) -> Result<()> {
    let dim = options
        .spatial_dimension
        .unwrap_or(Some(mesh.spatial_dimension()));
    for ghost in GhostType::ALL {
        if options.ghost_type.is_some_and(|g| g != ghost) {
            continue;
        }
        let tf = TypeFilter {
            spatial_dimension: dim,
            ghost_type: ghost,
            kind: options.kind,
        };
        match options.element_filter {
            None => {
                let types: Vec<ElementType> = mesh.element_types(tf).collect();
                for ty in types {
                    for e in 0..mesh.nb_element(ty, ghost) {
                        f(Element::try_new(ty, e, ghost)?)?;
                    }
                }
            }
            Some(filter) => {
                let types: Vec<ElementType> = filter.element_types(tf).collect();
                for ty in types {
                    for &e in filter.get(ty, ghost)?.as_slice() {
                        f(Element::from_raw_parts(ty, e, ghost)?)?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Kind of a filter functor (upstream `FilterFunctor::Type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterFunctorType {
    /// `_node_filter_functor`.
    Node,
    /// `_element_filter_functor`.
    Element,
}

/// Node filter (upstream `NodeFilterFunctor`): `true` keeps the node.
pub trait NodeFilter {
    /// Keep `node`?
    fn keep(&mut self, node: Idx) -> bool;
}

impl<F: FnMut(Idx) -> bool> NodeFilter for F {
    fn keep(&mut self, node: Idx) -> bool {
        self(node)
    }
}

/// Element filter (upstream `ElementFilterFunctor`; upstream's group
/// manager cannot apply element filters, `AKANTU_ERROR`).
pub trait ElementFilter {
    /// Keep `element`?
    fn keep(&mut self, element: Element) -> bool;
}

impl<F: FnMut(Element) -> bool> ElementFilter for F {
    fn keep(&mut self, element: Element) -> bool {
        self(element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessor::MeshAccessor;

    #[test]
    fn ranges_are_sorted_by_ghost_then_type() {
        let t = |ty, i, g| Element::new(ty, i, g);
        let by = MeshElementsByTypes::new(&[
            t(ElementType::Triangle3, 4, GhostType::Ghost),
            t(ElementType::Segment2, 2, GhostType::NotGhost),
            Element::NULL,
            t(ElementType::Triangle3, 1, GhostType::NotGhost),
            t(ElementType::Segment2, 0, GhostType::NotGhost),
        ]);
        let r = by.ranges();
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[0].ty, r[0].elements.clone()),
            (ElementType::Segment2, vec![0, 2])
        );
        assert_eq!(r[1].ty, ElementType::Triangle3);
        assert_eq!(r[2].ghost_type, GhostType::Ghost);
        assert_eq!(
            element_range(1, 3, ElementType::Point1, GhostType::NotGhost).count(),
            2
        );
    }

    #[test]
    fn for_each_element_defaults() {
        let mut m = Mesh::new(2, "m").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost)
                .push_tuple(&[0, 1, 2]);
            a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
                .push_tuple(&[0, 1]);
            a.connectivity_mut(ElementType::Triangle3, GhostType::Ghost)
                .push_tuple(&[0, 1, 2]);
        }
        let mut seen = Vec::new();
        for_each_element(&m, ElementIteration::default(), |e| {
            seen.push(e);
            Ok(())
        })
        .unwrap();
        // mesh dimension only, both ghost types
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].ghost_type(), GhostType::Ghost);
        let mut all = 0;
        for_each_element(&m, ElementIteration::default().all_dimensions(), |_| {
            all += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(all, 3);
        let mut filter = ElementTypeMapArray::new("f");
        filter
            .alloc(0, 1, ElementType::Segment2, GhostType::NotGhost, 0)
            .unwrap()
            .push_value(0);
        let mut got = Vec::new();
        let opts = ElementIteration {
            spatial_dimension: Some(Some(1)),
            element_filter: Some(&filter),
            ..ElementIteration::default()
        };
        for_each_element(&m, opts, |e| {
            got.push(e);
            Ok(())
        })
        .unwrap();
        assert_eq!(
            got,
            vec![Element::new(ElementType::Segment2, 0, GhostType::NotGhost)]
        );
    }
}
