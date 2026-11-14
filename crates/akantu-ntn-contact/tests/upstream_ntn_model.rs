// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent,
// upstream tolerances) of extra_packages/traction-at-split-node-contact/test/
// test_ntn_model/{test_ntn_basics.cc, test_ntn_fixture.hh}: the gtest suite
// TestNTNFixture<dim> (TestNodesPairing, TestLumpedBoundary, TestImpedance)
// for dim = 2 and 3, serial (the fixture's partitioning by x is the
// identity on one rank).

mod common;

use akantu_core::Real;
use akantu_core::enums::AnalysisMethod;
use akantu_model::ModelOptions;
use akantu_ntn_contact::{ModelState, NtnBaseContact, TasnUpstreamCompat};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use common::*;

struct Fixture {
    model: SolidMechanicsModel,
    contact: NtnBaseContact,
    dim: usize,
}

/// Upstream `TestNTNFixture::SetUp`.
fn set_up(dim: usize) -> Fixture {
    let mesh = read_mesh(&data(&format!("ntn_test_{dim}d.msh")), dim);
    let parser = Parser::parse_file(data("material_ntn_model.dat")).unwrap();
    let mut model = model(mesh, parser);
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let mesh = mesh_h.read().unwrap();
        NtnBaseContact::new_ntn(&mesh, dim, "contact", TasnUpstreamCompat::default()).unwrap()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    {
        let mesh = mesh_h.read().unwrap();
        let state = ModelState::of_model(&mesh, &mut model).unwrap();
        contact
            .add_surface_pair(&state, "slider_bottom", "base_top", 1)
            .unwrap();
        contact.update_normals(&state).unwrap();
        contact.update_lumped_boundary(&mesh).unwrap();
    }
    Fixture {
        model,
        contact,
        dim,
    }
}

impl Fixture {
    /// Upstream `initDynamics(time_step_factor)`.
    fn init_dynamics(&mut self, time_step_factor: Real) {
        let stable = self.model.stable_time_step().unwrap();
        self.model
            .set_time_step(stable * time_step_factor, None)
            .unwrap();
        let mesh_h = self.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let state = ModelState::of_model(&mesh, &mut self.model).unwrap();
        self.contact.update_impedance(&state).unwrap();
    }

    fn check_nodes_pairing(&self) {
        let mesh_h = self.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let pos = mesh.nodes();
        let pairs = self.contact.node_pairs().unwrap();
        assert!(!pairs.is_empty());
        for (s, m) in pairs {
            for d in 0..self.dim {
                let distance = pos.tuple(s as usize)[d] - pos.tuple(m as usize)[d];
                assert!(
                    distance.abs() <= 1e-10,
                    "pair ({s}, {m}) distance {distance}"
                );
            }
        }
    }

    fn check_lumped_boundary(&self) {
        let mesh_h = self.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let pos = mesh.nodes();
        let lower = mesh.lower_bounds().to_vec();
        let upper = mesh.upper_bounds().to_vec();
        let nb_contact_nodes = self.contact.nb_contact_nodes();
        let (x_min, x_max) = (lower[0], upper[0]);
        let size_x = x_max - x_min;
        let tol = 1e-11;
        let is_bound = |p: &[Real], dir: usize, min: Real, max: Real| {
            (p[dir] - min).abs() < tol || (p[dir] - max).abs() < tol
        };
        let ls = self.contact.lumped_boundary_slaves();
        let lm = self.contact.lumped_boundary_masters().unwrap();
        let pairs = self.contact.node_pairs().unwrap();
        match self.dim {
            2 => {
                let l_b_bulk = size_x / (nb_contact_nodes as Real - 1.0);
                let l_b_side = l_b_bulk / 2.0;
                let b = |bound: bool| if bound { l_b_side } else { l_b_bulk };
                for (i, (s, m)) in pairs.into_iter().enumerate() {
                    let sp = pos.tuple(s as usize);
                    let mp = pos.tuple(m as usize);
                    let e_s = b(is_bound(sp, 0, x_min, x_max));
                    assert!(
                        (e_s - ls.at(i)).abs() <= 1e-11,
                        "slave {s}: {} vs {e_s}",
                        ls.at(i)
                    );
                    let e_m = b(is_bound(mp, 0, x_min, x_max));
                    assert!(
                        (e_m - lm.at(i)).abs() <= 1e-11,
                        "master {m}: {} vs {e_m}",
                        lm.at(i)
                    );
                }
            }
            3 => {
                // upstream: Int nb_contact_per_sides = std::sqrt(nb_contact_nodes)
                let per_side = (nb_contact_nodes as Real).sqrt() as usize;
                let (z_min, z_max) = (lower[2], upper[2]);
                let size_z = z_max - z_min;
                let l_b_bulk = size_x * size_z / ((per_side - 1) * (per_side - 1)) as Real;
                let l_b_side = l_b_bulk / 2.0;
                let l_b_corner = l_b_bulk / 4.0;
                let b = |bx: bool, bz: bool| match (bx, bz) {
                    (true, true) => l_b_corner,
                    (true, false) | (false, true) => l_b_side,
                    (false, false) => l_b_bulk,
                };
                for (i, (s, m)) in pairs.into_iter().enumerate() {
                    let sp = pos.tuple(s as usize);
                    let mp = pos.tuple(m as usize);
                    let e_s = b(is_bound(sp, 0, x_min, x_max), is_bound(sp, 2, z_min, z_max));
                    assert!(
                        (e_s - ls.at(i)).abs() <= 1e-11,
                        "slave {s}: {} vs {e_s}",
                        ls.at(i)
                    );
                    let e_m = b(is_bound(mp, 0, x_min, x_max), is_bound(mp, 2, z_min, z_max));
                    assert!(
                        (e_m - lm.at(i)).abs() <= 1e-11,
                        "master {m}: {} vs {e_m}",
                        lm.at(i)
                    );
                }
            }
            _ => unreachable!(),
        }
    }

    fn check_impedance(&self) {
        let delta_t = self.model.time_step(None).unwrap();
        let mesh_h = self.model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let system_height = mesh.upper_bounds()[1] - mesh.lower_bounds()[1];
        let density = 1.0;
        // 1/[dt/2 (A_s/m_s + A_m/m_m)] reduces to ρ h / (4 Δt)
        let imp_ref = density * system_height / (4.0 * delta_t);
        assert!(self.contact.nb_contact_nodes() > 0);
        for &imp in self.contact.impedance().as_slice() {
            assert!((imp - imp_ref).abs() <= 1e-11, "{imp} vs {imp_ref}");
        }
    }
}

#[test]
fn test_ntn_fixture_nodes_pairing_2d() {
    set_up(2).check_nodes_pairing();
}

#[test]
fn test_ntn_fixture_nodes_pairing_3d() {
    set_up(3).check_nodes_pairing();
}

#[test]
fn test_ntn_fixture_lumped_boundary_2d() {
    set_up(2).check_lumped_boundary();
}

#[test]
fn test_ntn_fixture_lumped_boundary_3d() {
    set_up(3).check_lumped_boundary();
}

#[test]
fn test_ntn_fixture_impedance_2d() {
    let mut f = set_up(2);
    f.init_dynamics(0.1);
    f.check_impedance();
}

#[test]
fn test_ntn_fixture_impedance_3d() {
    let mut f = set_up(3);
    f.init_dynamics(0.1);
    f.check_impedance();
}
