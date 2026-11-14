// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/test_material_damage_iterative.cc (set-up).
// A failure injection, committed with the fix.
//! A failure on one rank inside the stress phase of a solve, before the
//! law's declared `Max` reduction (`ScopeReducer`, `akantu-solid`
//! `model/assembly.rs`): a NaN displacement at a node local to rank 1 only
//! makes rank 1's equivalent-stress eigen-decomposition fail
//! (`compute_all_stresses`). Every rank returns an error from `solve_step`
//! well before the local world's receive timeout (an agreed refusal, no
//! stall), for `damage_iterative` and `orthotropic_damage_iterative`, and
//! the other rank reports it **at the reducer's own agreement**
//! (`…:smm:law_stress_reduction:prepare`), which pins `ScopeReducer`'s
//! agreement of the stress phase's outcome (without it the
//! refusal still comes, one step later at `_smm_stress`, by the structural
//! rule of ARCHITECTURE).

mod common;

use std::time::{Duration, Instant};

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SparseSolverType, SpatialDirection};
use akantu_core::{Idx, Real};
use akantu_extra_materials::{ExtraLawOptions, register_extra_materials_with};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use common::data;

const TIMEOUT: Duration = Duration::from_secs(30);

fn run(comm: Communicator, dat: &str) -> (f64, Result<(), String>) {
    let rank = comm.rank();
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    if rank == 0 {
        akantu_io::read_mesh(
            &mut mesh,
            &data("test_material_damage/plate.msh"),
            MeshIoType::Gmsh,
        )
        .unwrap();
    }
    let mut p = RcbPartitioner;
    let _ = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    // a node local to this rank only (not shared, not ghost)
    let local_node = (0..mesh.nb_nodes())
        .find(|&n| mesh.node_flag(Idx::try_from(n).unwrap()).bits() == 0)
        .unwrap();
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    register_extra_materials_with(&mut model.physics.factory, ExtraLawOptions::default()).unwrap();
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    o.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    model.init_full(&o).unwrap();
    for (value, axis, group) in [
        (0., SpatialDirection::X, "left"),
        (0., SpatialDirection::Y, "bottom"),
        (2., SpatialDirection::Y, "top"),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value, axis }, group)
            .unwrap();
    }
    if rank == 1 {
        let mut u = model.displacement_mut().unwrap();
        u.as_mut_slice()[2 * local_node] = Real::NAN;
    }
    let t0 = Instant::now();
    let r = model.solve_step(None).map_err(|e| e.to_string());
    let dt = t0.elapsed().as_secs_f64();
    eprintln!("rank {rank}: solve_step after {dt:.2} s: {r:?}");
    (dt, r)
}

fn check(dat: &'static str) {
    let out = run_local(2, TIMEOUT, |c| run(c, dat));
    for (r, (dt, o)) in out.iter().enumerate() {
        assert!(
            o.is_err(),
            "rank {r}: the failure of rank 1 was not reported: {o:?}"
        );
        assert!(
            *dt < 5.0,
            "rank {r}: {dt} s: a stall, not an agreed refusal: {o:?}"
        );
        let msg = o.as_ref().unwrap_err();
        assert!(!msg.contains("timed out"), "rank {r}: timeout: {msg}");
        if r == 1 {
            assert!(msg.contains("non-finite eigenvalue"), "rank 1: {msg}");
        } else {
            assert!(
                msg.contains("smm:law_stress_reduction:prepare") && msg.contains("another rank"),
                "rank {r}: not refused at the reducer's agreement: {msg}"
            );
        }
    }
}

#[test]
fn one_rank_stress_failure_is_agreed_at_the_reducer_damage_iterative() {
    check(
        "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n \
         prescribed_dam = 0.1\n max_damage = 0.99\n]\n",
    );
}

#[test]
fn one_rank_stress_failure_is_agreed_at_the_reducer_orthotropic() {
    check(
        "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n \
         Sc = 10\n prescribed_dam = 0.1\n]\n",
    );
}
