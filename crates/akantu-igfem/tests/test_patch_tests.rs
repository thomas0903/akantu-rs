// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/patch_tests/
// test_igfem_triangle_4.cc, test_igfem_triangle_5.cc. Built only with the
// feature `cgal` (the interface is created with the spherical growing-gel
// generator, which links the GPL-3.0-or-later crate akantu-geometry-cgal,
// decision D-042).

//! TEST-191 / TEST-190: bi-material patch tests with a straight interface
//! (`E₁ = 10`, `E₂ = 1`, `ν = 0`), imposed displacement `u_x = 1` on the
//! right side, rollers left and bottom. The exact solution is piecewise
//! linear in `x` with a kink at the interface; IGFEM reproduces it exactly
//! (upstream criteria: area `4`, L2 error `≤ 1e-14` (tri4) / `≤ 1e-12`
//! (tri5)).

mod common;

use akantu_core::Real;
use akantu_geometry_cgal::SkSphere;
use common::model::*;
use common::{frozen_patch_plate, read_msh, read_msh_at};

const DAT: &str = include_str!("data/upstream/material.dat");

/// `test_igfem_triangle_4.cc`: a huge sphere approximates the straight
/// interface `x = 0.5` of `test_mesh.msh`; 4 `_igfem_triangle_4` expected.
#[test]
fn test_igfem_triangle_4() {
    let mesh = read_msh(2, "test_mesh.msh", "mesh");
    let mut model = model(mesh, DAT);
    straight_interface_selector(&mut model, 2);
    init_static(&mut model);
    let sphere = SkSphere::try_from_f64(
        [1_000_000_000.5, 0.0, 0.0],
        1_000_000_000.0 * 1_000_000_000.0,
    )
    .unwrap();
    model
        .register_geometry_object(vec![sphere], "inside")
        .unwrap();
    let pass = model.update("").unwrap();
    let c = counts(&model.mesh().read().unwrap());
    eprintln!("tri4 patch test: {pass:?}, counts {c:?}");
    assert_eq!(
        c[1..],
        [4, 0],
        "something went wrong in the interface creation"
    );
    patch_bcs(&mut model);
    let area = model.volume().unwrap();
    assert!(
        akantu_core::math::are_float_equal(area, 4.0, akantu_core::math::DEFAULT_TOLERANCE),
        "{area}"
    );
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let (e1, e2) = (10.0, 1.0);
    let exact = |x: Real| {
        if x < 0.5 {
            (2. * x * e2) / (e1 + 3. * e2) + (2. * e2) / (e1 + 3. * e2)
        } else {
            2. * x * e1 / (e1 + 3. * e2) - (e1 - 3. * e2) / (e1 + 3. * e2)
        }
    };
    let err = l2_error(&model, &exact);
    eprintln!("tri4 patch test: L2 error {err:e}");
    assert!(err <= 1e-14, "The patch test did not pass: {err:e}");
}

/// `test_igfem_triangle_5.cc`: the sphere `(6.57, 0), r = 6.32` cuts
/// `plate.msh` near `x = 0.25`; the interface nodes are then moved onto the
/// straight line `x = 0.25` (`y` rounded to multiples of `0.25`) and the
/// IGFEM shapes re-initialised. Upstream creates the model first and edits
/// the node coordinates of the model's mesh; akantu-rs has no coordinate
/// edit for a shared mesh with consumers, so the interface is generated
/// and adjusted on the mesh first and the model created afterwards (the
/// IGFEM elements then exist at `initFull`, UD-610). Same final state.
#[test]
fn test_igfem_triangle_5() {
    use akantu_igfem::growing_gel::MeshIgfemSphericalGrowingGel;
    use akantu_mesh::{MeshAccessor, MeshEvents, SharedMesh};
    let mesh = read_msh_at(2, &frozen_patch_plate(), "mesh");
    let shared = SharedMesh::new(mesh);
    let nb_regular_nodes = shared.read().unwrap().nb_nodes();
    let sphere = SkSphere::try_from_f64([6.57, 0.0, 0.0], 6.32 * 6.32).unwrap();
    {
        let mut gel = MeshIgfemSphericalGrowingGel::new(&shared.read().unwrap()).unwrap();
        gel.init(&shared.read().unwrap()).unwrap();
        let spheres = [sphere.clone()];
        shared
            .mutate(
                &mut [],
                |tx| gel.build_igfem_mesh_from_spheres(tx, &spheres).map(|_| ()),
                None,
            )
            .unwrap();
    }
    shared
        .setup(|m| {
            let mut acc = MeshAccessor::new(m);
            let mut nodes = acc.nodes_mut();
            for i in nb_regular_nodes..nodes.len() {
                let c = nodes.tuple_mut(i);
                c[0] = 0.25;
                let multiplier = (c[1] / 0.25).round();
                c[1] = 0.25 * multiplier;
            }
            Ok(MeshEvents::none())
        })
        .unwrap();
    let c = counts(&shared.read().unwrap());
    eprintln!(
        "tri5 patch test: counts {c:?}, enriched nodes {}",
        shared.read().unwrap().nb_nodes() - nb_regular_nodes
    );
    let parser = akantu_parser::Parser::parse_str(DAT, "material.dat").unwrap();
    let mut model = akantu_igfem::SolidMechanicsModelIgfem::new(
        shared,
        Some(2),
        akantu_igfem::model::DEFAULT_ID,
        parser,
    )
    .unwrap();
    straight_interface_selector(&mut model, 2);
    model
        .register_geometry_object(vec![sphere], "inside")
        .unwrap();
    init_static(&mut model);
    patch_bcs(&mut model);
    let area = model.volume().unwrap();
    assert!(
        akantu_core::math::are_float_equal(area, 4.0, akantu_core::math::DEFAULT_TOLERANCE),
        "{area}"
    );
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let (e1, e2) = (10.0, 1.0);
    let exact = |x: Real| {
        if x < 0.25 {
            (4. * x * e2) / (3. * e1 + 5. * e2) + (4. * e2) / (3. * e1 + 5. * e2)
        } else {
            4. * x * e1 / (3. * e1 + 5. * e2) - (e1 - 5. * e2) / (3. * e1 + 5. * e2)
        }
    };
    let err = l2_error(&model, &exact);
    eprintln!("tri5 patch test: L2 error {err:e}");
    assert!(err <= 1e-12, "The patch test did not pass: {err:e}");
}

/// A second interface update (enriched nodes present; the
/// live re-meshing of two mutations, D-095) with the same geometry
/// re-creates the same interface: same counts, same node coordinates (the
/// enriched nodes removed and re-created at the same positions), and the
/// patch test still passes with upstream's criterion; the previous enriched
/// nodes are removed by the second mutation.
#[test]
fn second_update_recreates_the_interface() {
    let mesh = read_msh(2, "test_mesh.msh", "mesh");
    let mut model = model(mesh, DAT);
    straight_interface_selector(&mut model, 2);
    init_static(&mut model);
    let sphere = SkSphere::try_from_f64(
        [1_000_000_000.5, 0.0, 0.0],
        1_000_000_000.0 * 1_000_000_000.0,
    )
    .unwrap();
    model
        .register_geometry_object(vec![sphere], "inside")
        .unwrap();
    model.update("").unwrap();
    let before = counts(&model.mesh().read().unwrap());
    let nodes = model.mesh().read().unwrap().nodes().as_slice().to_vec();
    let pass = model.update("").unwrap();
    assert_eq!(pass.removed_nodes, pass.new_nodes);
    assert!(pass.new_nodes > 0);
    assert_eq!(counts(&model.mesh().read().unwrap()), before);
    assert_eq!(model.mesh().read().unwrap().nodes().as_slice(), &nodes[..]);
    assert!(model.mesh().poisoned().unwrap().is_none());
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let (e1, e2) = (10.0, 1.0);
    let exact = |x: Real| {
        if x < 0.5 {
            (2. * x * e2) / (e1 + 3. * e2) + (2. * e2) / (e1 + 3. * e2)
        } else {
            2. * x * e1 / (e1 + 3. * e2) - (e1 - 3. * e2) / (e1 + 3. * e2)
        }
    };
    let err = l2_error(&model, &exact);
    eprintln!("second update, tri4 patch test: L2 error {err:e}");
    assert!(err <= 1e-14, "The patch test did not pass: {err:e}");
}
