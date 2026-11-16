// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.cc
// (assembleResidual, assembleInternalFlow, assembleDiffisivityMatrix,
// assembleCapacityMatrix, assembleCapacityMatrixLumped, ComputeRhoFunctor),
// src/model/diffusion_model/diffusion_law.cc (assembleInternalFlow,
// assembleDiffusivityMatrix), src/fe_engine/fe_engine_template_tmpl_field.hh
// (assembleFieldLumped, assembleFieldMatrix: DOF-manager part).

//! Model-side assembly (DIFF-004…006).
//!
//! Every routine follows the upstream statement order and calls the FE
//! engine and DOF-manager kernels with the upstream operands, so that the
//! internal flow, `K`, the lumped and the consistent capacity are bitwise
//! those of the reference with the `eigen` backend (D-008).

use akantu_core::array_shared::{ArrayMut, SharedArray};
use akantu_core::enums::{MatrixType, SynchronizationTag};
use akantu_core::{Array, Element, ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::LawPhase;
use akantu_model::model::{
    ReduceOperation, alloc_nodal_field, make_consistent_for_periodicity_values_in,
    periodicity_is_distributed,
};
use akantu_solver::DofManager;

use super::{DiffusionPhysics, check_nodal, not_allocated};
use crate::law::DiffusionLaw;

/// Elemental matrices of one element type: `(type, filter, K_e)`.
pub(crate) type ElementalBlock = (ElementType, Vec<akantu_core::Idx>, Array<Real>);

/// Upstream `diffusion::details::ComputeRhoFunctor`: every entry of the
/// element's `nb_dof × nq` matrix is the law's `getRho()`.
fn rho_of(
    laws: &ConstitutiveLawsHandler<dyn DiffusionLaw>,
    mat: &mut [Real],
    element: Element,
) -> Result<()> {
    let law_index = laws.law_index_map().element_ref(element)?[0];
    let law_index = akantu_core::decode_idx(law_index)?.ok_or_else(|| {
        Error::invalid(format!(
            "{element} has no constitutive law (capacity assembly)"
        ))
    })?;
    let rho = laws.law(law_index)?.rho();
    mat.fill(rho);
    Ok(())
}

/// Element types of the model dimension (regular kind) of `ghost`.
fn regular_types(mesh: &Mesh, dim: usize, ghost: GhostType) -> Vec<ElementType> {
    mesh.element_types(akantu_core::TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: ghost,
        kind: Some(ElementKind::Regular),
    })
    .collect()
}

impl DiffusionPhysics {
    /// Validate the user-resizable nodal arrays (no partial update).
    fn check_arrays(&self, mesh: &Mesh, dofs: &DofManager) -> Result<()> {
        let nn = mesh.nb_nodes();
        let u = dofs
            .dofs(&self.dof_name)
            .map_err(|_| not_allocated(&self.dof_name))?;
        check_nodal("diffusion: diffusion field", &u, nn, 1)?;
        let fe = self
            .external_flow
            .as_ref()
            .ok_or_else(|| not_allocated("external_flow"))?;
        check_nodal("diffusion: external_flow", fe, nn, 1)?;
        let fi = self
            .internal_flow
            .as_ref()
            .ok_or_else(|| not_allocated("internal_flow"))?;
        check_nodal("diffusion: internal_flow", fi, nn, 1)
    }

    /// Upstream `assembleResidual()` (DIFF-004): the internal flow, then
    /// residual `+= external_flow` and `+= internal_flow` (both with
    /// coefficient `+1`: the internal flow carries its sign).
    pub(crate) fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.assemble_internal_flow(dofs)?;
        let name = self.dof_name.clone();
        let fe = self
            .external_flow
            .as_ref()
            .ok_or_else(|| not_allocated("external_flow"))?;
        dofs.assemble_to_residual(&name, fe.read()?.view(), 1.0)?;
        let fi = self
            .internal_flow
            .as_ref()
            .ok_or_else(|| not_allocated("internal_flow"))?;
        dofs.assemble_to_residual(&name, fi.read()?.view(), 1.0)
    }

    /// Upstream `assembleInternalFlow()`: zero the internal flow;
    /// `synchronize(_diffusion)` (owners' nodal values onto the ghost
    /// elements' nodes, through the running operation's scope); then for
    /// each ghost type (not-ghost first): every law's
    /// `computeDiffusivityGradU`, then every law's `assembleInternalFlow`
    /// (`−∫ Bᵀ D∇u`).
    pub(crate) fn assemble_internal_flow(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.check_arrays(&mesh, dofs)?;
        if let Some(f) = self.internal_flow.as_mut() {
            f.write()?.zero();
        }
        if let Some(es) = &mesh.synchronizers().element {
            let fe = self.base.fems.get("")?;
            let nb_quad = super::parallel::nb_quad_map(fe, &mesh, self.base.spatial_dimension)?;
            let compat = self.upstream_compat;
            let name = self.dof_name.clone();
            let (u, laws) = {
                let u = dofs.family_mut(&name).map_err(|_| not_allocated(&name))?;
                // the exchange writes into the DOF array: take it out of the
                // manager for the duration of the exchange (`phases()`
                // borrows the manager)
                // (moving the `SharedArray` keeps its allocation and any
                // export)
                (
                    std::mem::replace(&mut u.dofs, SharedArray::new(Array::new(1))),
                    &mut self.laws,
                )
            };
            let mut acc = super::parallel::ElementAccessor {
                mesh: &mesh,
                laws,
                nb_quad: &nb_quad,
                diffusion: u,
                compat,
            };
            // no early return before the array is back in the manager
            let r = match dofs.phases() {
                Ok(mut ph) => es.synchronize_in(&mut ph, &mut acc, SynchronizationTag::Diffusion),
                Err(e) => Err(e),
            };
            let u = acc.diffusion;
            dofs.family_mut(&name)
                .map_err(|_| not_allocated(&name))?
                .dofs = u;
            r?;
        }
        for ghost in GhostType::ALL {
            self.with_laws(&mesh, dofs, LawPhase::Stress, |laws, ctx| {
                for law in laws.laws_mut() {
                    law.compute_diffusivity_grad_u(ctx, ghost)?;
                }
                Ok(())
            })?;
            let fe = self.base.fems.get("")?;
            // one write guard for the ghost type's assembly, scoped by the
            // loop body (D-071)
            let mut f_int = self
                .internal_flow
                .as_mut()
                .ok_or_else(|| not_allocated("internal_flow"))?
                .write()?;
            for law in self.laws.laws() {
                assemble_internal_flow_of(
                    law.as_ref(),
                    fe,
                    &mesh,
                    ghost,
                    self.upstream_compat.empty_filter_returns,
                    &mut f_int,
                )?;
            }
            drop(f_int);
        }
        Ok(())
    }

    /// Upstream `assembleDiffisivityMatrix()` (DIFF-005): zero `K` (which
    /// must exist), then every law's not-ghost `∫ Bᵀ D B`.
    pub(crate) fn assemble_diffusivity_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !dofs.has_matrix("K") {
            return Err(Error::invalid("The K matrix has not been initialized yet."));
        }
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.check_arrays(&mesh, dofs)?;
        dofs.zero_matrix("K")?;
        let fe = self.base.fems.get("")?;
        for law in self.laws.laws() {
            let blocks = elemental_diffusivity_of(
                law.as_ref(),
                fe,
                self.upstream_compat.empty_filter_returns,
            )?;
            for (ty, filter, k_e) in blocks {
                dofs.assemble_elemental_matrices_to_matrix(
                    &mesh,
                    "K",
                    &self.dof_name,
                    k_e.view(),
                    ty,
                    GhostType::NotGhost,
                    MatrixType::Symmetric,
                    Some(&filter),
                )?;
            }
        }
        Ok(())
    }

    /// Upstream `assembleCapacityMatrixLumped()` (DIFF-006): create the
    /// lumped `M` if missing, zero it; lump `ρc` element type by element
    /// type over both ghost types (ghost elements included: the capacity of
    /// shared nodes is complete on every rank); periodic consistency (sum);
    /// assemble into the lumped `M`.
    pub(crate) fn assemble_capacity_matrix_lumped(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !dofs.has_lumped_matrix("M") {
            dofs.new_lumped_matrix("M")?;
        }
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let dim = self.base.spatial_dimension;
        let mut capacity = None;
        let capacity = alloc_nodal_field(&mut capacity, mesh.nb_nodes(), 1)?;
        capacity.zero();
        dofs.zero_lumped_matrix("M")?;
        let fe = self.base.fems.get("")?;
        let laws = &self.laws;
        for ghost in GhostType::ALL {
            for ty in regular_types(&mesh, dim, ghost) {
                let mut functor = |m: &mut [Real], e: Element| -> Result<()> { rho_of(laws, m, e) };
                let lumped = fe.lumped_field_elemental(&mesh, &mut functor, 1, ty, ghost)?;
                DofManager::assemble_elemental_array_local_array(
                    &mesh,
                    lumped.view(),
                    capacity,
                    ty,
                    ghost,
                    1.0,
                    None,
                )?;
            }
        }
        {
            // Through the running operation's scope when distributed
            let mut scope = if periodicity_is_distributed(&mesh) {
                Some(dofs.phases()?)
            } else {
                None
            };
            let (len, nbc) = (capacity.len(), capacity.nb_component());
            make_consistent_for_periodicity_values_in(
                &mesh,
                scope.as_deref_mut(),
                capacity.as_mut_slice(),
                len,
                nbc,
                ReduceOperation::Sum,
                |a, b| a + b,
            )?;
        }
        dofs.assemble_to_lumped_matrix(&self.dof_name, capacity.view(), "M", 1.0)?;
        self.need_to_reassemble_capacity_lumped = false;
        Ok(())
    }

    /// Upstream `assembleCapacityMatrix()` (DIFF-006): zero `M`, then
    /// `∫ Nᵀ ρc N` per not-ghost element type (rule of order
    /// `2·polynomial degree`); assembled once (`need_to_reassemble_capacity`
    /// is checked by the caller, upstream `assembleMatrix("M")`).
    pub(crate) fn assemble_capacity_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        dofs.zero_matrix("M")?;
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let dim = self.base.spatial_dimension;
        let fe = self.base.fems.get("")?;
        let laws = &self.laws;
        for ty in regular_types(&mesh, dim, GhostType::NotGhost) {
            let mut functor = |m: &mut [Real], e: Element| -> Result<()> { rho_of(laws, m, e) };
            let m_e = fe.field_matrix_elemental(&mesh, &mut functor, 1, ty, GhostType::NotGhost)?;
            dofs.assemble_elemental_matrices_to_matrix(
                &mesh,
                "M",
                &self.dof_name,
                m_e.view(),
                ty,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                None,
            )?;
        }
        self.need_to_reassemble_capacity = false;
        Ok(())
    }
}

/// Upstream `DiffusionLaw::assembleInternalFlow(ghost)` for one law: per
/// type of its filter (model dimension, empty blocks included, enum order):
/// `computeBtD(D∇u)`, integrate over the filter, assemble with `−1`.
///
/// * An empty block is skipped; with `empty_filter_returns` the routine
///   returns instead, as upstream (UD-276).
/// * Upstream calls `computeBtD` **without** the element filter
///   (`diffusion_law.cc:63`): for a law that does not own every element of
///   the type in mesh order its zip runs over the whole type and reads /
///   writes past the law's arrays (UD-279, category A). The port passes the
///   filter (identical for a law owning every element of its types).
pub(crate) fn assemble_internal_flow_of(
    law: &dyn DiffusionLaw,
    fe: &FeEngine,
    mesh: &Mesh,
    ghost: GhostType,
    empty_filter_returns: bool,
    f_int: &mut ArrayMut<'_, Real>,
) -> Result<()> {
    let base = law.base();
    for ty in base.filter_types(ghost) {
        let filter = base.filter(ty, ghost).to_vec();
        if filter.is_empty() {
            if empty_filter_returns {
                return Ok(());
            }
            continue;
        }
        let nn = ty.nb_nodes_per_element();
        let d_grad_u = base.d_grad_u.get(ty, ghost)?;
        let mut bt_d_gu = Array::new(nn);
        fe.compute_btd(d_grad_u.view(), &mut bt_d_gu, ty, ghost, Some(&filter))?;
        let mut int_bt_d_gu = Array::new(nn);
        fe.integrate(
            bt_d_gu.view(),
            &mut int_bt_d_gu,
            nn,
            ty,
            ghost,
            Some(&filter),
        )?;
        DofManager::assemble_elemental_array_local_view(
            mesh,
            int_bt_d_gu.view(),
            f_int.view_mut(),
            ty,
            ghost,
            -1.0,
            Some(&filter),
        )?;
    }
    Ok(())
}

/// Elemental matrices `(type, filter, K_e)` of upstream
/// `DiffusionLaw::assembleDiffusivityMatrix()` up to (excluding) the
/// DOF-manager assembly: per not-ghost type of the filter,
/// `computeBtDB(diffusivity, order 2)`, integrate over the filter. Empty
/// blocks: as [`assemble_internal_flow_of`] (UD-276).
pub(crate) fn elemental_diffusivity_of(
    law: &dyn DiffusionLaw,
    fe: &FeEngine,
    empty_filter_returns: bool,
) -> Result<Vec<ElementalBlock>> {
    let base = law.base();
    let mut out = Vec::new();
    for ty in base.filter_types(GhostType::NotGhost) {
        let filter = base.filter(ty, GhostType::NotGhost).to_vec();
        if filter.is_empty() {
            if empty_filter_returns {
                return Ok(out);
            }
            continue;
        }
        let nn = ty.nb_nodes_per_element();
        let diffusivity = base.diffusivity.get(ty, GhostType::NotGhost)?;
        // `computeBtDB` then `integrate`, fused per element (the same
        // products, on the engine's threads)
        let mut k_e = Array::new(nn * nn);
        fe.compute_btdb_integrated(
            diffusivity.view(),
            &mut k_e,
            2,
            ty,
            GhostType::NotGhost,
            Some(&filter),
        )?;
        out.push((ty, filter, k_e));
    }
    Ok(out)
}
