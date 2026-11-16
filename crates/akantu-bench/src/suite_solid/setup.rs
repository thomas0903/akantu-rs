// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Model set-up cases
// of the benchmark suite (initFull, shape functions, mass and stiffness
// assembly of src/model/solid_mechanics/solid_mechanics_model_mass.cc and
// solid_mechanics_model.cc); no upstream test of their cost.

//! Set-up of a solid mechanics model: `init_full`, the shape-function
//! precomputation, the lumped and consistent mass, the stiffness matrix.

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Result};

use super::common;
use super::mesh::{BoxSpec, box_mesh};
use crate::suite::{Ctx, Outcome};

/// Set-up phases on a cube of `ty`.
///
/// Phases: `mesh` (generation), `init_explicit` (`init_full`, explicit
/// lumped mass), `shape_functions` (the FE engine's precomputation of
/// shapes, derivatives and `det J · w`), `mass_lumped`
/// (`assemble_mass_lumped` after `force_mass_reassembly`), `init_implicit`
/// (`init_full`, implicit dynamics: the matrix profiles), `mass_consistent`
/// (`assemble_mass` with reassembly), `stiffness_assembly`
/// (`assemble_stiffness_matrix` with reassembly).
///
/// # Errors
/// Model errors.
pub fn run(ctx: &mut Ctx<'_>, ty: ElementType, n: [usize; 3]) -> Result<Outcome> {
    let n = ctx.pick(n[0], n[1], n[2]);
    let spec = BoxSpec::unit(ty, n);
    let k = ctx.pick(1, 3, 3);
    let mesh = ctx.once("mesh", || box_mesh(&spec, None, true))?;
    let mut m = ctx.once("init_explicit", || {
        let mut m = common::solid_model(mesh, &common::elastic_dat(false))?;
        common::init(&mut m, AnalysisMethod::ExplicitLumpedMass)?;
        Ok(m)
    })?;
    common::set_counts(ctx, &m)?;
    ctx.repeat("shape_functions", k, |_| {
        let mesh_h = m.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        m.physics
            .base
            .fems
            .get_mut("")?
            .init_shape_functions(&mesh, GhostType::NotGhost)
    })?;
    ctx.repeat("mass_lumped", k, |_| {
        m.force_mass_reassembly();
        m.assemble_mass_lumped()
    })?;
    let mut h = crate::suite::Digest::new();
    h.reals(m.mass()?.as_slice());
    drop(m);
    // the same mesh again, untimed (meshes are not cloneable)
    let mesh2 = box_mesh(&spec, None, true)?;
    let mut m = ctx.once("init_implicit", || {
        let mut m = common::solid_model(mesh2, &common::elastic_dat(false))?;
        common::init(&mut m, AnalysisMethod::ImplicitDynamic)?;
        Ok(m)
    })?;
    ctx.repeat("mass_consistent", k, |_| {
        m.force_mass_reassembly();
        m.assemble_mass()
    })?;
    ctx.repeat("stiffness_assembly", k, |_| {
        m.assemble_stiffness_matrix(true)
    })?;
    {
        let backend = m.dof_manager().backend();
        for id in ["M", "K"] {
            let a = backend.aij(id)?;
            h.reals(a.values());
        }
    }
    ctx.set_digest(&h);
    Ok(Outcome::Done)
}

/// Set-up on tetrahedron_4.
///
/// # Errors
/// Model errors.
pub fn tet4(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    run(ctx, ElementType::Tetrahedron4, [3, 30, 60])
}

/// Set-up on tetrahedron_10.
///
/// # Errors
/// Model errors.
pub fn tet10(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    run(ctx, ElementType::Tetrahedron10, [2, 18, 36])
}

/// Set-up on hexahedron_8.
///
/// # Errors
/// Model errors.
pub fn hex8(ctx: &mut Ctx<'_>) -> Result<Outcome> {
    run(ctx, ElementType::Hexahedron8, [3, 40, 80])
}
