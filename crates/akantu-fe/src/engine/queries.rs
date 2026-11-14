// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/fe_engine_template_tmpl.hh
// (inverseMap, contains, computeShapesImpl, computeShapeDerivativesImpl,
// interpolateImpl), shape_lagrange_inline_impl.hh (inverseMap, contains,
// interpolate, computeShapes, computeShapeDerivatives),
// fe_engine_inline_impl.hh (getElementInradius), shape_lagrange_base_inline_impl.hh.

//! Point queries (FE-005, FE-009, FE-010, FE-024).
//!
//! Upstream evaluates these on single elements with dynamic matrices: the
//! element's node coordinates come from `Mesh::extractNodalValuesFromElement`
//! (`Matrix<Real>`), the natural coordinates are a dynamic vector of the
//! **mesh** dimension. Inverse maps therefore only make sense for elements
//! whose natural dimension equals the mesh dimension (otherwise upstream's
//! `natural_coords += dxi` mixes sizes); the port rejects other cases.

use akantu_core::array::ArrayView;
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementKind, ElementType, Error, Real, Result};
use akantu_mesh::Mesh;

use super::{Block, FeEngine, prepare_out};
use crate::eigen::Dims;
use crate::element_class::{
    ElementClass, INVERSE_MAP_MAX_ITERATIONS, INVERSE_MAP_TOLERANCE, LagrangeElement,
};
use crate::kernels;
use crate::with_element_class;

impl FeEngine {
    /// Element block and index, validated.
    fn element_block<'m>(
        &self,
        mesh: &'m Mesh,
        element: Element,
        context: &'static str,
    ) -> Result<(Block<'m>, usize)> {
        let e = element
            .try_idx()
            .ok_or_else(|| Error::invalid(format!("{context}: null element")))?;
        self.check_type(element.ty(), context)?;
        let block = Block::new(mesh, element.ty(), element.ghost_type())?;
        if e >= block.nb_element {
            return Err(Error::OutOfRange {
                what: "element",
                index: e,
                len: block.nb_element,
            });
        }
        Ok((block, e))
    }

    /// Natural coordinates of the real point `real` in `element` (upstream
    /// `FEEngineTemplate::inverseMap`, defaults
    /// [`INVERSE_MAP_MAX_ITERATIONS`]/[`INVERSE_MAP_TOLERANCE`]); `natural`
    /// has the natural dimension of the element (= mesh dimension).
    pub fn inverse_map(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        natural: &mut [Real],
    ) -> Result<()> {
        self.inverse_map_with(
            mesh,
            real,
            element,
            natural,
            INVERSE_MAP_MAX_ITERATIONS,
            INVERSE_MAP_TOLERANCE,
        )
    }

    /// [`FeEngine::inverse_map`] with explicit iteration limit and tolerance.
    pub fn inverse_map_with(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        natural: &mut [Real],
        max_iterations: usize,
        tolerance: Real,
    ) -> Result<()> {
        if self.kind == ElementKind::Cohesive {
            return Err(Error::NotImplemented(
                "inverseMap for cohesive elements (AKANTU_TO_IMPLEMENT upstream)",
            ));
        }
        let (block, e) = self.element_block(mesh, element, "inverse_map")?;
        let sdim = block.sdim;
        if real.len() != sdim {
            return Err(Error::DimensionMismatch {
                context: "inverse_map: real coordinates",
                expected: sdim,
                got: real.len(),
            });
        }
        let ty = element.ty();
        if ty.natural_space_dimension() != sdim {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "inverse_map (natural dimension must equal the mesh dimension)",
            });
        }
        let mut x = vec![0.0; sdim * block.nnpe];
        block.coords(e, &mut x)?;
        with_element_class!(ty, E => {
            E::inverse_map(real, &x, natural, max_iterations, tolerance)
        }, else Err(Error::UnsupportedElementType { ty, context: "inverse_map" }))
    }

    /// `true` if the real point lies in `element` (upstream `contains`:
    /// inverse map, then the shape-specific `contains` tolerance).
    pub fn contains(&self, mesh: &Mesh, real: &[Real], element: Element) -> Result<bool> {
        let ty = element.ty();
        let mut natural = vec![0.0; ty.natural_space_dimension()];
        self.inverse_map(mesh, real, element, &mut natural)?;
        Ok(with_element_class!(ty, E => E::contains(&natural), else false))
    }

    /// Shape functions of `element` at the real point `real` (upstream
    /// `computeShapes(real_coords, elem, type, shapes, ghost)`). `shapes`
    /// must hold exactly `nb_nodes_per_interpolation_element` values
    /// (`Error::DimensionMismatch` otherwise, checked before any work).
    pub fn compute_shapes_at(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        shapes: &mut [Real],
    ) -> Result<()> {
        let ty = element.ty();
        check_out_len(
            "compute_shapes_at: shapes",
            ty.nb_nodes_per_interpolation_element(),
            shapes.len(),
        )?;
        let mut natural = vec![0.0; ty.natural_space_dimension()];
        self.inverse_map(mesh, real, element, &mut natural)?;
        with_element_class!(ty, E => {
            E::compute_shapes(&natural, shapes);
            Ok(())
        }, else Err(Error::UnsupportedElementType { ty, context: "compute_shapes_at" }))
    }

    /// Physical shape derivatives of `element` at the real point `real`
    /// (upstream `computeShapeDerivatives(real_coords, element, type,
    /// shape_derivatives, ghost)`, used by the reinforcement material):
    /// column-major `sdim × nn`, same kernels and operand types as the
    /// precomputation (dynamic `dnds`, `J`, LU inverse). `out` must hold
    /// exactly `spatial_dimension · nb_nodes_per_interpolation_element`
    /// values (`Error::DimensionMismatch` otherwise, checked before any work).
    pub fn compute_shape_derivatives_at(
        &self,
        mesh: &Mesh,
        real: &[Real],
        element: Element,
        out: &mut [Real],
    ) -> Result<()> {
        let ty = element.ty();
        let nd = ty.natural_space_dimension();
        let (block, e) = self.element_block(mesh, element, "compute_shape_derivatives_at")?;
        let sdim = block.sdim;
        check_out_len(
            "compute_shape_derivatives_at: derivatives",
            sdim * ty.nb_nodes_per_interpolation_element(),
            out.len(),
        )?;
        let mut natural = vec![0.0; nd];
        self.inverse_map(mesh, real, element, &mut natural)?;
        let mut x = vec![0.0; sdim * block.nnpe];
        block.coords(e, &mut x)?;
        with_element_class!(ty, E => {
            let nn = E::NB_SHAPES;
            let mut dnds = vec![0.0; nd * nn];
            E::compute_dnds(&natural, &mut dnds);
            let mut j = vec![0.0; nd * sdim];
            kernels::jmat(&dnds, Dims::dynamic(nd, nn), &x, Dims::dynamic(sdim, nn), &mut j)?;
            kernels::shape_derivatives(&j, &dnds, Dims::dynamic(nd, nn), out)?;
            Ok(())
        }, else Err(Error::UnsupportedElementType { ty, context: "compute_shape_derivatives_at" }))
    }

    /// Shape functions (and optionally natural derivatives) of `ty` at
    /// arbitrary natural points (columns of `natural`, upstream
    /// `computeShapesOnIntegrationPoints` with custom points): one tuple of
    /// `nn` (resp. `nd·nn`, column-major) per point.
    pub fn compute_shapes_at_natural(
        &self,
        ty: ElementType,
        natural: &MatrixX,
        shapes: &mut Array<Real>,
        dnds: Option<&mut Array<Real>>,
    ) -> Result<()> {
        compute_shapes_at_natural(ty, natural, shapes, dnds)
    }

    /// Interpolate the nodal field `nodal` at the real point `real` of
    /// `element` (upstream `FEEngine::interpolate`; upstream takes the
    /// element's `nb_dof × nn` nodal values, the port extracts them from the
    /// global nodal array, which must have one tuple per mesh node). `out`
    /// must hold exactly `nb_dof` values (`Error::DimensionMismatch`
    /// otherwise, checked before any work).
    pub fn interpolate(
        &self,
        mesh: &Mesh,
        real: &[Real],
        nodal: ArrayView<'_, Real>,
        element: Element,
        out: &mut [Real],
    ) -> Result<()> {
        let ty = element.ty();
        let nb_dof = nodal.nb_component();
        check_out_len("interpolate: result", nb_dof, out.len())?;
        super::check_input(
            "interpolate: nodal field",
            nodal.len(),
            nb_dof,
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let nn = ty.nb_nodes_per_interpolation_element();
        let mut n = vec![0.0; nn];
        self.compute_shapes_at(mesh, real, element, &mut n)?;
        let (block, e) = self.element_block(mesh, element, "interpolate")?;
        let mut ue = vec![0.0; nb_dof * block.nnpe];
        block.gather(e, nodal.as_slice(), nb_dof, &mut ue)?;
        // nodal (Ref<const MatrixXr>) · shapes.col(0) (Vector<Real, nn>)
        kernels::interpolate(&ue, Dims::dynamic(nb_dof, nn), &n, Dims::fixed(nn, 1), out)?;
        Ok(())
    }

    /// Element characteristic length (upstream `getElementInradius`, dynamic
    /// coordinates matrix); `0` for cohesive and point elements.
    pub fn element_inradius(&self, mesh: &Mesh, element: Element) -> Result<Real> {
        let e = element
            .try_idx()
            .ok_or_else(|| Error::invalid("element_inradius: null element"))?;
        let block = Block::new(mesh, element.ty(), element.ghost_type())?;
        if e >= block.nb_element {
            return Err(Error::OutOfRange {
                what: "element",
                index: e,
                len: block.nb_element,
            });
        }
        let mut x = vec![0.0; block.sdim * block.nnpe];
        block.coords(e, &mut x)?;
        Ok(element_inradius_of(element.ty(), &x))
    }
}

/// Exact length of a caller-sized output buffer (never slice an
/// unchecked caller buffer in a `Result`-returning API).
fn check_out_len(context: &'static str, expected: usize, got: usize) -> Result<()> {
    if expected != got {
        return Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        });
    }
    Ok(())
}

/// Upstream static `FEEngine::getElementInradius(coord, type)` (any element
/// type with a geometry; column-major `sdim × nb_nodes` coordinates).
#[must_use]
pub fn element_inradius_of(ty: ElementType, coords: &[Real]) -> Real {
    crate::geometry::inradius(ty.geometrical_type(), coords, ty.nb_nodes_per_element())
}

/// Shapes / natural derivatives of `ty` at the columns of `natural`.
pub fn compute_shapes_at_natural(
    ty: ElementType,
    natural: &MatrixX,
    shapes: &mut Array<Real>,
    dnds: Option<&mut Array<Real>>,
) -> Result<()> {
    with_element_class!(ty, E => {
        let nd = E::NATURAL_DIM;
        let nn = E::NB_SHAPES;
        let np = natural.cols();
        if natural.rows() < nd {
            return Err(Error::DimensionMismatch {
                context: "compute_shapes_at_natural: rows of the natural points",
                expected: nd,
                got: natural.rows(),
            });
        }
        let rows = natural.rows();
        let pts = natural.as_slice();
        prepare_out(shapes, np, nn)?;
        for p in 0..np {
            E::compute_shapes(&pts[p * rows..p * rows + nd], &mut shapes.as_mut_slice()[p * nn..(p + 1) * nn]);
        }
        if let Some(d) = dnds {
            prepare_out(d, np, nd * nn)?;
            for p in 0..np {
                E::compute_dnds(&pts[p * rows..p * rows + nd], &mut d.as_mut_slice()[p * nd * nn..(p + 1) * nd * nn]);
            }
        }
        Ok(())
    }, else Err(Error::UnsupportedElementType { ty, context: "compute_shapes_at_natural" }))
}
