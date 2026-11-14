// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/patch_tests/
// test_interface_position.cc (+ test_interface_position.sh: 5 MPI ranks).
// Built only with the feature `cgal` (GPL-3.0-or-later sphere intersector,
// decision D-042).

//! TEST-192: patch test (homogeneous `E = 10`, exact `u_x = 0.5 x + 0.5`)
//! for three interface positions of a circle centred at the origin, moved
//! on a **live** model as upstream: `update()` with `r = 0.25`, then
//! `moveInterface(0.5 (1 − 1e-9))` (very close to the standard nodes,
//! 8 `_igfem_triangle_5`, 8 enriched nodes) and `moveInterface(0.5 (1 −
//! 1e-10))` (closer than the node tolerance: the circle "cuts through" the
//! nodes, 8 `_igfem_triangle_4`, 4 enriched nodes). Upstream criteria: area
//! 4, L2 error `≤ 1e-13` after each solve, the element counts after each
//! move.
//!
//! Each move is the IGFEM model's two mutations (D-095 rule 3): additions
//! and the erasure of the replaced elements, then the removal of the
//! previous enriched nodes (DOFs renumbered, SMM-015). Independent checks
//! beyond upstream's: after every move the live mesh equals, bit for bit,
//! the mesh the generator builds alone from the same passes, and the
//! live-moved solution equals the solution of a fresh model on that mesh
//! to rounding (`interface_move_equals_a_fresh_model`).
//!
//! Deviations (reported): serial only (the `.sh` runs 5 ranks; UD-609,
//! D-085). Upstream's enriched-node-count checks are guarded by `psize ==
//! 0`, which is never true (UD-615); they are checked here.

mod common;

use akantu_core::Real;
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use common::model::*;

const DAT: &str = include_str!("data/upstream/material_test_interface_position.dat");

fn circle(r: Real) -> Vec<SkSphere> {
    vec![SkSphere::try_from_f64([0.0, 0.0, 0.0], r * r).unwrap()]
}

const RADII: [Real; 3] = [0.25, 0.5 * (1.0 - 1e-9), 0.5 * (1.0 - 1e-10)];

/// `(tri4, tri5, enriched nodes)` expected after the moves (upstream counts;
/// the first position has no upstream count).
const EXPECTED: [Option<(usize, usize, usize)>; 3] = [None, Some((0, 8, 8)), Some((8, 0, 4))];

fn area_ok(model: &SolidMechanicsModelIgfem) {
    let area = model.volume().unwrap();
    assert!(
        akantu_core::math::are_float_equal(area, 4.0, akantu_core::math::DEFAULT_TOLERANCE),
        "Error in area computation of the 2D mesh: {area}"
    );
}

fn solve_and_check(model: &mut SolidMechanicsModelIgfem, stage: usize) -> Real {
    model.smm.solve_step(None).unwrap();
    let err = l2_error(model, &|x| 0.5 * x + 0.5);
    eprintln!("stage {stage}: L2 error {err:e}");
    assert!(err <= 1e-13, "The patch test did not pass: {err:e}");
    err
}

/// Upstream's program: one model, the interface created and moved twice.
#[test]
fn test_interface_position() {
    let mesh = common::read_msh(2, "test_interface_position.msh", "mesh");
    let nb_standard = mesh.nb_nodes();
    let mut model = model(mesh, DAT);
    init_static(&mut model);
    model
        .register_geometry_object(circle(RADII[0]), "inside")
        .unwrap();
    let pass = model.update("").unwrap();
    assert_eq!(pass.removed_nodes, 0, "first pass: nothing to remove");
    area_ok(&model);
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    solve_and_check(&mut model, 0);
    for stage in 1..3 {
        let enriched_before = model.mesh().read().unwrap().nb_nodes() - nb_standard;
        let pass = model.move_interface(RADII[stage], "").unwrap();
        let (c, enriched) = {
            let m = model.mesh().read().unwrap();
            (counts(&m), m.nb_nodes() - nb_standard)
        };
        eprintln!("stage {stage}: counts {c:?}, enriched nodes {enriched}, pass {pass:?}");
        let (t4, t5, n) = EXPECTED[stage].unwrap();
        assert_eq!(
            (c[1], c[2]),
            (t4, t5),
            "something went wrong in the interface creation"
        );
        assert_eq!(
            enriched, n,
            "something went wrong in the interface node creation"
        );
        // the previous enriched nodes were removed by the second mutation
        assert_eq!(pass.removed_nodes, enriched_before);
        assert_eq!(pass.new_nodes, n);
        // the nodal arrays follow the node count
        let nn = nb_standard + n;
        assert_eq!(model.smm.displacement().unwrap().len(), nn);
        assert_eq!(model.smm.blocked_dofs().unwrap().len(), nn);
        assert_eq!(model.real_displacement().unwrap().len(), nn);
        assert_eq!(model.real_force().unwrap().len(), nn);
        assert_eq!(model.real_residual().unwrap().len(), nn);
        area_ok(&model);
        solve_and_check(&mut model, stage);
    }
}

/// Independent check of the live move: after every move the mesh is the
/// one the generator builds alone from the same passes (nodes and
/// connectivities bit for bit), and the solution of the live model equals
/// the solution of a fresh model built on that mesh from the same starting
/// point and boundary conditions, to rounding (the node removal renumbers
/// the DOFs as a fresh registration; the laws' filter orders differ).
#[test]
fn interface_move_equals_a_fresh_model() {
    let mesh = common::read_msh(2, "test_interface_position.msh", "mesh");
    let mut live = model(mesh, DAT);
    init_static(&mut live);
    live.register_geometry_object(circle(RADII[0]), "inside")
        .unwrap();
    live.update("").unwrap();
    patch_bcs(&mut live);
    set_newton(&mut live, 2, 1e-12);
    live.smm.solve_step(None).unwrap();
    for stage in 1..3 {
        live.move_interface(RADII[stage], "").unwrap();
        // same starting point as the fresh model (Newton's first increment
        // depends on it at rounding level): zero displacement, same BCs
        live.smm
            .displacement_mut()
            .unwrap()
            .as_mut_slice()
            .fill(0.0);
        patch_bcs(&mut live);
        live.smm.solve_step(None).unwrap();
        let passes: Vec<Vec<SkSphere>> = RADII[..=stage].iter().map(|&r| circle(r)).collect();
        let (mut fresh, _, _) =
            model_on_generated("test_interface_position.msh", DAT, &passes, "inside", None);
        {
            let (a, b) = (live.mesh().read().unwrap(), fresh.mesh().read().unwrap());
            assert_eq!(a.nodes().as_slice(), b.nodes().as_slice(), "stage {stage}");
            for ty in akantu_igfem::growing_gel::ELEMENT_LIST {
                let g = akantu_core::GhostType::NotGhost;
                let conn = |m: &akantu_mesh::Mesh| {
                    m.connectivity(ty, g)
                        .map(|c| c.as_slice().to_vec())
                        .unwrap_or_default()
                };
                assert_eq!(conn(&a), conn(&b), "stage {stage}: {ty:?}");
            }
        }
        patch_bcs(&mut fresh);
        set_newton(&mut fresh, 2, 1e-12);
        fresh.smm.solve_step(None).unwrap();
        let (u_live, u_fresh) = (
            live.smm.displacement().unwrap().as_slice().to_vec(),
            fresh.smm.displacement().unwrap().as_slice().to_vec(),
        );
        let max_diff = u_live
            .iter()
            .zip(&u_fresh)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, Real::max);
        let u_max = u_fresh.iter().map(|v| v.abs()).fold(0.0, Real::max);
        eprintln!("stage {stage}: max |u_live − u_fresh| = {max_diff:e} (max |u| {u_max})");
        // not bitwise: the live laws keep their filter order on erasure
        // while a fresh filter is sorted, so the same element matrices are
        // assembled in another order; bound: upstream's
        // patch criterion of this test (1e-13) on the difference
        assert!(
            max_diff <= 1e-13 * u_max,
            "stage {stage}: live move vs fresh model: {max_diff:e}"
        );
    }
}
