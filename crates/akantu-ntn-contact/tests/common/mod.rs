// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared helpers of the
// akantu-ntn-contact tests (ported from extra_packages/
// traction-at-split-node-contact/test/test_ntn_model/{test_ntn_fixture.hh,
// test_ntn_friction_fixture.hh}).

#![allow(dead_code)] // each test binary uses a subset of the helpers

pub mod oracle;

use std::path::{Path, PathBuf};

use akantu_core::Real;
use akantu_core::enums::MeshIoType;
use akantu_core::enums::SpatialDirection;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::boundary_condition::DirichletFunctor;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

/// A test input of `tests/data`.
pub fn data(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

/// The frozen Gmsh mesh of the rate-and-state tests (D-009).
pub fn ras_mesh_path() -> PathBuf {
    akantu_validation::fixtures_root().join(
        "meshes/gmsh-4.13.1/extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_ras.msh",
    )
}

/// `Mesh::read(path)`.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh).unwrap();
    mesh
}

/// A solid mechanics model on `mesh` with the input file `parser`.
pub fn model(mesh: Mesh, parser: Parser) -> SolidMechanicsModel {
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)
        .unwrap()
}

/// Upstream test `Velocity` Dirichlet functor: displacement increment
/// `vel · Δt` on the axis (the velocity itself is set by
/// [`apply_velocity_bc`], upstream sets it inside the functor).
pub struct VelocityIncrement {
    pub axis: usize,
    pub disp: Real,
}

impl DirichletFunctor for VelocityIncrement {
    fn apply(
        &self,
        _node: akantu_core::Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> akantu_core::Result<()> {
        primal[self.axis] += self.disp;
        flags[self.axis] = true;
        Ok(())
    }
}

/// Upstream `applyBC(Velocity(model, vel, ax), group)`.
pub fn apply_velocity_bc(model: &mut SolidMechanicsModel, group: &str, vel: Real, axis: usize) {
    let dt = model.time_step(None).unwrap();
    let f = VelocityIncrement {
        axis,
        disp: vel * dt,
    };
    model.apply_dirichlet_bc(&f, group).unwrap();
    let nodes: Vec<akantu_core::Idx> = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.groups().element_group(group).unwrap().nodes().to_vec()
    };
    let dim = model.spatial_dimension();
    let mut v = model.velocity_mut().unwrap();
    for n in nodes {
        v.as_mut_slice()[n as usize * dim + axis] = vel;
    }
}

/// The axis of an index.
pub fn axis(i: usize) -> SpatialDirection {
    match i {
        0 => SpatialDirection::X,
        1 => SpatialDirection::Y,
        _ => SpatialDirection::Z,
    }
}
