// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the Diana
// reader (src/io/mesh_io/mesh_io_diana.cc) beyond the 3-D oracle fixtures
// (fixtures/io/mesh_io/diana): the 2-D case upstream cannot read (UD-048),
// malformed input and the material side file.

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, Error, GhostType, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_from_str, read_mesh_with};
use akantu_mesh::{Mesh, MeshDataValues};
use proptest::prelude::*;

const PLANAR: &str = "'COORDINATES'
     1  0.000000E+00  0.000000E+00  0.000000E+00
     2  1.000000E+00  0.000000E+00
     3  1.000000E+00  1.000000E+00  0.0
     4  0.000000E+00  1.000000E+00  0
     5  2.000000E+00  0.000000E+00  0
     6  2.000000E+00  1.000000E+00  0
     7  1.500000E+00  0.000000E+00  0
     8  2.000000E+00  0.500000E+00  0
     9  1.500000E+00  0.500000E+00  0
'ELEMENTS'
CONNECTIVITY
 10 Q12TM 1 2 3 4
 11 T9TM 2 5 6
 12 CT6CM 2 5 3
 7 8 9
MATERIALS
/ 10-11 / 3
'MATERIALS'
   1 YOUNG 1.0E+09
     POISON 0.25
'GROUPS'
ELEMEN
   1 LEFT / 10 /
   2 RIGHT / 11-12 /
NODES
   3 BASE / 1 2 5 /
   4 TOP / 4 3
 6 /
'SUPPORTS'
'END'
";

fn read_text(dim: usize, text: &str) -> akantu_core::Result<(Mesh, akantu_io::ReadReport)> {
    let mut m = Mesh::new(dim, "m")?;
    let opts = ReadOptions {
        write_diana_material_file: false,
        ..ReadOptions::default()
    };
    let r = read_mesh_from_str(&mut m, "t.diana", text, MeshIoType::Diana, &opts)?;
    Ok((m, r))
}

/// UD-048: a planar Diana mesh is read with two components per node
/// (missing or zero third coordinates), elements, materials and groups as
/// in 3-D. Expectations derived by hand from the file.
#[test]
fn planar_mesh_is_read_in_2d() {
    let (m, r) = read_text(2, PLANAR).unwrap();
    assert_eq!(m.nodes().nb_component(), 2);
    assert_eq!(
        m.nodes().as_slice(),
        &[
            0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 2.0, 0.0, 2.0, 1.0, 1.5, 0.0, 2.0, 0.5, 1.5,
            0.5
        ]
    );
    let ng = GhostType::NotGhost;
    assert_eq!(
        m.connectivity(ElementType::Quadrangle4, ng)
            .unwrap()
            .as_slice(),
        &[0, 1, 2, 3]
    );
    assert_eq!(
        m.connectivity(ElementType::Triangle3, ng)
            .unwrap()
            .as_slice(),
        &[1, 4, 5]
    );
    assert_eq!(
        m.connectivity(ElementType::Triangle6, ng)
            .unwrap()
            .as_slice(),
        &[1, 4, 2, 6, 7, 8]
    );
    let MeshDataValues::Int(mat) = m.data().elemental("material").unwrap() else {
        panic!()
    };
    assert_eq!(
        mat.get(ElementType::Quadrangle4, ng).unwrap().as_slice(),
        &[3]
    );
    assert_eq!(
        mat.get(ElementType::Triangle3, ng).unwrap().as_slice(),
        &[3]
    );
    assert_eq!(
        mat.get(ElementType::Triangle6, ng).unwrap().as_slice(),
        &[0],
        "no interval → 0"
    );
    let left = m.groups().element_group("LEFT").unwrap();
    assert_eq!(left.elements_of(ElementType::Quadrangle4, ng), &[0]);
    assert_eq!(left.dimension, 2);
    let right = m.groups().element_group("RIGHT").unwrap();
    assert_eq!(right.elements_of(ElementType::Triangle3, ng), &[0]);
    assert_eq!(right.elements_of(ElementType::Triangle6, ng), &[0]);
    assert_eq!(
        m.groups().node_group("BASE").unwrap().node_list(),
        vec![0, 1, 4]
    );
    assert_eq!(
        m.groups().node_group("TOP").unwrap().node_list(),
        vec![3, 2, 5],
        "continued interval list"
    );
    let mats = r.diana_materials.unwrap();
    assert_eq!(
        mats.to_input_file(),
        "material elastic [\n\tname = material1\n\tE = 1e+09\n\tnu = 0.25\n]\n"
    );
    assert_eq!(m.bbox().upper, vec![2.0, 1.0]);
    // In 3-D every node needs three coordinates (node 2 has two).
    assert!(matches!(
        read_text(3, PLANAR).map(|_| ()),
        Err(Error::Parse { line: 3, .. })
    ));
    let full = PLANAR.replace(
        "     2  1.000000E+00  0.000000E+00\n",
        "     2  1.000000E+00  0.000000E+00  0.25\n",
    );
    let (m3, _) = read_text(3, &full).unwrap();
    assert_eq!(m3.nodes().nb_component(), 3);
    assert_eq!(m3.nodes().tuple(1), &[1.0, 0.0, 0.25]);
}

/// UD-048 rule: a non-zero out-of-plane coordinate in a 2-D mesh is an
/// error (no silent projection); too few coordinates as well.
#[test]
fn non_planar_nodes_are_rejected_in_2d() {
    let bad = PLANAR.replace(
        "     4  0.000000E+00  1.000000E+00  0",
        "     4  0.0  1.0  0.5",
    );
    let e = read_text(2, &bad).map(|_| ()).unwrap_err();
    assert!(matches!(e, Error::Parse { line: 5, .. }), "{e}");
    let bad3 = bad.replace(
        "     2  1.000000E+00  0.000000E+00\n",
        "     2  1.0  0.0  0.0\n",
    );
    assert!(read_text(3, &bad3).is_ok());
    let short = PLANAR.replace("     4  0.000000E+00  1.000000E+00  0", "     4  0.0");
    assert!(matches!(
        read_text(2, &short).map(|_| ()).unwrap_err(),
        Error::Parse { line: 5, .. }
    ));
}

/// End of file inside a section (upstream loops forever), undefined or
/// duplicate numbers, malformed numbers: errors with locations; the mesh
/// stays empty.
#[test]
fn malformed_diana_files() {
    let cases: Vec<(String, &str)> = vec![
        ("'COORDINATES'\n 1 0 0 0\n".into(), "end of file"),
        (PLANAR.replace("'SUPPORTS'\n'END'\n", ""), "end of file"),
        (
            PLANAR.replace(
                "MATERIALS\n/ 10-11 / 3\n'MATERIALS'",
                "MATERIALS\n/ 10-11 / x\n'MATERIALS'",
            ),
            "material number",
        ),
        (
            PLANAR.replace(" 11 T9TM 2 5 6", " 11 T9TM 2 5 99"),
            "node 99",
        ),
        (
            PLANAR.replace("     2  1.000000E+00  0.000000E+00", "     1  1.0 0.0"),
            "duplicate node",
        ),
        (
            PLANAR.replace(" 11 T9TM 2 5 6", " 10 T9TM 2 5 6"),
            "duplicate element",
        ),
        (
            PLANAR.replace(" 12 CT6CM 2 5 3\n 7 8 9", " 12 CT6CM 2 5 3\nMATERIALS"),
            "needs 6 nodes",
        ),
        (
            PLANAR.replace("   3 BASE / 1 2 5 /", "   3 BASE / 1 2 77 /"),
            "node 77",
        ),
        (
            PLANAR.replace("   3 BASE / 1 2 5 /", "   3 BASE / 1- /"),
            "malformed interval",
        ),
        (
            PLANAR.replace(
                "     3  1.000000E+00  1.000000E+00  0.0",
                "     3  1.0  one  0.0",
            ),
            "coordinate",
        ),
    ];
    for (text, needle) in cases {
        let mut m = Mesh::new(2, "m").unwrap();
        let e = read_mesh_from_str(
            &mut m,
            "bad.diana",
            &text,
            MeshIoType::Diana,
            &ReadOptions::default(),
        )
        .expect_err(needle);
        assert!(
            e.to_string().contains(needle),
            "expected '{needle}', got '{e}'"
        );
        assert_eq!(m.nb_nodes(), 0);
    }
}

/// Unknown element types and unknown element numbers in groups are skipped
/// (as upstream), with a warning for the former.
#[test]
fn unknown_types_and_numbers_are_skipped() {
    let text = PLANAR
        .replace(" 11 T9TM 2 5 6", " 11 T9TM 2 5 6\n 13 BEAM2 1 2")
        .replace("   2 RIGHT / 11-12 /", "   2 RIGHT / 11-13 40 /");
    let (m, r) = read_text(2, &text).unwrap();
    assert!(
        r.warnings.iter().any(|w| w.contains("BEAM2")),
        "{:?}",
        r.warnings
    );
    let right = m.groups().element_group("RIGHT").unwrap();
    let types: Vec<ElementType> = right.elements.element_types(TypeFilter::new()).collect();
    assert_eq!(types, vec![ElementType::Triangle3, ElementType::Triangle6]);
}

/// Huge intervals are resolved against the defined numbers (no expansion,
/// no hang): undefined elements are skipped, an undefined node is an error.
#[test]
fn huge_intervals_are_cheap() {
    let text = PLANAR
        .replace("/ 10-11 / 3", "/ 10-9223372036854775807 / 3")
        .replace(
            "   2 RIGHT / 11-12 /",
            "   2 RIGHT / -9223372036854775807-9223372036854775807 /",
        );
    let (m, _) = read_text(2, &text).unwrap();
    let MeshDataValues::Int(mat) = m.data().elemental("material").unwrap() else {
        panic!()
    };
    assert_eq!(
        mat.get(ElementType::Triangle6, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        &[3]
    );
    let right = m.groups().element_group("RIGHT").unwrap();
    assert_eq!(right.elements.element_types(TypeFilter::new()).count(), 3);
    let bad = PLANAR.replace("   3 BASE / 1 2 5 /", "   3 BASE / 1-9223372036854775807 /");
    let e = read_text(2, &bad).map(|_| ()).unwrap_err();
    assert!(e.to_string().contains("node 10 is not defined"), "{e}");
}

/// Upstream writes `material_<input file name>` while reading; the port
/// does it next to the input when `write_diana_material_file` is set.
#[test]
fn material_side_file() {
    let dir = std::env::temp_dir().join(format!("akantu-io-diana-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("plate.diana");
    std::fs::write(&input, PLANAR).unwrap();
    let mut m = Mesh::new(2, "m").unwrap();
    let r = read_mesh_with(&mut m, &input, MeshIoType::Auto, &ReadOptions::default()).unwrap();
    let side = std::fs::read_to_string(dir.join("material_plate.diana")).unwrap();
    assert_eq!(side, r.diana_materials.unwrap().to_input_file());
    let mut m = Mesh::new(2, "m").unwrap();
    std::fs::remove_file(dir.join("material_plate.diana")).unwrap();
    let opts = ReadOptions {
        write_diana_material_file: false,
        ..ReadOptions::default()
    };
    read_mesh_with(&mut m, &input, MeshIoType::Auto, &opts).unwrap();
    assert!(!dir.join("material_plate.diana").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    /// Corrupted or truncated Diana files never panic or hang.
    #[test]
    fn fuzzed_diana_files_never_panic(
        cut in 0.0f64..1.0,
        edits in proptest::collection::vec((0.0f64..1.0, any::<u8>()), 0..6),
        dim in 2usize..4,
    ) {
        let mut bytes = PLANAR.as_bytes().to_vec();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let at = |f: f64, n: usize| ((f * n as f64) as usize).min(n.saturating_sub(1));
        for (pos, b) in edits {
            let i = at(pos, bytes.len());
            bytes[i] = b;
        }
        let keep = at(cut, bytes.len() + 1);
        bytes.truncate(keep.max(bytes.len() / 2));
        let s = String::from_utf8_lossy(&bytes);
        let mut m = Mesh::new(dim, "m").unwrap();
        let r = read_mesh_from_str(&mut m, "fuzz.diana", &s, MeshIoType::Diana, &ReadOptions::default());
        if r.is_err() {
            prop_assert_eq!(m.nb_nodes(), 0);
        } else {
            prop_assert!(m.check_consistency().is_ok());
        }
    }
}

/// The Diana reader ends with upstream's
/// `fillElementToSubElementsData` (`mesh_io_diana.cc:174`), staged inside
/// the read. Mixed-dimensional 3-D mesh: one hexahedron and the boundary
/// quadrangle of its bottom face. The data equal the (oracle-verified,
/// `fixtures/mesh_utils` `read/*`) algorithm applied to the same mesh, and
/// the expected adjacency derived by hand.
#[test]
fn reader_fills_element_to_subelement_data() {
    use akantu_core::Element;
    use akantu_mesh::utils::{
        ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, compute_element_to_subelements_data,
    };
    let text = "'COORDINATES'
     1  0.0  0.0  0.0
     2  1.0  0.0  0.0
     3  1.0  1.0  0.0
     4  0.0  1.0  0.0
     5  0.0  0.0  1.0
     6  1.0  0.0  1.0
     7  1.0  1.0  1.0
     8  0.0  1.0  1.0
'ELEMENTS'
CONNECTIVITY
 1 HX24L 1 2 3 4 5 6 7 8
 2 Q12TM 1 2 3 4
'END'
";
    let (m, _) = read_text(3, text).unwrap();
    let ng = GhostType::NotGhost;
    let hexa = Element::new(ElementType::Hexahedron8, 0, ng);
    let quad = Element::new(ElementType::Quadrangle4, 0, ng);
    let e2s = m.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    assert_eq!(e2s.element(quad).unwrap(), &[vec![hexa]]);
    let s2e = m.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
    let row = s2e.element(hexa).unwrap();
    assert_eq!(row.len(), 6);
    assert_eq!(row[0], quad);
    assert!(row[1..].iter().all(|e| e.is_null()));
    // the reader's data are exactly the algorithm's on the read mesh
    let mut bare = Mesh::new(3, "bare").unwrap();
    {
        let mut a = akantu_mesh::MeshAccessor::new(&mut bare);
        for x in m.nodes().tuples() {
            a.push_node(x).unwrap();
        }
        for (ty, g, c) in m.connectivities().as_map().iter_all() {
            a.connectivity_mut(ty, g).replace(c.borrow().clone());
        }
    }
    let (want_e2s, want_s2e) = compute_element_to_subelements_data(&bare).unwrap().unwrap();
    assert_eq!(format!("{e2s:?}"), format!("{want_e2s:?}"));
    assert_eq!(format!("{s2e:?}"), format!("{want_s2e:?}"));
    // a 3-D mesh without facets carries no such data (upstream: none)
    let only_hexa = text.replace(" 2 Q12TM 1 2 3 4\n", "");
    let (m, _) = read_text(3, &only_hexa).unwrap();
    assert!(!m.data().has_elemental(ELEMENT_TO_SUBELEMENT));
    assert!(!m.data().has_elemental(SUBELEMENT_TO_ELEMENT));
}
