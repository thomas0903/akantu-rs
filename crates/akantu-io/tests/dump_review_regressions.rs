// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the dumpers: each test builds a failing case (R1: descriptors, XML,
// arithmetic; R2: recovery after a failed output directory) and checks the
// corrected behaviour. Run in debug and release builds (overflow checks
// differ).

mod dump_common;

use std::path::{Path, PathBuf};

use akantu_core::{ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Result};
use akantu_io::{
    DumpBuffer, Dumpable, Dumper, DumperLammps, DumperParaview, DumperText, FieldDescriptor,
    FieldResolver, FieldSource, FieldSupport, FieldValueType, LammpsAtomStyle,
    MAX_EXPLICIT_PARALLEL_SIZE, MAX_TEXT_PRECISION, MeshFilter, MeshSelection, Padding,
    ParallelContext, TextDumpMode, pad_to_3d, solid_mechanics_field,
};
use akantu_mesh::{Mesh, MeshAccessor};
use dump_common::{XmlElement, files_below, scratch, xml_elements};

const NG: GhostType = GhostType::NotGhost;
const TRI: ElementType = ElementType::Triangle3;

/// R1 mesh: nodes (0,0) (1,0) (0,1) (1,1), triangles [0,1,2] and [1,3,2];
/// group `one` = triangle 0 with nodes 0,1,2; group `both` = both
/// triangles and all nodes, listed in reverse order.
fn r1_mesh() -> Mesh {
    let mut m = Mesh::new(2, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        let mut t = a.connectivity_mut(TRI, NG);
        t.push_tuple(&[0, 1, 2]);
        t.push_tuple(&[1, 3, 2]);
    }
    let mut g = m.groups_mut().create_element_group("one", 2).unwrap();
    g.elements.alloc(1, 1, TRI, NG, 0).unwrap();
    g.node_group.set_nodes(vec![0, 1, 2]);
    let mut g = m.groups_mut().create_element_group("both", 2).unwrap();
    g.elements
        .alloc(2, 1, TRI, NG, 0)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&[1, 0]);
    g.node_group.set_nodes(vec![3, 2, 1, 0]);
    m
}

/// R1 resolver: `c` components for every node / element of the block;
/// `c = usize::MAX` returns an empty buffer. Values identify the entity:
/// `100 + node`, `10 + element` (component offsets in tenths).
struct R1 {
    c: usize,
}

impl FieldResolver for R1 {
    fn resolve(
        &self,
        mesh: &Mesh,
        _: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        if self.c == usize::MAX {
            return Ok(DumpBuffer::Real {
                nb_component: self.c,
                values: Vec::new(),
            });
        }
        let (n, base) = match ty {
            None => (mesh.nb_nodes(), 100.0),
            Some(t) => (mesh.nb_element(t, ghost), 10.0),
        };
        let mut values = Vec::with_capacity(n * self.c);
        for i in 0..n {
            for k in 0..self.c {
                #[allow(clippy::cast_precision_loss)] // tiny exact integers
                values.push(base + i as f64 + k as f64 / 10.0);
            }
        }
        Ok(DumpBuffer::Real {
            nb_component: self.c,
            values,
        })
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        if name == "energy" {
            Ok(DumpBuffer::Real {
                nb_component: 1,
                values: vec![1.5],
            })
        } else {
            Err(Error::not_found("dump variable", name))
        }
    }
}

/// A resolver that must never be asked for a variable.
struct NoVariables;

impl FieldResolver for NoVariables {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        _: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        Err(Error::not_found("dump field", f.name.clone()))
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        panic!("variable '{name}' evaluated by a writer that ignores variables")
    }
}

fn field(name: &str, support: FieldSupport, group: Option<&str>) -> FieldDescriptor {
    let source = match support {
        FieldSupport::Nodal => FieldSource::ModelNodal("x".into()),
        FieldSupport::Elemental => FieldSource::ModelElemental("x".into()),
        FieldSupport::QuadraturePoint => FieldSource::Internal {
            law: None,
            name: "x".into(),
        },
    };
    FieldDescriptor {
        name: name.into(),
        source,
        support,
        value_type: FieldValueType::Real,
        group: group.map(str::to_owned),
        padding: Padding::None,
        homogenize: support == FieldSupport::QuadraturePoint,
    }
}

fn paraview(name: &str, dir: &Path, selection: Option<MeshSelection>) -> DumperParaview {
    let mut d = DumperParaview::new(name, dir.to_str().unwrap(), false);
    d.set_text_mode().unwrap();
    match selection {
        Some(s) => d.register_mesh_selection(s).unwrap(),
        None => d.register_mesh(2, NG, ElementKind::Regular).unwrap(),
    }
    d
}

fn read_xml(path: &Path) -> Vec<XmlElement> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    xml_elements(&text).unwrap_or_else(|e| panic!("{path:?} is not well-formed XML: {e}"))
}

/// ASCII values of the `DataArray` named `name`.
fn ascii_values(doc: &[XmlElement], name: &str) -> Vec<f64> {
    let a = doc
        .iter()
        .find(|e| e.name == "DataArray" && e.attr("Name") == Some(name))
        .unwrap_or_else(|| panic!("no DataArray {name}"));
    a.text
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect()
}

fn group_selection(name: &str) -> MeshSelection {
    MeshSelection::new(None, NG, ElementKind::NotDefined)
        .filtered(MeshFilter::ElementGroup(name.into()))
}

fn explicit_selection(elements: &[Idx], nodes: Vec<Idx>) -> MeshSelection {
    let mut els = ElementTypeMapArray::<Idx>::new("filter");
    els.alloc(elements.len(), 1, TRI, NG, 0)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(elements);
    MeshSelection::new(None, NG, ElementKind::NotDefined).filtered(MeshFilter::Explicit {
        elements: Box::new(els),
        nodes,
    })
}

// ------------------------------------------------------------------

/// R1 row 1: a one-triangle group field on the two-triangle mesh used to
/// return `Ok` with one value for two cells. It is now rejected before
/// anything is written, for elemental and nodal support (node 3 is not
/// in group `one`), on the ParaView and the text writer.
#[test]
fn dmp01_field_group_smaller_than_written_mesh_is_rejected() {
    let mesh = r1_mesh();
    let dir = scratch("dmp01_reject");
    for support in [
        FieldSupport::Elemental,
        FieldSupport::QuadraturePoint,
        FieldSupport::Nodal,
    ] {
        let out = dir.join(format!("{support:?}"));
        let mut d = paraview("r1", &out, None);
        d.register_field(field("x", support, Some("one"))).unwrap();
        let e = d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap_err();
        assert!(
            matches!(&e, Error::InvalidArgument(m) if m.contains("group 'one'")),
            "{support:?}: {e}"
        );
        assert_eq!(d.count(), 0);
        assert!(files_below(&out).is_empty(), "{support:?} wrote files");

        let mut t = DumperText::new("t", TextDumpMode::Space, false);
        t.set_directory(&out).unwrap();
        t.register_mesh(2, NG, ElementKind::Regular).unwrap();
        t.register_field(field("x", support, Some("one"))).unwrap();
        assert!(t.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
        assert!(files_below(&out).is_empty(), "text {support:?} wrote files");
    }
    // the group's mesh part is not covered by a group of other elements
    let mut d = paraview("r1x", &dir, Some(explicit_selection(&[1], vec![1, 3, 2])));
    d.register_field(field("x", FieldSupport::Elemental, Some("one")))
        .unwrap();
    assert!(d.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
}

/// Equal-sized groups listed in a different order than the written mesh
/// part: values are taken by node/element identity in the written order,
/// so the group field is identical to the unrestricted field.
#[test]
fn dmp01_equal_sized_reordered_groups_map_by_identity() {
    let mesh = r1_mesh();
    let dir = scratch("dmp01_identity");
    let fields = [
        field("e_all", FieldSupport::Elemental, None),
        field("e_both", FieldSupport::Elemental, Some("both")),
        field("q_both", FieldSupport::QuadraturePoint, Some("both")),
        field("n_all", FieldSupport::Nodal, None),
        field("n_both", FieldSupport::Nodal, Some("both")),
    ];
    // full mesh (cells 0,1 and nodes 0..4) vs group `both` (cells 1,0 and
    // nodes 3,2,1,0) and an explicit filter in mesh order
    let cases = [
        (None, vec![10.0, 11.0], vec![100.0, 101.0, 102.0, 103.0]),
        (
            Some(group_selection("both")),
            vec![11.0, 10.0],
            vec![103.0, 102.0, 101.0, 100.0],
        ),
        (
            Some(explicit_selection(&[0, 1], vec![0, 1, 2, 3])),
            vec![10.0, 11.0],
            vec![100.0, 101.0, 102.0, 103.0],
        ),
    ];
    for (k, (selection, cells, nodes)) in cases.into_iter().enumerate() {
        let out = dir.join(format!("case{k}"));
        let mut d = paraview("id", &out, selection.clone());
        for f in &fields {
            d.register_field(f.clone()).unwrap();
        }
        d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
        let doc = read_xml(&out.join("id-VTU/id_0000.vtu"));
        for name in ["e_all", "e_both", "q_both"] {
            assert_eq!(ascii_values(&doc, name), cells, "case {k} {name}");
        }
        for name in ["n_all", "n_both"] {
            assert_eq!(ascii_values(&doc, name), nodes, "case {k} {name}");
        }
        // text writer: same rows as `position`
        let mut t = DumperText::new("t", TextDumpMode::Space, false);
        t.set_directory(&out).unwrap();
        match selection {
            Some(s) => t.register_mesh_selection(s).unwrap(),
            None => t.register_mesh(2, NG, ElementKind::Regular).unwrap(),
        }
        for f in &fields {
            t.register_field(f.clone()).unwrap();
        }
        t.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
        let rows = |name: &str| -> Vec<f64> {
            std::fs::read_to_string(out.join(format!("t-DataFiles/t_{name}.out")))
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect()
        };
        assert_eq!(rows("e_both"), cells, "text case {k}");
        assert_eq!(rows("n_both"), nodes, "text case {k}");
        assert_eq!(rows("n_both"), rows("n_all"), "text case {k}");
    }
}

// ------------------------------------------------------------------

/// R1 row 2: a nodal field named `a&b` produced unreadable XML. Reserved
/// characters and attribute whitespace in field names and base names are
/// now escaped: every file parses and the names and referenced paths
/// round-trip.
#[test]
fn dmp02_reserved_characters_are_escaped() {
    let mesh = r1_mesh();
    let dir = scratch("dmp02_escape");
    let names = [
        "a&b",
        "<tag>",
        "q\"uote",
        "apos'",
        "tab\there",
        "new\nline",
        "cr\rx",
        "&amp;",
        "unicodé 漢",
    ];
    let base = "r&d<\"x\">";
    let mut d = paraview(base, &dir, None);
    d.set_time_step(0.5).unwrap();
    for (k, n) in names.iter().enumerate() {
        let support = if k % 2 == 0 {
            FieldSupport::Nodal
        } else {
            FieldSupport::Elemental
        };
        d.register_field(field(n, support, None)).unwrap();
    }
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();

    let vtu = read_xml(&dir.join(format!("{base}-VTU/{base}_0001.vtu")));
    let pvtu = read_xml(&dir.join(format!("{base}_0001.pvtu")));
    for n in names {
        assert!(
            vtu.iter()
                .any(|e| e.name == "DataArray" && e.attr("Name") == Some(n)),
            "VTU name {n:?}"
        );
        assert!(
            pvtu.iter()
                .any(|e| e.name == "PDataArray" && e.attr("Name") == Some(n)),
            "PVTU name {n:?}"
        );
    }
    let sources: Vec<&str> = pvtu
        .iter()
        .filter(|e| e.name == "Piece")
        .filter_map(|e| e.attr("Source"))
        .collect();
    assert_eq!(sources, [format!("{base}-VTU/{base}_0001.vtu")]);
    assert!(dir.join(sources[0]).is_file());
    let pvd = read_xml(&dir.join(format!("{base}.pvd")));
    let files: Vec<&str> = pvd
        .iter()
        .filter(|e| e.name == "DataSet")
        .filter_map(|e| e.attr("file"))
        .collect();
    assert_eq!(
        files,
        [format!("{base}_0000.pvtu"), format!("{base}_0001.pvtu")]
    );
    for f in files {
        assert!(dir.join(f).is_file(), "{f}");
    }
    // ordinary names are written unchanged
    let raw = std::fs::read_to_string(dir.join(format!("{base}-VTU/{base}_0001.vtu"))).unwrap();
    assert!(raw.contains("Name=\"a&amp;b\""), "{raw}");
    assert!(raw.contains("Name=\"positions\""), "{raw}");
}

/// Characters an XML document cannot hold are an error, before any file
/// is written and without advancing the dump count.
#[test]
fn dmp02_unrepresentable_names_are_errors() {
    let mesh = r1_mesh();
    let dir = scratch("dmp02_reject");
    for bad in ["nul\u{0}", "bell\u{7}", "esc\u{1b}", "nonchar\u{fffe}"] {
        let mut d = paraview("ok", &dir, None);
        d.register_field(field(bad, FieldSupport::Nodal, None))
            .unwrap();
        let e = d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap_err();
        assert!(matches!(e, Error::InvalidArgument(_)), "{bad:?}: {e}");
        assert_eq!(d.count(), 0);
        // as a base name (piece and collection references)
        let mut d = paraview(bad, &dir, None);
        d.set_time_step(1.0).unwrap();
        assert!(d.dump(&mesh, &R1 { c: 1 }, None, None).is_err(), "{bad:?}");
        assert_eq!(d.count(), 0);
    }
    assert!(files_below(&dir).is_empty());
}

// ------------------------------------------------------------------

/// R1 rows 3–5: boundary-size descriptors, resolver shapes and steps used
/// to panic (overflow), the step one after its files were written. All are
/// now errors, reported before any output and without state changes.
#[test]
fn dmp03_boundary_shapes_and_steps_are_errors() {
    let mesh = r1_mesh();
    let dir = scratch("dmp03_bounds");
    let components = |first: usize, count: usize| FieldDescriptor {
        source: FieldSource::Components {
            source: Box::new(FieldSource::ModelNodal("x".into())),
            first,
            count,
        },
        ..field("x", FieldSupport::Nodal, None)
    };
    let cases: Vec<(&str, FieldDescriptor, usize)> = vec![
        ("first=MAX", components(usize::MAX, 2), 1),
        ("first+count overflow", components(1, usize::MAX), 2),
        ("count=0", components(0, 0), 1),
        (
            "nodal c=MAX",
            field("x", FieldSupport::Nodal, None),
            usize::MAX,
        ),
        ("nodal c=0", field("x", FieldSupport::Nodal, None), 0),
        (
            "elemental c=MAX",
            field("x", FieldSupport::Elemental, None),
            usize::MAX,
        ),
        (
            "elemental c=0",
            field("x", FieldSupport::Elemental, None),
            0,
        ),
        (
            "quadrature c=MAX",
            field("x", FieldSupport::QuadraturePoint, None),
            usize::MAX,
        ),
        (
            "quadrature c=0",
            field("x", FieldSupport::QuadraturePoint, None),
            0,
        ),
    ];
    for (what, f, c) in cases {
        let mut d = paraview("b", &dir, None);
        d.register_field(f.clone()).unwrap();
        assert!(d.dump(&mesh, &R1 { c }, None, None).is_err(), "{what}");
        assert_eq!(d.count(), 0, "{what}");
        let mut t = DumperText::new("t", TextDumpMode::Space, false);
        t.set_directory(&dir).unwrap();
        t.register_field(f).unwrap();
        assert!(t.dump(&mesh, &R1 { c }, None, None).is_err(), "text {what}");
    }
    assert!(files_below(&dir).is_empty());

    // step = usize::MAX: the counter cannot advance past it
    let mut d = paraview("s", &dir, None);
    assert!(d.dump(&mesh, &R1 { c: 1 }, None, Some(usize::MAX)).is_err());
    assert_eq!(d.count(), 0);
    let mut t = DumperText::new("st", TextDumpMode::Space, false);
    t.set_directory(&dir).unwrap();
    t.register_mesh(2, NG, ElementKind::Regular).unwrap();
    assert!(t.dump(&mesh, &R1 { c: 1 }, None, Some(usize::MAX)).is_err());
    let mut l = DumperLammps::new("sl", LammpsAtomStyle::Atomic, None);
    l.set_directory(&dir).unwrap();
    l.register_mesh(2, NG, ElementKind::Regular).unwrap();
    assert!(l.dump(&mesh, &R1 { c: 1 }, None, Some(usize::MAX)).is_err());
    assert!(files_below(&dir).is_empty(), "{:?}", files_below(&dir));
    // the last representable step still works, the next one is an error
    d.dump(&mesh, &R1 { c: 1 }, None, Some(usize::MAX - 1))
        .unwrap();
    assert_eq!(d.count(), usize::MAX);
    let before = files_below(&dir);
    assert!(d.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
    assert_eq!(d.count(), usize::MAX);
    assert_eq!(files_below(&dir), before);

    // explicit parallel contexts out of range
    for ctx in [
        ParallelContext::Explicit {
            rank: 2,
            size: 2,
            root: 0,
        },
        ParallelContext::Explicit {
            rank: usize::MAX,
            size: 2,
            root: 0,
        },
        ParallelContext::Explicit {
            rank: 0,
            size: 2,
            root: 5,
        },
        ParallelContext::Explicit {
            rank: 0,
            size: usize::MAX,
            root: 0,
        },
    ] {
        let out = dir.join("ctx");
        let mut d = paraview("c", &out, None);
        Dumper::set_parallel_context(&mut d, ctx).unwrap();
        assert!(d.dump(&mesh, &R1 { c: 1 }, None, None).is_err(), "{ctx:?}");
        assert!(files_below(&out).is_empty(), "{ctx:?}");
    }

    // text precision beyond the formatter's range; public padding helper
    let mut t = DumperText::new("p", TextDumpMode::Space, false);
    assert!(t.set_precision(usize::MAX).is_err());
    assert!(t.set_precision(MAX_TEXT_PRECISION + 1).is_err());
    t.set_precision(MAX_TEXT_PRECISION).unwrap();
    assert_eq!(pad_to_3d(&[], 0, usize::MAX), (0, vec![]));
    assert_eq!(pad_to_3d(&[1.0], 1, usize::MAX), (1, vec![1.0]));
}

// ------------------------------------------------------------------

fn blocked_dir(root: &Path) -> PathBuf {
    let blocked = root.join("blocked");
    std::fs::write(&blocked, b"a regular file").unwrap();
    blocked
}

fn one_node_mesh() -> Mesh {
    let mut m = Mesh::new(2, "one").unwrap();
    MeshAccessor::new(&mut m).push_node(&[0.5, 0.25]).unwrap();
    m
}

fn pvd_entries(path: &Path) -> Vec<(String, String)> {
    read_xml(path)
        .iter()
        .filter(|e| e.name == "DataSet")
        .map(|e| {
            (
                e.attr("timestep").unwrap().to_owned(),
                e.attr("file").unwrap().to_owned(),
            )
        })
        .collect()
}

fn entries(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

/// R2 (ParaView): the first dump into a regular file fails; after fixing
/// the directory the retry used to list step 0000 twice (times 0 and
/// 0.25). Now the failed dump commits nothing.
#[test]
fn dmp04_paraview_retry_after_failed_directory() {
    let mesh = one_node_mesh();
    let root = scratch("dmp04_paraview");
    let blocked = blocked_dir(&root);
    let mut d = DumperParaview::new("retry", blocked.to_str().unwrap(), false);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.set_time_step(0.25).unwrap();
    assert!(matches!(
        d.dump(&mesh, &R1 { c: 1 }, None, None),
        Err(Error::Io { .. })
    ));
    assert_eq!(d.count(), 0);
    let out = root.join("out");
    d.set_directory(&out).unwrap();
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    let pvd = out.join("retry.pvd");
    assert_eq!(pvd_entries(&pvd), entries(&[("0", "retry_0000.pvtu")]));
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert_eq!(
        pvd_entries(&pvd),
        entries(&[("0", "retry_0000.pvtu"), ("0.25", "retry_0001.pvtu")])
    );
}

/// Write failures after the directories exist: a piece that cannot be
/// written, then a collection file that cannot be replaced. Each failed
/// dump keeps the count, the clock and the published `.pvd`; the retry
/// writes a correct history.
#[test]
fn dmp04_paraview_failed_writes_keep_history() {
    let mesh = one_node_mesh();
    let out = scratch("dmp04_paraview_writes");
    let mut d = DumperParaview::new("w", out.to_str().unwrap(), false);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.set_time_step(0.25).unwrap();
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    let pvd = out.join("w.pvd");
    let published = std::fs::read(&pvd).unwrap();

    // the piece of step 1 cannot be written
    let piece = out.join("w-VTU/w_0001.vtu");
    std::fs::create_dir(&piece).unwrap();
    assert!(matches!(
        d.dump(&mesh, &R1 { c: 1 }, None, None),
        Err(Error::Io { .. })
    ));
    assert_eq!(d.count(), 1);
    assert_eq!(std::fs::read(&pvd).unwrap(), published);
    assert!(
        !out.join("w_0001.pvtu").exists(),
        "metadata written before its piece"
    );
    std::fs::remove_dir(&piece).unwrap();

    // the collection file cannot be replaced
    std::fs::remove_file(&pvd).unwrap();
    std::fs::create_dir(&pvd).unwrap();
    assert!(d.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
    assert_eq!(d.count(), 1);
    std::fs::remove_dir(&pvd).unwrap();

    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert_eq!(
        pvd_entries(&pvd),
        entries(&[("0", "w_0000.pvtu"), ("0.25", "w_0001.pvtu")])
    );
    assert!(
        files_below(&out).iter().all(|f| !f.contains("akantu-tmp")),
        "{:?}",
        files_below(&out)
    );
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// R2 (text): the failed first dump used to remember the variable and
/// time paths below the blocked directory, so the retry failed there.
/// Now nothing is remembered and the retry writes below the new
/// directory.
#[test]
fn dmp04_text_retry_after_failed_directory() {
    let mesh = one_node_mesh();
    let root = scratch("dmp04_text");
    let blocked = blocked_dir(&root);
    let mut t = DumperText::new("retry", TextDumpMode::Space, false);
    t.set_directory(&blocked).unwrap();
    t.register_variable("energy").unwrap();
    t.set_time_step(0.25).unwrap();
    assert!(matches!(
        t.dump(&mesh, &R1 { c: 1 }, None, None),
        Err(Error::Io { .. })
    ));
    let out = root.join("out");
    t.set_directory(&out).unwrap();
    t.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    t.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert_eq!(
        read(&out.join("retry-DataFiles/retry_energy.out")),
        "1.500000e+00\n1.500000e+00\n"
    );
    assert_eq!(
        read(&out.join("retry.time")),
        "0 0.000000e+00\n1 2.500000e-01\n"
    );
    assert_eq!(read(&blocked), "a regular file");
}

/// A write failure in the middle of a later dump (the `.time` file cannot
/// be appended to): the field and variable lines already appended by that
/// dump are rolled back, so the retry does not duplicate them.
#[test]
fn dmp04_text_failed_append_is_rolled_back() {
    let mesh = one_node_mesh();
    let out = scratch("dmp04_text_append");
    let mut t = DumperText::new("a", TextDumpMode::Space, false);
    t.set_directory(&out).unwrap();
    t.register_mesh(2, NG, ElementKind::Regular).unwrap();
    t.register_field(field("x", FieldSupport::Nodal, None))
        .unwrap();
    t.register_variable("energy").unwrap();
    t.set_time_step(0.25).unwrap();
    t.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    let data = [
        out.join("a-DataFiles/a_position.out"),
        out.join("a-DataFiles/a_x.out"),
        out.join("a-DataFiles/a_energy.out"),
    ];
    let before: Vec<String> = data.iter().map(|p| read(p)).collect();
    let time = out.join("a.time");
    let saved = read(&time);
    std::fs::remove_file(&time).unwrap();
    std::fs::create_dir(&time).unwrap();
    assert!(t.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
    let after: Vec<String> = data.iter().map(|p| read(p)).collect();
    assert_eq!(after, before, "appended lines were not rolled back");
    std::fs::remove_dir(&time).unwrap();
    std::fs::write(&time, saved).unwrap();
    t.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert_eq!(read(&data[1]), "1.000000e+02\n\n1.000000e+02\n\n");
    assert_eq!(read(&data[2]), "1.500000e+00\n1.500000e+00\n");
    assert_eq!(read(&time), "0 0.000000e+00\n1 2.500000e-01\n");
}

/// LAMMPS: a failed dump keeps the count and the stream state (the first
/// successful dump still formats the bounds with `%g`).
#[test]
fn dmp04_lammps_failed_dump_keeps_stream_state() {
    let mesh = one_node_mesh();
    let root = scratch("dmp04_lammps");
    let blocked = blocked_dir(&root);
    let mut l = DumperLammps::new(
        "l",
        LammpsAtomStyle::Atomic,
        Some([0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
    );
    l.set_directory(&blocked).unwrap();
    l.register_mesh(2, NG, ElementKind::Regular).unwrap();
    assert!(l.dump(&mesh, &R1 { c: 1 }, None, None).is_err());
    let out = root.join("out");
    l.set_directory(&out).unwrap();
    l.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert!(read(&out.join("l")).contains("\n0 1 xlo xhi\n"));
    l.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    assert!(
        read(&out.join("l")).contains("\n0.0000000000000000e+00 1.0000000000000000e+00 xlo xhi\n")
    );
}

// ------------------------------------------------------------------

/// R1 row 6: upstream's `"all"` group sentinel gave `NotFound` for element
/// group `all`; it now gives the unrestricted descriptor.
#[test]
fn dmp05_all_group_sentinel_is_the_whole_mesh() {
    let all = solid_mechanics_field("displacement", Some("all"), false, |_| false).unwrap();
    let none = solid_mechanics_field("displacement", None, false, |_| false).unwrap();
    assert_eq!(all, none);
    let internal = |n: &str| n == "stress";
    assert_eq!(
        solid_mechanics_field("stress", Some("all"), true, internal).unwrap(),
        solid_mechanics_field("stress", None, true, internal).unwrap()
    );
    assert_eq!(
        solid_mechanics_field("displacement", Some("one"), false, |_| false)
            .unwrap()
            .group
            .as_deref(),
        Some("one")
    );
    struct Disp;
    impl FieldResolver for Disp {
        fn resolve(
            &self,
            mesh: &Mesh,
            _: &FieldDescriptor,
            _: Option<ElementType>,
            _: GhostType,
        ) -> Result<DumpBuffer> {
            Ok(DumpBuffer::Real {
                nb_component: 2,
                values: vec![2.0; 2 * mesh.nb_nodes()],
            })
        }
    }
    let dir = scratch("dmp05_all");
    let mut d = paraview("all", &dir, None);
    d.register_field(all).unwrap();
    d.dump(&r1_mesh(), &Disp, None, None).unwrap();
}

// ------------------------------------------------------------------

/// R1 row 7: variable registration was `Unsupported` on ParaView and
/// LAMMPS. Both now accept and ignore variables as upstream (never
/// evaluated), with duplicate and unregister errors.
#[test]
fn dmp06_paraview_and_lammps_accept_and_ignore_variables() {
    let mesh = r1_mesh();
    let dir = scratch("dmp06_variables");
    let mut reg = Dumpable::new();
    reg.register_paraview("pv", None, true);
    reg.set_directory(&dir).unwrap();
    reg.add_dump_mesh(MeshSelection::new(Some(2), NG, ElementKind::Regular))
        .unwrap();
    reg.register_dumper(
        "lmp",
        Box::new(DumperLammps::new("lmp", LammpsAtomStyle::Atomic, None)),
        false,
    );
    reg.set_directory_to_dumper("lmp", &dir).unwrap();
    reg.add_dump_mesh_to_dumper("lmp", MeshSelection::new(Some(2), NG, ElementKind::Regular))
        .unwrap();
    for d in ["pv", "lmp"] {
        reg.add_dump_variable_to_dumper(d, "energy").unwrap();
        assert!(matches!(
            reg.add_dump_variable_to_dumper(d, "energy"),
            Err(Error::AlreadyExists { .. })
        ));
        // names are shared with fields (documented deviation)
        assert!(matches!(
            reg.add_dump_field_to_dumper(d, field("energy", FieldSupport::Nodal, None)),
            Err(Error::AlreadyExists { .. })
        ));
        reg.dump_named(d, &mesh, &NoVariables, None, None).unwrap();
        reg.remove_dump_variable_from_dumper(d, "energy").unwrap();
        assert!(matches!(
            reg.remove_dump_variable_from_dumper(d, "energy"),
            Err(Error::NotFound { .. })
        ));
        // a variable is not a field
        reg.add_dump_variable_to_dumper(d, "energy").unwrap();
        assert!(reg.remove_dump_field_from_dumper(d, "energy").is_err());
    }
    assert!(dir.join("pv_0000.pvtu").is_file());
    assert!(dir.join("lmp").is_file());
}

// -----------------------------------------------------------------

/// Resolver returning `nodal` components per node, `elemental` per
/// element of a non-empty block and `empty` for an empty element block.
struct Comps {
    nodal: usize,
    elemental: usize,
    empty: usize,
}

impl FieldResolver for Comps {
    fn resolve(
        &self,
        mesh: &Mesh,
        _: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let (n, c) = match ty {
            None => (mesh.nb_nodes(), self.nodal),
            Some(t) if mesh.nb_element(t, ghost) == 0 => (0, self.empty),
            Some(t) => (mesh.nb_element(t, ghost), self.elemental),
        };
        // `n × c` values: an empty block is consistent with any `c`; the
        // test never combines entities with an out-of-range `c`
        assert!(n == 0 || c <= 3, "test resolver misuse: {n} × {c}");
        Ok(DumpBuffer::Real {
            nb_component: c,
            values: vec![1.0; n * c],
        })
    }
}

/// On an empty mesh or an empty element block the
/// length check holds for any component count, so a resolver's `nc = 0`
/// was written without `NumberOfComponents` and `nc = usize::MAX` as
/// `NumberOfComponents="18446744073709551615"` (`nb_components …` in the
/// text `.fields`). Every block now needs `1 ≤ nc ≤ i32::MAX` (VTK
/// `int`); the dump fails before anything is written and changes nothing.
/// (b): an explicit parallel context is bounded by
/// `MAX_EXPLICIT_PARALLEL_SIZE` (the root renders one piece reference per
/// rank in memory).
#[test]
fn dmp2_03_component_counts_and_explicit_size_are_bounded() {
    // 0 nodes and an empty Triangle3 block
    let mut empty = Mesh::new(2, "empty").unwrap();
    {
        let mut a = MeshAccessor::new(&mut empty);
        let _ = a.connectivity_mut(TRI, NG);
    }
    assert_eq!(empty.nb_nodes(), 0);
    assert_eq!(empty.nb_element(TRI, NG), 0);
    // R1 mesh with an extra empty Quadrangle4 block
    let mut mixed = r1_mesh();
    {
        let mut a = MeshAccessor::new(&mut mixed);
        let _ = a.connectivity_mut(ElementType::Quadrangle4, NG);
    }
    let dir = scratch("dmp2_03_bounds");
    let too_many = usize::try_from(i32::MAX).unwrap() + 1;
    for bad in [0, too_many, usize::MAX] {
        let empty_block = |support| {
            (
                support,
                Comps {
                    nodal: 1,
                    elemental: 1,
                    empty: bad,
                },
            )
        };
        let cases: [(&str, &Mesh, (FieldSupport, Comps)); 5] = [
            (
                "nodal, 0 nodes",
                &empty,
                (
                    FieldSupport::Nodal,
                    Comps {
                        nodal: bad,
                        elemental: 1,
                        empty: 1,
                    },
                ),
            ),
            (
                "elemental, empty mesh block",
                &empty,
                empty_block(FieldSupport::Elemental),
            ),
            (
                "quadrature, empty mesh block",
                &empty,
                empty_block(FieldSupport::QuadraturePoint),
            ),
            // Triangle3 block valid (1 component), empty Quadrangle4 block
            // with `bad`
            (
                "elemental, empty block of a non-empty mesh",
                &mixed,
                empty_block(FieldSupport::Elemental),
            ),
            (
                "quadrature, empty block of a non-empty mesh",
                &mixed,
                empty_block(FieldSupport::QuadraturePoint),
            ),
        ];
        for (what, mesh, (support, r)) in cases {
            let what = format!("{what}, nc = {bad}");
            let out = dir.join("pv");
            let mut d = paraview("b", &out, None);
            d.register_field(field("x", support, None)).unwrap();
            let e = d.dump(mesh, &r, None, None);
            assert!(
                matches!(
                    e,
                    Err(Error::InvalidArgument(_) | Error::DimensionMismatch { .. })
                ),
                "{what}: {e:?}"
            );
            assert_eq!(d.count(), 0, "{what}");
            let mut t = DumperText::new("t", TextDumpMode::Space, false);
            t.set_directory(&out).unwrap();
            t.register_mesh(2, NG, ElementKind::Regular).unwrap();
            t.register_field(field("x", support, None)).unwrap();
            let e = t.dump(mesh, &r, None, None);
            assert!(
                matches!(
                    e,
                    Err(Error::InvalidArgument(_) | Error::DimensionMismatch { .. })
                ),
                "text {what}: {e:?}"
            );
            assert!(
                files_below(&out).is_empty(),
                "{what}: {:?}",
                files_below(&out)
            );
        }
    }
    // the same empty meshes with valid counts still dump
    let out = dir.join("ok");
    let mut d = paraview("ok", &out, None);
    d.register_field(field("n", FieldSupport::Nodal, None))
        .unwrap();
    d.register_field(field("e", FieldSupport::Elemental, None))
        .unwrap();
    let r = Comps {
        nodal: 3,
        elemental: 2,
        empty: 2,
    };
    d.dump(&empty, &r, None, None).unwrap();
    let doc = read_xml(&out.join("ok-VTU/ok_0000.vtu"));
    let comps = |name: &str| {
        doc.iter()
            .find(|e| e.name == "DataArray" && e.attr("Name") == Some(name))
            .and_then(|e| e.attr("NumberOfComponents"))
            .map(str::to_owned)
    };
    assert_eq!(comps("n").as_deref(), Some("3"));
    assert_eq!(comps("e").as_deref(), Some("2"));
    d.dump(&mixed, &r, None, None).unwrap();
    assert_eq!(d.count(), 2);

    // (b) explicit size: the bound itself on a non-root rank (writes only
    // its own piece, no piece list), one more is refused on every rank
    let out = dir.join("ctx");
    let mesh = r1_mesh();
    let mut d = paraview("c", &out, None);
    Dumper::set_parallel_context(
        &mut d,
        ParallelContext::Explicit {
            rank: 1,
            size: MAX_EXPLICIT_PARALLEL_SIZE,
            root: 0,
        },
    )
    .unwrap();
    d.dump(&mesh, &R1 { c: 1 }, None, None).unwrap();
    let written = files_below(&out);
    assert_eq!(written.len(), 1, "{written:?}");
    let before = written;
    for (rank, root) in [(0, 0), (1, 0)] {
        let ctx = ParallelContext::Explicit {
            rank,
            size: MAX_EXPLICIT_PARALLEL_SIZE + 1,
            root,
        };
        let mut d = paraview("c", &out, None);
        Dumper::set_parallel_context(&mut d, ctx).unwrap();
        assert!(
            matches!(
                d.dump(&mesh, &R1 { c: 1 }, None, None),
                Err(Error::InvalidArgument(_))
            ),
            "{ctx:?}"
        );
        assert_eq!(d.count(), 0, "{ctx:?}");
        assert_eq!(files_below(&out), before, "{ctx:?}");
    }
}
