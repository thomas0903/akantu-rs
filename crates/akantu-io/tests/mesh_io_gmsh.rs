// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the Gmsh
// reader/writer that do not need an oracle run: ports of
// test/test_mesh_utils/test_mesh_io/test_mesh_io_msh{,_physical_names}.cc,
// round trips of every reference mesh, error handling and fuzzing.

mod common;

use std::fmt::Write as _;
use std::path::Path;

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, Error, GhostType, TypeFilter};
use akantu_io::{
    ReadOptions, WriteOptions, read_mesh, read_mesh_from_str, read_mesh_with, write_mesh,
    write_mesh_to_string,
};
use akantu_mesh::{Mesh, MeshDataValues};
use akantu_validation::manifest::StorageMode;
use akantu_validation::{Fixture, fixtures_root};
use proptest::prelude::*;

fn frozen(rel: &str) -> std::path::PathBuf {
    fixtures_root().join("meshes/gmsh-4.13.1").join(rel)
}

fn read_str(dim: usize, text: &str) -> akantu_core::Result<Mesh> {
    let mut m = Mesh::new(dim, "m")?;
    read_mesh_from_str(
        &mut m,
        "t.msh",
        text,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )?;
    Ok(m)
}

fn int_data(mesh: &Mesh, name: &str, ty: ElementType) -> Option<Vec<i32>> {
    match mesh.data().elemental(name).ok()? {
        MeshDataValues::Int(m) => Some(m.get(ty, GhostType::NotGhost).ok()?.as_slice().to_vec()),
        _ => None,
    }
}

fn strings(mesh: &Mesh, name: &str, ty: ElementType) -> Vec<String> {
    match mesh.data().elemental(name).unwrap() {
        MeshDataValues::String(m) => m.get(ty, GhostType::NotGhost).unwrap().as_slice().to_vec(),
        other => panic!("{other:?}"),
    }
}

/// Port of `test_mesh_io_msh_physical_names.cc`: read
/// `cube_physical_names.msh` and print the physical name of every element;
/// the output must equal the upstream `.verified` file (compared like the
/// upstream driver, `diff -biw`: blanks ignored).
#[test]
fn test_mesh_io_msh_physical_names() {
    let mut mesh = Mesh::new(3, "mesh").unwrap();
    read_mesh(
        &mut mesh,
        &frozen("test/test_mesh_utils/test_mesh_io/cube_physical_names.msh"),
        MeshIoType::Auto,
    )
    .unwrap();
    let mut out = String::new();
    for ty in mesh.element_types(TypeFilter::new()) {
        for (i, name) in strings(&mesh, "physical_names", ty).iter().enumerate() {
            let _ = writeln!(out, "Element {i} (of type {ty}) has physical name {name}.");
        }
    }
    let verified = include_str!("data/upstream/test_mesh_io_msh_physical_names.verified");
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect()
    };
    assert_eq!(norm(&out), norm(verified));
}

/// Port of `test_mesh_io_msh.cc`: read `cube.msh` (MSH 4.1) in 3-D and
/// write it back (`cube.out`). The written text equals the oracle's
/// (`fixtures/io/mesh_io/write/cube_msh41`, compared in
/// `mesh_io_probe_fixtures.rs`); here: the output reads back to the same
/// mesh, coordinates bit for bit.
#[test]
fn test_mesh_io_msh() {
    let mut mesh = Mesh::new(3, "mesh").unwrap();
    read_mesh(
        &mut mesh,
        &frozen("test/test_mesh_utils/test_mesh_io/cube.msh"),
        MeshIoType::Gmsh,
    )
    .unwrap();
    assert_eq!(mesh.nb_nodes(), 1149);
    assert_eq!(
        mesh.nb_element(ElementType::Tetrahedron4, GhostType::NotGhost),
        4617
    );
    let dir =
        std::env::temp_dir().join(format!("akantu-io-test-mesh-io-msh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("cube.out");
    write_mesh(&mesh, &out, MeshIoType::Gmsh).unwrap();
    let mut back = Mesh::new(3, "back").unwrap();
    read_mesh(&mut back, &out, MeshIoType::Gmsh).unwrap();
    assert_same_mesh("cube.msh", &mesh, &back, true);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Same topology, coordinates (bitwise) and `tag_0`/`tag_1` (when the
/// original has them).
fn assert_same_mesh(case: &str, a: &Mesh, b: &Mesh, tags: bool) {
    assert_eq!(a.nb_nodes(), b.nb_nodes(), "{case}");
    assert!(
        a.nodes()
            .as_slice()
            .iter()
            .zip(b.nodes().as_slice())
            .all(|(x, y)| x.to_bits() == y.to_bits()),
        "{case}: coordinates differ after the round trip"
    );
    let ta: Vec<ElementType> = a.element_types(TypeFilter::new()).collect();
    let tb: Vec<ElementType> = b.element_types(TypeFilter::new()).collect();
    assert_eq!(ta, tb, "{case}");
    for ty in ta {
        assert_eq!(
            a.connectivity(ty, GhostType::NotGhost).unwrap().as_slice(),
            b.connectivity(ty, GhostType::NotGhost).unwrap().as_slice(),
            "{case}: {ty} connectivity"
        );
        if tags {
            for t in ["tag_0", "tag_1"] {
                if let Some(v) = int_data(a, t, ty) {
                    assert_eq!(Some(v), int_data(b, t, ty), "{case}: {t} of {ty}");
                }
            }
        }
    }
}

/// Committed `fixtures/mesh_io` cases (all of them materialised in a fresh
/// clone); the external cases come on top (never counted here).
const COMMITTED_MESH_IO_CASES: usize = 218;

/// Every reference mesh (all `fixtures/mesh_io` inputs, MSH 2.1/2.2/4.0/4.1)
/// survives read → write (17 digits) → read with identical coordinates (bit
/// for bit), connectivities and `tag_0`/`tag_1` (UD-061, UD-093). Only the
/// input mesh is needed: external cases without local data use the frozen
/// Gmsh mesh they were generated from when it is committed
/// ([`common::mesh_input`]); the others are listed and skipped.
#[test]
fn every_reference_mesh_round_trips_bitwise() {
    let dirs = common::mesh_io_cases();
    let mut round_tripped = 0;
    let mut committed = 0;
    let mut skipped = Vec::new();
    for dir in &dirs {
        let fx = Fixture::open(dir).unwrap();
        let external = fx.manifest().storage.mode == StorageMode::External;
        if common::mesh_input(&fx).is_none() {
            assert!(
                external,
                "{}: committed fixture without its input mesh",
                fx.case_id()
            );
            skipped.push(fx.case_id().to_owned());
            continue;
        }
        committed += usize::from(!external);
        let a = common::read_fixture_mesh(&fx, &ReadOptions::default(), MeshIoType::Auto);
        let text = write_mesh_to_string(
            &a,
            Path::new("x.msh"),
            MeshIoType::Gmsh,
            &WriteOptions::default(),
        )
        .unwrap();
        let b = read_str(a.spatial_dimension(), &text).unwrap();
        assert_same_mesh(fx.case_id(), &a, &b, true);
        round_tripped += 1;
    }
    if !skipped.is_empty() {
        eprintln!(
            "every_reference_mesh_round_trips_bitwise: {} external case(s) without a local \
             input mesh skipped: {skipped:?}",
            skipped.len()
        );
    }
    eprintln!(
        "every_reference_mesh_round_trips_bitwise: {round_tripped} of {} meshes round-tripped",
        dirs.len()
    );
    assert_eq!(round_tripped + skipped.len(), dirs.len());
    assert!(
        committed >= COMMITTED_MESH_IO_CASES,
        "{committed} committed cases round-tripped, expected ≥ {COMMITTED_MESH_IO_CASES}"
    );
}

/// UD-061: with upstream's 15 digits the round trip is lossy for general
/// doubles, with 17 it is exact.
#[test]
fn seventeen_digits_are_needed() {
    let text = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n2\n1 0.1 0.30000000000000004 1e-300\n\
                2 0.3333333333333333 -2.5e+16 6.02214076e23\n$EndNodes\n$Elements\n1\n1 1 2 0 0 1 2\n$EndElements\n";
    let a = read_str(3, text).unwrap();
    let lossless = write_mesh_to_string(
        &a,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::default(),
    )
    .unwrap();
    assert_same_mesh("17 digits", &a, &read_str(3, &lossless).unwrap(), true);
    let lossy = write_mesh_to_string(
        &a,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::upstream_precision(),
    )
    .unwrap();
    assert!(
        lossy.contains("2 0.333333333333333 -2.5E+16 6.02214076E+23"),
        "{lossy}"
    );
    let b = read_str(3, &lossy).unwrap();
    assert_ne!(a.nodes().as_slice(), b.nodes().as_slice());
}

/// UD-064: elements of one type with differing tag counts get aligned
/// `tag_j` arrays, missing tags = 0.
#[test]
fn differing_tag_counts_are_aligned() {
    let text = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n3\n1 0 0 0\n2 1 0 0\n3 0 1 0\n$EndNodes\n\
                $Elements\n4\n1 1 4 7 8 9 10 1 2\n2 1 2 5 6 2 3\n3 1 3 1 2 3 3 1\n4 1 0 1 3\n$EndElements\n";
    let m = read_str(2, text).unwrap();
    let s = ElementType::Segment2;
    assert_eq!(int_data(&m, "tag_0", s).unwrap(), vec![7, 5, 1, 0]);
    assert_eq!(int_data(&m, "tag_1", s).unwrap(), vec![8, 6, 2, 0]);
    assert_eq!(int_data(&m, "tag_2", s).unwrap(), vec![9, 0, 3, 0]);
    assert_eq!(int_data(&m, "tag_3", s).unwrap(), vec![10, 0, 0, 0]);
    assert!(int_data(&m, "tag_4", s).is_none());
}

/// `$PhysicalNames` with blanks and escaped quotes (`std::quoted`), names
/// keyed by (dimension, tag), unnamed tags printed as numbers, groups from
/// the names (as `fixtures/io/mesh_io/data/data_blocks` records them).
#[test]
fn physical_names_and_groups() {
    let text = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n3\n1 4 \"a b\"\n2 4 \"\\\"q\\\"\"\n1 9 x\n$EndPhysicalNames\n\
                $Nodes\n3\n1 0 0 0\n2 1 0 0\n3 0 1 0\n$EndNodes\n$Elements\n3\n1 1 2 4 0 1 2\n2 1 2 12 0 2 3\n3 2 2 4 0 1 2 3\n$EndElements\n";
    let m = read_str(2, text).unwrap();
    assert_eq!(
        strings(&m, "physical_names", ElementType::Segment2),
        vec!["a b", "12"]
    );
    assert_eq!(
        strings(&m, "physical_names", ElementType::Triangle3),
        vec!["\"q\""]
    );
    let names: Vec<&str> = m.groups().element_groups().map(|g| g.name()).collect();
    assert_eq!(names, vec!["\"q\"", "12", "a b"]);
    let g = m.groups().element_group("a b").unwrap();
    assert_eq!((g.dimension, g.nodes().as_slice()), (1, &[0, 1][..]));
    assert_eq!(g.node_group.name(), "a b_nodes");
}

/// Structural reader: 2-node lines → Bernoulli beams (`_bernoulli_beam_2`
/// unless the mesh is 3-D), 3-node triangles → DKT; other types skipped.
#[test]
fn structural_reader_types() {
    let text = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n3\n1 0 0 0\n2 1 0 0\n3 0 1 0\n$EndNodes\n\
                $Elements\n3\n1 1 2 1 1 1 2\n2 2 2 1 1 1 2 3\n3 15 2 1 1 3\n$EndElements\n";
    for (dim, beam) in [
        (1, ElementType::BernoulliBeam2),
        (2, ElementType::BernoulliBeam2),
        (3, ElementType::BernoulliBeam3),
    ] {
        let mut m = Mesh::new(dim, "m").unwrap();
        let r = read_mesh_from_str(
            &mut m,
            "t.msh",
            text,
            MeshIoType::GmshStruct,
            &ReadOptions::default(),
        )
        .unwrap();
        let types: Vec<ElementType> = m.element_types(TypeFilter::new()).collect();
        assert_eq!(
            types,
            vec![beam, ElementType::DiscreteKirchhoffTriangle18],
            "dim {dim}"
        );
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("Unsuported element kind 15"))
        );
        assert_eq!(
            r.warnings.iter().any(|w| w.contains("extra_normal")),
            dim == 3
        );
    }
}

/// `$Periodic` is parsed and ignored (with and without an `Affine` line),
/// unknown sections are skipped with a warning.
#[test]
fn periodic_and_unknown_sections() {
    let base = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n$Elements\n1\n1 1 2 0 0 1 2\n$EndElements\n";
    for extra in [
        "$Periodic\n1\n0 1 2\n1\n1 2\n$EndPeriodic\n",
        "$Periodic\n1\n0 1 2\nAffine 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1\n1\n1 2\n$EndPeriodic\n",
        "$Comments\nanything $Nodes\n$EndComments\n",
    ] {
        let mut m = Mesh::new(1, "m").unwrap();
        let r = read_mesh_from_str(
            &mut m,
            "t.msh",
            &format!("{base}{extra}"),
            MeshIoType::Gmsh,
            &ReadOptions::default(),
        )
        .unwrap_or_else(|e| panic!("{extra}: {e}"));
        assert_eq!(m.nb_nodes(), 2);
        assert_eq!(
            r.warnings
                .iter()
                .any(|w| w.contains("Unsupported block_kind Comments")),
            extra.contains("Comments")
        );
    }
}

/// Malformed input: a parse error with the right line, never a panic, and
/// the target mesh is left untouched.
#[test]
fn malformed_files_are_errors_with_locations() {
    let h = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n";
    let nodes = "$Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n";
    let cases: Vec<(String, usize)> = vec![
        (format!("{h}$Nodes\n2\n1 0 0 0\n2 1 zero 0\n$EndNodes\n"), 7),
        (format!("{h}$Nodes\n2\n1 0 0 0\n$EndNodes\n"), 7),
        (format!("{h}$Nodes\n1\n1 0 0\n$EndNodes\n"), 6),
        (format!("{h}$Nodes\n2\n1 0 0 0\n1 1 0 0\n$EndNodes\n"), 7),
        (
            format!("{h}{nodes}$Elements\n1\n1 1 2 0 0 1 3\n$EndElements\n"),
            11,
        ),
        (
            format!("{h}{nodes}$Elements\n1\n1 1 2 0 0 1\n$EndElements\n"),
            11,
        ),
        (
            format!("{h}{nodes}$Elements\n1\n1 1 2 0 0 1 2\n$EndElemnts\n"),
            12,
        ),
        (
            format!("{h}{nodes}$Elements\n1\n1 1 -2 0 0 1 2\n$EndElements\n"),
            11,
        ),
        (format!("{h}{nodes}$Unknown\nno end\n"), 11),
        (format!("{h}{nodes}junk\n"), 9),
        ("$Nodes\n1\n1 0 0 0\n$EndNodes\n".into(), 1),
        (
            format!("{h}{nodes}$PhysicalNames\n1\n1 2 \"open\n$EndPhysicalNames\n"),
            11,
        ),
        (
            format!("{h}{nodes}$NodeData\n1\n\"t\"\n0\n3\n0\n1\n1\n3 1.0\n$EndNodeData\n"),
            17,
        ),
        (
            format!("{h}{nodes}$NodeData\n1\n\"t\"\n0\n2\n0\n1\n$EndNodeData\n"),
            15,
        ),
        (
            format!(
                "{h}{nodes}$Elements\n1\n1 1 2 0 0 1 2\n$EndElements\n$ElementData\n1\n\"d\"\n0\n3\n0\n1\n1\n9 1\n$EndElementData\n"
            ),
            21,
        ),
    ];
    for (text, line) in cases {
        let mut m = Mesh::new(2, "m").unwrap();
        let e = read_mesh_from_str(
            &mut m,
            "bad.msh",
            &text,
            MeshIoType::Gmsh,
            &ReadOptions::default(),
        )
        .expect_err(&text);
        match &e {
            Error::Parse { file, line: l, .. } => {
                assert_eq!(file, "bad.msh");
                assert_eq!(*l, line, "{e}\n{text}");
            }
            other => panic!("expected a parse error, got {other}\n{text}"),
        }
        assert_eq!(m.nb_nodes(), 0, "mesh modified by a failed read");
        assert!(m.data().is_empty() && m.groups().element_groups().next().is_none());
    }
    for (text, what) in [
        (h.to_string(), "no nodes"),
        ("$MeshFormat\n3.0 0 8\n$EndMeshFormat\n".to_string(), "version 3"),
        ("$MeshFormat\n2.2 1 8\n$EndMeshFormat\n".to_string(), "binary"),
        ("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$Nodes\n1 1 1 1\n2 1 1 1\n1\n0 0 0 0.5\n$EndNodes\n".to_string(), "parametric"),
    ] {
        let mut m = Mesh::new(2, "m").unwrap();
        let r = read_mesh_from_str(&mut m, "x.msh", &text, MeshIoType::Gmsh, &ReadOptions::default());
        match what {
            "no nodes" => assert!(r.unwrap().warnings.iter().any(|w| w.contains("good dimension"))),
            _ => assert!(matches!(r, Err(Error::Unsupported(_))), "{what}: {r:?}"),
        }
    }
    let mut m = Mesh::new(2, "m").unwrap();
    assert!(
        read_mesh_from_str(
            &mut m,
            "e.msh",
            "",
            MeshIoType::Gmsh,
            &ReadOptions::default()
        )
        .is_err()
    );
}

/// Counts in headers never size allocations beyond the input: huge counts
/// end in a parse error (end of file) or a capacity error, not an abort.
#[test]
fn huge_counts_are_errors() {
    let h = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n";
    let nodes = "$Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n";
    for text in [
        format!("{h}$Nodes\n1000000000000000000\n1 0 0 0\n$EndNodes\n"),
        format!("{h}$Nodes\n9223372036854775807\n$EndNodes\n"),
        "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$Nodes\n1 1000000000000000000 1 2\n0 1 0 1000000000000000000\n1\n$EndNodes\n".to_string(),
        format!("{h}{nodes}$Elements\n1\n1 1 1000000000000 0 0 1 2\n$EndElements\n"),
        format!("{h}{nodes}$NodeData\n1\n\"t\"\n0\n3\n0\n1000000000\n1\n1 1\n$EndNodeData\n"),
        format!("{h}{nodes}$NodeData\n1\n\"t\"\n0\n3\n0\n4000\n1000000000000\n1 1\n$EndNodeData\n"),
    ] {
        let r = read_str(2, &text);
        assert!(r.is_err(), "{text}");
    }
}

#[test]
fn reading_needs_an_empty_mesh() {
    let path = fixtures_root().join("mesh_io/test_fe_engine/_triangle_3/inputs/_triangle_3.msh");
    let mut m = Mesh::new(2, "m").unwrap();
    read_mesh(&mut m, &path, MeshIoType::Auto).unwrap();
    let n = m.nb_nodes();
    assert!(matches!(
        read_mesh(&mut m, &path, MeshIoType::Auto),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(m.nb_nodes(), n);
    let mut m = Mesh::new(2, "m").unwrap();
    let e = read_mesh(&mut m, Path::new("/nonexistent/x.msh"), MeshIoType::Auto).unwrap_err();
    assert!(matches!(e, Error::Io { .. }), "{e}");
    let e = read_mesh_with(
        &mut m,
        Path::new("x.unknown"),
        MeshIoType::Auto,
        &ReadOptions::default(),
    )
    .unwrap_err();
    assert!(
        e.to_string().contains("Cannot guess the type of file"),
        "{e}"
    );
}

/// Writing types without a Gmsh code (cohesive) skips them; Diana writing
/// is not implemented upstream.
#[test]
fn writer_edge_cases() {
    let mut m = Mesh::new(2, "m").unwrap();
    {
        let mut a = akantu_mesh::MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2]);
        a.connectivity_mut(ElementType::Cohesive2d4, GhostType::NotGhost)
            .push_tuple(&[0, 1, 3, 2]);
        a.connectivity_mut(ElementType::Triangle3, GhostType::Ghost)
            .push_tuple(&[0, 2, 3]);
    }
    let t = write_mesh_to_string(
        &m,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::default(),
    )
    .unwrap();
    assert!(
        t.contains("$Elements\n1\n1 2 2 0 0 1 2 3\n$EndElements\n"),
        "{t}"
    );
    assert!(
        t.contains("$Nodes\n4\n1 0 0 0\n2 1 0 0\n3 1 1 0\n4 0 1 0\n$EndNodes\n"),
        "{t}"
    );
    assert!(matches!(
        write_mesh_to_string(
            &m,
            Path::new("x.diana"),
            MeshIoType::Auto,
            &WriteOptions::default()
        ),
        Err(Error::NotImplemented(_))
    ));
}

fn fuzz_corpus() -> Vec<(usize, String)> {
    [
        ("mesh_io/test_fe_engine/_triangle_6", "_triangle_6.msh", 2),
        (
            "mesh_io/test_fe_engine/_hexahedron_20",
            "_hexahedron_20.msh",
            3,
        ),
        (
            "mesh_io/test_io/test_dumper/test_dumper",
            "test_dumper.msh",
            3,
        ),
        (
            "mesh_io/test_model/test_phase_field_model/test_two_element",
            "test_two_element.msh",
            2,
        ),
        (
            "mesh_io/test_model/test_phase_field_model/test_four_elements",
            "test_four_elements.msh",
            2,
        ),
        ("io/mesh_io/data/data_blocks", "data_blocks.msh", 2),
        (
            "io/mesh_io/msh4/unnamed_physical",
            "unnamed_physical_41.msh",
            2,
        ),
        ("io/mesh_io/msh1/msh1", "msh1.msh", 2),
    ]
    .iter()
    .map(|(case, file, dim)| {
        let p = fixtures_root().join(case).join("inputs").join(file);
        (
            *dim,
            std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        )
    })
    .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    /// Truncated, corrupted, shuffled or spliced files never panic; a
    /// failed read leaves the mesh empty.
    #[test]
    fn fuzzed_gmsh_files_never_panic(
        which in 0usize..8,
        cut in 0.0f64..1.0,
        edits in proptest::collection::vec((0.0f64..1.0, any::<u8>()), 0..6),
        swap in proptest::option::of((0.0f64..1.0, 0.0f64..1.0)),
        dup in proptest::option::of(0.0f64..1.0),
    ) {
        let corpus = fuzz_corpus();
        let (dim, text) = &corpus[which];
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let at = |f: f64, n: usize| ((f * n as f64) as usize).min(n.saturating_sub(1));
        if let Some((a, b)) = swap {
            let n = lines.len();
            lines.swap(at(a, n), at(b, n));
        }
        if let Some(d) = dup {
            let i = at(d, lines.len());
            let l = lines[i].clone();
            lines.insert(i, l);
        }
        let mut bytes = lines.join("\n").into_bytes();
        for (pos, b) in edits {
            let i = at(pos, bytes.len());
            bytes[i] = b;
        }
        let keep = at(cut, bytes.len() + 1);
        bytes.truncate(keep.max(bytes.len() * 9 / 10));
        let s = String::from_utf8_lossy(&bytes);
        let mut m = Mesh::new(*dim, "m").unwrap();
        let r = read_mesh_from_str(&mut m, "fuzz.msh", &s, MeshIoType::Gmsh, &ReadOptions::default());
        if r.is_err() {
            prop_assert_eq!(m.nb_nodes(), 0);
        } else {
            prop_assert!(m.check_consistency().is_ok());
        }
    }
}
