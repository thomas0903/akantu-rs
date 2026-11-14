// SPDX-License-Identifier: LGPL-3.0-or-later
// Test: mutation 2 of the IGFEM interface move refused at
// the capability check.
// Derived from Akantu (EPFL/LSMS) extra_packages/igfem/test/patch_tests/
// test_interface_position.cc.

use super::*;
use akantu_core::enums::AnalysisMethod;
use akantu_geometry_cgal::SkSphere;

const DAT: &str = include_str!("../../tests/data/upstream/material_test_interface_position.dat");

fn read_mesh() -> Mesh {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/upstream/test_interface_position.msh"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut m = Mesh::new(2, "mesh").unwrap();
    akantu_io::read_mesh_from_str(
        &mut m,
        path,
        &text,
        akantu_core::enums::MeshIoType::Gmsh,
        &akantu_io::ReadOptions::default(),
    )
    .unwrap();
    m
}

fn circle(r: Real) -> Vec<SkSphere> {
    vec![SkSphere::try_from_f64([0.0, 0.0, 0.0], r * r).unwrap()]
}

/// Upstream's patch test (`u_x = 0.5 x + 0.5`, homogeneous `E`): the
/// boundary conditions of TEST-192 and the maximum nodal error of the
/// solution over every node, standard and enriched (the exact field is
/// linear, so the criterion of TEST-192, `1e-13`, applies to the nodal
/// values as to the L2 error).
fn solve_patch_and_max_nodal_error(model: &mut SolidMechanicsModelIgfem) -> Real {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let mut lower = [Real::INFINITY; 2];
    let mut upper = [Real::NEG_INFINITY; 2];
    for x in pos.tuples() {
        for d in 0..2 {
            lower[d] = lower[d].min(x[d]);
            upper[d] = upper[d].max(x[d]);
        }
    }
    let eps = ((upper[0] - lower[0]) * 1e-6).abs();
    let mut blocked = model.smm.blocked_dofs().unwrap().clone();
    let mut disp = model.smm.displacement().unwrap().clone();
    for i in 0..pos.len() {
        let x = pos.tuple(i);
        if (x[1] - lower[1]).abs() < eps {
            blocked.tuple_mut(i)[1] = true;
            disp.tuple_mut(i)[1] = 0.0;
        }
        if (x[0] - lower[0]).abs() < eps {
            blocked.tuple_mut(i)[0] = true;
            disp.tuple_mut(i)[0] = 0.0;
        }
        if (x[0] - upper[0]).abs() < eps {
            blocked.tuple_mut(i)[0] = true;
            disp.tuple_mut(i)[0] = 1.0;
        }
    }
    model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .copy_from(&blocked)
        .unwrap();
    model
        .smm
        .displacement_mut()
        .unwrap()
        .copy_from(&disp)
        .unwrap();
    let nls = model.smm.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", akantu_parser::ParamValue::Int(2))
        .unwrap();
    nls.set_param("threshold", akantu_parser::ParamValue::Real(1e-12))
        .unwrap();
    nls.set_param(
        "convergence_type",
        akantu_parser::ParamValue::Enum("solution".into()),
    )
    .unwrap();
    model.smm.solve_step(None).unwrap();
    // the enriched DOFs are not nodal values: use the real displacement
    model.compute_values_on_enriched_nodes().unwrap();
    let u = model.real_displacement().unwrap();
    (0..pos.len())
        .map(|i| {
            (u.tuple(i)[0] - (0.5 * pos.tuple(i)[0] + 0.5))
                .abs()
                .max(u.tuple(i)[1].abs())
        })
        .fold(0.0, Real::max)
}

/// Mutation 1 of a move commits; mutation 2 is refused at the
/// capability check (a `real_force` of the wrong length makes the IGFEM
/// handler's compaction plan fail, standing in for any stage-3b refusal).
/// The refusal is recoverable and the mesh keeps the previous enriched
/// nodes as orphans; the generator's "removal owed" flag is still set (it is
/// cleared only after mutation 2 has committed), so the next `update` runs
/// its phase (ii) removal first, ends with the generator's mesh and the
/// patch criterion of TEST-192 holds.
#[test]
fn refused_second_mutation_is_recoverable_and_the_removal_stays_owed() {
    let mesh = read_mesh();
    let nb_standard = mesh.nb_nodes();
    let parser = Parser::parse_str(DAT, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModelIgfem::new(SharedMesh::new(mesh), Some(2), DEFAULT_ID, parser).unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    model
        .register_geometry_object(circle(0.25), "inside")
        .unwrap();
    model.update("").unwrap();
    assert!(
        !model.enrichment.intersector().has_pending_removal(),
        "a completed update owes nothing"
    );
    let e1 = model.mesh().read().unwrap().nb_nodes() - nb_standard;
    // the move by hand: mutation 1
    model.enrichment.set_radius(0.5 * (1.0 - 1e-9), "").unwrap();
    model.publish_geometry().unwrap();
    model
        .mutate_with_handler(|tx, e| e.update_additions(tx, ""))
        .unwrap();
    model.smm.reassign_material().unwrap();
    let after_1 = model.mesh().read().unwrap().nb_nodes();
    assert!(model.enrichment.intersector().has_pending_removal());
    // a stage-3b refusal of mutation 2
    let good = model.real_force.take().unwrap();
    model.real_force = Some(Array::try_zeros(good.len() + 1, 2).unwrap());
    let r = model.mutate_with_handler(|tx, e| e.remove_previous_enriched_nodes(tx));
    assert!(r.as_ref().is_err_and(|e| e.fatal_kind().is_none()), "{r:?}");
    assert_eq!(
        model.mesh().read().unwrap().nb_nodes(),
        after_1,
        "rolled back"
    );
    assert_eq!(
        after_1,
        nb_standard + 2 * e1,
        "previous enriched nodes orphaned"
    );
    // the removal is still owed after the refusal
    assert!(
        model.enrichment.intersector().has_pending_removal(),
        "the refused removal must stay owed"
    );
    model.real_force = Some(good);
    // the next update removes the orphans first (recovery block), then moves
    let pass = model.update("").unwrap();
    assert!(
        !model.enrichment.intersector().has_pending_removal(),
        "the completed update owes nothing"
    );
    assert!(pass.removed_nodes > 0, "{pass:?}");
    let (nodes, conn) = {
        let m = model.mesh().read().unwrap();
        (
            m.nodes().as_slice().to_vec(),
            m.connectivity(ElementType::IgfemTriangle5, GhostType::NotGhost)
                .map(|c| c.as_slice().to_vec())
                .unwrap_or_default(),
        )
    };
    // the generator alone, two passes: the live mesh equals it bit for bit
    let shared = SharedMesh::new(read_mesh());
    let mut gel =
        crate::growing_gel::MeshIgfemSphericalGrowingGel::new(&shared.read().unwrap()).unwrap();
    gel.init(&shared.read().unwrap()).unwrap();
    for s in [circle(0.25), circle(0.5 * (1.0 - 1e-9))] {
        shared
            .mutate(
                &mut [],
                |tx| gel.build_igfem_mesh_from_spheres(tx, &s).map(|_| ()),
                None,
            )
            .unwrap();
    }
    let g = shared.read().unwrap();
    assert_eq!(g.nodes().as_slice(), &nodes[..]);
    assert_eq!(
        g.connectivity(ElementType::IgfemTriangle5, GhostType::NotGhost)
            .map(|c| c.as_slice().to_vec())
            .unwrap_or_default(),
        conn
    );
    assert_eq!(model.real_force().unwrap().len(), g.nb_nodes());
    assert_eq!(model.smm.displacement().unwrap().len(), g.nb_nodes());
    // the patch criterion of TEST-192 holds on the recovered model
    let err = solve_patch_and_max_nodal_error(&mut model);
    assert!(err <= 1e-13, "The patch test did not pass: {err:e}");
}
