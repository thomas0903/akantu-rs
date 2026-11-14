// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the mesh readers, each built from a failing input; the related upstream
// code is
// src/io/mesh_io/mesh_io_msh.cc, mesh_io_diana.cc and
// src/mesh/group_manager.cc.

mod common;

use std::path::{Path, PathBuf};

use akantu_core::enums::MeshIoType;
use akantu_core::{Array, ElementType, ElementTypeMapArray, Error, GhostType, TypeFilter};
use akantu_io::{
    ReadOptions, ReadReport, WriteOptions, read_mesh_from_str, read_mesh_with, write_mesh_to_string,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshDataValues, NodalDataValues};
use akantu_validation::{Fixture, fixtures_root};

const H22: &str = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n";

fn read(dim: usize, name: &str, text: &str, format: MeshIoType) -> akantu_core::Result<Mesh> {
    read_with_report(dim, name, text, format).map(|(m, _)| m)
}

fn read_with_report(
    dim: usize,
    name: &str,
    text: &str,
    format: MeshIoType,
) -> akantu_core::Result<(Mesh, ReadReport)> {
    let mut m = Mesh::new(dim, "m")?;
    let opts = ReadOptions {
        write_diana_material_file: false,
        ..ReadOptions::default()
    };
    let r = read_mesh_from_str(&mut m, name, text, format, &opts)?;
    Ok((m, r))
}

/// The read fails, the mesh is left empty, and the error is a parse error
/// at `file:line:column`.
#[track_caller]
fn assert_parse_error(dim: usize, name: &str, text: &str, format: MeshIoType, at: (usize, usize)) {
    let mut m = Mesh::new(dim, "m").unwrap();
    let before = format!("{m:?}");
    let e =
        read_mesh_from_str(&mut m, name, text, format, &ReadOptions::default()).expect_err(text);
    match &e {
        Error::Parse {
            file, line, column, ..
        } => {
            assert_eq!(file, name, "{e}");
            assert_eq!((*line, *column), at, "{e}\n{text}");
        }
        other => panic!("expected a parse error, got {other}\n{text}"),
    }
    assert_eq!(format!("{m:?}"), before, "mesh modified by a failed read");
}

fn nodal(m: &Mesh, name: &str) -> (usize, Vec<f64>) {
    match m.data().nodal(name).unwrap() {
        NodalDataValues::Real(a) => {
            assert_eq!(a.len(), m.nb_nodes(), "{name}: one tuple per node");
            (a.nb_component(), a.as_slice().to_vec())
        }
        other => panic!("{name}: {other:?}"),
    }
}

fn scratch_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("akantu-io-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ------------------------------------------------------------------

/// Reproduction A: a zero-entry `$NodeData` block before `$Nodes`
/// and a second block of the same dataset after it (the installed C++
/// reader accepts the sequence) used to panic on the zero-length array.
#[test]
fn mio01_node_data_block_before_nodes_then_after() {
    let empty = "$NodeData\n1\n\"v\"\n0\n3\n0\n1\n0\n$EndNodeData\n";
    let nodes = "$Nodes\n1\n1 0 0 0\n$EndNodes\n";
    let full = "$NodeData\n1\n\"v\"\n0\n3\n0\n1\n1\n1 42\n$EndNodeData\n";
    let m = read(
        3,
        "ordered.msh",
        &format!("{H22}{empty}{nodes}{full}"),
        MeshIoType::Gmsh,
    )
    .unwrap();
    assert_eq!(nodal(&m, "v"), (1, vec![42.0]));
    // Same with an ordinary point-element section in between.
    let points = "$Elements\n1\n1 15 0 1\n$EndElements\n";
    let m = read(
        3,
        "ordered.msh",
        &format!("{H22}{empty}{nodes}{points}{full}"),
        MeshIoType::Gmsh,
    )
    .unwrap();
    assert_eq!(nodal(&m, "v"), (1, vec![42.0]));
    assert_eq!(m.nb_element(ElementType::Point1, GhostType::NotGhost), 1);
}

/// Component reuse over several blocks of one dataset (also a
/// 3-component one created before `$Nodes`), a dataset whose only block
/// precedes `$Nodes` (one zero tuple per node, UD-091), and the errors of
/// the sequence (different component count, entries before `$Nodes`):
/// located parse errors, never a panic.
#[test]
fn mio01_node_data_reuse_and_errors() {
    let block = |name: &str, nc: usize, entries: &[&str]| {
        format!(
            "$NodeData\n1\n\"{name}\"\n0\n3\n0\n{nc}\n{}\n{}$EndNodeData\n",
            entries.len(),
            entries.iter().map(|e| format!("{e}\n")).collect::<String>()
        )
    };
    let nodes = "$Nodes\n3\n1 0 0 0\n2 1 0 0\n7 2 0 0\n$EndNodes\n";
    let text = format!(
        "{H22}{}{}{}{nodes}{}{}{}",
        block("v", 1, &[]),
        block("w", 3, &[]),
        block("only_before", 2, &[]),
        block("v", 1, &["1 42", "2 43"]),
        block("w", 3, &["7 1 2 3"]),
        block("v", 1, &["2 -7.5"]),
    );
    let m = read(3, "reuse.msh", &text, MeshIoType::Gmsh).unwrap();
    assert_eq!(nodal(&m, "v"), (1, vec![42.0, -7.5, 0.0]));
    assert_eq!(
        nodal(&m, "w"),
        (3, vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0])
    );
    assert_eq!(nodal(&m, "only_before"), (2, vec![0.0; 6]));

    // Lines: 1-3 header, 4-9 nodes, 10-19 first block; the second block's
    // second integer tag (number of components) is on line 26.
    let mismatch = format!(
        "{H22}{nodes}{}{}",
        block("v", 1, &["1 1"]),
        block("v", 2, &["1 1 2"])
    );
    assert_parse_error(3, "bad.msh", &mismatch, MeshIoType::Gmsh, (26, 1));
    // Entries before `$Nodes`: the node is undefined (line 12, column 1).
    let early = format!("{H22}{}{nodes}", block("v", 1, &["1 42"]));
    assert_parse_error(3, "bad.msh", &early, MeshIoType::Gmsh, (12, 1));
}

/// `mio01_*` against the oracle: `fixtures/io/mesh_io/data/node_data_section_order`
/// (C++ `Mesh::read` through `oracle/probes/mesh_io`): empty blocks of `v`
/// (1 component) and `w` (3 components) before `$Nodes`, full blocks after
/// it (points and segments in between) and `v` overwritten by a later
/// block listing the nodes in another order. Every array equals the
/// oracle's, and the values are those derived from the file.
#[test]
fn mio01_section_order_matches_the_oracle() {
    let test = "mio01_section_order_matches_the_oracle";
    let dir = fixtures_root().join("io/mesh_io/data/node_data_section_order");
    let fx = Fixture::open(&dir).unwrap();
    let m = common::read_fixture_mesh(&fx, &ReadOptions::default(), MeshIoType::Auto);
    common::compare_with_fixture(test, &dir, &m).unwrap();
    assert_eq!(nodal(&m, "v"), (1, vec![7.5, 0.125, -8.25]));
    assert_eq!(
        nodal(&m, "w"),
        (3, vec![4.0, 5.0, 6.0, 1.0, 2.0, 3.0, 7.0, 8.0, 9.0])
    );
}

// ------------------------------------------------------------------

/// Node-group names, in iteration order.
fn node_group_names(m: &Mesh) -> Vec<String> {
    m.groups()
        .node_groups()
        .map(|g| g.name().to_owned())
        .collect()
}

/// The node list of a physical group is a real node group named
/// `<name>_nodes` (upstream `createElementGroup` →
/// `createNodeGroup(name + "_nodes")`), found and listed by the node-group
/// API and identical to the element group's nodes.
#[test]
fn mio03_physical_group_nodes_are_node_groups() {
    let text = format!(
        "{H22}$PhysicalNames\n1\n0 1 \"point\"\n$EndPhysicalNames\n$Nodes\n1\n1 0 0 0\n$EndNodes\n\
         $Elements\n1\n1 15 2 1 1 1\n$EndElements\n"
    );
    let m = read(1, "p.msh", &text, MeshIoType::Gmsh).unwrap();
    let g = m.groups();
    assert_eq!(g.element_group("point").unwrap().nodes().as_slice(), &[0]);
    assert_eq!(
        g.element_group("point").unwrap().node_group.name(),
        "point_nodes"
    );
    assert_eq!(g.node_group("point_nodes").unwrap().node_list(), vec![0]);
    assert_eq!(node_group_names(&m), vec!["point_nodes"]);
    assert!(g.node_group("point").is_err());
}

/// `mio03_*` on committed oracle cases (MSH 2/4 physical groups, unnamed
/// MSH 4 groups, Diana node and element groups): every element group's
/// `<name>_nodes` node group is listed and equals its nodes; Diana's
/// standalone node groups sit next to them.
#[test]
fn mio03_node_groups_of_fixture_meshes() {
    for (case, format, extra) in [
        ("io/mesh_io/msh4/unnamed_physical", MeshIoType::Gmsh, vec![]),
        ("io/mesh_io/data/data_blocks", MeshIoType::Gmsh, vec![]),
        (
            "mesh_io/test_io/test_dumper/test_dumper",
            MeshIoType::Gmsh,
            vec![],
        ),
        ("io/mesh_io/diana/diana_3d", MeshIoType::Diana, vec!["FIX"]),
    ] {
        let fx = Fixture::open(&fixtures_root().join(case)).unwrap();
        let opts = ReadOptions {
            write_diana_material_file: false,
            ..ReadOptions::default()
        };
        let m = common::read_fixture_mesh(&fx, &opts, format);
        let mut want: Vec<String> = m
            .groups()
            .element_groups()
            .map(|g| format!("{}_nodes", g.name()))
            .chain(extra.iter().map(|s| (*s).to_owned()))
            .collect();
        want.sort();
        assert!(!want.is_empty(), "{case}");
        assert_eq!(node_group_names(&m), want, "{case}");
        for eg in m.groups().element_groups() {
            let ng = m
                .groups()
                .node_group(&format!("{}_nodes", eg.name()))
                .unwrap();
            assert_eq!(
                ng.node_list(),
                eg.nodes().as_slice(),
                "{case}: {}",
                eg.name()
            );
        }
    }
}

/// `mio03_*`/07: a Diana node group named like the node group an element
/// group owns (`X_nodes`) is rejected with a location, in both orders
/// (upstream throws "already exists").
#[test]
fn mio03_diana_node_group_name_clash() {
    let base = "'COORDINATES'\n 1 0 0 0\n 2 1 0 0\n 3 0 1 0\n'ELEMENTS'\nCONNECTIVITY\n 1 T9TM 1 2 3\n'GROUPS'\n";
    let elem_first =
        format!("{base}ELEMEN\n   1 SURF / 1 /\nNODES\n   2 SURF_nodes / 1 2 /\n'SUPPORTS'\n");
    assert_parse_error(3, "c.diana", &elem_first, MeshIoType::Diana, (12, 6));
    let nodes_first =
        format!("{base}NODES\n   2 SURF_nodes / 1 2 /\nELEMEN\n   1 SURF / 1 /\n'SUPPORTS'\n");
    assert_parse_error(3, "c.diana", &nodes_first, MeshIoType::Diana, (12, 6));
    // Distinct names are fine and both kinds are listed.
    let ok = format!("{base}ELEMEN\n   1 SURF / 1 /\nNODES\n   2 SURF_n / 1 2 /\n'SUPPORTS'\n");
    let m = read(3, "c.diana", &ok, MeshIoType::Diana).unwrap();
    assert_eq!(node_group_names(&m), vec!["SURF_n", "SURF_nodes"]);
}

// ------------------------------------------------------------------

/// Reproduction B: when the Diana material side file cannot be
/// written (its path is a directory) the read fails with an I/O error and
/// the mesh is unchanged: nodes, connectivities, data, groups, bounding
/// box and every other field (compared through `Debug`).
#[test]
fn mio04_failed_side_file_leaves_the_mesh_untouched() {
    let fx = Fixture::open(&fixtures_root().join("io/mesh_io/diana/diana_3d")).unwrap();
    let dir = scratch_dir("mio04");
    let input = dir.join("mesh.diana");
    std::fs::copy(fx.input_path("mesh").unwrap(), &input).unwrap();
    let side = dir.join("material_mesh.diana");
    std::fs::create_dir(&side).unwrap();

    let mut m = Mesh::new(3, "m").unwrap();
    let before = format!("{m:?}");
    let e = read_mesh_with(&mut m, &input, MeshIoType::Diana, &ReadOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Io { .. }), "{e}");
    assert_eq!(format!("{m:?}"), before);
    assert_eq!(m.nb_nodes(), 0);
    assert_eq!(m.element_types(TypeFilter::new()).count(), 0);
    assert!(m.data().is_empty());
    assert_eq!(m.groups().element_groups().count(), 0);
    assert_eq!(m.groups().node_groups().count(), 0);

    // The same mesh object reads the file once the obstacle is gone.
    std::fs::remove_dir(&side).unwrap();
    read_mesh_with(&mut m, &input, MeshIoType::Diana, &ReadOptions::default()).unwrap();
    assert_eq!(m.nb_nodes(), 30);
    let oracle = std::fs::read_to_string(fx.input_path("oracle_material_file").unwrap()).unwrap();
    assert_eq!(std::fs::read_to_string(&side).unwrap(), oracle);
    std::fs::remove_dir_all(&dir).unwrap();
}

// ------------------------------------------------------------------

/// Reproduction C on the reading side: a quoted string tag is
/// decoded once; escaped boundary quotes stay part of the name.
#[test]
fn mio05_quoted_dataset_names_are_decoded_once() {
    let nodes = "$Nodes\n1\n1 0 0 0\n$EndNodes\n";
    for (tag, name) in [
        ("\"\\\"edge\\\"\"", "\"edge\""),
        ("\"edge\"", "edge"),
        ("\"a \\\\ b\"", "a \\ b"),
        ("\" spaced \"", " spaced "),
        ("bare\"", "bare\""),
    ] {
        let text = format!("{H22}{nodes}$NodeData\n1\n{tag}\n0\n3\n0\n1\n1\n1 42\n$EndNodeData\n");
        let m = read(3, "q.msh", &text, MeshIoType::Gmsh).unwrap();
        assert_eq!(
            m.data().nodal_names().collect::<Vec<_>>(),
            vec![name],
            "{tag}"
        );
    }
}

/// Elemental `Real` dataset with `values` for the single `Segment2`.
fn elemental(nc: usize, values: Vec<f64>) -> MeshDataValues {
    let mut m = ElementTypeMapArray::new("d");
    m.as_map_mut()
        .insert(
            ElementType::Segment2,
            GhostType::NotGhost,
            Array::from_vec(values, nc).unwrap(),
        )
        .unwrap();
    MeshDataValues::Real(m.into())
}

/// Reproduction C: dataset names with leading/trailing quotes,
/// backslashes and blanks survive write → read exactly (names, shapes and
/// values), including names that collide once quotes are trimmed, in node,
/// element and element-node blocks.
#[test]
fn mio05_dataset_names_round_trip() {
    let mut m = Mesh::new(3, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        a.push_node(&[0.0, 0.0, 0.0]).unwrap();
        a.push_node(&[1.0, 0.0, 0.0]).unwrap();
        a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
            .push_tuple(&[0, 1]);
    }
    let nodal_names = [
        "\"edge\"",
        "edge",
        "\"lead",
        "trail\"",
        "a\\b\\",
        "\\\"",
        " spaced  name ",
        "x\"y",
    ];
    for (k, name) in nodal_names.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // small k
        let v = vec![k as f64, -(k as f64) - 0.5];
        m.data_mut()
            .insert_nodal(*name, NodalDataValues::Real(Array::from_vec(v, 1).unwrap()));
    }
    // Element and element-node datasets that would merge (and clash) if
    // the reader trimmed quotes.
    let d = m.data_mut();
    d.insert_elemental("\"f\"", elemental(2, vec![1.0, 2.0]));
    d.insert_elemental("f", elemental(1, vec![3.0]));
    d.insert_elemental("\"g", elemental(1, vec![4.0]));
    d.insert_elemental("g\"", elemental(4, vec![5.0, 6.0, 7.0, 8.0]));
    d.set_elemental_nodal("g\"", true);
    d.insert_elemental("\"f", elemental(4, vec![9.0, 10.0, 11.0, 12.0]));
    d.set_elemental_nodal("\"f", true);

    let text = write_mesh_to_string(
        &m,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::default(),
    )
    .unwrap();
    let back = read(3, "x.msh", &text, MeshIoType::Gmsh).unwrap();
    let names = |m: &Mesh| -> Vec<String> { m.data().nodal_names().map(str::to_owned).collect() };
    assert_eq!(names(&back), names(&m), "{text}");
    for name in nodal_names {
        assert_eq!(nodal(&back, name), nodal(&m, name), "{name}");
    }
    let real = |m: &Mesh| -> Vec<(String, bool, usize, Vec<f64>)> {
        m.data()
            .elemental_names()
            .filter_map(|n| match m.data().elemental(n).unwrap() {
                MeshDataValues::Real(r) => {
                    let a = r.get(ElementType::Segment2, GhostType::NotGhost).unwrap();
                    assert_eq!(a.len(), 1, "{n}");
                    Some((
                        n.to_owned(),
                        m.data().is_elemental_nodal(n),
                        a.nb_component(),
                        a.as_slice().to_vec(),
                    ))
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(real(&back), real(&m), "{text}");
    assert_eq!(real(&m).len(), 5);

    // A line break cannot be represented: the writer refuses the name.
    m.data_mut().insert_nodal(
        "two\nlines",
        NodalDataValues::Real(Array::from_vec(vec![0.0, 0.0], 1).unwrap()),
    );
    let e = write_mesh_to_string(
        &m,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(e, Error::InvalidArgument(_)), "{e}");
}

// ------------------------------------------------------------------

/// Reproduction D: an MSH 4 block of 4-node cubic lines (code 26,
/// UD-090) is skipped and the following supported block is read; element
/// tags after the skipped block stay correct (`$ElementData`).
#[test]
fn mio06_cubic_line_block_is_skipped() {
    let text = "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$Nodes\n1 4 1 4\n1 1 0 4\n1\n2\n3\n4\n\
                0 0 0\n1 0 0\n0.3333333333333333 0 0\n0.6666666666666666 0 0\n$EndNodes\n\
                $Elements\n2 2 1 2\n1 1 26 1\n1 1 2 3 4\n1 1 1 1\n2 1 2\n$EndElements\n\
                $ElementData\n1\n\"d\"\n0\n3\n0\n1\n2\n1 5\n2 7\n$EndElementData\n";
    let (m, r) = read_with_report(1, "cubic.msh", text, MeshIoType::Gmsh).unwrap();
    assert_eq!(
        m.element_types(TypeFilter::new()).collect::<Vec<_>>(),
        vec![ElementType::Segment2]
    );
    assert_eq!(
        m.connectivity(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        &[0, 1]
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("Unsuported element kind 26")),
        "{:?}",
        r.warnings
    );
    let MeshDataValues::Real(d) = m.data().elemental("d").unwrap() else {
        panic!()
    };
    assert_eq!(
        d.get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        &[7.0]
    );
    // The node count of skipped elements is still checked: three node tags
    // for a 4-node line end at column 8 of line 19.
    let short = text.replace("1 1 2 3 4\n", "1 1 2 3\n");
    assert_parse_error(1, "cubic.msh", &short, MeshIoType::Gmsh, (19, 8));
}

/// An unsupported block of every type of the Gmsh manual's list
/// that Akantu does not read, with its documented node count (written
/// here independently of the reader's table), is skipped and the
/// following segment block is read.
#[test]
fn mio06_every_documented_unsupported_type_is_skipped() {
    // (code, nodes): Gmsh reference manual, MSH format, `elm-type`.
    let documented = [
        (7, 5),
        (10, 9),
        (12, 27),
        (13, 18),
        (14, 14),
        (19, 13),
        (20, 9),
        (21, 10),
        (22, 12),
        (23, 15),
        (24, 15),
        (25, 21),
        (26, 4),
        (27, 5),
        (28, 6),
        (29, 20),
        (30, 35),
        (31, 56),
        (92, 64),
        (93, 125),
    ];
    let n_nodes = 125;
    let mut nodes = format!("$Nodes\n1 {n_nodes} 1 {n_nodes}\n3 1 0 {n_nodes}\n");
    for i in 1..=n_nodes {
        nodes.push_str(&format!("{i}\n"));
    }
    for i in 1..=n_nodes {
        nodes.push_str(&format!("{i} 0 0\n"));
    }
    nodes.push_str("$EndNodes\n");
    for (code, k) in documented {
        let tags: Vec<String> = (1..=k).map(|i| i.to_string()).collect();
        let text = format!(
            "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n{nodes}$Elements\n2 2 1 2\n3 1 {code} 1\n1 {}\n\
             1 1 1 1\n2 1 2\n$EndElements\n",
            tags.join(" ")
        );
        let (m, r) = read_with_report(1, "skip.msh", &text, MeshIoType::Gmsh)
            .unwrap_or_else(|e| panic!("code {code}: {e}"));
        assert_eq!(
            m.nb_element(ElementType::Segment2, GhostType::NotGhost),
            1,
            "code {code}"
        );
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains(&format!("Unsuported element kind {code} "))),
            "code {code}: {:?}",
            r.warnings
        );
    }
}

// ------------------------------------------------------------------

const DIANA_BASE: &str = "'COORDINATES'\n 1 0 0 0\n 2 1 0 0\n 3 0 1 0\n'ELEMENTS'\nCONNECTIVITY\n 1 T9TM 1 2 3\n'GROUPS'\n";

/// Duplicate Diana node and element groups, an undefined node on
/// a continuation line of a group, and a malformed interval are parse
/// errors at `file:line:column`.
#[test]
fn mio07_diana_group_errors_are_located() {
    let d = DIANA_BASE;
    let dup_nodes = format!("{d}NODES\n   2 FIX / 1 2 /\n   3 FIX / 3 /\n'SUPPORTS'\n");
    assert_parse_error(3, "g.diana", &dup_nodes, MeshIoType::Diana, (11, 6));
    let dup_elems = format!("{d}ELEMEN\n   1 SURF / 1 /\n 2  SURF / 1 /\n'SUPPORTS'\n");
    assert_parse_error(3, "g.diana", &dup_elems, MeshIoType::Diana, (11, 5));
    let undefined = format!("{d}NODES\n   2 FIX / 1\n  2-9 /\n'SUPPORTS'\n");
    assert_parse_error(3, "g.diana", &undefined, MeshIoType::Diana, (11, 3));
    let malformed = format!("{d}NODES\n   2 FIX / 1 2- /\n'SUPPORTS'\n");
    assert_parse_error(3, "g.diana", &malformed, MeshIoType::Diana, (10, 14));
}

/// Text of a 2.2 file whose `$PhysicalNames` cannot be applied: the
/// segment has no tag (upstream throws looking for `tag_0`).
fn tagless_physical_names() -> String {
    format!(
        "{H22}$PhysicalNames\n1\n1 1 \"edge\"\n$EndPhysicalNames\n$Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n\
         $Elements\n1\n1 1 0 1 2\n$EndElements\n"
    )
}

/// Physical names with tagless elements point at the tag count of
/// the first element of the type, through the string and the file entry
/// points (the file name is the path).
#[test]
fn mio07_tagless_physical_names_are_located() {
    let text = tagless_physical_names();
    assert_parse_error(2, "bad.msh", &text, MeshIoType::Gmsh, (15, 5));
    let dir = scratch_dir("mio07");
    let path = dir.join("bad.msh");
    std::fs::write(&path, &text).unwrap();
    let mut m = Mesh::new(2, "m").unwrap();
    let e = read_mesh_with(&mut m, &path, MeshIoType::Auto, &ReadOptions::default()).unwrap_err();
    match &e {
        Error::Parse {
            file, line, column, ..
        } => {
            assert_eq!(file, &path.display().to_string());
            assert_eq!((*line, *column), (15, 5));
        }
        other => panic!("{other}"),
    }
    assert_eq!(m.nb_nodes(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Unsupported inputs keep their `Unsupported` classification and
/// carry `file:line:column` (binary file type, format version, parametric
/// nodes); invalid data-block headers point at the offending tag.
#[test]
fn mio07_unsupported_and_header_errors_are_located() {
    for (text, at) in [
        ("$MeshFormat\n2.2 1 8\n$EndMeshFormat\n".to_owned(), "x.msh:2:5: binary"),
        ("$MeshFormat\n3.0 0 8\n$EndMeshFormat\n".to_owned(), "x.msh:2:1: Gmsh MSH format version 3.0"),
        (
            "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$Nodes\n1 1 1 1\n2 1 1 1\n1\n0 0 0 0.5\n$EndNodes\n"
                .to_owned(),
            "x.msh:6:5: Akantu does not support parametric nodes",
        ),
    ] {
        let mut m = Mesh::new(2, "m").unwrap();
        let e = read_mesh_from_str(&mut m, "x.msh", &text, MeshIoType::Gmsh, &ReadOptions::default())
            .unwrap_err();
        match &e {
            Error::Unsupported(msg) => assert!(msg.starts_with(at), "{msg}"),
            other => panic!("{other}"),
        }
        assert_eq!(m.nb_nodes(), 0);
    }
    let nodes = "$Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n";
    for (nc, count, at) in [
        ("0", "1", (15, 1)),
        ("x2", "1", (15, 1)),
        ("1", "-1", (16, 1)),
    ] {
        let text =
            format!("{H22}{nodes}$NodeData\n1\n\"t\"\n0\n3\n0\n{nc}\n{count}\n$EndNodeData\n");
        assert_parse_error(2, "h.msh", &text, MeshIoType::Gmsh, at);
    }
}
