// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.cc
// (getStableTimeStep, getEnergy, getEnergy(element), computeThermalEnergyByNode).

//! Stable time step and energies (DIFF-007, DIFF-011).
//!
//! Each public method is one collective operation of the DOF manager: the
//! local part is computed (and its outcome agreed) before the rank
//! reduction (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches"), so a failure on one rank is reported on every rank.

use akantu_comm::ReduceOp;
use akantu_core::{Element, ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::LawPhase;
use akantu_solver::DofManager;

use super::{DiffusionModel, DiffusionPhysics, check_nodal, not_allocated};

/// `allReduce(op)` of one scalar through the running collective operation
/// of `dm`; the local value (or the local error) is agreed first.
pub(crate) fn all_reduce_in(
    dm: &mut DofManager,
    local: Result<Real>,
    op: ReduceOp,
) -> Result<Real> {
    let mut ph = dm.phases()?;
    ph.run_collective("diffusion:all_reduce", local.map(|x| [x]), |c, mut s| {
        c.all_reduce(&mut s, op)?;
        Ok(s[0])
    })
}

impl DiffusionPhysics {
    /// Local part of upstream `getStableTimeStep()`: the smallest inradius
    /// `h` over the not-ghost elements of the model dimension (regular
    /// kind — every kind with `stable_time_step_includes_all_kinds`,
    /// UD-811 — every type, initial coordinates), then `min` over the laws of
    /// `getStableTimeStep(h)` (from `Real::MAX`).
    fn stable_time_step_local(&self) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let dim = self.base.spatial_dimension;
        let x = mesh.nodes();
        check_nodal("getStableTimeStep: mesh nodes", &x, mesh.nb_nodes(), dim)?;
        let mut min_el_size = Real::MAX;
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::NotGhost,
                // UD-811: upstream iterates every kind,
                // whose generic inradius is 0
                kind: if self.upstream_compat.stable_time_step_includes_all_kinds {
                    None
                } else {
                    Some(ElementKind::Regular)
                },
            })
            .collect();
        for ty in types {
            let nnpe = ty.nb_nodes_per_element();
            let conn = mesh.connectivity(ty, GhostType::NotGhost)?;
            let mut coords = vec![0.0; dim * nnpe];
            for e in 0..conn.len() {
                for (k, &node) in conn.tuple(e).iter().enumerate() {
                    let node = akantu_core::try_to_usize(node)?;
                    // A connectivity entry beyond the
                    // nodes (possible through raw mesh access) is an error,
                    // not a slice panic
                    let xn = node
                        .checked_mul(dim)
                        .and_then(|start| x.as_slice().get(start..start + dim))
                        .ok_or_else(|| {
                            Error::invalid(format!(
                                "getStableTimeStep: element {e} of {ty} references node {node}, \
                                 the mesh has {} nodes",
                                mesh.nb_nodes()
                            ))
                        })?;
                    coords[k * dim..(k + 1) * dim].copy_from_slice(xn);
                }
                let el_size = akantu_fe::engine::element_inradius_of(ty, &coords);
                // std::min(min_el_size, el_size)
                if el_size < min_el_size {
                    min_el_size = el_size;
                }
            }
        }
        let mut min_dt = Real::MAX;
        for law in self.laws.laws() {
            let dt = law.stable_time_step(min_el_size)?;
            // std::min(law_dt, min_dt)
            if min_dt < dt {
                continue;
            }
            min_dt = dt;
        }
        Ok(min_dt)
    }

    /// Local part of upstream `getEnergy(id)`: `Σ` over the laws (creation
    /// order, from `0`).
    fn energy_local(&mut self, dofs: &DofManager, energy_id: &str) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.with_laws(&mesh, dofs, LawPhase::Energy, |laws, ctx| {
            let mut energy = 0.0;
            for law in laws.laws() {
                energy += law.law_energy(ctx, energy_id)?;
            }
            Ok(energy)
        })
    }

    /// Upstream (private, unused) `computeThermalEnergyByNode()`: `Σ_n
    /// internal_flow[n] · Δt` over local and master nodes (local part).
    pub(crate) fn thermal_energy_by_node_local(&self, mesh: &Mesh) -> Result<Real> {
        let fi = self
            .internal_flow
            .as_ref()
            .ok_or_else(|| not_allocated("internal_flow"))?
            .read()?;
        check_nodal(
            "computeThermalEnergyByNode: internal_flow",
            &fi,
            mesh.nb_nodes(),
            1,
        )?;
        let dt = self.time_step;
        let mut ethermal = 0.0;
        for (n, flow) in fi.as_slice().iter().enumerate() {
            let mut e = 0.0;
            if mesh
                .node_flag(akantu_core::try_idx(n)?)
                .is_local_or_master()
            {
                // (flow.array() * time_step).sum() over one component
                e += flow * dt;
            }
            ethermal += e;
        }
        Ok(ethermal)
    }
}

impl DiffusionModel {
    /// Upstream `getStableTimeStep()` (DIFF-007): local minimum, then
    /// `allReduce(_min)`.
    pub fn stable_time_step(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:stable_time_step", |dm| {
                let local = p.stable_time_step_local();
                all_reduce_in(dm, local, ReduceOp::Min)
            })
    }

    /// Upstream `getEnergy(id)` (DIFF-011): `Σ` over the laws of their
    /// `getEnergy(id)` (`"thermal"` for `heat_diffusion`; `0` for unknown
    /// ids), summed over ranks.
    pub fn energy(&mut self, energy_id: &str) -> Result<Real> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:energy", |dm| {
                let local = p.energy_local(dm, energy_id);
                all_reduce_in(dm, local, ReduceOp::Sum)
            })
    }

    /// Upstream `getEnergy(id, element)` (local, no reduction): the
    /// element's law energy of its law-local element (see
    /// `DiffusionUpstreamCompat::element_energy_uses_law_local_index`,
    /// UD-280).
    pub fn energy_of_element(&mut self, energy_id: &str, element: Element) -> Result<Real> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let laws = &self.physics.laws;
        let i = akantu_core::decode_idx(laws.law_index_map().element_ref(element)?[0])?
            .ok_or_else(|| Error::invalid(format!("{element} has no constitutive law")))?;
        let local = laws.local_numbering_map().element(element)?[0];
        let local = Element::new(element.ty(), local, element.ghost_type());
        self.physics.with_laws(
            &mesh,
            &self.solver.dof_manager,
            LawPhase::Energy,
            |laws, ctx| {
                laws.law(i)?
                    .law_energy_of_element(ctx, energy_id, local, element)
            },
        )
    }

    /// Upstream private `computeThermalEnergyByNode()` (exposed for tests):
    /// `Σ` over local and master nodes of `internal_flow · Δt`, summed over
    /// ranks.
    pub fn thermal_energy_by_node(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("diffusion:thermal_energy_by_node", |dm| {
                let local = p
                    .base
                    .mesh
                    .clone()
                    .read()
                    .and_then(|m| p.thermal_energy_by_node_local(&m));
                all_reduce_in(dm, local, ReduceOp::Sum)
            })
    }
}
