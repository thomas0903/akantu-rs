// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests
// of the dumpers against the oracle output in fixtures/dump/<case>/
// (written by oracle/probes/dump/dump_probe.cc, see
// oracle/probes/dump/README.md): every case of the probe is rebuilt with
// the same mesh, fields, registration order and dump calls, and the files
// are compared byte for byte.

mod dump_common;

use std::path::Path;

use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, NodeFlag, Real, Result,
};
use akantu_io::{
    DumpBuffer, Dumper, DumperLammps, DumperParaview, DumperText, FieldDescriptor, FieldResolver,
    FieldSource, FieldSupport, FieldValueType, LammpsAtomStyle, MeshFilter, MeshSelection, Padding,
    ParallelContext, TextDumpMode,
};
use akantu_mesh::{Mesh, MeshAccessor};
use dump_common::{Inputs, assert_same_file, assert_same_tree, fixtures, key, m2_mesh, scratch};

const NG: GhostType = GhostType::NotGhost;

fn fd(
    name: &str,
    source: FieldSource,
    support: FieldSupport,
    value_type: FieldValueType,
) -> FieldDescriptor {
    FieldDescriptor {
        name: name.into(),
        source,
        support,
        value_type,
        group: None,
        padding: Padding::None,
        homogenize: false,
    }
}

fn model(name: &str) -> FieldSource {
    FieldSource::ModelNodal(name.into())
}

fn model_el(name: &str) -> FieldSource {
    FieldSource::ModelElemental(name.into())
}

/// Resolver serving the probe's M2 arrays of the current stage.
struct M2 {
    inputs: Inputs,
    stage: usize,
    scalar: Real,
    iscalar: i32,
    vec: [Real; 2],
}

impl M2 {
    fn new(case: &str) -> Self {
        Self {
            inputs: Inputs::load(case),
            stage: 0,
            scalar: 1.234_556_789_1,
            iscalar: -3,
            vec: [1.0, -2.0 / 3.0],
        }
    }
}

impl FieldResolver for M2 {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        let name = match &f.source {
            FieldSource::ModelNodal(n) | FieldSource::ModelElemental(n) => n.clone(),
            FieldSource::Internal { name, .. } => name.clone(),
            _ => return Err(Error::not_found("source", f.name.clone())),
        };
        let s = self.stage;
        let i = &self.inputs;
        Ok(match ty {
            None => {
                let k = format!("{name}:{s}");
                let nb_component = i.shape(&k).1;
                match name.as_str() {
                    "disp" | "velo" => DumpBuffer::Real {
                        nb_component,
                        values: i.reals(&k),
                    },
                    "blocked" => DumpBuffer::Bool {
                        nb_component,
                        values: i.bools(&k),
                    },
                    "nint" => DumpBuffer::Int {
                        nb_component,
                        values: i.ints(&k),
                    },
                    "nuint" => DumpBuffer::UInt {
                        nb_component,
                        values: i.uints(&k),
                    },
                    _ => return Err(Error::not_found("nodal input", k)),
                }
            }
            Some(t) => {
                let k = key(&format!("{name}:{s}"), t);
                let nb_component = i.shape(&k).1;
                match name.as_str() {
                    "ereal" | "qp" => DumpBuffer::Real {
                        nb_component,
                        values: i.reals(&k),
                    },
                    _ => DumpBuffer::Int {
                        nb_component,
                        values: i.ints(&k),
                    },
                }
            }
        })
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        Ok(match name {
            "scalar" => DumpBuffer::Real {
                nb_component: 1,
                values: vec![self.scalar],
            },
            "iscalar" => DumpBuffer::Int {
                nb_component: 1,
                values: vec![self.iscalar],
            },
            "vec" => DumpBuffer::Real {
                nb_component: 2,
                values: self.vec.to_vec(),
            },
            _ => return Err(Error::not_found("variable", name)),
        })
    }
}

fn m2_fields(d: &mut dyn Dumper) {
    let real = FieldValueType::Real;
    let int = FieldValueType::Int;
    let fields = [
        FieldDescriptor {
            padding: Padding::To3D,
            ..fd("disp", model("disp"), FieldSupport::Nodal, real)
        },
        fd("velo", model("velo"), FieldSupport::Nodal, real),
        fd(
            "blocked",
            model("blocked"),
            FieldSupport::Nodal,
            FieldValueType::Bool,
        ),
        fd("nint", model("nint"), FieldSupport::Nodal, int),
        fd(
            "nuint",
            model("nuint"),
            FieldSupport::Nodal,
            FieldValueType::UInt,
        ),
        fd(
            "disp_y",
            FieldSource::Components {
                source: Box::new(model("disp")),
                first: 1,
                count: 1,
            },
            FieldSupport::Nodal,
            real,
        ),
        fd("ereal", model_el("ereal"), FieldSupport::Elemental, real),
        fd("eint", model_el("eint"), FieldSupport::Elemental, int),
        fd(
            "material_index",
            model_el("mat"),
            FieldSupport::Elemental,
            int,
        ),
        fd(
            "partitions",
            FieldSource::Partition,
            FieldSupport::Elemental,
            int,
        ),
        FieldDescriptor {
            homogenize: true,
            ..fd(
                "qp",
                FieldSource::Internal {
                    law: None,
                    name: "qp".into(),
                },
                FieldSupport::QuadraturePoint,
                real,
            )
        },
        fd(
            "tag_0",
            FieldSource::Mesh("tag_0".into()),
            FieldSupport::QuadraturePoint,
            int,
        ),
    ];
    for f in fields {
        d.register_field(f).unwrap();
    }
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Port of probe case `m2`: every field kind, base64 + ASCII, time
/// description, `dump()` / `dump(step)` / `dump(time, step)`.
#[test]
fn m2_matches_oracle() {
    let dir = scratch("m2");
    let pv = dir.join("paraview");
    let mut mesh = m2_mesh(true);
    dump_common::add_tag_data(&mut mesh);
    let mut r = M2::new("m2");

    let mut b64 = DumperParaview::new("m2", s(&pv), false);
    b64.register_mesh(2, NG, ElementKind::Regular).unwrap();
    m2_fields(&mut b64);
    b64.set_time_step(0.1).unwrap();
    let mut asc = DumperParaview::new("m2_ascii", s(&pv), false);
    asc.set_text_mode().unwrap();
    asc.register_mesh(2, NG, ElementKind::Regular).unwrap();
    m2_fields(&mut asc);
    asc.set_time_step(0.1).unwrap();
    let mut all = DumperParaview::new("m2_all", s(&pv), false);
    all.register_mesh_selection(MeshSelection::new(None, NG, ElementKind::NotDefined))
        .unwrap();
    all.register_field(fd(
        "velo",
        model("velo"),
        FieldSupport::Nodal,
        FieldValueType::Real,
    ))
    .unwrap();

    b64.dump(&mesh, &r, None, None).unwrap();
    asc.dump(&mesh, &r, None, None).unwrap();
    all.dump(&mesh, &r, None, None).unwrap();
    r.stage = 1;
    b64.dump(&mesh, &r, None, Some(5)).unwrap();
    asc.dump(&mesh, &r, None, Some(5)).unwrap();
    r.stage = 2;
    b64.dump(&mesh, &r, Some(0.35), Some(7)).unwrap();
    asc.dump(&mesh, &r, Some(0.35), Some(7)).unwrap();
    b64.dump(&mesh, &r, None, None).unwrap();
    asc.dump(&mesh, &r, None, None).unwrap();
    assert_eq!(b64.count(), 9);
    assert_same_tree(&pv, &fixtures().join("m2/paraview"));
}

/// Probe case `m2_raw_qp`: a non-homogenised quadrature field over types
/// with 3 and 4 points. Upstream writes 9 values per triangle and 12 per
/// quadrangle under `NumberOfComponents="9"`; akantu-rs refuses such a
/// field unless the upstream layout is requested.
#[test]
fn m2_raw_quadrature_field() {
    let dir = scratch("m2_raw_qp");
    let pv = dir.join("paraview");
    let mesh = m2_mesh(true);
    let r = M2::new("m2_raw_qp");
    let qp = fd(
        "qp_raw",
        FieldSource::Internal {
            law: None,
            name: "qp".into(),
        },
        FieldSupport::QuadraturePoint,
        FieldValueType::Real,
    );
    for (name, text) in [("raw", false), ("raw_ascii", true)] {
        let mut d = DumperParaview::new(name, s(&pv), false);
        if text {
            d.set_text_mode().unwrap();
        }
        d.register_mesh(2, NG, ElementKind::Regular).unwrap();
        d.register_field(qp.clone()).unwrap();
        let err = d.dump(&mesh, &r, None, None).unwrap_err();
        assert!(matches!(err, Error::DimensionMismatch { .. }), "{err}");
        assert_eq!(d.count(), 0, "a failed dump does not advance the count");
        d.set_compat_layout(true);
        d.dump(&mesh, &r, None, None).unwrap();
    }
    assert_same_tree(&pv, &fixtures().join("m2_raw_qp/paraview"));
}

const DUMPABLE_TYPES: [ElementType; 21] = [
    ElementType::Point1,
    ElementType::Segment2,
    ElementType::Segment3,
    ElementType::Triangle3,
    ElementType::Triangle6,
    ElementType::Quadrangle4,
    ElementType::Quadrangle8,
    ElementType::Tetrahedron4,
    ElementType::Tetrahedron10,
    ElementType::Pentahedron6,
    ElementType::Pentahedron15,
    ElementType::Hexahedron8,
    ElementType::Hexahedron20,
    ElementType::Cohesive1d2,
    ElementType::Cohesive2d4,
    ElementType::Cohesive2d6,
    ElementType::Cohesive3d12,
    ElementType::Cohesive3d6,
    ElementType::Cohesive3d8,
    ElementType::BernoulliBeam2,
    ElementType::BernoulliBeam3,
];

/// `build_types_mesh` of the probe: one element per type, node `k` of the
/// element of the `t`-th type is `(3k + t) mod 20`.
fn types_mesh(types: &[ElementType]) -> Mesh {
    let mut m = Mesh::new(3, "types").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for n in 0..20 {
            let x = f64::from(n);
            a.push_node(&[x, 0.5 * x * x, -1.0 / (x + 1.0)]).unwrap();
        }
        for (t, &ty) in types.iter().enumerate() {
            let conn: Vec<Idx> = (0..ty.nb_nodes_per_element())
                .map(|k| Idx::try_from((k * 3 + t) % 20).unwrap())
                .collect();
            a.connectivity_mut(ty, NG).push_tuple(&conn);
        }
    }
    m
}

/// Probe case `all_types`: VTK cell types and node reorders of every type
/// upstream can dump.
#[test]
fn all_element_types_match_oracle() {
    let dir = scratch("all_types");
    let pv = dir.join("paraview");
    let mesh = types_mesh(&DUMPABLE_TYPES);
    let r = M2::new("m2");
    for (name, text) in [("types", false), ("types_ascii", true)] {
        let mut d = DumperParaview::new(name, s(&pv), false);
        if text {
            d.set_text_mode().unwrap();
        }
        d.register_mesh_selection(MeshSelection::new(None, NG, ElementKind::NotDefined))
            .unwrap();
        d.dump(&mesh, &r, None, None).unwrap();
    }
    assert_same_tree(&pv, &fixtures().join("all_types/paraview"));
}

/// UD-053: upstream throws `getIOHelperType : not implemented yet !` for
/// `_cohesive_3d_16` and `_discrete_kirchhoff_triangle_18`
/// (`fixtures/dump/{coh3d16,dkt18}/status.txt`); akantu-rs dumps them as
/// the corner hexahedron and the 3-node triangle.
#[test]
fn ud053_cohesive_3d_16_and_dkt18_are_dumped() {
    for c in ["coh3d16", "dkt18"] {
        let status = std::fs::read_to_string(fixtures().join(c).join("status.txt")).unwrap();
        assert!(
            status.contains("exception: getIOHelperType : not implemented yet !"),
            "{status}"
        );
    }
    let dir = scratch("ud053");
    for (ty, vtk, conn) in [
        (ElementType::Cohesive3d16, "5 12", "1 4 7 10 5 8 11 14"),
        (ElementType::DiscreteKirchhoffTriangle18, "5 5", "1 4 7"),
    ] {
        let mesh = types_mesh(&[ElementType::Triangle3, ty]);
        let mut d = DumperParaview::new("ud053", s(&dir), false);
        d.set_text_mode().unwrap();
        d.register_mesh_selection(MeshSelection::new(None, NG, ElementKind::NotDefined))
            .unwrap();
        d.dump(&mesh, &M2::new("m2"), None, None).unwrap();
        let vtu = std::fs::read_to_string(dir.join("ud053-VTU/ud053_0000.vtu")).unwrap();
        // connectivity: the triangle (0 3 6) then the new cell; the
        // corners of a cohesive_3d_16 are its nodes 0-3 and 8-11.
        assert!(vtu.contains(&format!("0 3 6 {conn} \n")), "{vtu}");
        assert!(vtu.contains(&format!("\n{vtk} \n")), "{vtu}");
    }
}

/// Probe case `groups`: element-group dumper (filtered mesh, filtered
/// nodal/elemental/quadrature fields, base64 and ASCII) and node-group
/// dumper (point set).
#[test]
fn group_dumpers_match_oracle() {
    let dir = scratch("groups");
    let pv = dir.join("paraview");
    let mut mesh = m2_mesh(true);
    let mut r = M2::new("groups");
    {
        let i = &r.inputs;
        let mut g = mesh.groups_mut().create_element_group("grp", 2).unwrap();
        for (ty, k) in [
            (ElementType::Triangle3, "grp_tri3"),
            (ElementType::Quadrangle4, "grp_quad4"),
        ] {
            let ids = i.ints(k);
            let mut a = g.elements.alloc(ids.len(), 1, ty, NG, 0).unwrap();
            a.as_mut_slice().copy_from_slice(&ids);
        }
        g.node_group.set_nodes(i.ints("grp_nodes"));
        assert_eq!(g.node_group.node_list(), vec![1, 2, 3, 4, 5, 6, 7]);
        let ng = mesh.groups_mut().create_node_group("ngrp").unwrap();
        ng.set_nodes(r.inputs.ints("ngrp_nodes"));
    }
    let grp_sel = MeshSelection::new(None, NG, ElementKind::NotDefined)
        .filtered(MeshFilter::ElementGroup("grp".into()));
    let g = |f: FieldDescriptor| FieldDescriptor {
        group: Some("grp".into()),
        ..f
    };
    let real = FieldValueType::Real;
    let mut grp = DumperParaview::with_name("grp");
    grp.set_directory(&pv).unwrap();
    grp.register_mesh_selection(grp_sel.clone()).unwrap();
    for f in [
        g(FieldDescriptor {
            padding: Padding::To3D,
            ..fd("disp", model("disp"), FieldSupport::Nodal, real)
        }),
        g(fd(
            "blocked",
            model("blocked"),
            FieldSupport::Nodal,
            FieldValueType::Bool,
        )),
        g(fd(
            "disp_y",
            FieldSource::Components {
                source: Box::new(model("disp")),
                first: 1,
                count: 1,
            },
            FieldSupport::Nodal,
            real,
        )),
        g(fd(
            "ereal",
            model_el("ereal"),
            FieldSupport::Elemental,
            real,
        )),
        g(fd(
            "eint",
            model_el("eint"),
            FieldSupport::Elemental,
            FieldValueType::Int,
        )),
        g(FieldDescriptor {
            homogenize: true,
            ..fd(
                "qp",
                FieldSource::Internal {
                    law: None,
                    name: "qp".into(),
                },
                FieldSupport::QuadraturePoint,
                real,
            )
        }),
    ] {
        grp.register_field(f).unwrap();
    }
    grp.dump(&mesh, &r, None, None).unwrap();
    r.stage = 1;
    grp.dump(&mesh, &r, None, None).unwrap();

    let mut grp_ascii = DumperParaview::with_name("grp_ascii");
    grp_ascii.set_directory(&pv).unwrap();
    grp_ascii.register_mesh_selection(grp_sel).unwrap();
    grp_ascii.set_text_mode().unwrap();
    // inherits the dumper's group filter (no explicit group)
    grp_ascii
        .register_field(FieldDescriptor {
            padding: Padding::To3D,
            ..fd("disp", model("disp"), FieldSupport::Nodal, real)
        })
        .unwrap();
    grp_ascii.dump(&mesh, &r, None, None).unwrap();

    let mut ngrp = DumperParaview::with_name("ngrp");
    ngrp.set_directory(&pv).unwrap();
    ngrp.register_mesh_selection(
        MeshSelection::new(None, NG, ElementKind::NotDefined)
            .filtered(MeshFilter::NodeGroup("ngrp".into())),
    )
    .unwrap();
    ngrp.dump(&mesh, &r, None, None).unwrap();
    ngrp.set_text_mode().unwrap();
    ngrp.set_base_name("ngrp_ascii").unwrap();
    ngrp.dump(&mesh, &r, None, None).unwrap();
    assert_same_tree(&pv, &fixtures().join("groups/paraview"));
}

/// Probe case `parallel`: simulated parallel contexts (3 and 11 pieces,
/// rank suffix widths 1 and 2), as upstream `test_dumper`.
#[test]
fn parallel_pieces_match_oracle() {
    let dir = scratch("parallel");
    let pv = dir.join("paraview");
    let mesh = m2_mesh(true);
    let r = M2::new("parallel");
    for nbp in [3usize, 11] {
        let mut d = DumperParaview::new(&format!("par{nbp}"), s(&pv), false);
        d.register_mesh(2, NG, ElementKind::Regular).unwrap();
        d.register_field(fd(
            "velo",
            model("velo"),
            FieldSupport::Nodal,
            FieldValueType::Real,
        ))
        .unwrap();
        d.set_time_step(0.5).unwrap();
        for step in [0, 2] {
            for p in 0..nbp {
                d.set_parallel_context(ParallelContext::Explicit {
                    rank: p,
                    size: nbp,
                    root: 0,
                });
                d.dump(&mesh, &r, None, Some(step)).unwrap();
            }
        }
    }
    assert_same_tree(&pv, &fixtures().join("parallel/paraview"));
}

/// Probe case `text`: `DumperText` space and CSV modes, filtered mesh,
/// nodal/elemental fields, scalar/vector/integer variables, precision,
/// time description and its restart by a second `setTimeStep`.
#[test]
fn text_dumpers_match_oracle() {
    let dir = scratch("text");
    let tx = dir.join("text");
    let mut mesh = m2_mesh(true);
    let mut r = M2::new("text");
    {
        let ids = |v: &[Idx]| v.to_vec();
        let mut g = mesh.groups_mut().create_element_group("grp", 2).unwrap();
        g.elements
            .alloc(1, 1, ElementType::Quadrangle4, NG, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&ids(&[1]));
        g.elements
            .alloc(1, 1, ElementType::Triangle3, NG, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&ids(&[0]));
        g.node_group.set_nodes(r.inputs.ints("grp_nodes"));
    }
    let real = FieldValueType::Real;
    let mut space = DumperText::new("txt", TextDumpMode::Space, true);
    space.set_directory(&tx).unwrap();
    space.register_mesh(2, NG, ElementKind::Regular).unwrap();
    space
        .register_field(fd("disp", model("disp"), FieldSupport::Nodal, real))
        .unwrap();
    space
        .register_field(fd(
            "blocked",
            model("blocked"),
            FieldSupport::Nodal,
            FieldValueType::Bool,
        ))
        .unwrap();
    space
        .register_field(fd(
            "nint",
            model("nint"),
            FieldSupport::Nodal,
            FieldValueType::Int,
        ))
        .unwrap();
    space
        .register_field(fd(
            "ereal",
            model_el("ereal"),
            FieldSupport::Elemental,
            real,
        ))
        .unwrap();
    for v in ["scalar", "iscalar", "vec"] {
        space.register_variable(v).unwrap();
    }
    let mut csv = DumperText::new("txt_csv", TextDumpMode::Csv, true);
    csv.set_directory(&tx).unwrap();
    csv.set_precision(8).unwrap();
    csv.set_time_step(0.25).unwrap();
    csv.register_mesh_selection(
        MeshSelection::new(Some(2), NG, ElementKind::NotDefined)
            .filtered(MeshFilter::ElementGroup("grp".into())),
    )
    .unwrap();
    csv.register_field(fd("disp", model("disp"), FieldSupport::Nodal, real))
        .unwrap();
    csv.register_field(fd(
        "blocked",
        model("blocked"),
        FieldSupport::Nodal,
        FieldValueType::Bool,
    ))
    .unwrap();
    for v in ["scalar", "vec"] {
        csv.register_variable(v).unwrap();
    }

    space.dump(&mesh, &r, None, None).unwrap();
    csv.dump(&mesh, &r, None, None).unwrap();
    r.stage = 1;
    r.scalar += 2.;
    r.iscalar += 1;
    r.vec[0] += 0.1;
    space.dump(&mesh, &r, Some(0.5), Some(3)).unwrap();
    csv.dump(&mesh, &r, Some(0.5), Some(3)).unwrap();
    r.stage = 2;
    r.scalar *= -1e-12;
    r.vec[1] = 1e300;
    space.dump(&mesh, &r, None, None).unwrap();
    csv.dump(&mesh, &r, None, None).unwrap();
    csv.set_time_step(0.125).unwrap();
    csv.dump(&mesh, &r, None, None).unwrap();
    assert_same_tree(&tx, &fixtures().join("text/text"));
}

/// Probe cases `text_mpi_*` (2 MPI ranks) and UD-052: `nodes_type` is the
/// integer value of each node's `NodeFlag`; upstream's filtered path
/// writes it, the unfiltered `registerMesh` throws `std::bad_cast`
/// (`fixtures/dump/text_mpi_mesh/status.txt`). Both paths work here.
#[test]
fn ud052_text_nodes_type_matches_oracle() {
    let status = std::fs::read_to_string(fixtures().join("text_mpi_mesh/status.txt")).unwrap();
    assert!(status.contains("exception: std::bad_cast"), "{status}");
    let dir = scratch("text_mpi");
    let tx = dir.join("text");
    let mut mesh = m2_mesh(true);
    let flags = [
        NodeFlag::NORMAL,
        NodeFlag::MASTER,
        NodeFlag::SLAVE,
        NodeFlag::PURE_GHOST,
        NodeFlag::PERIODIC_MASTER,
        NodeFlag::PERIODIC_SLAVE,
        NodeFlag::PBC_SLAVE_DIST_GHOST,
        NodeFlag::DISTRIBUTED,
    ];
    MeshAccessor::new(&mut mesh)
        .nodes_flags_mut()
        .as_mut_slice()
        .copy_from_slice(&flags);
    let r = M2::new("text_mpi_filtered");
    let nodes = r.inputs.ints("filter_nodes");
    let mut elements = ElementTypeMapArray::<Idx>::new("all");
    for ty in [
        ElementType::Segment2,
        ElementType::Triangle3,
        ElementType::Quadrangle4,
    ] {
        let n = mesh.nb_element(ty, NG);
        let a = elements.alloc(n, 1, ty, NG, 0).unwrap();
        for (e, v) in a.as_mut_slice().iter_mut().enumerate() {
            *v = Idx::try_from(e).unwrap();
        }
    }
    let sel =
        MeshSelection::new(None, NG, ElementKind::NotDefined).filtered(MeshFilter::Explicit {
            elements: Box::new(elements),
            nodes,
        });
    for rank in 0..2 {
        let mut d = DumperText::new("txt_filtered", TextDumpMode::Space, true);
        d.set_directory(&tx).unwrap();
        d.set_parallel_context(ParallelContext::Explicit {
            rank,
            size: 2,
            root: 0,
        });
        d.register_mesh_selection(sel.clone()).unwrap();
        d.dump(&mesh, &r, None, None).unwrap();
    }
    assert_same_tree(&tx, &fixtures().join("text_mpi_filtered/text"));

    // unfiltered registerMesh on two ranks: the flags in mesh order
    let dir = scratch("text_mpi_mesh");
    let mut d = DumperText::new("txt_mesh", TextDumpMode::Space, true);
    d.set_directory(&dir).unwrap();
    d.set_parallel_context(ParallelContext::Explicit {
        rank: 1,
        size: 2,
        root: 0,
    });
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.dump(&mesh, &r, None, None).unwrap();
    let t = std::fs::read_to_string(dir.join("txt_mesh-DataFiles/txt_mesh_nodes_type.proc1.out"))
        .unwrap();
    assert_eq!(t, "0\n3\n5\n9\n48\n80\n89\n1\n\n");
}

/// Probe case `lammps`: atomic and bond writers, with and without bounds,
/// first dump and second dump of the same writer (stream state).
#[test]
fn lammps_writers_match_oracle() {
    let dir = scratch("lammps");
    let p3 = || vec![0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0, 123.456, 1e-300, 0.0];
    let p2 = vec![0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0];
    let bounds = [-1.5, 1.0 / 3.0, 0.0, 1e7, -2e-5, 12.0];
    let want = fixtures().join("lammps/lammps");
    for (name, style, pts, dim, b) in [
        ("atomic3", LammpsAtomStyle::Atomic, p3(), 3, None),
        (
            "atomic3_bounds",
            LammpsAtomStyle::Atomic,
            p3(),
            3,
            Some(bounds),
        ),
        ("atomic2", LammpsAtomStyle::Atomic, p2.clone(), 2, None),
        ("bond3", LammpsAtomStyle::Bond, p3(), 3, None),
        ("bond3_bounds", LammpsAtomStyle::Bond, p3(), 3, Some(bounds)),
    ] {
        let mut d = DumperLammps::new(name, style, b);
        d.set_directory(&dir).unwrap();
        let mut pts = pts;
        d.dump_points(&pts, dim).unwrap();
        assert_same_file(&dir.join(name), &want.join(format!("{name}.first")));
        for p in &mut pts {
            *p *= -2.0;
        }
        d.dump_points(&pts, dim).unwrap();
        assert_same_file(&dir.join(name), &want.join(name));
    }
}

/// Mesh of probe case `iohelper` (upstream IOHelper test mesh cube.msh,
/// tetrahedra only, read by the oracle).
fn iohelper_mesh(i: &Inputs) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in i.reals("nodes").chunks(3) {
            a.push_node(x).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Tetrahedron4, NG);
        for t in i.ints("tet4").chunks(4) {
            c.push_tuple(t);
        }
    }
    m
}

/// Ports of third-party/iohelper/test/test_paraview_dumper.cc and
/// test_text_dumper.cc (raw IOHelper API: tetrahedra, node field "test" =
/// coordinates), compared with the oracle IOHelper output.
#[test]
fn iohelper_tests_match_oracle() {
    let r = M2::new("iohelper");
    let mesh = iohelper_mesh(&r.inputs);
    let dir = scratch("iohelper");
    let out = dir.join("iohelper");
    let test = fd(
        "test",
        FieldSource::Mesh("positions".into()),
        FieldSupport::Nodal,
        FieldValueType::Real,
    );
    // test_paraview_dumper
    let mut pv = DumperParaview::new("coordinates2", s(&out), true);
    pv.set_parallel_context(ParallelContext::IoHelperDefault);
    pv.register_mesh(3, NG, ElementKind::Regular).unwrap();
    pv.register_field(test.clone()).unwrap();
    pv.set_base_name("test_tetra4").unwrap();
    pv.set_vtu_sub_directory("test_tetra4-VTUs");
    pv.dump(&mesh, &r, None, None).unwrap();
    // test_text_dumper
    let mut tx = DumperText::new("coordinates2", TextDumpMode::Space, true);
    tx.set_directory(&out).unwrap();
    tx.set_parallel_context(ParallelContext::IoHelperDefault);
    tx.register_field(fd(
        "positions",
        FieldSource::Mesh("positions".into()),
        FieldSupport::Nodal,
        FieldValueType::Real,
    ))
    .unwrap();
    tx.register_field(test).unwrap();
    tx.register_field(fd(
        "connectivities",
        FieldSource::Mesh("connectivities".into()),
        FieldSupport::Elemental,
        FieldValueType::Int,
    ))
    .unwrap();
    tx.set_base_name("test_text_tetra4").unwrap();
    tx.set_data_sub_directory("");
    tx.dump(&mesh, &r, None, Some(0)).unwrap();
    assert_same_tree(&out, &fixtures().join("iohelper/iohelper"));
}
