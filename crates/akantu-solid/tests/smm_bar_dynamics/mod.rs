// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_dynamics.cc, test_solid_mechanics_model_fixture.hh,
// test_solid_mechanics_model_dynamics_material.dat} (`TestSMMFixtureBar`):
// the bar wave of TEST-109, shared by upstream_solid_mechanics_model_dynamics.rs
// (serial and local-world runs) and mpi_smm.rs (real MPI).

// shared by two test binaries, each uses a subset
#![allow(dead_code)]

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Idx, Real, TypeFilter};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;

/// `test_solid_mechanics_model_dynamics_material.dat` (verbatim).
pub const MATERIAL: &str = "material elastic [\n\
\t name = uniterial\n\
\t rho = 1    # density\n\
\t E   = 1    # young's modulus\n\
\t nu  = 0.0  # poisson's ratio\n\
]\n";

/// Upstream's `Verification<dim>` constants (`poisson = 3/10`).
pub const A: Real = 1e-1;
pub const E: Real = 1.;
pub const POISSON: Real = 3. / 10.;
pub const RHO: Real = 1.;
pub const K: [Real; 3] = [0.5, 0., 0.];
pub const PSI1: [Real; 3] = [0., 0., 1.];
pub const PSI2: [Real; 3] = [0., 1., 0.];

pub fn cross(a: [Real; 3], b: [Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Upstream `Verification<dim>::{displacement, velocity}` at `x`, `t`.
pub fn verification(dim: usize, x: &[Real], t: Real) -> (Vec<Real>, Vec<Real>) {
    let lambda = E * POISSON / (1. + POISSON) / (1. - 2. * POISSON);
    let mu = E / 2. / (1. + POISSON);
    let cp = ((lambda + 2. * mu) / RHO).sqrt();
    let cs = (mu / RHO).sqrt();
    let c = (E / RHO).sqrt();
    let knorm = (K[0] * K[0] + K[1] * K[1] + K[2] * K[2]).sqrt();
    match dim {
        1 => {
            let omega = c * K[0];
            (
                vec![A * (K[0] * x[0] - omega * t).cos()],
                vec![omega * A * (K[0] * x[0] - omega * t).sin()],
            )
        }
        2 => {
            let kshear = [K[1], K[0]];
            let kpush = [K[0], K[1]];
            let (op, os) = (knorm * cp, knorm * cs);
            let dot = x[0] * kpush[0] + x[1] * kpush[1];
            let (pp, ps) = (dot - op * t, dot - os * t);
            (
                (0..2)
                    .map(|i| A * (kpush[i] * pp.cos() + kshear[i] * ps.cos()))
                    .collect(),
                (0..2)
                    .map(|i| A * (kpush[i] * op * pp.cos() + kshear[i] * os * ps.cos()))
                    .collect(),
            )
        }
        _ => {
            let kpush = K;
            let ks1 = cross(K, PSI1);
            let ks2 = cross(K, PSI2);
            let (op, os) = (knorm * cp, knorm * cs);
            let dot = x[0] * kpush[0] + x[1] * kpush[1] + x[2] * kpush[2];
            let (pp, ps) = (dot - op * t, dot - os * t);
            (
                (0..3)
                    .map(|i| A * (kpush[i] * pp.cos() + ks1[i] * ps.cos() + ks2[i] * ps.cos()))
                    .collect(),
                (0..3)
                    .map(|i| {
                        A * (kpush[i] * op * pp.cos()
                            + ks1[i] * os * ps.cos()
                            + ks2[i] * os * ps.cos())
                    })
                    .collect(),
            )
        }
    }
}

pub fn bar_path(ty: ElementType) -> std::path::PathBuf {
    let name = ty.to_string();
    fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join(format!("bar{name}.msh"))
}

/// Upstream `SolutionFunctor` on the group `BC`.
pub fn apply_solution(m: &mut SolidMechanicsModel, t: Real) {
    let dim = m.spatial_dimension();
    let (nodes, x) = {
        let mesh = m.physics.base.mesh.read().unwrap();
        let nodes = mesh
            .groups()
            .element_group("BC")
            .map(|g| g.nodes().to_vec())
            .unwrap_or_default();
        (nodes, mesh.nodes().as_slice().to_vec())
    };
    for n in nodes {
        let n = n as usize;
        let (u, v) = verification(dim, &x[n * dim..(n + 1) * dim], t);
        m.blocked_dofs_mut().unwrap().as_mut_slice()[n * dim] = true;
        m.displacement_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&u);
        m.velocity_mut().unwrap().as_mut_slice()[n * dim..(n + 1) * dim].copy_from_slice(&v);
    }
}

/// Result of one `TestSMMFixtureBar<type, method>::solveStep()` run.
#[derive(Debug, Clone)]
pub struct BarRun {
    /// Upstream's `max_error` (same value on every rank).
    pub max_error: Real,
    /// Final displacement and velocity gathered by global node id (rank 0
    /// only; `None` elsewhere).
    pub u: Option<Vec<Real>>,
    pub v: Option<Vec<Real>>,
}

/// `TestSMMFixtureBar<type, method>::solveStep()`: the maximum error.
pub fn max_error(comm: Communicator, ty: ElementType, method: AnalysisMethod) -> Real {
    run_bar(comm, ty, method)
        .unwrap_or_else(|e| panic!("{ty} {method:?}: {e}"))
        .max_error
}

/// The bar run of `TestSMMFixtureBar<type, method>` on `comm` (mesh read on
/// rank 0, distributed with RCB on more than one rank; upstream's
/// `PARALLEL` registration distributes with its default partitioner):
/// `max_error` and the final `u`, `v` gathered on rank 0.
pub fn run_bar(
    comm: Communicator,
    ty: ElementType,
    method: AnalysisMethod,
) -> Result<BarRun, String> {
    fn e(w: &str) -> impl Fn(akantu_core::Error) -> String + '_ {
        move |x| format!("{w}: {x}")
    }
    let dim = ty.spatial_dimension();
    let mut mesh = Mesh::with_communicator(dim, "mesh", comm.clone()).map_err(e("mesh"))?;
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &bar_path(ty), MeshIoType::Gmsh).map_err(e("read"))?;
        let nb_facets: usize = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: GhostType::NotGhost,
                kind: None,
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|t| mesh.nb_element(t, GhostType::NotGhost))
            .sum();
        if dim > 1 && nb_facets == 0 {
            let _ = akantu_mesh::utils::build_facets(&mut mesh).map_err(e("facets"))?;
        }
    }
    if comm.size() > 1 {
        mesh.compute_bounding_box().map_err(e("bbox"))?;
        let mut p = RcbPartitioner;
        let _ = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0)
            .map_err(e("distribute"))?;
    }
    let nb_global_nodes = mesh.nb_global_nodes();
    let nn = mesh.nb_nodes();
    let x = mesh.nodes().as_slice().to_vec();
    let local_or_master: Vec<bool> = (0..nn)
        .map(|n| mesh.is_local_or_master_node(n as Idx))
        .collect::<Result<_, _>>()
        .map_err(e("node flags"))?;
    let parser = Parser::parse_str(MATERIAL, "test_solid_mechanics_model_dynamics_material.dat")
        .map_err(e("parser"))?;
    let shared = SharedMesh::new(mesh);
    let mut m = SolidMechanicsModel::with_parser(shared.clone(), None, &ty.to_string(), parser)
        .map_err(e("model"))?;
    m.init_full(&ModelOptions::new(method))
        .map_err(e("init_full"))?;
    let dt = m.stable_time_step().map_err(e("stable time step"))? / 10.;
    m.set_time_step(dt, None).map_err(e("time step"))?;
    for n in 0..nn {
        let (u, v) = verification(dim, &x[n * dim..(n + 1) * dim], 0.);
        m.displacement_mut().map_err(e("u"))?.as_mut_slice()[n * dim..(n + 1) * dim]
            .copy_from_slice(&u);
        m.velocity_mut().map_err(e("v"))?.as_mut_slice()[n * dim..(n + 1) * dim]
            .copy_from_slice(&v);
    }
    apply_solution(&mut m, 0.);
    let simulation_time = 5. / 1.;
    let max_steps = (simulation_time / dt) as usize;
    let mut current_time = 0.;
    let mut max_error = -1.0_f64;
    for step in 0..max_steps {
        apply_solution(&mut m, current_time);
        let u_g = m.displacement().map_err(e("u"))?;
        let u = u_g.as_slice();
        let mut disp_error = [0.0_f64];
        for n in 0..nn {
            if !local_or_master[n] {
                continue;
            }
            let (s, _) = verification(dim, &x[n * dim..(n + 1) * dim], current_time);
            for d in 0..dim {
                let diff = s[d] - u[n * dim + d];
                disp_error[0] += diff * diff;
            }
        }
        drop(u_g);
        comm.all_reduce(&mut disp_error, ReduceOp::Sum)
            .map_err(e("all_reduce"))?;
        let err = disp_error[0].sqrt() / nb_global_nodes as Real;
        max_error = max_error.max(err);
        m.solve_step(None)
            .map_err(e(&format!("solve step {step}")))?;
        current_time += dt;
    }
    let u = m.displacement().map_err(e("u"))?.as_slice().to_vec();
    let v = m.velocity().map_err(e("v"))?.as_slice().to_vec();
    let mesh = shared.read().map_err(e("read"))?;
    let (gu, gv) = match mesh.synchronizers().node.as_ref() {
        Some(ns) if comm.size() > 1 => {
            let ent = NodeEntities { mesh: &mesh };
            (
                ns.gather(&ent, &u, dim).map_err(e("gather u"))?,
                ns.gather(&ent, &v, dim).map_err(e("gather v"))?,
            )
        }
        // serial or one rank: the global ids are the local ones
        _ if comm.rank() == 0 => (Some(u), Some(v)),
        _ => (None, None),
    };
    Ok(BarRun {
        max_error,
        u: gu,
        v: gv,
    })
}

/// Largest absolute difference of two fields, largest magnitude of `b`,
/// and the number of bitwise-equal entries.
pub fn diff(a: &[Real], b: &[Real]) -> (Real, Real, usize) {
    let mut d = 0.0_f64;
    let mut mag = 0.0_f64;
    let mut same = 0;
    for (x, y) in a.iter().zip(b) {
        d = d.max((x - y).abs());
        mag = mag.max(y.abs());
        if x.to_bits() == y.to_bits() {
            same += 1;
        }
    }
    (d, mag, same)
}

/// The gathered final `u` and `v` of a `P`-rank run against the serial run,
/// within `rel_tol` of their maximum. TEST-109's callers pass `1e-12`
/// (the explicit vs-serial value of `par_smm` as a regression
/// guard; the `1e-10` of `smm_parallel.rs`
/// `implicit_dynamic_distributed_vs_serial` is the cube's a-priori bound).
/// `Err` names the first field out of bounds.
pub fn compare_with_serial(par: &BarRun, ser: &BarRun, rel_tol: Real) -> Result<String, String> {
    let (pu, pv) = (
        par.u.as_ref().ok_or("no gathered u")?,
        par.v.as_ref().ok_or("no gathered v")?,
    );
    let (su, sv) = (
        ser.u.as_ref().ok_or("no serial u")?,
        ser.v.as_ref().ok_or("no serial v")?,
    );
    if pu.len() != su.len() || pv.len() != sv.len() {
        return Err(format!(
            "sizes {} / {} vs {} / {}",
            pu.len(),
            pv.len(),
            su.len(),
            sv.len()
        ));
    }
    let (du, mu, eu) = diff(pu, su);
    let (dv, mv, ev) = diff(pv, sv);
    let line = format!(
        "|du|max {du:.3e} (|u|max {mu:.3e}, {eu}/{} bitwise), |dv|max {dv:.3e} (|v|max {mv:.3e}, \
         {ev}/{} bitwise), max_error {:.17e} vs serial {:.17e}",
        su.len(),
        sv.len(),
        par.max_error,
        ser.max_error
    );
    if du > rel_tol * mu.max(Real::MIN_POSITIVE) || dv > rel_tol * mv.max(Real::MIN_POSITIVE) {
        return Err(format!("differs from the serial run: {line}"));
    }
    Ok(line)
}
