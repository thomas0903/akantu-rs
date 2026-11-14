// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.cc (assembleStiffnessMatrix, computeStresses,
// assembleInternalForce, assembleResidual),
// structural_mechanics_model_inline_impl.hh (assembleStiffnessMatrix<type>,
// computeStressOnQuad<type>), structural_mechanics_model_mass.cc
// (ComputeRhoFunctorStruct, assembleMassMatrix, assembleLumpedMassMatrix),
// src/fe_engine/fe_engine_template_tmpl_field.hh (assembleFieldMatrix,
// DOF-manager part).

//! Model-side assembly (STRUCT-006, 007, 009, 010).
//!
//! The stiffness, the stresses, the internal forces and the consistent mass
//! follow upstream's statement order and call the structural FE engine and
//! the DOF manager with upstream's operands, so that the assembled values
//! are bitwise those of the reference with the `eigen` backend (D-008) in
//! [`super::StructuralModelUpstreamCompat::UPSTREAM`] mode. The lumped mass
//! is the corrected one (UD-005: upstream's is identically zero) with the
//! rotary inertia of [`LumpedRotaryInertia`] (UD-059, D-059: by default the
//! exact nodal 3×3 block in 3-D; see
//! [`StructuralMechanicsPhysics::assemble_lumped_mass_matrix`]).

use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::MatrixType;
use akantu_core::{Array, ElementKind, ElementType, Error, GhostType, Real, Result, TypeFilter};
use akantu_fe::quadrature::gauss_rule;
use akantu_fe::structural::kernels::beam_lumped_mass_contributions;
use akantu_mesh::Mesh;
use akantu_model::model::alloc_nodal_field;
use akantu_solver::DofManager;

use super::{
    AssemblyInputs, DOF_ID, LumpedRotaryInertia, StructuralMechanicsPhysics, check_nodal,
    not_allocated, structural_types,
};
use crate::material::{checked_material_indices, nb_degree_of_freedom, tangent_moduli};

const NG: GhostType = GhostType::NotGhost;

impl StructuralMechanicsPhysics {
    /// Structural types of the model dimension (upstream
    /// `mesh.elementTypes(spatial_dimension, ghost, _ek_structural)`).
    fn model_types(&self, mesh: &Mesh, ghost: GhostType) -> Vec<ElementType> {
        mesh.element_types(TypeFilter {
            spatial_dimension: Some(self.base.spatial_dimension),
            ghost_type: ghost,
            kind: Some(ElementKind::Structural),
        })
        .collect()
    }

    /// The FE blocks of every structural type of the model dimension (not
    /// ghost) match the mesh (revision, element count, `extra_normal` bit
    /// pattern). Checked by every assembly **before** it decides that its
    /// matrix is current: a data-only `extra_normal`
    /// edit raises no mesh event, so matrices assembled from the old frames
    /// must not be reused; the error asks for
    /// [`super::StructuralMechanicsModel::refresh_geometry`].
    fn check_fe_current(&self, mesh: &Mesh) -> Result<()> {
        for ty in self.model_types(mesh, NG) {
            self.fe.check_up_to_date(mesh, ty, NG)?;
        }
        Ok(())
    }

    /// Inputs of an assembly of the model types (see [`AssemblyInputs`]).
    fn assembly_inputs(&self, mesh: &Mesh) -> AssemblyInputs {
        AssemblyInputs::of(
            &self.materials,
            &self.element_material,
            &self.model_types(mesh, NG),
        )
    }

    /// Generalised tangents of every quadrature point of `ty` (not ghost).
    fn tangents(&self, mesh: &Mesh, ty: ElementType) -> Result<Array<Real>> {
        let nq = self.fe.nb_integration_points(ty, NG)?;
        let map = self.element_material.get(ty, NG).map_err(|_| {
            Error::invalid(format!(
                "no element_material block for {} (call init_full first)",
                ty.name()
            ))
        })?;
        tangent_moduli(
            ty,
            &self.materials,
            &map,
            mesh.nb_element(ty, NG),
            nq,
            self.compat.dkt_bending,
        )
    }

    /// The displacement-rotation array, checked against the mesh.
    fn checked_displacement<'d>(
        &self,
        mesh: &Mesh,
        dofs: &'d DofManager,
    ) -> Result<ArrayRef<'d, Real>> {
        let u = dofs
            .dofs(DOF_ID)
            .map_err(|_| not_allocated("displacement"))?;
        check_nodal(
            "structural model: displacement",
            &*u,
            mesh.nb_nodes(),
            self.nb_degree_of_freedom,
        )?;
        Ok(u)
    }

    /// Upstream `assembleStiffnessMatrix()` (STRUCT-006): after checking
    /// that the FE blocks match the mesh, when needed (first call,
    /// or a material / element → material input differs bitwise from the
    /// last assembly, UD-508), create `K` (symmetric) if
    /// missing, zero it, and per structural type of the model dimension
    /// assemble `∫ (B·T)ᵀ D (B·T)` (symmetric elemental matrices).
    pub(crate) fn assemble_stiffness_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.check_fe_current(&mesh)?;
        let inputs = self.assembly_inputs(&mesh);
        if self.assembled_stiffness.as_ref() == Some(&inputs) && dofs.has_matrix("K") {
            return Ok(());
        }
        if !dofs.has_matrix("K") {
            dofs.new_matrix("K", MatrixType::Symmetric)?;
        }
        // validate everything before the matrix is touched
        let mut blocks = Vec::new();
        for ty in self.model_types(&mesh, NG) {
            let d = self.tangents(&mesh, ty)?;
            let mut k_e = Array::new(1);
            self.fe
                .element_stiffness(&mesh, d.view(), &mut k_e, ty, NG)?;
            blocks.push((ty, k_e));
        }
        dofs.zero_matrix("K")?;
        for (ty, k_e) in blocks {
            dofs.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                DOF_ID,
                k_e.view(),
                ty,
                NG,
                MatrixType::Symmetric,
                None,
            )?;
        }
        self.assembled_stiffness = Some(inputs);
        Ok(())
    }

    /// Upstream `computeStresses()` (STRUCT-007): `σ_q = (D_q·(B·T)_q)·u_e`
    /// for every structural type of the model dimension (not ghost).
    pub(crate) fn compute_stresses(&mut self, dofs: &DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let u = self.checked_displacement(&mesh, dofs)?;
        for ty in self.model_types(&mesh, NG) {
            let d = self.tangents(&mesh, ty)?;
            let sigma = self.stress.get_mut(ty, NG).map_err(|_| {
                Error::invalid(format!(
                    "no stress block for {} (call init_full first)",
                    ty.name()
                ))
            })?;
            self.fe
                .compute_stresses(&mesh, d.view(), u.view(), sigma, ty, NG)?;
        }
        Ok(())
    }

    /// Upstream `assembleInternalForce()` (STRUCT-007): zero `f_int`,
    /// compute the stresses, then for every structural type (all
    /// dimensions, not ghost) `f_int −= ∫ (B·T)ᵀ σ`.
    pub(crate) fn assemble_internal_force(&mut self, dofs: &DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        {
            let f = self
                .internal_force
                .as_ref()
                .ok_or_else(|| not_allocated("internal_force"))?
                .read()?;
            check_nodal(
                "assembleInternalForce: internal_force",
                &f,
                mesh.nb_nodes(),
                self.nb_degree_of_freedom,
            )?;
        }
        self.compute_stresses(dofs)?;
        let mut blocks = Vec::new();
        for ty in structural_types(&mesh, NG) {
            let sigma = self.stress.get(ty, NG)?;
            let mut btd = Array::new(1);
            self.fe
                .compute_btd(&mesh, sigma.view(), &mut btd, ty, NG, None)?;
            let nc = btd.nb_component();
            let mut int_btd = Array::new(nc);
            self.fe
                .integrate(&mesh, btd.view(), &mut int_btd, nc, ty, NG, None)?;
            blocks.push((ty, int_btd));
        }
        let mut f = self
            .internal_force
            .as_mut()
            .ok_or_else(|| not_allocated("internal_force"))?
            .write()?;
        f.zero();
        for (ty, int_btd) in blocks {
            DofManager::assemble_elemental_array_local_view(
                &mesh,
                int_btd.view(),
                f.view_mut(),
                ty,
                NG,
                -1.0,
                None,
            )?;
        }
        Ok(())
    }

    /// Upstream `assembleResidual()` (STRUCT-007): internal forces, then
    /// residual `+= f_ext` and `+= f_int` (both with coefficient `+1`).
    pub(crate) fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        {
            let nn = self.base.mesh.read()?.nb_nodes();
            let fe = self
                .external_force
                .as_ref()
                .ok_or_else(|| not_allocated("external_force"))?
                .read()?;
            check_nodal(
                "assembleResidual: external_force",
                &fe,
                nn,
                self.nb_degree_of_freedom,
            )?;
        }
        self.assemble_internal_force(dofs)?;
        let fe = self
            .external_force
            .as_ref()
            .ok_or_else(|| not_allocated("external_force"))?
            .read()?;
        dofs.assemble_to_residual(DOF_ID, fe.view(), 1.0)?;
        drop(fe);
        let fi = self
            .internal_force
            .as_ref()
            .ok_or_else(|| not_allocated("internal_force"))?
            .read()?;
        dofs.assemble_to_residual(DOF_ID, fi.view(), 1.0)
    }

    /// The mass field of `ty` (`nb_element · nb_quad_mass` tuples of
    /// `nb_stress` values): upstream `ComputeRhoFunctorStruct` sets every
    /// component to the mass per unit length `ρ·A`; for `_bernoulli_beam_3`
    /// the torsion row (`θx`, component 3) gets `ρ·(Iy + Iz)` unless
    /// [`super::StructuralModelUpstreamCompat::beam3_torsional_mass_rho_a`]
    /// (UD-507).
    fn mass_field(&self, mesh: &Mesh, ty: ElementType) -> Result<Array<Real>> {
        let ns = crate::material::nb_stress_components(ty)?;
        let nq = gauss_rule(ty, self.fe.integration_orders().mass_order(ty))?.nb_points();
        let ne = mesh.nb_element(ty, NG);
        let map = self.element_material.get(ty, NG).map_err(|_| {
            Error::invalid(format!(
                "no element_material block for {} (call init_full first)",
                ty.name()
            ))
        })?;
        checked_material_indices(&map, ne, self.materials.len(), ty)?;
        let torsion_rho_ip =
            ty == ElementType::BernoulliBeam3 && !self.compat.beam3_torsional_mass_rho_a;
        let mut field = Array::try_zeros(
            ne.checked_mul(nq)
                .ok_or_else(|| Error::invalid("mass field size overflow"))?,
            ns,
        )?;
        let fs = field.as_mut_slice();
        for (e, &mi) in map.as_slice().iter().enumerate() {
            let m = &self.materials[mi as usize];
            // const Real mat_mass_per_length = mat_rho * mat_A;
            let mass_per_length = m.rho * m.a;
            let torsion = if torsion_rho_ip {
                m.rho * (m.iy + m.iz)
            } else {
                mass_per_length
            };
            for q in 0..nq {
                let row = &mut fs[(e * nq + q) * ns..(e * nq + q + 1) * ns];
                row.fill(mass_per_length);
                if torsion_rho_ip {
                    row[3] = torsion;
                }
            }
        }
        Ok(field)
    }

    /// Upstream `assembleMassMatrix()` (STRUCT-009): when needed, create
    /// `M` (symmetric) if missing, zero it and assemble, per structural type
    /// of the model dimension (not ghost), `∫ (N_m·T)ᵀ diag(ρ) (N_m·T)` on
    /// the mass rule (`assembleFieldMatrix`, symmetric elemental matrices).
    pub(crate) fn assemble_mass_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        self.check_fe_current(&mesh)?;
        let inputs = self.assembly_inputs(&mesh);
        if self.assembled_mass.as_ref() == Some(&inputs) && dofs.has_matrix("M") {
            return Ok(());
        }
        if !dofs.has_matrix("M") {
            dofs.new_matrix("M", MatrixType::Symmetric)?;
        }
        let mut blocks = Vec::new();
        for ty in self.model_types(&mesh, NG) {
            let field = self.mass_field(&mesh, ty)?;
            let mut m_e = Array::new(1);
            self.fe
                .consistent_mass(&mesh, field.view(), &mut m_e, ty, NG)?;
            blocks.push((ty, m_e));
        }
        dofs.zero_matrix("M")?;
        for (ty, m_e) in blocks {
            dofs.assemble_elemental_matrices_to_matrix(
                &mesh,
                "M",
                DOF_ID,
                m_e.view(),
                ty,
                NG,
                MatrixType::Symmetric,
                None,
            )?;
        }
        self.assembled_mass = Some(inputs);
        Ok(())
    }

    /// Upstream `assembleLumpedMassMatrix()` (STRUCT-010), corrected
    /// (UD-005) with the rotary inertia of
    /// [`super::StructuralModelUpstreamCompat::lumped_rotary_inertia`]
    /// (UD-059, D-059).
    ///
    /// When needed: allocate the nodal `mass` (one value per DOF) and
    /// `rotary_inertia` (`nr × nr` per node), create and zero the lumped
    /// matrix `M`, then for every Bernoulli beam of the model dimension
    /// (other structural types: `Error::NotImplemented`, as upstream) with
    /// `L = |x₂ − x₁|`, `m = (L·A)·ρ`, `J_b = m·L²/78`, `J_t = ρ·(Iy + Iz)·L/2`:
    ///
    /// * translations: `m/2` on every translational DOF of **both** nodes
    ///   (upstream uses the first node twice, `node2 = conn(0)`, so its
    ///   masses are zero, and in 3-D overwrites the translational masses by
    ///   the rotational inertia; UD-005);
    /// * rotations ([`LumpedRotaryInertia`]):
    ///   * `Block3x3` (default): 2-D `J_b` per node; 3-D the element block
    ///     `Tₑᵀ diag(J_t, J_b, J_b) Tₑ` (`Tₑ` the element's global → member
    ///     rotation of the FE engine, upper triangle computed once and
    ///     mirrored) added to both nodes;
    ///   * `IsotropicMax`: 2-D `J_b`; 3-D `max(J_t, J_b)` on the three
    ///     rotations of both nodes (diagonal, the interim rule);
    ///
    /// The rotational entries of `mass` are the diagonal of the nodal
    /// blocks. `M` receives `mass` and, for `Block3x3` in 3-D, the
    /// off-diagonal entries of the blocks as block couplings of the DOF
    /// manager (`(θx,θy), (θx,θz), (θy,θz)` per node; block product in the
    /// residual and block solve of the free sub-block in the explicit
    /// lumped solve).
    ///
    /// Every free (unblocked) DOF must get a positive, finite entry and, for
    /// `Block3x3` in 3-D, the free rotary sub-block of every node must be
    /// numerically positive definite (every `LDLᵀ` pivot above
    /// [`super::ROTARY_BLOCK_DEFINITENESS`] times its largest diagonal
    /// entry): `ρ·A = 0` (upstream's default `ρ = 0`), a node without any
    /// beam, or (`Block3x3`) a node whose beams are collinear with no polar
    /// moment is refused with `Error::Numerical` naming the node
    /// (upstream divides by its zero mass). The check runs against
    /// the **current** blocked flags at every call, also when the assembled
    /// mass is reused (UD-518).
    pub(crate) fn assemble_lumped_mass_matrix(&mut self, dofs: &mut DofManager) -> Result<()> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        // the lumped mass reads the node coordinates directly; the engine's
        // stamps tell whether the mesh changed since the geometry was last
        // processed (refuse instead of reusing an old mass)
        self.check_fe_current(&mesh)?;
        let inputs = self.assembly_inputs(&mesh);
        let dim = self.base.spatial_dimension;
        let ndof = self.nb_degree_of_freedom;
        let nr = ndof - dim;
        let rule = self.compat.lumped_rotary_inertia;
        let coupled = rule == LumpedRotaryInertia::Block3x3 && nr == 3;
        if self.assembled_lumped_mass.as_ref() == Some(&inputs)
            && dofs.has_lumped_matrix("M")
            && (!coupled || dofs.lumped_block_couplings("M").is_some())
            && let (Some(mass), Some(rotary)) = (self.mass.as_ref(), self.rotary_inertia.as_ref())
        {
            // the blocked flags are not an assembly input: a DOF unblocked
            // since the assembly must not reach the lumped solve with a
            // zero or non-finite mass
            check_free_lumped_entries(&mesh, dofs, mass, rotary, ndof, coupled)?;
            return Ok(());
        }
        // validate before any state change
        let types = self.model_types(&mesh, NG);
        for &ty in &types {
            if ty != ElementType::BernoulliBeam2 && ty != ElementType::BernoulliBeam3 {
                return Err(Error::NotImplemented(
                    "The lumped mass was not implemented for non Bernoulli Beams",
                ));
            }
            let map = self.element_material.get(ty, NG).map_err(|_| {
                Error::invalid(format!(
                    "no element_material block for {} (call init_full first)",
                    ty.name()
                ))
            })?;
            checked_material_indices(&map, mesh.nb_element(ty, NG), self.materials.len(), ty)?;
        }
        check_nodal(
            "assembleLumpedMassMatrix: mesh nodes",
            &*mesh.nodes(),
            mesh.nb_nodes(),
            dim,
        )?;
        let nn = mesh.nb_nodes();
        let mut lumped: Array<Real> = Array::try_zeros(nn, ndof)?;
        let mut rotary: Array<Real> = Array::try_zeros(nn, nr * nr)?;
        {
            let lm = lumped.as_mut_slice();
            let jr = rotary.as_mut_slice();
            let x_g = mesh.nodes();
            let x = x_g.as_slice();
            for &ty in &types {
                debug_assert_eq!(nb_degree_of_freedom(ty)?, ndof);
                let conn = mesh.connectivity(ty, NG)?;
                let map = self.element_material.get(ty, NG)?;
                // member frames of the element blocks (Tₑ: ne tuples of
                // ndof² values, column-major: T[k][i] = t[k + i·ndof])
                let frames = if coupled {
                    let r = self.fe.rotations(&mesh, ty, NG)?;
                    check_nodal(
                        "assembleLumpedMassMatrix: element rotations",
                        r,
                        conn.len(),
                        ndof * ndof,
                    )?;
                    Some(r.as_slice())
                } else {
                    None
                };
                for e in 0..conn.len() {
                    let nodes = conn.tuple(e);
                    let (n1, n2) = (
                        akantu_core::try_to_usize(nodes[0])?,
                        akantu_core::try_to_usize(nodes[1])?,
                    );
                    if n1 >= nn || n2 >= nn {
                        return Err(Error::OutOfRange {
                            what: "node of a beam",
                            index: n1.max(n2),
                            len: nn,
                        });
                    }
                    let m = &self.materials[map.as_slice()[e] as usize];
                    let x1 = &x[n1 * dim..(n1 + 1) * dim];
                    let x2 = &x[n2 * dim..(n2 + 1) * dim];
                    let (half_mass, _) = beam_lumped_mass_contributions(x1, x2, m.rho, m.a)?;
                    let length = akantu_fe::eigen::norm(dim, false, |i| x1[i] - x2[i]);
                    let mass = half_mass * 2.;
                    let j_bending = mass * length * length / 78.;
                    // the element's nodal rotary block (nr × nr, row-major)
                    let mut block = [0.0; 9];
                    match (rule, frames) {
                        _ if nr == 1 => block[0] = j_bending,
                        (LumpedRotaryInertia::IsotropicMax, _) => {
                            let j_torsion = m.rho * (m.iy + m.iz) * length / 2.;
                            // explicit selection (no f64::max: NaN and ±0
                            // stay visible to the check below)
                            let j = if j_torsion > j_bending {
                                j_torsion
                            } else {
                                j_bending
                            };
                            block[0] = j;
                            block[4] = j;
                            block[8] = j;
                        }
                        (LumpedRotaryInertia::Block3x3, Some(frames)) => {
                            let j_torsion = m.rho * (m.iy + m.iz) * length / 2.;
                            let t = &frames[e * ndof * ndof..(e + 1) * ndof * ndof];
                            for i in 0..3 {
                                for j in i..3 {
                                    // Σ_k J_k T[k][i] T[k][j], J = (J_t, J_b, J_b)
                                    let v = j_torsion * (t[i * ndof] * t[j * ndof])
                                        + j_bending
                                            * (t[1 + i * ndof] * t[1 + j * ndof]
                                                + t[2 + i * ndof] * t[2 + j * ndof]);
                                    block[i * 3 + j] = v;
                                    block[j * 3 + i] = v;
                                }
                            }
                        }
                        (LumpedRotaryInertia::Block3x3, None) => {
                            return Err(Error::invalid(
                                "lumped rotary block without member frames",
                            ));
                        }
                    }
                    for n in [n1, n2] {
                        for d in 0..dim {
                            lm[n * ndof + d] += half_mass;
                        }
                        let dst = &mut jr[n * nr * nr..(n + 1) * nr * nr];
                        for (d, v) in dst.iter_mut().zip(&block) {
                            *d += *v;
                        }
                    }
                }
            }
            // the rotational entries of the per-DOF mass: the block diagonal
            for n in 0..nn {
                for i in 0..nr {
                    lm[n * ndof + dim + i] = jr[n * nr * nr + i * nr + i];
                }
            }
        }
        // every free DOF needs a positive, finite lumped entry and every free
        // rotary sub-block must be positive definite (the explicit lumped
        // solve divides by / solves with it)
        check_free_lumped_entries(&mesh, dofs, &lumped, &rotary, ndof, coupled)?;
        // the off-diagonal entries of the blocks, before any state change
        let couplings = if coupled {
            let mut off: Array<Real> = Array::try_zeros(nn, 3)?;
            let jr = rotary.as_slice();
            for (n, o) in off
                .as_mut_slice()
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .enumerate()
            {
                let b = &jr[n * 9..(n + 1) * 9];
                o.copy_from_slice(&[b[1], b[2], b[5]]);
            }
            Some(off)
        } else {
            None
        };
        alloc_nodal_field(&mut self.mass, nn, ndof)?;
        alloc_nodal_field(&mut self.rotary_inertia, nn, nr * nr)?;
        if !dofs.has_lumped_matrix("M") {
            dofs.new_lumped_matrix("M")?;
        }
        // also drops the block couplings of a previous assembly
        dofs.zero_lumped_matrix("M")?;
        let rot = self
            .rotary_inertia
            .as_mut()
            .ok_or_else(|| not_allocated("rotary_inertia"))?;
        check_nodal("assembleLumpedMassMatrix: rotary_inertia", rot, nn, nr * nr)?;
        rot.copy_from(&rotary)?;
        let mass = self
            .mass
            .as_mut()
            .ok_or_else(|| not_allocated("lumped_mass"))?;
        check_nodal("assembleLumpedMassMatrix: lumped_mass", mass, nn, ndof)?;
        mass.copy_from(&lumped)?;
        dofs.assemble_to_lumped_matrix(DOF_ID, mass.view(), "M", 1.0)?;
        if let Some(off) = couplings {
            dofs.assemble_lumped_block_couplings(DOF_ID, "M", dim, off.view(), 1.0)?;
        }
        self.assembled_lumped_mass = Some(inputs);
        Ok(())
    }
}

/// Every free (unblocked) DOF has a positive, finite lumped entry, against
/// the **current** blocked flags: the
/// explicit lumped solve divides each DOF's residual by its entry. Blocked
/// DOFs may carry a zero entry (their acceleration is prescribed), but no
/// entry may be non-finite. With `coupled` (`Block3x3` in 3-D, D-059) every
/// rotary entry must be finite too and the free rotary sub-block of every
/// node must be numerically positive definite (every `LDLᵀ` pivot above
/// [`super::ROTARY_BLOCK_DEFINITENESS`] times its largest diagonal entry):
/// the explicit lumped solve solves with it.
fn check_free_lumped_entries(
    mesh: &Mesh,
    dofs: &DofManager,
    lumped: &Array<Real>,
    rotary: &Array<Real>,
    ndof: usize,
    coupled: bool,
) -> Result<()> {
    let nn = mesh.nb_nodes();
    let blocked = dofs
        .blocked_dofs(DOF_ID)
        .map_err(|_| not_allocated("blocked_dofs"))?;
    check_nodal(
        "assembleLumpedMassMatrix: blocked_dofs",
        &*blocked,
        nn,
        ndof,
    )?;
    check_nodal("assembleLumpedMassMatrix: lumped_mass", lumped, nn, ndof)?;
    let bl = blocked.as_slice();
    let bad = lumped
        .as_slice()
        .iter()
        .zip(bl)
        .position(|(&m, &b)| !m.is_finite() || (!b && m <= 0.0));
    if let Some(k) = bad {
        return Err(Error::Numerical(format!(
            "lumped mass of node {} DOF {} is {:e}: every free DOF needs a positive, \
             finite lumped mass (check rho and A of the materials, and that every free \
             node belongs to a beam; upstream divides by its zero mass)",
            k / ndof,
            k % ndof,
            lumped.as_slice()[k]
        )));
    }
    if !coupled {
        return Ok(());
    }
    check_nodal("assembleLumpedMassMatrix: rotary_inertia", rotary, nn, 9)?;
    let jr = rotary.as_slice();
    if let Some(k) = jr.iter().position(|v| !v.is_finite()) {
        return Err(Error::Numerical(format!(
            "rotary inertia block of node {} has a non-finite entry {:e}",
            k / 9,
            jr[k]
        )));
    }
    let first = ndof - 3;
    for n in 0..nn {
        let mut free = [0usize; 3];
        let mut k = 0;
        for i in 0..3 {
            if !bl[n * ndof + first + i] {
                free[k] = i;
                k += 1;
            }
        }
        if k < 2 {
            // a single free rotation: its positive diagonal entry (above)
            continue;
        }
        let b = &jr[n * 9..(n + 1) * 9];
        let mut m = [[0.0; 3]; 3];
        let mut largest: Real = 0.0;
        for p in 0..k {
            for q in 0..k {
                m[p][q] = b[free[p] * 3 + free[q]];
            }
            largest = largest.max(m[p][p]);
        }
        let threshold = super::ROTARY_BLOCK_DEFINITENESS * largest;
        let pivots = ldlt_pivots(&m, k);
        if let Some(p) = pivots[..k]
            .iter()
            .position(|&d| d.is_nan() || d <= threshold)
        {
            return Err(Error::Numerical(format!(
                "rotary inertia block of node {n} (free rotations {:?}) is not positive \
                 definite: LDLᵀ pivot {p} is {:e} (largest diagonal entry {largest:e}); a \
                 node whose beams are collinear needs a polar moment (Iy + Iz > 0) with the \
                 Block3x3 rotary inertia, or use IsotropicMax",
                &free[..k],
                pivots[p]
            )));
        }
    }
    Ok(())
}

/// `LDLᵀ` pivots `d` of a symmetric `k × k` matrix (`k ≤ 3`, lower triangle
/// read); a non-positive pivot ends the factorisation (the later entries
/// are left at zero, failing the caller's test as well).
fn ldlt_pivots(m: &[[Real; 3]; 3], k: usize) -> [Real; 3] {
    let mut l = [[0.0; 3]; 3];
    let mut d = [0.0; 3];
    for i in 0..k {
        for j in 0..i {
            let mut s = m[i][j];
            for p in 0..j {
                s -= l[i][p] * d[p] * l[j][p];
            }
            l[i][j] = s / d[j];
        }
        let mut s = m[i][i];
        for p in 0..i {
            s -= l[i][p] * d[p] * l[i][p];
        }
        d[i] = s;
        if s.is_nan() || s <= 0.0 {
            break;
        }
    }
    d
}
