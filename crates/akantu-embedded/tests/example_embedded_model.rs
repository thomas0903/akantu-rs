// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/embedded_model/{embedded.cc,
// material.dat, concrete.geo, reinforcement.geo} (meshes: the frozen Gmsh
// 4.13.1 meshes of fixtures/meshes, D-009).

//! Same-intent run of the upstream example `embedded_model` (EX-008; the
//! facade-crate example itself is tested with the examples): a `10 × 1` concrete
//! beam of quadratic triangles (`_triangle_6`, `SecondOrderIncomplete`)
//! with a pre-stressed bar along `y = 0.25` (`E = 210e9`, `A = 1e-4`,
//! `pre_stress = 10e6`), `XBlocked` (x = 10) blocked in x, `YBlocked`
//! (corner (0, 0)) in y, a traction `(0, −1)` on `Force` (top edge); static,
//! Newton–Raphson limited to one iteration (threshold 1e-6, residual);
//! dumps of the concrete and of the reinforcement.
//!
//! Upstream asserts nothing; this test checks what the run must satisfy:
//! convergence in one iteration (linear problem), global equilibrium of the
//! reactions with the applied load (forces and moment), the bar stress
//! around the pre-stress and the dump files.

mod common;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{ElementType, GhostType, Real};
use akantu_embedded::{EmbeddedInterfaceModelOptions, REINFORCEMENT_DUMPER};
use akantu_model::boundary_condition::{FixedValue, FromTraction};

const MATERIAL: &str = "material elastic [
  name = concrete
  E = 30e9
  nu = 0.1
]

material reinforcement elastic [
  name = reinforcement
  E = 210e9
  area = 1e-4
  pre_stress = 10e6
]
";

#[test]
fn example_embedded_model() {
    const DIM: usize = 2;
    let dir =
        akantu_validation::fixtures_root().join("meshes/gmsh-4.13.1/examples/c++/embedded_model");
    let mesh = common::read_mesh(&dir.join("concrete.msh"), DIM, "mesh");
    assert!(mesh.nb_element(ElementType::Triangle6, GhostType::NotGhost) > 0);
    // upstream: "Exception is raised because reinforcement mesh contains only
    // segments" (caught); the port reads it without error
    let reinforcement =
        common::read_mesh(&dir.join("reinforcement.msh"), DIM, "reinforcement_mesh");
    let mut model = common::model(mesh, reinforcement, MATERIAL);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();

    model
        .smm
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "XBlocked",
        )
        .unwrap();
    model
        .smm
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::Y,
            },
            "YBlocked",
        )
        .unwrap();
    model
        .smm
        .apply_neumann_bc(&FromTraction::new(vec![0.0, -1.0]), "Force")
        .unwrap();

    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("example_embedded_model");
    let _ = std::fs::remove_dir_all(&out);
    model.smm.set_base_name("concrete").unwrap();
    model.smm.set_directory(&out).unwrap();
    model.smm.add_dump_field_vector("displacement").unwrap();
    model.smm.add_dump_field_vector("external_force").unwrap();
    model.smm.add_dump_field_vector("internal_force").unwrap();
    model.smm.add_dump_field_tensor("stress").unwrap();
    model
        .set_base_name_to_dumper(REINFORCEMENT_DUMPER, "reinforcement")
        .unwrap();
    model
        .set_directory_to_dumper(REINFORCEMENT_DUMPER, &out)
        .unwrap();
    // upstream dumps `stress_embedded`, removed in 2018; the reinforcement
    // stress is `stress`
    model
        .add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, "stress")
        .unwrap();

    common::set_newton(&mut model, 1, 1e-6);
    model.solve_step(None).unwrap();

    model.dump(None, None).unwrap();
    model.dump_named(REINFORCEMENT_DUMPER, None, None).unwrap();
    let files: Vec<_> = std::fs::read_dir(&out)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(!files.is_empty(), "no dump written in {out:?}");

    // global equilibrium: reactions (−(f_ext + f_int) on blocked dofs, and
    // ~0 elsewhere) balance the applied traction (resultant (0, −10) at
    // x = 5, y = 1) in force and moment
    model.assemble_internal_forces().unwrap();
    let nodes = common::nodes(&model);
    let f_int = model.smm.internal_force().unwrap().as_slice().to_vec();
    let f_ext = model.smm.external_force().unwrap().as_slice().to_vec();
    let blocked = model.smm.blocked_dofs().unwrap().as_slice().to_vec();
    let (mut fx, mut fy, mut m) = (0.0, 0.0, 0.0);
    let mut free_residual: Real = 0.0;
    for (n, x) in nodes.iter().enumerate() {
        let r = [
            f_ext[n * DIM] + f_int[n * DIM],
            f_ext[n * DIM + 1] + f_int[n * DIM + 1],
        ];
        for i in 0..DIM {
            if !blocked[n * DIM + i] {
                free_residual = free_residual.max(r[i].abs());
            }
        }
        let (ex, ey) = (f_ext[n * DIM], f_ext[n * DIM + 1]);
        fx += ex;
        fy += ey;
        m += x[0] * ey - x[1] * ex;
        // reaction = −r on blocked dofs
        if blocked[n * DIM] {
            fx -= r[0];
            m -= -x[1] * r[0];
        }
        if blocked[n * DIM + 1] {
            fy -= r[1];
            m -= x[0] * r[1];
        }
    }
    let load: Real = f_ext.iter().skip(1).step_by(2).sum();
    assert!((load + 10.0).abs() <= 1e-12 * 10.0, "applied load {load}");
    // the pre-stress force is A σ_p = 1000: equilibrium to 1e-6 of it
    // (direct solve of a linear problem, NUMERICAL_VALIDATION)
    assert!(free_residual <= 1e-6, "free residual {free_residual}");
    assert!(
        fx.abs() <= 1e-6 && fy.abs() <= 1e-6 && m.abs() <= 1e-5,
        "{fx} {fy} {m}"
    );

    // the bar stays in tension around its pre-stress, the concrete pays
    let bar = model.smm.material_by_name("reinforcement").unwrap();
    let s = bar
        .base()
        .stress
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    assert!(!s.is_empty());
    for &x in s.as_slice() {
        assert!(x > 0.0 && x < 10e6, "bar stress {x}");
    }
}
