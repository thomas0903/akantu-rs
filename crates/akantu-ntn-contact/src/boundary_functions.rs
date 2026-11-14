// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/functions/{boundary_functions.hh, boundary_functions.cc}.

//! Boundary utility functions of the package (TASN-011).
//!
//! **UD-398 (B):** upstream `boundaryFix` ("a fix so that all subboundaries
//! exist on all procs") creates the group when it **already exists**
//! (`if (mesh.elementGroupExists(eg)) createElementGroup(...)`), which throws
//! "already exists", and never creates a missing one. The port creates the
//! missing groups (empty, dimension `dim − 1`); the compat switch
//! [`crate::TasnUpstreamCompat::boundary_fix_inverted`] reproduces the
//! inverted test (error on an existing group, nothing otherwise).

use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;
use akantu_solid::SolidMechanicsModel;

/// Upstream `integrateResidual(sub_boundary, model, dir)`: the internal force
/// (upstream's "residual" here is `getInternalForce()`) along `dir` summed
/// over the local or master nodes of the group (serial: no reduction).
///
/// # Errors
/// Unknown group, `dir` out of range, internal force not allocated (agreed
/// on every rank of a distributed mesh).
pub fn integrate_residual(
    sub_boundary_name: &str,
    model: &SolidMechanicsModel,
    dir: usize,
) -> Result<Real> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = mesh_h.read()?;
    let local = (|| {
        let residual = model.internal_force()?;
        let dim = residual.nb_component();
        if dir >= dim {
            return Err(Error::OutOfRange {
                what: "direction",
                index: dir,
                len: dim,
            });
        }
        let boundary = mesh.groups().element_group(sub_boundary_name)?;
        let mut int_res = 0.0;
        for &node in boundary.nodes().as_slice() {
            if mesh.is_local_or_master_node(node)? {
                let n = usize::try_from(node).map_err(|_| Error::InvalidIndex {
                    what: "node",
                    value: i64::from(node),
                })?;
                int_res += residual.get_tuple(n)?[dir];
            }
        }
        Ok(int_res)
    })();
    // upstream `allReduce(int_res, _sum)` (collective on a distributed mesh;
    // a rank-local error is agreed first)
    let mut v = [*local.as_ref().unwrap_or(&0.0)];
    crate::parallel::sum_reals(&mesh, local.map(|_| ()), &mut v, "integrate_residual")?;
    Ok(v[0])
}

/// Upstream `boundaryFix(mesh, sub_boundary_names)` (UD-398, see the module
/// docs; `upstream_inverted` = compat switch).
///
/// # Errors
/// Group creation failure, or (compat) a group that already exists.
pub fn boundary_fix(
    mesh: &mut Mesh,
    sub_boundary_names: &[&str],
    upstream_inverted: bool,
) -> Result<()> {
    let dim = mesh.spatial_dimension();
    for &eg in sub_boundary_names {
        let exists = mesh.groups().element_group_exists(eg);
        if upstream_inverted {
            if exists {
                return Err(Error::AlreadyExists {
                    kind: "element group",
                    id: eg.into(),
                });
            }
        } else if !exists {
            mesh.groups_mut()
                .create_element_group(eg, dim.saturating_sub(1))?;
        }
    }
    Ok(())
}
