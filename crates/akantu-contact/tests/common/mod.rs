// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helpers of the
// contact mechanics tests (meshes of test/test_model/test_contact_mechanics_model
// frozen in fixtures/meshes, programmatic two-block meshes).

#![allow(dead_code)] // each test binary uses a subset

use std::path::{Path, PathBuf};

use akantu_contact::{ContactMechanicsModel, PhysicalSurfaceSelector};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_mesh::utils::fill_element_to_subelements_data;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_parser::Parser;
use akantu_validation::fixtures_root;

/// Frozen upstream test mesh `test/test_model/test_contact_mechanics_model/<rel>`.
pub fn contact_mesh_path(rel: &str) -> PathBuf {
    fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/test_contact_mechanics_model")
        .join(rel)
}

/// Read a Gmsh mesh.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh).unwrap();
    mesh
}

/// `contact_detector` + `penalty_linear` input text.
pub fn contact_dat(
    master: &str,
    slave: &str,
    mu: Real,
    eps_n: Real,
    eps_t: Real,
    deformable: bool,
) -> String {
    format!(
        "contact_detector [\n  type = explicit\n  master = {master}\n  slave = {slave}\n  \
         projection_tolerance = 1e-10\n  max_iterations = 100\n  extension_tolerance = 1e-5\n]\n\n\
         contact_resolution penalty_linear [\n  name = contact\n  mu = {mu}\n  epsilon_n = {eps_n}\n  \
         epsilon_t = {eps_t}\n  is_master_deformable = {deformable}\n]\n"
    )
}

/// Contact model on `mesh` with input text `dat`, initialised (explicit
/// lumped) and given a physical surface selector.
pub fn contact_model(mesh: Mesh, dat: &str) -> (SharedMesh, ContactMechanicsModel) {
    let parser = Parser::parse_str(dat, "contact.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let shared = SharedMesh::new(mesh);
    let mut model =
        ContactMechanicsModel::with_parser(shared.clone(), None, "contact_mechanics_model", parser)
            .unwrap();
    model.init_full(AnalysisMethod::ExplicitLumpedMass).unwrap();
    model.set_surface_selector(Box::new(selector)).unwrap();
    (shared, model)
}

/// Nodes of the elements whose `physical_names` is `name`.
pub fn nodes_of_physical(mesh: &Mesh, name: &str) -> Vec<akantu_core::Idx> {
    let g = mesh.groups().element_group(name).unwrap();
    g.nodes().to_vec()
}

/// Add an element group `name` of dimension `dim` with the given elements
/// (and their nodes), optimised.
pub fn add_group(mesh: &mut Mesh, name: &str, dim: usize, elements: &[Element]) {
    mesh.groups_mut().create_element_group(name, dim).unwrap();
    for &e in elements {
        mesh.element_group_add(name, e, true, true).unwrap();
    }
    mesh.groups_mut()
        .element_group_mut(name)
        .unwrap()
        .optimize();
}

/// Two 2-D blocks with distinct interface nodes: `lower` = `nx_l` quads on
/// `[-wl, wl] × [-1, 0]`, `upper` = `nx_u` quads on `[-wu, wu] × [0, 1]`;
/// groups `master` (top edges of lower), `slave` (bottom edges of upper),
/// `lower_body`, `upper_body`.
pub fn two_blocks_2d(nx_l: usize, wl: Real, nx_u: usize, wu: Real) -> Mesh {
    let mut m = Mesh::new(2, "two_blocks_2d").unwrap();
    let q = ElementType::Quadrangle4;
    let s = ElementType::Segment2;
    let g = GhostType::NotGhost;
    let mut quads = Vec::new();
    let mut segs: Vec<(Vec<i32>, &str)> = Vec::new();
    {
        let mut a = MeshAccessor::new(&mut m);
        let mut grid = |nx: usize, x0: Real, w: Real, y0: Real| -> (Vec<i32>, Vec<i32>) {
            let mut bottom = Vec::new();
            let mut top = Vec::new();
            for j in 0..=nx {
                #[allow(clippy::cast_precision_loss)]
                let x = x0 + 2.0 * w * (j as Real) / (nx as Real);
                bottom.push(a.push_node(&[x, y0]).unwrap());
            }
            for j in 0..=nx {
                #[allow(clippy::cast_precision_loss)]
                let x = x0 + 2.0 * w * (j as Real) / (nx as Real);
                top.push(a.push_node(&[x, y0 + 1.0]).unwrap());
            }
            (bottom, top)
        };
        let (lb, lt) = grid(nx_l, -wl, wl, -1.0);
        let (ub, ut) = grid(nx_u, -wu, wu, 0.0);
        for j in 0..nx_l {
            quads.push((vec![lb[j], lb[j + 1], lt[j + 1], lt[j]], "lower_body"));
            segs.push((vec![lt[j], lt[j + 1]], "master"));
        }
        for j in 0..nx_u {
            quads.push((vec![ub[j], ub[j + 1], ut[j + 1], ut[j]], "upper_body"));
            segs.push((vec![ub[j], ub[j + 1]], "slave"));
        }
        for (c, _) in &quads {
            a.connectivity_mut(q, g).push_tuple(c);
        }
        for (c, _) in &segs {
            a.connectivity_mut(s, g).push_tuple(c);
        }
    }
    for name in ["lower_body", "upper_body"] {
        let els: Vec<Element> = quads
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| *n == name)
            .map(|(i, _)| Element::try_new(q, i, g).unwrap())
            .collect();
        add_group(&mut m, name, 2, &els);
    }
    for name in ["master", "slave"] {
        let els: Vec<Element> = segs
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| *n == name)
            .map(|(i, _)| Element::try_new(s, i, g).unwrap())
            .collect();
        add_group(&mut m, name, 1, &els);
    }
    fill_element_to_subelements_data(&mut m).unwrap();
    m
}

/// Hexahedron node offsets (Akantu `_hexahedron_8` order).
const HEX: [[Real; 3]; 8] = [
    [0., 0., 0.],
    [1., 0., 0.],
    [1., 1., 0.],
    [0., 1., 0.],
    [0., 0., 1.],
    [1., 0., 1.],
    [1., 1., 1.],
    [0., 1., 1.],
];

/// Two 3-D blocks with distinct interface nodes: `lower` = `n_l × n_l`
/// cells on `[-wl, wl]² × [-1, 0]`, `upper` = `n_u × n_u` cells on
/// `[-wu, wu]² × [0, 1]` (normal along z); cells are hexahedra
/// (`tets = false`, facets `_quadrangle_4`) or 6 tetrahedra each (facets
/// `_triangle_3`). Groups `master` (top facets of lower), `slave` (bottom
/// facets of upper), `lower_body`, `upper_body`.
pub fn two_blocks_3d(n_l: usize, wl: Real, n_u: usize, wu: Real, tets: bool) -> Mesh {
    let mut m = Mesh::new(3, "two_blocks_3d").unwrap();
    let g = GhostType::NotGhost;
    let (vt, ft) = if tets {
        (ElementType::Tetrahedron4, ElementType::Triangle3)
    } else {
        (ElementType::Hexahedron8, ElementType::Quadrangle4)
    };
    let mut vols: Vec<(Vec<i32>, &str)> = Vec::new();
    let mut facets: Vec<(Vec<i32>, &str)> = Vec::new();
    {
        let mut a = MeshAccessor::new(&mut m);
        let mut block =
            |n: usize, w: Real, z0: Real, body: &'static str, face: &'static str, top: bool| {
                let mut ids = vec![vec![vec![0i32; 2]; n + 1]; n + 1];
                #[allow(clippy::cast_precision_loss)]
                let h = 2.0 * w / n as Real;
                for (i, row) in ids.iter_mut().enumerate() {
                    for (j, col) in row.iter_mut().enumerate() {
                        for (k, id) in col.iter_mut().enumerate() {
                            #[allow(clippy::cast_precision_loss)]
                            let p = [-w + h * i as Real, -w + h * j as Real, z0 + k as Real];
                            *id = a.push_node(&p).unwrap();
                        }
                    }
                }
                for i in 0..n {
                    for j in 0..n {
                        let node =
                            |o: [Real; 3]| ids[i + o[0] as usize][j + o[1] as usize][o[2] as usize];
                        let c: Vec<i32> = HEX.iter().map(|&o| node(o)).collect();
                        let face_nodes: Vec<i32> = if top {
                            vec![c[4], c[5], c[6], c[7]]
                        } else {
                            vec![c[0], c[1], c[2], c[3]]
                        };
                        if tets {
                            for t in [
                                [0, 1, 2, 6],
                                [0, 2, 3, 6],
                                [0, 3, 7, 6],
                                [0, 7, 4, 6],
                                [0, 4, 5, 6],
                                [0, 5, 1, 6],
                            ] {
                                vols.push((t.iter().map(|&k| c[k]).collect(), body));
                            }
                            if top {
                                facets.push((vec![c[4], c[5], c[6]], face));
                                facets.push((vec![c[4], c[6], c[7]], face));
                            } else {
                                facets.push((vec![c[0], c[1], c[2]], face));
                                facets.push((vec![c[0], c[2], c[3]], face));
                            }
                        } else {
                            vols.push((c, body));
                            facets.push((face_nodes, face));
                        }
                    }
                }
            };
        block(n_l, wl, -1.0, "lower_body", "master", true);
        block(n_u, wu, 0.0, "upper_body", "slave", false);
        for (c, _) in &vols {
            a.connectivity_mut(vt, g).push_tuple(c);
        }
        for (c, _) in &facets {
            a.connectivity_mut(ft, g).push_tuple(c);
        }
    }
    for name in ["lower_body", "upper_body"] {
        let els: Vec<Element> = vols
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| *n == name)
            .map(|(i, _)| Element::try_new(vt, i, g).unwrap())
            .collect();
        add_group(&mut m, name, 3, &els);
    }
    for name in ["master", "slave"] {
        let els: Vec<Element> = facets
            .iter()
            .enumerate()
            .filter(|(_, (_, n))| *n == name)
            .map(|(i, _)| Element::try_new(ft, i, g).unwrap())
            .collect();
        add_group(&mut m, name, 2, &els);
    }
    fill_element_to_subelements_data(&mut m).unwrap();
    m
}

/// Move the nodes of group `group` by `d` in the model's positions.
pub fn translate_group(
    model: &mut ContactMechanicsModel,
    mesh: &SharedMesh,
    group: &str,
    d: &[Real],
) {
    let nodes = nodes_of_physical(&mesh.read().unwrap(), group);
    let dim = d.len();
    let p = model.positions_mut();
    for n in nodes {
        let t = p.tuple_mut(n as usize);
        for i in 0..dim {
            t[i] += d[i];
        }
    }
}
