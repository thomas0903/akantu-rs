// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Embedded-interface
// cases of the model suite; the set-up follows examples/c++/embedded_model/
// embedded.cc (+ material.dat).

//! Embedded reinforcement (`akantu-embedded`): a concrete block with a
//! grid of straight bars that do not follow the mesh. The intersection
//! set-up (segment–element intersections of the bars with the background
//! mesh, `EmbeddedInterfaceIntersector::construct_data`) is timed inside
//! `init` and alone; the per-step cost of the reinforcement through the
//! internal-force and stiffness assemblies; one static solve. 2-D
//! (triangles) and 3-D (tetrahedra). Serial only (upstream: no parallel
//! embedded model).

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Real, Result};
use akantu_embedded::{EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions};
use akantu_mesh::SharedMesh;
use akantu_model::Model;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_parser::param::ParamValue;

use super::common::{nb_elements, parser};
use super::harness::{Case, Digest, Run};
use super::mesh::{Block, Cell, Msh};

/// The embedded cases.
pub const CASES: &[Case] = &[
    Case {
        name: "embedded_rebar_tri3",
        description: "embedded reinforcement, 2-D: bar/mesh intersection set-up, internal-force \
                      and stiffness assembly with the reinforcement, one static solve",
        run: rebar_tri3,
    },
    Case {
        name: "embedded_rebar_tet4",
        description: "embedded reinforcement, 3-D tet4: intersection set-up, assemblies, one \
                      static solve",
        run: rebar_tet4,
    },
];

const DAT: &str = "material elastic [\n  name = concrete\n  E = 30e9\n  nu = 0.1\n]\n\n\
    material reinforcement elastic [\n  name = reinforcement\n  E = 210e9\n  area = 1e-4\n  \
    pre_stress = 10e6\n]\n";

/// Bars along the axes: `nb` levels per direction, each bar cut into `k`
/// straight segments (positions offset from the background nodes).
fn bars(dim: usize, nb: usize, k: usize) -> Msh {
    let mut msh = Msh::new(dim);
    let phys = msh.physical(1, "reinforcement");
    let pos = |b: usize| (b as Real + 0.5) / nb as Real + 0.0123;
    let line = |msh: &mut Msh, a: [Real; 3], b: [Real; 3]| {
        let mut prev = msh.node(a);
        for s in 1..=k {
            let t = s as Real / k as Real;
            let x = [
                a[0] + t * (b[0] - a[0]),
                a[1] + t * (b[1] - a[1]),
                a[2] + t * (b[2] - a[2]),
            ];
            let cur = msh.node(x);
            msh.element(1, phys, vec![prev, cur]);
            prev = cur;
        }
    };
    let lo = 0.01;
    let hi = 0.99;
    if dim == 2 {
        for b in 0..nb {
            let y = pos(b).min(hi);
            line(&mut msh, [lo, y, 0.0], [hi, y, 0.0]);
            line(&mut msh, [y, lo, 0.0], [y, hi, 0.0]);
        }
    } else {
        for b in 0..nb {
            for c in 0..nb {
                let (u, v) = (pos(b).min(hi), pos(c).min(hi));
                line(&mut msh, [lo, u, v], [hi, u, v]);
                line(&mut msh, [u, lo, v], [u, hi, v]);
            }
        }
    }
    msh
}

fn rebar(run: &mut Run, cell: Cell, n: usize, nb: usize, k: usize) -> Result<Digest> {
    let dim = if cell == Cell::Tet4 { 3 } else { 2 };
    let comm = run.comm.clone();
    if comm.size() > 1 {
        return Err(akantu_core::Error::Unsupported(
            "the embedded model is serial (upstream: no parallel embedded model)".into(),
        ));
    }
    let (mesh, reinforcement) = run.once("mesh", || {
        let mut msh = Msh::new(dim);
        msh.block(&Block::new("concrete", cell, [0.0; 3], [1.0; 3], [n; 3]));
        Ok((
            msh.read(&comm, "mesh")?,
            bars(dim, nb, k).read(&comm, "reinforcement_mesh")?,
        ))
    })?;
    run.counts.elements = nb_elements(&mesh, dim) + nb_elements(&reinforcement, 1);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * dim;
    let mut model = EmbeddedInterfaceModel::with_parser(
        SharedMesh::new(mesh),
        reinforcement,
        Some(dim),
        "embedded_interface_model",
        parser(DAT)?,
    )?;
    let options = EmbeddedInterfaceModelOptions {
        base: super::common::eigen_options(run, AnalysisMethod::Static),
        ..EmbeddedInterfaceModelOptions::default()
    };
    run.once("init", || model.init_full(&options))?;
    let fixed = |axis| FixedValue { value: 0.0, axis };
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::X), "concrete_xmin")?;
    model
        .smm
        .apply_dirichlet_bc(&fixed(SpatialDirection::Y), "concrete_ymin")?;
    if dim == 3 {
        model
            .smm
            .apply_dirichlet_bc(&fixed(SpatialDirection::Z), "concrete_zmin")?;
    }
    let mut force = vec![0.0; dim];
    force[1] = -1e6;
    model
        .smm
        .apply_neumann_bc(&FromTraction::new(force), "concrete_ymax")?;
    {
        let solver = model.smm.solver.non_linear_solver_mut(None)?;
        solver.set_param("max_iterations", ParamValue::Int(1))?;
        solver.set_param("threshold", ParamValue::Real(1e-6))?;
        solver.set_param("convergence_type", ParamValue::Enum("residual".into()))?;
    }
    // the intersection set-up alone (the result is discarded)
    let calls = run.size().pick(1, 5, 5);
    let mut nb_interface = 0;
    {
        let mesh_h = model.smm.mesh().clone();
        run.looped("intersect", calls, |_| {
            let mesh = mesh_h.read()?;
            let imesh = model
                .intersector
                .construct_data(&mesh, model.primitive_mesh())?;
            nb_interface = imesh.nb_nodes();
            Ok(())
        })?;
    }
    run.once("solve_step", || model.solve_step(None))?;
    let mut d = Digest::new();
    d.reals(model.smm.displacement()?.as_slice())
        .reals(model.smm.internal_force()?.as_slice())
        .ints([nb_interface as u64]);
    run.looped(
        "assemble_internal_forces",
        run.size().pick(1, 20, 20),
        |_| model.assemble_internal_forces(),
    )?;
    run.looped("assemble_stiffness", run.size().pick(1, 5, 5), |_| {
        model.assemble_stiffness_matrix(true)
    })?;
    Ok(d)
}

fn rebar_tri3(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    rebar(
        run,
        Cell::Tri3,
        s.pick(10, 100, 280),
        s.pick(3, 20, 56),
        s.pick(7, 130, 370),
    )
}

fn rebar_tet4(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    rebar(
        run,
        Cell::Tet4,
        s.pick(4, 15, 30),
        s.pick(2, 5, 10),
        s.pick(5, 30, 60),
    )
}
