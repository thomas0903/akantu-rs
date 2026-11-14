// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh/element_type_map.hh,
// element_type_map_tmpl.hh.

//! Per-`(ElementType, GhostType)` containers.
//!
//! Upstream stores a `std::map<ElementType, Stored>` per ghost type, so
//! iteration is in **enum order**. Here the map is a dense table
//! `[[Option<T>; ElementType::COUNT]; 2]`: O(1) access, no allocation for the
//! map itself, and the same deterministic enum-order iteration. Iteration
//! order matters for bit-reproducibility (assembly order, DOF numbering,
//! dumper output), so never iterate element types in any other order.

use std::fmt;

use crate::array::Array;
use crate::element::Element;
use crate::element_type::{ElementKind, ElementType, GhostType};
use crate::error::{Error, Result};
use crate::types::Id;

/// Filter for element-type iteration (upstream named arguments
/// `_spatial_dimension`, `_ghost_type`, `_element_kind`).
///
/// Defaults: all dimensions, `NotGhost`, all kinds (upstream
/// `ElementTypeMap::elementTypes()` default). Note that upstream
/// `FEEngine::elementTypes()` defaults to `_ek_regular`; FE code must pass the
/// kind explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TypeFilter {
    /// Keep only types of this spatial dimension (`None` = `_all_dimensions`).
    pub spatial_dimension: Option<usize>,
    /// Ghost block to iterate.
    pub ghost_type: GhostType,
    /// Keep only types of this kind (`None` = `_ek_not_defined`).
    pub kind: Option<ElementKind>,
}

impl TypeFilter {
    /// Default filter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restrict to a spatial dimension.
    #[must_use]
    pub fn dim(mut self, d: usize) -> Self {
        self.spatial_dimension = Some(d);
        self
    }

    /// Select the ghost block.
    #[must_use]
    pub fn ghost(mut self, g: GhostType) -> Self {
        self.ghost_type = g;
        self
    }

    /// Restrict to a kind.
    #[must_use]
    pub fn kind(mut self, k: ElementKind) -> Self {
        self.kind = Some(k);
        self
    }

    /// Whether `ty` passes the dimension and kind criteria.
    #[must_use]
    pub fn accepts(&self, ty: ElementType) -> bool {
        self.spatial_dimension
            .is_none_or(|d| ty.spatial_dimension() == d)
            && self.kind.is_none_or(|k| ty.kind() == k)
    }
}

/// Dense map `(ElementType, GhostType) → T`.
#[derive(Clone, PartialEq)]
pub struct ElementTypeMap<T> {
    data: [[Option<T>; ElementType::COUNT]; 2],
}

impl<T> Default for ElementTypeMap<T> {
    fn default() -> Self {
        Self {
            data: [std::array::from_fn(|_| None), std::array::from_fn(|_| None)],
        }
    }
}

impl<T> ElementTypeMap<T> {
    /// Empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `exists(type, ghost_type)`.
    #[inline]
    #[must_use]
    pub fn exists(&self, ty: ElementType, ghost: GhostType) -> bool {
        self.data[ghost.index()][ty.index()].is_some()
    }

    /// Entry, if present.
    #[inline]
    #[must_use]
    pub fn get(&self, ty: ElementType, ghost: GhostType) -> Option<&T> {
        self.data[ghost.index()][ty.index()].as_ref()
    }

    /// Mutable entry, if present.
    #[inline]
    pub fn get_mut(&mut self, ty: ElementType, ghost: GhostType) -> Option<&mut T> {
        self.data[ghost.index()][ty.index()].as_mut()
    }

    /// Insert a new entry; error if one already exists (upstream throws).
    pub fn insert(&mut self, ty: ElementType, ghost: GhostType, value: T) -> Result<&mut T> {
        let slot = &mut self.data[ghost.index()][ty.index()];
        if slot.is_some() {
            return Err(Error::AlreadyExists {
                kind: "element type entry",
                id: format!("{ty}:{ghost}"),
            });
        }
        Ok(slot.insert(value))
    }

    /// Insert or replace, returning the previous value.
    pub fn replace(&mut self, ty: ElementType, ghost: GhostType, value: T) -> Option<T> {
        self.data[ghost.index()][ty.index()].replace(value)
    }

    /// Entry, creating it with `f` if absent.
    pub fn get_or_insert_with(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        f: impl FnOnce() -> T,
    ) -> &mut T {
        self.data[ghost.index()][ty.index()].get_or_insert_with(f)
    }

    /// Remove an entry.
    pub fn remove(&mut self, ty: ElementType, ghost: GhostType) -> Option<T> {
        self.data[ghost.index()][ty.index()].take()
    }

    /// Remove everything.
    pub fn clear(&mut self) {
        for g in &mut self.data {
            for e in g.iter_mut() {
                *e = None;
            }
        }
    }

    /// Present element types passing `filter`, in enum order
    /// (upstream `elementTypes(...)`).
    pub fn element_types(&self, filter: TypeFilter) -> impl Iterator<Item = ElementType> + '_ {
        let g = filter.ghost_type.index();
        ElementType::ALL
            .into_iter()
            .filter(move |&t| self.data[g][t.index()].is_some() && filter.accepts(t))
    }

    /// `(type, &value)` for one ghost block, in enum order.
    pub fn iter(&self, ghost: GhostType) -> impl Iterator<Item = (ElementType, &T)> + '_ {
        ElementType::ALL
            .into_iter()
            .filter_map(move |t| self.data[ghost.index()][t.index()].as_ref().map(|v| (t, v)))
    }

    /// `(type, &mut value)` for one ghost block, in enum order.
    pub fn iter_mut(
        &mut self,
        ghost: GhostType,
    ) -> impl Iterator<Item = (ElementType, &mut T)> + '_ {
        // ALL is ordered like the table minus index 0 (NotDefined).
        self.data[ghost.index()]
            .iter_mut()
            .enumerate()
            .skip(1)
            .filter_map(|(i, v)| {
                v.as_mut()
                    .map(|v| (ElementType::from_index(i).unwrap_or_default(), v))
            })
    }

    /// `(type, ghost, &value)` for all entries: `NotGhost` block first, then
    /// `Ghost`, each in enum order.
    pub fn iter_all(&self) -> impl Iterator<Item = (ElementType, GhostType, &T)> + '_ {
        GhostType::ALL
            .into_iter()
            .flat_map(move |g| self.iter(g).map(move |(t, v)| (t, g, v)))
    }
}

impl<T: fmt::Debug> fmt::Debug for ElementTypeMap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.iter_all().map(|(t, g, v)| (format!("{t}:{g}"), v)))
            .finish()
    }
}

/// Per-type arrays (upstream `ElementTypeMapArray<T>`): connectivities,
/// per-element data, per-quadrature-point fields (`nb_quad * nb_element`
/// tuples), internal fields of constitutive laws, element filters, …
#[derive(Clone, Default)]
pub struct ElementTypeMapArray<T> {
    map: ElementTypeMap<Array<T>>,
    id: Id,
}

impl<T> ElementTypeMapArray<T> {
    /// Empty container with an id (upstream `parent_id:id`).
    #[must_use]
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            map: ElementTypeMap::new(),
            id: id.into(),
        }
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Underlying map.
    #[must_use]
    pub fn as_map(&self) -> &ElementTypeMap<Array<T>> {
        &self.map
    }

    /// Underlying mutable map.
    pub fn as_map_mut(&mut self) -> &mut ElementTypeMap<Array<T>> {
        &mut self.map
    }

    /// Upstream `exists`.
    #[must_use]
    pub fn exists(&self, ty: ElementType, ghost: GhostType) -> bool {
        self.map.exists(ty, ghost)
    }

    /// Array of `(ty, ghost)`; error if absent (upstream throws).
    pub fn get(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<T>> {
        self.map.get(ty, ghost).ok_or_else(|| {
            Error::not_found("element type array", format!("{}:{ty}:{ghost}", self.id))
        })
    }

    /// Mutable array of `(ty, ghost)`.
    pub fn get_mut(&mut self, ty: ElementType, ghost: GhostType) -> Result<&mut Array<T>> {
        let id = &self.id;
        self.map
            .get_mut(ty, ghost)
            .ok_or_else(|| Error::not_found("element type array", format!("{id}:{ty}:{ghost}")))
    }

    /// Tuple of `element` (upstream `get(element)`); error for
    /// [`Element::NULL`].
    pub fn element(&self, element: Element) -> Result<&[T]> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::element: ElementNull"))?;
        self.get(element.ty(), element.ghost_type())?.get_tuple(i)
    }

    /// Mutable tuple of `element`.
    pub fn element_mut(&mut self, element: Element) -> Result<&mut [T]> {
        let i = element
            .try_idx()
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::element_mut: ElementNull"))?;
        let a = self.get_mut(element.ty(), element.ghost_type())?;
        let len = a.len();
        if i >= len {
            return Err(Error::OutOfRange {
                what: "element",
                index: i,
                len,
            });
        }
        Ok(a.tuple_mut(i))
    }

    /// Present types passing `filter`, in enum order.
    pub fn element_types(&self, filter: TypeFilter) -> impl Iterator<Item = ElementType> + '_ {
        self.map.element_types(filter)
    }

    /// Remove an array.
    pub fn remove(&mut self, ty: ElementType, ghost: GhostType) -> Option<Array<T>> {
        self.map.remove(ty, ghost)
    }

    /// Remove all arrays (upstream `free`).
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Total number of tuples over all types and ghost types.
    #[must_use]
    pub fn total_len(&self) -> usize {
        self.map.iter_all().map(|(_, _, a)| a.len()).sum()
    }
}

impl<T: Clone> ElementTypeMapArray<T> {
    /// Allocate `(ty, ghost)` with `len` tuples of `nb_component` values; if
    /// it already exists it is resized instead (upstream `alloc`).
    ///
    /// * `nb_component == 0` is valid (upstream point-element
    ///   specialisations: natural dimension 0 ⇒ zero derivative components).
    /// * Array id = `"{id}:{type}"`, plus `":ghost"` for the ghost block
    ///   (upstream naming).
    /// * Upstream silently ignores a different `nb_component` for an existing
    ///   block; the port rejects it ([`Error::DimensionMismatch`]).
    /// * Sizes are checked; on error nothing is inserted or resized.
    pub fn alloc(
        &mut self,
        len: usize,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        default: T,
    ) -> Result<&mut Array<T>> {
        if let Some(a) = self.map.get(ty, ghost)
            && a.nb_component() != nb_component
        {
            return Err(Error::DimensionMismatch {
                context: "ElementTypeMapArray::alloc",
                expected: a.nb_component(),
                got: nb_component,
            });
        }
        if !self.map.exists(ty, ghost) {
            let mut a = Array::try_filled(len, nb_component, default)?;
            let ghost_suffix = if ghost == GhostType::Ghost {
                ":ghost"
            } else {
                ""
            };
            a.set_id(format!("{}:{ty}{ghost_suffix}", self.id));
            return self.map.insert(ty, ghost, a);
        }
        let a = self
            .map
            .get_mut(ty, ghost)
            .ok_or_else(|| Error::invalid("ElementTypeMapArray::alloc: missing block"))?;
        a.try_resize(len, default)?;
        Ok(a)
    }

    /// Allocate/resize several blocks at once (replaces upstream
    /// `initialize(mesh, _nb_component=…, …)`; the caller — typically the
    /// mesh or FE engine — provides `(type, ghost, nb_tuples, nb_component)`).
    pub fn initialize<I>(&mut self, blocks: I, default: T) -> Result<()>
    where
        I: IntoIterator<Item = (ElementType, GhostType, usize, usize)>,
    {
        for (ty, g, len, nbc) in blocks {
            self.alloc(len, nbc, ty, g, default.clone())?;
        }
        Ok(())
    }

    /// Set every value to `value` (upstream `set`).
    pub fn fill(&mut self, value: T) {
        for g in GhostType::ALL {
            for (_, a) in self.map.iter_mut(g) {
                a.fill(value.clone());
            }
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for ElementTypeMapArray<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ElementTypeMapArray")
            .field("id", &self.id)
            .field("map", &self.map)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_type::ElementType as Et;

    #[test]
    fn iteration_is_enum_ordered_and_filtered() {
        let mut m: ElementTypeMap<u32> = ElementTypeMap::new();
        m.insert(Et::Hexahedron8, GhostType::NotGhost, 3).unwrap();
        m.insert(Et::Triangle3, GhostType::NotGhost, 1).unwrap();
        m.insert(Et::Cohesive2d4, GhostType::NotGhost, 4).unwrap();
        m.insert(Et::Segment2, GhostType::Ghost, 2).unwrap();
        assert!(m.insert(Et::Triangle3, GhostType::NotGhost, 9).is_err());
        let types: Vec<_> = m.element_types(TypeFilter::new()).collect();
        assert_eq!(types, vec![Et::Triangle3, Et::Hexahedron8, Et::Cohesive2d4]);
        let types: Vec<_> = m.element_types(TypeFilter::new().dim(2)).collect();
        assert_eq!(types, vec![Et::Triangle3, Et::Cohesive2d4]);
        let types: Vec<_> = m
            .element_types(TypeFilter::new().dim(2).kind(ElementKind::Regular))
            .collect();
        assert_eq!(types, vec![Et::Triangle3]);
        let types: Vec<_> = m
            .element_types(TypeFilter::new().ghost(GhostType::Ghost))
            .collect();
        assert_eq!(types, vec![Et::Segment2]);
        let all: Vec<_> = m.iter_all().map(|(t, g, v)| (t, g, *v)).collect();
        assert_eq!(
            all,
            vec![
                (Et::Triangle3, GhostType::NotGhost, 1),
                (Et::Hexahedron8, GhostType::NotGhost, 3),
                (Et::Cohesive2d4, GhostType::NotGhost, 4),
                (Et::Segment2, GhostType::Ghost, 2),
            ]
        );
        for (_, v) in m.iter_mut(GhostType::NotGhost) {
            *v *= 10;
        }
        assert_eq!(m.get(Et::Hexahedron8, GhostType::NotGhost), Some(&30));
        let muts: Vec<_> = m.iter_mut(GhostType::NotGhost).map(|(t, _)| t).collect();
        assert_eq!(muts, vec![Et::Triangle3, Et::Hexahedron8, Et::Cohesive2d4]);
        assert_eq!(m.remove(Et::Segment2, GhostType::Ghost), Some(2));
        m.clear();
        assert_eq!(m.iter_all().count(), 0);
    }

    #[test]
    fn map_array_alloc_and_element_access() {
        let mut a: ElementTypeMapArray<f64> = ElementTypeMapArray::new("stress");
        a.alloc(3, 4, Et::Quadrangle4, GhostType::NotGhost, 0.0)
            .unwrap();
        a.initialize([(Et::Triangle3, GhostType::Ghost, 2, 4)], 1.0)
            .unwrap();
        assert_eq!(a.total_len(), 5);
        // resize on re-alloc, nb_component must match
        a.alloc(5, 4, Et::Quadrangle4, GhostType::NotGhost, 2.0)
            .unwrap();
        assert!(
            a.alloc(5, 3, Et::Quadrangle4, GhostType::NotGhost, 2.0)
                .is_err()
        );
        let e = Element::new(Et::Quadrangle4, 4, GhostType::NotGhost);
        assert_eq!(a.element(e).unwrap(), &[2.0; 4]);
        a.element_mut(e).unwrap()[1] = 5.0;
        assert_eq!(
            a.get(Et::Quadrangle4, GhostType::NotGhost).unwrap()[(4, 1)],
            5.0
        );
        assert!(
            a.element(Element::new(Et::Quadrangle4, 9, GhostType::NotGhost))
                .is_err()
        );
        assert!(a.get(Et::Hexahedron8, GhostType::NotGhost).is_err());
        a.fill(0.0);
        assert_eq!(a.element(e).unwrap(), &[0.0; 4]);
        assert_eq!(
            a.get(Et::Triangle3, GhostType::Ghost).unwrap().id(),
            "stress:_triangle_3:ghost"
        );
        assert_eq!(
            a.get(Et::Quadrangle4, GhostType::NotGhost).unwrap().id(),
            "stress:_quadrangle_4"
        );
        assert!(a.element(Element::NULL).is_err());
    }

    #[test]
    fn zero_component_blocks_and_checked_sizes() {
        // Point elements (natural dimension 0) have 0 derivative
        // components per quadrature point.
        let mut d: ElementTypeMapArray<f64> = ElementTypeMapArray::new("dnds");
        let a = d.alloc(4, 0, Et::Point1, GhostType::NotGhost, 0.0).unwrap();
        assert_eq!((a.len(), a.nb_component()), (4, 0));
        let e = Element::new(Et::Point1, 3, GhostType::NotGhost);
        assert_eq!(d.element(e).unwrap(), &[] as &[f64]);
        assert!(
            d.element(Element::new(Et::Point1, 4, GhostType::NotGhost))
                .is_err()
        );
        assert_eq!(d.total_len(), 4);
        // Overflow rejected before insertion / resizing
        assert!(
            d.alloc(
                usize::MAX / 2 + 1,
                2,
                Et::Segment2,
                GhostType::NotGhost,
                0.0
            )
            .is_err()
        );
        assert!(!d.exists(Et::Segment2, GhostType::NotGhost));
        d.alloc(2, 3, Et::Segment2, GhostType::NotGhost, 1.0)
            .unwrap();
        assert!(
            d.alloc(usize::MAX / 2, 3, Et::Segment2, GhostType::NotGhost, 0.0)
                .is_err()
        );
        assert_eq!(d.get(Et::Segment2, GhostType::NotGhost).unwrap().len(), 2);
    }
}

#[cfg(test)]
mod point_element_tests {
    //! Point elements have natural dimension 0, so their per-point
    //! shape-derivative blocks have `natural_dim × nb_nodes = 0` components
    //! (upstream `InterpolationElement<_itp_lagrange_point_1>::computeDNDS`
    //! writes nothing into a 0×1 matrix, `computeShapes` writes `N = 1`).
    use super::*;
    use crate::element_type::ElementType as Et;
    use crate::tensor::{MatrixExt, as_matrix};

    #[test]
    fn point_shape_and_derivative_storage() {
        let ty = Et::Point1;
        let (nat, nodes) = (ty.natural_space_dimension(), ty.nb_nodes_per_element());
        assert_eq!((nat, nodes), (0, 1));
        let nb_elem = 3;
        let mut shapes: ElementTypeMapArray<f64> = ElementTypeMapArray::new("shapes");
        let mut dnds: ElementTypeMapArray<f64> = ElementTypeMapArray::new("shapes_derivatives");
        shapes
            .alloc(nb_elem, nodes, ty, GhostType::NotGhost, 1.0)
            .unwrap();
        dnds.alloc(nb_elem, nat * nodes, ty, GhostType::NotGhost, 0.0)
            .unwrap();
        let d = dnds.get(ty, GhostType::NotGhost).unwrap();
        assert_eq!((d.len(), d.nb_component()), (nb_elem, 0));
        for q in 0..nb_elem {
            // dN/ds viewed as a (natural_dim x nb_nodes) = 0 x 1 matrix
            let m = d.tuple_matrix::<0, 1>(q).unwrap();
            assert_eq!(m.len(), 1);
            assert!(m.as_flat().is_empty());
            // interpolation of a nodal field: sum over the single shape N = 1
            let n = shapes.get(ty, GhostType::NotGhost).unwrap().tuple(q);
            assert_eq!(n, &[1.0]);
        }
        // gradient of any field on a point element: (field_dim x natural_dim)
        // = u (field_dim x nb_nodes) * dN^T (nb_nodes x 0) -> field_dim x 0
        let u = [[7.0_f64; 2]]; // 2 x 1 (one node, two components)
        let dn_t: crate::tensor::Matrix<1, 0> = [];
        let grad = u.matmul(&dn_t);
        assert_eq!(grad.len(), 0);
        assert_eq!(as_matrix::<2, 0>(&[]).unwrap().len(), 0);
    }
}
