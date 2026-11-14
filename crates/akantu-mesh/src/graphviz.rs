// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/boost_graph_converter.{hh,cc}
// (BoostGraphConverter, toGraphviz). The text layout follows
// boost::write_graphviz (Boost Graph Library, boost/graph/graphviz.hpp,
// Boost Software License 1.0) for a directed adjacency_list<> with vertex
// and edge property writers.

//! Element adjacency graph of a mesh and its facet mesh, written as
//! Graphviz `.dot` (upstream `BoostGraphConverter`, package `debug_tools`;
//! MUTIL-017).
//!
//! Vertices, in upstream order: the regular elements of the mesh dimension
//! (both ghost types, not-ghost first, types in enum order), every element
//! of the facet mesh (all dimensions), the cohesive elements of the mesh
//! dimension. An element listed twice gets a second vertex and later edges
//! use the most recent one (upstream `std::map` assignment).
//!
//! Edges (directed, parallel edges kept, out-edges per source in insertion
//! order as Boost's `vecS` lists): for every regular element of the mesh,
//! then every facet of dimension `1 .. dim − 1`, then every cohesive
//! element, an `up` edge to each of its sub-elements
//! (`subelement_to_element` of the facet mesh) followed by `down` edges from
//! that sub-element to each element of its `element_to_subelement` row;
//! edges touching the null element are skipped.
//!
//! Output: `digraph G {`, one line `i[label="Element [type, index, ghost]",
//! …` (ghost type as 0/1, as upstream's `std::to_string(Element)`)
//! `bgcolor=c, color=c];` per vertex (colour by spatial dimension: gold,
//! cadetblue1, coral1, antiquewhite2), one line `s->t [color=c];` per edge
//! (black, darkorange, blue2 for none/up/down), `}`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use akantu_core::{Element, ElementKind, Error, GhostType, Result, TypeFilter};

use crate::mesh::Mesh;
use crate::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};

/// Kind of a graph edge (upstream `EdgeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// `_none`.
    None,
    /// `_up`: element → sub-element.
    Up,
    /// `_down`: sub-element → adjacent element.
    Down,
}

impl EdgeKind {
    fn color(self) -> &'static str {
        match self {
            EdgeKind::None => "black",
            EdgeKind::Up => "darkorange",
            EdgeKind::Down => "blue2",
        }
    }
}

/// The element graph (upstream `BoostGraphConverter`).
#[derive(Clone, Debug, Default)]
pub struct ElementGraph {
    /// Element of every vertex.
    pub vertices: Vec<Element>,
    /// Out-edges of every vertex `(target, kind)`, in insertion order.
    pub out_edges: Vec<Vec<(usize, EdgeKind)>>,
}

/// Elements of `mesh` of `dim` (`None`: all) and `kind` (`None`: all), both
/// ghost types (not-ghost first), types in enum order (upstream
/// `for_each_element`).
fn elements_of(mesh: &Mesh, dim: Option<usize>, kind: Option<ElementKind>) -> Vec<Element> {
    let mut out = Vec::new();
    for g in GhostType::ALL {
        let f = TypeFilter {
            spatial_dimension: dim,
            ghost_type: g,
            kind,
        };
        for ty in mesh.element_types(f) {
            for i in 0..mesh.nb_element(ty, g) {
                if let Ok(e) = Element::try_new(ty, i, g) {
                    out.push(e);
                }
            }
        }
    }
    out
}

impl ElementGraph {
    /// Upstream `BoostGraphConverter(mesh)`.
    ///
    /// # Errors
    /// `mesh` has no facet mesh (upstream `getMeshFacets` throws), a
    /// sub-element row of an element is missing or holds the null element
    /// (upstream throws when it looks up the null element's neighbours), or
    /// an element referenced by the adjacency data has no vertex (upstream
    /// `AKANTU_EXCEPTION`).
    pub fn new(mesh: &Mesh) -> Result<Self> {
        let facets = mesh
            .mesh_facets()
            .ok_or_else(|| Error::Mesh("BoostGraphConverter: the mesh has no facet mesh".into()))?;
        let dim = mesh.spatial_dimension();
        let mut g = ElementGraph::default();
        let mut vertex_of: HashMap<Element, usize> = HashMap::new();
        let mut add_vertex = |g: &mut ElementGraph, e: Element| {
            vertex_of.insert(e, g.vertices.len());
            g.vertices.push(e);
            g.out_edges.push(Vec::new());
        };
        for e in elements_of(mesh, Some(dim), Some(ElementKind::Regular)) {
            add_vertex(&mut g, e);
        }
        for e in elements_of(facets, None, None) {
            add_vertex(&mut g, e);
        }
        for e in elements_of(mesh, Some(dim), Some(ElementKind::Cohesive)) {
            add_vertex(&mut g, e);
        }
        let s2e = facets.data().element_map(SUBELEMENT_TO_ELEMENT)?;
        let e2s = facets.data().element_lists(ELEMENT_TO_SUBELEMENT)?;
        let vertex = |e: Element| -> Result<Option<usize>> {
            if e.is_null() {
                return Ok(None);
            }
            vertex_of
                .get(&e)
                .copied()
                .map(Some)
                .ok_or_else(|| Error::Mesh(format!("Element {e:?} not in element to vertex")))
        };
        let add_neighbors = |g: &mut ElementGraph, element: Element| -> Result<()> {
            for &facet in s2e.element(element)? {
                if let (Some(a), Some(b)) = (vertex(element)?, vertex(facet)?) {
                    g.out_edges[a].push((b, EdgeKind::Up));
                }
                if facet.is_null() {
                    return Err(Error::Mesh(format!(
                        "BoostGraphConverter: element {element:?} has a null sub-element \
                         (upstream throws looking up its neighbours)"
                    )));
                }
                for &c in &e2s.element(facet)?[0] {
                    if let (Some(a), Some(b)) = (vertex(facet)?, vertex(c)?) {
                        g.out_edges[a].push((b, EdgeKind::Down));
                    }
                }
            }
            Ok(())
        };
        for e in elements_of(mesh, Some(dim), Some(ElementKind::Regular)) {
            add_neighbors(&mut g, e)?;
        }
        for s in 1..dim {
            for e in elements_of(facets, Some(s), None) {
                add_neighbors(&mut g, e)?;
            }
        }
        for e in elements_of(mesh, Some(dim), Some(ElementKind::Cohesive)) {
            add_neighbors(&mut g, e)?;
        }
        Ok(g)
    }

    /// Upstream `toGraphviz` text (`boost::write_graphviz` layout).
    #[must_use]
    pub fn to_graphviz(&self) -> String {
        const COLORS: [&str; 4] = ["gold", "cadetblue1", "coral1", "antiquewhite2"];
        let mut s = String::from("digraph G {\n");
        for (i, e) in self.vertices.iter().enumerate() {
            let c = if e.is_null() {
                "deeppink4"
            } else {
                COLORS
                    .get(e.ty().spatial_dimension())
                    .copied()
                    .unwrap_or("deeppink4")
            };
            let label = if e.is_null() {
                "ElementNull".to_owned()
            } else {
                // upstream `std::to_string(element)`: the ghost type is a
                // plain enum, printed as its integer value
                format!(
                    "Element [{}, {}, {}]",
                    e.ty().name(),
                    e.index(),
                    u8::from(e.ghost_type() == GhostType::Ghost)
                )
            };
            let _ = writeln!(s, "{i}[label=\"{label}\", bgcolor={c}, color={c}];");
        }
        for (a, out) in self.out_edges.iter().enumerate() {
            for &(b, k) in out {
                let _ = writeln!(s, "{a}->{b} [color={}];", k.color());
            }
        }
        s.push_str("}\n");
        s
    }

    /// Write [`ElementGraph::to_graphviz`] to `path`.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn write_graphviz(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_graphviz()).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })
    }
}
