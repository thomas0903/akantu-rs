// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Node-to-node contact
// case of the model suite; the set-up follows extra_packages/
// traction-at-split-node-contact/test/test_ntn_model/ (test_ntn_friction
// fixture, material_ntn_friction_slip_weakening.dat).

//! Node-to-node contact at split nodes (`akantu-ntn-contact`): a slider on
//! a base with coincident interface nodes, compressed and sheared by a
//! traction on its top; explicit steps with the contact and the
//! slip-weakening friction update (`NtnContactSolverCallback::solve_step`).

use akantu_core::Result;
use akantu_core::enums::SpatialDirection;
use akantu_mesh::SharedMesh;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_ntn_contact::{NtnContactSolverCallback, TasnUpstreamCompat};
use akantu_solid::SolidMechanicsModel;

use super::common::{nb_elements, parser};
use super::harness::{Case, Digest, Run};
use super::mesh::{Block, Cell, Msh};

/// The node-to-node contact cases.
pub const CASES: &[Case] = &[Case {
    name: "ntn_friction_quad4",
    description: "node-to-node split-node contact with slip-weakening friction, explicit \
                  steps (contact and friction update per step), 2-D quad4 slider on a base",
    run: friction_quad4,
}];

const DAT: &str = "friction linear_slip_weakening_no_healing no_regularisation [\n  \
    mu_s = 0.25\n  mu_k = 0.1\n  d_c = 0.00005\n]\n\n\
    material elastic [\n  name = slider\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n  \
    Plane_Stress = 1\n]\n\n\
    material elastic [\n  name = base\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n  \
    Plane_Stress = 1\n]\n";

fn friction_quad4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let (nx, ny, steps) = s.pick((40, 4, 10), (4000, 25, 300), (11300, 70, 300));
    let comm = run.comm.clone();
    if comm.size() > 1 {
        return Err(akantu_core::Error::Unsupported(
            "the node-to-node case runs serial (the parallel cases are heat and phase field)"
                .into(),
        ));
    }
    let (l, h) = (1.0, 0.025 * ny as f64 / 25.0);
    let mesh = run.once("mesh", || {
        let mut msh = Msh::new(2);
        msh.block(&Block::new("base", Cell::Quad4, [0.0; 3], [l, h, 0.0], [nx, ny, 1]));
        msh.block(&Block::new("slider", Cell::Quad4, [0.0, h, 0.0], [l, h, 0.0], [nx, ny, 1]));
        msh.read(&comm, "mesh")
    })?;
    run.counts.elements = nb_elements(&mesh, 2);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * 2;
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser(DAT)?)?;
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    let mut solver = run.once("init", || {
        NtnContactSolverCallback::new(
            &mut model,
            "slider_ymin",
            "base_ymax",
            1,
            0.1,
            TasnUpstreamCompat::default(),
        )
    })?;
    for axis in [SpatialDirection::X, SpatialDirection::Y] {
        model.apply_dirichlet_bc(&FlagOnly { axis }, "base_ymin")?;
    }
    model.apply_neumann_bc(&FromSameDim::new(vec![3e5, -1e6]), "slider_ymax")?;
    run.looped("solve_step", steps, |_| solver.solve_step(&mut model, None))?;
    let mut d = Digest::new();
    d.reals(model.displacement()?.as_slice())
        .reals(model.velocity()?.as_slice())
        .reals(solver.contact_pressure().as_slice());
    Ok(d)
}
