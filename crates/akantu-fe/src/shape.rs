// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/shape_functions.hh,
// shape_lagrange*.hh, shape_cohesive*.hh, shape_structural*.hh.

//! Shape-function families and their precomputed data.

use akantu_core::{ElementTypeMapArray, Real};

/// Which shape-function family an [`crate::FeEngine`] uses (upstream
/// `FEEngineTemplate<IntegratorGauss, Shape*, kind>` instantiations).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeFamily {
    /// `ShapeLagrange<_ek_regular>`.
    Lagrange,
    /// `ShapeLagrange<_ek_cohesive>` (cohesive: interpolation on the
    /// mid-surface, jumps between the two sides).
    Cohesive,
    /// `ShapeStructural<_ek_structural>`: not handled by
    /// [`crate::FeEngine`]; structural elements use
    /// [`crate::structural::StructuralFeEngine`].
    Structural,
}

/// How cohesive shape functions combine the two sides of a cohesive
/// element (upstream `CohesiveReduceFunctionMean` / `...Opening` of
/// `shape_cohesive.hh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CohesiveReduction {
    /// `½ (u⁺ + u⁻)`: mid-surface interpolation (normals, positions).
    Mean,
    /// `u⁺ − u⁻`: opening.
    Opening,
}

/// Integration orders of an engine (upstream integration order functors;
/// configurable per element type, with a separate order for consistent
/// mass assembly, upstream `assembleFieldMatrix` with order `2·degree`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntegrationOrders {
    /// Explicit order per element type (`None`: the upstream default of the
    /// type, `ElementType::polynomial_degree` / cohesive functor).
    pub per_type: Vec<(akantu_core::ElementType, usize)>,
    /// Order for mass matrices (`None`: `2 · degree`).
    pub mass: Option<usize>,
}

impl IntegrationOrders {
    /// Integration order of `ty` (explicit entry, else the upstream default
    /// `ElementClassProperty<type>::polynomial_degree`).
    #[must_use]
    pub fn order(&self, ty: akantu_core::ElementType) -> usize {
        self.per_type
            .iter()
            .rev()
            .find(|(t, _)| *t == ty)
            .map_or(ty.polynomial_degree(), |(_, o)| *o)
    }

    /// Order of the consistent field-matrix (mass) integration of `ty`
    /// (upstream `assembleFieldMatrix`: `2 · polynomial_degree`, regardless
    /// of the order functor, unless [`IntegrationOrders::mass`] is set).
    #[must_use]
    pub fn mass_order(&self, ty: akantu_core::ElementType) -> usize {
        self.mass.unwrap_or(2 * ty.polynomial_degree())
    }

    /// Upstream `FacetsCohesiveIntegrationOrderFunctor` (facet FE engine of
    /// the cohesive model): every facet type that has a cohesive type uses
    /// the polynomial degree of that cohesive type; other types keep their
    /// default.
    #[must_use]
    pub fn facets_cohesive() -> Self {
        let per_type = akantu_core::ElementType::ALL
            .into_iter()
            .filter_map(|t| t.cohesive_type().map(|c| (t, c.polynomial_degree())))
            .collect();
        Self {
            per_type,
            mass: None,
        }
    }
}

/// Precomputed per-`(type, ghost)` data (upstream `ShapeLagrange::shapes`,
/// `shapes_derivatives`, `IntegratorGauss::jacobians`). Shapes:
/// * Lagrange/cohesive: `nb_shapes` and `natural_dim · nb_shapes`
///   components per point — **0 derivative components for point
///   elements** (`Point1`, natural dimension 0; zero-component arrays are
///   valid);
/// * structural: `N·T`/`B·T` blocks of [`crate::element_class::StructuralElement`]
///   (`nb_dof × nb_nodes·nb_dof` and `nb_stress × nb_nodes·nb_dof`).
#[derive(Clone, Debug, Default)]
pub struct PrecomputedShapes {
    /// `nb_element * nb_quad` tuples of `nb_shapes`.
    pub shapes: ElementTypeMapArray<Real>,
    /// `nb_element * nb_quad` tuples of `dim * nb_shapes` (column-major).
    pub shapes_derivatives: ElementTypeMapArray<Real>,
    /// `nb_element * nb_quad` jacobians multiplied by quadrature weights.
    pub jacobians: ElementTypeMapArray<Real>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::{ElementType, GhostType};

    /// Point elements store one shape value and **zero**
    /// derivative components per integration point.
    #[test]
    fn point_element_storage_shapes() {
        let mut p = PrecomputedShapes::default();
        let ty = ElementType::Point1;
        let nat = ty.natural_space_dimension();
        assert_eq!(nat, 0);
        let nshapes = ty.nb_nodes_per_interpolation_element();
        p.shapes
            .alloc(3, nshapes, ty, GhostType::NotGhost, 1.0)
            .unwrap();
        p.shapes_derivatives
            .alloc(3, nat * nshapes, ty, GhostType::NotGhost, 0.0)
            .unwrap();
        let d = p.shapes_derivatives.get(ty, GhostType::NotGhost).unwrap();
        assert_eq!((d.len(), d.nb_component(), d.as_slice().len()), (3, 0, 0));
        assert_eq!(
            p.shapes.get(ty, GhostType::NotGhost).unwrap().as_slice(),
            &[1.0; 3]
        );
    }
}
