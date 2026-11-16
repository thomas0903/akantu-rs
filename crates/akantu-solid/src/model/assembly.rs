// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model.cc (assembleResidual, assembleInternalForces,
// assembleStiffnessMatrix), solid_mechanics_model_mass.cc
// (assembleMassLumped, assembleMass, ComputeRhoFunctor), material.cc
// (assembleInternalForces, assembleStiffnessMatrix,
// assembleInternalForcesFiniteDeformation, assembleStiffnessMatrixNL/L2),
// material_inline_impl.hh (stressToVoigt, setCauchyStressMatrix),
// src/fe_engine/fe_engine_template_tmpl_field.hh (assembleFieldLumped,
// assembleFieldMatrix: DOF-manager part).

//! Model-side assembly (SMM-004…008, SMM-022 pipeline, SMM-023).
//!
//! Every routine follows the upstream statement order and calls the FE
//! engine and DOF-manager kernels with the upstream operands, so that the
//! assembled internal forces, `K`, lumped mass and `M` are bitwise those of
//! the reference with the `eigen` backend (D-008).

use akantu_comm::ReduceOp;
use akantu_core::array_shared::ArrayMut;
use akantu_core::enums::MatrixType;
use akantu_core::error::if_present;
use akantu_core::exec::NodeScatter;
use akantu_core::{Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{LawContext, LawPhase, LawReducer};
use akantu_model::model::{
    ReduceOperation, alloc_nodal_shared_field, make_consistent_for_periodicity_values_in,
    periodicity_is_distributed,
};
use akantu_solver::DofManager;

use super::{DOF_ID, SolidMechanicsPhysics, check_nodal, not_allocated};
use crate::material::Material;

mod finite_deformation;

/// Elemental matrices of one element type: `(type, filter, K_e)`.
pub(crate) type ElementalBlock = (ElementType, Vec<Idx>, Array<Real>);

/// Per-point and per-element buffers of the internal-force assembly, kept
/// by the model between steps so that the explicit step does not allocate
/// (and fault in) them at every call. Every use overwrites
/// every entry it reads (the FE kernels resize their outputs and write all
/// of them), so the values never depend on what a buffer held before.
#[derive(Default)]
pub(crate) struct AssemblyScratch {
    /// `Bᵀσ` (small strain) or `(B_L0 + B_L1)ᵀ S` (finite deformation) per
    /// quadrature point.
    pub(crate) per_point: Array<Real>,
    /// Integrated element vectors.
    pub(crate) per_element: Array<Real>,
    /// Node-ordered scatter tables of the threaded assembly (D-123), per
    /// `(law, type, ghost)`; built on first use with more than one thread.
    pub(crate) scatters: std::collections::HashMap<(usize, ElementType, GhostType), ScatterEntry>,
    /// Per-node terms of the threaded external-work increment (behind a
    /// lock: the increment is computed from `&self`).
    pub(crate) work_terms: std::sync::Mutex<Vec<Real>>,
}

/// A cached [`NodeScatter`] with what it was built from: the mesh topology
/// revision and the law's filter. The table is reused while both are
/// unchanged and the connectivity is not exported (foreign code could write
/// it without a revision); otherwise it is checked row by row against the
/// connectivity ([`NodeScatter::matches`]) and rebuilt when it differs.
pub(crate) struct ScatterEntry {
    revision: u64,
    filter: Vec<Idx>,
    table: NodeScatter,
}

/// `f_int −= ∫ Bᵀσ` of one block: the serial scatter, or with more than one
/// thread the node-ordered scatter (same additions in the same order per
/// node, D-123) with its table from the cache.
#[allow(clippy::too_many_arguments)] // the block, its table cache and the output
fn scatter_block(
    exec: &akantu_core::exec::Exec,
    mesh: &Mesh,
    r_e: &Array<Real>,
    f_int: &mut ArrayMut<'_, Real>,
    ty: ElementType,
    ghost: GhostType,
    filter: &[Idx],
    key: (usize, ElementType, GhostType),
    scatters: &mut std::collections::HashMap<(usize, ElementType, GhostType), ScatterEntry>,
) -> Result<()> {
    let serial = |f_int: &mut ArrayMut<'_, Real>| {
        DofManager::assemble_elemental_array_local_view(
            mesh,
            r_e.view(),
            f_int.view_mut(),
            ty,
            ghost,
            -1.0,
            Some(filter),
        )
    };
    // the serial routine's early return, before any access to the block
    if !exec.is_parallel() || (mesh.nb_element(ty, ghost) == 0 && r_e.is_empty()) {
        return serial(f_int);
    }
    let conn = mesh.connectivity(ty, ghost)?;
    let nnpe = conn.nb_component();
    let nb_nodes = f_int.len();
    let nb_dof = r_e.nb_component().checked_div(nnpe).unwrap_or(0);
    // every size the serial routine checks: on a mismatch it reports it,
    // with its own error
    if nnpe == 0
        || !r_e.nb_component().is_multiple_of(nnpe)
        || f_int.nb_component() != nb_dof
        || r_e.len() != filter.len()
    {
        drop(conn);
        return serial(f_int);
    }
    let revision = mesh.revision();
    let exported = mesh.connectivities().shared(ty, ghost)?.is_exported();
    let (cs, fl) = (conn.as_slice(), Some(filter));
    let valid = scatters.get_mut(&key).is_some_and(|e| {
        let same = e.revision == revision && !exported && e.filter.as_slice() == filter;
        if same || e.table.matches(exec, cs, nnpe, fl, nb_nodes) {
            e.revision = revision;
            if !same {
                e.filter.clear();
                e.filter.extend_from_slice(filter);
            }
            true
        } else {
            false
        }
    });
    if !valid {
        // an element or node out of range: the serial routine reports it
        let Ok(table) = NodeScatter::new(cs, nnpe, fl, nb_nodes) else {
            drop(conn);
            return serial(f_int);
        };
        scatters.insert(
            key,
            ScatterEntry {
                revision,
                filter: filter.to_vec(),
                table,
            },
        );
    }
    let entry = scatters
        .get(&key)
        .ok_or_else(|| Error::invalid("internal force assembly: scatter table missing"))?;
    debug_assert!(
        entry.table.matches(exec, cs, nnpe, fl, nb_nodes),
        "stale node scatter table of {ty:?} {ghost:?}"
    );
    entry
        .table
        .scatter_add(exec, r_e.as_slice(), nb_dof, -1.0, f_int.as_mut_slice())
}

impl std::fmt::Debug for AssemblyScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssemblyScratch")
            .field(
                "per_point",
                &(self.per_point.len(), self.per_point.nb_component()),
            )
            .field(
                "per_element",
                &(self.per_element.len(), self.per_element.nb_component()),
            )
            .finish()
    }
}

/// Upstream `ComputeRhoFunctor`: the `nb_dof × nq` density matrix of
/// `element` (every row = the material's `getRho` of the law-local element).
/// `rho` is the caller's scratch buffer, reused across elements.
/// `law_index_block` is the law-index block of the element's type,
/// guarded **once per element type** by the caller (a
/// guard per element cost ≈ 5 % of the lumped-mass assembly); `None` if the
/// block is absent, in which case every element reports the error of the
/// per-element accessor, as before.
fn rho_of(
    materials: &akantu_model::ConstitutiveLawsHandler<dyn Material>,
    law_index_block: Option<&Array<Idx>>,
    mat: &mut [Real],
    element: Element,
    nb_dof: usize,
    rho: &mut Vec<Real>,
) -> Result<()> {
    // the errors of `SharedElementTypeMapArray::element_ref`, in its order
    let law_index = match law_index_block {
        Some(block) => {
            let i = element
                .try_idx()
                .ok_or_else(|| Error::invalid("ElementTypeMapArray::element: ElementNull"))?;
            block.get_tuple(i)?[0]
        }
        None => materials.law_index_map().element_ref(element)?[0],
    };
    let local = materials.local_numbering_map().element(element)?[0];
    let law_index = akantu_core::decode_idx(law_index)?
        .ok_or_else(|| Error::invalid(format!("{element} has no material (mass assembly)")))?;
    let law = materials.law(law_index)?;
    let nq = mat.len() / nb_dof.max(1);
    rho.clear();
    rho.resize(nq, 0.0);
    law.rho_on_quadrature_points(Element::new(element.ty(), local, element.ghost_type()), rho);
    for (q, r) in rho.iter().enumerate() {
        for d in 0..nb_dof {
            mat[d + q * nb_dof] = *r;
        }
    }
    Ok(())
}

/// [`LawReducer`] of a model phase running inside the DOF manager's
/// collective operation: each reduction is one checked collective
/// of the operation's scope (`Phases::run_collective`: agreement on the
/// local values, then the matched all-reduce), so it is legal inside a
/// solver callback where a raw communicator call is refused (ARCHITECTURE, "Agreement across ranks"). The first reduction also agrees the phase's local outcome (a rank
/// whose phase failed makes every rank fail at that step); without any
/// reduction the outcome is returned by [`ScopeReducer::finish`] and agreed
/// by the enclosing step, as before.
pub(crate) struct ScopeReducer<'a> {
    dofs: &'a mut DofManager,
    nb_ranks: usize,
    pending: Option<Result<()>>,
    name: &'static str,
}

impl<'a> ScopeReducer<'a> {
    /// A reducer for the phase whose local outcome is `phase`.
    pub(crate) fn new(
        dofs: &'a mut DofManager,
        nb_ranks: usize,
        phase: Result<()>,
        name: &'static str,
    ) -> Self {
        Self {
            dofs,
            nb_ranks,
            pending: Some(phase),
            name,
        }
    }

    /// The phase's local outcome if no reduction consumed it.
    pub(crate) fn finish(self) -> Result<()> {
        self.pending.unwrap_or(Ok(()))
    }
}

impl LawReducer for ScopeReducer<'_> {
    fn nb_ranks(&self) -> usize {
        self.nb_ranks
    }

    fn all_reduce_real(&mut self, values: &mut [Real], op: ReduceOp) -> Result<()> {
        let mut ph = self.dofs.phases()?;
        let prepared = match self.pending.take() {
            Some(Err(e)) => Err(e),
            _ => Ok(values.to_vec()),
        };
        let out = ph.run_collective(self.name, prepared, |c, mut v: Vec<Real>| {
            c.all_reduce(&mut v, op)?;
            Ok(v)
        })?;
        values.copy_from_slice(&out);
        Ok(())
    }
}

impl SolidMechanicsPhysics {
    /// Upstream `assembleResidual()` (SMM-004): accumulate the external work
    /// increment when the displacement changed since the last time, the
    /// internal forces, then residual `+= f_ext` and `+= f_int` (both with
    /// coefficient `+1`: `f_int` already carries its sign).
    pub(crate) fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        // Validate the user-resizable nodal arrays before
        // any accumulation (no partial update of `external_work`)
        {
            let nn = self.base.mesh.read()?.nb_nodes();
            let d = self.base.spatial_dimension;
            let fe = self
                .external_force
                .as_ref()
                .ok_or_else(|| not_allocated("external_force"))?;
            check_nodal("assembleResidual: external_force", fe, nn, d)?;
            let fi = self
                .internal_force
                .as_ref()
                .ok_or_else(|| not_allocated("internal_force"))?;
            check_nodal("assembleResidual: internal_force", fi, nn, d)?;
            check_nodal(
                "assembleResidual: displacement",
                &dofs
                    .dofs(DOF_ID)
                    .map_err(|_| not_allocated("displacement"))?,
                nn,
                d,
            )?;
        }
        if self.external_work_release != self.displacement_release {
            let w = self.external_work_increment(dofs)?;
            self.external_work += w;
            self.external_work_release = self.displacement_release;
        }
        self.assemble_internal_forces(dofs)?;
        let fe = self
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()?;
        dofs.assemble_to_residual(DOF_ID, fe.view(), 1.0)?;
        let fi = self
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?;
        dofs.assemble_to_residual(DOF_ID, fi.view(), 1.0)
    }

    /// Upstream `assembleInternalForces()` (SMM-005): zero `f_int`; every
    /// material computes its stresses on the not-ghost elements (`∇u − ε⁰`
    /// then `computeStress`); every material assembles `−∫ Bᵀσ` of its
    /// not-ghost then ghost elements; periodic consistency (sum).
    pub fn assemble_internal_forces(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let d = self.base.spatial_dimension;
        check_nodal(
            "assembleInternalForces: displacement",
            &dofs
                .dofs(DOF_ID)
                .map_err(|_| not_allocated("displacement"))?,
            mesh.nb_nodes(),
            d,
        )?;
        let f_int = self
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))?;
        check_nodal(
            "assembleInternalForces: internal_force",
            f_int,
            mesh.nb_nodes(),
            d,
        )?;
        f_int.write()?.zero();
        let stresses = {
            let (laws, services) = self.split(&mesh, dofs);
            let ctx = LawContext {
                phase: LawPhase::Stress,
                services: &services,
            };
            laws.laws_mut()
                .iter_mut()
                .try_for_each(|law| law.compute_all_stresses(&ctx, GhostType::NotGhost))
        };
        // The laws' declared rank reductions of `computeAllStresses`
        // (upstream: a raw `allReduce` at its end), through the running
        // collective operation's scope; the first one agrees the stress
        // phase's local outcome
        {
            let mut reducer = ScopeReducer::new(
                dofs,
                mesh.communicator().size(),
                stresses,
                "smm:law_stress_reduction",
            );
            for law in self.materials.laws_mut() {
                law.reduce_after_stresses(GhostType::NotGhost, &mut reducer)?;
            }
            reducer.finish()?;
        }
        // upstream: `if (isNonLocal()) getNonLocalManager().computeAllNonLocalContribution()`
        if let Some(mut manager) = self.non_local.take() {
            // distributed: the exchanges `_mnl_for_average` /
            // `_mnl_weight` over the grid synchronisers run through the
            // running operation's scope (ARCHITECTURE.md, "DOF manager"), the laws read the DOF
            // arrays borrowed apart from it
            let mut ghosts = self.non_local_ghosts.take();
            let r = match ghosts.as_mut() {
                Some(g) => (|| {
                    let (mut guard, family) = dofs.phases_with_family(super::DOF_ID)?;
                    let (laws, services) = self.split_arrays(
                        &mesh,
                        super::services::DofArrays::of_family(family.map(|f| &*f)),
                    );
                    let ctx = LawContext {
                        phase: LawPhase::Stress,
                        services: &services,
                    };
                    let mut cb = super::non_local::SmmNonLocalCallback { laws, ctx: &ctx };
                    let mut ex = g.exchange_in(&mut guard);
                    manager.compute_all_non_local_contribution(&mut cb, &mut ex)
                })(),
                None => {
                    let (laws, services) = self.split(&mesh, dofs);
                    let ctx = LawContext {
                        phase: LawPhase::Stress,
                        services: &services,
                    };
                    let mut cb = super::non_local::SmmNonLocalCallback { laws, ctx: &ctx };
                    manager.compute_all_non_local_contribution(
                        &mut cb,
                        &mut akantu_model::non_local::NoGhosts,
                    )
                }
            };
            self.non_local = Some(manager);
            self.non_local_ghosts = ghosts;
            // The laws' declared reductions of the end
            // of `computeNonLocalStresses` (upstream: a raw `allReduce`),
            // through the running operation's scope; the first one agrees
            // the non-local phase's outcome (without any, `finish` returns
            // it as before)
            let mut reducer = ScopeReducer::new(
                dofs,
                mesh.communicator().size(),
                r,
                "smm:law_non_local_reduction",
            );
            for law in self.materials.laws_mut() {
                if let Some(nl) = law.as_non_local_mut() {
                    nl.reduce_after_non_local_stresses(GhostType::NotGhost, &mut reducer)?;
                }
            }
            reducer.finish()?;
        }
        // _smm_stress: owners send the stresses of the elements that are
        // ghosts elsewhere (upstream asynchronousSynchronize before the
        // local assembly, waitEndSynchronize before the ghost one; the
        // values and the assembly order are the same done in one step),
        // through the running operation's scope (ARCHITECTURE.md, "DOF manager")
        if let Some(es) = &mesh.synchronizers().element {
            let fe = self.base.fems.get("")?;
            let nb_quad = nb_quad_map(fe, &mesh, self.base.spatial_dimension)?;
            let mut acc = super::parallel::StressAccessor {
                materials: &mut self.materials,
                nb_quad: &nb_quad,
                dim: self.base.spatial_dimension,
            };
            es.synchronize_in(
                &mut *dofs.phases()?,
                &mut acc,
                akantu_core::enums::SynchronizationTag::SmmStress,
            )?;
        }
        let fe = self.base.fems.get("")?;
        let mut f_int = self
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))?
            .write()?;
        for ghost in GhostType::ALL {
            for (law_index, law) in self.materials.laws().iter().enumerate() {
                // laws with their own residual (cohesive laws)
                // assemble at their position in the loop (upstream virtual
                // `Material::assembleInternalForces`)
                if let Some(r) = law.assemble_internal_forces_custom(
                    &self.base.fems,
                    &mesh,
                    ghost,
                    f_int.view_mut(),
                ) {
                    r?;
                    continue;
                }
                assemble_internal_forces_of(
                    law_index,
                    law.as_ref(),
                    fe,
                    &mesh,
                    self.base.spatial_dimension,
                    ghost,
                    &mut f_int,
                    &mut self.assembly_scratch,
                )?;
            }
        }
        let (len, nbc) = (f_int.len(), f_int.nb_component());
        // On a distributed periodic mesh through the running
        // operation's scope
        let mut scope = if periodicity_is_distributed(&mesh) {
            Some(dofs.phases()?)
        } else {
            None
        };
        make_consistent_for_periodicity_values_in(
            &mesh,
            scope.as_deref_mut(),
            f_int.as_mut_slice(),
            len,
            nbc,
            ReduceOperation::Sum,
            |a, b| a + b,
        )
    }

    /// Upstream `assembleStiffnessMatrix(need_to_reassemble)` (SMM-006):
    /// create `K` (type of `getMatrixType("K")`) if missing; reassemble when
    /// asked or when a material reports `hasMatrixChanged("K")` (stiffness
    /// changed or finite deformation; asked with the phase services,
    /// [`Material::has_matrix_changed_in`], so a law whose answer depends on
    /// the model — `viscoelastic_maxwell` and the time step — reads the
    /// model's current state): zero `K`, then every material's not-ghost
    /// elements.
    pub fn assemble_stiffness_matrix(
        &mut self,
        dofs: &mut DofManager,
        mut need_to_reassemble: bool,
    ) -> Result<()> {
        if !dofs.has_matrix("K") {
            dofs.new_matrix("K", self.model_matrix_type("K"))?;
        }
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        {
            // the laws read the model state of the phase (upstream
            // `hasStiffnessMatrixChanged` of `viscoelastic_maxwell` compares
            // its last tangent's time step with `model.getTimeStep()`)
            let (laws, services) = self.split(&mesh, dofs);
            let ctx = LawContext {
                phase: LawPhase::Tangent,
                services: &services,
            };
            for law in laws.laws() {
                need_to_reassemble |= law.has_matrix_changed_in(&ctx, "K");
                // a runtime law's exception in that answer:
                // raised here, before `K` is touched, as upstream
                law.take_deferred_error()?;
            }
        }
        need_to_reassemble |= self.need_to_reassemble_stiffness;
        if !need_to_reassemble {
            return Ok(());
        }
        check_nodal(
            "assembleStiffnessMatrix: displacement",
            &dofs
                .dofs(DOF_ID)
                .map_err(|_| not_allocated("displacement"))?,
            mesh.nb_nodes(),
            self.base.spatial_dimension,
        )?;
        dofs.zero_matrix("K")?;
        let subtract_eigen = !self.upstream_compat.stiffness_grad_u_ignores_eigen_grad_u;
        let mut scratch = std::mem::take(&mut self.stiffness_scratch);
        let r = self.assemble_stiffness_blocks(dofs, &mesh, subtract_eigen, &mut scratch);
        self.stiffness_scratch = scratch;
        r?;
        self.need_to_reassemble_stiffness = false;
        Ok(())
    }

    /// The elemental stiffness blocks of every law, assembled into `K`
    /// (the loop of [`SolidMechanicsPhysics::assemble_stiffness_matrix`]);
    /// the small-strain elemental arrays come from and return to `scratch`.
    fn assemble_stiffness_blocks(
        &mut self,
        dofs: &mut DofManager,
        mesh: &Mesh,
        subtract_eigen: bool,
        scratch: &mut StiffnessScratch,
    ) -> Result<()> {
        for i in 0..self.materials.nb_laws() {
            let blocks = {
                let (laws, services) = self.split(mesh, dofs);
                let ctx = LawContext {
                    phase: LawPhase::Tangent,
                    services: &services,
                };
                let law = laws.law_mut(i)?;
                // laws with their own tangent (cohesive laws,
                // `_unsymmetric` blocks, UD-034) — upstream virtual
                // `Material::assembleStiffnessMatrix`
                match law.elemental_stiffness_custom(&ctx, GhostType::NotGhost) {
                    Some(r) => r?,
                    None => elemental_stiffness_with(
                        law,
                        &ctx,
                        GhostType::NotGhost,
                        subtract_eigen,
                        scratch,
                    )?
                    .into_iter()
                    .map(|(ty, filter, k_e)| (ty, filter, k_e, MatrixType::Symmetric))
                    .collect(),
                }
            };
            for (ty, filter, k_e, elemental_type) in blocks {
                dofs.assemble_elemental_matrices_to_matrix(
                    mesh,
                    "K",
                    DOF_ID,
                    k_e.view(),
                    ty,
                    GhostType::NotGhost,
                    elemental_type,
                    Some(&filter),
                )?;
                scratch.give_back(k_e);
            }
        }
        Ok(())
    }

    /// Upstream `assembleMassLumped()` (SMM-007): when needed (first call,
    /// node additions, or a material's `hasMatrixChanged("M")`): allocate
    /// and zero the nodal `mass`, create/zero the lumped matrix `M`, lump
    /// `ρ` element type by element type (not-ghost, then ghost), periodic
    /// consistency, assemble into `M`; then, as upstream with
    /// `AKANTU_NDEBUG` undefined (the oracle build), entries of `mass` with
    /// `|m| < ε` or NaN are set to `0` (after the assembly into `M`).
    pub(crate) fn assemble_mass_lumped(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mut need = self.need_to_reassemble_lumped_mass;
        for law in self.materials.laws() {
            need |= law.has_matrix_changed("M");
        }
        if !need {
            return Ok(());
        }
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let dim = self.base.spatial_dimension;
        // _smm_density (upstream: started asynchronously before the
        // not-ghost lumping, completed before the ghost one; the exchanged
        // values do not depend on the lumping): the phase-field law with
        // `degrade_mass` sends its damage and `grad_u` to the ghost copies
        // (MPI-036); the other laws have no data (empty
        // messages). First, so that a failed exchange (agreed on every rank)
        // changes nothing: the mass, `M` and the reassembly flag stay as
        // they were.
        if let Some(es) = &mesh.synchronizers().element {
            let nb_quad = nb_quad_map(self.base.fems.get("")?, &mesh, dim)?;
            let mut acc = super::parallel::LawDataAccessor {
                materials: &mut self.materials,
                nb_quad: &nb_quad,
            };
            es.synchronize_in(
                &mut *dofs.phases()?,
                &mut acc,
                akantu_core::enums::SynchronizationTag::SmmDensity,
            )?;
        }
        alloc_nodal_shared_field(&mut self.mass, mesh.nb_nodes(), dim)?;
        if !dofs.has_lumped_matrix("M") {
            dofs.new_lumped_matrix("M")?;
        }
        dofs.zero_lumped_matrix("M")?;
        let fe = self.base.fems.get("")?;
        let mut mass = self
            .mass
            .as_mut()
            .ok_or_else(|| not_allocated("mass"))?
            .write()?;
        mass.zero();
        for ghost in GhostType::ALL {
            let materials = &self.materials;
            let types: Vec<ElementType> = mesh
                .element_types(akantu_core::TypeFilter {
                    spatial_dimension: Some(dim),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Regular),
                })
                .collect();
            for ty in types {
                let mut rho = Vec::new();
                // one guard of the law-index block per type
                let law_index = if_present(materials.law_index_map().get(ty, ghost))?;
                let law_index = law_index.as_deref();
                let mut functor = |m: &mut [Real], e: Element| -> Result<()> {
                    rho_of(materials, law_index, m, e, dim, &mut rho)
                };
                let lumped = fe.lumped_field_elemental(&mesh, &mut functor, dim, ty, ghost)?;
                DofManager::assemble_elemental_array_local_view(
                    &mesh,
                    lumped.view(),
                    mass.view_mut(),
                    ty,
                    ghost,
                    1.0,
                    None,
                )?;
            }
        }
        let (len, nbc) = (mass.len(), mass.nb_component());
        {
            // Through the running operation's scope when distributed
            let mut scope = if periodicity_is_distributed(&mesh) {
                Some(dofs.phases()?)
            } else {
                None
            };
            make_consistent_for_periodicity_values_in(
                &mesh,
                scope.as_deref_mut(),
                mass.as_mut_slice(),
                len,
                nbc,
                ReduceOperation::Sum,
                |a, b| a + b,
            )?;
        }
        dofs.assemble_to_lumped_matrix(DOF_ID, mass.view(), "M", 1.0)?;
        for m in mass.as_mut_slice() {
            if m.abs() < Real::EPSILON || m.is_nan() {
                *m = 0.0;
            }
        }
        // _smm_mass (after the lumping) is packed but never unpacked
        // upstream: it changes no state and is not exchanged (documented,
        // SMM-016)
        self.need_to_reassemble_lumped_mass = false;
        Ok(())
    }

    /// Upstream `assembleMass()` (SMM-008): create `M` (symmetric) if
    /// missing; assembled only once (`need_to_reassemble_mass`): zero `M`,
    /// `∫ Nᵀ ρ N` per not-ghost element type (rule of order
    /// `2·polynomial degree`).
    pub(crate) fn assemble_mass(&mut self, dofs: &mut DofManager) -> Result<()> {
        if !dofs.has_matrix("M") {
            dofs.new_matrix("M", self.model_matrix_type("M"))?;
        }
        if !self.need_to_reassemble_mass {
            return Ok(());
        }
        dofs.zero_matrix("M")?;
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let dim = self.base.spatial_dimension;
        let fe = self.base.fems.get("")?;
        let materials = &self.materials;
        let types: Vec<ElementType> = mesh
            .element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let mut rho = Vec::new();
            // one guard of the law-index block per type
            let law_index = if_present(materials.law_index_map().get(ty, GhostType::NotGhost))?;
            let law_index = law_index.as_deref();
            let mut functor = |m: &mut [Real], e: Element| -> Result<()> {
                rho_of(materials, law_index, m, e, dim, &mut rho)
            };
            let m_e =
                fe.field_matrix_elemental(&mesh, &mut functor, dim, ty, GhostType::NotGhost)?;
            dofs.assemble_elemental_matrices_to_matrix(
                &mesh,
                "M",
                DOF_ID,
                m_e.view(),
                ty,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                None,
            )?;
        }
        self.need_to_reassemble_mass = false;
        Ok(())
    }
}

/// Quadrature points per element type of the model dimension (both ghost
/// types share the rule).
pub(crate) fn nb_quad_map(
    fe: &akantu_fe::FeEngine,
    mesh: &Mesh,
    dim: usize,
) -> Result<akantu_core::ElementTypeMap<usize>> {
    let mut out = akantu_core::ElementTypeMap::default();
    for ghost in GhostType::ALL {
        for ty in mesh.element_types(akantu_core::TypeFilter::new().dim(dim).ghost(ghost)) {
            if out.get(ty, GhostType::NotGhost).is_none() {
                out.insert(
                    ty,
                    GhostType::NotGhost,
                    fe.nb_integration_points(ty, ghost)?,
                )?;
            }
        }
    }
    Ok(out)
}

/// Upstream `Material::assembleInternalForces(ghost)` for one material:
/// every type of its filter of the model dimension; small strain: `Bᵀσ`
/// (`computeBtD` on `stress`), integrated, assembled with `−1`; finite
/// deformation: the total-Lagrangian residual. `scratch` holds the
/// intermediate arrays between calls (fully overwritten by each use).
#[allow(clippy::too_many_arguments)] // the law and its index, the block, the output, the buffers
pub(crate) fn assemble_internal_forces_of(
    law_index: usize,
    law: &dyn Material,
    fe: &FeEngine,
    mesh: &Mesh,
    dim: usize,
    ghost: GhostType,
    f_int: &mut ArrayMut<'_, Real>,
    scratch: &mut AssemblyScratch,
) -> Result<()> {
    for ty in law.base().filter_types(ghost) {
        if ty.spatial_dimension() != dim {
            continue;
        }
        let filter = law
            .base()
            .element_filter
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        if filter.is_empty() {
            continue;
        }
        let AssemblyScratch {
            per_point,
            per_element: r_e,
            scatters,
            ..
        } = &mut *scratch;
        if law.base().finite_deformation {
            finite_deformation::internal_forces(law, fe, dim, ty, ghost, &filter, per_point, r_e)?;
        } else {
            let stress = law.base().stress.get(ty, ghost)?;
            let nn = ty.nb_nodes_per_element();
            // sigma_dphi (upstream: a new array per call)
            let sigma_dphi = per_point;
            fe.compute_btd(stress.view(), sigma_dphi, ty, ghost, Some(&filter))?;
            fe.integrate(sigma_dphi.view(), r_e, dim * nn, ty, ghost, Some(&filter))?;
        }
        scatter_block(
            fe.exec(),
            mesh,
            r_e,
            f_int,
            ty,
            ghost,
            &filter,
            (law_index, ty, ghost),
            scatters,
        )?;
    }
    Ok(())
}

/// Reused arrays of the stiffness assembly: the gradient and tangent of
/// the current block and a pool of elemental-matrix arrays. Reusing them
/// keeps their pages mapped from one assembly to the next; the values are
/// the same as with fresh arrays (every value is written before it is
/// read, the tangent is zeroed before the law fills it).
#[derive(Debug, Default)]
pub(crate) struct StiffnessScratch {
    grad_u: Array<Real>,
    tangent: Array<Real>,
    pool: Vec<Array<Real>>,
}

impl StiffnessScratch {
    /// An array of `nb_component` components from the pool (the largest
    /// one), or a new one.
    fn take(&mut self, nb_component: usize) -> Array<Real> {
        let best = self
            .pool
            .iter()
            .enumerate()
            .filter(|(_, a)| a.nb_component() == nb_component)
            .max_by_key(|(_, a)| a.capacity())
            .map(|(k, _)| k);
        match best {
            Some(k) => self.pool.swap_remove(k),
            None => Array::new(nb_component),
        }
    }

    /// Return an elemental-matrix array to the pool.
    fn give_back(&mut self, a: Array<Real>) {
        self.pool.push(a);
    }
}

/// Upstream `Material::assembleStiffnessMatrix(ghost)` up to (excluding)
/// the DOF-manager assembly: per type of the filter (model dimension, non
/// empty), the elemental matrices `K_e` (`(dim·nn)²`, column-major) with
/// the filter. Small strain: `grad_u ← ∇u − eigen_grad_u` (UD-208,
/// upstream stores `∇u` without the subtraction, reproduced
/// with `subtract_eigen = false`), zeroed tangent, `computeTangentModuli`,
/// `computeBtDB(order 4)`, integration. Finite deformation: the geometric
/// (`NL`) then the material (`L2`) parts, each its own block (upstream
/// assembles them one after the other; the `L2` part's `grad_u` follows
/// the same `subtract_eigen` rule).
/// The arrays come from (and the elemental matrices return to) `scratch`.
pub(crate) fn elemental_stiffness_with(
    law: &mut dyn Material,
    ctx: &LawContext<'_>,
    ghost: GhostType,
    subtract_eigen: bool,
    scratch: &mut StiffnessScratch,
) -> Result<Vec<ElementalBlock>> {
    let dim = law.base().spatial_dimension;
    let mut out = Vec::new();
    for ty in law.base().filter_types(ghost) {
        if ty.spatial_dimension() != dim {
            continue;
        }
        let filter = law
            .base()
            .element_filter
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        // Upstream's finite-deformation stiffness path
        // has no early return for an empty filter — it still registers the
        // type's connectivity in K's profile — so only the small-strain
        // path skips empty filters (K's COO order matches the oracle).
        if filter.is_empty() && !law.base().finite_deformation {
            continue;
        }
        if law.base().finite_deformation {
            let (k_nl, k_l2) =
                finite_deformation::stiffness(law, ctx, ty, ghost, &filter, subtract_eigen)?;
            out.push((ty, filter.clone(), k_nl));
            out.push((ty, filter, k_l2));
            continue;
        }
        let fe = ctx.services.fe_engine("")?;
        let nq = fe.nb_integration_points(ty, ghost)?;
        let nn = ty.nb_nodes_per_element();
        // gradu_vect.resize(nq·ne); gradientOnIntegrationPoints(u, gradu_vect)
        if scratch.grad_u.nb_component() != dim * dim {
            scratch.grad_u = Array::new(dim * dim);
        }
        let gu = &mut scratch.grad_u;
        ctx.services
            .gradient_on_integration_points("displacement", gu, dim, ty, ghost, &filter)?;
        {
            let base = law.base_mut();
            if subtract_eigen {
                // `gradu_vect -= eigengradu` as in `computeAllStresses`
                let eigen = base.eigen_grad_u.get(ty, ghost)?;
                if eigen.len() != gu.len() || eigen.nb_component() != gu.nb_component() {
                    return Err(Error::DimensionMismatch {
                        context: "assembleStiffnessMatrix: grad_u -= eigen_grad_u",
                        expected: eigen.len(),
                        got: gu.len(),
                    });
                }
                for (g, e) in gu.as_mut_slice().iter_mut().zip(eigen.as_slice()) {
                    *g -= *e;
                }
            }
            base.grad_u.get_mut(ty, ghost)?.copy_from(gu)?;
        }
        let voigt = crate::material_tensor::tangent_stiffness_voigt_size(dim);
        // a zeroed tangent (upstream `tangent_stiffness_matrix.zero()`)
        let tangent = &mut scratch.tangent;
        if tangent.nb_component() != voigt * voigt {
            *tangent = Array::new(voigt * voigt);
        }
        tangent.try_resize(filter.len() * nq, 0.0)?;
        fe.exec().for_each(
            akantu_core::exec::Tuples::new(tangent.as_mut_slice(), 1),
            1 << 16,
            |_, mut t| t.as_mut_slice().fill(0.0),
        );
        law.compute_tangent_moduli(ctx, ty, ghost, tangent)?;
        let size = dim * nn;
        // `computeBtDB` then `integrate`, fused per element (the same
        // products, on the engine's threads)
        let mut k_e = scratch.take(size * size);
        let r = fe.compute_btdb_integrated(
            scratch.tangent.view(),
            &mut k_e,
            4,
            ty,
            ghost,
            Some(&filter),
        );
        if let Err(e) = r {
            scratch.give_back(k_e);
            return Err(e);
        }
        out.push((ty, filter, k_e));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::exec::{Exec, ExecPolicy, Grain};
    use akantu_mesh::MeshAccessor;

    /// Two triangles; nodal array of 4 nodes × 2 components.
    fn two_triangles() -> Mesh {
        let mut m = Mesh::new(2, "m").unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[0, 2, 3]);
        }
        m
    }

    fn run(
        exec: &Exec,
        mesh: &Mesh,
        r_e: &Array<Real>,
        ty: ElementType,
        filter: &[Idx],
    ) -> (Result<()>, Vec<Real>) {
        let mut f = akantu_core::SharedArray::new(Array::zeros(4, 2));
        let mut guard = f.write().unwrap();
        let mut scatters = std::collections::HashMap::new();
        let r = scatter_block(
            exec,
            mesh,
            r_e,
            &mut guard,
            ty,
            GhostType::NotGhost,
            filter,
            (0, ty, GhostType::NotGhost),
            &mut scatters,
        );
        (r, guard.as_slice().to_vec())
    }

    /// The threaded scatter returns what the serial routine returns: `Ok`
    /// for an empty block of a type the mesh does not have (before any
    /// access to its connectivity), the same error for a wrong number of
    /// element vectors or an element out of range, the same values
    /// otherwise.
    #[test]
    fn threaded_scatter_matches_the_serial_routine_on_edge_cases() {
        let mesh = two_triangles();
        let serial = Exec::serial();
        let threaded = Exec::new(
            ExecPolicy::serial()
                .with_threads(2)
                .unwrap()
                .with_grain(Grain::Finest),
        )
        .unwrap();
        let empty = Array::new(6);
        let one = Array::from_vec((0..6).map(Real::from).collect(), 6).unwrap();
        let two = Array::from_vec((0..12).map(Real::from).collect(), 6).unwrap();
        let cases: [(&Array<Real>, ElementType, &[Idx]); 4] = [
            (&empty, ElementType::Tetrahedron4, &[]),
            (&one, ElementType::Triangle3, &[0, 1]),
            (&one, ElementType::Triangle3, &[5]),
            (&two, ElementType::Triangle3, &[1, 0]),
        ];
        for (r_e, ty, filter) in cases {
            let (rs, vs) = run(&serial, &mesh, r_e, ty, filter);
            let (rt, vt) = run(&threaded, &mesh, r_e, ty, filter);
            assert_eq!(
                rs.as_ref().map_err(ToString::to_string),
                rt.as_ref().map_err(ToString::to_string),
                "{ty:?} {filter:?}"
            );
            if rs.is_ok() {
                assert_eq!(vs, vt);
            }
        }
    }
}
