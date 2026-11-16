// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model.cc (getStableTimeStep, getKineticEnergy,
// getExternalWork, getExternalWorkIncrement, getExternalWorkNew, getEnergy).

//! Energies and the stable time step (SMM-009…012).
//!
//! Each public method is one collective operation of the DOF manager: the
//! local part is computed (and its outcome agreed) before the rank
//! reduction (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches"), so a failure on one rank is reported on every rank.
//! Sums follow upstream's accumulation order exactly.

use akantu_comm::ReduceOp;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter,
};
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{LawContext, LawPhase};
use akantu_solver::DofManager;

use super::{
    DOF_ID, SolidMechanicsModel, SolidMechanicsPhysics, check_element_map, check_nodal,
    not_allocated,
};
use crate::material::Material;

/// `allReduce(op)` of one scalar through the running collective operation
/// of `dm`; the local value (or the local error) is agreed first.
pub(crate) fn all_reduce_in(
    dm: &mut DofManager,
    local: Result<Real>,
    op: ReduceOp,
) -> Result<Real> {
    let mut ph = dm.phases()?;
    ph.run_collective("smm:all_reduce", local.map(|x| [x]), |c, mut s| {
        c.all_reduce(&mut s, op)?;
        Ok(s[0])
    })
}

/// Upstream counting of a node in nodal sums: local or master, and not a
/// periodic slave.
fn counted(mesh: &Mesh, n: usize) -> Result<bool> {
    let f = mesh.node_flag(akantu_core::try_idx(n)?);
    Ok(f.is_local_or_master() && !f.is_periodic_slave())
}

/// Eigen `a.dot(b)` of two dynamic vectors (non-vectorised redux: first
/// product, then sequential additions).
fn dot(a: &[Real], b: &[Real]) -> Real {
    let mut it = a.iter().zip(b);
    let Some((a0, b0)) = it.next() else {
        return 0.0;
    };
    let mut r = a0 * b0;
    for (x, y) in it {
        r += x * y;
    }
    r
}

/// Buffers of the per-element energies, reused across the elements of a
/// group.
#[derive(Default)]
struct ElementEnergyScratch {
    vq: Option<Array<Real>>,
    rho: Vec<Real>,
    rho_v2: Vec<Real>,
}

impl SolidMechanicsPhysics {
    /// Upstream `getExternalWorkIncrement()`: `Σ_n c_n (f_ext·Δu − f_int·Δu)`
    /// with `c_n = 1` for counted nodes, `0` otherwise (local, no
    /// reduction).
    pub(crate) fn external_work_increment(&self, dofs: &DofManager) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let d = self.base.spatial_dimension;
        let inc = dofs
            .family(DOF_ID)
            .ok()
            .and_then(|f| f.increment.as_ref())
            .ok_or_else(|| not_allocated("displacement_increment"))?
            .read()?;
        let fe = self
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()?;
        let fi = self
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?;
        let nn = mesh.nb_nodes();
        check_nodal(
            "getExternalWorkIncrement: displacement_increment",
            &inc,
            nn,
            d,
        )?;
        check_nodal("getExternalWorkIncrement: external_force", &fe, nn, d)?;
        check_nodal("getExternalWorkIncrement: internal_force", &fi, nn, d)?;
        let term = |n: usize| -> Result<Real> {
            let c = if counted(&mesh, n)? { 1.0 } else { 0.0 };
            let i = &inc.as_slice()[n * d..(n + 1) * d];
            let a = dot(&fe.as_slice()[n * d..(n + 1) * d], i);
            let b = dot(&fi.as_slice()[n * d..(n + 1) * d], i);
            Ok(c * (a - b))
        };
        let exec = self.base.fems.exec();
        if !exec.is_parallel() {
            // the serial loop as before the threads (D-123)
            let mut work = 0.0;
            for n in 0..nn {
                let c = if counted(&mesh, n)? { 1.0 } else { 0.0 };
                let i = &inc.as_slice()[n * d..(n + 1) * d];
                let a = dot(&fe.as_slice()[n * d..(n + 1) * d], i);
                let b = dot(&fi.as_slice()[n * d..(n + 1) * d], i);
                work += c * (a - b);
            }
            return Ok(work);
        }
        // the per-node terms on the threads, their sum in node order (the
        // serial additions, D-123)
        let mut terms = vec![0.0; nn];
        exec.try_for_each(
            akantu_core::exec::Tuples::new(&mut terms, 1),
            131_072,
            |first, mut t| {
                for (k, v) in t.as_mut_slice().iter_mut().enumerate() {
                    *v = term(first + k)?;
                }
                Ok(())
            },
        )?;
        Ok(terms.iter().fold(0.0, |work, &t| work + t))
    }

    /// Local part of upstream `getStableTimeStep(_not_ghost)` (SMM-009):
    /// `min_e h_e / c_e`, `h_e` the inradius of the element on the current
    /// positions, `c_e` the material's celerity of the law-local element.
    ///
    /// A law's celerity may call back into foreign code
    /// (`ForeignMaterial::push_wave_speed`), and no native guard may be alive
    /// across such a callback (D-071, storage-migration guide P12): the
    /// element data are gathered with the guards alive
    /// ([`Self::stable_time_step_elements`]), then the celerities are queried
    /// without them. Same values and order as one element loop; the error of
    /// the first failing element is still the one returned (the celerities
    /// of the elements before it are queried first, as in one loop).
    fn stable_time_step_local(&mut self, dofs: &DofManager) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.update_current_position(&mesh, dofs)?;
        let mut elements = Vec::new();
        let gathered = self.stable_time_step_elements(&mesh, &mut elements);
        let ghost = GhostType::NotGhost;
        let mut min_dt = Real::MAX;
        for &(ty, el_h, law, local) in &elements {
            let el_c = self
                .materials
                .law(law)?
                .celerity(Element::new(ty, local, ghost))?;
            let el_dt = el_h / el_c;
            // std::min(min_dt, el_dt)
            if el_dt < min_dt {
                min_dt = el_dt;
            }
        }
        gathered?;
        Ok(min_dt)
    }

    /// The element loop of [`Self::stable_time_step_local`] up to the
    /// celerity: `(type, inradius on the current positions, law, law-local
    /// element)` of every regular `_not_ghost` element in upstream's order,
    /// pushed to `out` until the first error, which is returned.
    fn stable_time_step_elements(
        &self,
        mesh: &Mesh,
        out: &mut Vec<(ElementType, Real, usize, akantu_core::Idx)>,
    ) -> Result<()> {
        let dim = self.base.spatial_dimension;
        let cp = self
            .current_position
            .as_ref()
            .ok_or_else(|| not_allocated("current_position"))?
            .read()?;
        let ghost = GhostType::NotGhost;
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: ghost,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let nnpe = ty.nb_nodes_per_element();
            // upstream extracts the coordinates with `_not_ghost` whatever
            // the ghost type (only called with `_not_ghost`)
            let conn = mesh.connectivity(ty, GhostType::NotGhost)?;
            let idx = self.materials.law_index_map().get(ty, ghost)?;
            let loc = self.materials.local_numbering_map().get(ty, ghost)?;
            check_element_map("getStableTimeStep: material index", &idx, conn.len())?;
            check_element_map(
                "getStableTimeStep: material local numbering",
                loc,
                conn.len(),
            )?;
            out.reserve(conn.len());
            let mut x = vec![0.0; dim * nnpe];
            for e in 0..conn.len() {
                for (k, &node) in conn.tuple(e).iter().enumerate() {
                    let node = akantu_core::try_to_usize(node)?;
                    x[k * dim..(k + 1) * dim]
                        .copy_from_slice(&cp.as_slice()[node * dim..(node + 1) * dim]);
                }
                let law = akantu_core::decode_idx(idx.as_slice()[e])?.ok_or_else(|| {
                    Error::invalid(format!(
                        "element {e} of {ty} has no material (stable time step)"
                    ))
                })?;
                let el_h = akantu_fe::engine::element_inradius_of(ty, &x);
                out.push((ty, el_h, law, loc.as_slice()[e]));
            }
        }
        Ok(())
    }

    /// Local part of upstream `getKineticEnergy()` (SMM-010).
    fn kinetic_energy_local(&mut self, dofs: &mut DofManager) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let d = self.base.spatial_dimension;
        let mut ekin = 0.0;
        if dofs.has_lumped_matrix("M") {
            dofs.step("smm:assemble_lumped_matrix:M", |dm| {
                self.assemble_mass_lumped(dm)
            })?;
            let m = self
                .mass
                .as_ref()
                .ok_or_else(|| not_allocated("mass"))?
                .read()?;
            let v = dofs
                .dofs_derivative(DOF_ID, 1)
                .map_err(|_| not_allocated("velocity"))?;
            check_nodal("getKineticEnergy: mass", &m, mesh.nb_nodes(), d)?;
            check_nodal("getKineticEnergy: velocity", &v, mesh.nb_nodes(), d)?;
            for n in 0..mesh.nb_nodes() {
                let mut mv2 = 0.0;
                if counted(&mesh, n)? {
                    for i in 0..d {
                        let mi = m.as_slice()[n * d + i];
                        let vi = v.as_slice()[n * d + i];
                        if mi > Real::EPSILON {
                            mv2 += vi * vi * mi;
                        }
                    }
                }
                ekin += mv2;
            }
        } else if dofs.has_matrix("M") {
            dofs.step("smm:assemble_matrix:M", |dm| self.assemble_mass(dm))?;
            dofs.finalize_matrix("M")?;
            // the copy (needed by the product below) ends the guard before
            // `dofs` is borrowed mutably (explicit guard `Drop`)
            let v = {
                let v = dofs
                    .dofs_derivative(DOF_ID, 1)
                    .map_err(|_| not_allocated("velocity"))?;
                check_nodal("getKineticEnergy: velocity", &v, mesh.nb_nodes(), d)?;
                v.as_slice().to_vec()
            };
            let mut mv = Array::try_zeros(mesh.nb_nodes(), d)?;
            dofs.assemble_mat_mul_vect_to_array(DOF_ID, "M", &v, &mut mv, 1.0)?;
            for n in 0..mesh.nb_nodes() {
                let w = if mesh
                    .node_flag(akantu_core::try_idx(n)?)
                    .is_local_or_master()
                {
                    1.0
                } else {
                    0.0
                };
                ekin += dot(&v[n * d..(n + 1) * d], &mv.as_slice()[n * d..(n + 1) * d]) * w;
            }
        } else {
            return Err(Error::invalid(
                "No function called to assemble the mass matrix.",
            ));
        }
        Ok(ekin)
    }

    /// Local part of upstream `getExternalWork()` (SMM-011).
    fn external_work_local(&self, dofs: &DofManager) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let d = self.base.spatial_dimension;
        let f = dofs
            .family(DOF_ID)
            .map_err(|_| not_allocated("displacement"))?;
        let incr_or_velo = if self.base.method == AnalysisMethod::Static {
            f.increment
                .as_ref()
                .ok_or_else(|| not_allocated("displacement_increment"))?
        } else {
            f.derivatives
                .get(&1)
                .ok_or_else(|| not_allocated("velocity"))?
        }
        .read()?;
        let blocked = f
            .blocked
            .as_ref()
            .ok_or_else(|| not_allocated("blocked_dofs"))?
            .read()?;
        let fe = self
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()?;
        let fi = self
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?;
        let nn = mesh.nb_nodes();
        check_nodal(
            "getExternalWork: displacement increment / velocity",
            &incr_or_velo,
            nn,
            d,
        )?;
        check_nodal("getExternalWork: blocked_dofs", &blocked, nn, d)?;
        check_nodal("getExternalWork: external_force", &fe, nn, d)?;
        check_nodal("getExternalWork: internal_force", &fi, nn, d)?;
        let mut work = 0.0;
        for n in 0..nn {
            if !counted(&mesh, n)? {
                continue;
            }
            for i in 0..d {
                let k = n * d + i;
                if blocked.as_slice()[k] {
                    work -= fi.as_slice()[k] * incr_or_velo.as_slice()[k];
                } else {
                    work += fe.as_slice()[k] * incr_or_velo.as_slice()[k];
                }
            }
        }
        Ok(work)
    }

    /// Local part of the materials' energy `energy_id` (`Σ_m
    /// material.getEnergy(id)` from `0`).
    fn materials_energy_local(&mut self, dofs: &DofManager, energy_id: &str) -> Result<Real> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let (laws, services) = self.split(&mesh, dofs);
        let ctx = LawContext {
            phase: LawPhase::Energy,
            services: &services,
        };
        let mut energy = 0.0;
        for law in laws.laws_mut() {
            energy += law.energy_of(&ctx, energy_id)?;
        }
        Ok(energy)
    }
}

impl SolidMechanicsModel {
    /// Upstream `getStableTimeStep()` (SMM-009): the local minimum reduced
    /// with `min` over the ranks.
    pub fn stable_time_step(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        self.solver
            .dof_manager
            .collective("smm:stable_time_step", |dm| {
                let local = p.stable_time_step_local(dm);
                all_reduce_in(dm, local, ReduceOp::Min)
            })
    }

    /// Upstream `getKineticEnergy()`: lumped `½ Σ m v²` over counted nodes
    /// (components with `m ≤ ε` skipped), or consistent `½ Σ v·(M v)` over
    /// local-or-master nodes; summed over ranks.
    pub fn kinetic_energy(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        let e = self
            .solver
            .dof_manager
            .collective("smm:kinetic_energy", |dm| {
                let local = p.kinetic_energy_local(dm);
                all_reduce_in(dm, local, ReduceOp::Sum)
            })?;
        Ok(e / 2.)
    }

    /// Upstream `getExternalWork()` (`"external work"`): power of `f_ext`
    /// (free DOFs) and of the reactions `−f_int` (blocked DOFs) on the
    /// velocity (`Δu` for `_static`), summed over ranks, times `Δt` except
    /// for `_static`.
    pub fn external_work(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        let w = self
            .solver
            .dof_manager
            .collective("smm:external_work", |dm| {
                let local = p.external_work_local(dm);
                all_reduce_in(dm, local, ReduceOp::Sum)
            })?;
        if self.physics.base.method == AnalysisMethod::Static {
            Ok(w)
        } else {
            Ok(w * self.solver.time_step(None)?)
        }
    }

    /// Upstream `getExternalWorkNew()` (`"external work new"`): the work
    /// accumulated by `assembleResidual`, summed over ranks.
    pub fn external_work_new(&mut self) -> Result<Real> {
        let w = self.physics.external_work;
        self.solver
            .dof_manager
            .collective("smm:external_work_new", |dm| {
                all_reduce_in(dm, Ok(w), ReduceOp::Sum)
            })
    }

    /// Upstream `getEnergy(id)` (SMM-012): `kinetic`, `external work`,
    /// `external work new`; any other id is `Σ` over materials of
    /// `getEnergy(id)` (`potential`, dissipated, …), summed over ranks.
    pub fn energy(&mut self, energy_id: &str) -> Result<Real> {
        match energy_id {
            "kinetic" => self.kinetic_energy(),
            "external work" => self.external_work(),
            "external work new" => self.external_work_new(),
            _ => {
                let p = &mut self.physics;
                self.solver.dof_manager.collective("smm:energy", |dm| {
                    let local = p.materials_energy_local(dm, energy_id);
                    all_reduce_in(dm, local, ReduceOp::Sum)
                })
            }
        }
    }

    /// This rank's contribution to [`SolidMechanicsModel::energy`] before
    /// the reduction over the ranks (`kinetic`: `½ Σ m v²` over the local
    /// and master nodes; any other id: the sum over the laws), for the
    /// D-039 checks of distributed runs (the reduced value is the rank-order
    /// fold of these contributions). A collective operation of the DOF
    /// manager (every rank calls it), communicating nothing else.
    ///
    /// # Errors
    /// As [`SolidMechanicsModel::energy`]; `external work` ids are refused.
    pub fn local_energy(&mut self, energy_id: &str) -> Result<Real> {
        let p = &mut self.physics;
        match energy_id {
            "kinetic" => {
                let e = self
                    .solver
                    .dof_manager
                    .collective("smm:local_kinetic_energy", |dm| p.kinetic_energy_local(dm))?;
                Ok(e / 2.)
            }
            "external work" | "external work new" => Err(Error::invalid(format!(
                "local_energy: '{energy_id}' has no per-rank contribution here"
            ))),
            _ => self
                .solver
                .dof_manager
                .collective("smm:local_energy", |dm| {
                    p.materials_energy_local(dm, energy_id)
                }),
        }
    }

    /// Upstream `getKineticEnergy(element)`: `½ ∫_e ρ v·v` (velocity
    /// interpolated at the element's quadrature points).
    pub fn kinetic_energy_of_element(&mut self, element: Element) -> Result<Real> {
        self.kinetic_energy_of_element_with(element, &mut ElementEnergyScratch::default())
    }

    /// [`SolidMechanicsModel::kinetic_energy_of_element`] with buffers
    /// reused across elements (`energy_of_group` calls it
    /// per element).
    fn kinetic_energy_of_element_with(
        &mut self,
        element: Element,
        scratch: &mut ElementEnergyScratch,
    ) -> Result<Real> {
        let mesh_h = self.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let d = self.physics.base.spatial_dimension;
        let fe = self.physics.base.fems.get("")?;
        let v = self
            .solver
            .dof_manager
            .dofs_derivative(DOF_ID, 1)
            .map_err(|_| not_allocated("velocity"))?;
        check_nodal(
            "getKineticEnergy(element): velocity",
            &v,
            mesh.nb_nodes(),
            d,
        )?;
        let filter = [element.index()];
        let vq = scratch.vq.get_or_insert_with(|| Array::new(d));
        fe.interpolate_on_integration_points(
            &mesh,
            v.view(),
            vq,
            d,
            element.ty(),
            GhostType::NotGhost,
            Some(&filter),
        )?;
        let nq = vq.len();
        let law = self.physics.materials.law_of(element)?;
        let local = self
            .physics
            .materials
            .local_numbering_map()
            .element(element)?[0];
        scratch.rho.clear();
        scratch.rho.resize(nq, 0.0);
        law.rho_on_quadrature_points(
            Element::new(element.ty(), local, element.ghost_type()),
            &mut scratch.rho,
        );
        scratch.rho_v2.clear();
        for q in 0..nq {
            let vv = &vq.as_slice()[q * d..(q + 1) * d];
            scratch.rho_v2.push(scratch.rho[q] * dot(vv, vv));
        }
        let integral = fe.integrate_scalar(
            akantu_core::array::ArrayView::new(&scratch.rho_v2, 1)?,
            element.ty(),
            GhostType::NotGhost,
            Some(&filter),
        )?;
        Ok(0.5 * integral)
    }

    /// Upstream `getEnergy(id, element)`: `kinetic` by element, else the
    /// element's material `getEnergy(id, law-local element)`
    /// ([`Material::energy_of_element`]; `potential`: UD-032 corrected
    /// integrand).
    pub fn energy_of_element(&mut self, energy_id: &str, element: Element) -> Result<Real> {
        self.energy_of_element_with(energy_id, element, &mut ElementEnergyScratch::default())
    }

    fn energy_of_element_with(
        &mut self,
        energy_id: &str,
        element: Element,
        scratch: &mut ElementEnergyScratch,
    ) -> Result<Real> {
        if energy_id == "kinetic" {
            return self.kinetic_energy_of_element_with(element, scratch);
        }
        let i = akantu_core::decode_idx(
            self.physics
                .materials
                .law_index_map()
                .element_ref(element)?[0],
        )?
        .ok_or_else(|| Error::invalid(format!("{element} has no material")))?;
        let local = self
            .physics
            .materials
            .local_numbering_map()
            .element(element)?[0];
        let mat_element = Element::new(element.ty(), local, element.ghost_type());
        self.with_material(i, LawPhase::Energy, |law: &mut dyn Material, ctx| {
            law.energy_of_element(ctx, energy_id, mat_element)
        })
    }

    /// Upstream `getEnergy(id, group)`: sum over the group's elements (type
    /// order, element order), then over ranks.
    pub fn energy_of_group(&mut self, energy_id: &str, group: &str) -> Result<Real> {
        // Every local failure (unknown group, element without material, …)
        // becomes the local value of the agreed reduction: no early return
        // before the collective (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches").
        let elements: Result<Vec<Element>> = (|| {
            let mesh_h = self.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let g = mesh.groups().element_group(group)?;
            Ok(g.elements
                .as_map()
                .iter(GhostType::NotGhost)
                .flat_map(|(ty, a)| {
                    a.borrow()
                        .as_slice()
                        .iter()
                        .map(move |&i| Element::new(ty, i, GhostType::NotGhost))
                        .collect::<Vec<_>>()
                })
                .collect())
        })();
        let mut scratch = ElementEnergyScratch::default();
        let local = elements.and_then(|elements| {
            let mut acc = 0.0;
            for e in elements {
                acc += self.energy_of_element_with(energy_id, e, &mut scratch)?;
            }
            Ok(acc)
        });
        self.solver
            .dof_manager
            .collective("smm:energy_of_group", |dm| {
                all_reduce_in(dm, local, ReduceOp::Sum)
            })
    }
}
