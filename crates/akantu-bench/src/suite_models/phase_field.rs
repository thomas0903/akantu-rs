// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Phase-field cases of
// the model suite; the set-up follows examples/c++/phase_field_model/
// phase_field_notch.cc (+ material_notch.dat).

//! Phase field (`akantu-phase-field`) coupled with solid mechanics
//! (`akantu-couplers`, `CouplerSolidPhaseField`): the staggered step of the
//! notch example (displacement increment on the top, fixed number of
//! staggered iterations), each piece of `CouplerSolidPhaseField::solve` a
//! phase of its own; then the two stiffness assemblies alone. Runs
//! distributed under `mpirun`.

use std::time::Instant;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{GhostType, Result};
use akantu_couplers::CouplerSolidPhaseField;
use akantu_mesh::SharedMesh;
use akantu_model::Model;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};

use super::common::{
    digest_nodal, distribute, eigen_options, global_count, nb_elements, nb_owned_nodes, parser,
};
use super::harness::{Case, Digest, Run};
use super::mesh::{Block, Cell, Msh};

/// The phase-field cases.
pub const CASES: &[Case] = &[
    Case {
        name: "phasefield_staggered_tri3",
        description: "phase field + solid, staggered static step (solid solve, strain \
                      transfer, phase-field solve, damage transfer, internal forces) and the \
                      two stiffness assemblies, 2-D tri3; distributed under mpirun",
        run: staggered_tri3,
    },
    Case {
        name: "phasefield_staggered_tet4",
        description: "phase field + solid, staggered static step and stiffness assemblies, \
                      3-D tet4 (volume kernels, larger bandwidth)",
        run: staggered_tet4,
    },
];

const DAT_2D: &str = "model solid_mechanics_model [\n  material phasefield [\n    name = body\n    \
    rho = 1.\n    E = 210.0\n    nu = 0.3\n    eta = 0\n    Plane_Stress = false\n  ]\n]\n\n\
    model phase_field_model [\n  phasefield quadratic [\n    name = body\n    l0 = 0.02\n    \
    gc = 2.7e-3\n    E = 210.0\n    nu = 0.3\n  ]\n]\n";

const DAT_3D: &str = "model solid_mechanics_model [\n  material phasefield [\n    name = body\n    \
    rho = 1.\n    E = 210.0\n    nu = 0.3\n    eta = 0\n  ]\n]\n\n\
    model phase_field_model [\n  phasefield quadratic [\n    name = body\n    l0 = 0.05\n    \
    gc = 2.7e-3\n    E = 210.0\n    nu = 0.3\n  ]\n]\n";

fn staggered(run: &mut Run, cell: Cell, n: usize, load_steps: usize) -> Result<Digest> {
    let dim = if cell == Cell::Tet4 { 3 } else { 2 };
    let comm = run.comm.clone();
    let mut mesh = run.once("mesh", || {
        let mut msh = Msh::new(dim);
        msh.block(&Block::new("body", cell, [0.0; 3], [1.0; 3], [n; 3]));
        msh.read(&comm, "mesh")
    })?;
    if comm.size() > 1 {
        run.once("distribute", || distribute(&mut mesh))?;
    }
    run.counts.elements = global_count(&comm, nb_elements(&mesh, dim))?;
    run.counts.nodes = global_count(&comm, nb_owned_nodes(&mesh))?;
    run.counts.dofs = run.counts.nodes * (dim + 1);
    let dat = if dim == 2 { DAT_2D } else { DAT_3D };
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser(dat)?,
    )?;
    let options = eigen_options(run, AnalysisMethod::Static);
    run.once("init", || {
        c.solid.init_full(&options)?;
        c.phase.init_full(&options)
    })?;
    let fixed = |axis| FixedValue { value: 0.0, axis };
    c.solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "body_ymin")?;
    c.solid
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "body_xmin")?;
    if dim == 3 {
        c.solid
            .apply_dirichlet_bc(&fixed(SpatialDirection::Z), "body_zmin")?;
    }
    let increment = IncrementValue {
        value: if dim == 2 { 4e-4 } else { 1e-3 },
        axis: SpatialDirection::Y,
    };
    let nb_staggered = 2;
    let names = [
        "solid_solve",
        "strain_transfer",
        "phase_solve",
        "damage_transfer",
        "internal_forces",
    ];
    let mut t = [0.0f64; 5];
    let mut calls = 0;
    let step = |c: &mut CouplerSolidPhaseField, t: &mut [f64; 5]| -> Result<()> {
        c.solid.apply_dirichlet_bc(&increment, "body_ymax")?;
        for _ in 0..nb_staggered {
            let t0 = Instant::now();
            c.solid.solve_step(None)?;
            let t1 = Instant::now();
            c.compute_strain_on_quad_points(GhostType::NotGhost)?;
            let t2 = Instant::now();
            c.phase.solve_step(None)?;
            let t3 = Instant::now();
            c.compute_damage_on_quad_points(GhostType::NotGhost)?;
            c.compute_damage_on_quad_points(GhostType::Ghost)?;
            let t4 = Instant::now();
            c.solid.assemble_internal_forces()?;
            let t5 = Instant::now();
            for (k, (a, b)) in [(t0, t1), (t1, t2), (t2, t3), (t3, t4), (t4, t5)]
                .into_iter()
                .enumerate()
            {
                t[k] += (b - a).as_secs_f64();
            }
        }
        Ok(())
    };
    let mut scratch = [0.0; 5];
    for _ in 0..3 {
        step(&mut c, &mut scratch)?;
    }
    let min = run.opts.loop_seconds.unwrap_or(0.0);
    let t_start = Instant::now();
    loop {
        for _ in 0..load_steps {
            step(&mut c, &mut t)?;
            calls += nb_staggered;
        }
        if t_start.elapsed().as_secs_f64() >= min {
            break;
        }
    }
    for (k, name) in names.iter().enumerate() {
        run.record(name, calls, t[k]);
    }
    let mut d = Digest::new();
    {
        let m = c.solid.mesh().read()?;
        digest_nodal(&mut d, &m, c.solid.displacement()?.as_slice(), dim)?;
        digest_nodal(&mut d, &m, c.phase.damage()?.as_slice(), 1)?;
    }
    // the assemblies alone (after the digest: they only rebuild matrices)
    run.looped("solid_assemble_stiffness", run.size().pick(1, 5, 5), |_| {
        c.solid.assemble_stiffness_matrix(true)
    })?;
    run.looped("phase_assemble_stiffness", run.size().pick(1, 5, 5), |_| {
        c.phase.assemble_stiffness_matrix()
    })?;
    Ok(d)
}

fn staggered_tri3(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    staggered(run, Cell::Tri3, s.pick(10, 160, 450), s.pick(1, 2, 2))
}

fn staggered_tet4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    staggered(run, Cell::Tet4, s.pick(3, 10, 20), s.pick(1, 2, 2))
}
