// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of the dumpers (no oracle): VTK structure and size consistency of the
// corrected layout, exact round trip of the base64 payloads, determinism,
// live data after a topology change, and robustness (bad inputs are
// errors, never panics).

mod dump_common;

use std::collections::BTreeMap;
use std::path::Path;

use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_io::{
    DerivedQuantity, DumpBuffer, Dumpable, Dumper, DumperLammps, DumperParaview, DumperText,
    FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType, LammpsAtomStyle,
    MeshFilter, MeshSelection, Padding, TextDumpMode,
};
use akantu_mesh::{Mesh, MeshAccessor};
use dump_common::{m2_mesh, scratch};

const NG: GhostType = GhostType::NotGhost;

/// One `DataArray` of a VTU file.
#[derive(Debug)]
struct Array {
    ty: String,
    components: usize,
    binary: bool,
    body: String,
}

/// Minimal reader for the VTU files written by the dumpers: piece sizes
/// and data arrays by section (`Points`, `Cells`, `PointData`,
/// `CellData`).
fn read_vtu(path: &Path) -> (usize, usize, BTreeMap<(String, String), Array>) {
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.starts_with(
        "<VTKFile type=\"UnstructuredGrid\" version=\"0.1\" byte_order=\"LittleEndian\">\n"
    ));
    assert!(text.ends_with("</VTKFile>\n"));
    let attr = |tag: &str, name: &str| -> Option<String> {
        let k = format!("{name}=");
        let i = tag.find(&k)? + k.len();
        let rest = tag[i..].trim_start();
        let rest = rest.strip_prefix('"')?;
        Some(rest[..rest.find('"')?].to_owned())
    };
    let piece = &text[text.find("<Piece").unwrap()..];
    let piece = &piece[..piece.find('>').unwrap()];
    let np: usize = attr(piece, "NumberOfPoints").unwrap().parse().unwrap();
    let nc: usize = attr(piece, "NumberOfCells").unwrap().parse().unwrap();
    let mut arrays = BTreeMap::new();
    let mut rest = text.as_str();
    let mut sec = String::new();
    while let Some(i) = rest.find('<') {
        let tag_end = rest[i..].find('>').unwrap() + i;
        let tag = &rest[i..=tag_end];
        if let Some(s) = ["<Points>", "<Cells>", "<PointData>", "<CellData>"]
            .iter()
            .find(|s| tag == **s)
        {
            sec = s.trim_matches(['<', '>']).to_owned();
        }
        if tag.starts_with("<DataArray ") {
            let body_end = rest[tag_end..].find("\n    </DataArray>").unwrap() + tag_end;
            let name = attr(tag, "Name").unwrap();
            arrays.insert(
                (sec.clone(), name),
                Array {
                    ty: attr(tag, "type").unwrap(),
                    components: attr(tag, "NumberOfComponents").map_or(1, |c| c.parse().unwrap()),
                    binary: attr(tag, "format").unwrap() == "binary",
                    body: rest[tag_end + 2..body_end].to_owned(),
                },
            );
            rest = &rest[body_end..];
        } else {
            rest = &rest[tag_end + 1..];
        }
    }
    (np, nc, arrays)
}

/// Standard base64 decoding of IOHelper's single stream; returns the
/// header value and the payload bytes.
fn decode(b64: &str) -> (u32, Vec<u8>) {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("not base64: {c}"),
        }
    };
    let mut bytes = Vec::new();
    let s = b64.trim().as_bytes();
    assert_eq!(s.len() % 4, 0, "base64 length");
    for g in s.chunks(4) {
        let pad = g.iter().filter(|&&c| c == b'=').count();
        let n = g.iter().take(4 - pad).fold(0u32, |a, &c| (a << 6) | val(c)) << (6 * pad);
        let b = n.to_be_bytes();
        bytes.extend_from_slice(&b[1..4 - pad]);
    }
    let header = u32::from_le_bytes(bytes[..4].try_into().unwrap());
    (header, bytes[4..].to_vec())
}

fn width(ty: &str) -> usize {
    match ty {
        "Float64" => 8,
        "Int32" | "UInt32" => 4,
        "UInt8" => 1,
        t => panic!("type {t}"),
    }
}

/// Number of values of an array and its payload.
fn values(a: &Array) -> (usize, Vec<u8>) {
    if a.binary {
        let (header, payload) = decode(&a.body);
        assert_eq!(
            header as usize,
            payload.len() + 4,
            "IOHelper header counts itself"
        );
        assert_eq!(payload.len() % width(&a.ty), 0);
        (payload.len() / width(&a.ty), payload)
    } else {
        (a.body.split_whitespace().count(), Vec::new())
    }
}

/// Check the size consistency VTK readers rely on.
fn check_vtu(path: &Path) -> (usize, usize, BTreeMap<(String, String), Array>) {
    let (np, nc, arrays) = read_vtu(path);
    let get = |s: &str, n: &str| {
        arrays
            .get(&(s.to_owned(), n.to_owned()))
            .unwrap_or_else(|| panic!("{n}"))
    };
    assert_eq!(values(get("Points", "positions")).0, 3 * np, "{path:?}");
    assert_eq!(values(get("Cells", "offsets")).0, nc, "{path:?}");
    assert_eq!(values(get("Cells", "types")).0, nc, "{path:?}");
    let conn = values(get("Cells", "connectivity")).0;
    let offsets = get("Cells", "offsets");
    let last = if nc == 0 {
        0
    } else if offsets.binary {
        let (_, p) = decode(&offsets.body);
        u32::from_le_bytes(p[p.len() - 4..].try_into().unwrap()) as usize
    } else {
        offsets
            .body
            .split_whitespace()
            .last()
            .unwrap()
            .parse()
            .unwrap()
    };
    assert_eq!(conn, last, "{path:?}: connectivity length = last offset");
    for ((sec, name), a) in &arrays {
        let n = match sec.as_str() {
            "PointData" => np,
            "CellData" => nc,
            _ => continue,
        };
        assert_eq!(values(a).0, n * a.components, "{path:?}: {name}");
    }
    (np, nc, arrays)
}

/// Resolver with simple analytic fields on any mesh.
struct Analytic {
    dim: usize,
}

impl FieldResolver for Analytic {
    fn resolve(
        &self,
        mesh: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        g: GhostType,
    ) -> Result<DumpBuffer> {
        let x = mesh.nodes();
        let x = x.as_slice();
        Ok(match (f.name.as_str(), ty) {
            ("u", None) => DumpBuffer::Real {
                nb_component: self.dim,
                values: x
                    .iter()
                    .enumerate()
                    .map(|(i, v)| v * 1.5 - i as f64 / 7.0)
                    .collect(),
            },
            ("flag", None) => DumpBuffer::Bool {
                nb_component: 1,
                values: (0..mesh.nb_nodes()).map(|i| i % 3 == 0).collect(),
            },
            ("id", Some(t)) => DumpBuffer::Int {
                nb_component: 1,
                values: (0..mesh.nb_element(t, g))
                    .map(|e| i32::try_from(e).unwrap() * 10 + t as i32)
                    .collect(),
            },
            ("grad_u" | "stress", Some(t)) => {
                let n = mesh.nb_element(t, g) * 3; // 3 points per element
                let d2 = self.dim * self.dim;
                DumpBuffer::Real {
                    nb_component: d2,
                    values: (0..n * d2)
                        .map(|k| ((k * 37 % 101) as f64 - 50.0) * 1e-3)
                        .collect(),
                }
            }
            _ => match (&f.source, ty) {
                (FieldSource::Internal { name, .. }, Some(t))
                    if name == "grad_u" || name == "stress" =>
                {
                    let n = mesh.nb_element(t, g) * 3;
                    let d2 = self.dim * self.dim;
                    DumpBuffer::Real {
                        nb_component: d2,
                        values: (0..n * d2)
                            .map(|k| ((k * 37 % 101) as f64 - 50.0) * 1e-3)
                            .collect(),
                    }
                }
                _ => return Err(Error::not_found("field", f.name.clone())),
            },
        })
    }

    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        g: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        let n = mesh.nb_element(ty, g);
        Ok(match name {
            "nu" => vec![0.3; n],
            _ => (0..n).map(|e| (e % 2) as Real).collect(),
        })
    }
}

fn fd(
    name: &str,
    source: FieldSource,
    support: FieldSupport,
    vt: FieldValueType,
) -> FieldDescriptor {
    FieldDescriptor {
        name: name.into(),
        source,
        support,
        value_type: vt,
        group: None,
        padding: Padding::None,
        homogenize: false,
    }
}

fn analytic_fields(d: &mut dyn Dumper) {
    let real = FieldValueType::Real;
    d.register_field(FieldDescriptor {
        padding: Padding::To3D,
        ..fd(
            "u",
            FieldSource::ModelNodal("u".into()),
            FieldSupport::Nodal,
            real,
        )
    })
    .unwrap();
    d.register_field(fd(
        "flag",
        FieldSource::ModelNodal("flag".into()),
        FieldSupport::Nodal,
        FieldValueType::Bool,
    ))
    .unwrap();
    d.register_field(fd(
        "id",
        FieldSource::ModelElemental("id".into()),
        FieldSupport::Elemental,
        FieldValueType::Int,
    ))
    .unwrap();
    d.register_field(fd(
        "rank",
        FieldSource::Partition,
        FieldSupport::Elemental,
        FieldValueType::Int,
    ))
    .unwrap();
    for (name, q) in [
        ("strain", DerivedQuantity::SmallStrain),
        ("Green strain", DerivedQuantity::GreenStrain),
        ("principal strain", DerivedQuantity::PrincipalSmallStrain),
        (
            "principal Green strain",
            DerivedQuantity::PrincipalGreenStrain,
        ),
        ("Von Mises stress", DerivedQuantity::VonMisesStress),
    ] {
        d.register_field(FieldDescriptor {
            homogenize: true,
            padding: Padding::To3D,
            ..fd(
                name,
                FieldSource::Derived {
                    quantity: q,
                    law: None,
                },
                FieldSupport::QuadraturePoint,
                real,
            )
        })
        .unwrap();
    }
    d.register_field(FieldDescriptor {
        homogenize: true,
        padding: Padding::To3D,
        ..fd(
            "stress",
            FieldSource::Internal {
                law: None,
                name: "stress".into(),
            },
            FieldSupport::QuadraturePoint,
            real,
        )
    })
    .unwrap();
}

/// Corrected layout: every array has exactly NumberOfComponents values
/// per point/cell (including the derived matrix fields upstream writes as
/// squares), in base64 and ASCII, and the nodal payload round-trips
/// exactly.
#[test]
fn vtu_sizes_are_consistent_and_payloads_round_trip() {
    let mesh = m2_mesh(true);
    let r = Analytic { dim: 2 };
    let dir = scratch("props_vtu");
    for (name, text) in [("b", false), ("a", true)] {
        let mut d = DumperParaview::new(name, dir.to_str().unwrap(), false);
        if text {
            d.set_text_mode().unwrap();
        }
        d.register_mesh(2, NG, ElementKind::Regular).unwrap();
        analytic_fields(&mut d);
        d.dump(&mesh, &r, None, None).unwrap();
        let (np, nc, arrays) = check_vtu(&dir.join(format!("{name}-VTU/{name}_0000.vtu")));
        assert_eq!((np, nc), (8, 5));
        let comps: BTreeMap<&str, usize> = arrays
            .iter()
            .filter(|((s, _), _)| s == "CellData")
            .map(|((_, n), a)| (n.as_str(), a.components))
            .collect();
        assert_eq!(comps["strain"], 9, "padded 2-D strain");
        assert_eq!(comps["Green strain"], 9);
        assert_eq!(comps["stress"], 9);
        assert_eq!(comps["principal strain"], 2);
        assert_eq!(comps["Von Mises stress"], 1);
        if !text {
            // exact round trip of the padded displacement
            let u = &arrays[&("PointData".to_owned(), "u".to_owned())];
            let (_, payload) = decode(&u.body);
            let got: Vec<f64> = payload
                .chunks(8)
                .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                .collect();
            let DumpBuffer::Real { values, .. } = r
                .resolve(
                    &mesh,
                    &fd(
                        "u",
                        FieldSource::ModelNodal("u".into()),
                        FieldSupport::Nodal,
                        FieldValueType::Real,
                    ),
                    None,
                    NG,
                )
                .unwrap()
            else {
                unreachable!()
            };
            let want: Vec<f64> = values.chunks(2).flat_map(|t| [t[0], t[1], 0.0]).collect();
            assert_eq!(
                got.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                want.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            );
        }
    }
}

/// Same field set in 3-D on every element type upstream can dump plus the
/// UD-053 additions, base64: sizes consistent, offsets = cumulative node
/// counts.
#[test]
fn every_element_type_gives_a_consistent_vtu() {
    let mut types: Vec<ElementType> = ElementType::ALL
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                ElementType::IgfemSegment3
                    | ElementType::IgfemTriangle4
                    | ElementType::IgfemTriangle5
            )
        })
        .collect();
    types.sort_by_key(|t| *t as usize);
    let mut mesh = Mesh::new(3, "all").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for n in 0..20 {
            let x = f64::from(n);
            a.push_node(&[x, x * x, -x]).unwrap();
        }
        for (t, &ty) in types.iter().enumerate() {
            let conn: Vec<Idx> = (0..ty.nb_nodes_per_element())
                .map(|k| Idx::try_from((k * 7 + t) % 20).unwrap())
                .collect();
            a.connectivity_mut(ty, NG).push_tuple(&conn);
            a.connectivity_mut(ty, NG).push_tuple(&conn);
        }
    }
    let dir = scratch("props_types");
    let mut d = DumperParaview::new("t", dir.to_str().unwrap(), false);
    d.register_mesh_selection(MeshSelection::new(None, NG, ElementKind::NotDefined))
        .unwrap();
    d.register_field(fd(
        "id",
        FieldSource::ModelElemental("id".into()),
        FieldSupport::Elemental,
        FieldValueType::Int,
    ))
    .unwrap();
    d.dump(&mesh, &Analytic { dim: 3 }, None, None).unwrap();
    let (_, nc, _) = check_vtu(&dir.join("t-VTU/t_0000.vtu"));
    assert_eq!(nc, 2 * types.len());

    // IGFEM types need the IGFEM extension: an error, not a panic.
    MeshAccessor::new(&mut mesh)
        .connectivity_mut(ElementType::IgfemTriangle4, NG)
        .push_tuple(&[0, 1, 2, 3]);
    let err = d.dump(&mesh, &Analytic { dim: 3 }, None, None).unwrap_err();
    assert!(matches!(err, Error::UnsupportedElementType { .. }), "{err}");
}

/// Two dumps of the same state write identical bytes; a dump after mesh
/// growth sees the new nodes and elements (descriptor contract).
#[test]
fn dumps_are_deterministic_and_follow_topology_changes() {
    let mut mesh = m2_mesh(false);
    let r = Analytic { dim: 2 };
    let dir = scratch("props_det");
    let mut d = DumperParaview::new("d", dir.to_str().unwrap(), false);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    analytic_fields(&mut d);
    d.dump(&mesh, &r, None, Some(0)).unwrap();
    let first = std::fs::read(dir.join("d-VTU/d_0000.vtu")).unwrap();
    d.dump(&mesh, &r, None, Some(0)).unwrap();
    assert_eq!(first, std::fs::read(dir.join("d-VTU/d_0000.vtu")).unwrap());
    {
        let mut a = MeshAccessor::new(&mut mesh);
        let n = a.push_node(&[3.0, 3.0]).unwrap();
        a.connectivity_mut(ElementType::Triangle3, NG)
            .push_tuple(&[5, n, 6]);
    }
    d.dump(&mesh, &r, None, Some(1)).unwrap();
    let (np, nc, _) = check_vtu(&dir.join("d-VTU/d_0001.vtu"));
    assert_eq!((np, nc), (9, 6));
}

/// Resolver returning malformed buffers.
struct Bad(u8);

impl FieldResolver for Bad {
    fn resolve(
        &self,
        mesh: &Mesh,
        _: &FieldDescriptor,
        ty: Option<ElementType>,
        g: GhostType,
    ) -> Result<DumpBuffer> {
        let n = ty.map_or(mesh.nb_nodes(), |t| mesh.nb_element(t, g));
        Ok(match self.0 {
            0 => DumpBuffer::Real {
                nb_component: 2,
                values: vec![1.0; n * 2 + 1],
            },
            1 => DumpBuffer::Int {
                nb_component: 1,
                values: vec![1; n],
            },
            2 => DumpBuffer::Real {
                nb_component: 0,
                values: vec![1.0; 3],
            },
            3 => DumpBuffer::Real {
                nb_component: 3,
                values: vec![f64::NAN; n * 3],
            },
            _ => return Err(Error::User("resolver failure".into())),
        })
    }
}

/// Bad inputs are errors (never panics), a failed dump changes nothing.
#[test]
fn bad_inputs_are_errors() {
    let mesh = m2_mesh(true);
    let dir = scratch("props_bad");
    let real_nodal = fd(
        "x",
        FieldSource::ModelNodal("x".into()),
        FieldSupport::Nodal,
        FieldValueType::Real,
    );
    let real_el = fd(
        "e",
        FieldSource::ModelElemental("e".into()),
        FieldSupport::Elemental,
        FieldValueType::Real,
    );
    let qp = FieldDescriptor {
        homogenize: true,
        ..fd(
            "q",
            FieldSource::Internal {
                law: None,
                name: "q".into(),
            },
            FieldSupport::QuadraturePoint,
            FieldValueType::Real,
        )
    };
    for (k, f) in [
        (0, &real_nodal),
        (1, &real_nodal),
        (2, &real_el),
        (0, &real_el),
        (2, &qp),
        (4, &qp),
    ] {
        let mut d = DumperParaview::new("bad", dir.to_str().unwrap(), false);
        d.register_mesh(2, NG, ElementKind::Regular).unwrap();
        d.register_field(f.clone()).unwrap();
        assert!(
            d.dump(&mesh, &Bad(k), None, None).is_err(),
            "case {k} {}",
            f.name
        );
        assert_eq!(d.count(), 0);
    }
    // non-finite values are data, written as upstream would
    let mut d = DumperParaview::new("nan", dir.to_str().unwrap(), false);
    d.set_text_mode().unwrap();
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.register_field(fd(
        "n",
        FieldSource::ModelNodal("n".into()),
        FieldSupport::Nodal,
        FieldValueType::Real,
    ))
    .unwrap();
    d.dump(&mesh, &Bad(3), None, None).unwrap();
    let t = std::fs::read_to_string(dir.join("nan-VTU/nan_0000.vtu")).unwrap();
    assert!(t.contains("                   nan "), "{t}");

    // registration errors
    let mut d = DumperParaview::new("reg", dir.to_str().unwrap(), false);
    d.register_field(real_nodal.clone()).unwrap();
    assert!(matches!(
        d.register_field(real_nodal.clone()),
        Err(Error::AlreadyExists { .. })
    ));
    assert!(
        d.dump(&mesh, &Bad(4), None, None).is_err(),
        "no mesh registered"
    );
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    assert!(
        d.register_mesh(2, NG, ElementKind::Regular).is_err(),
        "mesh twice"
    );
    assert!(d.unregister_field("nope").is_err());

    // filters referring to missing groups or out-of-range entities
    let mut els = ElementTypeMapArray::<Idx>::new("bad");
    els.alloc(1, 1, ElementType::Triangle3, NG, 99).unwrap();
    for filter in [
        MeshFilter::ElementGroup("missing".into()),
        MeshFilter::NodeGroup("missing".into()),
        MeshFilter::Explicit {
            elements: Box::new(ElementTypeMapArray::new("e")),
            nodes: vec![-1],
        },
        MeshFilter::Explicit {
            elements: Box::new(els.clone()),
            nodes: vec![0, 1],
        },
        MeshFilter::Explicit {
            elements: Box::new(ElementTypeMapArray::new("e")),
            nodes: vec![8],
        },
    ] {
        let mut d = DumperParaview::new("filter", dir.to_str().unwrap(), false);
        d.register_mesh_selection(
            MeshSelection::new(None, NG, ElementKind::NotDefined).filtered(filter.clone()),
        )
        .unwrap();
        assert!(d.dump(&mesh, &Bad(4), None, None).is_err(), "{filter:?}");
    }
    // element whose nodes are not in the node filter
    let mut tri = ElementTypeMapArray::<Idx>::new("t");
    tri.alloc(1, 1, ElementType::Triangle3, NG, 0).unwrap();
    let mut d = DumperParaview::new("filter2", dir.to_str().unwrap(), false);
    d.register_mesh_selection(
        MeshSelection::new(None, NG, ElementKind::NotDefined).filtered(MeshFilter::Explicit {
            elements: Box::new(tri),
            nodes: vec![3, 4],
        }),
    )
    .unwrap();
    assert!(matches!(
        d.dump(&mesh, &Bad(4), None, None),
        Err(Error::Mesh(_))
    ));

    // derived fields need dim² components and real values; padders need
    // law parameters
    let mut d = DumperParaview::new("derived", dir.to_str().unwrap(), false);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.register_field(FieldDescriptor {
        homogenize: true,
        ..fd(
            "strain",
            FieldSource::Derived {
                quantity: DerivedQuantity::SmallStrain,
                law: None,
            },
            FieldSupport::QuadraturePoint,
            FieldValueType::Real,
        )
    })
    .unwrap();
    assert!(d.dump(&mesh, &Bad(0), None, None).is_err());
    let mut d = DumperParaview::new("pad", dir.to_str().unwrap(), false);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.register_field(FieldDescriptor {
        homogenize: true,
        padding: Padding::To3D,
        ..fd(
            "stress",
            FieldSource::Internal {
                law: None,
                name: "stress".into(),
            },
            FieldSupport::QuadraturePoint,
            FieldValueType::Real,
        )
    })
    .unwrap();
    struct NoParams;
    impl FieldResolver for NoParams {
        fn resolve(
            &self,
            m: &Mesh,
            _: &FieldDescriptor,
            ty: Option<ElementType>,
            g: GhostType,
        ) -> Result<DumpBuffer> {
            Ok(DumpBuffer::Real {
                nb_component: 4,
                values: vec![1.0; 4 * ty.map_or(0, |t| m.nb_element(t, g))],
            })
        }
    }
    assert!(matches!(
        d.dump(&mesh, &NoParams, None, None),
        Err(Error::Unsupported(_))
    ));

    // text, LAMMPS and the registry
    let mut t = DumperText::new("t", TextDumpMode::Space, false);
    t.set_directory(&dir).unwrap();
    t.register_variable("v").unwrap();
    assert!(
        t.dump(&mesh, &Bad(4), None, None).is_err(),
        "unknown variable"
    );
    let mut l = DumperLammps::new("l", LammpsAtomStyle::Atomic, None);
    l.set_directory(&dir).unwrap();
    assert!(l.dump_points(&[1.0, 2.0, 3.0], 2).is_err());
    assert!(l.dump(&mesh, &Bad(4), None, None).is_err(), "no positions");
    let mut reg = Dumpable::new();
    assert!(
        reg.dump(&mesh, &Bad(4), None, None).is_err(),
        "no default dumper"
    );
    assert!(reg.dumper("x").is_err());
    reg.register_text("t", None, true);
    assert!(reg.dumper_as::<DumperParaview>("t").is_err());
    assert!(reg.dumper_as::<DumperText>("t").is_ok());
}

/// Seeded random registrations/dumps on random small meshes: every
/// outcome is `Ok` or `Err`, and successful ParaView files are consistent.
#[test]
fn random_inputs_never_panic() {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let dir = scratch("props_random");
    for case in 0..200 {
        let dim = 1 + usize::try_from(next(3)).unwrap();
        let mut mesh = Mesh::new(dim, "r").unwrap();
        let nn = usize::try_from(next(6)).unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for i in 0..nn {
                let x: Vec<f64> = (0..dim).map(|d| (i * 3 + d) as f64 * 0.25).collect();
                a.push_node(&x).unwrap();
            }
            let types = [
                ElementType::Segment2,
                ElementType::Triangle3,
                ElementType::Quadrangle4,
                ElementType::Tetrahedron4,
            ];
            for _ in 0..next(4) {
                let ty = types[usize::try_from(next(4)).unwrap()];
                let conn: Vec<Idx> = (0..ty.nb_nodes_per_element())
                    .map(|_| Idx::try_from(next(nn as u64 + 2)).unwrap() - 1)
                    .collect();
                a.connectivity_mut(ty, NG).push_tuple(&conn);
            }
        }
        let mut d = DumperParaview::new(&format!("r{case}"), dir.to_str().unwrap(), false);
        if next(2) == 0 {
            d.set_text_mode().unwrap();
        }
        let _ = d.register_mesh_selection(MeshSelection::new(
            (next(2) == 0).then_some(dim),
            NG,
            ElementKind::NotDefined,
        ));
        let r = Analytic { dim };
        if next(2) == 0 {
            analytic_fields(&mut d);
        }
        if d.dump(&mesh, &r, None, None).is_ok() {
            check_vtu(&dir.join(format!("r{case}-VTU/r{case}_0000.vtu")));
        }
    }
}
