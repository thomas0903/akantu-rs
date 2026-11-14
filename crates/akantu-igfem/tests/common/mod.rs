// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_fe_engine/
// igfem_mesh_generation.cc (generateIGFEMMesh) and the `.verified` output
// convention of upstream's test harness.

//! Shared helpers of the IGFEM tests.

#![allow(dead_code)] // each test binary uses a subset

pub mod cut;
pub mod model;

use std::path::{Path, PathBuf};

use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};

/// Directory of the upstream test inputs copied verbatim.
pub fn upstream(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream")
        .join(file)
}

/// Port of upstream `generateIGFEMMesh(type, mesh, filename)`: read the
/// `$Nodes` (index x y z) and `$Elements` (one connectivity per line, node
/// numbers shifted by the smallest node number) blocks of a mesh-info file
/// into a mesh of dimension `dim` with elements of type `ty`.
pub fn generate_igfem_mesh(ty: ElementType, dim: usize, path: &Path) -> Mesh {
    let text = std::fs::read_to_string(path).unwrap();
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    let mut lines = text.lines();
    let mut nodes: Vec<(usize, [Real; 3])> = Vec::new();
    let mut elements: Vec<Vec<usize>> = Vec::new();
    while let Some(line) = lines.next() {
        if line == "$Nodes" || line == "$NOD" {
            let n: usize = lines.next().unwrap().trim().parse().unwrap();
            for _ in 0..n {
                let mut it = lines.next().unwrap().split_whitespace();
                let idx: usize = it.next().unwrap().parse().unwrap();
                let c: Vec<Real> = it.map(|v| v.parse().unwrap()).collect();
                nodes.push((idx, [c[0], c[1], c[2]]));
            }
        }
        if line == "$Elements" || line == "$ELM" {
            let n: usize = lines.next().unwrap().trim().parse().unwrap();
            for _ in 0..n {
                let conn: Vec<usize> = lines
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect();
                elements.push(conn);
            }
        }
    }
    let first = nodes.iter().map(|(i, _)| *i).min().unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for (_, c) in &nodes {
            acc.push_node(&c[..dim]).unwrap();
        }
        let mut conn = acc.connectivity_mut(ty, GhostType::NotGhost);
        for e in &elements {
            assert_eq!(e.len(), ty.nb_nodes_per_element());
            let row: Vec<Idx> = e
                .iter()
                .map(|&n| Idx::try_from(n - first).unwrap())
                .collect();
            conn.push_tuple(&row);
        }
    }
    mesh
}

/// One array printed by upstream's `Array::printself` in a `.verified`
/// file: its id and its values as printed (C++ `operator<<(double)`,
/// precision 6), one inner vector per tuple.
#[derive(Debug, Clone)]
pub struct VerifiedArray {
    pub id: String,
    pub values: Vec<Vec<String>>,
}

/// Every `+ id … + values : {{…}}` array of a `.verified` file, in order.
pub fn parse_verified(path: &Path) -> Vec<VerifiedArray> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = Vec::new();
    let mut id: Option<String> = None;
    for line in text.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("+ id") {
            id = Some(rest.trim_start().trim_start_matches(':').trim().to_owned());
        } else if let Some(rest) = l.strip_prefix("+ values") {
            let v = rest.trim_start().trim_start_matches(':').trim();
            let v = v.trim_start_matches('{').trim_end_matches('}');
            let values = v
                .split("}, {")
                .map(|t| t.split(", ").map(|x| x.trim().to_owned()).collect())
                .collect();
            out.push(VerifiedArray {
                id: id.take().unwrap_or_default(),
                values,
            });
        }
    }
    out
}

/// `x` rounded to 6 significant digits, in a canonical form (`d.ddddde±x`),
/// with the sign of zero dropped: the representation upstream's output
/// stream prints with its default precision 6 (`%g`), up to notation.
pub fn six_digits(x: Real) -> String {
    if x == 0.0 {
        return "0".to_owned();
    }
    format!("{x:.5e}")
}

/// Compare computed tuples with a printed upstream array: every value must
/// print (6 significant digits) as upstream printed it. Returns the number
/// of compared values.
pub fn assert_matches_verified(
    what: &str,
    computed: &[Real],
    nb_component: usize,
    printed: &VerifiedArray,
) -> usize {
    assert_eq!(
        computed.len(),
        printed.values.len() * nb_component,
        "{what}: size differs from upstream's {}",
        printed.id
    );
    let mut n = 0;
    for (t, tuple) in printed.values.iter().enumerate() {
        assert_eq!(
            tuple.len(),
            nb_component,
            "{what}: components of {}",
            printed.id
        );
        for (c, p) in tuple.iter().enumerate() {
            let pv: Real = p.parse().unwrap();
            let mine = computed[t * nb_component + c];
            assert_eq!(
                six_digits(mine),
                six_digits(pv),
                "{what}: tuple {t} component {c}: computed {mine:e}, upstream printed {p} ({})",
                printed.id
            );
            n += 1;
        }
    }
    n
}

/// Read an upstream Gmsh test mesh copied to `tests/data/upstream`.
pub fn read_msh(dim: usize, name: &str, id: &str) -> Mesh {
    read_msh_at(dim, &upstream(name), id)
}

/// The frozen Gmsh 4.13.1 mesh of upstream `patch_tests/plate.geo` (the
/// repository's `fixtures/meshes`, SHA-256 `7796a339…`): generated at
/// build time upstream, so it is not a verbatim copy of an upstream file and
/// is not kept under `tests/data/upstream`.
pub fn frozen_patch_plate() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/meshes/gmsh-4.13.1/extra_packages/igfem/test/patch_tests/plate.msh")
}

/// Read the Gmsh mesh at `path`.
pub fn read_msh_at(dim: usize, path: &Path, id: &str) -> Mesh {
    let name = path.to_string_lossy();
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut m = Mesh::new(dim, id).unwrap();
    akantu_io::read_mesh_from_str(
        &mut m,
        &name,
        &text,
        akantu_core::enums::MeshIoType::Gmsh,
        &akantu_io::ReadOptions::default(),
    )
    .unwrap();
    m
}
