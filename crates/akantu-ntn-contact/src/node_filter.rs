// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/functions/node_filter.hh.

//! Geometric node filters of the package (upstream `GeometryFilter`,
//! `FilterPositionsGreaterThan`, `FilterPositionsLessThan`; TASN-011).
//!
//! The return values are upstream's: `FilterPositionsGreaterThan` returns
//! `false` for nodes whose coordinate is **greater** than the limit and
//! `true` otherwise (upstream's comment notes that the filter convention
//! changed to "true → keep node" after these were written, so under the
//! current convention they keep the complementary set; behaviour preserved,
//! UD-404, category C). The base `GeometryFilter::operator()` is
//! `AKANTU_TO_IMPLEMENT()` upstream ([`GeometryFilter::call`] returns
//! `NotImplemented`).

use std::cmp::Ordering;

use akantu_core::{Error, Idx, Real, Result};
use akantu_mesh::Mesh;

/// Upstream `GeometryFilter(mesh, dir, limit)` (reference positions of the
/// mesh nodes).
#[derive(Clone, Copy, Debug)]
pub struct GeometryFilter<'a> {
    mesh: &'a Mesh,
    dir: usize,
    limit: Real,
}

impl<'a> GeometryFilter<'a> {
    /// New filter.
    ///
    /// # Errors
    /// `dir` is not a direction of the mesh.
    pub fn new(mesh: &'a Mesh, dir: usize, limit: Real) -> Result<Self> {
        if dir >= mesh.spatial_dimension() {
            return Err(Error::OutOfRange {
                what: "direction",
                index: dir,
                len: mesh.spatial_dimension(),
            });
        }
        Ok(Self { mesh, dir, limit })
    }

    /// Upstream base `operator()(node)`: not implemented.
    ///
    /// # Errors
    /// Always `NotImplemented`.
    pub fn call(&self, _node: Idx) -> Result<bool> {
        Err(Error::NotImplemented(
            "GeometryFilter::operator() (AKANTU_TO_IMPLEMENT upstream)",
        ))
    }

    fn coordinate(&self, node: Idx) -> Result<Real> {
        let n = usize::try_from(node).map_err(|_| Error::InvalidIndex {
            what: "node",
            value: i64::from(node),
        })?;
        Ok(self.mesh.nodes().get_tuple(n)?[self.dir])
    }
}

/// Upstream `FilterPositionsGreaterThan`.
#[derive(Clone, Copy, Debug)]
pub struct FilterPositionsGreaterThan<'a>(pub GeometryFilter<'a>);

impl FilterPositionsGreaterThan<'_> {
    /// Upstream `operator()(node)`: `false` if the coordinate is greater than
    /// the limit, `true` otherwise.
    ///
    /// # Errors
    /// Invalid node.
    pub fn call(&self, node: Idx) -> Result<bool> {
        // upstream `!(x > limit)`: also true when x or the limit is NaN
        let c = self.0.coordinate(node)?.partial_cmp(&self.0.limit);
        Ok(c != Some(Ordering::Greater))
    }
}

/// Upstream `FilterPositionsLessThan`.
#[derive(Clone, Copy, Debug)]
pub struct FilterPositionsLessThan<'a>(pub GeometryFilter<'a>);

impl FilterPositionsLessThan<'_> {
    /// Upstream `operator()(node)`: `false` if the coordinate is less than
    /// the limit, `true` otherwise.
    ///
    /// # Errors
    /// Invalid node.
    pub fn call(&self, node: Idx) -> Result<bool> {
        // upstream `!(x < limit)`: also true when x or the limit is NaN
        let c = self.0.coordinate(node)?.partial_cmp(&self.0.limit);
        Ok(c != Some(Ordering::Less))
    }
}
