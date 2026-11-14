// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
//   src/common/aka_element_classes_info.hh            (enums, enum order)
//   src/common/aka_common.hh                          (GhostType)
//   src/fe_engine/element_class.hh                    (shapes, defaults)
//   src/fe_engine/element_classes/element_class_*_inline_impl.hh
//                                                     (ElementClassProperty)
//   src/fe_engine/geometrical_element_property.hh     (facets, p1, facet types)
//   src/fe_engine/interpolation_element_tmpl.hh       (InterpolationProperty)
//   src/fe_engine/cohesive_element.hh                 (cohesive properties)
//   src/fe_engine/element_class_structural.hh         (structural properties)
//   src/fe_engine/element_classes/element_class_hermite_inline_impl.hh
//   extra_packages/igfem/src/{element_class_igfem.hh,
//     geometrical_element_igfem.hh, interpolation_element_igfem_tmpl.hh,
//     element_classes_igfem/*.hh}                     (IGFEM properties)

//! Element-type enumerations and their **static property tables**.
//!
//! These tables are the single source of truth for per-type constants
//! (number of nodes, dimensions, facets, kinds, …) used by the mesh, the
//! FE engine, I/O and the models. All values are transcribed verbatim from
//! the upstream sources listed in the file header and are checked by the unit
//! tests below (including topological invariants).
//!
//! Differences to upstream, all deliberate (see D-012/D-013):
//! * every variant is always present (upstream compiles cohesive/structural/
//!   IGFEM types only with the corresponding CMake package); Cargo features
//!   gate *implementations*, never enum variants;
//! * `_max_element_type` is replaced by [`ElementType::COUNT`];
//! * `GhostType::_casper` is not a variant (it is only used inside
//!   `ElementNull`, see [`crate::Element::NULL`]);
//! * upstream does not declare `_ek_igfem` in core (the IGFEM extra package is
//!   stale); it is [`ElementKind::Igfem`] here, so `ElementKind::NotDefined`
//!   has discriminant 4 instead of 3;
//! * properties that upstream leaves `AKANTU_TO_IMPLEMENT()` (P1 type and
//!   facet types of structural and IGFEM elements) are `None`.

use std::fmt;
use std::str::FromStr;

use crate::error::Error;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Element types (upstream `enum ElementType`, same order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ElementType {
    /// `_not_defined` (sentinel, e.g. in `ElementNull`).
    #[default]
    NotDefined = 0,
    /// `_point_1`
    Point1,
    /// `_segment_2`
    Segment2,
    /// `_segment_3`
    Segment3,
    /// `_triangle_3`
    Triangle3,
    /// `_triangle_6`
    Triangle6,
    /// `_quadrangle_4`
    Quadrangle4,
    /// `_quadrangle_8`
    Quadrangle8,
    /// `_tetrahedron_4`
    Tetrahedron4,
    /// `_tetrahedron_10`
    Tetrahedron10,
    /// `_pentahedron_6`
    Pentahedron6,
    /// `_pentahedron_15`
    Pentahedron15,
    /// `_hexahedron_8`
    Hexahedron8,
    /// `_hexahedron_20`
    Hexahedron20,
    /// `_cohesive_1d_2`
    Cohesive1d2,
    /// `_cohesive_2d_4`
    Cohesive2d4,
    /// `_cohesive_2d_6`
    Cohesive2d6,
    /// `_cohesive_3d_12`
    Cohesive3d12,
    /// `_cohesive_3d_16`
    Cohesive3d16,
    /// `_cohesive_3d_6`
    Cohesive3d6,
    /// `_cohesive_3d_8`
    Cohesive3d8,
    /// `_bernoulli_beam_2`
    BernoulliBeam2,
    /// `_bernoulli_beam_3`
    BernoulliBeam3,
    /// `_discrete_kirchhoff_triangle_18`
    DiscreteKirchhoffTriangle18,
    /// `_igfem_segment_3`
    IgfemSegment3,
    /// `_igfem_triangle_4`
    IgfemTriangle4,
    /// `_igfem_triangle_5`
    IgfemTriangle5,
}

/// Element kinds (upstream `enum ElementKind` + `_ek_igfem`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementKind {
    /// `_ek_regular`
    Regular = 0,
    /// `_ek_cohesive`
    Cohesive = 1,
    /// `_ek_structural`
    Structural = 2,
    /// `_ek_igfem` (extra package)
    Igfem = 3,
    /// `_ek_not_defined`
    NotDefined = 4,
}

/// Ghost status of an element block (upstream `enum GhostType` without the
/// `_casper` sentinel).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum GhostType {
    /// `_not_ghost`: owned by this rank.
    #[default]
    NotGhost = 0,
    /// `_ghost`: copy of an element owned by another rank.
    Ghost = 1,
}

/// Geometrical types (upstream `enum GeometricalType`, same order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)] // names mirror upstream `_gt_*` one-to-one
pub enum GeometricalType {
    Point = 0,
    Segment2,
    Segment3,
    Triangle3,
    Triangle6,
    Quadrangle4,
    Quadrangle8,
    Tetrahedron4,
    Tetrahedron10,
    Hexahedron8,
    Hexahedron20,
    Pentahedron6,
    Pentahedron15,
    Cohesive1d2,
    Cohesive2d4,
    Cohesive2d6,
    Cohesive3d12,
    Cohesive3d16,
    Cohesive3d6,
    Cohesive3d8,
    IgfemSegment3,
    IgfemTriangle4,
    IgfemTriangle5,
    NotDefined,
}

/// Interpolation types (upstream `enum InterpolationType`, same order).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)] // names mirror upstream `_itp_*` one-to-one
pub enum InterpolationType {
    LagrangePoint1 = 0,
    LagrangeSegment2,
    LagrangeSegment3,
    LagrangeTriangle3,
    LagrangeTriangle6,
    LagrangeQuadrangle4,
    SerendipQuadrangle8,
    LagrangeTetrahedron4,
    LagrangeTetrahedron10,
    LagrangeHexahedron8,
    SerendipHexahedron20,
    LagrangePentahedron6,
    LagrangePentahedron15,
    Hermite2,
    BernoulliBeam2,
    BernoulliBeam3,
    DiscreteKirchhoffTriangle18,
    IgfemSegment3,
    IgfemTriangle4,
    IgfemTriangle5,
    NotDefined,
}

/// Shape families used by `contains` (upstream `GeometricalShapeType`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum GeometricalShapeType {
    Point,
    Triangle,
    Square,
    Prism,
    NotDefined,
}

/// Families of Gauss rules (upstream `GaussIntegrationType`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum GaussIntegrationType {
    Point,
    Segment,
    Triangle,
    Tetrahedron,
    Pentahedron,
    NotDefined,
}

/// Interpolation families (upstream `InterpolationKind`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum InterpolationKind {
    Lagrangian,
    Structural,
    Igfem,
    NotDefined,
}

// ---------------------------------------------------------------------------
// Property records
// ---------------------------------------------------------------------------

/// Local connectivity of one *facet type* of a geometrical type.
///
/// `data` is copied verbatim from upstream `facet_connectivity_vect`, which is
/// an Eigen **column-major** `nb_facets × nb_nodes_per_facet` matrix: the
/// local node `n` of facet `f` is `data[f + n * nb_facets]`. Use
/// [`FacetConnectivity::node`] rather than indexing `data` directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FacetConnectivity {
    /// Number of facets of this facet type.
    pub nb_facets: usize,
    /// Number of nodes of each facet.
    pub nb_nodes_per_facet: usize,
    /// Column-major `nb_facets × nb_nodes_per_facet` table.
    pub data: &'static [u8],
}

impl FacetConnectivity {
    /// Local (element) node number of local node `n` of facet `f`.
    #[inline]
    #[must_use]
    pub fn node(&self, f: usize, n: usize) -> usize {
        debug_assert!(f < self.nb_facets && n < self.nb_nodes_per_facet);
        self.data[f + n * self.nb_facets] as usize
    }

    /// Local nodes of facet `f`, in upstream order.
    pub fn facet(&self, f: usize) -> impl ExactSizeIterator<Item = usize> + '_ {
        (0..self.nb_nodes_per_facet).map(move |n| self.node(f, n))
    }
}

/// Static properties of a [`GeometricalType`]
/// (upstream `GeometricalElementProperty` + `GeometricalShape`).
#[derive(Clone, Copy, Debug)]
pub struct GeometricalTypeInfo {
    /// The type described.
    pub ty: GeometricalType,
    /// Upstream name (`_gt_*`).
    pub name: &'static str,
    /// Spatial dimension of the geometry.
    pub spatial_dimension: usize,
    /// Nodes per element.
    pub nb_nodes_per_element: usize,
    /// Shape family (upstream default `_gst_point` where not specialised).
    pub shape: GeometricalShapeType,
    /// One entry per facet type.
    pub facets: &'static [FacetConnectivity],
}

/// Extra data of structural interpolations
/// (`AKANTU_DEFINE_STRUCTURAL_INTERPOLATION_TYPE_PROPERTY`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralInterpolationInfo {
    /// Lagrange interpolation describing the geometry.
    pub geometry_type: InterpolationType,
    /// DOFs per node.
    pub nb_degree_of_freedom: usize,
    /// Stress components.
    pub nb_stress_components: usize,
    /// Columns of the `dnds` matrices.
    pub dnds_columns: usize,
}

/// Extra data of IGFEM interpolations (`interpolation_element_igfem_tmpl.hh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IgfemInterpolationInfo {
    /// Interpolation of the parent element.
    pub parent: InterpolationType,
    /// Interpolations of the two sub-elements.
    pub sub_types: [InterpolationType; 2],
    /// Nodes of each sub-element.
    pub sub_element_nb_nodes: [usize; 2],
    /// Sub-element connectivities, concatenated sub-element by sub-element.
    pub sub_element_connectivity: &'static [u8],
    /// Number of enriched nodes per sub-element.
    pub nb_enrichments: usize,
    /// Enriched nodes, `nb_enrichments` per sub-element, concatenated.
    pub enrichments: &'static [u8],
}

/// Static properties of an [`InterpolationType`] (`InterpolationProperty`).
#[derive(Clone, Copy, Debug)]
pub struct InterpolationTypeInfo {
    /// The type described.
    pub ty: InterpolationType,
    /// Upstream name (`_itp_*`).
    pub name: &'static str,
    /// Interpolation family.
    pub kind: InterpolationKind,
    /// Number of shape functions (nodes of the interpolation element).
    pub nb_nodes_per_element: usize,
    /// Dimension of the natural (reference) space.
    pub natural_space_dimension: usize,
    /// Structural extras.
    pub structural: Option<StructuralInterpolationInfo>,
    /// IGFEM extras.
    pub igfem: Option<IgfemInterpolationInfo>,
}

/// Static properties of an [`ElementType`] (`ElementClassProperty` +
/// `ElementClassExtraGeometryProperties` + `CohesiveFacetProperty`).
#[derive(Clone, Copy, Debug)]
pub struct ElementTypeInfo {
    /// The type described.
    pub ty: ElementType,
    /// Upstream name (`_triangle_3`, …).
    pub name: &'static str,
    /// Element kind.
    pub kind: ElementKind,
    /// Spatial dimension of the element (of the *mesh* for cohesive and
    /// structural elements, e.g. `_cohesive_2d_4` → 2).
    pub spatial_dimension: usize,
    /// Geometry.
    pub geometrical_type: GeometricalType,
    /// Interpolation.
    pub interpolation_type: InterpolationType,
    /// Gauss rule family.
    pub gauss_integration_type: GaussIntegrationType,
    /// Default integration order (upstream `polynomial_degree`).
    pub polynomial_degree: usize,
    /// Linear version of the element (`None` = not defined upstream).
    pub p1_type: Option<ElementType>,
    /// Element type of each facet type (`None` = not defined upstream).
    pub facet_types: Option<&'static [ElementType]>,
    /// Cohesive element inserted on a facet of this type.
    pub cohesive_type: Option<ElementType>,
    /// Parent type (structural: `parent_element_type`; IGFEM: parent).
    pub parent_element_type: Option<ElementType>,
    /// IGFEM sub-element types.
    pub igfem_sub_element_types: Option<[ElementType; 2]>,
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

const fn fc(nb_facets: usize, nb_nodes_per_facet: usize, data: &'static [u8]) -> FacetConnectivity {
    FacetConnectivity {
        nb_facets,
        nb_nodes_per_facet,
        data,
    }
}

const fn gti(
    ty: GeometricalType,
    name: &'static str,
    spatial_dimension: usize,
    nb_nodes_per_element: usize,
    shape: GeometricalShapeType,
    facets: &'static [FacetConnectivity],
) -> GeometricalTypeInfo {
    GeometricalTypeInfo {
        ty,
        name,
        spatial_dimension,
        nb_nodes_per_element,
        shape,
        facets,
    }
}

use GeometricalShapeType as Gst;
use GeometricalType as Gt;

/// Geometrical properties, indexed by `GeometricalType as usize`.
/// Source: `geometrical_element_property.hh` (regular, cohesive),
/// `geometrical_element_igfem.hh` (IGFEM), shapes from `element_class.hh`.
pub static GEOMETRICAL_TYPES: [GeometricalTypeInfo; 24] = [
    gti(Gt::Point, "_gt_point", 0, 1, Gst::Point, &[fc(1, 1, &[0])]),
    gti(
        Gt::Segment2,
        "_gt_segment_2",
        1,
        2,
        Gst::Square,
        &[fc(2, 1, &[0, 1])],
    ),
    gti(
        Gt::Segment3,
        "_gt_segment_3",
        1,
        3,
        Gst::Square,
        &[fc(2, 1, &[0, 1])],
    ),
    gti(
        Gt::Triangle3,
        "_gt_triangle_3",
        2,
        3,
        Gst::Triangle,
        &[fc(3, 2, &[0, 1, 2, 1, 2, 0])],
    ),
    gti(
        Gt::Triangle6,
        "_gt_triangle_6",
        2,
        6,
        Gst::Triangle,
        &[fc(3, 3, &[0, 1, 2, 1, 2, 0, 3, 4, 5])],
    ),
    gti(
        Gt::Quadrangle4,
        "_gt_quadrangle_4",
        2,
        4,
        Gst::Square,
        &[fc(4, 2, &[0, 1, 2, 3, 1, 2, 3, 0])],
    ),
    gti(
        Gt::Quadrangle8,
        "_gt_quadrangle_8",
        2,
        8,
        Gst::Square,
        &[fc(4, 3, &[0, 1, 2, 3, 1, 2, 3, 0, 4, 5, 6, 7])],
    ),
    gti(
        Gt::Tetrahedron4,
        "_gt_tetrahedron_4",
        3,
        4,
        Gst::Triangle,
        &[fc(4, 3, &[0, 1, 2, 0, 2, 2, 0, 1, 1, 3, 3, 3])],
    ),
    gti(
        Gt::Tetrahedron10,
        "_gt_tetrahedron_10",
        3,
        10,
        Gst::Triangle,
        &[fc(
            4,
            6,
            &[
                0, 1, 2, 0, //
                2, 2, 0, 1, //
                1, 3, 3, 3, //
                6, 5, 6, 4, //
                5, 9, 7, 8, //
                4, 8, 9, 7,
            ],
        )],
    ),
    gti(
        Gt::Hexahedron8,
        "_gt_hexahedron_8",
        3,
        8,
        Gst::Square,
        &[fc(
            6,
            4,
            &[
                0, 0, 1, 2, 3, 4, //
                3, 1, 2, 3, 0, 5, //
                2, 5, 6, 7, 4, 6, //
                1, 4, 5, 6, 7, 7,
            ],
        )],
    ),
    gti(
        Gt::Hexahedron20,
        "_gt_hexahedron_20",
        3,
        20,
        Gst::Square,
        &[fc(
            6,
            8,
            &[
                0, 1, 2, 3, 0, 4, //
                1, 2, 3, 0, 3, 5, //
                5, 6, 7, 4, 2, 6, //
                4, 5, 6, 7, 1, 7, //
                8, 9, 10, 11, 11, 16, //
                13, 14, 15, 12, 10, 17, //
                16, 17, 18, 19, 9, 18, //
                12, 13, 14, 15, 8, 19,
            ],
        )],
    ),
    gti(
        Gt::Pentahedron6,
        "_gt_pentahedron_6",
        3,
        6,
        Gst::Prism,
        &[
            fc(2, 3, &[0, 3, 2, 4, 1, 5]),
            fc(3, 4, &[0, 0, 1, 1, 3, 2, 4, 5, 5, 3, 2, 4]),
        ],
    ),
    gti(
        Gt::Pentahedron15,
        "_gt_pentahedron_15",
        3,
        15,
        Gst::Prism,
        &[
            fc(2, 6, &[0, 3, 2, 4, 1, 5, 8, 12, 7, 13, 6, 14]),
            fc(
                3,
                8,
                &[
                    0, 0, 1, //
                    1, 3, 2, //
                    4, 5, 5, //
                    3, 2, 4, //
                    6, 9, 7, //
                    10, 14, 11, //
                    12, 11, 13, //
                    9, 8, 10,
                ],
            ),
        ],
    ),
    // Cohesive geometries: upstream has no `AKANTU_DEFINE_SHAPE` for them, so
    // the primary template's default `_gst_point` applies.
    gti(
        Gt::Cohesive1d2,
        "_gt_cohesive_1d_2",
        1,
        2,
        Gst::Point,
        &[fc(2, 1, &[0, 1])],
    ),
    gti(
        Gt::Cohesive2d4,
        "_gt_cohesive_2d_4",
        2,
        4,
        Gst::Point,
        &[fc(2, 2, &[0, 2, 1, 3])],
    ),
    gti(
        Gt::Cohesive2d6,
        "_gt_cohesive_2d_6",
        2,
        6,
        Gst::Point,
        &[fc(2, 3, &[0, 3, 1, 4, 2, 5])],
    ),
    gti(
        Gt::Cohesive3d12,
        "_gt_cohesive_3d_12",
        3,
        12,
        Gst::Point,
        &[fc(2, 6, &[0, 6, 1, 7, 2, 8, 3, 9, 4, 10, 5, 11])],
    ),
    gti(
        Gt::Cohesive3d16,
        "_gt_cohesive_3d_16",
        3,
        16,
        Gst::Point,
        &[fc(
            2,
            8,
            &[0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13, 6, 14, 7, 15],
        )],
    ),
    gti(
        Gt::Cohesive3d6,
        "_gt_cohesive_3d_6",
        3,
        6,
        Gst::Point,
        &[fc(2, 3, &[0, 3, 1, 4, 2, 5])],
    ),
    gti(
        Gt::Cohesive3d8,
        "_gt_cohesive_3d_8",
        3,
        8,
        Gst::Point,
        &[fc(2, 4, &[0, 4, 1, 5, 2, 6, 3, 7])],
    ),
    gti(
        Gt::IgfemSegment3,
        "_gt_igfem_segment_3",
        1,
        3,
        Gst::Point,
        &[fc(2, 1, &[0, 1])],
    ),
    gti(
        Gt::IgfemTriangle4,
        "_gt_igfem_triangle_4",
        2,
        4,
        Gst::Point,
        &[fc(2, 2, &[0, 2, 1, 0]), fc(1, 3, &[1, 2, 3])],
    ),
    gti(
        Gt::IgfemTriangle5,
        "_gt_igfem_triangle_5",
        2,
        5,
        Gst::Point,
        &[fc(1, 2, &[1, 2]), fc(2, 3, &[0, 2, 1, 0, 3, 4])],
    ),
    gti(
        Gt::NotDefined,
        "_gt_not_defined",
        0,
        0,
        Gst::Point,
        &[fc(0, 0, &[])],
    ),
];

const fn iti(
    ty: InterpolationType,
    name: &'static str,
    kind: InterpolationKind,
    nb_nodes_per_element: usize,
    natural_space_dimension: usize,
) -> InterpolationTypeInfo {
    InterpolationTypeInfo {
        ty,
        name,
        kind,
        nb_nodes_per_element,
        natural_space_dimension,
        structural: None,
        igfem: None,
    }
}

#[allow(clippy::too_many_arguments)] // mirrors the upstream property macro
const fn iti_struct(
    ty: InterpolationType,
    name: &'static str,
    geometry: InterpolationType,
    geometry_nb_nodes: usize,
    geometry_natural_dim: usize,
    nb_degree_of_freedom: usize,
    nb_stress_components: usize,
    dnds_columns: usize,
) -> InterpolationTypeInfo {
    InterpolationTypeInfo {
        ty,
        name,
        kind: InterpolationKind::Structural,
        nb_nodes_per_element: geometry_nb_nodes,
        natural_space_dimension: geometry_natural_dim,
        structural: Some(StructuralInterpolationInfo {
            geometry_type: geometry,
            nb_degree_of_freedom,
            nb_stress_components,
            dnds_columns,
        }),
        igfem: None,
    }
}

const fn iti_igfem(
    ty: InterpolationType,
    name: &'static str,
    nb_nodes_per_element: usize,
    natural_space_dimension: usize,
    igfem: IgfemInterpolationInfo,
) -> InterpolationTypeInfo {
    InterpolationTypeInfo {
        ty,
        name,
        kind: InterpolationKind::Igfem,
        nb_nodes_per_element,
        natural_space_dimension,
        structural: None,
        igfem: Some(igfem),
    }
}

use InterpolationKind as Itk;
use InterpolationType as Itp;

/// Interpolation properties, indexed by `InterpolationType as usize`.
/// Sources: `interpolation_element_tmpl.hh` (Lagrange),
/// `element_class_structural.hh` + `element_class_hermite_inline_impl.hh`
/// (structural; nodes/natural dim are those of `itp_geometry_type`),
/// `interpolation_element_igfem_tmpl.hh` (IGFEM).
pub static INTERPOLATION_TYPES: [InterpolationTypeInfo; 21] = [
    iti(
        Itp::LagrangePoint1,
        "_itp_lagrange_point_1",
        Itk::Lagrangian,
        1,
        0,
    ),
    iti(
        Itp::LagrangeSegment2,
        "_itp_lagrange_segment_2",
        Itk::Lagrangian,
        2,
        1,
    ),
    iti(
        Itp::LagrangeSegment3,
        "_itp_lagrange_segment_3",
        Itk::Lagrangian,
        3,
        1,
    ),
    iti(
        Itp::LagrangeTriangle3,
        "_itp_lagrange_triangle_3",
        Itk::Lagrangian,
        3,
        2,
    ),
    iti(
        Itp::LagrangeTriangle6,
        "_itp_lagrange_triangle_6",
        Itk::Lagrangian,
        6,
        2,
    ),
    iti(
        Itp::LagrangeQuadrangle4,
        "_itp_lagrange_quadrangle_4",
        Itk::Lagrangian,
        4,
        2,
    ),
    iti(
        Itp::SerendipQuadrangle8,
        "_itp_serendip_quadrangle_8",
        Itk::Lagrangian,
        8,
        2,
    ),
    iti(
        Itp::LagrangeTetrahedron4,
        "_itp_lagrange_tetrahedron_4",
        Itk::Lagrangian,
        4,
        3,
    ),
    iti(
        Itp::LagrangeTetrahedron10,
        "_itp_lagrange_tetrahedron_10",
        Itk::Lagrangian,
        10,
        3,
    ),
    iti(
        Itp::LagrangeHexahedron8,
        "_itp_lagrange_hexahedron_8",
        Itk::Lagrangian,
        8,
        3,
    ),
    iti(
        Itp::SerendipHexahedron20,
        "_itp_serendip_hexahedron_20",
        Itk::Lagrangian,
        20,
        3,
    ),
    iti(
        Itp::LagrangePentahedron6,
        "_itp_lagrange_pentahedron_6",
        Itk::Lagrangian,
        6,
        3,
    ),
    iti(
        Itp::LagrangePentahedron15,
        "_itp_lagrange_pentahedron_15",
        Itk::Lagrangian,
        15,
        3,
    ),
    iti_struct(
        Itp::Hermite2,
        "_itp_hermite_2",
        Itp::LagrangeSegment2,
        2,
        1,
        2,
        1,
        4,
    ),
    iti_struct(
        Itp::BernoulliBeam2,
        "_itp_bernoulli_beam_2",
        Itp::LagrangeSegment2,
        2,
        1,
        3,
        2,
        6,
    ),
    iti_struct(
        Itp::BernoulliBeam3,
        "_itp_bernoulli_beam_3",
        Itp::LagrangeSegment2,
        2,
        1,
        6,
        4,
        6,
    ),
    iti_struct(
        Itp::DiscreteKirchhoffTriangle18,
        "_itp_discrete_kirchhoff_triangle_18",
        Itp::LagrangeTriangle3,
        3,
        2,
        6,
        6,
        21,
    ),
    iti_igfem(
        Itp::IgfemSegment3,
        "_itp_igfem_segment_3",
        3,
        1,
        IgfemInterpolationInfo {
            parent: Itp::LagrangeSegment2,
            sub_types: [Itp::LagrangeSegment2, Itp::LagrangeSegment2],
            sub_element_nb_nodes: [2, 2],
            sub_element_connectivity: &[0, 2, 2, 1],
            nb_enrichments: 1,
            enrichments: &[1, 0],
        },
    ),
    iti_igfem(
        Itp::IgfemTriangle4,
        "_itp_igfem_triangle_4",
        4,
        2,
        IgfemInterpolationInfo {
            parent: Itp::LagrangeTriangle3,
            sub_types: [Itp::LagrangeTriangle3, Itp::LagrangeTriangle3],
            sub_element_nb_nodes: [3, 3],
            sub_element_connectivity: &[0, 1, 3, 0, 3, 2],
            nb_enrichments: 1,
            enrichments: &[2, 1],
        },
    ),
    iti_igfem(
        Itp::IgfemTriangle5,
        "_itp_igfem_triangle_5",
        5,
        2,
        IgfemInterpolationInfo {
            parent: Itp::LagrangeTriangle3,
            sub_types: [Itp::LagrangeTriangle3, Itp::LagrangeQuadrangle4],
            sub_element_nb_nodes: [3, 4],
            sub_element_connectivity: &[0, 3, 4, 3, 1, 2, 4],
            nb_enrichments: 2,
            enrichments: &[1, 2, 0, 3],
        },
    ),
    iti(Itp::NotDefined, "_itp_not_defined", Itk::NotDefined, 0, 0),
];

use ElementKind as Ek;
use ElementType as Et;
use GaussIntegrationType as Git;

#[allow(clippy::too_many_arguments)] // mirrors the upstream property macro
const fn eti(
    ty: ElementType,
    name: &'static str,
    geometrical_type: GeometricalType,
    interpolation_type: InterpolationType,
    kind: ElementKind,
    spatial_dimension: usize,
    gauss_integration_type: GaussIntegrationType,
    polynomial_degree: usize,
    p1_type: Option<ElementType>,
    facet_types: Option<&'static [ElementType]>,
) -> ElementTypeInfo {
    ElementTypeInfo {
        ty,
        name,
        kind,
        spatial_dimension,
        geometrical_type,
        interpolation_type,
        gauss_integration_type,
        polynomial_degree,
        p1_type,
        facet_types,
        cohesive_type: None,
        parent_element_type: None,
        igfem_sub_element_types: None,
    }
}

const fn with_cohesive(mut info: ElementTypeInfo, c: ElementType) -> ElementTypeInfo {
    info.cohesive_type = Some(c);
    info
}

const fn with_parent(mut info: ElementTypeInfo, p: ElementType) -> ElementTypeInfo {
    info.parent_element_type = Some(p);
    info
}

const fn with_igfem(
    mut info: ElementTypeInfo,
    p: ElementType,
    s: [ElementType; 2],
) -> ElementTypeInfo {
    info.parent_element_type = Some(p);
    info.igfem_sub_element_types = Some(s);
    info
}

/// Element properties, indexed by `ElementType as usize`.
///
/// Columns follow `AKANTU_DEFINE_ELEMENT_CLASS_PROPERTY(type, geom, interp,
/// kind, spatial_dim, gauss_type, polynomial_degree)`; P1 and facet types from
/// `ElementClassExtraGeometryProperties`; cohesive types from
/// `AKANTU_DEFINE_COHESIVE_FACET_PROPERTY`; structural parents from
/// `AKANTU_DEFINE_STRUCTURAL_ELEMENT_CLASS_PROPERTY`; IGFEM from
/// `AKANTU_DEFINE_IGFEM_ELEMENT_CLASS_PROPERTY` (no Gauss type in upstream's
/// IGFEM macro → `NotDefined`).
#[rustfmt::skip] // one row per upstream property macro, for comparison with upstream
pub static ELEMENT_TYPES: [ElementTypeInfo; ElementType::COUNT] = [
    // `_not_defined`: primary-template defaults (kind `_ek_not_defined` here,
    // upstream's tuple_dispatch throws for it).
    eti(Et::NotDefined, "_not_defined", Gt::NotDefined, Itp::NotDefined, Ek::NotDefined, 0, Git::NotDefined, 0, None, None),
    with_cohesive(eti(Et::Point1, "_point_1", Gt::Point, Itp::LagrangePoint1, Ek::Regular, 0, Git::Point, 1, Some(Et::Point1), Some(&[Et::Point1])), Et::Cohesive1d2),
    with_cohesive(eti(Et::Segment2, "_segment_2", Gt::Segment2, Itp::LagrangeSegment2, Ek::Regular, 1, Git::Segment, 1, Some(Et::Segment2), Some(&[Et::Point1])), Et::Cohesive2d4),
    with_cohesive(eti(Et::Segment3, "_segment_3", Gt::Segment3, Itp::LagrangeSegment3, Ek::Regular, 1, Git::Segment, 2, Some(Et::Segment2), Some(&[Et::Point1])), Et::Cohesive2d6),
    with_cohesive(eti(Et::Triangle3, "_triangle_3", Gt::Triangle3, Itp::LagrangeTriangle3, Ek::Regular, 2, Git::Triangle, 1, Some(Et::Triangle3), Some(&[Et::Segment2])), Et::Cohesive3d6),
    with_cohesive(eti(Et::Triangle6, "_triangle_6", Gt::Triangle6, Itp::LagrangeTriangle6, Ek::Regular, 2, Git::Triangle, 2, Some(Et::Triangle3), Some(&[Et::Segment3])), Et::Cohesive3d12),
    with_cohesive(eti(Et::Quadrangle4, "_quadrangle_4", Gt::Quadrangle4, Itp::LagrangeQuadrangle4, Ek::Regular, 2, Git::Segment, 2, Some(Et::Quadrangle4), Some(&[Et::Segment2])), Et::Cohesive3d8),
    with_cohesive(eti(Et::Quadrangle8, "_quadrangle_8", Gt::Quadrangle8, Itp::SerendipQuadrangle8, Ek::Regular, 2, Git::Segment, 3, Some(Et::Quadrangle4), Some(&[Et::Segment3])), Et::Cohesive3d16),
    eti(Et::Tetrahedron4, "_tetrahedron_4", Gt::Tetrahedron4, Itp::LagrangeTetrahedron4, Ek::Regular, 3, Git::Tetrahedron, 1, Some(Et::Tetrahedron4), Some(&[Et::Triangle3])),
    eti(Et::Tetrahedron10, "_tetrahedron_10", Gt::Tetrahedron10, Itp::LagrangeTetrahedron10, Ek::Regular, 3, Git::Tetrahedron, 2, Some(Et::Tetrahedron4), Some(&[Et::Triangle6])),
    eti(Et::Pentahedron6, "_pentahedron_6", Gt::Pentahedron6, Itp::LagrangePentahedron6, Ek::Regular, 3, Git::Pentahedron, 1, Some(Et::Pentahedron6), Some(&[Et::Triangle3, Et::Quadrangle4])),
    eti(Et::Pentahedron15, "_pentahedron_15", Gt::Pentahedron15, Itp::LagrangePentahedron15, Ek::Regular, 3, Git::Pentahedron, 2, Some(Et::Pentahedron6), Some(&[Et::Triangle6, Et::Quadrangle8])),
    eti(Et::Hexahedron8, "_hexahedron_8", Gt::Hexahedron8, Itp::LagrangeHexahedron8, Ek::Regular, 3, Git::Segment, 2, Some(Et::Hexahedron8), Some(&[Et::Quadrangle4])),
    eti(Et::Hexahedron20, "_hexahedron_20", Gt::Hexahedron20, Itp::SerendipHexahedron20, Ek::Regular, 3, Git::Segment, 3, Some(Et::Hexahedron8), Some(&[Et::Quadrangle8])),
    eti(Et::Cohesive1d2, "_cohesive_1d_2", Gt::Cohesive1d2, Itp::LagrangePoint1, Ek::Cohesive, 1, Git::Point, 1, Some(Et::Cohesive1d2), Some(&[Et::Point1])),
    eti(Et::Cohesive2d4, "_cohesive_2d_4", Gt::Cohesive2d4, Itp::LagrangeSegment2, Ek::Cohesive, 2, Git::Segment, 2, Some(Et::Cohesive2d4), Some(&[Et::Segment2])),
    eti(Et::Cohesive2d6, "_cohesive_2d_6", Gt::Cohesive2d6, Itp::LagrangeSegment3, Ek::Cohesive, 2, Git::Segment, 3, Some(Et::Cohesive2d4), Some(&[Et::Segment3])),
    eti(Et::Cohesive3d12, "_cohesive_3d_12", Gt::Cohesive3d12, Itp::LagrangeTriangle6, Ek::Cohesive, 3, Git::Triangle, 3, Some(Et::Cohesive3d6), Some(&[Et::Triangle6])),
    eti(Et::Cohesive3d16, "_cohesive_3d_16", Gt::Cohesive3d16, Itp::SerendipQuadrangle8, Ek::Cohesive, 3, Git::Segment, 3, Some(Et::Cohesive3d8), Some(&[Et::Quadrangle8])),
    eti(Et::Cohesive3d6, "_cohesive_3d_6", Gt::Cohesive3d6, Itp::LagrangeTriangle3, Ek::Cohesive, 3, Git::Triangle, 2, Some(Et::Cohesive3d6), Some(&[Et::Triangle3])),
    eti(Et::Cohesive3d8, "_cohesive_3d_8", Gt::Cohesive3d8, Itp::LagrangeQuadrangle4, Ek::Cohesive, 3, Git::Segment, 2, Some(Et::Cohesive3d8), Some(&[Et::Quadrangle4])),
    with_parent(eti(Et::BernoulliBeam2, "_bernoulli_beam_2", Gt::Segment2, Itp::BernoulliBeam2, Ek::Structural, 2, Git::Segment, 3, None, None), Et::Segment2),
    with_parent(eti(Et::BernoulliBeam3, "_bernoulli_beam_3", Gt::Segment2, Itp::BernoulliBeam3, Ek::Structural, 3, Git::Segment, 3, None, None), Et::Segment2),
    with_parent(eti(Et::DiscreteKirchhoffTriangle18, "_discrete_kirchhoff_triangle_18", Gt::Triangle3, Itp::DiscreteKirchhoffTriangle18, Ek::Structural, 3, Git::Triangle, 2, None, None), Et::Triangle3),
    with_igfem(eti(Et::IgfemSegment3, "_igfem_segment_3", Gt::IgfemSegment3, Itp::IgfemSegment3, Ek::Igfem, 1, Git::NotDefined, 1, None, None), Et::Segment2, [Et::Segment2, Et::Segment2]),
    with_igfem(eti(Et::IgfemTriangle4, "_igfem_triangle_4", Gt::IgfemTriangle4, Itp::IgfemTriangle4, Ek::Igfem, 2, Git::NotDefined, 1, None, None), Et::Triangle3, [Et::Triangle3, Et::Triangle3]),
    with_igfem(eti(Et::IgfemTriangle5, "_igfem_triangle_5", Gt::IgfemTriangle5, Itp::IgfemTriangle5, Ek::Igfem, 2, Git::NotDefined, 1, None, None), Et::Triangle3, [Et::Triangle3, Et::Quadrangle4]),
];

// ---------------------------------------------------------------------------
// ElementType API
// ---------------------------------------------------------------------------

impl ElementType {
    /// Number of variants including [`ElementType::NotDefined`]
    /// (upstream `_max_element_type`).
    pub const COUNT: usize = 27;

    /// All real element types in upstream enum order (excludes `NotDefined`).
    /// Iteration over element types anywhere in akantu-rs must follow this
    /// order (upstream iterates `std::map<ElementType, …>` in enum order).
    pub const ALL: [ElementType; 26] = [
        Et::Point1,
        Et::Segment2,
        Et::Segment3,
        Et::Triangle3,
        Et::Triangle6,
        Et::Quadrangle4,
        Et::Quadrangle8,
        Et::Tetrahedron4,
        Et::Tetrahedron10,
        Et::Pentahedron6,
        Et::Pentahedron15,
        Et::Hexahedron8,
        Et::Hexahedron20,
        Et::Cohesive1d2,
        Et::Cohesive2d4,
        Et::Cohesive2d6,
        Et::Cohesive3d12,
        Et::Cohesive3d16,
        Et::Cohesive3d6,
        Et::Cohesive3d8,
        Et::BernoulliBeam2,
        Et::BernoulliBeam3,
        Et::DiscreteKirchhoffTriangle18,
        Et::IgfemSegment3,
        Et::IgfemTriangle4,
        Et::IgfemTriangle5,
    ];

    /// Dense index (`self as usize`), for table lookups.
    #[inline(always)]
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Inverse of [`ElementType::index`].
    #[must_use]
    pub const fn from_index(i: usize) -> Option<Self> {
        if i == 0 {
            Some(Et::NotDefined)
        } else if i < Self::COUNT {
            Some(Self::ALL[i - 1])
        } else {
            None
        }
    }

    /// Full static property record.
    #[inline(always)]
    #[must_use]
    pub const fn info(self) -> &'static ElementTypeInfo {
        &ELEMENT_TYPES[self as usize]
    }

    /// Upstream name, e.g. `"_triangle_3"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.info().name
    }

    /// Element kind (upstream `Mesh::getKind`).
    #[must_use]
    pub const fn kind(self) -> ElementKind {
        self.info().kind
    }

    /// Spatial dimension (upstream `Mesh::getSpatialDimension`).
    #[must_use]
    pub const fn spatial_dimension(self) -> usize {
        self.info().spatial_dimension
    }

    /// Geometrical type.
    #[must_use]
    pub const fn geometrical_type(self) -> GeometricalType {
        self.info().geometrical_type
    }

    /// Interpolation type (upstream `FEEngine::getInterpolationType`).
    #[must_use]
    pub const fn interpolation_type(self) -> InterpolationType {
        self.info().interpolation_type
    }

    /// Gauss rule family.
    #[must_use]
    pub const fn gauss_integration_type(self) -> GaussIntegrationType {
        self.info().gauss_integration_type
    }

    /// Default integration order.
    #[must_use]
    pub const fn polynomial_degree(self) -> usize {
        self.info().polynomial_degree
    }

    /// Nodes per element (upstream `Mesh::getNbNodesPerElement`, i.e. the
    /// geometrical node count; `0` for `NotDefined`).
    #[must_use]
    pub const fn nb_nodes_per_element(self) -> usize {
        self.geometrical_type().info().nb_nodes_per_element
    }

    /// Natural-space dimension (upstream `Mesh::getNaturalSpaceDimension`).
    #[must_use]
    pub const fn natural_space_dimension(self) -> usize {
        self.interpolation_type().info().natural_space_dimension
    }

    /// Number of shape functions (upstream `getShapeSize` /
    /// `getNbNodesPerInterpolationElement`).
    #[must_use]
    pub const fn nb_nodes_per_interpolation_element(self) -> usize {
        self.interpolation_type().info().nb_nodes_per_element
    }

    /// Linear version (upstream `Mesh::getP1ElementType`).
    #[must_use]
    pub const fn p1_type(self) -> Option<ElementType> {
        self.info().p1_type
    }

    /// Facet element types, one per facet type (upstream
    /// `Mesh::getAllFacetTypes`). `None` where upstream does not define them.
    #[must_use]
    pub const fn facet_types(self) -> Option<&'static [ElementType]> {
        self.info().facet_types
    }

    /// Facet element type `t` (upstream `Mesh::getFacetType`).
    #[must_use]
    pub fn facet_type(self, t: usize) -> Option<ElementType> {
        self.facet_types().and_then(|f| f.get(t).copied())
    }

    /// Number of facet types (upstream `Mesh::getNbFacetTypes`).
    #[must_use]
    pub const fn nb_facet_types(self) -> usize {
        self.geometrical_type().info().facets.len()
    }

    /// Local facet connectivity of facet type `t`
    /// (upstream `Mesh::getFacetLocalConnectivity`).
    #[must_use]
    pub fn facet_connectivity(self, t: usize) -> Option<&'static FacetConnectivity> {
        self.geometrical_type().info().facets.get(t)
    }

    /// Total number of facets (upstream `Mesh::getNbFacetsPerElement`).
    #[must_use]
    pub fn nb_facets_per_element(self) -> usize {
        self.geometrical_type()
            .info()
            .facets
            .iter()
            .map(|f| f.nb_facets)
            .sum()
    }

    /// Cohesive element type to insert on a facet of this type (upstream
    /// `FEEngine::getCohesiveElementType`).
    #[must_use]
    pub const fn cohesive_type(self) -> Option<ElementType> {
        self.info().cohesive_type
    }

    /// Structural/IGFEM parent element type.
    #[must_use]
    pub const fn parent_element_type(self) -> Option<ElementType> {
        self.info().parent_element_type
    }

    /// Iterator over all real element types of `kind`, in enum order.
    pub fn of_kind(kind: ElementKind) -> impl Iterator<Item = ElementType> {
        Self::ALL.into_iter().filter(move |t| t.kind() == kind)
    }
}

impl GeometricalType {
    /// Static property record.
    #[must_use]
    pub const fn info(self) -> &'static GeometricalTypeInfo {
        &GEOMETRICAL_TYPES[self as usize]
    }
}

impl InterpolationType {
    /// Static property record.
    #[must_use]
    pub const fn info(self) -> &'static InterpolationTypeInfo {
        &INTERPOLATION_TYPES[self as usize]
    }
}

impl ElementKind {
    /// All real kinds (excludes `NotDefined`).
    pub const ALL: [ElementKind; 4] = [Ek::Regular, Ek::Cohesive, Ek::Structural, Ek::Igfem];

    /// Upstream name, e.g. `"_ek_regular"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Ek::Regular => "_ek_regular",
            Ek::Cohesive => "_ek_cohesive",
            Ek::Structural => "_ek_structural",
            Ek::Igfem => "_ek_igfem",
            Ek::NotDefined => "_ek_not_defined",
        }
    }
}

impl GhostType {
    /// Both ghost types, `NotGhost` first (upstream `ghost_types`).
    pub const ALL: [GhostType; 2] = [GhostType::NotGhost, GhostType::Ghost];

    /// Upstream Python/enum name (`"_not_ghost"`, `"_ghost"`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            GhostType::NotGhost => "_not_ghost",
            GhostType::Ghost => "_ghost",
        }
    }

    /// Dense index.
    #[inline(always)]
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for ElementType {
    /// Upstream `operator<<(ElementType)`: `_triangle_3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for ElementKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for GhostType {
    /// Upstream `operator<<(GhostType)`: `not_ghost` / `ghost`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            GhostType::NotGhost => "not_ghost",
            GhostType::Ghost => "ghost",
        })
    }
}

impl fmt::Display for GeometricalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.info().name)
    }
}

impl fmt::Display for InterpolationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.info().name)
    }
}

impl FromStr for ElementType {
    type Err = Error;
    /// Exact lookup of **one token**, the upstream identifier
    /// (`"_triangle_3"`), like the map lookup of upstream's
    /// `AKANTU_ENUM_INPUT_STREAM(ElementType, AKANTU_ALL_ELEMENT_TYPE)`
    /// (`aka_element_classes_info_inline_impl.hh:143`): `"triangle_3"` and
    /// `"_not_defined"` are rejected.
    /// Stream tokenisation is the caller's job: upstream's
    /// `operator>>` first reads one whitespace-delimited token (`stream >>
    /// str`), so it also accepts `" _triangle_3"`, `"_triangle_3\n"` or
    /// `"_triangle_3 _quadrangle_4"` (rest unread), which this `FromStr`
    /// refuses (oracle `ENUM` records). No input path
    /// of the port reaches the difference (upstream's only parser use,
    /// `MaterialFE2`'s `element_type`, is not ported), so it is not a UD
    /// row. The IGFEM types, always compiled in the port, parse as their
    /// names; the strict oracle build (IGFEM off) refuses them.
    fn from_str(s: &str) -> Result<Self, Error> {
        ELEMENT_TYPES
            .iter()
            .find(|i| i.ty != Et::NotDefined && i.name == s)
            .map(|i| i.ty)
            .ok_or_else(|| Error::invalid(format!("'{s}' is not a valid ElementType")))
    }
}

impl FromStr for ElementKind {
    type Err = Error;
    /// Exact lookup of one token, the upstream identifier (`"_ek_regular"`),
    /// like upstream's `AKANTU_ENUM_INPUT_STREAM(ElementKind,
    /// AKANTU_ELEMENT_KIND)`: the list holds the kinds but not
    /// `_ek_not_defined`; whitespace around the token is the
    /// caller's to strip, as for [`ElementType`]. `_ek_igfem` is the
    /// IGFEM extra package's kind (refused by the strict oracle build).
    fn from_str(s: &str) -> Result<Self, Error> {
        ElementKind::ALL
            .into_iter()
            .find(|k| k.name() == s)
            .ok_or_else(|| Error::invalid(format!("'{s}' is not a valid ElementKind")))
    }
}

impl FromStr for GhostType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        match s.strip_prefix('_').unwrap_or(s) {
            "not_ghost" => Ok(GhostType::NotGhost),
            "ghost" => Ok(GhostType::Ghost),
            _ => Err(Error::invalid(format!("'{s}' is not a valid GhostType"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn tables_are_indexed_by_discriminant() {
        for (i, info) in ELEMENT_TYPES.iter().enumerate() {
            assert_eq!(info.ty as usize, i, "{}", info.name);
            assert_eq!(ElementType::from_index(i), Some(info.ty));
        }
        assert_eq!(ElementType::from_index(ElementType::COUNT), None);
        for (i, info) in GEOMETRICAL_TYPES.iter().enumerate() {
            assert_eq!(info.ty as usize, i, "{}", info.name);
        }
        for (i, info) in INTERPOLATION_TYPES.iter().enumerate() {
            assert_eq!(info.ty as usize, i, "{}", info.name);
        }
        assert_eq!(ElementType::ALL.len() + 1, ElementType::COUNT);
        assert!(ElementType::ALL.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn names_roundtrip() {
        for t in ElementType::ALL
            .into_iter()
            .chain([ElementType::NotDefined])
        {
            assert_eq!(t.to_string(), t.name());
        }
        for t in ElementType::ALL {
            assert_eq!(t.name().parse::<ElementType>().unwrap(), t);
        }
        assert!("triangle_4".parse::<ElementType>().is_err());
        assert_eq!(
            "_ek_cohesive".parse::<ElementKind>().unwrap(),
            ElementKind::Cohesive
        );
        assert_eq!(
            "not_ghost".parse::<GhostType>().unwrap(),
            GhostType::NotGhost
        );
        assert_eq!(GhostType::Ghost.to_string(), "ghost");
    }

    /// Upstream's
    /// `AKANTU_ENUM_INPUT_STREAM` looks one token up exactly among the
    /// identifiers of `AKANTU_ALL_ELEMENT_TYPE` / `AKANTU_ELEMENT_KIND`; the
    /// bare name, the `not_defined` sentinels and decorated spellings are
    /// rejected. Whitespace-padded spellings are rejected here, while
    /// upstream's stream tokenises them away first (the
    /// caller's job; no ported input path reaches it).
    #[test]
    fn element_type_and_kind_parsing_is_exact_like_upstream() {
        for t in ElementType::ALL {
            let bare = &t.name()[1..];
            assert!(bare.parse::<ElementType>().is_err(), "`{bare}` accepted");
            for bad in [
                format!(" {}", t.name()),
                format!("{} ", t.name()),
                t.name().to_uppercase(),
                format!("_{}", t.name()),
            ] {
                assert!(bad.parse::<ElementType>().is_err(), "`{bad}` accepted");
            }
        }
        for bad in ["_not_defined", "not_defined", "_max_element_type", ""] {
            assert!(bad.parse::<ElementType>().is_err(), "`{bad}` accepted");
        }
        for k in ElementKind::ALL {
            assert_eq!(k.name().parse::<ElementKind>().unwrap(), k);
            let bare = &k.name()[1..];
            assert!(bare.parse::<ElementKind>().is_err(), "`{bare}` accepted");
        }
        for bad in ["_ek_not_defined", "ek_not_defined", "_EK_REGULAR", ""] {
            assert!(bad.parse::<ElementKind>().is_err(), "`{bad}` accepted");
        }
    }

    /// Values spot-checked against the upstream macros (see file header).
    #[test]
    fn upstream_values() {
        // (type, nb_nodes, spatial_dim, natural_dim, kind, polynomial_degree, nb_facets)
        let expected = [
            (Et::Point1, 1, 0, 0, Ek::Regular, 1, 1),
            (Et::Segment2, 2, 1, 1, Ek::Regular, 1, 2),
            (Et::Segment3, 3, 1, 1, Ek::Regular, 2, 2),
            (Et::Triangle3, 3, 2, 2, Ek::Regular, 1, 3),
            (Et::Triangle6, 6, 2, 2, Ek::Regular, 2, 3),
            (Et::Quadrangle4, 4, 2, 2, Ek::Regular, 2, 4),
            (Et::Quadrangle8, 8, 2, 2, Ek::Regular, 3, 4),
            (Et::Tetrahedron4, 4, 3, 3, Ek::Regular, 1, 4),
            (Et::Tetrahedron10, 10, 3, 3, Ek::Regular, 2, 4),
            (Et::Pentahedron6, 6, 3, 3, Ek::Regular, 1, 5),
            (Et::Pentahedron15, 15, 3, 3, Ek::Regular, 2, 5),
            (Et::Hexahedron8, 8, 3, 3, Ek::Regular, 2, 6),
            (Et::Hexahedron20, 20, 3, 3, Ek::Regular, 3, 6),
            (Et::Cohesive1d2, 2, 1, 0, Ek::Cohesive, 1, 2),
            (Et::Cohesive2d4, 4, 2, 1, Ek::Cohesive, 2, 2),
            (Et::Cohesive2d6, 6, 2, 1, Ek::Cohesive, 3, 2),
            (Et::Cohesive3d12, 12, 3, 2, Ek::Cohesive, 3, 2),
            (Et::Cohesive3d16, 16, 3, 2, Ek::Cohesive, 3, 2),
            (Et::Cohesive3d6, 6, 3, 2, Ek::Cohesive, 2, 2),
            (Et::Cohesive3d8, 8, 3, 2, Ek::Cohesive, 2, 2),
            (Et::BernoulliBeam2, 2, 2, 1, Ek::Structural, 3, 2),
            (Et::BernoulliBeam3, 2, 3, 1, Ek::Structural, 3, 2),
            (
                Et::DiscreteKirchhoffTriangle18,
                3,
                3,
                2,
                Ek::Structural,
                2,
                3,
            ),
            (Et::IgfemSegment3, 3, 1, 1, Ek::Igfem, 1, 2),
            (Et::IgfemTriangle4, 4, 2, 2, Ek::Igfem, 1, 3),
            (Et::IgfemTriangle5, 5, 2, 2, Ek::Igfem, 1, 3),
        ];
        assert_eq!(expected.len(), ElementType::ALL.len());
        for (t, nn, sd, nd, k, pd, nf) in expected {
            assert_eq!(t.nb_nodes_per_element(), nn, "{t} nodes");
            assert_eq!(t.spatial_dimension(), sd, "{t} spatial dim");
            assert_eq!(t.natural_space_dimension(), nd, "{t} natural dim");
            assert_eq!(t.kind(), k, "{t} kind");
            assert_eq!(t.polynomial_degree(), pd, "{t} degree");
            assert_eq!(t.nb_facets_per_element(), nf, "{t} facets");
        }
        assert_eq!(Et::NotDefined.nb_nodes_per_element(), 0);
        assert_eq!(Et::NotDefined.nb_facets_per_element(), 0);
        // Structural DOF data.
        let s = Et::BernoulliBeam3
            .interpolation_type()
            .info()
            .structural
            .unwrap();
        assert_eq!(
            (
                s.nb_degree_of_freedom,
                s.nb_stress_components,
                s.dnds_columns
            ),
            (6, 4, 6)
        );
        let s = Et::DiscreteKirchhoffTriangle18
            .interpolation_type()
            .info()
            .structural
            .unwrap();
        assert_eq!(
            (
                s.nb_degree_of_freedom,
                s.nb_stress_components,
                s.dnds_columns
            ),
            (6, 6, 21)
        );
        assert_eq!(
            Et::DiscreteKirchhoffTriangle18.nb_nodes_per_interpolation_element(),
            3
        );
        // Facet lookup.
        assert_eq!(Et::Pentahedron15.facet_type(1), Some(Et::Quadrangle8));
        assert_eq!(Et::BernoulliBeam2.facet_type(0), None);
        assert_eq!(Et::Triangle6.cohesive_type(), Some(Et::Cohesive3d12));
        assert_eq!(Et::Tetrahedron4.cohesive_type(), None);
        assert_eq!(Et::Hexahedron20.p1_type(), Some(Et::Hexahedron8));
        assert_eq!(
            Et::IgfemTriangle5.info().igfem_sub_element_types,
            Some([Et::Triangle3, Et::Quadrangle4])
        );
    }

    #[test]
    fn decoded_facets_match_upstream_orientation() {
        let tri = Et::Triangle3.facet_connectivity(0).unwrap();
        let f: Vec<Vec<usize>> = (0..3).map(|i| tri.facet(i).collect()).collect();
        assert_eq!(f, vec![vec![0, 1], vec![1, 2], vec![2, 0]]);
        let tet = Et::Tetrahedron4.facet_connectivity(0).unwrap();
        let f: Vec<Vec<usize>> = (0..4).map(|i| tet.facet(i).collect()).collect();
        assert_eq!(
            f,
            vec![vec![0, 2, 1], vec![1, 2, 3], vec![2, 0, 3], vec![0, 1, 3]]
        );
        let hex = Et::Hexahedron8.facet_connectivity(0).unwrap();
        assert_eq!(hex.facet(0).collect::<Vec<_>>(), vec![0, 3, 2, 1]);
        assert_eq!(hex.facet(5).collect::<Vec<_>>(), vec![4, 5, 6, 7]);
        let coh = Et::Cohesive2d4.facet_connectivity(0).unwrap();
        assert_eq!(coh.facet(1).collect::<Vec<_>>(), vec![2, 3]);
    }

    /// Structural consistency of all facet tables.
    #[test]
    fn facet_tables_are_consistent() {
        for t in ElementType::ALL {
            let g = t.geometrical_type().info();
            let mut seen_nodes = BTreeSet::new();
            for (ft, conn) in g.facets.iter().enumerate() {
                assert_eq!(
                    conn.data.len(),
                    conn.nb_facets * conn.nb_nodes_per_facet,
                    "{t}"
                );
                if let Some(fty) = t.facet_type(ft) {
                    assert_eq!(
                        fty.nb_nodes_per_element(),
                        conn.nb_nodes_per_facet,
                        "{t} facet {ft}"
                    );
                    if t.kind() == Ek::Regular && t != Et::Point1 {
                        assert_eq!(fty.spatial_dimension() + 1, t.spatial_dimension(), "{t}");
                    }
                }
                for f in 0..conn.nb_facets {
                    let nodes: BTreeSet<usize> = conn.facet(f).collect();
                    assert_eq!(
                        nodes.len(),
                        conn.nb_nodes_per_facet,
                        "{t} facet {f} repeats a node"
                    );
                    assert!(nodes.iter().all(|&n| n < g.nb_nodes_per_element), "{t}");
                    seen_nodes.extend(nodes);
                }
            }
            // Point facets (1D elements) cannot contain interior nodes.
            if g.facets.iter().all(|c| c.nb_nodes_per_facet > 1) {
                assert_eq!(
                    seen_nodes.len(),
                    g.nb_nodes_per_element,
                    "{t}: facets must cover all nodes"
                );
            }
            if let Some(ft) = t.facet_types() {
                assert_eq!(ft.len(), g.facets.len(), "{t}");
            }
        }
    }

    /// For linear 3D solids, every facet edge must be shared by exactly two
    /// facets (closed surface) and traversed in opposite directions
    /// (consistent orientation).
    #[test]
    fn linear_solid_facets_form_oriented_closed_surfaces() {
        for t in [Et::Tetrahedron4, Et::Hexahedron8, Et::Pentahedron6] {
            let mut edges: BTreeMap<(usize, usize), i32> = BTreeMap::new();
            for conn in t.geometrical_type().info().facets {
                for f in 0..conn.nb_facets {
                    let nodes: Vec<usize> = conn.facet(f).collect();
                    for i in 0..nodes.len() {
                        let (a, b) = (nodes[i], nodes[(i + 1) % nodes.len()]);
                        *edges.entry((a.min(b), a.max(b))).or_default() +=
                            if a < b { 1 } else { -1 };
                    }
                }
            }
            assert!(
                edges.values().all(|&c| c == 0),
                "{t}: inconsistent facet orientation"
            );
            let count: BTreeMap<(usize, usize), usize> = t
                .geometrical_type()
                .info()
                .facets
                .iter()
                .flat_map(|conn| {
                    (0..conn.nb_facets).flat_map(move |f| {
                        let n: Vec<usize> = conn.facet(f).collect();
                        (0..n.len()).map(move |i| {
                            let (a, b) = (n[i], n[(i + 1) % n.len()]);
                            (a.min(b), a.max(b))
                        })
                    })
                })
                .fold(BTreeMap::new(), |mut m, e| {
                    *m.entry(e).or_default() += 1;
                    m
                });
            assert!(count.values().all(|&c| c == 2), "{t}: open surface");
        }
    }

    #[test]
    fn igfem_tables_are_consistent() {
        for itp in [Itp::IgfemSegment3, Itp::IgfemTriangle4, Itp::IgfemTriangle5] {
            let info = itp.info();
            let ig = info.igfem.unwrap();
            assert_eq!(
                ig.sub_element_connectivity.len(),
                ig.sub_element_nb_nodes[0] + ig.sub_element_nb_nodes[1]
            );
            for (s, sub) in ig.sub_types.iter().enumerate() {
                assert_eq!(sub.info().nb_nodes_per_element, ig.sub_element_nb_nodes[s]);
            }
            assert_eq!(ig.enrichments.len(), 2 * ig.nb_enrichments);
            assert!(
                ig.sub_element_connectivity
                    .iter()
                    .all(|&n| (n as usize) < info.nb_nodes_per_element)
            );
        }
    }

    #[test]
    fn kinds_partition_types() {
        let total: usize = ElementKind::ALL
            .iter()
            .map(|&k| ElementType::of_kind(k).count())
            .sum();
        assert_eq!(total, ElementType::ALL.len());
        assert_eq!(ElementType::of_kind(Ek::Regular).count(), 13);
        assert_eq!(ElementType::of_kind(Ek::Cohesive).count(), 7);
        assert_eq!(ElementType::of_kind(Ek::Structural).count(), 3);
        assert_eq!(ElementType::of_kind(Ek::Igfem).count(), 3);
    }
}
