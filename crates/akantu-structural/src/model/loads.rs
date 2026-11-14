// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.cc (computeForcesByLocalTractionArray,
// computeForcesByGlobalTractionArray), structural_mechanics_model_boundary.cc
// (empty upstream).

//! Line loads given as traction arrays (STRUCT-012).
//!
//! A traction array holds `nb_degree_of_freedom` values (distributed
//! forces, then distributed moments) per quadrature point of every element
//! of one structural type. The equivalent nodal loads `∫ (N·T)ᵀ t` are
//! **added** to `external_force`. Upstream's `computeForcesFromFunction`
//! is dead code (`#if 0`) and is not ported.

use akantu_core::{Array, ElementType, Error, GhostType, Real, Result};
use akantu_solver::DofManager;

use super::{StructuralMechanicsModel, check_nodal, not_allocated};
use crate::material::nb_degree_of_freedom;

const NG: GhostType = GhostType::NotGhost;

impl StructuralMechanicsModel {
    /// Upstream `computeForcesByLocalTractionArray(tractions, type)`:
    /// `f_ext += ∫ (N·T)ᵀ t` with `t` given in the **local** element basis
    /// (`nb_element · nb_quad` tuples of `nb_degree_of_freedom` values).
    ///
    /// # Errors
    /// Upstream only debug-asserts the sizes; the port checks the type
    /// (structural, of the model dimension), the number of tuples and of
    /// components; stale FE data; the DKT has no shape functions upstream
    /// (UD-053, `Error::NotImplemented`). `external_force` is unchanged on
    /// error.
    pub fn compute_forces_by_local_traction_array(
        &mut self,
        tractions: &Array<Real>,
        ty: ElementType,
    ) -> Result<()> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let p = &mut self.physics;
        let ndof = p.nb_degree_of_freedom;
        if ty.spatial_dimension() != p.fe.element_dimension() || nb_degree_of_freedom(ty)? != ndof {
            return Err(Error::invalid(format!(
                "element type dimension does not match the dimension of boundaries : {} != {}",
                p.fe.element_dimension(),
                ty.spatial_dimension()
            )));
        }
        let nq = p.fe.nb_integration_points(ty, NG)?;
        let ne = mesh.nb_element(ty, NG);
        if tractions.len() != nq * ne {
            return Err(Error::DimensionMismatch {
                context: "the size of the vector should be the total number of quadrature points",
                expected: nq * ne,
                got: tractions.len(),
            });
        }
        if tractions.nb_component() != ndof {
            return Err(Error::DimensionMismatch {
                context: "the number of components should be the number of degrees of freedom \
                          per node",
                expected: ndof,
                got: tractions.nb_component(),
            });
        }
        let mut int_funct = Array::new(1);
        p.fe.line_load_local(&mesh, tractions.view(), &mut int_funct, ty, NG)?;
        let mut f = p
            .external_force
            .as_mut()
            .ok_or_else(|| not_allocated("external_force"))?
            .write()?;
        check_nodal(
            "computeForcesByLocalTractionArray: external_force",
            &f,
            mesh.nb_nodes(),
            ndof,
        )?;
        DofManager::assemble_elemental_array_local_view(
            &mesh,
            int_funct.view(),
            f.view_mut(),
            ty,
            NG,
            1.0,
            None,
        )
    }

    /// Upstream `computeForcesByGlobalTractionArray(traction_global, type)`:
    /// every quadrature value is rotated to the element basis (`t = R·T`,
    /// `R` the element rotation) and the local version is applied.
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::compute_forces_by_local_traction_array`].
    pub fn compute_forces_by_global_traction_array(
        &mut self,
        traction_global: &Array<Real>,
        ty: ElementType,
    ) -> Result<()> {
        let local = {
            let mesh_h = self.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let p = &self.physics;
            if ty.spatial_dimension() != p.fe.element_dimension()
                || nb_degree_of_freedom(ty)? != p.nb_degree_of_freedom
            {
                return Err(Error::invalid(format!(
                    "{} is not a structural type of this {}-D model",
                    ty.name(),
                    p.base.spatial_dimension
                )));
            }
            let mut local = Array::new(p.nb_degree_of_freedom);
            p.fe.tractions_to_local(&mesh, traction_global.view(), &mut local, ty, NG)?;
            local
        };
        self.compute_forces_by_local_traction_array(&local, ty)
    }
}
