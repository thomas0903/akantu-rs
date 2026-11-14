// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests
// against `fixtures/io/mesh_io/*` (oracle/probes/mesh_io/mesh_io_probe.cc):
// explicit reader types (structural Gmsh, Diana), data blocks, the text of
// `Mesh::write`, the Diana material file and inputs the oracle rejects.

mod common;

use std::path::{Path, PathBuf};

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, Error, GhostType, TypeFilter};
use akantu_io::mesh_io::DianaMaterials;
use akantu_io::{ReadOptions, ReadReport, WriteOptions, read_mesh_with, write_mesh_to_string};
use akantu_mesh::{Mesh, MeshDataValues, NodalDataValues};
use akantu_validation::{Fixture, fixtures_root};
use common::CompareOpts;

fn case_dir(name: &str) -> PathBuf {
    fixtures_root().join("io/mesh_io").join(name)
}

/// Oracle parity options (upstream leaves `msh_block_*` groups empty).
fn compat() -> ReadOptions {
    ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    }
}

fn format_of(fx: &Fixture) -> MeshIoType {
    match fx.manifest().parameters["mesh_io_type"].as_str().unwrap() {
        "gmsh" => MeshIoType::Gmsh,
        "gmsh_struct" => MeshIoType::GmshStruct,
        "diana" => MeshIoType::Diana,
        _ => MeshIoType::Auto,
    }
}

fn read_case(fx: &Fixture, opts: &ReadOptions) -> akantu_core::Result<(Mesh, ReadReport)> {
    let p = &fx.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_u64().unwrap()).unwrap();
    let mut mesh = Mesh::new(dim, "mesh")?;
    let path = fx.input_path("mesh").expect("mesh input");
    let report = read_mesh_with(&mut mesh, &path, format_of(fx), opts)?;
    Ok((mesh, report))
}

fn expects_error(fx: &Fixture) -> bool {
    fx.manifest().expect.status == akantu_validation::manifest::ExpectedStatus::Error
}

/// Deviations of a case (each documented in docs/UPSTREAM_DEFECTS.md).
fn deviations(case: &str) -> CompareOpts {
    match case {
        // UD-047: upstream keeps only the first node's values.
        "io/mesh_io/data/data_blocks" => CompareOpts {
            skip_quantities: vec!["mesh_data/nodal_temp/_triangle_3".into()],
            ..CompareOpts::default()
        },
        "io/mesh_io/data/element_node_data_3d" => CompareOpts {
            skip_quantities: vec![
                "mesh_data/field/_tetrahedron_10".into(),
                "mesh_data/field/_pentahedron_6".into(),
            ],
            ..CompareOpts::default()
        },
        // UD-094: Diana group members under an uninitialised ghost type.
        c if c.starts_with("io/mesh_io/diana/") => CompareOpts {
            fold_ghost_groups: true,
            ..CompareOpts::default()
        },
        _ => CompareOpts::default(),
    }
}

/// The `io/mesh_io` cases (all committed; external ones would be listed
/// and skipped when not materialised).
fn all_cases(test: &str) -> Vec<Fixture> {
    let set = common::CaseSet::new(&common::case_dirs(&fixtures_root().join("io/mesh_io")));
    set.check_and_announce(test);
    set.materialized
}

#[test]
fn successful_oracle_reads_match() {
    let cases = all_cases("successful_oracle_reads_match");
    assert!(cases.len() >= 28, "found {} io/mesh_io cases", cases.len());
    let mut failures = Vec::new();
    let mut checked = 0;
    for fx in &cases {
        let dir = fx.dir();
        if expects_error(fx) {
            continue;
        }
        let (mesh, _) = read_case(fx, &compat())
            .unwrap_or_else(|e| panic!("{}: Rust read failed: {e}", fx.case_id()));
        if let Err(e) = common::compare_with_fixture_opts(
            "successful_oracle_reads_match",
            dir,
            &mesh,
            &deviations(fx.case_id()),
        ) {
            failures.push(format!("{}:\n{e}", fx.case_id()));
        }
        checked += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    assert!(checked >= 24, "{checked}");
}

/// Node order the reader applies (Gmsh node j → Akantu slot order[j]),
/// written here independently of the implementation.
fn gmsh_read_order(ty: &str) -> Option<Vec<usize>> {
    Some(match ty {
        "11" => vec![0, 1, 2, 3, 4, 5, 6, 7, 9, 8],
        "6" => vec![2, 0, 1, 5, 3, 4],
        "18" => vec![2, 0, 1, 5, 3, 4, 8, 7, 11, 6, 9, 10, 14, 13, 12],
        "17" => vec![
            0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 9, 13, 10, 14, 15, 16, 19, 17, 18,
        ],
        _ => return None,
    })
}

/// Compare a Rust-written file with the oracle's `Mesh::write` output: every
/// line up to `$EndElements` must be identical, except that element lines
/// of types with a node permutation list the nodes in Gmsh order (UD-093:
/// upstream writes Akantu order). Rust may append data blocks (UD-044).
fn compare_written(case: &str, rust: &str, oracle: &str, structural: bool) -> Result<(), String> {
    let r: Vec<&str> = rust.lines().collect();
    let o: Vec<&str> = oracle.lines().collect();
    let end = o
        .iter()
        .position(|l| *l == "$EndElements")
        .ok_or("oracle output has no $EndElements")?;
    if o.len() != end + 1 {
        return Err(format!(
            "{case}: oracle wrote data blocks; extend the comparison"
        ));
    }
    if r.len() < o.len() {
        return Err(format!(
            "{case}: Rust output has {} lines, oracle {}",
            r.len(),
            o.len()
        ));
    }
    let start = o.iter().position(|l| *l == "$Elements").unwrap() + 2;
    let mut permuted = 0;
    for (i, (a, b)) in r.iter().zip(&o).enumerate().take(end + 1) {
        if a == b {
            continue;
        }
        let fa: Vec<&str> = a.split(' ').collect();
        let fb: Vec<&str> = b.split(' ').collect();
        let order = if structural {
            None
        } else {
            gmsh_read_order(fb.get(1).copied().unwrap_or(""))
        };
        match order {
            Some(order) if i >= start && i < end && fa.len() == fb.len() && fa[..5] == fb[..5] => {
                // Rust lists Gmsh node j at position j; upstream lists Akantu
                // slot s at position s, slot order[j] holding Gmsh node j.
                let (rn, on) = (&fa[5..], &fb[5..]);
                if (0..rn.len()).any(|j| rn[j] != on[order[j]]) {
                    return Err(format!(
                        "{case}:{}: '{a}' is not the Gmsh order of '{b}'",
                        i + 1
                    ));
                }
                permuted += 1;
            }
            _ => return Err(format!("{case}:{}: Rust '{a}' vs oracle '{b}'", i + 1)),
        }
    }
    // Anything after `$EndElements` must be data blocks (UD-044).
    const ALLOWED: [&str; 6] = [
        "$NodeData",
        "$EndNodeData",
        "$ElementData",
        "$EndElementData",
        "$ElementNodeData",
        "$EndElementNodeData",
    ];
    if let Some(l) = r[end + 1..]
        .iter()
        .find(|l| l.starts_with('$') && !ALLOWED.contains(l))
    {
        return Err(format!("{case}: unexpected section {l} after $EndElements"));
    }
    if permuted > 0 && structural {
        return Err(format!(
            "{case}: the structural writer must not permute nodes"
        ));
    }
    Ok(())
}

#[test]
fn written_text_matches_the_oracle_writer() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for fx in all_cases("written_text_matches_the_oracle_writer") {
        let Ok(oracle_path) = fx.input_path("oracle_written") else {
            continue;
        };
        let (mesh, _) = read_case(&fx, &compat()).unwrap();
        let format = match format_of(&fx) {
            MeshIoType::Auto => MeshIoType::Gmsh,
            f => f,
        };
        let rust = write_mesh_to_string(
            &mesh,
            Path::new("x.msh"),
            format,
            &WriteOptions::upstream_precision(),
        )
        .unwrap();
        let oracle = std::fs::read_to_string(&oracle_path).unwrap();
        if let Err(e) = compare_written(
            fx.case_id(),
            &rust,
            &oracle,
            format == MeshIoType::GmshStruct,
        ) {
            failures.push(e);
        }
        checked += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked >= 13, "{checked}");
}

/// UD-093 evidence: the oracle writes 10-node tetrahedra in Akantu order,
/// so reading its output back permutes their connectivity; the Rust output
/// reads back unchanged.
#[test]
fn written_files_read_back_unchanged() {
    let fx = Fixture::open(&case_dir("write/tetrahedron_10")).unwrap();
    let (mesh, _) = read_case(&fx, &compat()).unwrap();
    let ty = ElementType::Tetrahedron10;
    let conn = mesh
        .connectivity(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let reread = |text: &str| {
        let mut m = Mesh::new(3, "m").unwrap();
        akantu_io::read_mesh_from_str(&mut m, "w.msh", text, MeshIoType::Gmsh, &compat()).unwrap();
        m.connectivity(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec()
    };
    let rust = write_mesh_to_string(
        &mesh,
        Path::new("x.msh"),
        MeshIoType::Gmsh,
        &WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(reread(&rust), conn);
    let oracle = std::fs::read_to_string(fx.input_path("oracle_written").unwrap()).unwrap();
    assert_ne!(
        reread(&oracle),
        conn,
        "upstream round trip should permute tet10 nodes 8/9"
    );
}

/// UD-044: the oracle cannot write meshes with nodal data (it throws); the
/// Rust writer writes node, element and element-node data, and reading the
/// output gives the same datasets back.
#[test]
fn data_blocks_are_written_and_read_back() {
    for name in [
        "data/data_blocks",
        "write/node_data_only",
        "data/element_node_data_3d",
    ] {
        let fx = Fixture::open(&case_dir(name)).unwrap();
        let p = &fx.manifest().parameters;
        if name != "data/element_node_data_3d" {
            assert!(
                p["write_error"]
                    .as_str()
                    .unwrap_or("")
                    .contains("No dataset named"),
                "{name}: the oracle is expected to fail writing (UD-044)"
            );
        }
        let (mesh, _) = read_case(&fx, &ReadOptions::default()).unwrap();
        let text = write_mesh_to_string(
            &mesh,
            Path::new("x.msh"),
            MeshIoType::Gmsh,
            &WriteOptions::default(),
        )
        .unwrap();
        let mut back = Mesh::new(mesh.spatial_dimension(), "back").unwrap();
        akantu_io::read_mesh_from_str(
            &mut back,
            "back.msh",
            &text,
            MeshIoType::Gmsh,
            &ReadOptions::default(),
        )
        .unwrap();
        assert_eq!(
            mesh.data().nodal_names().collect::<Vec<_>>(),
            back.data().nodal_names().collect::<Vec<_>>(),
            "{name}: nodal dataset names"
        );
        // Real elemental datasets are written (UD-044); `tag_0`/`tag_1`
        // too, `physical_names` not (as upstream).
        let real_names = |m: &Mesh| -> Vec<String> {
            m.data()
                .elemental_names()
                .filter(|n| matches!(m.data().elemental(n), Ok(MeshDataValues::Real(_))))
                .map(str::to_owned)
                .collect()
        };
        assert_eq!(
            real_names(&mesh),
            real_names(&back),
            "{name}: element datasets"
        );
        for n in mesh.data().nodal_names() {
            let (NodalDataValues::Real(a), NodalDataValues::Real(b)) =
                (mesh.data().nodal(n).unwrap(), back.data().nodal(n).unwrap())
            else {
                panic!("{name}: {n} not real");
            };
            assert_eq!(a.nb_component(), b.nb_component());
            assert_eq!(a.len(), mesh.nb_nodes(), "{name}: {n}");
            assert_eq!(b.len(), back.nb_nodes(), "{name}: {n}");
            assert!(
                a.as_slice()
                    .iter()
                    .zip(b.as_slice())
                    .all(|(x, y)| x.to_bits() == y.to_bits()),
                "{name}: {n}"
            );
        }
        for n in mesh.data().elemental_names() {
            let MeshDataValues::Real(a) = mesh.data().elemental(n).unwrap() else {
                continue;
            };
            let MeshDataValues::Real(b) = back.data().elemental(n).unwrap() else {
                panic!("{name}: {n} lost");
            };
            assert_eq!(
                mesh.data().is_elemental_nodal(n),
                back.data().is_elemental_nodal(n)
            );
            for ty in a.element_types(TypeFilter::new()) {
                let (x, y) = (
                    a.get(ty, GhostType::NotGhost).unwrap(),
                    b.get(ty, GhostType::NotGhost).unwrap(),
                );
                assert_eq!(x.nb_component(), y.nb_component(), "{name}: {n} {ty}");
                assert_eq!(x.len(), y.len(), "{name}: {n} {ty}");
                assert!(
                    x.as_slice()
                        .iter()
                        .zip(y.as_slice())
                        .all(|(u, v)| u.to_bits() == v.to_bits()),
                    "{name}: {n} {ty}"
                );
            }
        }
    }
}

/// UD-047: element-node values are read completely and stored in Akantu
/// local node order; the oracle keeps the first node's values only.
#[test]
fn element_node_data_is_read_completely() {
    let fx = Fixture::open(&case_dir("data/element_node_data_3d")).unwrap();
    let (mesh, _) = read_case(&fx, &compat()).unwrap();
    let MeshDataValues::Real(field) = mesh.data().elemental("field").unwrap() else {
        panic!()
    };
    assert!(mesh.data().is_elemental_nodal("field"));
    // File (2 components per node): penta6 node j = (j+1, 10 (j+1)),
    // tet10 node j = (j+1, -(j+1)).
    for (ty, gmsh_code, f) in [
        (ElementType::Pentahedron6, "6", 10.0),
        (ElementType::Tetrahedron10, "11", -1.0),
    ] {
        let order = gmsh_read_order(gmsh_code).unwrap();
        let v = field.get(ty, GhostType::NotGhost).unwrap();
        assert_eq!(v.nb_component(), 2 * order.len());
        let mut want = vec![0.0; 2 * order.len()];
        for (j, &s) in order.iter().enumerate() {
            let x = j as f64 + 1.0;
            want[2 * s] = x;
            want[2 * s + 1] = f * x;
        }
        assert_eq!(v.as_slice(), &want[..], "{ty}");
        // The oracle: components 0..2 = first Gmsh node's values, rest 0.
        let o = fx
            .load_f64(&format!("mesh_data/field/{}", ty.name()))
            .unwrap()
            .data;
        assert_eq!(&o[..2], &[1.0, f]);
        assert!(o[2..].iter().all(|&x| x == 0.0), "{ty}: {o:?}");
    }
    // data_blocks: triangle values 1 2 3 / 4 5 6 (identity order).
    let fx = Fixture::open(&case_dir("data/data_blocks")).unwrap();
    let (mesh, _) = read_case(&fx, &compat()).unwrap();
    let MeshDataValues::Real(t) = mesh.data().elemental("nodal_temp").unwrap() else {
        panic!()
    };
    assert_eq!(
        t.get(ElementType::Triangle3, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    );
    let o = fx
        .load_f64("mesh_data/nodal_temp/_triangle_3")
        .unwrap()
        .data;
    assert_eq!(o, vec![1.0, 0.0, 0.0, 4.0, 0.0, 0.0]);
}

/// UD-046: the oracle cannot read MSH 1.0 (it scans to end of file); the
/// Rust reader reads it, with the same result as the oracle's read of the
/// equivalent MSH 2.2 file.
#[test]
fn msh1_reads_like_the_equivalent_msh2() {
    let fx = Fixture::open(&case_dir("msh1/msh1")).unwrap();
    assert!(expects_error(&fx));
    assert!(
        fx.manifest()
            .expect
            .message_contains
            .as_deref()
            .unwrap_or("")
            .contains("end of the file")
    );
    let (mesh, report) = read_case(&fx, &compat()).unwrap();
    assert_eq!(report.msh_version, Some(1000));
    common::compare_with_fixture(
        "msh1_reads_like_the_equivalent_msh2",
        &case_dir("msh1/msh1_equivalent_22"),
        &mesh,
    )
    .unwrap();
}

#[test]
fn binary_files_are_rejected_like_upstream() {
    let fx = Fixture::open(&case_dir("errors/binary_header")).unwrap();
    assert!(expects_error(&fx));
    let e = read_case(&fx, &compat()).map(|_| ()).unwrap_err();
    assert!(
        matches!(e, Error::Unsupported(ref m) if m.contains("binary")),
        "{e}"
    );
}

/// UD-090: the oracle misreads the element lines of an unsupported MSH 4
/// block as block headers and fails; the Rust reader skips the block.
#[test]
fn unsupported_msh4_blocks_are_skipped() {
    let fx = Fixture::open(&case_dir("msh4/pyramid_block")).unwrap();
    assert!(expects_error(&fx));
    let (mesh, report) = read_case(&fx, &compat()).unwrap();
    assert_eq!(
        mesh.element_types(TypeFilter::new()).collect::<Vec<_>>(),
        vec![ElementType::Tetrahedron4]
    );
    assert_eq!(
        mesh.connectivity(ElementType::Tetrahedron4, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        &[0, 1, 2, 5]
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Unsuported element kind 7")),
        "{:?}",
        report.warnings
    );
}

/// UD-045 fix: with the default options the `msh_block_<tag>` groups hold
/// the elements of the unnamed physical groups (independent expectation
/// derived from the file), everything else equals the oracle read.
#[test]
fn unnamed_physical_groups_are_filled() {
    let fx = Fixture::open(&case_dir("msh4/unnamed_physical")).unwrap();
    let (mesh, _) = read_case(&fx, &ReadOptions::default()).unwrap();
    let g = |n: &str| mesh.groups().element_group(n).unwrap();
    let (seg, tri) = (ElementType::Segment2, ElementType::Triangle3);
    assert_eq!(g("msh_block_3").elements_of(seg, GhostType::NotGhost), &[0]);
    assert_eq!(g("msh_block_3").nodes().as_slice(), &[0, 1]);
    assert_eq!(g("msh_block_6").elements_of(seg, GhostType::NotGhost), &[1]);
    assert_eq!(g("msh_block_6").dimension, 1);
    assert_eq!(
        g("msh_block_5").elements_of(tri, GhostType::NotGhost),
        &[0, 1]
    );
    assert_eq!(g("msh_block_5").nodes(), &[0, 1, 2, 3]);
    assert_eq!(g("named8").elements_of(tri, GhostType::NotGhost), &[0, 1]);
    assert_eq!(g("plate").elements_of(tri, GhostType::NotGhost), &[2, 3]);
    // Same as the oracle apart from the msh_block groups.
    let (compat_mesh, _) = read_case(&fx, &compat()).unwrap();
    for name in ["named8", "plate"] {
        assert_eq!(
            g(name).nodes(),
            compat_mesh.groups().element_group(name).unwrap().nodes()
        );
    }
    assert!(
        compat_mesh
            .groups()
            .element_group("msh_block_5")
            .unwrap()
            .nodes()
            .is_empty()
    );
}

#[test]
fn diana_material_file_matches_the_oracle() {
    for name in ["diana/diana_3d", "diana/diana_3d_auto"] {
        let fx = Fixture::open(&case_dir(name)).unwrap();
        let (_, report) = read_case(&fx, &compat()).unwrap();
        let mats: DianaMaterials = report.diana_materials.unwrap();
        let oracle =
            std::fs::read_to_string(fx.input_path("oracle_material_file").unwrap()).unwrap();
        assert_eq!(mats.to_input_file(), oracle, "{name}");
    }
}

/// UD-094 (D-003 A): Diana element-group members are stored by upstream
/// under an uninitialised ghost type, i.e. undefined behaviour: the aarch64
/// oracle runs stored them as ghost, x86_64 runs as not-ghost or ghost from
/// run to run (class X6). The Rust groups hold them as
/// not-ghost elements (the defined behaviour; independent expectation from
/// the file). The oracle evidence is checked only for its defined part —
/// which element types each group holds — whatever ghost type it shows.
#[test]
fn diana_groups_hold_not_ghost_elements() {
    let fx = Fixture::open(&case_dir("diana/diana_3d")).unwrap();
    let (mesh, _) = read_case(&fx, &compat()).unwrap();
    let block = mesh.groups().element_group("BLOCK").unwrap();
    let ng = GhostType::NotGhost;
    assert_eq!(block.elements_of(ElementType::Hexahedron8, ng), &[0]);
    assert_eq!(block.elements_of(ElementType::Hexahedron20, ng), &[0]);
    assert_eq!(block.elements_of(ElementType::Pentahedron6, ng), &[0]);
    assert!(
        block.nodes().is_empty(),
        "upstream adds no nodes to Diana element groups"
    );
    let other = mesh.groups().element_group("OTHER").unwrap();
    assert_eq!(other.elements_of(ElementType::Pentahedron15, ng), &[0]);
    assert_eq!(other.elements_of(ElementType::Tetrahedron4, ng), &[0]);
    assert_eq!(
        mesh.groups().node_group("FIX").unwrap().node_list(),
        vec![0, 1, 2, 3, 8]
    );
    let p = &fx.manifest().parameters;
    let oracle_types = |group: usize| -> Vec<String> {
        let mut types: Vec<String> = p["element_groups"][group]["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = t.as_str().unwrap();
                let (ty, ghost) = t.rsplit_once('/').unwrap();
                assert!(
                    ghost == "ghost" || ghost == "not_ghost",
                    "unexpected ghost type in {t}"
                );
                ty.to_string()
            })
            .collect();
        types.sort();
        types
    };
    assert_eq!(
        oracle_types(0),
        ["_hexahedron_20", "_hexahedron_8", "_pentahedron_6"],
        "oracle evidence for UD-094 changed: {}",
        p["element_groups"]
    );
    assert_eq!(
        oracle_types(1),
        ["_pentahedron_15", "_tetrahedron_4"],
        "oracle evidence for UD-094 changed: {}",
        p["element_groups"]
    );
}
