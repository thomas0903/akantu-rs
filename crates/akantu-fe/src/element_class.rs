// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_class.hh,
// element_class_tmpl.hh, interpolation_element_tmpl.hh,
// element_classes/element_class_*_inline_impl.hh, cohesive_element.hh,
// element_class_structural.hh.

//! Element classes: compile-time element types.
//!
//! [`ElementClass`] carries the static properties (as associated `const`s
//! computed from the `akantu-core` tables, so there is a single source of
//! truth). Interpolation kernels are separate traits implemented:
//!
//! * [`LagrangeElement`]: shape functions `N(ξ)`, `dN/dξ`, `contains`,
//!   in-radius, natural-coordinate inverse map — regular and cohesive
//!   (cohesive types reuse the Lagrange interpolation of their facet);
//! * structural (Hermite/Bernoulli/DKT) and IGFEM kernels live with their
//!   shape-function families (`shape::structural`, `akantu-igfem`).
//!
//! Kernel conventions (slices, no allocation):
//! * `xi`: natural coordinates, length `NATURAL_DIM`;
//! * `n`: length `NB_SHAPES`;
//! * `dnds`: column-major `NATURAL_DIM × NB_SHAPES` (upstream `dnds`
//!   `Matrix(natural_dim, nb_nodes)`);
//! * nodal coordinates: column-major `SPATIAL_DIM × NB_NODES`.
//!   Implementations must use the **same operation order as upstream's
//!   formulas** (`element_classes/*_inline_impl.hh`) and never `mul_add`.

use akantu_core::element_type::{
    ElementKind, ElementType, GaussIntegrationType, GeometricalShapeType, GeometricalType,
    InterpolationType,
};
use akantu_core::{Real, Result};

use crate::lagrange::Interpolation;

/// Static properties of an element type.
pub trait ElementClass: Copy + Default + Send + Sync + 'static {
    /// Runtime tag.
    const TYPE: ElementType;
    /// Kind.
    const KIND: ElementKind = Self::TYPE.kind();
    /// Interpolation type.
    const INTERPOLATION: InterpolationType = Self::TYPE.interpolation_type();
    /// Geometric nodes per element.
    const NB_NODES: usize = Self::TYPE.nb_nodes_per_element();
    /// Shape functions per element (interpolation nodes).
    const NB_SHAPES: usize = Self::TYPE.nb_nodes_per_interpolation_element();
    /// Natural-space dimension.
    const NATURAL_DIM: usize = Self::TYPE.natural_space_dimension();
    /// Spatial dimension.
    const SPATIAL_DIM: usize = Self::TYPE.spatial_dimension();
    /// Default integration order.
    const POLYNOMIAL_DEGREE: usize = Self::TYPE.polynomial_degree();
    /// Geometrical type.
    const GEOMETRICAL_TYPE: GeometricalType = Self::TYPE.geometrical_type();
    /// Shape family used by `contains` (upstream `GeometricalShape`; the
    /// cohesive geometries keep upstream's default `_gst_point`).
    const SHAPE: GeometricalShapeType = Self::GEOMETRICAL_TYPE.info().shape;
    /// Gauss rule family.
    const GAUSS_INTEGRATION_TYPE: GaussIntegrationType = Self::TYPE.gauss_integration_type();
}

/// Upstream default of `inverseMap(..., max_iterations = 100, ...)`
/// (`element_class.hh:412`).
pub const INVERSE_MAP_MAX_ITERATIONS: usize = 100;
/// Upstream default of `inverseMap(..., tolerance = 1e-10)`
/// (`element_class.hh:413`).
pub const INVERSE_MAP_TOLERANCE: Real = 1e-10;

/// Lagrange/serendipity interpolation kernels (regular and cohesive kinds;
/// cohesive types interpolate with their facet's Lagrange family). Point
/// elements have `NATURAL_DIM == 0`: `xi` and `dnds` are empty slices and
/// `n == [1]`.
///
/// The provided methods delegate to the upstream-verbatim kernels of
/// [`crate::lagrange`], the geometric helpers of [`crate::geometry`] and
/// the Gauss–Newton inverse map of [`crate::kernels::inverse_map`].
pub trait LagrangeElement: ElementClass {
    /// Interpolation family (upstream `ElementClassProperty::interpolation_type`).
    type Itp: Interpolation;

    /// Shape-function values at `xi` (upstream `computeShapes`).
    #[inline]
    fn compute_shapes(xi: &[Real], n: &mut [Real]) {
        Self::Itp::shapes(xi, n);
    }
    /// Derivatives w.r.t. natural coordinates, column-major
    /// `NATURAL_DIM × NB_SHAPES` (upstream `computeDNDS`).
    #[inline]
    fn compute_dnds(xi: &[Real], dnds: &mut [Real]) {
        Self::Itp::dnds(xi, dnds);
    }
    /// Second derivatives, column-major `NATURAL_DIM² × NB_SHAPES`
    /// (upstream `computeD2NDS2`; `Error::NotImplemented` where upstream has
    /// `AKANTU_TO_IMPLEMENT`).
    #[inline]
    fn compute_d2nds2(xi: &[Real], d2nds2: &mut [Real]) -> Result<()> {
        Self::Itp::d2nds2(xi, d2nds2)
    }
    /// `true` if `xi` lies in the reference element (upstream `contains`,
    /// default `Math::tolerance`).
    #[inline]
    fn contains(xi: &[Real]) -> bool {
        Self::contains_with_tolerance(xi, akantu_core::math::DEFAULT_TOLERANCE)
    }
    /// `contains` with an explicit `Math::tolerance` (upstream global
    /// `Math::setTolerance`).
    #[inline]
    fn contains_with_tolerance(xi: &[Real], tolerance: Real) -> bool {
        crate::geometry::contains(Self::SHAPE, xi, tolerance)
    }
    /// Characteristic length (upstream `getInradius`); `coords` is the
    /// column-major `sdim × NB_NODES` matrix of node coordinates.
    #[inline]
    fn inradius(coords: &[Real]) -> Real {
        crate::geometry::inradius(Self::GEOMETRICAL_TYPE, coords, Self::NB_NODES)
    }
    /// Natural coordinates of the real point `real` (length `sdim`) in the
    /// element with nodal coordinates `node_coords` (column-major
    /// `sdim × NB_SHAPES`), upstream `inverseMap`
    /// (`element_class_tmpl.hh:425-552`): Gauss–Newton iterations from
    /// `ξ = 0` — with the least-squares `JᵀJ` system for embedded elements —
    /// stopping at `tolerance` on the physical residual or failing after
    /// `max_iterations` (callers pass [`INVERSE_MAP_MAX_ITERATIONS`] /
    /// [`INVERSE_MAP_TOLERANCE`] unless upstream passes others).
    /// Non-convergence is `Error::NotConverged`, never a silent result.
    /// Upstream has no closed form for linear simplices; cohesive kinds
    /// return `Error::NotImplemented` (upstream `AKANTU_TO_IMPLEMENT`).
    fn inverse_map(
        real: &[Real],
        node_coords: &[Real],
        natural: &mut [Real],
        max_iterations: usize,
        tolerance: Real,
    ) -> Result<()> {
        if Self::KIND != ElementKind::Regular {
            return Err(akantu_core::Error::NotImplemented(
                "inverseMap for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        crate::kernels::inverse_map::<Self>(real, node_coords, natural, max_iterations, tolerance)
    }
}

/// Structural interpolation kernels (Hermite beams, DKT,
/// INT-002) live in [`crate::structural::element`]; re-exported here so the
/// family traits sit side by side. The trait replaces the earlier,
/// unimplemented placeholder (`compute_rotation`, `compute_shapes_rotated`,
/// `compute_shapes_derivatives_rotated`) by primitive fallible kernels;
/// the storage contract — `N·T`, `B·T` per quadrature point, rotations
/// `ndof × ndof` per element — is kept in [`crate::structural::engine`]
/// (`docs/ARCHITECTURE.md`).
pub use crate::structural::element::StructuralElement;

macro_rules! declare_element_classes {
    ($($name:ident),* $(,)?) => {$(
        #[doc = concat!("Compile-time element type `", stringify!($name), "`.")]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $name;
        impl ElementClass for $name {
            const TYPE: ElementType = ElementType::$name;
        }
    )*};
}

/// Zero-sized element-type markers.
pub mod types {
    use super::{ElementClass, ElementType};
    declare_element_classes!(
        Point1,
        Segment2,
        Segment3,
        Triangle3,
        Triangle6,
        Quadrangle4,
        Quadrangle8,
        Tetrahedron4,
        Tetrahedron10,
        Pentahedron6,
        Pentahedron15,
        Hexahedron8,
        Hexahedron20,
        Cohesive1d2,
        Cohesive2d4,
        Cohesive2d6,
        Cohesive3d12,
        Cohesive3d16,
        Cohesive3d6,
        Cohesive3d8,
        BernoulliBeam2,
        BernoulliBeam3,
        DiscreteKirchhoffTriangle18,
        IgfemSegment3,
        IgfemTriangle4,
        IgfemTriangle5,
    );
}

macro_rules! lagrange_elements {
    ($($ty:ident => $itp:ident),* $(,)?) => {$(
        impl LagrangeElement for types::$ty {
            type Itp = crate::lagrange::$itp;
        }
    )*};
}

lagrange_elements!(
    Point1 => ItpPoint1,
    Segment2 => ItpSegment2,
    Segment3 => ItpSegment3,
    Triangle3 => ItpTriangle3,
    Triangle6 => ItpTriangle6,
    Quadrangle4 => ItpQuadrangle4,
    Quadrangle8 => ItpQuadrangle8,
    Tetrahedron4 => ItpTetrahedron4,
    Tetrahedron10 => ItpTetrahedron10,
    Pentahedron6 => ItpPentahedron6,
    Pentahedron15 => ItpPentahedron15,
    Hexahedron8 => ItpHexahedron8,
    Hexahedron20 => ItpHexahedron20,
    Cohesive1d2 => ItpPoint1,
    Cohesive2d4 => ItpSegment2,
    Cohesive2d6 => ItpSegment3,
    Cohesive3d6 => ItpTriangle3,
    Cohesive3d12 => ItpTriangle6,
    Cohesive3d8 => ItpQuadrangle4,
    Cohesive3d16 => ItpQuadrangle8,
);

/// Run `$body` with `$E` bound to the [`ElementClass`] ZST of `$ty`
/// (regular and cohesive kinds), or evaluate `$fallback` for other types.
///
/// ```
/// use akantu_core::ElementType;
/// use akantu_fe::with_element_class;
/// use akantu_fe::element_class::ElementClass;
/// let n = with_element_class!(ElementType::Hexahedron20, E => E::NB_NODES, else 0);
/// assert_eq!(n, 20);
/// ```
#[macro_export]
macro_rules! with_element_class {
    ($ty:expr, $E:ident => $body:expr, else $fallback:expr) => {{
        use $crate::element_class::types as __t;
        match $ty {
            akantu_core::ElementType::Point1 => {
                type $E = __t::Point1;
                $body
            }
            akantu_core::ElementType::Segment2 => {
                type $E = __t::Segment2;
                $body
            }
            akantu_core::ElementType::Segment3 => {
                type $E = __t::Segment3;
                $body
            }
            akantu_core::ElementType::Triangle3 => {
                type $E = __t::Triangle3;
                $body
            }
            akantu_core::ElementType::Triangle6 => {
                type $E = __t::Triangle6;
                $body
            }
            akantu_core::ElementType::Quadrangle4 => {
                type $E = __t::Quadrangle4;
                $body
            }
            akantu_core::ElementType::Quadrangle8 => {
                type $E = __t::Quadrangle8;
                $body
            }
            akantu_core::ElementType::Tetrahedron4 => {
                type $E = __t::Tetrahedron4;
                $body
            }
            akantu_core::ElementType::Tetrahedron10 => {
                type $E = __t::Tetrahedron10;
                $body
            }
            akantu_core::ElementType::Pentahedron6 => {
                type $E = __t::Pentahedron6;
                $body
            }
            akantu_core::ElementType::Pentahedron15 => {
                type $E = __t::Pentahedron15;
                $body
            }
            akantu_core::ElementType::Hexahedron8 => {
                type $E = __t::Hexahedron8;
                $body
            }
            akantu_core::ElementType::Hexahedron20 => {
                type $E = __t::Hexahedron20;
                $body
            }
            akantu_core::ElementType::Cohesive1d2 => {
                type $E = __t::Cohesive1d2;
                $body
            }
            akantu_core::ElementType::Cohesive2d4 => {
                type $E = __t::Cohesive2d4;
                $body
            }
            akantu_core::ElementType::Cohesive2d6 => {
                type $E = __t::Cohesive2d6;
                $body
            }
            akantu_core::ElementType::Cohesive3d12 => {
                type $E = __t::Cohesive3d12;
                $body
            }
            akantu_core::ElementType::Cohesive3d16 => {
                type $E = __t::Cohesive3d16;
                $body
            }
            akantu_core::ElementType::Cohesive3d6 => {
                type $E = __t::Cohesive3d6;
                $body
            }
            akantu_core::ElementType::Cohesive3d8 => {
                type $E = __t::Cohesive3d8;
                $body
            }
            _ => $fallback,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::types::*;
    use super::*;

    #[test]
    fn consts_follow_core_tables() {
        assert_eq!(Hexahedron20::NB_NODES, 20);
        assert_eq!(Cohesive2d4::NB_SHAPES, 2);
        assert_eq!(Cohesive2d4::NATURAL_DIM, 1);
        assert_eq!(BernoulliBeam2::NB_NODES, 2);
        assert_eq!(DiscreteKirchhoffTriangle18::SPATIAL_DIM, 3);
        assert_eq!(Tetrahedron10::KIND, ElementKind::Regular);
        for t in ElementType::ALL {
            let n = with_element_class!(t, E => {
                assert_eq!(E::TYPE, t);
                E::NB_NODES
            }, else t.nb_nodes_per_element());
            assert_eq!(n, t.nb_nodes_per_element());
        }
    }

    #[test]
    fn lagrange_families_match_core_tables() {
        for t in ElementType::ALL {
            with_element_class!(t, E => {
                assert_eq!(<E as LagrangeElement>::Itp::ITP, t.interpolation_type());
                assert_eq!(<E as LagrangeElement>::Itp::NB_NODES, E::NB_SHAPES);
                assert_eq!(<E as LagrangeElement>::Itp::NATURAL_DIM, E::NATURAL_DIM);
            }, else ());
        }
        assert_eq!(Cohesive2d4::SHAPE, GeometricalShapeType::Point);
        assert_eq!(Pentahedron15::SHAPE, GeometricalShapeType::Prism);
    }
}
