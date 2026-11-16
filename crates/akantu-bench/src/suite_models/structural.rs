// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Structural-mechanics
// cases of the model suite; the set-ups follow test/test_model/
// test_structural_mechanics_model/ (Bernoulli beams, DKT18 plates, beam
// dynamics).

//! Structural mechanics (`akantu-structural`): a 3-D frame of Bernoulli
//! beams (static: stiffness assembly and one sparse solve; implicit
//! dynamic Newmark steps with the consistent mass), and a plate of
//! discrete Kirchhoff triangles (static). Meshes are built with the
//! `MeshAccessor`, as upstream's beam tests do.

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_parser::param::ParamValue;
use akantu_structural::{StructuralMaterial, StructuralMechanicsModel};

use super::harness::{Case, Digest, Run};

/// The structural cases.
pub const CASES: &[Case] = &[
    Case {
        name: "structural_beam3_frame_static",
        description: "structural, 3-D frame of Bernoulli beams (6 DOFs per node): stiffness \
                      assembly and one sparse solve",
        run: frame_static,
    },
    Case {
        name: "structural_beam3_frame_dynamic",
        description: "structural, 3-D frame of Bernoulli beams, implicit dynamic (Newmark) \
                      steps: residual, matrices and solve per step",
        run: frame_dynamic,
    },
    Case {
        name: "structural_dkt18_plate_static",
        description: "structural, plate of discrete Kirchhoff triangles (DKT18, 6 DOFs per \
                      node): stiffness assembly and one sparse solve",
        run: plate_static,
    },
];

const NDOF: usize = 6;

/// The structural cases build serial meshes: under `mpirun -n P`, `P > 1`,
/// they would only repeat the serial run on every rank.
fn serial_only(run: &Run) -> Result<()> {
    if run.comm.size() > 1 {
        return Err(akantu_core::Error::Unsupported(
            "the structural cases are serial (meshes built with the MeshAccessor on one rank)"
                .into(),
        ));
    }
    Ok(())
}

fn to_idx(i: usize) -> Idx {
    Idx::try_from(i).unwrap_or(Idx::MAX)
}

/// Frame of `n³` cells (`(n+1)³` nodes, beams along the three axes, edge
/// length 1); returns the mesh and one normal per beam.
fn frame(n: usize) -> Result<(Mesh, Vec<Real>)> {
    let mut mesh = Mesh::new(3, "frame")?;
    let id = |i: usize, j: usize, k: usize| to_idx((k * (n + 1) + j) * (n + 1) + i);
    let mut conn: Vec<Idx> = Vec::new();
    let mut normals: Vec<Real> = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    acc.push_node(&[i as Real, j as Real, k as Real])?;
                }
            }
        }
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    if i < n {
                        conn.extend([id(i, j, k), id(i + 1, j, k)]);
                        normals.extend([0.0, 0.0, 1.0]);
                    }
                    if j < n {
                        conn.extend([id(i, j, k), id(i, j + 1, k)]);
                        normals.extend([0.0, 0.0, 1.0]);
                    }
                    if k < n {
                        conn.extend([id(i, j, k), id(i, j, k + 1)]);
                        normals.extend([1.0, 0.0, 0.0]);
                    }
                }
            }
        }
        let mut c = acc.connectivity_mut(ElementType::BernoulliBeam3, GhostType::NotGhost);
        for e in conn.as_chunks::<2>().0 {
            c.push_tuple(e);
        }
        drop(c);
        acc.make_ready()?;
    }
    Ok((mesh, normals))
}

fn beam_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2.1e11,
        a: 1e-2,
        iy: 1e-5,
        iz: 1e-5,
        gj: 1e5,
        rho: 7800.0,
        ..StructuralMaterial::default()
    }
}

/// Frame model: base (`z = 0`) clamped, lateral and vertical loads on the
/// top nodes.
fn frame_model(
    run: &mut Run,
    n: usize,
    method: AnalysisMethod,
) -> Result<StructuralMechanicsModel> {
    let (mesh, normals) = run.once("mesh", || frame(n))?;
    run.counts.elements = mesh.nb_element(ElementType::BernoulliBeam3, GhostType::NotGhost);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * NDOF;
    let coords = mesh.nodes().as_slice().to_vec();
    let mut model = StructuralMechanicsModel::new(SharedMesh::new(mesh), None, "structural")?;
    model.set_extra_normals(ElementType::BernoulliBeam3, &normals)?;
    model.add_material(beam_material(), "")?;
    run.once("init", || model.init_full_with(method))?;
    let top = n as Real;
    {
        let mut b = model.blocked_dofs_mut()?;
        for (node, x) in coords.as_chunks::<3>().0.iter().enumerate() {
            if x[2] == 0.0 {
                b.as_mut_slice()[node * NDOF..(node + 1) * NDOF].fill(true);
            }
        }
    }
    {
        let mut f = model.external_force_mut()?;
        for (node, x) in coords.as_chunks::<3>().0.iter().enumerate() {
            if x[2] == top {
                f.as_mut_slice()[node * NDOF] = 1e3;
                f.as_mut_slice()[node * NDOF + 2] = -1e3;
            }
        }
    }
    Ok(model)
}

fn digest(model: &StructuralMechanicsModel, dynamic: bool) -> Result<Digest> {
    let mut d = Digest::new();
    d.reals(model.displacement()?.as_slice());
    if dynamic {
        d.reals(model.velocity()?.as_slice())
            .reals(model.acceleration()?.as_slice());
    }
    d.reals(model.internal_force()?.as_slice());
    Ok(d)
}

fn frame_static(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let mut model = frame_model(run, s.pick(3, 12, 24), AnalysisMethod::Static)?;
    run.looped("assemble_stiffness", s.pick(1, 5, 5), |_| {
        // the model keeps an assembled matrix until it is invalidated
        model.invalidate_matrices();
        model.assemble_stiffness_matrix()
    })?;
    run.once("solve_step", || model.solve_step(None))?;
    digest(&model, false)
}

fn frame_dynamic(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let mut model = frame_model(run, s.pick(2, 12, 24), AnalysisMethod::ImplicitDynamic)?;
    model.set_time_step(1e-4, None)?;
    {
        let nls = model.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(10))?;
        nls.set_param("threshold", ParamValue::Real(1e-8))?;
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))?;
    }
    run.looped("solve_step", s.pick(2, 60, 60), |_| model.solve_step(None))?;
    digest(&model, true)
}

/// Plate `[0, 1]²` in the plane `z = 0`, `n × n` squares split into DKT18
/// triangles; edges clamped, drilling rotations blocked, transverse load on
/// every free node.
fn plate_static(run: &mut Run) -> Result<Digest> {
    let s = run.size();
    let n = s.pick(6, 60, 170);
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    serial_only(run)?;
    let mesh = run.once("mesh", || {
        let mut mesh = Mesh::new(3, "plate")?;
        let id = |i: usize, j: usize| to_idx(j * (n + 1) + i);
        let mut acc = MeshAccessor::new(&mut mesh);
        let h = 1.0 / n as Real;
        for j in 0..=n {
            for i in 0..=n {
                acc.push_node(&[i as Real * h, j as Real * h, 0.0])?;
            }
        }
        let mut c = acc.connectivity_mut(ty, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
        drop(c);
        acc.make_ready()?;
        Ok(mesh)
    })?;
    run.counts.elements = mesh.nb_element(ty, GhostType::NotGhost);
    run.counts.nodes = mesh.nb_nodes();
    run.counts.dofs = run.counts.nodes * NDOF;
    let coords = mesh.nodes().as_slice().to_vec();
    let mut model = StructuralMechanicsModel::new(SharedMesh::new(mesh), None, "plate")?;
    model.add_material(
        StructuralMaterial {
            e: 2.1e11,
            nu: 0.3,
            t: 0.01,
            ..StructuralMaterial::default()
        },
        "",
    )?;
    run.once("init", || model.init_full_with(AnalysisMethod::Static))?;
    {
        let mut b = model.blocked_dofs_mut()?;
        let b = b.as_mut_slice();
        for (node, x) in coords.as_chunks::<3>().0.iter().enumerate() {
            if x[0] == 0.0 || x[1] == 0.0 || x[0] == 1.0 || x[1] == 1.0 {
                b[node * NDOF..(node + 1) * NDOF].fill(true);
            }
            b[node * NDOF + 5] = true;
        }
    }
    {
        let mut f = model.external_force_mut()?;
        let f = f.as_mut_slice();
        for node in 0..coords.len() / 3 {
            f[node * NDOF + 2] = -1.0;
        }
    }
    run.looped("assemble_stiffness", s.pick(1, 5, 5), |_| {
        // the model keeps an assembled matrix until it is invalidated
        model.invalidate_matrices();
        model.assemble_stiffness_matrix()
    })?;
    run.once("solve_step", || model.solve_step(None))?;
    digest(&model, false)
}
