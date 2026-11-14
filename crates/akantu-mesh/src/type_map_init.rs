// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/element_type_map_tmpl.hh
// (ElementTypeMapArrayInitializer, MeshElementTypeMapArrayInitializer,
// ElementTypeMapArray::initialize(const Mesh &, ...)), src/mesh/mesh.cc
// (Mesh::getNbDataPerElem).

//! Allocation of per-type arrays from a mesh (MESH-019) and the number of
//! values per element of an attached field (MESH-027 helper).
//!
//! Upstream `ElementTypeMapArray<T>::initialize(mesh, _nb_component = …,
//! _spatial_dimension = …, _ghost_type = …, _all_ghost_types = …,
//! _element_kind = …, _with_nb_element = …, _with_nb_nodes_per_element = …,
//! _element_filter = …, _default_value = …, _do_not_default = …)` maps to
//! [`initialize_from_mesh`] with a [`MeshInitialize`] value whose fields are
//! the named arguments and whose `Default` holds upstream's defaults:
//!
//! * spatial dimension: **the mesh's** (not `_all_dimensions`);
//! * ghost types: both (upstream `_casper` ⇒ `all_ghost_types`), not-ghost
//!   first;
//! * kind: every kind; `nb_component` 1; sizes 0 unless `with_nb_element`;
//! * element types: those of the element filter when one is given, else the
//!   mesh's, filtered by dimension and kind, in enum order;
//! * an existing block is resized (to 0 without `with_nb_element`: its
//!   values are lost, as upstream); a new block is allocated filled with the
//!   default value.
//!
//! Differences: an existing block whose number of components differs from
//! the requested one is an error (`ElementTypeMapArray::alloc`, upstream
//! keeps the old count silently); `do_not_default` fills new values with
//! `T::default()` (upstream leaves them as `Array::resize` makes them).

use akantu_core::{
    ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Idx, Result,
    SharedElementTypeMapArray, TypeFilter,
};

use crate::mesh::Mesh;

/// Number of components per block (upstream `_nb_component`: a scalar or a
/// functor of `(type, ghost_type)`).
#[derive(Clone, Copy)]
pub enum NbComponent<'a> {
    /// Same count for every block.
    Fixed(usize),
    /// Count per `(type, ghost)`.
    PerType(&'a dyn Fn(ElementType, GhostType) -> usize),
}

impl std::fmt::Debug for NbComponent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(n) => write!(f, "Fixed({n})"),
            Self::PerType(_) => f.write_str("PerType(..)"),
        }
    }
}

/// Spatial-dimension filter of [`MeshInitialize`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InitDimension {
    /// Upstream default: the mesh's spatial dimension.
    #[default]
    Mesh,
    /// `_all_dimensions`.
    All,
    /// One dimension.
    Dim(usize),
}

/// Named arguments of upstream `ElementTypeMapArray::initialize(mesh, …)`.
#[derive(Clone, Debug)]
pub struct MeshInitialize<'a, T> {
    /// `_nb_component` (default 1).
    pub nb_component: NbComponent<'a>,
    /// `_spatial_dimension` (default: the mesh's).
    pub spatial_dimension: InitDimension,
    /// `_ghost_type` (default `None`: upstream `_casper`, i.e. both).
    pub ghost_type: Option<GhostType>,
    /// `_all_ghost_types` (default: `ghost_type.is_none()`).
    pub all_ghost_types: Option<bool>,
    /// `_element_kind` (default `None`: every kind).
    pub element_kind: Option<ElementKind>,
    /// `_with_nb_element`: one tuple per element (default `false`: size 0).
    pub with_nb_element: bool,
    /// `_with_nb_nodes_per_element`: multiply `nb_component` by the number
    /// of nodes per element.
    pub with_nb_nodes_per_element: bool,
    /// `_element_filter`: types and sizes from these element lists.
    pub element_filter: Option<&'a ElementTypeMapArray<Idx>>,
    /// `_default_value` (default `T::default()`).
    pub default_value: T,
    /// `_do_not_default`.
    pub do_not_default: bool,
}

impl<T: Default> Default for MeshInitialize<'_, T> {
    fn default() -> Self {
        Self {
            nb_component: NbComponent::Fixed(1),
            spatial_dimension: InitDimension::Mesh,
            ghost_type: None,
            all_ghost_types: None,
            element_kind: None,
            with_nb_element: false,
            with_nb_nodes_per_element: false,
            element_filter: None,
            default_value: T::default(),
            do_not_default: false,
        }
    }
}

/// Upstream `ElementTypeMapArray<T>::initialize(mesh, …)` (see the module
/// documentation for the defaults).
///
/// # Errors
/// An existing block with another number of components, or an allocation
/// size that overflows.
pub fn initialize_from_mesh<T: Clone + Default>(
    array: &mut ElementTypeMapArray<T>,
    mesh: &Mesh,
    opts: &MeshInitialize<'_, T>,
) -> Result<()> {
    let all_ghost_types = opts.all_ghost_types.unwrap_or(opts.ghost_type.is_none());
    let spatial_dimension = match opts.spatial_dimension {
        InitDimension::Mesh => Some(mesh.spatial_dimension()),
        InitDimension::All => None,
        InitDimension::Dim(d) => Some(d),
    };
    for g in GhostType::ALL {
        if opts.ghost_type != Some(g) && !all_ghost_types {
            continue;
        }
        let f = TypeFilter {
            spatial_dimension,
            ghost_type: g,
            kind: opts.element_kind.filter(|k| *k != ElementKind::NotDefined),
        };
        let types: Vec<ElementType> = match opts.element_filter {
            Some(filter) => filter.element_types(f).collect(),
            None => mesh.element_types(f).collect(),
        };
        for ty in types {
            let size = if opts.with_nb_element {
                match opts.element_filter {
                    Some(filter) => filter.get(ty, g)?.len(),
                    None => mesh.nb_element(ty, g),
                }
            } else {
                0
            };
            let base = match opts.nb_component {
                NbComponent::Fixed(n) => n,
                NbComponent::PerType(func) => func(ty, g),
            };
            let nb_component = if opts.with_nb_nodes_per_element {
                base.checked_mul(ty.nb_nodes_per_element())
                    .ok_or_else(|| Error::invalid("initialize: component count overflow"))?
            } else {
                base
            };
            let value = if opts.do_not_default {
                T::default()
            } else {
                opts.default_value.clone()
            };
            array.alloc(size, nb_component, ty, g, value)?;
        }
    }
    Ok(())
}

/// Upstream `Mesh::getNbDataPerElem(arrays)`: for every not-ghost block of
/// `arrays` (all dimensions and kinds, enum order), `nb_component × size /
/// nb_element(type)` (integer division).
///
/// # Errors
/// A block whose type has no not-ghost element in `mesh` (upstream divides
/// by zero, UD-667).
pub fn nb_data_per_elem<T>(
    mesh: &Mesh,
    arrays: &ElementTypeMapArray<T>,
) -> Result<ElementTypeMap<usize>> {
    nb_data_per_elem_of_shapes(
        mesh,
        arrays.element_types(TypeFilter::new()).map(|ty| {
            arrays
                .get(ty, GhostType::NotGhost)
                .map(|a| (ty, a.nb_component(), a.len()))
        }),
    )
}

/// [`nb_data_per_elem`] of an exportable map (e.g. a `MeshData` typed map):
/// reads only the blocks' shapes (metadata, no guard; storage migration
/// guide P17).
///
/// # Errors
/// As [`nb_data_per_elem`].
pub fn nb_data_per_elem_shared<T>(
    mesh: &Mesh,
    arrays: &SharedElementTypeMapArray<T>,
) -> Result<ElementTypeMap<usize>> {
    nb_data_per_elem_of_shapes(
        mesh,
        arrays.element_types(TypeFilter::new()).map(|ty| {
            arrays
                .shared(ty, GhostType::NotGhost)
                .map(|a| (ty, a.nb_component(), a.len()))
        }),
    )
}

/// Shapes `(type, nb_component, len)` of the not-ghost blocks, in order
/// (a lookup error stops at its block, as upstream's loop).
fn nb_data_per_elem_of_shapes(
    mesh: &Mesh,
    shapes: impl Iterator<Item = Result<(ElementType, usize, usize)>>,
) -> Result<ElementTypeMap<usize>> {
    let mut out = ElementTypeMap::new();
    for shape in shapes {
        let (ty, nb_component, len) = shape?;
        let nb_elements = mesh.nb_element(ty, GhostType::NotGhost);
        if nb_elements == 0 {
            return Err(Error::invalid(format!(
                "getNbDataPerElem: the mesh has no element of type {ty} (upstream divides by \
                 zero)"
            )));
        }
        let values = nb_component
            .checked_mul(len)
            .ok_or_else(|| Error::invalid("getNbDataPerElem: size overflow"))?;
        out.replace(ty, GhostType::NotGhost, values / nb_elements);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MeshAccessor;
    use akantu_core::ElementType as Et;

    const NG: GhostType = GhostType::NotGhost;

    fn mesh() -> Mesh {
        let mut m = Mesh::new(2, "m").unwrap();
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [2.0, 0.0]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(Et::Quadrangle4, NG)
            .push_tuple(&[0, 1, 2, 3]);
        a.connectivity_mut(Et::Triangle3, NG).push_tuple(&[1, 4, 2]);
        a.connectivity_mut(Et::Triangle3, NG).push_tuple(&[1, 4, 3]);
        a.connectivity_mut(Et::Segment2, NG).push_tuple(&[0, 4]);
        a.connectivity_mut(Et::Segment2, GhostType::Ghost)
            .push_tuple(&[3, 2]);
        m
    }

    fn blocks<T>(a: &ElementTypeMapArray<T>) -> Vec<(Et, GhostType, usize, usize)> {
        a.as_map()
            .iter_all()
            .map(|(t, g, x)| (t, g, x.len(), x.nb_component()))
            .collect()
    }

    #[test]
    fn defaults_follow_upstream() {
        let m = mesh();
        let mut a = ElementTypeMapArray::<f64>::new("p:a");
        initialize_from_mesh(&mut a, &m, &MeshInitialize::default()).unwrap();
        // mesh dimension only (2), both ghost types, size 0, 1 component
        assert_eq!(
            blocks(&a),
            vec![(Et::Triangle3, NG, 0, 1), (Et::Quadrangle4, NG, 0, 1)]
        );
        assert_eq!(a.get(Et::Triangle3, NG).unwrap().id(), "p:a:_triangle_3");
        let opts = MeshInitialize {
            nb_component: NbComponent::Fixed(3),
            with_nb_element: true,
            default_value: 2.5,
            ..MeshInitialize::default()
        };
        initialize_from_mesh(&mut a, &m, &opts).unwrap_err(); // 1 → 3 components
        let mut b = ElementTypeMapArray::<f64>::new("p:b");
        initialize_from_mesh(&mut b, &m, &opts).unwrap();
        assert_eq!(
            blocks(&b),
            vec![(Et::Triangle3, NG, 2, 3), (Et::Quadrangle4, NG, 1, 3)]
        );
        assert!(
            b.get(Et::Triangle3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .all(|&x| x == 2.5)
        );
        // re-initialisation without with_nb_element: sizes 0
        let reinit = MeshInitialize {
            nb_component: NbComponent::Fixed(3),
            ..MeshInitialize::default()
        };
        initialize_from_mesh(&mut b, &m, &reinit).unwrap();
        assert_eq!(
            blocks(&b),
            vec![(Et::Triangle3, NG, 0, 3), (Et::Quadrangle4, NG, 0, 3)]
        );
    }

    #[test]
    fn dimensions_ghosts_kinds_filters_and_functors() {
        let m = mesh();
        let mut a = ElementTypeMapArray::<i64>::new("p:a");
        initialize_from_mesh(
            &mut a,
            &m,
            &MeshInitialize {
                nb_component: NbComponent::Fixed(2),
                spatial_dimension: InitDimension::All,
                with_nb_element: true,
                with_nb_nodes_per_element: true,
                default_value: -7,
                ..MeshInitialize::default()
            },
        )
        .unwrap();
        assert_eq!(
            blocks(&a),
            vec![
                (Et::Segment2, NG, 1, 4),
                (Et::Triangle3, NG, 2, 6),
                (Et::Quadrangle4, NG, 1, 8),
                (Et::Segment2, GhostType::Ghost, 1, 4),
            ]
        );
        let mut b = ElementTypeMapArray::<i64>::new("p:b");
        initialize_from_mesh(
            &mut b,
            &m,
            &MeshInitialize {
                spatial_dimension: InitDimension::Dim(1),
                ghost_type: Some(GhostType::Ghost),
                element_kind: Some(ElementKind::Regular),
                with_nb_element: true,
                ..MeshInitialize::default()
            },
        )
        .unwrap();
        assert_eq!(blocks(&b), vec![(Et::Segment2, GhostType::Ghost, 1, 1)]);
        let per_type =
            |t: Et, g: GhostType| t.nb_nodes_per_element() + usize::from(g == GhostType::Ghost);
        let mut c = ElementTypeMapArray::<i64>::new("p:c");
        initialize_from_mesh(
            &mut c,
            &m,
            &MeshInitialize {
                nb_component: NbComponent::PerType(&per_type),
                spatial_dimension: InitDimension::All,
                with_nb_element: true,
                ..MeshInitialize::default()
            },
        )
        .unwrap();
        assert_eq!(
            blocks(&c),
            vec![
                (Et::Segment2, NG, 1, 2),
                (Et::Triangle3, NG, 2, 3),
                (Et::Quadrangle4, NG, 1, 4),
                (Et::Segment2, GhostType::Ghost, 1, 3),
            ]
        );
        let mut filter = ElementTypeMapArray::<Idx>::new("p:filter");
        filter.alloc(1, 1, Et::Triangle3, NG, 1).unwrap();
        let mut d = ElementTypeMapArray::<f64>::new("p:d");
        initialize_from_mesh(
            &mut d,
            &m,
            &MeshInitialize {
                element_filter: Some(&filter),
                with_nb_element: true,
                nb_component: NbComponent::Fixed(2),
                default_value: 1.5,
                ..MeshInitialize::default()
            },
        )
        .unwrap();
        assert_eq!(blocks(&d), vec![(Et::Triangle3, NG, 1, 2)]);
    }

    #[test]
    fn nb_data_per_elem_divides_by_the_element_count() {
        let m = mesh();
        let mut a = ElementTypeMapArray::<f64>::new("p:a");
        a.alloc(6, 2, Et::Triangle3, NG, 0.0).unwrap(); // 2 elements × 3 × 2
        a.alloc(1, 5, Et::Quadrangle4, NG, 0.0).unwrap();
        let n = nb_data_per_elem(&m, &a).unwrap();
        assert_eq!(n.get(Et::Triangle3, NG), Some(&6));
        assert_eq!(n.get(Et::Quadrangle4, NG), Some(&5));
        a.alloc(1, 1, Et::Hexahedron8, NG, 0.0).unwrap();
        assert!(nb_data_per_elem(&m, &a).is_err());
    }
}
