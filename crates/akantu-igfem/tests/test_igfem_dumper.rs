// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/solid_mechanics_model_igfem.cc
// (the "igfem elements" dumper: constructor, addDumpGroupFieldToDumper,
// onDump, createElementalField, createNodalFieldReal) and the dumper
// adaptors dumper_igfem_*.hh; driven like the dump calls of upstream's
// IGFEM tests (test_interface_position.cc, test_igfem_triangle_4.cc).
// Built only with the feature `cgal` (GPL-3.0-or-later sphere intersector,
// decision D-042).

//! IGFEM-010: the model-level `"igfem elements"` dumper. No
//! upstream output can be compared (the package does not compile),
//! so the written file is checked against the sub-element mesh computed in
//! the test: on the `_igfem_triangle_4` patch test (four elements cut by the
//! straight interface `x = 0.5`, sub-materials `E = 10` / `E = 1`, exact
//! piecewise-linear solution) the VTU piece must hold every node of the
//! model and one cell per sub-element with the sub-element connectivity;
//! `material_index` the IGFEM law on both sub-elements; `mu` the shear
//! modulus of each sub-element's side; `stress` and `strain` the average
//! of the law's points of each sub-element (`strain` derived per point
//! from `grad_u`); `real_displacement` the exact solution at every node,
//! the enriched ones included (IGFEM interpolation of `onDump`).

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use akantu_igfem::dump::{IGFEM_DUMPER, sub_element_mesh};
use common::model::*;

const DAT: &str = include_str!("data/upstream/material.dat");

/// The arrays of an ASCII VTU piece by name.
fn parse_vtu(path: &std::path::Path) -> BTreeMap<String, Vec<f64>> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = BTreeMap::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<DataArray") {
        rest = &rest[i..];
        let head_end = rest.find('>').unwrap();
        let head = &rest[..head_end];
        let name = head
            .split("Name=\"")
            .nth(1)
            .and_then(|t| t.split('"').next())
            .unwrap()
            .to_owned();
        assert!(head.contains("format=\"ascii\""), "{head}");
        let body_end = rest.find("</DataArray>").unwrap();
        let body = &rest[head_end + 1..body_end];
        let v: Vec<f64> = body
            .split_ascii_whitespace()
            .map(|t| t.parse().unwrap())
            .collect();
        out.insert(name, v);
        rest = &rest[body_end..];
    }
    out
}

/// Equal to the 16 significant digits the ASCII VTU writer prints.
fn printed_eq(file: f64, value: f64) -> bool {
    (file - value).abs() <= 1e-15 * value.abs().max(1e-300) || file == value
}

fn vtu_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(vtu_files(&p));
        } else if p.extension().is_some_and(|x| x == "vtu") {
            v.push(p);
        }
    }
    v.sort();
    v
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("akantu-igfem-dump-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The `_igfem_triangle_4` patch test, solved.
fn solved_patch_model() -> SolidMechanicsModelIgfem {
    let mesh = common::read_msh(2, "test_mesh.msh", "mesh");
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
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    model
}

fn exact(x: Real) -> Real {
    let (e1, e2) = (10.0, 1.0);
    if x < 0.5 {
        (2. * x * e2) / (e1 + 3. * e2) + (2. * e2) / (e1 + 3. * e2)
    } else {
        2. * x * e1 / (e1 + 3. * e2) - (e1 - 3. * e2) / (e1 + 3. * e2)
    }
}

#[test]
fn igfem_dumper_writes_the_sub_element_mesh() {
    let mut model = solved_patch_model();
    let dir = scratch("split");
    {
        let d = &mut model.smm.dumpers.dumpable;
        d.set_directory_to_dumper(IGFEM_DUMPER, &dir).unwrap();
        d.set_text_mode_to_dumper(IGFEM_DUMPER).unwrap();
    }
    for f in [
        "material_index",
        "mu",
        "stress",
        "strain",
        "partitions",
        "blocked_dofs",
    ] {
        model.add_dump_field_to_dumper(IGFEM_DUMPER, f).unwrap();
    }
    for f in ["displacement", "real_displacement"] {
        model
            .add_dump_field_vector_to_dumper(IGFEM_DUMPER, f)
            .unwrap();
    }
    model.dump_named(IGFEM_DUMPER, None, None).unwrap();
    let files = vtu_files(&dir);
    assert_eq!(files.len(), 1, "{files:?}");
    let vtu = parse_vtu(&files[0]);

    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let sub = sub_element_mesh(&mesh, GhostType::NotGhost).unwrap();
    let nb_nodes = mesh.nb_nodes();
    let n_igfem = mesh.nb_element(ElementType::IgfemTriangle4, GhostType::NotGhost);
    assert_eq!(n_igfem, 4);
    // points: every node (3-D positions)
    let pos = &vtu["positions"];
    assert_eq!(pos.len(), 3 * nb_nodes);
    let nodes = mesh.nodes();
    for n in 0..nb_nodes {
        for d in 0..2 {
            assert!(printed_eq(pos[3 * n + d], nodes.tuple(n)[d]), "node {n}");
        }
    }
    // cells: one per sub-element, the sub-element connectivity (VTK 5 =
    // triangle), in the sub-element mesh's order
    let cells = sub.cells(ElementType::Triangle3).to_vec();
    assert_eq!(cells.len(), 2 * n_igfem);
    assert!(vtu["types"].iter().all(|&t| t == 5.0), "{:?}", vtu["types"]);
    let conn: Vec<f64> = sub
        .mesh
        .connectivity(ElementType::Triangle3, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|&v| f64::from(v))
        .collect();
    assert_eq!(vtu["connectivity"], conn);
    let ty = ElementType::IgfemTriangle4;
    let law = law_of(
        &model,
        Element::try_new(ty, 0, GhostType::NotGhost).unwrap(),
    );
    let sub_mat = uint_internal(&model, law, "sub_material", ty);
    let stress = real_internal(&model, law, "stress", ty);
    let grad_u = real_internal(&model, law, "grad_u", ty);
    let nq = model
        .ctx
        .engine()
        .unwrap()
        .nb_integration_points(ty)
        .unwrap();
    assert_eq!(nq, 2, "one point per triangular sub-element");
    for (i, &(el, s)) in cells.iter().enumerate() {
        let (l, local) = law_and_local(&model, el);
        assert_eq!(vtu["material_index"][i], l as f64, "cell {i}");
        assert_eq!(vtu["partitions"][i], 0.0);
        let q = local * nq + s;
        // mu of the sub-element's side (E = 10 or 1, nu = 0: mu = E / 2)
        let expect_mu = real_internal(&model, law, "mu", ty)[q];
        assert_eq!(vtu["mu"][i], expect_mu, "cell {i}");
        let side_mu = if sub_mat[q] == 0 { 0.5 } else { 5.0 };
        assert_eq!(expect_mu, side_mu, "cell {i}: sub-material {}", sub_mat[q]);
        // one point per sub-element: the average is the point's value
        for k in 0..4 {
            assert!(
                printed_eq(vtu["stress"][4 * i + k], stress[4 * q + k]),
                "cell {i} stress {k}: {} vs {}",
                vtu["stress"][4 * i + k],
                stress[4 * q + k]
            );
        }
        let g = &grad_u[4 * q..4 * q + 4];
        let eps = [g[0], 0.5 * (g[1] + g[2]), 0.5 * (g[2] + g[1]), g[3]];
        let w = &vtu["strain"][4 * i..4 * i + 4];
        for k in 0..4 {
            assert!(
                (w[k] - eps[k]).abs() <= 1e-15 * (1. + eps[k].abs()),
                "cell {i} strain"
            );
        }
    }
    // the two sub-elements of every element lie on both sides
    let mu_out = &vtu["mu"];
    for e in 0..n_igfem {
        assert_ne!(mu_out[2 * e], mu_out[2 * e + 1], "element {e}: two sides");
    }
    // real_displacement: the exact solution at every node (enriched nodes
    // interpolated; the patch test is exact to rounding)
    let rd = &vtu["real_displacement"];
    let u = &vtu["displacement"];
    assert_eq!(rd.len(), 3 * nb_nodes);
    let nb_standard = model.enrichment.nb_standard_nodes();
    for n in 0..nb_nodes {
        let x = nodes.tuple(n)[0];
        assert!(
            (rd[3 * n] - exact(x)).abs() <= 1e-13,
            "node {n}: real_displacement {} vs {}",
            rd[3 * n],
            exact(x)
        );
        if n < nb_standard {
            assert!(printed_eq(rd[3 * n], u[3 * n]), "standard node {n}");
        }
    }
    assert!(nb_nodes > nb_standard, "enriched nodes are written");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The route through the solid model writes the same split mesh but
/// refuses `real_displacement`; the regular dumper of the IGFEM model has
/// `real_displacement` (upstream `createNodalFieldReal` override); refused
/// registrations (UD-1208).
#[test]
fn igfem_dumper_route_and_refusals() {
    let mut model = solved_patch_model();
    let dir = scratch("route");
    {
        let d = &mut model.smm.dumpers.dumpable;
        d.set_directory_to_dumper(IGFEM_DUMPER, &dir).unwrap();
        d.set_directory(&dir).unwrap();
        d.set_text_mode_to_dumper(IGFEM_DUMPER).unwrap();
        d.set_base_name_to_dumper(IGFEM_DUMPER, "igfem_elements")
            .unwrap();
    }
    model
        .add_dump_field_to_dumper(IGFEM_DUMPER, "material_index")
        .unwrap();
    model.smm.dump_named(IGFEM_DUMPER, None, None).unwrap();
    let files = vtu_files(&dir);
    assert_eq!(files.len(), 1, "{files:?}");
    let vtu = parse_vtu(&files[0]);
    assert_eq!(
        vtu["types"].len(),
        8,
        "eight sub-element cells through the route"
    );
    model
        .add_dump_field_vector_to_dumper(IGFEM_DUMPER, "real_displacement")
        .unwrap();
    let e = model.smm.dump_named(IGFEM_DUMPER, None, None).unwrap_err();
    assert!(format!("{e}").contains("real_displacement"), "{e}");
    model.dump_named(IGFEM_DUMPER, None, None).unwrap();
    // refusals on the IGFEM dumper
    assert!(
        model
            .add_dump_field_vector_to_dumper(IGFEM_DUMPER, "stress")
            .is_err()
    );
    assert!(
        model
            .add_dump_field_to_dumper(IGFEM_DUMPER, "no_such_field")
            .is_err()
    );
    assert!(
        model
            .add_dump_group_field_to_dumper(IGFEM_DUMPER, "stress", "some_group", false)
            .is_err()
    );
    // the default dumper: real_displacement besides the solid fields
    model.add_dump_field_vector("real_displacement").unwrap();
    model.add_dump_field_vector("displacement").unwrap();
    model.smm.dumpers.dumpable.set_text_mode().unwrap();
    model.dump(None, None).unwrap();
    let regular: Vec<PathBuf> = vtu_files(&dir)
        .into_iter()
        .filter(|p| !p.to_string_lossy().contains("igfem_elements"))
        .collect();
    assert_eq!(regular.len(), 1, "{regular:?}");
    let vtu = parse_vtu(&regular[0]);
    let nb_nodes = model.mesh().read().unwrap().nb_nodes();
    assert_eq!(vtu["real_displacement"].len(), 3 * nb_nodes);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `_igfem_triangle_5` (TEST-192's second position, 8 elements): the
/// quadrangle sub-elements (VTK 9) hold the average of their 4 points.
#[test]
fn igfem_dumper_averages_quadrangle_sub_elements() {
    let dat = include_str!("data/upstream/material_test_interface_position.dat");
    let mesh = common::read_msh(2, "test_interface_position.msh", "mesh");
    let mut model = model(mesh, dat);
    init_static(&mut model);
    let r: Real = 0.5 * (1.0 - 1e-9);
    model
        .register_geometry_object(
            vec![SkSphere::try_from_f64([0., 0., 0.], 0.25 * 0.25).unwrap()],
            "inside",
        )
        .unwrap();
    model.update("").unwrap();
    model.move_interface(r, "").unwrap();
    patch_bcs(&mut model);
    set_newton(&mut model, 2, 1e-12);
    model.smm.solve_step(None).unwrap();
    let dir = scratch("tri5");
    {
        let d = &mut model.smm.dumpers.dumpable;
        d.set_directory_to_dumper(IGFEM_DUMPER, &dir).unwrap();
        d.set_text_mode_to_dumper(IGFEM_DUMPER).unwrap();
    }
    model
        .add_dump_field_to_dumper(IGFEM_DUMPER, "stress")
        .unwrap();
    model.dump_named(IGFEM_DUMPER, None, None).unwrap();
    let files = vtu_files(&dir);
    let vtu = parse_vtu(&files[0]);
    let ty = ElementType::IgfemTriangle5;
    let mesh_h = model.mesh().clone();
    let sub = sub_element_mesh(&mesh_h.read().unwrap(), GhostType::NotGhost).unwrap();
    let (tris, quads) = (
        sub.cells(ElementType::Triangle3).to_vec(),
        sub.cells(ElementType::Quadrangle4).to_vec(),
    );
    assert_eq!((tris.len(), quads.len()), (8, 8));
    let types = &vtu["types"];
    assert!(types[..8].iter().all(|&t| t == 5.0) && types[8..].iter().all(|&t| t == 9.0));
    let law = law_of(
        &model,
        Element::try_new(ty, 0, GhostType::NotGhost).unwrap(),
    );
    let stress = real_internal(&model, law, "stress", ty);
    let nq = model
        .ctx
        .engine()
        .unwrap()
        .nb_integration_points(ty)
        .unwrap();
    assert_eq!(nq, 5, "1 + 4 points");
    for (i, &(el, s)) in tris.iter().chain(&quads).enumerate() {
        let (_, local) = law_and_local(&model, el);
        let pts: Vec<usize> = if s == 0 { vec![0] } else { vec![1, 2, 3, 4] };
        for k in 0..4 {
            let mut avg = 0.0;
            for &q in &pts {
                avg += stress[(local * nq + q) * 4 + k];
            }
            avg /= pts.len() as Real;
            let w = vtu["stress"][4 * i + k];
            assert!(
                (w - avg).abs() <= 1e-15 * avg.abs().max(1.0),
                "cell {i} component {k}: {w} vs {avg}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
