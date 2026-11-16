// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Heat-transfer cases of
// the model suite; the set-ups follow examples/c++/diffusion_model/
// heat_diffusion_dynamics_3d.cc, heat_diffusion_static_2d.cc and
// test/test_model/test_diffusion_model/.

//! Heat transfer (`akantu-diffusion`): explicit forward Euler with the
//! lumped capacity in 3-D and 2-D, backward Euler with a
//! temperature-dependent conductivity (Newton–Raphson, re-assembly per
//! iteration), and the static solve. The explicit 3-D case also runs
//! distributed (`mpirun -n P`, mesh read on rank 0 and distributed by
//! recursive coordinate bisection).

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Real, Result};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::SharedMesh;
use akantu_model::{Model, ModelOptions};
use akantu_parser::param::ParamValue;

use super::common::{digest_nodal, distribute, global_count, nb_elements, nb_owned_nodes, parser};
use super::harness::{Case, Digest, Run};
use super::mesh::{Block, Cell, Msh};

/// The heat-transfer cases.
pub const CASES: &[Case] = &[
    Case {
        name: "heat_explicit_tet4",
        description: "heat transfer, explicit lumped steps, 3-D tet4 cube (residual assembly, \
                      lumped update); distributed under mpirun",
        run: explicit_tet4,
    },
    Case {
        name: "heat_explicit_quad4",
        description: "heat transfer, explicit lumped steps, 2-D quad4 square (4 quadrature \
                      points per element)",
        run: explicit_quad4,
    },
    Case {
        name: "heat_implicit_hex8_kvar",
        description: "heat transfer, backward Euler, conductivity depending on T: Newton with \
                      conductivity update, assembly and sparse solve per iteration, 3-D hex8",
        run: implicit_hex8,
    },
    Case {
        name: "heat_static_tri6",
        description: "heat transfer, static: conductivity matrix assembly and one sparse \
                      solve, 2-D quadratic triangles",
        run: static_tri6,
    },
];

fn dat(dim: usize, variation: Real) -> String {
    let k = match dim {
        2 => "[[401, 0], [0, 401]]",
        _ => "[[401, 0, 0], [0, 401, 0], [0, 0, 401]]",
    };
    format!(
        "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = mat\n    \
         density = 8940\n    capacity = 385\n    conductivity = {k}\n    \
         conductivity_variation = {variation:?}\n    temperature_reference = 100\n  ]\n]\n"
    )
}

/// Model on a unit square / cube of `cell` with `n` cells per direction.
fn model(run: &mut Run, cell: Cell, n: usize, variation: Real) -> Result<HeatTransferModel> {
    let dim = if matches!(cell, Cell::Tet4 | Cell::Hex8) { 3 } else { 2 };
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
    run.counts.dofs = run.counts.nodes;
    HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser(&dat(dim, variation))?)
}

/// Initial temperature 100, the nodes within `r` of the centre `c` held at
/// 300 (the examples' heat source); with `walls`, the boundary held at 100.
fn heat_source(model: &mut HeatTransferModel, c: [Real; 3], r: Real, walls: bool) -> Result<()> {
    let (coords, dim) = {
        let m = model.mesh().read()?;
        (m.nodes().as_slice().to_vec(), m.spatial_dimension())
    };
    let nb = coords.len() / dim;
    let mut t = vec![100.0; nb];
    let mut b = vec![false; nb];
    for i in 0..nb {
        let x = &coords[i * dim..(i + 1) * dim];
        let d: Real = (0..dim).map(|k| (x[k] - c[k]) * (x[k] - c[k])).sum::<Real>().sqrt();
        if d < r {
            b[i] = true;
            t[i] = 300.0;
        }
        if walls && x.iter().any(|&v| v == 0.0 || v == 1.0) {
            b[i] = true;
        }
    }
    model.temperature_mut()?.as_mut_slice().copy_from_slice(&t);
    model.blocked_dofs_mut()?.as_mut_slice().copy_from_slice(&b);
    Ok(())
}

fn digest(model: &HeatTransferModel, rate: bool) -> Result<Digest> {
    let mut d = Digest::new();
    let m = model.mesh().read()?;
    digest_nodal(&mut d, &m, model.temperature()?.as_slice(), 1)?;
    if rate {
        digest_nodal(&mut d, &m, model.temperature_rate()?.as_slice(), 1)?;
    }
    digest_nodal(&mut d, &m, model.internal_heat_rate()?.as_slice(), 1)?;
    Ok(d)
}

fn explicit(run: &mut Run, cell: Cell, n: usize, steps: usize) -> Result<Digest> {
    let mut model = model(run, cell, n, 0.0)?;
    run.once("init", || {
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
    })?;
    let dim = model.spatial_dimension();
    let c = [0.5; 3];
    heat_source(&mut model, c, if dim == 3 { 0.1 } else { 0.05 }, false)?;
    let dt = run.once("stable_time_step", || model.stable_time_step())?;
    model.set_time_step(0.8 * dt, None)?;
    run.looped("solve_step", steps, |_| model.solve_step(None))?;
    digest(&model, true)
}

fn explicit_tet4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    explicit(run, Cell::Tet4, s.pick(6, 36, 72), s.pick(10, 400, 400))
}

fn explicit_quad4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    explicit(run, Cell::Quad4, s.pick(16, 400, 1130), s.pick(10, 400, 400))
}

fn implicit_hex8(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let mut model = model(run, Cell::Hex8, s.pick(4, 24, 48), 0.01)?;
    run.once("init", || {
        model.init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))
    })?;
    heat_source(&mut model, [0.5; 3], 0.15, true)?;
    let dt = model.stable_time_step()?;
    model.set_time_step(20.0 * dt, None)?;
    {
        let nls = model.solver.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(30))?;
        nls.set_param("threshold", ParamValue::Real(1e-10))?;
    }
    let mut iterations = 0i64;
    run.looped("solve_step", s.pick(2, 10, 10), |_| {
        model.solve_step(None)?;
        if let ParamValue::Int(i) = model.solver.non_linear_solver(None)?.get_param("nb_iterations")? {
            iterations += i;
        }
        Ok(())
    })?;
    let mut d = digest(&model, true)?;
    d.ints([u64::try_from(iterations).unwrap_or(0)]);
    Ok(d)
}

fn static_tri6(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let mut model = model(run, Cell::Tri6, s.pick(8, 220, 620), 0.0)?;
    run.once("init", || model.init_full(&ModelOptions::new(AnalysisMethod::Static)))?;
    heat_source(&mut model, [0.25, 0.25, 0.0], 0.1, true)?;
    run.looped("assemble_conductivity", s.pick(1, 5, 5), |_| {
        model.assemble_diffusivity_matrix()
    })?;
    run.once("solve_step", || model.solve_step(None))?;
    digest(&model, false)
}
