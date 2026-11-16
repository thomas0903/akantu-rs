// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Contact cases of the
// model suite; the set-ups follow examples/c++/contact_mechanics_model/
// (contact_explicit_dynamic.cc, contact_explicit_static.cc, hertz material.dat).

//! Contact mechanics (`akantu-contact`) and the solid–contact coupler
//! (`akantu-couplers`, `CouplerSolidContact`): two blocks with
//! non-matching surface meshes, the upper one pressed into the lower one.
//! Detection alone (grid construction, candidate pairs, projections) in
//! 2-D and 3-D with many surface elements; explicit penalty steps; the
//! implicit static Newton steps with the contact stiffness. The contact
//! model is serial only (upstream: no parallel contact detection).

use akantu_contact::PhysicalSurfaceSelector;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Real, Result};
use akantu_couplers::CouplerSolidContact;
use akantu_mesh::SharedMesh;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_model::{Model, ModelOptions};
use akantu_parser::param::ParamValue;

use super::common::{nb_elements, parser};
use super::harness::{Case, Digest, Run};
use super::mesh::{Block, Cell, Msh};

/// The contact cases.
pub const CASES: &[Case] = &[
    Case {
        name: "contact_search_quad4",
        description: "contact detection alone (grid, candidate pairs, projections) between two \
                      non-matching 2-D surfaces of many segments, every slave node in contact",
        run: search_quad4,
    },
    Case {
        name: "contact_search_tet4",
        description: "contact detection alone between two non-matching 3-D triangle surfaces \
                      (tet4 blocks)",
        run: search_tet4,
    },
    Case {
        name: "contact_explicit_quad4",
        description: "solid + contact coupler, explicit penalty steps (detection every step, \
                      contact forces, lumped update), 2-D quad4 blocks",
        run: explicit_quad4,
    },
    Case {
        name: "contact_implicit_static_quad4",
        description: "solid + contact coupler, static Newton steps with the contact stiffness \
                      (detection and assembly per iteration, sparse solve), 2-D quad4 blocks",
        run: implicit_quad4,
    },
    Case {
        name: "contact_cohesive_quad4",
        description: "solid + cohesive + contact coupler, explicit steps: extrinsic insertion \
                      along an internal line, then contact between the crack faces",
        run: cohesive_contact_quad4,
    },
];

const DAT: &str = "material elastic [\n  name = upper\n  rho = 1.0\n  E = 1e3\n  nu = 0.3\n]\n\n\
    material elastic [\n  name = lower\n  rho = 1.0\n  E = 1e3\n  nu = 0.3\n]\n\n\
    contact_detector [\n  type = explicit\n  master = lower_top\n  slave = upper_bottom\n  \
    projection_tolerance = 1e-10\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n\n\
    contact_resolution penalty_linear [\n  name = upper_bottom\n  mu = 0.0\n  epsilon_n = 4e5\n  \
    epsilon_t = 1e5\n  is_master_deformable = false\n]\n";

/// Two blocks: `lower` (`[0, 1] × [0, h]`, `nl` cells along the surface)
/// and `upper` (`[δ, 1 − δ] × [h − p, 2h − p]`, `nu` cells), the upper one
/// penetrating by `p`. The detector's groups are `lower_top` /
/// `upper_bottom` (aliases of the `ymax` / `ymin` boundaries in 2-D,
/// `zmax` / `zmin` in 3-D).
fn coupler(
    run: &mut Run,
    cell: Cell,
    nl: usize,
    nu: usize,
    depth: usize,
    p: Real,
    method: AnalysisMethod,
) -> Result<CouplerSolidContact> {
    let dim = if cell == Cell::Tet4 || cell == Cell::Hex8 { 3 } else { 2 };
    let comm = run.comm.clone();
    if comm.size() > 1 {
        return Err(akantu_core::Error::Unsupported(
            "the contact model is serial (upstream has no parallel contact detection)".into(),
        ));
    }
    let h = 0.1;
    let d = 0.05;
    let mesh = run.once("mesh", || {
        let mut msh = Msh::new(dim);
        let (lower, upper) = if dim == 2 {
            (
                Block::new("lower", cell, [0.0; 3], [1.0, h, 0.0], [nl, depth, 1]),
                Block::new("upper", cell, [d, h - p, 0.0], [1.0 - 2.0 * d, h, 0.0], [nu, depth, 1]),
            )
        } else {
            (
                Block::new("lower", cell, [0.0; 3], [1.0, 1.0, h], [nl, nl, depth]),
                Block::new(
                    "upper",
                    cell,
                    [d, d, h - p],
                    [1.0 - 2.0 * d, 1.0 - 2.0 * d, h],
                    [nu, nu, depth],
                ),
            )
        };
        msh.block(&lower);
        msh.block(&upper);
        msh.alias(dim - 1, if dim == 2 { "lower_ymax" } else { "lower_zmax" }, "lower_top");
        msh.alias(dim - 1, if dim == 2 { "upper_ymin" } else { "upper_zmin" }, "upper_bottom");
        msh.read(&comm, "mesh")
    })?;
    run.counts.elements = nb_elements(&mesh, dim);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * dim;
    let parser = parser(DAT)?;
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser)?;
    let mut c =
        CouplerSolidContact::with_parser(SharedMesh::new(mesh), None, "coupler_solid_contact", parser)?;
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    run.once("init", || c.init_full(&ModelOptions::new(method)))?;
    c.contact.set_surface_selector(Box::new(selector))?;
    let fixed = |axis| FixedValue { value: 0.0, axis };
    let axes: &[SpatialDirection] = if dim == 2 {
        &[SpatialDirection::X, SpatialDirection::Y]
    } else {
        &[SpatialDirection::X, SpatialDirection::Y, SpatialDirection::Z]
    };
    let (bottom, top) = if dim == 2 {
        ("lower_ymin", "upper_ymax")
    } else {
        ("lower_zmin", "upper_zmax")
    };
    for &a in axes {
        c.apply_dirichlet_bc(&fixed(a), bottom)?;
        c.apply_dirichlet_bc(&fixed(a), top)?;
    }
    Ok(c)
}

fn digest(run: &Run, c: &CouplerSolidContact) -> Result<Digest> {
    run.note(&format!("contact: {} contact elements at the end", c.contact.contact_elements().len()));
    let mut d = Digest::new();
    d.reals(c.displacement()?.as_slice())
        .reals(c.contact.gaps().as_slice())
        .reals(c.contact.normals().as_slice())
        .ints([c.contact.contact_elements().len() as u64]);
    Ok(d)
}

fn search(run: &mut Run, cell: Cell, nl: usize, nu: usize, calls: usize) -> Result<Digest> {
    let mut c = coupler(run, cell, nl, nu, 2, 1e-3, AnalysisMethod::ExplicitLumpedMass)?;
    // positions = X (u = 0)
    let x = c.solid.mesh().read()?.nodes().clone();
    c.contact.positions_mut().copy_from(&x)?;
    run.looped("search", calls, |_| c.contact.search())?;
    if c.contact.contact_elements().is_empty() {
        return Err(akantu_core::Error::invalid("contact search found no contact"));
    }
    digest(run, &c)
}

fn search_quad4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    search(run, Cell::Quad4, s.pick(40, 40_000, 120_000), s.pick(30, 30_000, 90_000), s.pick(2, 20, 20))
}

fn search_tet4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    search(run, Cell::Tet4, s.pick(6, 160, 450), s.pick(5, 120, 340), s.pick(2, 10, 10))
}

fn explicit_quad4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let (nl, nu, depth) = s.pick((40, 30, 4), (1200, 900, 60), (3400, 2550, 170));
    let mut c = coupler(run, Cell::Quad4, nl, nu, depth, 0.0, AnalysisMethod::ExplicitLumpedMass)?;
    let dt = run.once("stable_time_step", || c.stable_time_step())?;
    c.set_time_step(0.1 * dt, None)?;
    let inc = IncrementValue {
        value: -2e-6,
        axis: SpatialDirection::Y,
    };
    run.looped("solve_step", s.pick(10, 200, 200), |_| {
        c.apply_dirichlet_bc(&inc, "upper_ymax")?;
        c.solve_step(None)
    })?;
    digest(run, &c)
}

fn implicit_quad4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let (nl, nu, depth) = s.pick((20, 15, 4), (400, 300, 40), (1130, 850, 113));
    let mut c = coupler(run, Cell::Quad4, nl, nu, depth, 0.0, AnalysisMethod::Static)?;
    {
        let nls = c.solid.solver.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(30))?;
        nls.set_param("threshold", ParamValue::Real(1e-10))?;
    }
    let inc = IncrementValue {
        value: -2e-4,
        axis: SpatialDirection::Y,
    };
    let mut iterations = 0i64;
    run.looped("solve_step", s.pick(2, 5, 5), |_| {
        c.apply_dirichlet_bc(&inc, "upper_ymax")?;
        c.solve_step(None)?;
        if let ParamValue::Int(i) = c.solid.solver.non_linear_solver(None)?.get_param("nb_iterations")? {
            iterations += i;
        }
        Ok(())
    })?;
    run.note(&format!("contact static: {iterations} Newton iterations"));
    let mut d = digest(run, &c)?;
    d.ints([u64::try_from(iterations).unwrap_or(0)]);
    Ok(d)
}

const COHESIVE_DAT: &str = "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    \
    cohesive_surfaces = [interface]\n  ]\n\n  material elastic [\n    name = body\n    \
    rho = 1.0\n    E = 1e3\n    nu = 0.3\n  ]\n\n  material cohesive_linear [\n    \
    name = interface\n    beta = 1\n    G_c = 6.25e-4\n    sigma_c = 1\n    kappa = 1\n    \
    penalty = 0\n    contact_after_breaking = false\n  ]\n]\n\n\
    contact_detector [\n  type = explicit\n  master = interface\n  slave = interface\n  \
    projection_tolerance = 1e-10\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n\n\
    contact_resolution penalty_linear [\n  name = interface\n  mu = 0.0\n  epsilon_n = 1e3\n  \
    epsilon_t = 0\n  is_master_deformable = true\n]\n";

/// Solid–cohesive–contact coupler (`CouplerSolidCohesiveContact`, set-up of
/// examples/c++/contact_mechanics_model/hertz cohesive-contact): a block
/// with the internal facet line `interface` at mid-height, opened by
/// extrinsic cohesive insertion and closed again (contact between the
/// crack faces); explicit steps with the stress check.
fn cohesive_contact_quad4(run: &mut Run) -> Result<Digest> {
    use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
    use akantu_contact::CohesiveSurfaceSelector;
    use akantu_couplers::CouplerSolidCohesiveContact;
    let s = run.size();
    let (nx, ny, steps) = s.pick((20, 4, 20), (1600, 40, 600), (4500, 112, 600));
    let comm = run.comm.clone();
    if comm.size() > 1 {
        return Err(akantu_core::Error::Unsupported(
            "the contact model is serial (upstream has no parallel contact detection)".into(),
        ));
    }
    let (w, h) = (1.0, 0.1 * ny as Real / 40.0);
    let mesh = run.once("mesh", || {
        let mut msh = Msh::new(2);
        msh.block(&Block::new("body", Cell::Quad4, [0.0; 3], [w, h, 0.0], [nx, ny, 1]));
        // the internal facets at mid-height (block nodes are numbered row
        // by row from 0)
        let phys = msh.physical(1, "interface");
        let j = ny / 2;
        for i in 0..nx {
            msh.element(1, phys, vec![j * (nx + 1) + i, j * (nx + 1) + i + 1]);
        }
        msh.read(&comm, "mesh")
    })?;
    run.counts.elements = nb_elements(&mesh, 2);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * 2;
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser(COHESIVE_DAT)?,
    )?;
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))?;
    run.once("init", || {
        c.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
    })?;
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "body_xmin")?;
    c.apply_dirichlet_bc(&fix, "body_xmax")?;
    let dt = c.stable_time_step()? * 0.1;
    c.set_time_step(dt, None)?;
    // opening for the first third of the calls, closing afterwards (the
    // crack faces are pressed together for the last third)
    let total = steps + 3;
    let inc = 4e-3 * h / total as Real;
    let mut inserted = 0usize;
    run.looped("solve_step", steps, |i| {
        let v = if i < total / 3 { inc } else { -inc };
        c.apply_dirichlet_bc(&IncrementValue { value: v, axis: SpatialDirection::Y }, "body_ymax")?;
        c.apply_dirichlet_bc(&IncrementValue { value: -v, axis: SpatialDirection::Y }, "body_ymin")?;
        c.solve_step(None)?;
        inserted += c.check_cohesive_stress()?;
        Ok(())
    })?;
    run.note(&format!(
        "contact_cohesive: {inserted} cohesive elements inserted, {} contact elements at the end",
        c.contact.contact_elements().len()
    ));
    let mut d = Digest::new();
    d.reals(c.solid.smm.displacement()?.as_slice())
        .reals(c.contact.gaps().as_slice())
        .ints([inserted as u64, c.contact.contact_elements().len() as u64]);
    Ok(d)
}
