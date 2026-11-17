// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phase_field_model.cc
// (assembleResidual, assembleInternalForces, assembleStiffnessMatrix),
// phasefield.cc (assembleInternalForces, assembleStiffnessMatrix).

//! Model-side assembly of the phase-field residual and stiffness
//! (PF-003/004), in upstream statement order with upstream operands, so the
//! assembled vectors and `K` are bitwise those of the reference with the
//! `eigen` backend (D-008).

use akantu_core::array::ArrayViewMut;
use akantu_core::enums::MatrixType;
use akantu_core::{Array, ElementType, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{LawContext, LawPhase};
use akantu_solver::DofManager;

use super::{DOF_ID, PhaseFieldPhysics, check_nodal, not_allocated};
use crate::phasefield::PhaseField;

/// Elemental stiffness of one element type: `(type, filter, ∫NᵀbN, ∫BᵀDB)`.
pub(crate) type ElementalStiffness = (ElementType, Vec<Idx>, Array<Real>, Array<Real>);

impl PhaseFieldPhysics {
    /// Upstream `assembleResidual()`: `assembleInternalForces()`, then the
    /// residual `+= f_ext` and `+= f_int` (coefficient `+1` both; `f_int`
    /// carries its sign).
    pub(crate) fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.assemble_internal_forces(dofs)?;
        let fe = self
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?;
        dofs.assemble_to_residual(DOF_ID, fe.view(), 1.0)?;
        let fi = self
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?;
        dofs.assemble_to_residual(DOF_ID, fi.view(), 1.0)
    }

    /// Upstream `assembleInternalForces()`: zero `f_int`; every law
    /// `computeAllDrivingForces(_not_ghost)`; on a distributed mesh the
    /// `_pfm_damage` exchange (owners send the driving force, driving energy
    /// and damage of the elements that are ghosts elsewhere; upstream starts
    /// it asynchronously before the local assembly and completes it before
    /// the ghost one: the values and the assembly order are the same done in
    /// one step) through the running operation's scope (ARCHITECTURE.md, "DOF manager", MPI-044);
    /// every law assembles its not-ghost then its ghost elements.
    pub(crate) fn assemble_internal_forces(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let nn = mesh.nb_nodes();
        {
            let mut f_int = self
                .internal_force
                .as_mut()
                .ok_or_else(|| not_allocated("internal_force"))?
                .write()?;
            check_nodal("assembleInternalForces: internal_force", &f_int, nn, 1)?;
            check_nodal(
                "assembleInternalForces: damage",
                &*dofs.dofs(DOF_ID).map_err(|_| not_allocated("damage"))?,
                nn,
                1,
            )?;
            f_int.zero();
        }
        {
            let (laws, services) = self.split(&mesh, dofs);
            let ctx = LawContext {
                phase: LawPhase::Stress,
                services: &services,
            };
            for law in laws.laws_mut() {
                law.compute_all_driving_forces(&ctx, GhostType::NotGhost)?;
            }
        }
        if let Some(es) = &mesh.synchronizers().element {
            let fe = self.base.fems.get("")?;
            let dim = self.base.spatial_dimension;
            let nb_quad = super::parallel::nb_quad_map(fe, &mesh, dim)?;
            let mut acc = super::parallel::PfmDamageAccessor {
                laws: &mut self.phasefields,
                nb_quad: &nb_quad,
                dim,
            };
            es.synchronize_in(
                &mut *dofs.phases()?,
                &mut acc,
                akantu_core::enums::SynchronizationTag::PfmDamage,
            )?;
        }
        let fe = self.base.fems.get("")?;
        let mut f_int = self
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))?
            .write()?;
        for ghost in GhostType::ALL {
            for law in self.phasefields.laws() {
                assemble_internal_forces_of(law.as_ref(), fe, &mesh, ghost, f_int.view_mut())?;
            }
        }
        Ok(())
    }

    /// Upstream `assembleStiffnessMatrix()`: create `K` (symmetric) if
    /// missing, zero it, then every law's not-ghost elements.
    pub(crate) fn assemble_stiffness_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !dofs.has_matrix("K") {
            dofs.new_matrix("K", MatrixType::Symmetric)?;
        }
        dofs.zero_matrix("K")?;
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let fe = self.base.fems.get("")?;
        for law in self.phasefields.laws() {
            for (ty, filter, k_n, k_b) in elemental_stiffness_of(law.as_ref(), fe)? {
                for k_e in [k_n, k_b] {
                    dofs.assemble_elemental_matrices_to_matrix(
                        &mesh,
                        "K",
                        DOF_ID,
                        k_e.view(),
                        ty,
                        GhostType::NotGhost,
                        MatrixType::Symmetric,
                        Some(&filter),
                    )?;
                }
            }
        }
        Ok(())
    }
}

/// Upstream `PhaseField::assembleInternalForces(ghost)`: for every type of
/// the law's filter with elements, `−∫ Nᵀ driving_force` then
/// `−∫ Bᵀ driving_energy` into `f_int`.
pub(crate) fn assemble_internal_forces_of(
    law: &dyn PhaseField,
    fe: &FeEngine,
    mesh: &Mesh,
    ghost: GhostType,
    mut f_int: ArrayViewMut<'_, Real>,
) -> Result<()> {
    let base = law.base();
    for ty in base.filter_types(ghost) {
        let filter = base.filter(ty, ghost);
        if filter.is_empty() {
            continue;
        }
        let nn = ty.nb_nodes_per_element();
        let df = base.driving_force.get(ty, ghost)?;
        let mut nt_df = Array::new(nn);
        fe.compute_ntb(df.view(), &mut nt_df, ty, ghost, Some(filter))?;
        let mut int_nt_df = Array::new(nn);
        fe.integrate(nt_df.view(), &mut int_nt_df, nn, ty, ghost, Some(filter))?;
        DofManager::assemble_elemental_array_local_view(
            mesh,
            int_nt_df.view(),
            f_int.reborrow(),
            ty,
            ghost,
            -1.0,
            Some(filter),
        )?;
        let de = base.driving_energy.get(ty, ghost)?;
        let mut bt_de = Array::new(nn);
        fe.compute_btd(de.view(), &mut bt_de, ty, ghost, Some(filter))?;
        let mut int_bt_de = Array::new(nn);
        fe.integrate(bt_de.view(), &mut int_bt_de, nn, ty, ghost, Some(filter))?;
        DofManager::assemble_elemental_array_local_view(
            mesh,
            int_bt_de.view(),
            f_int.reborrow(),
            ty,
            ghost,
            -1.0,
            Some(filter),
        )?;
    }
    Ok(())
}

/// Upstream `PhaseField::assembleStiffnessMatrix(_not_ghost)` up to the
/// DOF-manager assembly: per type of the filter (law dimension), the
/// elemental `∫ Nᵀ (damage_energy_density) N` and `∫ Bᵀ (damage_energy) B`
/// matrices (`nn²`, column-major). An empty filter skips the type (UD-378;
/// compat: stops the law's loop, as upstream's `return`).
pub(crate) fn elemental_stiffness_of(
    law: &dyn PhaseField,
    fe: &FeEngine,
) -> Result<Vec<ElementalStiffness>> {
    let base = law.base();
    let ghost = GhostType::NotGhost;
    let mut out = Vec::new();
    for ty in base.filter_types(ghost) {
        let filter = base.filter(ty, ghost);
        if filter.is_empty() {
            if base.compat.stiffness_stops_at_empty_filter {
                break;
            }
            continue;
        }
        let nn = ty.nb_nodes_per_element();
        let ded = base.damage_energy_density.get(ty, ghost)?;
        let de = base.damage_energy.get(ty, ghost)?;
        // `computeBtDB` / `computeNtbN` then `integrate`, fused per element
        // (the same products, on the engine's threads)
        let mut k_b = Array::new(nn * nn);
        fe.compute_btdb_integrated(de.view(), &mut k_b, 2, ty, ghost, Some(filter))?;
        let mut k_n = Array::new(nn * nn);
        fe.compute_ntbn_integrated(ded.view(), &mut k_n, ty, ghost, Some(filter))?;
        out.push((ty, filter.to_vec(), k_n, k_b));
    }
    Ok(out)
}
